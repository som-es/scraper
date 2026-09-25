use std::{future::Future, str::FromStr};

use chrono::NaiveDate;
use common_scrapes::{Document, FileType, RisAmendmentRef, Thing};
use regex::Regex;
use reqwest::Method;
use serde_json::Value;

use crate::{error::ScraperError, request};

use super::{AustriaScraper, Scraper};

pub trait RisScraper: Scraper {
    const STATE_LAW_URL: &'static str = "https://data.bka.gv.at/ris/api/v2.6/Bundesrecht";
    const DECREE_QUERY_PARAMS: &'static str = "?Applikation=BgblAuth&Typ.SucheInVerordnungen=true";

    fn fetch_decrees(&self, page: u32) -> impl Future<Output = Result<Value, ScraperError>> {
        let url = format!(
            "{}{}&Seitennummer={}&DokumenteProSeite=OneHundred",
            Self::STATE_LAW_URL,
            Self::DECREE_QUERY_PARAMS,
            page
        );
        request::request(Method::GET, self.client(), (), url)
    }

    fn fetch_and_parse_ministrial_decrees(
        &self,
        page: u32,
    ) -> impl Future<Output = Result<Vec<Thing>, ScraperError>> {
        async move {
            let decree = self.fetch_decrees(page).await?;
            let decrees = search_refs(&decree)
                .into_iter()
                .filter_map(|decree| self.parse_ministrial_decree(decree))
                .collect::<Vec<Thing>>();

            Ok(decrees)
        }
    }

    fn fetch_consolidated_decrees(
        &self,
        page: u32,
    ) -> impl Future<Output = Result<Vec<Thing>, ScraperError>> {
        async move {
            let url = format!(
                "{}?Applikation=BrKons&Seitennummer={}&DokumenteProSeite=OneHundred&Typ=V&Abschnitt.Von=0&Abschnitt.Bis=0&Abschnitt.Typ=Paragraph",
                Self::STATE_LAW_URL,
                page
            );
            let data = request::request(Method::GET, self.client(), (), url).await?;
            Ok(search_refs(&data)
                .into_iter()
                .filter_map(|r| self.parse_consolidated_norm_entry(r))
                .collect())
        }
    }

    fn fetch_single_ris_entry(
        &self,
        dokumentnummer: &str,
    ) -> impl Future<Output = Result<Thing, ScraperError>> {
        async move {
            let url = format!(
                "{}?Applikation=BgblAuth&Suchworte={}",
                Self::STATE_LAW_URL,
                dokumentnummer
            );
            let data = request::request(Method::GET, self.client(), (), url).await?;
            search_refs(&data)
                .into_iter()
                .find_map(|r| self.parse_ministrial_decree(r))
                .ok_or(ScraperError::ParseError)
        }
    }

    fn parse_ministrial_decree(&self, decree_raw: &Value) -> Option<Thing> {
        let mut thing = self.general_parse_thing(decree_raw, false)?;

        if thing.issuer.to_lowercase().contains("(") {
            thing.issuer =
                thing.issuer[thing.issuer.find('(')? + 1..thing.issuer.find(')')?].to_string();
        }
        Some(thing)
    }

    fn parse_consolidated_norm_entry(&self, raw: &Value) -> Option<Thing> {
        let br_kons = &raw["Data"]["Metadaten"]["Bundesrecht"]["BrKons"];
        if br_kons["Dokumenttyp"].as_str() != Some("Norm") {
            return None;
        }
        let mut thing = self.general_parse_thing(raw, true)?;
        thing.amendments = br_kons["Aenderung"]
            .as_str()
            .map(parse_amendments)
            .unwrap_or_default();

        if thing.issuer.to_lowercase().contains("(") {
            thing.issuer =
                thing.issuer[thing.issuer.find('(')? + 1..thing.issuer.find(')')?].to_string();
        }
        Some(thing)
    }

