use async_trait::async_trait;
use common_scrapes::GovOfficial;
use serde_json::{json, Value};

use crate::{
    caching::{disk_caching, get_disk_cache_json}, delegates::DelegateScraper, error::ScraperError, extract_text_from_html, AustriaScraper, Scraper
};

#[async_trait]
pub trait GovOfficials: Scraper {
    const FILTER_PATH: &'static str;

    async fn post_gov_officials_payload(&self) -> Result<Value, ScraperError> {
        self.post_payload_filter(self.gov_officials_payload(), Self::FILTER_PATH)
            .await
    }

    fn gov_officials_payload(&self) -> Self::Payload;

    // function that takes raw json (like the other scrapers)
    // mind asyncness
    async fn gov_officials(&self) -> Option<Vec<GovOfficial>>;
}

pub fn extract_gov_from_html(gov_html: &str) -> String {
    extract_text_from_html(gov_html)
}

// /WWER/PAD_18140/index.shtml => 18140 (could use regex)
pub fn extract_id_from_unnecessary_link(link_containing_id: &str) -> Option<u64> {
    let start_idx = link_containing_id.find('_')? + 1;

    let end_idx = link_containing_id.rfind('/')?;

    link_containing_id
        .get(start_idx..end_idx)?
        .parse::<u64>()
        .ok()
}

pub async fn extract_gov_official(
    raw_official: &Value,
    delegate_scraper: &(impl DelegateScraper + Sync),
) -> Option<GovOfficial> {
    // println!("{}", serde_json::to_string_pretty(raw_official).unwrap());
    let raw_official = raw_official.as_array()?;
    let function_and_dates = extract_function_and_dates(raw_official[3].as_str()?)?;

    let id = raw_official[0].as_u64()?;
    // let id = extract_id_from_unnecessary_link(raw_official[9].as_str()?)?;

    let start_date =
        chrono::NaiveDate::parse_from_str(function_and_dates.start_date, "%d.%m.%Y").ok()?;

    let end_date = function_and_dates
        .end_date
        .and_then(|end_date| chrono::NaiveDate::parse_from_str(end_date, "%d.%m.%Y").ok());

    // let delegate = delegate_scraper
    //     .get_delegate(format!("/person/{id}").as_str().into())
    //     .await
    //     .ok()?;
    Some(GovOfficial {
        government: raw_official[1].as_str()?.to_string(),
        function: function_and_dates.function.to_string(),
        ministry: raw_official[5].as_str()?.to_string(),
        start_date,
        end_date,
        delegate_id: id,
    })
}

#[async_trait]
impl GovOfficials for AustriaScraper {
    const FILTER_PATH: &'static str =
        // "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFW_018&listeId=undefined&showAll=true";
        "/Filter/api/filter/data/1718?1=1&showAll=true";

    async fn gov_officials(&self) -> Option<Vec<GovOfficial>> {
        let path = "officials.json";
        let gov_officials_raw = match get_disk_cache_json(path).await {
            Some(raw) => raw,
            None => {
                let raw_proposal = self.post_gov_officials_payload()
                    .await
                    .ok()?;

                if let Err(e) = disk_caching(path, raw_proposal.to_string()).await {
                    log::warn!("could cache raw {path}: {e:?}")
                }
                raw_proposal
            },
        };
        
        Some(
            futures::future::join_all(
                    gov_officials_raw 
                    .get("rows")?
                    .as_array()?
                    .iter()
                    .map(|raw| extract_gov_official(raw, self))
                    .collect::<Vec<_>>(),
            )
            .await
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
        )
    }

