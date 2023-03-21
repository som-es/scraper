use async_trait::async_trait;
use reqwest::Method;
use serde::Serialize;
use serde_json::{json, Value};

use crate::{error::ScraperError, request::request, Delegate, Seat};

use super::{AustriaScraper, Scraper};

#[async_trait]
pub trait DelegateScraper: Scraper {
    const DELEGATE_FILTER_PATH: &'static str;
    const TO_DELEGATES_IDX: &'static str;
    type DelegatePayload: Serialize + Send;

    fn persons_payload(&self) -> Self::DelegatePayload;

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

    fn person_paths(&self, persons: Value) -> Option<PersonPaths>;

    async fn get_person_info(&self, person_path: &str) -> Result<Value, ScraperError> {
        // let base_url = Url::new(Self::BASE_URL);

        let full_url = if person_path.starts_with("/") {
            format!(
                "{base_url}{person_path}{json_suffix}",
                base_url = Self::BASE_URL,
                json_suffix = Self::JSON_SUFFIX
            )
        } else {
            format!(
                "{base_url}/{person_path}{json_suffix}",
                base_url = Self::BASE_URL,
                json_suffix = Self::JSON_SUFFIX
            )
        };

        request(Method::GET, self.client(), (), full_url).await
        // reqwest::get(full_url).await?.json::<serde_json::Value>().await
    }
    fn person_info_into_delegate(person_info: Value) -> Option<Delegate>;

    async fn get_delegate(&self, person_path: &str) -> Result<Delegate, ScraperError> {
        let person_info = self.get_person_info(person_path).await?;
        Self::person_info_into_delegate(person_info).ok_or(ScraperError::InvalidDelegateInfoError)
    }
}

impl DelegateScraper for AustriaScraper {
    type DelegatePayload = Value;
    const DELEGATE_FILTER_PATH: &'static str =
        "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFW_008&listeId=10008&showAll=true";

    const TO_DELEGATES_IDX: &'static str = "rows";

    #[inline]
    fn persons_payload(&self) -> Value {
        json!({"M": ["M"], "W": ["W"], "R_WF": ["FR"], "NRBR": ["NR"], "GP": ["XXVII"]})
    }

    fn person_info_into_delegate(person_info: Value) -> Option<Delegate> {
        let mandate = &person_info["content"]["banner"]["mandate"];
        let party = &mandate[0]["wahlpartei"];
        let constituency = &mandate[0]["wahlkreis"];

        let name = &person_info["meta"]["description"];

        println!("name: {name:?}");
        
        // contains sitzplatz at "sitzplatz"
        let api_person_info = &person_info["content"]["personInfo"];
        let council = &api_person_info["gremium"];

        let seat = &api_person_info["sitzplatz"];

        let (row, col) = seat.as_str()?.split_once('/')?;
        let (row, col) = (row.parse::<u16>().ok()?, col.parse::<u16>().ok()?);


        Some(Delegate {
            name: name.as_str()?.to_string(),
            party: party.as_str()?.to_string(),
            constituency: constituency.as_str()?.to_string(),
            council: council.as_str()?.to_string(),
            seat: Seat { row, col },
        })
    }

    fn person_paths(&self, persons: Value) -> Option<PersonPaths> {
        Some(
            persons[Self::TO_DELEGATES_IDX]
                .as_array()?
                .iter()
                .flat_map(|person| {
                    println!("person: {person}");
                    let person = person.as_array()?;
                    Some(person[person.len() - 1].as_str()?.to_string())
                })
                .collect(),
        )
    }
}

pub type PersonPaths = Vec<String>;

#[cfg(test)]
mod tests {
    use crate::{error::ScraperError, scraper::delegates::DelegateScraper};

    use super::{AustriaScraper, Scraper};

    #[tokio::test]
    async fn test_austria_person_info_scraper() {
        let scraper = AustriaScraper::default();
        let val = scraper.get_person_info("/person/16218").await.unwrap();

        // und sitzplatz
        let person_info = &val["content"]["personInfo"];
        println!("person_info: {person_info}");

        let mandate = &val["content"]["banner"]["mandate"];

        let party = &mandate[0]["wahlpartei"];

        println!("party: {party}");

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
        assert_eq!(res.party, "ÖVP");
        assert_eq!(res.council, "nr");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_all_persons() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();

        let persons = scraper.post_persons_payload().await?;
        for person_path in scraper.person_paths(persons).unwrap() {
            let Ok(delegate) = scraper.get_delegate(&person_path).await else {
                continue;
            };
            println!("del: {delegate:?}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_person_paths() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();

        let persons = scraper.post_persons_payload().await?;
        let person_paths = scraper.person_paths(persons).unwrap();
        for person_path in person_paths {
            let delegate = scraper.get_delegate(&person_path).await?;
            println!("del: {delegate:?}");
        }
        Ok(())
    }
}

//   curl -s \
//    -w '\n' \
//    -H 'Content-Type: application/json' \
//    -d '{"M": ["M"], "W": ["W"], "R_WF": ["FR"], "GP": ["XXVII"]}' \
//    'https://www.parlament.gv.at/Filter/api/json/post?jsMode=EVAL&FBEZ=WFW_008&listeId=10008&showAll=true'