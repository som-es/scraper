use async_trait::async_trait;

use serde::Serialize;
use serde_json::{json, Value};

use crate::error::ScraperError;

use super::{AustriaScraper, Scraper};

#[async_trait]
pub trait ProposalScraper: Scraper {
    const FILTER_PATH: &'static str;

    type Payload: Serialize + Send;

    fn proposal_payload(&self) -> Self::Payload;

    async fn post_proposal_payload(&self) -> Result<Value, ScraperError> {
        self.post_payload(
            self.proposal_payload(),
            format!(
                "{base_url}{filter}",
                base_url = Self::BASE_URL,
                filter = Self::FILTER_PATH
            ),
        )
        .await
    }
}

impl ProposalScraper for AustriaScraper {
    const FILTER_PATH: &'static str = "/Filter/api/filter/data/101?js=eval&showAll=true";
    type Payload = Value;

    fn proposal_payload(&self) -> Self::Payload {
        json!({"NRBR": [self.scrape_info.council_type], "GP_CODE": self.scrape_info.gp})
    }
}

#[cfg(test)]
mod tests {
    use crate::scraper::{AustriaScraper, proposals::ProposalScraper};

    #[tokio::test]
    async fn test_post_proposal_payload() {
        let scraper = AustriaScraper::default();

        let proposals = scraper.post_proposal_payload().await.unwrap();
        println!("proposals: {:?}", proposals);
    }
}