    fn general_parse_thing(&self, thing_raw: &Value, is_norm: bool) -> Option<Thing> {
        let data = &thing_raw["Data"]["Metadaten"];
        let technical = &data["Technisch"];
        let id = technical["ID"].as_str()?;
        let organization = technical["Organ"].as_str().unwrap_or_default();
        let issuer = technical["Einbringer"].as_str().unwrap_or(organization);
        let document_url = data["Allgemein"]["DokumentUrl"]
            .as_str()
            .unwrap_or_default();

        let general_info = &data["Bundesrecht"];
        let eli = general_info["Eli"].as_str().unwrap_or_default();
        let title = general_info["Titel"].as_str().unwrap_or_default();
        let short_title = general_info["Kurztitel"].as_str().unwrap_or_default();

        let bgbl_auth = &general_info["BgblAuth"];
        let br_kons = &general_info["BrKons"];

        let bgbl = bgbl_auth["Bgblnummer"]
            .as_str()
            .or_else(|| br_kons["Kundmachungsorgan"].as_str())
            .unwrap_or_default();
        let part = bgbl_auth["Teil"].as_str().unwrap_or_default();
        let date = bgbl_auth["Ausgabedatum"]
            .as_str()
            .or_else(|| br_kons["Inkrafttretensdatum"].as_str())
            .unwrap_or_default();
        let thing_type = bgbl_auth["Typ"]
            .as_str()
            .or_else(|| br_kons["Typ"].as_str())
            .unwrap_or_default();

        let documents = parse_documents(thing_raw);

        Some(Thing {
            id: id.to_string(),
            organization: organization.to_string(),
            issuer: issuer.to_string(),
            title: title.to_string(),
            short_title: short_title.to_string(),
            date: NaiveDate::from_str(date).ok()?,
            thing_type: thing_type.to_string(),
            part: part.to_string(),
            bgbl: bgbl.split_whitespace().collect::<Vec<_>>().join(" "),
            eli: eli.to_string(),
            document_url: document_url.to_string(),
            documents,
            is_norm,
            amendments: Vec::new(),
        })
    }
}

impl RisScraper for AustriaScraper {}

fn search_refs(data: &Value) -> Vec<&Value> {
    as_list(&data["OgdSearchResult"]["OgdDocumentResults"]["OgdDocumentReference"])
}

fn as_list(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(items) => items.iter().collect(),
        Value::Null => Vec::new(),
        other => vec![other],
    }
}

