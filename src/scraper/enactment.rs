use async_trait::async_trait;
use chrono::NaiveDateTime;
use common_scrapes::LegislativeInitiative;
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    error::ScraperError, legis_init::LegislativeInitiatives, AustriaScraper, GegenstandPaths,
    Scraper,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Enactment {
    pub nr_date: NaiveDateTime,
    pub br_date: Option<NaiveDateTime>,
    pub legislative_initiative: LegislativeInitiative,
}

#[async_trait]
pub trait EnactmentScraper: Scraper {
    const FILTER_PATH: &'static str;

    async fn post_enacment_payload(&self) -> Result<Value, ScraperError> {
        self.post_payload_filter(self.enactment_payload(), Self::FILTER_PATH)
            .await
    }

    fn enactment_payload(&self) -> Self::Payload;

    fn enactment_paths(&self, raw_enactments: Value) -> Option<GegenstandPaths>;

    async fn raw_enactment_into_enactment(
        &self,
        raw_enactment: &Value,
    ) -> Option<Enactment>;

    async fn get_enactment(
        &self,
        enactment_path: &str,
    ) -> Result<Enactment, ScraperError> {
        let raw_enactment = self.get_gegenstand_info(enactment_path).await.unwrap();
        self.raw_enactment_into_enactment(&raw_enactment)
            .await
            .ok_or(ScraperError::NoLegisInit)
    }

    fn legis_inits_from_enactments();
}

#[async_trait]
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

    async fn raw_enactment_into_enactment(
        &self,
        raw_enactment: &Value,
    ) -> Option<Enactment> {
        let content = &raw_enactment["content"];
        let reference = &content["reference"].as_array()?[0];
        let legis_init_path = reference["url"].as_str()?;

        let stages = &content["stages"].as_array()?;
        let nr_stage = &stages[0]; 
        let nr_date = NaiveDateTime::parse_from_str(
            &nr_stage["date"].as_str()?, "%Y-%m-%dT%H:%M:%S",
        ).ok()?;

        let br_date = if let Some(br_stage) = stages.get(1) {
            Some(            NaiveDateTime::parse_from_str(
                br_stage["date"].as_str()?, "%Y-%m-%dT%H:%M:%S",
            ).ok()?)
        } else {
            None
        };
    
        let legislative_initiative = self.get_legis_init(legis_init_path).await.ok()?;
        Some(Enactment {
            nr_date,
            br_date,
            legislative_initiative
        })
    }

    fn legis_inits_from_enactments() {
        todo!()
    }
}


#[cfg(test)]
mod tests {
    use crate::{AustriaScraper, enactment::EnactmentScraper};

    #[tokio::test]
    async fn test_scrape_enactments() {
        let scraper = AustriaScraper::default();

        let raw_enactments = scraper.post_enacment_payload().await.unwrap();
        for path in scraper.enactment_paths(raw_enactments).unwrap() {
            println!("path: {path}");
            let Ok(legis_init) = scraper.get_enactment(&path).await else {
                println!("not processable");
                continue;
            };
            println!("{legis_init:?}");
        }
    }
}