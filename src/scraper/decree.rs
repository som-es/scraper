use std::{future::Future, str::FromStr};

use chrono::NaiveDate;
use common_scrapes::{Document, FileType, RisAmendment, RisDecree, Thing};
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
            let decrees = decree["OgdSearchResult"]["OgdDocumentResults"]["OgdDocumentReference"]
                .as_array()
                .ok_or(ScraperError::ParseError)?
                .iter()
                .filter_map(|decree| self.parse_ministrial_decree(decree))
                .collect::<Vec<Thing>>();

            Ok(decrees)
        }
    }

    /// Fetch a single RIS act (Gesetz/Verordnung) by its document number, e.g.
    /// `BGBLA_2021_II_363`, together with the acts that have amended it. The
    /// document number is the one referenced by `bgbllinks` on a legislative
    /// initiative (see [`crate::legis_init::LegislativeInitiative::ris_link`]).
    fn fetch_single_decree(
        &self,
        doc_number: &str,
    ) -> impl Future<Output = Result<RisDecree, ScraperError>> {
        async move {
            let (year, _part, number) =
                parse_doc_number(doc_number).ok_or(ScraperError::ParseError)?;
            let suchworte = format!("{}/{}", number, year).replace('/', "%2F");
            let url = format!("{}?Suchworte={}", Self::STATE_LAW_URL, suchworte);
            let data = request::request(Method::GET, self.client(), (), url).await?;
            parse_ris_decree(&data, doc_number).ok_or(ScraperError::ParseError)
        }
    }

    fn parse_ministrial_decree(&self, decree_raw: &Value) -> Option<Thing> {
        let mut thing = self.general_parse_thing(decree_raw)?;
        if !thing.issuer.to_lowercase().contains("minister") {
            return None;
        }
        thing.issuer =
            thing.issuer[thing.issuer.find('(')? + 1..thing.issuer.find(')')?].to_string();
        Some(thing)
    }

    fn general_parse_thing(&self, thing_raw: &Value) -> Option<Thing> {
        let data = &thing_raw["Data"]["Metadaten"];
        // println!("data: {}", serde_json::to_string_pretty(data).unwrap());
        let document_url = data["Allgemein"]["DokumentUrl"].as_str()?.to_string();
        let technical_data = &data["Technisch"];
        let id = technical_data["ID"].as_str().unwrap();
        let issuer = technical_data["Einbringer"].as_str().unwrap();
        let organization = technical_data["Organ"].as_str().unwrap();

        let general_info = &data["Bundesrecht"];
        let eli = general_info["Eli"].as_str().unwrap().to_string();
        let title = general_info["Titel"].as_str().unwrap();
        let short_title = general_info["Kurztitel"].as_str().unwrap();

        let general_bgbl_info = &general_info["BgblAuth"];

        let part = general_bgbl_info["Teil"].as_str().unwrap();
        let date = general_bgbl_info["Ausgabedatum"].as_str().unwrap();
        let thing_type = general_bgbl_info["Typ"].as_str().unwrap();

        let documents = thing_raw["Data"]["Dokumentliste"]["ContentReference"]["Urls"]
            ["ContentUrl"]
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
            eli,
            document_url,
            documents,
        })
    }
}

impl RisScraper for AustriaScraper {}

/// Split a RIS document number like `BGBLA_2021_II_363` into `(year, part, number)`.
fn parse_doc_number(doc_number: &str) -> Option<(String, String, String)> {
    let mut parts = doc_number.split('_');
    let _prefix = parts.next()?;
    let year = parts.next()?.to_string();
    let part = parts.next()?.to_string();
    let number = parts.next()?.to_string();
    Some((year, part, number))
}