    #[inline]
    fn gov_officials_payload(&self) -> Self::Payload {
        json!({
          "REG": [
            "0"
          ],
          
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct FunctionAndDates<'a> {
    pub function: &'a str,
    pub start_date: &'a str,
    pub end_date: Option<&'a str>,
}

fn extract_function_and_dates(function_text: &str) -> Option<FunctionAndDates> {
    let break_str = if function_text.contains("<br>") {
        "<br>"
    } else if function_text.contains("<br/>") {
        "<br/>"
    } else {
        "<br />"
    };
    let mut function_text = function_text.split(break_str);
    let function = function_text.next()?;
    let dates = function_text.next()?;

    let mut dates = dates.split(" - ").filter(|date| !date.is_empty());

    let start_date = dates.next()?;
    let end_date = dates.next();

    Some(FunctionAndDates {
        function,
        start_date,
        end_date,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{extract_function_and_dates, GovOfficials};
    use crate::{
        gov_officials::{extract_id_from_unnecessary_link, FunctionAndDates},
        AustriaScraper,
    };

    #[tokio::test]
    async fn test_extract_all_govs_from_austria_api() {
        let scraper = AustriaScraper::default();
        let gov_officials = scraper.gov_officials().await;
        println!("gov_officials: {gov_officials:?}");
    }

    #[tokio::test]
    async fn test_extract_single_gov_official_from_json() {
        let raw_gov = json!(
            [65321,"Kurz II","Sebastian Kurz","Bundeskanzler<br/>07.01.2020 - 11.10.2021","Bundeskanzler:in","Bundeskanzleramt","Kurz","Männer","Tue Jan 07 00:00:00 CET 2020","/person/65321",1]
        );
        let scraper = AustriaScraper::default();
        let gov_official = super::extract_gov_official(&raw_gov, &scraper)
            .await
            .unwrap();
        assert_eq!(gov_official.government, "Kurz II");
        assert_eq!(gov_official.function, "Bundeskanzler");
        assert_eq!(
            gov_official.ministry,
            "Bundeskanzleramt"
        );
        assert_eq!(
            gov_official.start_date,
            chrono::NaiveDate::from_ymd_opt(2020, 1, 7).unwrap()
        );
        // assert_eq!(gov_official.end_date, None);
        assert_eq!(gov_official.delegate_id, 65321);
        // assert_eq!(gov_official.delegate.name, "Kurz Sebastian");
        // assert!(gov_official.delegate.is_active);
    }

    #[test]
    fn test_string_to_num_parsing() {
        let num = "0123".parse::<u64>().unwrap();
        assert_eq!(num, 123);
    }

    #[test]
    fn test_unnecessary_id_link_extraction() {
        let link_containing_id = "/WWER/PAD_18140/index.shtml";
        let id = extract_id_from_unnecessary_link(link_containing_id).unwrap();
        assert_eq!(id, 18140)
    }

    #[test]
    fn test_unnecessary_id_link_extraction_other_id() {
        let link_containing_id = "/WWER/PAD_08242/index.shtml";
        let id = extract_id_from_unnecessary_link(link_containing_id).unwrap();
        assert_eq!(id, 8242)
    }

    #[test]
    fn test_extract_gov_from_html() {
        let gov_html = r#"<span title="06.12.2021 - " class="zeigeTooltip">Nehammer</span>"#;
        let gov = super::extract_gov_from_html(gov_html);
        assert_eq!(gov, "Nehammer");
    }

    #[test]
    fn test_function_extraction_without_end() {
        let function_text = "Bundesministerin für EU und Verfassung<br />06.12.2021 - ";
        let function_and_dates = extract_function_and_dates(function_text);
        assert_eq!(
            function_and_dates,
            Some(FunctionAndDates {
                function: "Bundesministerin für EU und Verfassung",
                start_date: "06.12.2021",
                end_date: None
            })
        );
    }

    #[test]
    fn test_function_extraction_with_end() {
        let function_text = "Bundesministerin für EU und Verfassung<br />06.12.2021 - 01.01.2022";
        let function_and_dates = extract_function_and_dates(function_text);
        assert_eq!(
            function_and_dates,
            Some(FunctionAndDates {
                function: "Bundesministerin für EU und Verfassung",
                start_date: "06.12.2021",
                end_date: Some("01.01.2022")
            })
        );
    }
}
