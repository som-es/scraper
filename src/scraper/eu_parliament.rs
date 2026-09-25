//! Scraper for the European Parliament's Open Data Portal
//! (<https://data.europarl.europa.eu/api/v2>).
//!
//! This is a separate, self-contained scraper rather than an implementation of the
//! [`Scraper`](crate::Scraper) trait used by [`AustriaScraper`](crate::AustriaScraper):
//! the AT trait's `POST` filter-payload/`gegenstand`-path conventions are specific to
//! `parlament.gv.at` and don't map onto the EU's plain REST/JSON-LD API. Where the
//! underlying data lines up, this module reuses the shared `common_scrapes` types
//! (`Delegate`, `Party`, `Proposal`, ...) so downstream consumers can treat both
//! sources uniformly. Where it genuinely doesn't line up (most notably per-MEP
//! roll-call votes vs. AT's party-level vote tallies), new EU-specific types are
//! introduced instead of forcing a bad fit.
//!
//! See `docs/eu-parliament-scraper.md` for a full comparison against the Austrian
//! scraper.

pub mod delegates;
pub mod parties;
pub mod plenary;
pub mod proposals;
pub mod voteable;
pub mod votes;

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::{Datelike, NaiveDate};
use common_scrapes::eu_hemicycle::{load_hemicycle, HemicycleLayout};
use once_cell::sync::Lazy;
use reqwest::{Client, Method};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::{
    caching::{disk_caching, get_disk_cache_json, CacheConfig},
    error::ScraperError,
    http_client,
    request::request,
    today, GeneralScrapeInfo, Scraper,
};

pub struct EuGp {
    gp: &'static str,
    start_date: NaiveDate,
    end_date: Option<NaiveDate>,
}

const GPS: [EuGp; 4] = [
    EuGp {
        gp: "10",
        start_date: NaiveDate::from_ymd_opt(2024, 7, 16).unwrap(),
        end_date: None,
    },
    EuGp {
        gp: "9",
        start_date: NaiveDate::from_ymd_opt(2019, 7, 2).unwrap(),
        end_date: Some(NaiveDate::from_ymd_opt(2024, 7, 15).unwrap()),
    },
    EuGp {
        gp: "8",
        start_date: NaiveDate::from_ymd_opt(2014, 7, 1).unwrap(),
        end_date: Some(NaiveDate::from_ymd_opt(2019, 7, 1).unwrap()),
    },
    EuGp {
        gp: "7",
        start_date: NaiveDate::from_ymd_opt(2009, 7, 14).unwrap(),
        end_date: Some(NaiveDate::from_ymd_opt(2014, 6, 30).unwrap()),
    },
];

#[derive(Debug, Clone)]
pub struct EuParliamentScraper {
    client: Client,
    pub cache_config: CacheConfig,
    pub scrape_info: GeneralScrapeInfo,
    pub delegate_seats: Arc<RwLock<Option<HashMap<u64, u64>>>>,
}

impl Default for EuParliamentScraper {
    fn default() -> Self {
        let mut cache_config = CacheConfig::default();
        cache_config.redo_secs = 60 * 60 * 24;
        Self {
            client: http_client().unwrap(),
            cache_config,
            delegate_seats: Default::default(),
            scrape_info: GeneralScrapeInfo {
                gp: vec![today().year().to_string()],
                council_type: vec![],
                ..Default::default()
            },
        }
    }
}

pub static STRASBOURG_HEMICYCLE: Lazy<HemicycleLayout> = Lazy::new(|| load_hemicycle());

#[async_trait]
impl Scraper for EuParliamentScraper {
    const BASE_URL: &'static str = "https://data.europarl.europa.eu/api/v2";
    const JSON_SUFFIX: &'static str = "?format=application%2Fld%2Bjson";
    const LIST_ENTRIES: &'static str = "";

    type Payload = Value;

    #[inline]
    fn client(&self) -> &Client {
        &self.client
    }

    fn cache_config(&self) -> &CacheConfig {
        &self.cache_config
    }

    async fn get_gegenstand_info(&self, _path: &str) -> Result<Value, ScraperError> {
        unimplemented!()
    }

    async fn cache(&self, _path: &str) -> Result<Value, ScraperError> {
        unimplemented!()
    }

    fn scrape_info(&self) -> &GeneralScrapeInfo {
        &self.scrape_info
    }

    fn scrape_info_mut(&mut self) -> &mut GeneralScrapeInfo {
        &mut self.scrape_info
    }
}

impl EuParliamentScraper {
    pub const BASE_URL: &'static str = "https://data.europarl.europa.eu/api/v2";
    const PAGE_LIMIT: usize = 500;

