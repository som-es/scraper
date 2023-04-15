use async_trait::async_trait;
use serde_json::{json, Value};

use crate::{
    error::ScraperError, proposals::ProposalScraper, AustriaScraper, GegenstandPaths, Scraper, documents,
};

pub struct Vote {
    party: String,
    fraction: u32,
    infavor: bool,
}

pub struct LegislativeInitiative {
    title: String,
    description: String,
    emphases: String,
    council: String,
    votes: Vec<Vote>,
}

#[async_trait]
pub trait LegislativeInitiatives: Scraper {
    const FILTER_PATH: &'static str;

    fn legis_payload(&self) -> Self::Payload;

    async fn post_legis_payload(&self) -> Result<Value, ScraperError> {
        let payload = self.legis_payload();
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

    fn raw_legis_into_legis(raw_legis: Value) -> Option<LegislativeInitiative>;

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError>;

    async fn get_legis_init(
        &self,
        proposal_path: &str,
    ) -> Result<LegislativeInitiative, ScraperError> {
        let raw_proposal = self.get_gegenstand_info(proposal_path).await?;
        Self::raw_legis_into_legis(raw_proposal).ok_or(ScraperError::LegisInitParseError)
    }
}

impl LegislativeInitiatives for AustriaScraper {
    const FILTER_PATH: &'static str =
        "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFP_003&listeId=10103&showAll";

    fn legis_payload(&self) -> Self::Payload {
        json!({"GP": self.scrape_info.gp})
    }

    fn raw_legis_into_legis(raw_legis: Value) -> Option<LegislativeInitiative> {
        let description = raw_legis["meta"]["description"].as_str()?.to_string();
        let title = raw_legis["meta"]["title"].as_str()?.to_string();

        let legis_content = &raw_legis["content"];

        let ityp = legis_content["ityp"].as_str()?;
        let inr = legis_content["inr"].as_u64()?;

        let raw_documents = legis_content["documents"].as_array()?;
        let documents = documents(raw_documents)?;
        println!("documents: {documents:?}");

        let json = serde_json::to_string_pretty(&raw_legis).unwrap();
        // println!("{json}");
        todo!()
    }

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError> {
        self.gegenstand_paths(initiatives, 12)
            .ok_or(ScraperError::ProposalParseError)
    }
}

#[cfg(test)]
mod tests {
    use crate::{legis_init::LegislativeInitiatives, AustriaScraper};

    #[tokio::test]
    async fn test_legis_init() {
        let scraper = AustriaScraper::default();

        let initiatives = scraper.post_legis_payload().await.unwrap();
        for path in scraper.legis_paths(initiatives).unwrap() {
            println!("path: {path:?}");
        }
    }

    #[tokio::test]
    async fn test_get_legis_init() {
        let scraper = AustriaScraper::default();

        let initiatives = scraper.post_legis_payload().await.unwrap();
        for path in scraper.legis_paths(initiatives).unwrap() {
            let init = scraper.get_legis_init(&path).await.unwrap();
            
        }
    }

    #[tokio::test]
    async fn test_get_legis_init_i_1954_path() {
        let scraper = AustriaScraper::default();

        let init = scraper.get_legis_init("/gegenstand/XXVII/I/1954").await.unwrap();
    }
}