fn parse_documents(thing_raw: &Value) -> Vec<Document> {
    as_list(&thing_raw["Data"]["Dokumentliste"]["ContentReference"])
        .into_iter()
        .flat_map(|content_reference| as_list(&content_reference["Urls"]))
        .flat_map(|urls| as_list(&urls["ContentUrl"]))
        .filter_map(|content_url| {
            let doc_type = content_url["DataType"].as_str()?;
            let url = content_url["Url"].as_str()?;
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
        .collect()
}

fn parse_amendments(aenderung: &str) -> Vec<RisAmendmentRef> {
    aenderung
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .map(|bgbl| {
            let doc_number = derive_doc_number(&bgbl).unwrap_or_default();
            RisAmendmentRef { bgbl, doc_number }
        })
        .collect()
}

fn derive_doc_number(bgbl: &str) -> Option<String> {
    let re = Regex::new(r"(?i)BGBl\.\s*(?<teil>I{1,3})\s*Nr\.\s*(?<nr>\d+)\s*/\s*(?<jahr>\d{4})")
        .ok()?;
    let caps = re.captures(bgbl)?;
    let number: u32 = caps["nr"].parse().ok()?;
    let year: u32 = caps["jahr"].parse().ok()?;
    Some(format!("BGBLA_{}_{}_{}", year, &caps["teil"], number))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::sleep;

    use super::*;
    use crate::scraper::AustriaScraper;

    fn fixture(name: &str) -> Value {
        let raw = std::fs::read_to_string(format!("test-data/ris/{name}.json")).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    #[test]
    fn test_derive_doc_number() {
        assert_eq!(
            derive_doc_number("BGBl. II Nr. 122/2022").as_deref(),
            Some("BGBLA_2022_II_122")
        );
        assert_eq!(
            derive_doc_number("BGBl. I Nr. 75/2021").as_deref(),
            Some("BGBLA_2021_I_75")
        );
        assert_eq!(
            derive_doc_number("BGBl. III Nr. 62/2023").as_deref(),
            Some("BGBLA_2023_III_62")
        );
        assert_eq!(
            derive_doc_number("idF: BGBl. II Nr. 119/2025").as_deref(),
            Some("BGBLA_2025_II_119")
        );
        assert_eq!(derive_doc_number("BGBl. Nr. 506/1976"), None);
        assert_eq!(derive_doc_number("dRGBl. I S 42/1944"), None);
        assert_eq!(derive_doc_number("not a bgbl citation"), None);
    }

    #[test]
    fn test_parse_amendments() {
        assert!(parse_amendments("").is_empty());
        assert!(parse_amendments("   \r\n  \r\n").is_empty());

        let two = parse_amendments("BGBl. II Nr. 122/2022\r\nBGBl. II Nr. 323/2022\r\n");
        assert_eq!(two.len(), 2);
        assert_eq!(two[0].bgbl, "BGBl. II Nr. 122/2022");
        assert_eq!(two[0].doc_number, "BGBLA_2022_II_122");
        assert_eq!(two[1].doc_number, "BGBLA_2022_II_323");

        let spaced = parse_amendments("BGBl. Nr.  50/1968\r\nBGBl. II  Nr. 74/2025");
        assert_eq!(spaced[0].bgbl, "BGBl. Nr. 50/1968");
        assert_eq!(spaced[0].doc_number, "");
        assert_eq!(spaced[1].bgbl, "BGBl. II Nr. 74/2025");
        assert_eq!(spaced[1].doc_number, "BGBLA_2025_II_74");

        let unparseable = parse_amendments("dRGBl. I S 42/1944");
        assert_eq!(unparseable.len(), 1);
        assert_eq!(unparseable[0].bgbl, "dRGBl. I S 42/1944");
        assert_eq!(unparseable[0].doc_number, "");
    }

    #[test]
    fn test_search_refs_normalizes_object_and_array() {
        let single: Value = serde_json::json!({"OgdSearchResult":{"OgdDocumentResults":{"OgdDocumentReference":{"Data":{}}}}});
        assert_eq!(search_refs(&single).len(), 1);

        let many: Value = serde_json::json!({"OgdSearchResult":{"OgdDocumentResults":{"OgdDocumentReference":[{"Data":{}},{"Data":{}}]}}});
        assert_eq!(search_refs(&many).len(), 2);

        let none: Value = serde_json::json!({"OgdSearchResult":{"OgdDocumentResults":{}}});
        assert!(search_refs(&none).is_empty());
    }

    #[test]
    fn test_general_parse_thing_authentic_entry() {
        let data = fixture("BGBLA_2021_II_363_auth");
        let scraper = AustriaScraper::default();
        let entry = scraper
            .general_parse_thing(search_refs(&data)[0], false)
            .unwrap();

        assert_eq!(entry.id, "BGBLA_2021_II_363");
        assert_eq!(entry.bgbl, "BGBl. II Nr. 363/2021");
        assert_eq!(entry.part, "Teil2");
        assert_eq!(entry.thing_type, "Verordnung");
        assert_eq!(entry.date, NaiveDate::from_ymd_opt(2021, 8, 18).unwrap());
        assert!(!entry.documents.is_empty());
        assert!(entry.amendments.is_empty());
    }

    #[test]
    fn test_parse_ministrial_decree_extracts_ministry() {
        let data = fixture("BGBLA_2021_II_363_auth");
        let scraper = AustriaScraper::default();
        let decree = scraper
            .parse_ministrial_decree(search_refs(&data)[0])
            .unwrap();

        assert_eq!(
            decree.issuer,
            "Bundesministerium für Klimaschutz, Umwelt, Energie, Mobilität, Innovation und Technologie"
        );
        assert_eq!(decree.id, "BGBLA_2021_II_363");
        assert!(!decree.is_norm);
    }

    #[test]
    fn test_parse_consolidated_entry_amendments() {
        let data = fixture("BGBLA_2021_II_363_p0");
        let scraper = AustriaScraper::default();
        let entry = scraper
            .parse_consolidated_norm_entry(search_refs(&data)[0])
            .unwrap();

        assert_eq!(entry.bgbl, "BGBl. II Nr. 363/2021");
        assert_eq!(entry.short_title, "Einführung des Klimatickets");
        assert_eq!(entry.amendments.len(), 9);
        assert!(entry.is_norm);

        let doc_numbers: Vec<&str> = entry
            .amendments
            .iter()
            .map(|a| a.doc_number.as_str())
            .collect();
        assert_eq!(
            doc_numbers,
            vec![
                "BGBLA_2022_II_122",
                "BGBLA_2022_II_323",
                "BGBLA_2022_II_425",
                "BGBLA_2023_II_136",
                "BGBLA_2023_II_293",
                "BGBLA_2024_II_136",
                "BGBLA_2024_II_310",
                "BGBLA_2025_II_74",
                "BGBLA_2025_II_119",
            ]
        );
    }

    #[test]
    fn test_parse_consolidated_entry_no_amendments() {
        let data = fixture("BGBLA_2026_II_282_p0");
        let scraper = AustriaScraper::default();
        let entry = scraper
            .parse_consolidated_norm_entry(search_refs(&data)[0])
            .unwrap();

        assert_eq!(entry.bgbl, "BGBl. II Nr. 282/2026");
        assert_eq!(entry.thing_type, "V");
        assert!(entry.is_norm);
        assert!(entry.amendments.is_empty());
    }

    #[test]
    fn test_parse_consolidated_entry_skips_paragraph_records() {
        let mut data = fixture("BGBLA_2021_II_363_p0");
        data["OgdSearchResult"]["OgdDocumentResults"]["OgdDocumentReference"]["Data"]
            ["Metadaten"]["Bundesrecht"]["BrKons"]["Dokumenttyp"] =
            Value::String("Paragraph".to_string());

        let scraper = AustriaScraper::default();
        assert!(scraper
            .parse_consolidated_norm_entry(search_refs(&data)[0])
            .is_none());
    }

    #[tokio::test]
    async fn test_fetch_decrees() {
        let scraper = AustriaScraper::default();
        let result = scraper.fetch_decrees(1).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fetch_and_parse_ministrial_decrees() {
        let scraper = AustriaScraper::default();
        let decrees = scraper
            .fetch_and_parse_ministrial_decrees(30)
            .await
            .unwrap();
        assert!(!decrees.is_empty());
        assert!(decrees.iter().all(|d| d.bgbl.starts_with("BGBl")));
    }

    #[tokio::test]
    async fn test_fetch_consolidated_decrees_ignore() {
        let scraper = AustriaScraper::default();
        let decrees = scraper.fetch_consolidated_decrees(8).await.unwrap();
        // assert!(decrees.iter().all(|d| d.bgbl.starts_with("BGBl")));
        dbg!(decrees);
    }

    #[tokio::test]
    async fn test_fetch_consolidated_decrees() {
        let scraper = AustriaScraper::default();
        let decrees = scraper.fetch_consolidated_decrees(1).await.unwrap();
        assert!(decrees.iter().all(|d| d.bgbl.starts_with("BGBl")));
        dbg!(decrees);
    }

    #[tokio::test]
    async fn test_fetch_single_entry() {
        let scraper = AustriaScraper::default();
        let entry = scraper
            .fetch_single_ris_entry("BGBLA_2025_II_74")
            .await
            .unwrap();
        assert_eq!(entry.id, "BGBLA_2025_II_74");
        assert_eq!(entry.bgbl, "BGBl. II Nr. 74/2025");
        dbg!(&entry);
    }

    // BGBLA_2021_II_363
    #[tokio::test]
    async fn test_fetch_single_entry_klimaticket() {
        let scraper = AustriaScraper::default();
        let entry = scraper
            .fetch_single_ris_entry("BGBLA_2021_II_363")
            .await
            .unwrap();
        assert_eq!(entry.id, "BGBLA_2021_II_363");
        dbg!(&entry);
    }

    #[tokio::test]
    async fn test_find_klimaticket_2021() {
        let scraper = AustriaScraper::default();
        let page = 20;
        for page in page..30 {
            let decrees = scraper
                .fetch_and_parse_ministrial_decrees(page)
                .await
                .unwrap();
            let found = decrees
                .iter()
                .find(|decree| decree.id == "BGBLA_2021_II_363");
            if found.is_some() {
                let decrees = scraper.fetch_decrees(page).await.unwrap();
                dbg!(decrees);
                dbg!(found);
            }
            sleep(Duration::from_secs(2)).await;
        }
    }
}