    #[inline]
    pub fn client(&self) -> &Client {
        &self.client
    }

    #[inline]
    pub fn cache_config(&self) -> &CacheConfig {
        &self.cache_config
    }

    /// GETs `path` under the API base URL with the raw `query` string appended (no
    /// leading `&`, e.g. `"year=2024"`), and returns the `data` array of the API's
    /// `{"data": [...], "@context": [...]}` JSON-LD envelope.
    pub async fn get_data(&self, path: &str, query: &str) -> Result<Vec<Value>, ScraperError> {
        let sep = if query.is_empty() { "" } else { "&" };
        let url = format!(
            "{base}/{path}?format=application%2Fld%2Bjson{sep}{query}",
            base = Self::BASE_URL,
        );
        let value = request(Method::GET, self.client(), (), url).await?;
        Ok(value["data"].as_array().cloned().unwrap_or_default())
    }

    /// Same as [`Self::get_data`], but follows `limit`/`offset` pagination until a page
    /// comes back with fewer rows than requested.
    pub async fn get_all_pages(&self, path: &str, query: &str) -> Result<Vec<Value>, ScraperError> {
        let mut offset = 0usize;
        let mut all = Vec::new();
        loop {
            let paged_query = if query.is_empty() {
                format!("limit={}&offset={offset}", Self::PAGE_LIMIT)
            } else {
                format!("{query}&limit={}&offset={offset}", Self::PAGE_LIMIT)
            };
            let page = self.get_data(path, &paged_query).await?;
            let got = page.len();
            all.extend(page);
            if got < Self::PAGE_LIMIT {
                break;
            }
            offset += Self::PAGE_LIMIT;
        }
        Ok(all)
    }

    pub async fn get_all_pages_cached(
        &self,
        path: &str,
        query: &str,
    ) -> Result<Vec<Value>, ScraperError> {
        let cache_key = format!("/{path}/{query}");
        if let Some(Value::Array(cached)) =
            get_disk_cache_json(&cache_key, self.cache_config()).await
        {
            return Ok(cached);
        }
        let value = Value::Array(self.get_all_pages(path, query).await?);
        if let Err(e) = disk_caching(&cache_key, value.to_string(), self.cache_config()).await {
            log::warn!("could not cache {cache_key}: {e:?}");
        }
        let Value::Array(value) = value else { panic!() };
        Ok(value)
    }

    /// GETs a single-resource `path` and returns the first element of `data`.
    pub async fn get_one(&self, path: &str) -> Result<Value, ScraperError> {
        self.get_one_query(path, "").await
    }

    pub async fn get_one_query(&self, path: &str, query: &str) -> Result<Value, ScraperError> {
        let mut data = self.get_data(path, query).await?;
        if data.is_empty() {
            return Err(ScraperError::NotFound);
        }
        Ok(data.remove(0))
    }

    /// Same as [`Self::get_one`], but reads from / writes to the on-disk cache first.
    ///
    /// `path` must not start with `/` (it is used as-is in the request URL, and a
    /// leading slash there produces a `//` that the API rejects with 400). The cache
    /// key is `/{path}`, since [`crate::caching`]'s helpers concatenate it directly
    /// onto `cache_dir` (which itself has no trailing slash).
    pub async fn get_one_cached(&self, path: &str) -> Result<Value, ScraperError> {
        let cache_key = format!("/{path}");
        if let Some(cached) = get_disk_cache_json(&cache_key, self.cache_config()).await {
            return Ok(cached);
        }
        let value = self.get_one(path).await?;
        if let Err(e) = disk_caching(&cache_key, value.to_string(), self.cache_config()).await {
            log::warn!("could not cache {cache_key}: {e:?}");
        }
        Ok(value)
    }
}

/// `"person/1234"` -> `"1234"`, `"def/ep-procedure-types/BUD"` -> `"BUD"`, etc.
pub(crate) fn last_path_segment(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

/// `"person/1234"` -> `1234`
pub(crate) fn id_to_u64(id: &str) -> Option<u64> {
    last_path_segment(id).parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_last_path_segment() {
        assert_eq!(last_path_segment("person/1234"), "1234");
        assert_eq!(last_path_segment("def/ep-procedure-types/BUD"), "BUD");
        assert_eq!(last_path_segment("noslash"), "noslash");
    }

    #[test]
    fn test_id_to_u64() {
        assert_eq!(id_to_u64("person/1234"), Some(1234));
        assert_eq!(id_to_u64("org/PPE"), None);
    }

    #[tokio::test]
    async fn test_get_data_meps() {
        let scraper = EuParliamentScraper::default();
        let rows = scraper.get_data("meps", "limit=3").await.unwrap();
        assert_eq!(rows.len(), 3);
    }
}
