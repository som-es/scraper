use std::collections::HashMap;

use chrono::NaiveDate;
use common_scrapes::{Contact, Delegate, Gender, Image, Mandate, Seat, SocialMedia};
use reqwest::Method;
use serde_json::Value;

use crate::{
    caching::{disk_caching, get_disk_cache_json},
    error::ScraperError,
    request::request,
    Scraper,
};

use super::{id_to_u64, last_path_segment, EuParliamentScraper};

const POLITICAL_GROUP_INDEX_CACHE_PATH: &str = "/eu-parliament/political-group-index";

#[derive(Debug, Clone, Copy)]
pub struct DelegateSeat {
    pub delegate_id: u64,
    pub seat_id: u64,
}

impl EuParliamentScraper {
    /// Ids (e.g. `"person/1234"`) of every MEP with an active mandate today.
    pub async fn current_mep_ids(&self) -> Result<Vec<String>, ScraperError> {
        let rows = self.get_all_pages("meps/show-current", "").await?;
        Ok(rows
            .into_iter()
            .flat_map(|row| row["id"].as_str().map(ToString::to_string))
            .collect())
    }

    pub async fn all_mep_ids(&self) -> Result<Vec<String>, ScraperError> {
        let rows = self.get_all_pages("meps", "").await?;
        Ok(rows
            .into_iter()
            .flat_map(|row| row["id"].as_str().map(ToString::to_string))
            .collect())
    }

    pub async fn current_seat_order(
        &self,
        strasbourg_hemicyle: bool,
    ) -> Result<Vec<DelegateSeat>, ScraperError> {
        let parliament = if strasbourg_hemicyle { "str" } else { "bru" };
        let url =
            format!("https://www.europarl.europa.eu/erpl-public/hemicycle//{parliament}/en.json");
        let res = request(Method::GET, self.client(), (), url).await?;
        Ok(res["seats"]
            .as_array()
            .ok_or(ScraperError::NotFound)?
            .iter()
            .flat_map(|seat| {
                let seat_id = seat["seatId"].as_u64()?;
                let delegate_id = seat["persId"].as_u64()?;
                Some(DelegateSeat {
                    delegate_id,
                    seat_id,
                })
            })
            .collect())
    }

    /// Maps a political-group organization id (e.g. `"org/PPE"`) to its short code
    /// (e.g. `"PPE"`). Needed to resolve `hasMembership.organization` references on a
    /// MEP's membership history, since those only carry the organization id.
    pub async fn political_group_codes(&self) -> Result<HashMap<String, String>, ScraperError> {
        if let Some(cached) =
            get_disk_cache_json(POLITICAL_GROUP_INDEX_CACHE_PATH, self.cache_config()).await
        {
            if let Ok(map) = serde_json::from_value::<HashMap<String, String>>(cached) {
                return Ok(map);
            }
        }

        let rows = self
            .get_all_pages("corporate-bodies", "body-classification=EU_POLITICAL_GROUP")
            .await?;

        let map: HashMap<String, String> = rows
            .into_iter()
            .flat_map(|row| {
                let id = row["id"].as_str()?.to_string();
                let label = row["label"].as_str()?.to_string();
                Some((id, label))
            })
            .collect();

        if let Ok(serialized) = serde_json::to_string(&map) {
            if let Err(e) = disk_caching(
                POLITICAL_GROUP_INDEX_CACHE_PATH,
                serialized,
                self.cache_config(),
            )
            .await
            {
                log::warn!("could not cache {POLITICAL_GROUP_INDEX_CACHE_PATH}: {e:?}");
            }
        }

        Ok(map)
    }

    pub async fn get_delegate(&mut self, mep_id: &str) -> Result<Delegate, ScraperError> {
        let path = format!("meps/{}", last_path_segment(mep_id));
        let raw = self.get_one_cached(&path).await?;
        let groups = self.political_group_codes().await?;
        let seats = self.current_seats().await?;
        raw_mep_into_delegate(&raw, &groups, seats).ok_or(ScraperError::InvalidDelegateInfoError)
    }

    async fn current_seats(&mut self) -> Result<&HashMap<u64, u64>, ScraperError> {
        if self.delegate_seats.is_none() {
            let seats = self.current_seat_order(true).await?;
            self.delegate_seats = Some(
                seats
                    .into_iter()
                    .map(|seat| (seat.delegate_id, seat.seat_id))
                    .collect(),
            );
        }
        Ok(self.delegate_seats.as_ref().unwrap())
    }

    pub async fn get_delegates(&mut self) -> Result<Vec<Delegate>, ScraperError> {
        let ids = self.current_mep_ids().await?;
        let groups = self.political_group_codes().await?;
        let seats = self.current_seats().await?.clone();

        let mut delegates = Vec::with_capacity(ids.len());
        for id in ids {
            let path = format!("meps/{}", last_path_segment(&id));
            let Ok(raw) = self.get_one_cached(&path).await else {
                continue;
            };
            if let Some(delegate) = raw_mep_into_delegate(&raw, &groups, &seats) {
                delegates.push(delegate);
            }
        }
        Ok(delegates)
    }
}

