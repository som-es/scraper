use std::collections::HashSet;

use chrono::NaiveDate;
use once_cell::sync::Lazy;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GovOfficial {
    pub government: String,
    pub function: String,
    pub ministry: String,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub delegate_id: u64,
    // pub delegate: Delegate,
}

pub static MALE_INDICATOR: Lazy<HashSet<&str>> = Lazy::new(|| {
    HashSet::from([
        "M",
        "Staatssekretär",
        "Bundesminister",
        "Bundeskanzler",
        "Bundespräsident",
        "Abgeordneter",
        "Schriftführer",
        "Landeshauptmann",
        "Präsident",
        "Vizepräsident",
    ])
});

pub static FEMALE_INDICATOR: Lazy<HashSet<&str>> = Lazy::new(|| {
    HashSet::from([
        "W",
        "Staatssekretärin",
        "Bundesministerin",
        "Bundeskanzlerin",
        "Bundespräsidentin",
        "Abgeordnete",
        "Schriftführerin",
        "Landeshauptfrau",
        "Präsidentin",
        "Vizepräsidentin",
    ])
});

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Gender {
    Female,
    Male,
    Unknown,
}

impl Gender {
    pub fn from_sentence_analysis(indicator_sentence: &str) -> Gender {
        for indicator in indicator_sentence.split_whitespace() {
            let gender = Gender::indicate(indicator);
            match gender {
                Self::Female | Self::Male => return gender,
                _ => continue,
            }
        }
        Gender::Unknown
    }
    pub fn from_indicators<'a, I: IntoIterator<Item = &'a str>>(indicators: I) -> Gender {
        for indicator_sentence in indicators {
            let gender = Gender::from_sentence_analysis(indicator_sentence);
            match gender {
                Self::Female | Self::Male => return gender,
                _ => continue,
            }
        }
        Gender::Unknown
        //   curl -s \
        //    -w '\n' \
        //    -H 'Content-Type: application/json' \
        //    -d '{"M": ["M"], "W": ["W"], "R_WF": ["FR"], "GP": ["XXVII"]}' \
        //    'https://www.parlament.gv.at/Filter/api/json/post?jsMode=EVAL&FBEZ=WFW_008&listeId=10008&showAll=true'
    }
}

/*impl From<Option<&str>> for Gender {
    fn from(indicators: Option<&str>) -> Self {
        let Some(indicators) = indicators else {
            return Gender::Unknown
        };
        Gender::from_idicators(indicators)
    }
}*/

impl Gender {
    pub fn indicate(indicator: &str) -> Gender {
        if MALE_INDICATOR.contains(&indicator) {
            Gender::Male
        } else if FEMALE_INDICATOR.contains(&indicator) {
            Gender::Female
        } else {
            Gender::Unknown
        }
    }

    pub fn to_string_opt(&self) -> Option<String> {
        match self {
            Gender::Female => Some("f".to_string()),
            Gender::Male => Some("m".to_string()),
            Gender::Unknown => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Gender;

    #[test]
    fn gender_from_indicators() {
        let indicators = ["Mitglied", "Abgeordnete", "Schriftführerin"];
        let gender = Gender::from_indicators(indicators);
        assert_eq!(gender, Gender::Female)
    }

    #[test]
    fn gender_from_indicator_sentences() {
        let indicators = [
            "Mitglied des Bundesrates",
            "Abgeordnete zum Nationalrat",
            "Schriftführerin des Bundesrates",
        ];
        let gender = Gender::from_indicators(indicators);
        assert_eq!(gender, Gender::Female)
    }

    #[test]
    fn unknown_gender_indicator_at_last() {
        let indicators = [
            "Mitglied des Bundesrates",
            "Abgeordnete zum Nationalrat",
            "Abgeordnete zum Nationalrat",
            "Vizepräsidentin des Bundesrates",
        ];
        let gender = Gender::from_indicators(indicators);
        assert_eq!(gender, Gender::Female)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Image {
    pub url: String,
    pub copyright: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Delegate {
    pub id: u64,
    pub name: String,
    pub is_active: bool,
    pub image: Option<Image>,
    pub party: Option<String>,
    pub constituency: Option<String>,
    pub council: Option<String>,
    pub divisions: Option<Vec<String>>,
    pub seat: Option<Seat>,
    pub contact: Contact,
    pub mandates: Vec<Mandate>,
    pub gender: Gender,
    pub birthdate: Option<NaiveDate>,
    pub prev_name_info: Option<PrevNameInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct PrevNameInfo {
    pub prev_name: String,
    pub till: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Mandate {
    pub name: String,
    pub party: Option<String>,
    pub constituency: Option<String>,
    pub start: NaiveDate,
    pub end: Option<NaiveDate>,
    pub function: Option<String>,
    pub is_nr: bool,
    pub is_gov: bool,
    pub is_ministry: bool,
    pub is_chancellor: bool,
}

impl Mandate {
    #[inline]
    pub fn is_active(&self) -> bool {
        self.end.is_none()
    }
}

#[cfg(test)]
#[test]
fn test_mandate_is_active() {
    let mandate = Mandate {
        name: "Abgeordnete zum Nationalrat".into(),
        start: NaiveDate::from_ymd_opt(2019, 10, 29).unwrap(),
        party: None,
        constituency: None,
        end: None,
        is_nr: false,
        function: Some("hihi".into()),
        is_gov: false,
        is_chancellor: false,
        is_ministry: false,
    };
    assert!(mandate.is_active())
}

#[cfg(test)]
#[test]
fn test_mandate_is_not_active() {
    let mandate = Mandate {
        name: "Abgeordnete zum Nationalrat".into(),
        party: None,
        constituency: None,
        start: NaiveDate::from_ymd_opt(2019, 10, 29).unwrap(),
        end: Some(NaiveDate::from_ymd_opt(2019, 10, 29).unwrap()),
        function: Some("hihi".into()),
        is_nr: false,
        is_gov: false,
        is_chancellor: false,
        is_ministry: false,
    };
    assert!(!mandate.is_active())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Contact {
    pub mail: Option<String>,
    pub phone_number: Option<String>,
    pub social_media: SocialMedia,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct SocialMedia {
    pub twitter: Option<String>,
    pub facebook: Option<String>,
    pub website: Option<String>,
    pub tiktok: Option<String>,
    pub youtube: Option<String>,
    pub instagram: Option<String>,
}

pub enum SocialMediumChannel {
    Facebook,
    Twitter,
    Instagram,
    Youtube,
    Website,
    TikTok,
}

pub struct SocialMedium {
    pub url: String,
    pub channel: SocialMediumChannel,
}

impl From<Vec<SocialMedium>> for SocialMedia {
    fn from(social_media: Vec<SocialMedium>) -> Self {
        social_media
            .into_iter()
            .fold(SocialMedia::default(), |mut social_media, sm| {
                match sm.channel {
                    SocialMediumChannel::Facebook => social_media.facebook = Some(sm.url),
                    SocialMediumChannel::Twitter => social_media.twitter = Some(sm.url),
                    SocialMediumChannel::Instagram => social_media.instagram = Some(sm.url),
                    SocialMediumChannel::Youtube => social_media.youtube = Some(sm.url),
                    SocialMediumChannel::Website => social_media.website = Some(sm.url),
                    SocialMediumChannel::TikTok => social_media.tiktok = Some(sm.url),
                }
                social_media
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Seat {
    pub row: u16,
    pub col: u16,
}
