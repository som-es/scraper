use async_trait::async_trait;
use chrono::NaiveDate;
use common_scrapes::{
    Contact, Delegate, Gender, Mandate, Seat, SocialMedia, SocialMedium, SocialMediumChannel,
};

use regex::Regex;
use reqwest::Method;
use serde_json::{json, Value};

use crate::{error::ScraperError, request::request};

use super::{AustriaScraper, Scraper};

pub const PERSON_PATH_IDX: usize = 4;
pub const BEREICHE_IDX: usize = 6;

pub struct BereichsprecherInfo {
    pub path: String,
    pub bereiche: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct DelegateInfo {
    pub path: String,
    pub gender: String,
}

impl From<&str> for DelegateInfo {
    fn from(value: &str) -> Self {
        DelegateInfo {
            path: value.to_string(),
            gender: "".to_string(),
        }
    }
}

impl AustriaScraper {
    pub fn bereichsprecher_info(&self, gegenstaende: Value) -> Option<Vec<BereichsprecherInfo>> {
        Some(
            gegenstaende[Self::LIST_ENTRIES]
                .as_array()?
                .iter()
                .flat_map(|gegenstand| {
                    let geg = gegenstand.as_array()?;
                    let path = geg[PERSON_PATH_IDX].as_str()?.to_string();
                    let bereiche = serde_json::from_str::<Vec<Value>>(geg[BEREICHE_IDX].as_str()?)
                        .ok()?
                        .iter()
                        .flat_map(Value::as_str)
                        .map(ToString::to_string)
                        .collect();

                    Some(BereichsprecherInfo { path, bereiche })
                })
                .collect(),
        )
    }

    pub fn delegate_info(&self, gegenstaende: Value) -> Option<Vec<DelegateInfo>> {
        Some(
            gegenstaende[Self::LIST_ENTRIES]
                .as_array()?
                .iter()
                .flat_map(|gegenstand| {
                    let geg = gegenstand.as_array()?;
                    let path = geg[4].as_str()?.to_string();
                    let gender = geg[5].as_str()?.to_string();
                    Some(DelegateInfo { path, gender })
                })
                .collect(),
        )
    }
}

#[async_trait]
pub trait DelegateScraper: Scraper {
    const DELEGATE_FILTER_PATH: &'static str;
    const DELEGATE_BS_FILTER_PATH: &'static str;

    fn persons_payload(&self) -> Self::Payload;

    async fn post_persons_payload_bs(&self) -> Result<Value, ScraperError> {
        request(
            Method::POST,
            self.client(),
            json!({
              "CODE": [
                "BS"
              ]
            }),
            format!(
                "{base_url}{filter}",
                base_url = Self::BASE_URL,
                filter = Self::DELEGATE_BS_FILTER_PATH
            ),
        )
        .await
    }

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

    fn person_paths(&self, persons: Value) -> Option<Vec<DelegateInfo>>;
    fn person_paths_bs(&self, persons: Value) -> Option<Vec<BereichsprecherInfo>>;

    fn person_info_into_delegate(person_info: Value) -> Option<Delegate>;

    async fn get_delegate(&self, info: DelegateInfo) -> Result<Delegate, ScraperError> {
        let person_info = self.get_gegenstand_info(&info.path).await?;
        let mut delegate = Self::person_info_into_delegate(person_info)
            .ok_or(ScraperError::InvalidDelegateInfoError)?;
        let new_gender = Gender::from_indicators([info.gender.as_str()].into_iter());
        if new_gender != Gender::Unknown {
            if delegate.gender == Gender::Unknown {
                delegate.gender = new_gender;
            } else {
                // should work HOWEVER they sometimes used the wrong gender e.g. for Dipl.-Kfm. Ruth Feldgrill-Zankel
                // /person/64401
                // assert_eq!(delegate.gender, new_gender)
            }
        }
        Ok(delegate)
    }
    async fn get_delegate_bs(
        &self,
        bs_info: BereichsprecherInfo,
    ) -> Result<Delegate, ScraperError> {
        let person_info = self.get_gegenstand_info(&bs_info.path).await?;
        let mut delegate = Self::person_info_into_delegate(person_info)
            .ok_or(ScraperError::InvalidDelegateInfoError)?;
        delegate.divisions = Some(bs_info.bereiche);
        Ok(delegate)
    }
}

impl DelegateScraper for AustriaScraper {
    const DELEGATE_FILTER_PATH: &'static str =
        // "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFW_008&listeId=10008&showAll=true";
        "/Filter/api/filter/data/10400?js=eval&showAll=true";

