use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use common_scrapes::{Volksbg, VolksbgEintragungswoche};

use crate::{
    caching::{disk_caching, get_disk_cache, CacheConfig},
    error::ScraperError,
    request::request_text,
    Scraper,
};

pub mod archive;
pub mod dates;
pub mod detail;
pub mod list;

#[cfg(test)]
mod tests;

pub use archive::*;
pub use dates::*;
pub use detail::*;
pub use list::*;

pub const BMI_BASE_URL: &str = "https://www.bmi.gv.at";
pub const BMI_VOLKSBEGEHREN_URL: &str = "https://www.bmi.gv.at/411/start.html";

/// BMI overview of all Volksbegehren seit der XX. Gesetzgebungsperiode
pub const BMI_VOLKSBEGEHREN_INDEX_URL: &str =
    "https://www.bmi.gv.at/411/volksbegehren_der_xx_gesetzgebungsperiode/start.html";

/// ID Austria / eIDAS entry point for supporting a Volksbegehren,
/// identical for every Volksbegehren
pub const VOLKSBG_SUPPORT_URL: &str =
    "https://citizen.bmi.gv.at/at.gv.bmi.fnsweb-p/vbg/checked/VolksbegehrenBuerger";

pub const VOLKSBG_REDO_SECS: u64 = 60 * 60 * 3;

#[async_trait]
pub trait VolksbgScraper: Scraper {
    async fn bmi_html(&self, url: &str) -> Result<String, ScraperError> {
        let cache_config = CacheConfig {
            redo_secs: VOLKSBG_REDO_SECS,
            ..self.cache_config().clone()
        };

        if let Some(html) = get_disk_cache(url, &cache_config).await {
            return Ok(html);
        }

        let html = request_text(self.client(), url).await?;

        if let Err(error) = disk_caching(url, &html, &cache_config).await {
            log::warn!("could not cache {url}: {error}");
        }

        Ok(html)
    }

    /// The currently announced or running Eintragungswoche, `None` while the
    /// BMI publishes no Eintragungszeitraum (e.g. after the results were established)
    async fn eintragungswoche(&self) -> Result<Option<VolksbgEintragungswoche>, ScraperError> {
        self.eintragungswoche_of(BMI_VOLKSBEGEHREN_URL).await
    }

    /// Same as [`VolksbgScraper::eintragungswoche`], for any BMI overview page
    async fn eintragungswoche_of(
        &self,
        url: &str,
    ) -> Result<Option<VolksbgEintragungswoche>, ScraperError> {
        let html = self.bmi_html(url).await?;

        let Some(mut eintragungswoche) = parse_eintragungswoche(&html, url) else {
            return Ok(None);
        };

        eintragungswoche.apply_state(today());

        Ok(Some(eintragungswoche))
    }

    /// Links to the BMI pages of the Volksbegehren listed on `url`,
    /// without fetching the detail pages
    async fn volksbg_links_of(&self, url: &str) -> Result<Vec<Volksbg>, ScraperError> {
        Ok(parse_volksbg_links(&self.bmi_html(url).await?, url))
    }

    /// Links to the BMI pages of the Volksbegehren on the BMI Volksbegehren page,
    /// those are the ones of the Eintragungswoche and of the results announcement
    async fn volksbg_links(&self) -> Result<Vec<Volksbg>, ScraperError> {
        self.volksbg_links_of(BMI_VOLKSBEGEHREN_URL).await
    }

    /// Links to the BMI pages of all Volksbegehren seit der XX. Gesetzgebungsperiode
    async fn all_volksbg_links(&self) -> Result<Vec<Volksbg>, ScraperError> {
        self.volksbg_links_of(BMI_VOLKSBEGEHREN_INDEX_URL).await
    }

    /// Eintragungswoche that signatures can be handed in for
    async fn open_eintragungswoche(&self) -> Result<Option<VolksbgEintragungswoche>, ScraperError> {
        Ok(self
            .eintragungswoche()
            .await?
            .filter(|woche| woche.is_open_on(today())))
    }

    /// Eintragungswoche including Wortlaut, documents and ruling dates of each Volksbegehren
    async fn eintragungswoche_with_details(
        &self,
        eintragungswoche: &VolksbgEintragungswoche,
    ) -> Result<VolksbgEintragungswoche, ScraperError> {
        let mut eintragungswoche = eintragungswoche.clone();

        for volksbg in eintragungswoche.volksbegehren.iter_mut() {
            match self.bmi_html(&volksbg.overview_url).await {
                Ok(html) => {
                    let overview_url = volksbg.overview_url.clone();
                    merge_volksbg_detail(volksbg, &html, &overview_url)
                }
                Err(error) => log::warn!("could not fetch {}: {error}", volksbg.overview_url),
            }
        }

        Ok(eintragungswoche)
    }
}

pub fn today() -> NaiveDate {
    Utc::now()
        .with_timezone(&chrono_tz::Europe::Vienna)
        .date_naive()
}

impl VolksbgScraper for crate::AustriaScraper {}
