use async_trait::async_trait;
use common_scrapes::Party;

use crate::{enactment::EnactmentScraper, error::ScraperError, AustriaScraper, Scraper};

#[async_trait]
pub trait Parties: Scraper {
    async fn get_parties(&self) -> Result<Vec<Party>, ScraperError>;
}

#[async_trait]
impl Parties for AustriaScraper {
    async fn get_parties(&self) -> Result<Vec<Party>, ScraperError> {
        let raw_enactment_paths = self.post_enactment_payload().await?;
        let enactment_paths = self
            .enactment_paths(raw_enactment_paths)
            .ok_or(ScraperError::NoEnactmentError)?;
        let enactment = self.get_enactment(&enactment_paths.get(0).ok_or(ScraperError::NoEnactmentError)?).await?;

        Ok(enactment
            .legislative_initiative
            .votes
            .into_iter()
            .map(|vote| vote.party)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use common_scrapes::Party;

    use crate::AustriaScraper;

    use super::Parties;

    #[tokio::test]
    async fn test_get_parties_xxvii() {
        let scraper = AustriaScraper::default();

        let parties = scraper.get_parties().await.unwrap();
        assert_eq!(
            parties,
            vec![
                Party {
                    name: "ÖVP".to_string(),
                    color: "#62C3D0".to_string(),
                    fraction: 71,
                },
                Party {
                    name: "SPÖ".to_string(),
                    color: "#FF0000".to_string(),
                    fraction: 40,
                },
                Party {
                    name: "FPÖ".to_string(),
                    color: "#0052FB".to_string(),
                    fraction: 30,
                },
                Party {
                    name: "GRÜNE".to_string(),
                    color: "#69B12E".to_string(),
                    fraction: 26,
                },
                Party {
                    name: "NEOS".to_string(),
                    color: "#E3257B".to_string(),
                    fraction: 15,
                }
            ]
        )
    }

    #[tokio::test]
    async fn test_get_parties_xx() {
        let mut scraper = AustriaScraper::default();
        scraper.scrape_info.gp = vec!["XXIII".to_string()];

        let parties = scraper.get_parties().await.unwrap();
        assert_eq!(
            parties,
            vec![
                Party {
                    name: "SPÖ".to_string(),
                    color: "#FF0000".to_string(),
                    fraction: 68,
                },
                Party {
                    name: "ÖVP".to_string(),
                    color: "#000000".to_string(),
                    fraction: 66,
                },
                Party {
                    name: "GRÜNE".to_string(),
                    color: "#69B12E".to_string(),
                    fraction: 21,
                },
                Party {
                    name: "FPÖ".to_string(),
                    color: "#0052FB".to_string(),
                    fraction: 21,
                },
                
                Party {
                    name: "BZÖ".to_string(),
                    color: "#FECB56".to_string(),
                    fraction: 7,
                }
            ]
        )
    }
}