fn raw_mep_into_delegate(
    raw: &Value,
    groups: &HashMap<String, String>,
    seats: &HashMap<u64, u64>,
) -> Option<Delegate> {
    let id = id_to_u64(raw["id"].as_str()?)?;
    let seat_id = seats.get(&id);
    let name = raw["label"].as_str()?.to_string();

    let memberships = raw["hasMembership"].as_array().cloned().unwrap_or_default();

    let mut mandates: Vec<Mandate> = memberships
        .iter()
        .filter(|m| {
            m["membershipClassification"].as_str() == Some("def/ep-entities/EU_POLITICAL_GROUP")
        })
        .flat_map(|m| membership_to_mandate(m, groups))
        .collect();
    mandates.sort_by(|a, b| b.start.cmp(&a.start));

    let newest_mandate = mandates.first();
    let is_active = newest_mandate.map(|m| m.end.is_none()).unwrap_or(false);
    let party = newest_mandate.and_then(|m| m.party.clone());

    let gender = match raw["hasGender"].as_str() {
        Some(uri) if uri.ends_with("MALE") => Gender::Male,
        Some(uri) if uri.ends_with("FEMALE") => Gender::Female,
        _ => Gender::Unknown,
    };

    // EU MEPs represent an entire member state rather than a national electoral
    // district, so `constituency` here is a country code (e.g. "BEL"), not a district
    // name like AT's "3B - Waldviertel".
    let constituency = raw["citizenship"]
        .as_str()
        .map(last_path_segment)
        .map(ToString::to_string);

    let mail = raw["hasEmail"]
        .as_str()
        .map(|mail| mail.trim_start_matches("mailto:").to_string());

    let image = raw["img"].as_str().map(|url| Image {
        url: url.to_string(),
        copyright: None,
    });

    Some(Delegate {
        id,
        name,
        is_active,
        image,
        party,
        constituency,
        council: Some("ep".to_string()),
        divisions: None,
        seat: seat_id.map(|seat_id| Seat {
            row: *seat_id as u16,
            col: 0,
        }),
        contact: Contact {
            mail,
            phone_number: None,
            social_media: SocialMedia::default(),
        },
        mandates,
        gender,
        // not exposed by the `meps/{id}` endpoint
        birthdate: None,
        prev_name_info: None,
    })
}

fn membership_to_mandate(membership: &Value, groups: &HashMap<String, String>) -> Option<Mandate> {
    let org_id = membership["organization"].as_str()?;
    let code = groups.get(org_id).cloned();

    let period = &membership["memberDuring"];
    let start = NaiveDate::parse_from_str(period["startDate"].as_str()?, "%Y-%m-%d").ok()?;
    let end = period["endDate"]
        .as_str()
        .and_then(|end| NaiveDate::parse_from_str(end, "%Y-%m-%d").ok());

    Some(Mandate {
        name: "Member of the European Parliament".to_string(),
        party: code,
        constituency: None,
        start,
        end,
        function: Some("MEP".to_string()),
        is_nr: true,
        is_gov: false,
        is_ministry: false,
        is_chancellor: false,
    })
}

#[cfg(test)]
mod tests {
    use common_scrapes::Seat;

    use crate::caching::CacheConfig;

    use super::*;

    #[tokio::test]
    async fn test_get_delegate() {
        let mut scraper = EuParliamentScraper::default();
        let delegate = scraper.get_delegate("person/1294").await.unwrap();
        assert_eq!(delegate.name, "Elio DI RUPO");
        assert_eq!(delegate.constituency.as_deref(), Some("BEL"));
        assert_eq!(delegate.gender, Gender::Male);
        assert!(!delegate.mandates.is_empty());
    }

    #[tokio::test]
    async fn test_current_mep_ids_non_empty() {
        let scraper = EuParliamentScraper::default();
        let ids = scraper.current_mep_ids().await.unwrap();
        assert!(ids.len() > 500);
    }

    #[tokio::test]
    async fn test_get_delegate_andreas_schieder() {
        let mut scraper = EuParliamentScraper::default();
        let delegate = scraper.get_delegate("person/197670").await.unwrap();

        scraper.cache_config = CacheConfig {
            cache_dir: "./test-data".into(),
            ignore_current_gp_cache: false,
            redo_secs: u64::MAX,
        };

        assert_eq!(delegate.seat.unwrap(), Seat { row: 131, col: 0 });
        assert_eq!(delegate.name, "Andreas SCHIEDER");
        assert_eq!(delegate.constituency.as_deref(), Some("AUT"));
        assert_eq!(delegate.gender, Gender::Male);
        assert_eq!(delegate.party.as_deref(), Some("S&D"));
        assert!(delegate.is_active);
        assert_eq!(
            delegate.image.as_ref().unwrap().url,
            "https://www.europarl.europa.eu/mepphoto/197670.jpg"
        );

        // two EU_POLITICAL_GROUP memberships on record: pre-2024 S&D, then re-elected
        // 2024 term S&D, newest first
        assert!(delegate.mandates.len() >= 2);
        assert_eq!(delegate.mandates[0].party.as_deref(), Some("S&D"));
        assert!(delegate.mandates[0].end.is_none());
    }

    #[tokio::test]
    async fn test_current_seat_order() {
        let scraper = EuParliamentScraper::default();
        let seat_order = scraper.current_seat_order(true).await.unwrap();
        dbg!(seat_order);
    }
}
