use async_trait::async_trait;
use reqwest::{Client, Method};
use serde::Serialize;
use serde_json::Value;

use crate::{
    caching::{disk_caching, get_disk_cache_json},
    error::ScraperError,
    http_client,
    request::request,
    TextExtractor,
};

pub mod call_to_order;
pub mod dates;
pub mod decree;
pub mod delegates;
pub mod enactment;
pub mod gov_officials;
pub mod legis_init;
pub mod ministrial_proposals;
pub mod oral_inquiries;
pub mod parliamentary_questions;
pub mod parties;
pub mod plenar;
pub mod proposals;

#[async_trait]
pub trait Scraper {
    const BASE_URL: &'static str;
    const JSON_SUFFIX: &'static str;
    const LIST_ENTRIES: &'static str;

    type Payload: Serialize + Send;

    // type DelegatePayload: Serialize;

    fn client(&self) -> &Client;

    async fn post_payload<P>(&self, payload: P, path: String) -> Result<Value, ScraperError>
    where
        P: Serialize + Send,
    {
        request(Method::POST, self.client(), payload, path).await
    }

    async fn post_payload_filter<P>(&self, payload: P, filter: &str) -> Result<Value, ScraperError>
    where
        P: Serialize + Send,
    {
        self.post_payload(
            payload,
            format!("{base_url}{filter}", base_url = Self::BASE_URL,),
        )
        .await
    }

    async fn get_gegenstand_info(&self, path: &str) -> Result<Value, ScraperError> {
        // let base_url = Url::new(Self::BASE_URL);

        let full_url = if path.starts_with('/') {
            format!(
                "{base_url}{path}{json_suffix}",
                base_url = Self::BASE_URL,
                json_suffix = Self::JSON_SUFFIX
            )
        } else {
            format!(
                "{base_url}/{path}{json_suffix}",
                base_url = Self::BASE_URL,
                json_suffix = Self::JSON_SUFFIX
            )
        };

        request(Method::GET, self.client(), (), full_url).await
        // reqwest::get(full_url).await?.json::<serde_json::Value>().await
    }

    async fn cache(&self, path: &str) -> Result<Value, ScraperError> {
        Ok(match get_disk_cache_json(path).await {
            Some(raw) => raw,
            None => {
                let raw_value = self.get_gegenstand_info(path).await?;
                if let Err(e) = disk_caching(path, raw_value.to_string()).await {
                    log::warn!("could cache raw {path}: {e:?}")
                }
                raw_value
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GeneralScrapeInfo {
    pub gp: Vec<String>,
    pub council_type: Vec<String>,
    pub date_start: Option<chrono::NaiveDateTime>,
    pub date_end: Option<chrono::NaiveDateTime>,
}

impl Default for GeneralScrapeInfo {
    fn default() -> Self {
        Self {
            gp: vec!["XXVIII".to_string()],
            council_type: vec!["NR".to_string()],
            date_start: None,
            date_end: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct DatePayload {
    date_start_end: Option<[chrono::NaiveDate; 2]>,
}

impl DatePayload {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_date_range(
        &mut self,
        date_start: chrono::NaiveDate,
        date_end: chrono::NaiveDate,
    ) -> &mut Self {
        self.date_start_end = Some([date_start, date_end]);
        self
    }

    pub fn to_at_format(&self) -> Option<[String; 2]> {
        self.date_start_end.map(|[start, end]| {
            [
                format!("{start}T00:00:00.000Z"),
                format!("{end}T00:00:00.000Z"),
            ]
        })
    }
}

pub type GegenstandPaths = Vec<String>;

#[derive(Debug, Clone)]
pub struct AustriaScraper {
    client: Client,
    pub text_extractor: TextExtractor,
    pub scrape_info: GeneralScrapeInfo,
    pub proposal_payload: Value,
}

impl Default for AustriaScraper {
    fn default() -> Self {
        Self {
            client: http_client().unwrap(),
            text_extractor: TextExtractor::new(),
            scrape_info: Default::default(),
            proposal_payload: Default::default(),
        }
    }
}

impl AustriaScraper {
    /*pub async fn post_payload<P: Serialize + Send>(&self, payload: P, filter: &str) -> Result<Value, ScraperError> {
        Scraper::post_payload(self, payload, format!(
            "{base_url}{filter}",
            base_url = Self::BASE_URL,
        ),).await
    }*/

    pub fn gegenstand_paths(
        &self,
        gegenstaende: Value,
        gegenstand_path_idx: usize,
    ) -> Option<Vec<String>> {
        Some(
            gegenstaende[Self::LIST_ENTRIES]
                .as_array()?
                .iter()
                .flat_map(|gegenstand| {
                    let person = gegenstand.as_array()?;
                    Some(person[gegenstand_path_idx].as_str()?.to_string())
                })
                .collect(),
        )
    }
}

impl Scraper for AustriaScraper {
    const BASE_URL: &'static str = "https://www.parlament.gv.at";
    const JSON_SUFFIX: &'static str = "?json=True";
    const LIST_ENTRIES: &'static str = "rows";

    type Payload = Value;

    #[inline]
    fn client(&self) -> &Client {
        &self.client
    }
}
