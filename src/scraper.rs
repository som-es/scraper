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
