use async_trait::async_trait;
use reqwest::Method;
use serde::Serialize;
use serde_json::{json, Value};

use crate::{error::ScraperError, request::request, Delegate, Seat};

use super::{AustriaScraper, Scraper};

#[async_trait]
pub trait DelegateScraper: Scraper {
    const DELEGATE_FILTER_PATH: &'static str;
    type DelegatePayload: Serialize + Send;

    fn delegates_payload(&self) -> Self::DelegatePayload;

    async fn post_delegates_payload(&self) {
        let payload = self.delegates_payload();
        request(
            Method::POST,
            self.client(),
            payload,
            format!(
                "{base_url}{filter}",
                base_url = Self::BASE_URL,
                filter = Self::DELEGATE_FILTER_PATH
            ),
        ).await;
    }

    async fn get_person_info(&self, person_url: &str) -> Result<Value, ScraperError> {
        // let base_url = Url::new(Self::BASE_URL);

        let full_url = if person_url.starts_with("/") {
            format!(
                "{base_url}{person_url}{json_suffix}",
                base_url = Self::BASE_URL,
                json_suffix = Self::JSON_SUFFIX
            )
        } else {
            format!(
                "{base_url}/{person_url}{json_suffix}",
                base_url = Self::BASE_URL,
                json_suffix = Self::JSON_SUFFIX
            )
        };

        request(Method::GET, self.client(), (), full_url).await
        // reqwest::get(full_url).await?.json::<serde_json::Value>().await
    }
    fn person_info_into_delegate(person_info: Value) -> Option<Delegate>;

    async fn get_delegate(&self, person_url: &str) -> Result<Delegate, ScraperError> {
        let person_info = self.get_person_info(person_url).await?;
        Self::person_info_into_delegate(person_info).ok_or(ScraperError::InvalidDelegateInfoError)
    }
}

impl DelegateScraper for AustriaScraper {
    type DelegatePayload = Value;
    const DELEGATE_FILTER_PATH: &'static str =
        "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFW_008&listeId=10008?showAll=true";

    #[inline]
    fn delegates_payload(&self) -> Value {
        json!({"M": ["M"], "W": ["W"], "NRBR": ["NR"], "GP": [self.gp]})
    }

    fn person_info_into_delegate(person_info: Value) -> Option<Delegate> {
        // contains sitzplatz at "sitzplatz"
        let api_person_info = &person_info["content"]["personInfo"];
        let council = &api_person_info["gremium"];
        let seat = &api_person_info["sitzplatz"];

        
        let (row, col) = seat.as_str()?.split_once('/')?;
        let (row, col) = (row.parse::<u16>().ok()?, col.parse::<u16>().ok()?);

        let mandate = &person_info["content"]["banner"]["mandate"];
        let party = &mandate[0]["wahlpartei"];
        let constituency = &mandate[0]["wahlkreis"];

        let name = &person_info["meta"]["description"];

        Some(Delegate {
            name: name.as_str()?.to_string(),
            party: party.as_str()?.to_string(),
            constituency: constituency.as_str()?.to_string(),
            council: council.as_str()?.to_string(),
            seat: Seat {
                row,
                col
            }
        })
    }
}

pub struct DelegateURLs {
    pub delegate_urls: Vec<String>,
}