/// Parse a single decree and the acts that amended it out of a `Bundesrecht`
/// search response, keyed by the decree's document number. The search is issued
/// for `"<number>/<year>"`, which returns the decree itself plus every act that
/// references it; the amendments are those whose short title carries the main
/// decree's short title (e.g. "Änderung der Verordnung über ... Klimatickets").
fn parse_ris_decree(data: &Value, doc_number: &str) -> Option<RisDecree> {
    let (year, part, number) = parse_doc_number(doc_number)?;
    let main_bgbl = format!("BGBl. {} Nr. {}/{}", part, number, year);
    // With a single hit the API returns one object; with several, an array.
    let raw_refs = &data["OgdSearchResult"]["OgdDocumentResults"]["OgdDocumentReference"];
    let refs: Vec<&Value> = match raw_refs {
        Value::Array(items) => items.iter().collect(),
        item if !item.is_null() => vec![item],
        _ => Vec::new(),
    };

    let main = refs.iter().find(|r| {
        r["Data"]["Metadaten"]["Technisch"]["Applikation"].as_str() == Some("BgblAuth")
            && r["Data"]["Metadaten"]["Bundesrecht"]["BgblAuth"]["Bgblnummer"].as_str()
                == Some(main_bgbl.as_str())
    })?;

    let main_meta = &main["Data"]["Metadaten"]["Bundesrecht"];
    let main_keys = title_match_keys(main_meta["Kurztitel"].as_str().unwrap_or_default());

    let amendments = refs
        .iter()
        .filter(|r| {
            r["Data"]["Metadaten"]["Technisch"]["Applikation"].as_str() == Some("BgblAuth")
                && r["Data"]["Metadaten"]["Technisch"]["ID"].as_str() != Some(doc_number)
        })
        .filter_map(|r| {
            let meta = &r["Data"]["Metadaten"]["Bundesrecht"];
            let short = meta["Kurztitel"].as_str().unwrap_or_default();
            let short_lc = short.to_lowercase();
            let is_amendment = !main_keys.is_empty() && main_keys.iter().any(|k| short_lc.contains(k));
            is_amendment.then(|| RisAmendment {
                doc_number: r["Data"]["Metadaten"]["Technisch"]["ID"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                bgbl: meta["BgblAuth"]["Bgblnummer"].as_str().unwrap_or_default().to_string(),
                short_title: short.to_string(),
                eli: meta["Eli"].as_str().unwrap_or_default().to_string(),
            })
        })
        .collect::<Vec<_>>();
    let mut amendments = amendments;
    amendments.sort_by_key(|a| doc_number_sort_key(&a.doc_number));

    Some(RisDecree {
        doc_number: doc_number.to_string(),
        short_title: main_meta["Kurztitel"].as_str().unwrap_or_default().to_string(),
        title: main_meta["Titel"].as_str().unwrap_or_default().to_string(),
        typ: main_meta["BgblAuth"]["Typ"].as_str().unwrap_or_default().to_string(),
        bgbl: main_bgbl,
        publication_date: main_meta["BgblAuth"]["Ausgabedatum"]
            .as_str()
            .and_then(|d| NaiveDate::from_str(d).ok()),
        eli: main_meta["Eli"].as_str().unwrap_or_default().to_string(),
        documents: parse_content_documents(&main["Data"]),
        amendments,
    })
}

/// Chronological sort key for a document number: `(year, part, number)`.
fn doc_number_sort_key(doc_number: &str) -> (u32, u32, u32) {
    let parts: Vec<&str> = doc_number.split('_').collect();
    let year = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
    let part = parts
        .get(2)
        .map(|p| p.chars().filter(|c| c.is_ascii_alphabetic()).count() as u32)
        .unwrap_or(0);
    let number = parts.last().and_then(|p| p.parse().ok()).unwrap_or(0);
    (year, part, number)
}

/// Phrases to look for in an amending act's short title: the full short title
/// plus each comma-separated alias long enough to be distinctive. E.g.
/// "NetzdienstleistungsVO Strom 2012, END-VO 2012" yields both parts, so that
/// amendments phrased as "... – Novelle YYYY" or "END-VO 2012 – ..." match.
fn title_match_keys(short_title: &str) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let full = short_title.trim().to_lowercase();
    if !full.is_empty() {
        keys.push(full);
    }
    for part in short_title.split(',') {
        let p = part.trim().to_lowercase();
        let specific = p.chars().count() >= 10 && p.split_whitespace().count() >= 2;
        if specific && !keys.contains(&p) {
            keys.push(p);
        }
    }
    keys
}

