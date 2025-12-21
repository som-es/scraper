use std::future::Future;

use chrono::NaiveDate;
use common_scrapes::LegislativeInitiative;
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    error::ScraperError, legis_init::LegislativeInitiatives, AustriaScraper, GegenstandPaths,
    Scraper,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Enactment {
    pub nr_date: NaiveDate,
    pub br_date: Option<NaiveDate>,
    pub legislative_initiative: LegislativeInitiative,
}

// #[async_trait]
pub trait EnactmentScraper: Scraper + Send + Sync {
    const FILTER_PATH: &'static str;

    fn post_enactment_payload(&self) -> impl Future<Output = Result<Value, ScraperError>> {
        self.post_payload_filter(self.enactment_payload(), Self::FILTER_PATH)
    }

    fn enactment_payload(&self) -> Self::Payload;

    fn enactment_paths(&self, raw_enactments: Value) -> Option<GegenstandPaths>;

    fn raw_enactment_into_enactment(
        &self,
        raw_enactment: &Value,
    ) -> impl Future<Output = Option<Enactment>>;

    fn get_enactment(
        &self,
        enactment_path: &str,
    ) -> impl Future<Output = Result<Enactment, ScraperError>> {
        async move {
            let raw_enactment = self.get_gegenstand_info(enactment_path).await.unwrap();
            self.raw_enactment_into_enactment(&raw_enactment)
                .await
                .ok_or(ScraperError::NoEnactmentError)
        }
    }

    fn legis_inits_from_enactments();
}

// #[async_trait]
impl EnactmentScraper for AustriaScraper {
    const FILTER_PATH: &'static str = "/Filter/api/filter/data/101?js=eval&showAll=true";

    fn enactment_payload(&self) -> Self::Payload {
        json! {{
          "NRBR": [
            "NR"
          ],
          "GP_CODE": self.scrape_info.gp,
          "VHG": [
            "BNR"
          ],
          "VHG2": [
            "BNR"
          ],
          "DOKTYP": [
            "BNR"
          ]
        }}
    }

    fn enactment_paths(&self, raw_enactments: Value) -> Option<GegenstandPaths> {
        self.gegenstand_paths(raw_enactments, 14)
    }

    async fn raw_enactment_into_enactment(&self, raw_enactment: &Value) -> Option<Enactment> {
        let content = &raw_enactment["content"];
        let reference = &content["reference"].as_array()?[0];
        let legis_init_path = reference["url"].as_str().unwrap();

        let stages = &content["stages"].as_array()?;
        let nr_stage = &stages[0];
        let nr_date = NaiveDate::parse_from_str(
            nr_stage["date"]
                .as_str()
                .expect("BNR should be passing the nr stage"),
            "%d.%m.%Y",
        )
        .ok()?;

        let br_date = if let Some(br_stage) = stages.get(1) {
            Some(NaiveDate::parse_from_str(br_stage["date"].as_str()?, "%d.%m.%Y").ok()?)
        } else {
            None
        };

        let legislative_initiative = self.get_legis_init(legis_init_path).await.ok()?;
        Some(Enactment {
            nr_date,
            br_date,
            legislative_initiative,
        })
    }

    fn legis_inits_from_enactments() {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use common_scrapes::AcceptedState;

    use crate::{enactment::EnactmentScraper, AustriaScraper};

    /*#[tokio::test]
    async fn test_scrape_enactments() {
        let scraper = AustriaScraper::default();

        let raw_enactments = scraper.post_enactment_payload().await.unwrap();
        for path in scraper.enactment_paths(raw_enactments).unwrap() {
            // println!("path: {path}");
            let Ok(legis_init) = scraper.get_enactment(&path).await else {
                println!("not processable {path}");
                continue;
            };
            if legis_init
                .legislative_initiative
                .accepted
                .unwrap_or_default()
                == AcceptedState::Declined
            {
                println!("{legis_init:?}");
            }
        }
    }*/

    #[tokio::test]
    async fn test_xxvii_bnr_746() {
        let scraper = AustriaScraper::default();

        let enactment = scraper
            .get_enactment("/gegenstand/XXVII/BNR/746")
            .await
            .unwrap();

        assert_eq!(
            enactment.nr_date,
            NaiveDate::from_ymd_opt(2023, 6, 1).unwrap()
        );
        assert_eq!(enactment.br_date, NaiveDate::from_ymd_opt(2023, 6, 7));
        assert_eq!(
            enactment.legislative_initiative.id,
            "XXVII/A/3427".to_string()
        )
    }
}
