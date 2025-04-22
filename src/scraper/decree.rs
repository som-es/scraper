use std::str::FromStr;

use chrono::NaiveDate;
use common_scrapes::{Document, FileType};
use reqwest::Method;
use serde_json::Value;

use crate::{error::ScraperError, request};

use super::{AustriaScraper, Scraper};

pub struct Thing {
    pub id: String,
    pub issuer: String,
    pub organization: String,
    pub title: String,
    pub short_title: String,
    pub date: NaiveDate,
    pub thing_type: String,
    pub part: String,
    pub documents: Vec<Document>,
}

pub trait RisScraper: Scraper {
    const STATE_LAW_URL: &'static str = "https://data.bka.gv.at/ris/api/v2.6/Bundesrecht";
    const DECREE_QUERY_PARAMS: &'static str = "?Applikation=BgblAuth&Typ.SucheInVerordnungen=true";

    async fn fetch_decrees(&self, page: u32) -> Result<Value, ScraperError> {
        let url = format!("{}{}&Seitennummer={}", Self::STATE_LAW_URL, Self::DECREE_QUERY_PARAMS, page);
        request::request(Method::GET, self.client(), (), &url).await
    }

    async fn fetch_and_parse_ministrial_decrees(&self, page: u32) -> Result<Vec<Thing>, ScraperError> {
        let decree = self.fetch_decrees(page).await?;
        let decrees = decree["OgdSearchResult"]["OgdDocumentResults"]["OgdDocumentReference"]
            .as_array()
            .ok_or(ScraperError::ParseError)?
            .iter()
            .filter_map(|decree| {
                self.parse_ministrial_decree(&decree)})
            .collect::<Vec<Thing>>();

        Ok(decrees)
    }

    fn parse_ministrial_decree(&self, decree_raw: &Value) -> Option<Thing> {
        let mut thing = self.general_parse_thing(decree_raw)?;
        if !thing.issuer.to_lowercase().contains("minister") {
            return None;
        }
        thing.issuer = thing.issuer[thing.issuer.find('(')?+1..thing.issuer.find(')')?].to_string();
        Some(thing)
    }

    fn general_parse_thing(&self, thing_raw: &Value) -> Option<Thing> {
        let data = &thing_raw["Data"]["Metadaten"];
        let technical_data = &data["Technisch"];
        let id = technical_data["ID"].as_str().unwrap();
        let issuer = technical_data["Einbringer"].as_str().unwrap();
        let organization = technical_data["Organ"].as_str().unwrap();

        let general_info = &data["Bundesrecht"];
        let title = general_info["Titel"].as_str().unwrap();
        let short_title = general_info["Kurztitel"].as_str().unwrap();

        let general_bgbl_info = &general_info["BgblAuth"];

        let part = general_bgbl_info["Teil"].as_str().unwrap();
        let date = general_bgbl_info["Ausgabedatum"].as_str().unwrap();
        let thing_type = general_bgbl_info["Typ"].as_str().unwrap();

        let documents = thing_raw["Data"]["Dokumentliste"]["ContentReference"]["Urls"]["ContentUrl"]
            .as_array()?
            .iter()
            .filter_map(|doc| {
                let doc_type = doc["DataType"].as_str()?;
                let url = doc["Url"].as_str()?;
                let ftype = match doc_type {
                    "Html" => FileType::HTML,
                    "Authentisch" => FileType::OriginalPDF,
                    _ => FileType::Unknown,
                };
                Some(Document {
                    title: doc_type.to_string(),
                    link: url.to_string(),
                    ftype,
                })
            })
            .collect::<Vec<Document>>();

        Some(Thing {
            id: id.to_string(),
            organization: organization.to_string(),
            issuer: issuer.to_string(),
            title: title.to_string(),
            short_title: short_title.to_string(),
            date: NaiveDate::from_str(date).unwrap(),
            thing_type: thing_type.to_string(),
            part: part.to_string(),
            documents,
        })
    }
}

impl RisScraper for AustriaScraper {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scraper::AustriaScraper;

    #[tokio::test]
    async fn test_fetch_decrees() {
        let scraper = AustriaScraper::default();
        let result = scraper.fetch_decrees(1).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fetch_and_parse_ministrial_decrees() {
        let scraper = AustriaScraper::default();
        let decrees = scraper.fetch_and_parse_ministrial_decrees(1).await.unwrap();
        assert!(!decrees.is_empty());
    }
}