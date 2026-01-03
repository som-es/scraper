use std::future::Future;

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::OffsetComponents;
use chrono_tz::{Europe::Vienna, Tz};
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert_vienna_datetime_to_utc;
use crate::{error::ScraperError, AustriaScraper, Scraper};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FilterDimension {
    Gremium,
    Terminart,
    Ort,
    Themen,
    Daterange,
    Format,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Gremium {
    Nationalrat,
    Bundesrat,
    Bundesversammlung,
    #[serde(rename = "Parlamentsdirektion / Klubs")]
    ParlamentsdirektionKlubs,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Terminart {
    #[serde(rename = "Plenarsitzung")]
    Plenarsitzung,
    #[serde(rename = "Sitzung der Bundesversammlung")]
    Bundesversammlung,
    #[serde(rename = "Ausschusssitzung oder Ausschuss")]
    Ausschuss,
    #[serde(rename = "Abteilungsintern")]
    Abteilungsintern,
    #[serde(rename = "Klubveranstaltung")]
    Klubveranstaltung,
    #[serde(rename = "Parlamentarische Enquete")]
    Enquete,
    #[serde(rename = "Veranstaltung")]
    Veranstaltung,
    #[serde(rename = "Fest-/Gedenksitzung")]
    FestGedenksitzung,
    #[serde(rename = "Demokratiebildung")]
    Demokratiebildung,
    #[serde(rename = "Präsidialkonferenz")]
    Praesidialkonferenz,
    #[serde(rename = "Sonstiger Termin")]
    Sonstiger,
    #[serde(rename = "Pressekonferenz")]
    Pressekonferenz,
    #[serde(rename = "Führung")]
    Fuehrung,
    Other(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct DateRange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<NaiveDateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<NaiveDateTime>,
}

impl DateRange {
    pub fn to_vec(&self) -> Vec<Option<String>> {
        vec![
            self.from
                .map(|x| x.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()),
            self.to
                .map(|x| x.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DateFilter {
    #[serde(skip_serializing_if = "Vec::is_empty", rename = "GREMIUM")]
    pub gremium: Vec<Gremium>,
    #[serde(skip_serializing_if = "Vec::is_empty", rename = "TERMINART")]
    pub terminart: Vec<Terminart>,
    #[serde(skip_serializing_if = "Vec::is_empty", rename = "ORT")]
    pub ort: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", rename = "THEMEN")]
    pub themen: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "DATERANGE")]
    pub daterange: Option<DateRange>,
    #[serde(skip_serializing_if = "Vec::is_empty", rename = "FORMAT")]
    pub format: Vec<String>,
}

impl DateFilter {
    pub fn to_payload(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();

        if !self.gremium.is_empty() {
            map.insert("GREMIUM".into(), serde_json::json!(self.gremium));
        }
        if !self.terminart.is_empty() {
            map.insert("TERMINART".into(), serde_json::json!(self.terminart));
        }
        if !self.ort.is_empty() {
            map.insert("ORT".into(), serde_json::json!(self.ort));
        }
        if !self.themen.is_empty() {
            map.insert("THEMEN".into(), serde_json::json!(self.themen));
        }
        if let Some(dr) = &self.daterange {
            map.insert("DATERANGE".into(), serde_json::json!(dr.to_vec()));
        }
        if !self.format.is_empty() {
            map.insert("FORMAT".into(), serde_json::json!(self.format));
        }

        serde_json::Value::Object(map)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ParliamentDate {
    pub date: NaiveDate,
    pub date_sort: Option<DateTime<Utc>>,
    pub time: Option<NaiveTime>,
    pub title: String,
    pub link: Option<String>,
    pub appointment_type: Terminart,
    pub appointment_type_display: Option<String>,
    pub info_link: Option<String>,
    pub location: Option<String>,
    pub topics: Vec<String>,
    pub committee: Gremium,
    pub year_month: Option<String>, // keep raw "yyyymm"
    pub repeated_date: Option<NaiveDate>,
    pub month_year: Option<String>,
    pub media_relevant: bool,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub past: bool,
    pub day_with_date: Option<String>, // textual day + date
    pub tour_format: Option<String>,
    pub tech1: Option<i64>,
    pub tech2: Option<String>,
    pub tech3: Vec<String>,
}

impl ParliamentDate {
    fn new(value: Value) -> Option<Self> {
        let row = value;
        let parse_date = |s: &str| NaiveDate::parse_from_str(s, "%d.%m.%Y").ok();
        let parse_time = |s: &str| NaiveTime::parse_from_str(s, "%H:%M").ok();
        let parse_datetime = |s: &str| {
            NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
                .ok()
                .map(convert_vienna_datetime_to_utc)
                .flatten()
        };

        let date = row.get(0).and_then(|v| v.as_str()).and_then(parse_date)?;

        let date_sort: Option<DateTime<Utc>> = row
            .get(1)
            .and_then(|v| v.as_str())
            .and_then(|s| {
                NaiveDateTime::parse_from_str(s, "%Y%m%d%H%M")
                    .ok()
                    .map(convert_vienna_datetime_to_utc)
            })
            .flatten();
        let time = row.get(2).and_then(|v| v.as_str()).and_then(parse_time);
        let title = row.get(3).and_then(|v| v.as_str()).map(String::from)?;
        let link = row.get(4).and_then(|v| v.as_str()).map(String::from);
        let appointment_type = row.get(5).and_then(|v| {
            serde_json::from_value(v.clone())
                .unwrap_or_else(|_e| Some(Terminart::Other(v.as_str()?.to_string())))
        })?;
        let appointment_type_display = row.get(6).and_then(|v| v.as_str()).map(String::from);
        let info_link = row.get(7).and_then(|v| v.as_str()).map(String::from);
        let location = row.get(8).and_then(|v| v.as_str()).map(String::from);

        let topics = row
            .get(9)
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .unwrap_or_default();

        let committee = row.get(10).and_then(|v| {
            serde_json::from_value(v.clone())
                .unwrap_or_else(|_e| Some(Gremium::Other(v.as_str()?.to_string())))
        })?;
        let year_month = row.get(11).and_then(|v| v.as_str()).map(String::from);
        let repeated_date = row.get(12).and_then(|v| v.as_str()).and_then(parse_date);
        let month_year = row.get(13).and_then(|v| v.as_str()).map(String::from);

        let media_relevant = row
            .get(14)
            .and_then(|v| v.as_str())
            .map(|s| s == "J")
            .unwrap_or(false);
        let start = row
            .get(15)
            .and_then(|v| v.as_str())
            .and_then(parse_datetime);
        let end = row
            .get(16)
            .and_then(|v| v.as_str())
            .and_then(parse_datetime);

        let past = row
            .get(17)
            .and_then(|v| v.as_str())
            .map(|s| s == "J")
            .unwrap_or(false);

        let day_with_date = row.get(18).and_then(|v| v.as_str()).map(String::from);
        let tour_format = row.get(19).and_then(|v| v.as_str()).map(String::from);

        let tech1 = row
            .get(20)
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok());
        let tech2 = row.get(21).and_then(|v| v.as_str()).map(String::from);
        let tech3 = row
            .get(22)
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .unwrap_or_default();

        Some(ParliamentDate {
            date,
            date_sort,
            time,
            title,
            link,
            appointment_type,
            appointment_type_display,
            info_link,
            location,
            topics,
            committee,
            year_month,
            repeated_date,
            month_year,
            media_relevant,
            start,
            end,
            past,
            day_with_date,
            tour_format,
            tech1,
            tech2,
            tech3,
        })
    }
}

pub trait Dates: Scraper + Sync {
    const FILTER_PATH: &'static str =
        "/Filter/api/filter/data/600?js=eval&showAll=true&export=true";

    fn dates_payload(&self, date_filter: &DateFilter) -> Self::Payload;
    fn raw_dates_into_parliament_dates(
        &self,
        raw_legis: Value,
    ) -> impl Future<Output = Option<Vec<ParliamentDate>>>;

    fn post_dates_payload(
        &self,
        date_filter: &DateFilter,
    ) -> impl Future<Output = Result<Value, ScraperError>> {
        self.post_payload_filter(self.dates_payload(date_filter), Self::FILTER_PATH)
    }
}

impl Dates for AustriaScraper {
    fn dates_payload(&self, date_filter: &DateFilter) -> Self::Payload {
        println!("{}", date_filter.to_payload());
        date_filter.to_payload()
    }

    async fn raw_dates_into_parliament_dates(
        &self,
        raw_dates: Value,
    ) -> Option<Vec<ParliamentDate>> {
        Some(
            raw_dates["rows"]
                .as_array()?
                .iter()
                .cloned()
                .flat_map(ParliamentDate::new)
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDateTime, Utc};

    use crate::{
        dates::{DateFilter, DateRange, Dates, Terminart},
        AustriaScraper,
    };

    #[tokio::test]
    async fn test_dates() {
        let scraper = AustriaScraper::default();
        let res = scraper
            .post_dates_payload(&DateFilter::default())
            .await
            .unwrap();
        let dates = scraper.raw_dates_into_parliament_dates(res).await.unwrap();
        dbg!(dates);
    }

    #[tokio::test]
    async fn test_dates_filtering() {
        let now: NaiveDateTime = Utc::now().naive_utc();
        // today();
        let scraper = AustriaScraper::default();
        let res = scraper
            .post_dates_payload(&DateFilter {
                daterange: Some(DateRange {
                    from: Some(now),
                    to: None,
                }),
                terminart: vec![Terminart::Plenarsitzung],
                ..Default::default()
            })
            .await
            .unwrap();
        let dates = scraper.raw_dates_into_parliament_dates(res).await.unwrap();
        dbg!(dates);
    }
}
