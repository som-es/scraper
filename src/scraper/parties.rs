use async_trait::async_trait;
use common_scrapes::Party;

use crate::{AustriaScraper, Scraper, enactment::EnactmentScraper, error::ScraperError};



#[async_trait]
pub trait Parties: Scraper {
    async fn get_parties(&self) -> Result<Vec<Party>, ScraperError>;
}

#[async_trait]
impl Parties for AustriaScraper {
    async fn get_parties(&self) -> Result<Vec<Party>, ScraperError> {
        let raw_enactment_paths = self.post_enactment_payload().await?;
        let enactment_paths = self.enactment_paths(raw_enactment_paths).unwrap();

        todo!()
    }
}