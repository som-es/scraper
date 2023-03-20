/*pub trait Scraper {

}*/

use async_trait::async_trait;
use reqwest::Client;

mod delegates;

pub trait Scraper {
    const BASE_URL: &'static str;
    const JSON_SUFFIX: &'static str;

    // type DelegatePayload: Serialize;

    fn client(&self) -> &Client;
}

pub struct AustriaScraper {
    client: Client,
    pub gp: String,
}

impl Default for AustriaScraper {
    fn default() -> Self {
        Self {
            client: Default::default(),
            gp: "XXVII".to_string(),
        }
    }
}

impl Scraper for AustriaScraper {
    const BASE_URL: &'static str = "https://www.parlament.gv.at";
    const JSON_SUFFIX: &'static str = "?json=True";

    #[inline]
    fn client(&self) -> &Client {
        &self.client
    }
}

#[cfg(test)]
mod tests {
    use crate::scraper::delegates::DelegateScraper;

    use super::{AustriaScraper, Scraper};

    #[tokio::test]
    async fn test_austria_person_info_scraper() {
        let scraper = AustriaScraper::default();
        let val = scraper.get_person_info("/person/16218").await.unwrap();

        // und sitzplatz
        let person_info = &val["content"]["personInfo"];
        println!("person_info: {person_info}");

        let mandate = &val["content"]["banner"]["mandate"];

        let party = &mandate[0]["wahlpartei"];

        println!("party: {party}");

        let name = &val["meta"]["description"];

        println!("name: {name}");

        let pretty_val = serde_json::to_string_pretty(&val).unwrap();

        let delegate = AustriaScraper::person_info_into_delegate(val);
        println!("delegate: {delegate:?}");

        // let val = AustriaScraper::get_person_info("/person/16218").await;
        // println!("val: {pretty_val}");
    }

    #[tokio::test]
    async fn test_get_delegate() {
        let scraper = AustriaScraper::default();

        let res = scraper.get_delegate("/person/16218").await;
        println!("res: {res:?}");
    }
}
