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
    const LIST_ENTRIES: &'static str;

    // type DelegatePayload: Serialize;

    fn client(&self) -> &Client;

    async fn post_payload<P: Serialize + Send>(
        &self,
        payload: P,
        path: String,
    ) -> Result<Value, ScraperError> {
        request(Method::POST, self.client(), payload, path).await
    }

    async fn get_gegenstand_info(&self, path: &str) -> Result<Value, ScraperError> {
        // let base_url = Url::new(Self::BASE_URL);

        let full_url = if path.starts_with("/") {
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

pub type GegenstandPaths = Vec<String>;

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

    pub fn gegenstand_paths(&self, gegenstaende: Value, gegenstand_path_idx: usize) -> Option<Vec<String>> {
        Some(
            gegenstaende[Self::LIST_ENTRIES]
                .as_array()?
                .iter()
                .flat_map(|person| {
                    let person = person.as_array()?;
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
    

    #[inline]
    fn client(&self) -> &Client {
        &self.client
    }

}
