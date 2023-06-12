use async_trait::async_trait;
use chrono::NaiveDate;
use common_scrapes::{
    Contact, Delegate, Gender, Mandate, Seat, SocialMedia, SocialMedium, SocialMediumChannel,
};

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
        let gender_indicator = &mandate[0]["mandat"].as_str()?.split_whitespace().next()?;

        let constituency = &mandate[0]["wahlkreis"];

        let name = &person_info["meta"]["description"];
        let image_url = person_info["meta"]["openGraph"]["image"]
            .as_str()
            .map(|path| path.to_string());

        // contains sitzplatz at "sitzplatz"
        let api_person_info = &person_info["content"]["personInfo"];
        let id = &api_person_info["pad_intern"];
        let council = &api_person_info["gremium"];
        let is_active = api_person_info["aktiv"].as_bool()?;

        let api_contact_info = &person_info["content"]["kontakt"];

        let mandates = extract_mandates(&person_info["content"]["biografie"]["mandatefunktionen"])?;

        let phone_number = api_contact_info
            .get("telefonnr")
            .map(|phones| phones.get(0).map(|phone| phone.as_str()).flatten())
            .flatten()
            .map(ToString::to_string);

        let mail = api_contact_info
            .get("mail")
            .map(|mails| mails.get(0).map(|mail| mail.as_str()).flatten())
            .flatten()
            .map(ToString::to_string);

        let social_media = api_contact_info
            .get("socialMedia")
            .map(|raw_social_media| extract_social_media(raw_social_media))
            .flatten()
            .map(SocialMedia::from)
            .unwrap_or_default();

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
            mandates,
            contact: Contact {
                mail,
                phone_number,
                social_media,
            },
            gender: Gender::from(*gender_indicator),
        })
    }

    fn person_paths(&self, persons: Value) -> Option<GegenstandPaths> {
        // use idx 8 for delegates in parlament
        self.gegenstand_paths(persons, 4)
    }
}

pub fn extract_mandates(raw_mandates: &Value) -> Option<Vec<Mandate>> {
    Some(
        raw_mandates["mandate"]
            .as_array()?
            .iter()
            .flat_map(extract_mandate)
            .collect(),
    )
}

pub fn extract_mandate(raw_mandate: &Value) -> Option<Mandate> {
    let name = raw_mandate["mandat"].as_str()?.to_string();

    let start = raw_mandate["mandatVon"].as_str()?.to_string();
    let end = raw_mandate["mandatBis"].as_str()?.to_string();
    let party = raw_mandate["wahlpartei"].as_str().map(ToString::to_string);

    let start = NaiveDate::parse_from_str(&start, "%d.%m.%Y").ok()?;

    let end = if end.is_empty() {
        None
    } else {
        NaiveDate::parse_from_str(&end, "%d.%m.%Y").ok()
    };

    Some(Mandate {
        name,
        party,
        start,
        end,
    })
}

pub fn extract_social_media(raw_social_media: &Value) -> Option<Vec<SocialMedium>> {
    Some(
        raw_social_media
            .as_array()?
            .iter()
            .flat_map(extract_social_medium)
            .collect(),
    )
}

pub fn extract_social_medium(raw_medium: &Value) -> Option<SocialMedium> {
    let url = raw_medium["url"].as_str()?.to_string();
    let channel = match raw_medium["type"].as_str()? {
        "FACEBOOK" => SocialMediumChannel::Facebook,
        "TWITTER" => SocialMediumChannel::Twitter,
        "INSTAGRAM" => SocialMediumChannel::Instagram,
        "YOUTUBE" => SocialMediumChannel::Youtube,
        "WEBSITE" => SocialMediumChannel::Website,
        "TIKTOK" => SocialMediumChannel::TikTok,
        _ => return None,
    };

    Some(SocialMedium { url, channel })
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use common_scrapes::{Gender, Mandate, SocialMedia};

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
        println!("res: {res:?}");

        assert_eq!(res.name, "Diesner-Wais Martina");
        assert_eq!(res.party.unwrap(), "ÖVP");
        assert_eq!(res.council.unwrap(), "nr");

        // subject to change in coming elections
        assert_eq!(res.contact.phone_number, Some("+43 1 401 10-4687".into()));
        assert_eq!(
            res.contact.mail,
            Some("martina.diesner-wais@parlament.gv.at".into())
        );
        assert_eq!(res.contact.social_media, SocialMedia::default());
        assert_eq!(res.gender, Gender::Female);
        assert_eq!(
            res.mandates,
            [
                Mandate {
                    name: "Abgeordnete zum Nationalrat".into(),
                    party: Some("ÖVP".into()),
                    start: NaiveDate::from_ymd_opt(2013, 10, 29).unwrap(),
                    end: None
                },
                Mandate {
                    name: "Mitglied des Bundesrates".into(),
                    party: Some("ÖVP".into()),
                    start: NaiveDate::from_ymd_opt(2003, 4, 24).unwrap(),
                    end: Some(NaiveDate::from_ymd_opt(2013, 4, 23).unwrap())
                },
                Mandate {
                    name: "Schriftführerin des Bundesrates".into(),
                    party: None,
                    start: NaiveDate::from_ymd_opt(2010, 11, 5).unwrap(),
                    end: Some(NaiveDate::from_ymd_opt(2013, 4, 23).unwrap())
                },
                Mandate {
                    name: "Schriftführerin des Bundesrates".into(),
                    party: None,
                    start: NaiveDate::from_ymd_opt(2006, 1, 1).unwrap(),
                    end: Some(NaiveDate::from_ymd_opt(2006, 6, 30).unwrap())
                },
            ]
        );

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
    async fn test_person_next_check_contacts_72959() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();
        let res = scraper.get_delegate("/person/72959").await?;
        println!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_person_special2() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();
        let delegate = scraper.get_delegate("/person/21029").await?;
        assert_eq!(delegate.name, "Rauch Johannes");
        assert_eq!(
            delegate.contact.mail,
            Some("johannes.rauch@sozialministerium.at".into())
        );
        assert_eq!(delegate.contact.phone_number, None);
        assert_eq!(
            delegate.contact.social_media,
            SocialMedia {
                twitter: Some("https://twitter.com/johannes_rauch".into()),
                facebook: None,
                website: Some("https://www.johannes-rauch.at/".into()),
                tiktok: None,
                youtube: None,
                instagram: None
            }
        );
        assert!(delegate.is_active);
        assert_eq!(delegate.gender, Gender::Male);

        Ok(())
    }
}

//   curl -s \
//    -w '\n' \
//    -H 'Content-Type: application/json' \
//    -d '{"M": ["M"], "W": ["W"], "R_WF": ["FR"], "GP": ["XXVII"]}' \
//    'https://www.parlament.gv.at/Filter/api/json/post?jsMode=EVAL&FBEZ=WFW_008&listeId=10008&showAll=true'
