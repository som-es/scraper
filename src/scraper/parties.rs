use std::collections::HashSet;

use async_trait::async_trait;
use common_scrapes::{Mandate, Party};

use crate::{
    error::ScraperError,
    legis_init::{LegislativeInitiatives, LEGIS_LAW_DETERMINATORS},
    AustriaScraper, Scraper,
};

pub struct DelegateMandate {
    pub delegate_id: u32,
    pub mandates: Vec<Mandate>,
    pub party: Option<String>,
}

#[async_trait]
pub trait Parties: Scraper + Send {
    async fn get_parties(&self, delegates: &[DelegateMandate]) -> Result<Vec<Party>, ScraperError>;
}

#[async_trait]
impl Parties for AustriaScraper {
    async fn get_parties(
        &self,
        _delegates: &[DelegateMandate],
    ) -> Result<Vec<Party>, ScraperError> {
        /*let raw_enactment_paths = self.post_enactment_payload().await?;
        let enactment_paths = self
            .enactment_paths(raw_enactment_paths)
            .ok_or(ScraperError::NoEnactmentError)?;
        let enactment = self.get_enactment(&enactment_paths.get(0).ok_or(ScraperError::NoEnactmentError)?).await?;*/

        let raw = self
            .post_legis_payload(Some(LEGIS_LAW_DETERMINATORS.as_slice()))
            .await?;

        let legis_paths = self.legis_paths(raw)?;

        let mut votes = HashSet::new();
        let mut added_vote_legis_inits = 0;
        for path in legis_paths.iter().rev() {
            // println!("path: {path}");
            let Ok(legis_init) = self.get_legis_init(path).await else {
                continue;
            };
            if legis_init.votes.is_empty() {
                continue;
            }
            let parties = legis_init.votes.into_iter().map(|vote| vote.party);

            votes.extend(parties);

            added_vote_legis_inits += 1;

            if added_vote_legis_inits >= 5 {
                break;
            }
        }

        added_vote_legis_inits = 0;
        for path in legis_paths.iter() {
            let Ok(legis_init) = self.get_legis_init(path).await else {
                continue;
            };
            if legis_init.votes.is_empty() {
                continue;
            }
            votes.extend(legis_init.votes.into_iter().map(|vote| vote.party));

            added_vote_legis_inits += 1;

            if added_vote_legis_inits >= 5 {
                break;
            }
        }
        Ok(votes.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use common_scrapes::Party;

    use crate::{caching::CacheConfig, AustriaScraper};

    use super::Parties;

    #[tokio::test]
    async fn test_get_parties_xxvii() {
        let mut scraper = AustriaScraper::default();
        scraper.cache_config = CacheConfig {
            cache_dir: "./test-data".into(),
            ignore_current_gp_cache: false,
            redo_secs: u64::MAX,
        };
        scraper.text_extractor.cache_config = CacheConfig {
            cache_dir: "./test-data".into(),
            ignore_current_gp_cache: false,
            redo_secs: u64::MAX,
        };
        scraper.scrape_info.gp = vec!["XXVII".to_string()];

        let mut parties = scraper.get_parties(&vec![]).await.unwrap();
        parties.sort_by(|a, b| b.fraction.cmp(&a.fraction).then(b.name.cmp(&a.name)));
        assert_eq!(
            parties,
            vec![
                Party {
                    name: "ÖVP".to_string(),
                    color: "#62C3D0".to_string(),
                    fraction: 71,
                    code: "V".to_string(),
                },
                Party {
                    name: "SPÖ".to_string(),
                    color: "#FF0000".to_string(),
                    fraction: 40,
                    code: "S".to_string(),
                },
                Party {
                    name: "FPÖ".to_string(),
                    color: "#0052FB".to_string(),
                    fraction: 30,
                    code: "F".to_string(),
                },
                Party {
                    name: "GRÜNE".to_string(),
                    color: "#69B12E".to_string(),
                    fraction: 26,
                    code: "G".to_string(),
                },
                Party {
                    name: "NEOS".to_string(),
                    color: "#E3257B".to_string(),
                    fraction: 15,
                    code: "N".to_string(),
                }
            ]
        )
    }

    #[tokio::test]
    async fn test_get_parties_xxiii() {
        let mut scraper = AustriaScraper::default();
        scraper.cache_config = CacheConfig {
            cache_dir: "./test-data".into(),
            ignore_current_gp_cache: false,
            redo_secs: u64::MAX,
        };
        scraper.text_extractor.cache_config = CacheConfig {
            cache_dir: "./test-data".into(),
            ignore_current_gp_cache: false,
            redo_secs: u64::MAX,
        };
        scraper.scrape_info.gp = vec!["XXIII".to_string()];

        let mut parties = scraper.get_parties(&vec![]).await.unwrap();
        parties.sort_by(|a, b| b.fraction.cmp(&a.fraction).then(b.name.cmp(&a.name)));
        assert_eq!(
            parties,
            vec![
                Party {
                    name: "SPÖ".to_string(),
                    color: "#FF0000".to_string(),
                    fraction: 68,
                    code: "S".to_string(),
                },
                Party {
                    name: "ÖVP".to_string(),
                    color: "#000000".to_string(),
                    fraction: 66,
                    code: "V".to_string(),
                },
                Party {
                    name: "GRÜNE".to_string(),
                    color: "#69B12E".to_string(),
                    fraction: 21,
                    code: "G".to_string(),
                },
                Party {
                    name: "FPÖ".to_string(),
                    color: "#0052FB".to_string(),
                    fraction: 21,
                    code: "F".to_string(),
                },
                Party {
                    name: "BZÖ".to_string(),
                    color: "#FECB56".to_string(),
                    fraction: 7,
                    code: "B".to_string(),
                }
            ]
        )
    }

    #[tokio::test]
    async fn test_get_parties_xxii() {
        let mut scraper = AustriaScraper::default();
        scraper.cache_config = CacheConfig {
            cache_dir: "./test-data".into(),
            ignore_current_gp_cache: false,
            redo_secs: u64::MAX,
        };
        scraper.text_extractor.cache_config = CacheConfig {
            cache_dir: "./test-data".into(),
            ignore_current_gp_cache: false,
            redo_secs: u64::MAX,
        };
        for gp in ["XXVII", "XXVI", "XXV", "XXIV", "XXIII", "XXII"] {
            scraper.scrape_info.gp = vec![gp.to_string()];
            let parties = scraper.get_parties(&vec![]).await.unwrap();
            println!("gp: {gp}, parties: {parties:?}");
        }
    }
}
