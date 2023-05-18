use async_trait::async_trait;
use serde_json::{json, Value};

use crate::{error::ScraperError, AustriaScraper, GegenstandPaths, Scraper};

pub struct PlenarInfo {
    description: String,
    ityp: String,
    inr: u64,
}

#[async_trait]
pub trait Plenar: Scraper {
    const FILTER_PATH: &'static str;
    fn plenar_payload(&self) -> Self::Payload;

    async fn post_plenar_payload(&self) -> Result<Value, ScraperError> {
        let payload = self.plenar_payload();
        self.post_payload(
            payload,
            format!(
                "{base_url}{filter}",
                base_url = Self::BASE_URL,
                filter = Self::FILTER_PATH
            ),
        )
        .await
    }

    fn plenar_paths(&self, plenar_infos: Value) -> Result<GegenstandPaths, ScraperError>;

    fn raw_plenar_into_plenar_info(raw_plenar: Value) -> Option<PlenarInfo>;

    async fn get_plenar_init(&self, plenar_path: &str) -> Result<PlenarInfo, ScraperError> {
        let raw_proposal = self.get_gegenstand_info(plenar_path).await?;
        Self::raw_plenar_into_plenar_info(raw_proposal).ok_or(ScraperError::LegisInitParseError)
    }
}

impl Plenar for AustriaScraper {
    const FILTER_PATH: &'static str =
        "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFP_007&listeId=undefined&showAll=true";

    fn plenar_payload(&self) -> Self::Payload {
        json!({"MODUS": ["PLENAR"], "GP": self.scrape_info.gp, "NRBRBV": self.scrape_info.council_type})
    }

    fn plenar_paths(&self, plenar_infos: Value) -> Result<GegenstandPaths, ScraperError> {
        self.gegenstand_paths(plenar_infos, 8)
            .ok_or(ScraperError::ProposalParseError)
    }

    fn raw_plenar_into_plenar_info(raw_plenar: Value) -> Option<PlenarInfo> {
        let description = raw_plenar["meta"]["description"].as_str()?.to_string();

        let plenar_content = &raw_plenar["content"];

        let ityp = plenar_content["ityp"].as_str()?.to_string();
        let inr = plenar_content["inr"].as_u64()?;

        Some(PlenarInfo {
            description,
            ityp,
            inr,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{AustriaScraper, Scraper};

    use super::Plenar;

    #[tokio::test]
    async fn test_get_gegenstand_paths_plenar() {
        let scraper = AustriaScraper::default();
        let plenar_infos = scraper.post_plenar_payload().await.unwrap();

        let paths = scraper
            .plenar_paths(plenar_infos)
            .expect("Failed to get paths");
        println!("paths: {paths:?}");
    }

    #[tokio::test]
    async fn test_look_all_plenar_gegenstand_paths() {
        let scraper = AustriaScraper::default();

        let plenar_infos = scraper.post_plenar_payload().await.unwrap();

        let paths = scraper
            .plenar_paths(plenar_infos)
            .expect("Failed to get paths");
        for path in paths {
            scraper.get_gegenstand_info(&path).await.unwrap();
        }
    }
}
