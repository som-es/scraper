use async_trait::async_trait;
use common_scrapes::{Delegate, Seat};
use reqwest::Method;
use serde_json::{json, Value};

use crate::{error::ScraperError, request::request};

use super::{AustriaScraper, GegenstandPaths, Scraper};

#[async_trait]
pub trait DelegateScraper: Scraper {
    const DELEGATE_FILTER_PATH: &'static str;

    fn persons_payload(&self) -> Self::Payload;

    async fn post_persons_payload(&self) -> Result<Value, ScraperError> {
        let payload = self.persons_payload();
        request(
            Method::POST,
            self.client(),
            payload,
            format!(
                "{base_url}{filter}",
                base_url = Self::BASE_URL,
                filter = Self::DELEGATE_FILTER_PATH
            ),
        )
        .await
    }

    fn person_paths(&self, persons: Value) -> Option<GegenstandPaths>;

    fn person_info_into_delegate(person_info: Value) -> Option<Delegate>;

    async fn get_delegate(&self, person_path: &str) -> Result<Delegate, ScraperError> {
        let person_info = self.get_gegenstand_info(person_path).await?;
        Self::person_info_into_delegate(person_info).ok_or(ScraperError::InvalidDelegateInfoError)
    }
}

impl DelegateScraper for AustriaScraper {
    const DELEGATE_FILTER_PATH: &'static str =
        // "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFW_008&listeId=10008&showAll=true";
        "/Filter/api/filter/data/10400?js=eval&showAll=true";

    #[inline]
    fn persons_payload(&self) -> Value {
        // json!({"M": ["M"], "W": ["W"], /*"R_WF": ["FR"],*//* "NRBR": [self.scrape_info.council_type],*/ "GP": self.scrape_info.gp})
        json! {{}}
    }

    fn person_info_into_delegate(person_info: Value) -> Option<Delegate> {
        // println!("{}", serde_json::to_string_pretty(&person_info).unwrap());
        let mandate = &person_info["content"]["banner"]["mandate"];
        let party = &mandate[0]["wahlpartei"];

        let constituency = &mandate[0]["wahlkreis"];

        let name = &person_info["meta"]["description"];
        let image_url = person_info["meta"]["openGraph"]["image"].as_str().map(|path| path.to_string());

        // contains sitzplatz at "sitzplatz"
        let api_person_info = &person_info["content"]["personInfo"];
        let id = &api_person_info["pad_intern"];
        let council = &api_person_info["gremium"];
        let is_active = api_person_info["aktiv"].as_bool()?;

        let seat = &api_person_info["sitzplatz"];

        let (row, col) = seat.as_str()?.split_once('/')?;
        let seat = if col.is_empty() {
            None
        } else {
            let (row, col) = (row.parse::<u16>().ok()?, col.parse::<u16>().ok()?);
            Some(Seat { row, col })
        };

        Some(Delegate {
            is_active,
            image_url,
            id: id.as_u64()?,
            name: name.as_str()?.to_string(),
            party: party.as_str().map(|party| party.to_string()),
            constituency: constituency
                .as_str()
                .map(|constituency| constituency.to_string()),
            council: council.as_str().map(|council| council.to_string()),
            seat,
        })
    }

    fn person_paths(&self, persons: Value) -> Option<GegenstandPaths> {
        // use idx 8 for delegates in parlament
        self.gegenstand_paths(persons, 4)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        error::ScraperError,
        scraper::{delegates::DelegateScraper, Scraper},
    };

    use super::AustriaScraper;

    #[tokio::test]
    async fn test_austria_person_info_scraper() {
        let scraper = AustriaScraper::default();
        let val = scraper.get_gegenstand_info("/person/16218").await.unwrap();

        // und sitzplatz
        let person_info = &val["content"]["personInfo"];
        println!("person_info: {person_info}");

        let mandate = &val["content"]["banner"]["mandate"];

        let party = &mandate[0]["wahlpartei"];

        let name = &val["meta"]["description"];

        println!("name: {name}");

        let pretty_val = serde_json::to_string_pretty(&val).unwrap();

        let delegate = AustriaScraper::person_info_into_delegate(val);
        println!("delegate: {delegate:?}");

        // let val = AustriaScraper::get_person_info("/person/16218").await;
        // println!("val: {pretty_val}");
    }

    #[tokio::test]
    async fn test_get_delegate() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();

        let res = scraper.get_delegate("/person/16218").await?;
        assert_eq!(res.name, "Diesner-Wais Martina");
        assert_eq!(res.party.unwrap(), "ÖVP");
        assert_eq!(res.council.unwrap(), "nr");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_all_persons() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();

        let mut delegates = vec![];

        let persons = scraper.post_persons_payload().await?;
        for person_path in scraper.person_paths(persons).unwrap_or_default() {
            let Ok(delegate) = scraper.get_delegate(&person_path).await else {
                continue;
            };
            println!("del: {delegate:?}");
            delegates.push(delegate);
        }

        let contents = serde_json::to_string(&delegates).unwrap();
        std::fs::write("../delegates.json", contents).unwrap();

        Ok(())
    }

    #[tokio::test]
    async fn test_person_paths() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();

        let persons = scraper.post_persons_payload().await?;
        let person_paths = scraper.person_paths(persons).unwrap();

        for person in person_paths {
            println!("{person}");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_person_special() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();
        let res = scraper.get_delegate("/person/55727").await?;
        println!("res: {res:?}");

        Ok(())
    }

    #[tokio::test]
    async fn test_person_special2() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();
        scraper.get_delegate("/person/21029").await?;
        Ok(())
    }
}

//   curl -s \
//    -w '\n' \
//    -H 'Content-Type: application/json' \
//    -d '{"M": ["M"], "W": ["W"], "R_WF": ["FR"], "GP": ["XXVII"]}' \
//    'https://www.parlament.gv.at/Filter/api/json/post?jsMode=EVAL&FBEZ=WFW_008&listeId=10008&showAll=true'