/// The downloadable representations (XML/HTML/RTF/PDF) of a document.
fn parse_content_documents(data: &Value) -> Vec<Document> {
    data["Dokumentliste"]["ContentReference"]["Urls"]["ContentUrl"]
        .as_array()
        .map(|urls| {
            urls.iter()
                .filter_map(|doc| {
                    let doc_type = doc["DataType"].as_str()?;
                    let url = doc["Url"].as_str()?;
                    let ftype = match doc_type {
                        "Html" => FileType::HTML,
                        "Pdf" | "Authentisch" => FileType::OriginalPDF,
                        _ => FileType::Unknown,
                    };
                    Some(Document {
                        title: doc_type.to_string(),
                        link: url.to_string(),
                        ftype,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

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
        let decrees = scraper
            .fetch_and_parse_ministrial_decrees(30)
            .await
            .unwrap();
        assert!(!decrees.is_empty());
        println!("decrees count: {}", decrees.len());
        dbg!(&decrees[..10]);
    }

    fn fixture(doc_number: &str) -> Value {
        let path = format!("test-data/ris/{doc_number}.json");
        let raw = std::fs::read_to_string(path).expect("fixture file");
        serde_json::from_str(&raw).expect("valid json")
    }

    #[test]
    fn test_parse_ris_decree_klimaticket() {
        let data = fixture("BGBLA_2021_II_363");
        let decree = parse_ris_decree(&data, "BGBLA_2021_II_363").unwrap();

        assert_eq!(decree.doc_number, "BGBLA_2021_II_363");
        assert_eq!(decree.short_title, "Einführung des Klimatickets");
        assert_eq!(decree.typ, "Verordnung");
        assert_eq!(decree.bgbl, "BGBl. II Nr. 363/2021");
        assert_eq!(
            decree.publication_date,
            NaiveDate::from_ymd_opt(2021, 8, 18)
        );

        let amendments: Vec<&str> =
            decree.amendments.iter().map(|a| a.doc_number.as_str()).collect();
        assert_eq!(
            amendments,
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

        let first = &decree.amendments[0];
        assert_eq!(first.bgbl, "BGBl. II Nr. 122/2022");
        assert!(first.short_title.contains("Klimaticket"));
        assert!(first.eli.contains("/eli/bgbl/II/2022/122"));
    }

    #[test]
    fn test_parse_ris_decree_aliased_short_title() {
        // The main decree carries two names ("... , END-VO 2012"), and its
        // amendments reference either of them, so both must match.
        let data = fixture("BGBLA_2012_II_477");
        let decree = parse_ris_decree(&data, "BGBLA_2012_II_477").unwrap();

        assert_eq!(
            decree.short_title,
            "NetzdienstleistungsVO Strom 2012, END-VO 2012"
        );
        let amendments: Vec<&str> =
            decree.amendments.iter().map(|a| a.doc_number.as_str()).collect();
        assert_eq!(
            amendments,
            vec!["BGBLA_2013_II_192", "BGBLA_2023_II_394", "BGBLA_2026_II_177"]
        );
    }

    #[test]
    fn test_parse_ris_decree_no_amendments() {
        // A single hit is returned as one object (not an array) by the API.
        let data = fixture("BGBLA_2026_II_282");
        let decree = parse_ris_decree(&data, "BGBLA_2026_II_282").unwrap();
        assert_eq!(decree.doc_number, "BGBLA_2026_II_282");
        assert_eq!(decree.bgbl, "BGBl. II Nr. 282/2026");
        assert!(decree.amendments.is_empty());
    }

    #[test]
    fn test_parse_ris_decree_unknown_number() {
        let data = fixture("BGBLA_2021_II_363");
        assert!(parse_ris_decree(&data, "BGBLA_2021_II_999").is_none());
    }

    #[test]
    fn test_parse_doc_number() {
        assert_eq!(
            parse_doc_number("BGBLA_2021_II_363"),
            Some(("2021".into(), "II".into(), "363".into()))
        );
        assert_eq!(
            parse_doc_number("BGBLA_2024_I_114"),
            Some(("2024".into(), "I".into(), "114".into()))
        );
        assert!(parse_doc_number("not-a-doc-number").is_none());
    }

    #[tokio::test]
    async fn test_fetch_single_decree() {
        let scraper = AustriaScraper::default();
        let decree = scraper
            .fetch_single_decree("BGBLA_2021_II_363")
            .await
            .unwrap();
        assert_eq!(decree.doc_number, "BGBLA_2021_II_363");
        assert!(!decree.amendments.is_empty());
    }
}