    const DELEGATE_BS_FILTER_PATH: &'static str =
        "/Filter/api/filter/data/403?js=eval&showAll=true&export=true";

    #[inline]
    fn persons_payload(&self) -> Value {
        // json!({"M": ["M"], "W": ["W"], /*"R_WF": ["FR"],*//* "NRBR": [self.scrape_info.council_type],*/ "GP": self.scrape_info.gp})
        json! {{}}
    }

    fn person_info_into_delegate(person_info: Value) -> Option<Delegate> {
        // println!("{}", serde_json::to_string_pretty(&person_info).unwrap());

        // let gender_indicator = mandate[0]["mandat"].as_str();

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

        let birthdate =
            extract_birthdate(&person_info["content"]["biografie"]["kurzbiografie"]["gebtext"]);

        // choose newest mandate
        let newest_mandate = mandates.first();

        let party = newest_mandate.and_then(|mandate| mandate.party.clone());

        let gender_indicators = mandates.iter().map(|mandate| mandate.name.as_str());

        let constituency = newest_mandate.and_then(|mandate| mandate.constituency.clone());

        let phone_number = api_contact_info
            .get("telefonnr")
            .and_then(|phones| phones.get(0).and_then(|phone| phone.as_str()))
            .map(ToString::to_string);

        let mail = api_contact_info
            .get("mail")
            .and_then(|mails| mails.get(0).and_then(|mail| mail.as_str()))
            .map(ToString::to_string);

        let social_media = api_contact_info
            .get("socialMedia")
            .and_then(extract_social_media)
            .map(SocialMedia::from)
            .unwrap_or_default();

        let seat = &api_person_info["sitzplatz"];

        let seat = if let Some(seat) = seat.as_str() {
            let (row, col) = seat.split_once('/')?;
            if col.is_empty() {
                None
            } else {
                let (row, col) = (row.parse::<u16>().ok()?, col.parse::<u16>().ok()?);
                Some(Seat { row, col })
            }
        } else {
            None
        };

        Some(Delegate {
            is_active,
            image_url,
            id: id.as_u64()?,
            name: name.as_str()?.to_string(),
            party,
            divisions: None,
            constituency,
            council: council.as_str().map(|council| council.to_string()),
            seat,
            gender: Gender::from_indicators(gender_indicators),
            birthdate,
            mandates,
            contact: Contact {
                mail,
                phone_number,
                social_media,
            },
        })
    }

    fn person_paths(&self, persons: Value) -> Option<Vec<DelegateInfo>> {
        self.delegate_info(persons)
    }

    fn person_paths_bs(&self, persons: Value) -> Option<Vec<BereichsprecherInfo>> {
        self.bereichsprecher_info(persons)
    }
}

/*pub fn extract_party() -> Option<Party> {

}*/

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
    let constituency = raw_mandate["wahlkreis"].as_str().map(ToString::to_string);
    let function = raw_mandate["funktion"].as_str().map(ToString::to_string);

    let start = NaiveDate::parse_from_str(&start, "%d.%m.%Y").ok()?;

    let end = if end.is_empty() {
        None
    } else {
        NaiveDate::parse_from_str(&end, "%d.%m.%Y").ok()
    };

    let ref_function = function.as_ref().map(|x| x.as_str());

    let mut is_gov = false;
    let mut is_ministry = false;
    let mut is_chancellor = false;
    if ref_function == Some("BM") {
        is_gov = true;
        is_ministry = true;
    }
    
    if ref_function == Some("BK") {
        is_gov = true;
        is_chancellor = true;
    }

    if ref_function == Some("STS") {
        is_gov = true;
    }
    
