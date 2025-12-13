use std::{collections::HashSet, future::Future};

use common_scrapes::Party;

use crate::{AustriaScraper, Scraper, error::ScraperError, legis_init::{LEGIS_LAW_DETERMINATORS, LegislativeInitiatives}};

// #[async_trait]
pub trait Parties: Scraper + Send {
    fn get_parties(&self) -> impl Future<Output = Result<Vec<Party>, ScraperError>>;
}

// #[async_trait]
impl Parties for AustriaScraper {
    async fn get_parties(&self) -> Result<Vec<Party>, ScraperError> {
        /*let raw_enactment_paths = self.post_enactment_payload().await?;
        let enactment_paths = self
            .enactment_paths(raw_enactment_paths)
            .ok_or(ScraperError::NoEnactmentError)?;
        let enactment = self.get_enactment(&enactment_paths.get(0).ok_or(ScraperError::NoEnactmentError)?).await?;*/

        let raw = self.post_legis_payload(Some(LEGIS_LAW_DETERMINATORS.as_slice())).await?;
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

            // if path.contains("XXIV") {
            //     let parties = parties.clone().collect::<Vec<_>>();
            //     log::error!("path: {path:?} parties: {parties:?}");
            // }
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
        scraper.scrape_info.gp = vec!["XXIII".to_string()];

        let parties = scraper.get_parties().await.unwrap();
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
        for gp in ["XXVII", "XXVI", "XXV", "XXIV", "XXIII", "XXII"] {
            scraper.scrape_info.gp = vec![gp.to_string()];
            let parties = scraper.get_parties().await.unwrap();
            println!("gp: {gp}, parties: {parties:?}");
        }
    }
}
