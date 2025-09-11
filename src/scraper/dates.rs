use std::future::Future;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use serde::Serialize;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Debug, Clone, Serialize)]
pub enum Gremium {
    Nationalrat,
    Bundesrat,
    Bundesversammlung,
    #[serde(rename = "Parlamentsdirektion / Klubs")]
    ParlamentsdirektionKlubs,
}

#[derive(Debug, Clone, Serialize)]
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
        vec![self.from.map(|x| x.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()), 
        self.to.map(|x| x.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())]
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


#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ParliamentDate {
    pub datum: Option<NaiveDate>,
    pub datesort: Option<NaiveDateTime>,
    pub uhrzeit: Option<NaiveTime>,
    pub bezeichnung: Option<String>,
    pub link: Option<String>,
    pub terminart: Option<String>,
    pub terminart_anzeige: Option<String>,
    pub info_link: Option<String>,
    pub ort: Option<String>,
    pub themen: Vec<String>,
    pub gremium: Option<String>,
    pub jahr_monat: Option<String>, // keep raw "yyyymm"
    pub datum_wiederholt: Option<NaiveDate>,
    pub monat_jahr: Option<String>,
    pub medienrelevant: bool,
    pub start: Option<NaiveDateTime>,
    pub ende: Option<NaiveDateTime>,
    pub vergangen: bool,
    pub tag_mit_datum: Option<String>, // textual day + date
    pub fuehrungsformat: Option<String>,
    pub technik1: Option<i64>,
    pub technik2: Option<String>,
    pub technik3: Vec<String>,
}

impl From<Value> for ParliamentDate {
    fn from(value: Value) -> Self {
        let row = value;
        let parse_date = |s: &str| NaiveDate::parse_from_str(s, "%d.%m.%Y").ok();
        let parse_time = |s: &str| NaiveTime::parse_from_str(s, "%H:%M").ok();
        let parse_datetime = |s: &str| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok();

        let datum = row.get(0).and_then(|v| v.as_str()).and_then(parse_date);
        let datesort = row
            .get(1)
            .and_then(|v| v.as_str())
            .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y%m%d%H%M").ok());
        let uhrzeit = row.get(2).and_then(|v| v.as_str()).and_then(parse_time);
        let bezeichnung = row.get(3).and_then(|v| v.as_str()).map(String::from);
        let link = row.get(4).and_then(|v| v.as_str()).map(String::from);
        let terminart = row.get(5).and_then(|v| v.as_str()).map(String::from);
        let terminart_anzeige = row.get(6).and_then(|v| v.as_str()).map(String::from);
        let info_link = row.get(7).and_then(|v| v.as_str()).map(String::from);
        let ort = row.get(8).and_then(|v| v.as_str()).map(String::from);

        let themen = row
            .get(9)
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .unwrap_or_default();

        let gremium = row.get(10).and_then(|v| v.as_str()).map(String::from);
        let jahr_monat = row.get(11).and_then(|v| v.as_str()).map(String::from);
        let datum_wiederholt = row.get(12).and_then(|v| v.as_str()).and_then(parse_date);
        let monat_jahr = row.get(13).and_then(|v| v.as_str()).map(String::from);

        let medienrelevant = row
            .get(14)
            .and_then(|v| v.as_str())
            .map(|s| s == "J")
            .unwrap_or(false);
        let start = row
            .get(15)
            .and_then(|v| v.as_str())
            .and_then(parse_datetime);
        let ende = row
            .get(16)
            .and_then(|v| v.as_str())
            .and_then(parse_datetime);
        let vergangen = row
            .get(17)
            .and_then(|v| v.as_str())
            .map(|s| s == "J")
            .unwrap_or(false);

        let tag_mit_datum = row.get(18).and_then(|v| v.as_str()).map(String::from);
        let fuehrungsformat = row.get(19).and_then(|v| v.as_str()).map(String::from);

        let technik1 = row
            .get(20)
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok());
        let technik2 = row.get(21).and_then(|v| v.as_str()).map(String::from);
        let technik3 = row
            .get(22)
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .unwrap_or_default();

        ParliamentDate {
            datum,
            datesort,
            uhrzeit,
            bezeichnung,
            link,
            terminart,
            terminart_anzeige,
            info_link,
            ort,
            themen,
            gremium,
            jahr_monat,
            datum_wiederholt,
            monat_jahr,
            medienrelevant,
            start,
            ende,
            vergangen,
            tag_mit_datum,
            fuehrungsformat,
            technik1,
            technik2,
            technik3,
        }
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
    
    async fn post_dates_payload(&self, date_filter: &DateFilter) -> Result<Value, ScraperError> {
        self.post_payload_filter(self.dates_payload(date_filter), Self::FILTER_PATH)
            .await
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
                .map(|row| ParliamentDate::from(row))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDateTime, Utc};

    use crate::{dates::{DateFilter, DateRange, Dates, Terminart}, AustriaScraper};

    #[tokio::test]
    async fn test_dates() {
        let scraper = AustriaScraper::default();
        let res = scraper.post_dates_payload(&DateFilter::default()).await.unwrap();
        let dates = scraper.raw_dates_into_parliament_dates(res).await.unwrap();
        dbg!(dates);
    }
    
    #[tokio::test]
    async fn test_dates_filtering() {
        let now: NaiveDateTime = Utc::now().naive_utc();
        // today();
        let scraper = AustriaScraper::default();
        let res = scraper.post_dates_payload(&DateFilter {
            daterange: Some(DateRange { from: Some(now), to: None }),
            terminart: vec![Terminart::Plenarsitzung],
            ..Default::default()
        }).await.unwrap();
        let dates = scraper.raw_dates_into_parliament_dates(res).await.unwrap();
        dbg!(dates);
    }
}