    let mut is_nr = false;
    if ref_function == Some("NR") {
        is_nr =  true;
    }

    Some(Mandate {
        name,
        party,
        constituency,
        start,
        end,
        function,
        is_nr,
        is_gov,
        is_ministry,
        is_chancellor,
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

pub fn extract_birthdate(raw_birthinfo: &Value) -> Option<NaiveDate> {
    if raw_birthinfo.is_null() {
        return None;
    }

    // dd.mm.yyyy
    let date_matcher: Regex = Regex::new("(\\d{2}\\.){2}\\d{4}").ok()?;
    let date = date_matcher.find(raw_birthinfo.as_str()?)?;

    NaiveDate::parse_from_str(date.as_str(), "%d.%m.%Y").ok()
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
    async fn test_division_delegate_scraper() {
        let scraper = AustriaScraper::default();

        let info = scraper.post_persons_payload_bs().await.unwrap();
        for path in scraper.person_paths_bs(info).unwrap() {
            let delegate = scraper.get_delegate_bs(path).await.unwrap();
            println!(
                "delegate: {}, divs: {:?}",
                delegate.name, delegate.divisions
            )
        }
    }

    #[tokio::test]
    async fn test_austria_person_info_scraper() {
        let scraper = AustriaScraper::default();
        let val = scraper.get_gegenstand_info("/person/16218").await.unwrap();

        // und sitzplatz
        let person_info = &val["content"]["personInfo"];
        println!("person_info: {person_info}");

        let mandate = &val["content"]["banner"]["mandate"];

        let _party = &mandate[0]["wahlpartei"];

        let name = &val["meta"]["description"];

        println!("name: {name}");

        let _pretty_val = serde_json::to_string_pretty(&val).unwrap();

        let delegate = AustriaScraper::person_info_into_delegate(val);
        println!("delegate: {delegate:?}");

        // let val = AustriaScraper::get_person_info("/person/16218").await;
        // println!("val: {pretty_val}");
    }

    #[tokio::test]
    async fn test_get_delegate() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();

        let res = scraper.get_delegate("/person/16218".into()).await?;
        println!("res: {res:?}");

        assert_eq!(res.name, "Diesner-Wais Martina");
        assert_eq!(res.party.unwrap(), "ÖVP");
        // subject to change in coming elections
        assert_eq!(res.council.unwrap(), "nr");

        assert_eq!(res.birthdate.unwrap().to_string(), "1968-02-10");

        assert_eq!(res.contact.phone_number, Some("+43 1 401 10-4687".into()));
        assert_eq!(
            res.contact.mail,
            Some("martina.diesner-wais@parlament.gv.at".into())
        );
        // assert_eq!(res.contact.social_media, SocialMedia::default());
        assert_eq!(res.gender, Gender::Female);
        assert_eq!(
            res.mandates,
            [
                Mandate {
                    name: "Abgeordnete zum Nationalrat".into(),
                    constituency: Some("3B - Waldviertel".to_string()),
                    party: Some("ÖVP".into()),
                    start: NaiveDate::from_ymd_opt(2013, 10, 29).unwrap(),
                    end: None,
                    function: Some("NR".into()),
                    is_gov: false,
                    is_nr: true,
                    is_ministry: false,
                    is_chancellor: false,
                },
                Mandate {
                    name: "Mitglied des Bundesrates".into(),
                    party: Some("ÖVP".into()),
                    constituency: Some(
                        "In den Bundesrat entsendet vom Niederösterreichischen Landtag".into()
                    ),
                    start: NaiveDate::from_ymd_opt(2003, 4, 24).unwrap(),
                    end: Some(NaiveDate::from_ymd_opt(2013, 4, 23).unwrap()),
                    function: Some("BR".into()),
                    is_gov: false,
                    is_nr: false,
                    is_ministry: false,
                    is_chancellor: false,
                },
                Mandate {
                    name: "Schriftführerin des Bundesrates".into(),
                    party: None,
                    constituency: None,
                    start: NaiveDate::from_ymd_opt(2010, 11, 5).unwrap(),
                    is_chancellor: false,
                    end: Some(NaiveDate::from_ymd_opt(2013, 4, 23).unwrap()),
                    is_gov: false,
                    is_ministry: false,
                    function: Some("ZSB".into()),
                    is_nr: false,
                },
                Mandate {
                    name: "Schriftführerin des Bundesrates".into(),
                    is_gov: false,
                    is_chancellor: false,
                    is_ministry: false,
                    is_nr: false,
                    party: None,
                    constituency: None,
                    function: Some("ZSB".into()),
                    start: NaiveDate::from_ymd_opt(2006, 1, 1).unwrap(),
                    end: Some(NaiveDate::from_ymd_opt(2006, 6, 30).unwrap())
                },
            ]
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_delegate_84060() {
        let scraper = AustriaScraper::default();

        let res = scraper.get_delegate("/person/84060".into()).await.unwrap();
        println!("res: {res:?}");
    }

    #[tokio::test]
    async fn test_get_all_persons() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();

        let mut delegates = vec![];

        let mut count = 0;
        let persons = scraper.post_persons_payload().await?;

        let mut title_candidates = vec![];

        for person_path in scraper.person_paths(persons).unwrap_or_default() {
            let Ok(delegate) = scraper.get_delegate(person_path).await else {
                continue;
            };

            if delegate.name.contains(".") {
                title_candidates.push(delegate.name.clone());
            }

            if delegate.is_active && delegate.council == Some("nr".into()) {
                count += 1;
                println!(
                    "count: {count} del: {}, {:?}",
                    delegate.name, delegate.party
                );
            }

            // println!("del: {delegate:?}");
            delegates.push(delegate);
        }

        println!("title_candidates: {title_candidates:?}");

        // let contents = serde_json::to_string(&delegates).unwrap();
        // std::fs::write("../delegates.json", contents).unwrap();

        Ok(())
    }

    #[tokio::test]
    async fn test_person_paths() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();

        let persons = scraper.post_persons_payload().await?;
        let person_paths = scraper.person_paths(persons).unwrap();

        for person in person_paths {
            println!("{person:?}");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_person_special() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();
        let res = scraper.get_delegate("/person/55727".into()).await?;
        println!("res: {res:?}");

        Ok(())
    }

    #[tokio::test]
    async fn test_person_next_check_contacts_72959() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();
        let res = scraper.get_delegate("/person/72959".into()).await?;
        println!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_person_special2() -> Result<(), ScraperError> {
        let scraper = AustriaScraper::default();
        let delegate = scraper.get_delegate("/person/21029".into()).await?;
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

        assert_eq!(
            delegate.birthdate,
            Some(NaiveDate::parse_from_str("1959-04-24", "%Y-%m-%d").unwrap())
        );

        Ok(())
    }

    #[tokio::test]
    pub async fn test_gender_of_previously_unknown() {
        let scraper = AustriaScraper::default();
        let delegate = scraper.get_delegate("/person/2814".into()).await.unwrap();
        assert_eq!(Gender::Female, delegate.gender)
    }

    #[tokio::test]
    async fn test_bundesrat_to_nr_switcher() {
        let scraper = AustriaScraper::default();
        let delegate = scraper.get_delegate("/person/6407".into()).await.unwrap();
        println!("delegate: {delegate:?}");
    }

    #[tokio::test]
    pub async fn test_seat_lost() {
        let scraper = AustriaScraper::default();
        let delegate = scraper.get_delegate("/person/5682".into()).await.unwrap();
        assert_eq!(Gender::Male, delegate.gender);
        let delegate = scraper.get_gegenstand_info("/person/5682").await.unwrap();
        println!(
            "delegate: {}",
            serde_json::to_string_pretty(&delegate).unwrap()
        );
    }
}

//   curl -s \
//    -w '\n' \
//    -H 'Content-Type: application/json' \
//    -d '{"M": ["M"], "W": ["W"], "R_WF": ["FR"], "GP": ["XXVII"]}' \
//    'https://www.parlament.gv.at/Filter/api/json/post?jsMode=EVAL&FBEZ=WFW_008&listeId=10008&showAll=true'
