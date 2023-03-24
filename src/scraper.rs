/*pub trait Scraper {

}*/

use async_trait::async_trait;
use reqwest::{Client, Method};
use serde::Serialize;
use serde_json::Value;

use crate::{error::ScraperError, request::request};

mod delegates;
mod proposals;

#[async_trait]
pub trait Scraper {
    const BASE_URL: &'static str;
    const JSON_SUFFIX: &'static str;

    // type DelegatePayload: Serialize;

    fn client(&self) -> &Client;

    async fn post_payload<P: Serialize + Send>(
        &self,
        payload: P,
        path: String,
    ) -> Result<Value, ScraperError> {
        request(Method::POST, self.client(), payload, path).await
    }
}

pub struct GeneralScrapeInfo {
    gp: Vec<String>,
    council_type: String,
}

impl Default for GeneralScrapeInfo {
    fn default() -> Self {
        Self {
            gp: vec!["XXVII".to_string()],
            council_type: "NR".to_string(),
        }
    }
}

#[derive(Default)]
pub struct AustriaScraper {
    client: Client,
    pub scrape_info: GeneralScrapeInfo,
}

impl AustriaScraper {
    /*pub async fn post_payload<P: Serialize + Send>(&self, payload: P, filter: &str) -> Result<Value, ScraperError> {
        Scraper::post_payload(self, payload, format!(
            "{base_url}{filter}",
            base_url = Self::BASE_URL,
        ),).await
    }*/
}

impl Scraper for AustriaScraper {
    const BASE_URL: &'static str = "https://www.parlament.gv.at";
    const JSON_SUFFIX: &'static str = "?json=True";

    #[inline]
    fn client(&self) -> &Client {
        &self.client
    }
}
