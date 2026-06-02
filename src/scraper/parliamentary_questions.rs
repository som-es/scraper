use async_trait::async_trait;

use chrono::Local;
use common_scrapes::{ParliamentaryQAEntity, Proposal};
use serde_json::Value;

use crate::{error::ScraperError, proposals::ProposalScraper, LastUpdated};

use super::{AustriaScraper, Scraper};

impl LastUpdated for ParliamentaryQAEntity {
    fn last_updated(&self) -> chrono::DateTime<chrono::Utc> {
        self.base_info.updated_at.unwrap_or(Local::now().to_utc())
    }
}

#[async_trait]
pub trait ParliamentaryQAScraper: ProposalScraper {
    fn raw_value_into_parliamentary_qa(
        raw_value: Value,
        proposal: Proposal,
    ) -> Option<ParliamentaryQAEntity>;

    async fn get_parliamentary_qa(
        &self,
        path: &str,
    ) -> Result<ParliamentaryQAEntity, ScraperError> {
        let raw = self.cache(path).await?;
        let proposal = Self::raw_proposal_into_proposal(raw.clone())
            .ok_or(ScraperError::InvalidProposalInfoError)?;
        Self::raw_value_into_parliamentary_qa(raw, proposal)
            .ok_or(ScraperError::InvalidProposalInfoError)
    }
}

impl ParliamentaryQAScraper for AustriaScraper {
    fn raw_value_into_parliamentary_qa(
        raw_value: Value,
        proposal: Proposal,
    ) -> Option<ParliamentaryQAEntity> {
        None
    }
}
