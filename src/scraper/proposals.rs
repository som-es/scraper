use std::fmt::format;

use async_trait::async_trait;

use serde::Serialize;
use serde_json::{json, Value};

use crate::error::ScraperError;

use super::{AustriaScraper, Scraper, GegenstandPaths, ProposalPayload};

#[async_trait]
pub trait ProposalScraper: Scraper {
    const FILTER_PATH: &'static str;
    const TO_DELEGATES_IDX: &'static str;

    type Payload: Serialize + Send;

    fn proposals_payload(&self) -> Self::Payload;

    async fn post_proposals_payload(&self, proposal_payload: ProposalPayload) -> Result<Value, ScraperError> {
        self.post_payload(
            proposal_payload.to_json(),
            format!(
                "{base_url}{filter}",
                base_url = Self::BASE_URL,
                filter = Self::FILTER_PATH
            ),
        )
        .await
    }

    fn raw_proposal_into_proposal(raw_proposal: Value) -> Option<Proposal>;

    fn proposal_paths(&self, proposals: Value) -> Option<GegenstandPaths>;

    async fn get_proposal(&self, proposal_path: &str) -> Result<Proposal, ScraperError> {
        let raw_proposal = self.get_gegenstand_info(proposal_path).await?;
        Self::raw_proposal_into_proposal(raw_proposal).ok_or(ScraperError::InvalidProposalInfoError)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Proposal {
    pub id: String,
    pub gp: String,
    pub title: String,
    pub description: String,
    pub issuer_ids: Vec<String>,
    pub receiver_ids: Vec<String>,
}

impl ProposalScraper for AustriaScraper {
    const FILTER_PATH: &'static str = "/Filter/api/filter/data/101?js=eval&showAll=true";
    // TODO
    // const FILTER_PATH: &'static str = "/Filter/api/filter/data/101?js=eval";
    const TO_DELEGATES_IDX: &'static str = "rows";
    type Payload = Value;

    fn proposals_payload(&self, ) -> Self::Payload {
        
        json!({"NRBR": [self.scrape_info.council_type], "GP_CODE": self.scrape_info.gp})
    }

    fn proposal_paths(&self, proposals: Value) -> Option<GegenstandPaths> {
        self.gegenstand_paths(proposals, 14)
    }

    fn raw_proposal_into_proposal(raw_proposal: Value) -> Option<Proposal> {
        let raw_proposal_content = &raw_proposal["content"];

        let names = raw_proposal_content["names"].as_array()?;

        let issuer_ids = names
            .iter()
            .filter(|name| name["funktext"] == "Eingebracht von")
            .flat_map(|name| Some(person_url_to_id(name["url"].as_str()?)?))
            .collect::<Vec<String>>();

        let receiver_ids = names
            .iter()
            .filter(|name| name["funktext"] == "Beantwortet durch")
            .flat_map(|name| Some(person_url_to_id(name["url"].as_str()?)?))
            .collect::<Vec<String>>();

        let description = raw_proposal_content["description"].as_str()?.to_string();

        let title = raw_proposal["meta"]["title"].as_str()?.to_string();

        let inr = raw_proposal_content["inr"].as_u64()?;
        let ityp = raw_proposal_content["ityp"].as_str()?;

        let id = format!("{ityp}/{inr}");
        
        let gp = raw_proposal_content["gp_code"].as_str()?.to_string();
        Some(Proposal {
            id,
            gp,
            title,
            description,
            issuer_ids,
            receiver_ids,
        })
    }
}

/// /person/83153 -> 83153
fn person_url_to_id(url: &str) -> Option<String> {
    url.split('/').last().map(|id| id.to_string())
}

#[cfg(test)]
mod tests {
    use crate::scraper::{AustriaScraper, proposals::ProposalScraper, ProposalPayload};

    #[tokio::test]
    async fn test_post_proposal_payload() {
        let scraper = AustriaScraper::default();

        let proposal_payload = ProposalPayload::default();
        let proposals = scraper.post_proposals_payload(proposal_payload).await.unwrap();

        for proposal in proposals["rows"].as_array().unwrap() {
            println!("{}", proposal[14]);
        }
        
        // println!("proposals: {}", serde_json::to_string_pretty(&proposals).unwrap());
    }

    #[tokio::test]
    async fn test_get_all_proposals() -> Result<(), Box<dyn std::error::Error>> {
        let scraper = AustriaScraper::default();

        let proposal_payload = ProposalPayload::default();
        let proposals = scraper.post_proposals_payload(proposal_payload).await?;

        let proposal_paths = scraper.proposal_paths(proposals).unwrap();

        for proposal_path in &proposal_paths[600..] {
            println!("proposal_path: {}", proposal_path);
            let proposal = scraper.get_proposal(&proposal_path).await?;
            println!("proposal: {proposal:?}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_get_proposal_from_path() -> Result<(), Box<dyn std::error::Error>> {
        let scraper = AustriaScraper::default();

        let proposal = scraper.get_proposal("/gegenstand/XXVII/UEA/568").await?;
        println!("proposal: {proposal:?}");
        
        Ok(())
    }
}
