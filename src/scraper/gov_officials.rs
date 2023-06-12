use async_trait::async_trait;
use common_scrapes::{Delegate, GovOfficial};
use serde_json::{json, Value};

use crate::{error::ScraperError, extract_text_from_html, AustriaScraper, Scraper};

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

// /WWER/PAD_18140/index.shtml => 18140
pub fn extract_id_from_unnecessary_link(link_containing_id: &str) -> Option<u64> {
    let start_idx = link_containing_id.find('_')? + 1;

    let end_idx = link_containing_id.rfind('/')?;

    link_containing_id
        .get(start_idx..end_idx)?
        .parse::<u64>()
        .ok()
}

pub fn extract_gov_official(raw_official: &Value) -> Option<GovOfficial> {
    let raw_official = raw_official.as_array()?;
    let function_and_dates = extract_function_and_dates(raw_official[2].as_str()?)?;

    let name = raw_official[1].as_str()?.to_string();
    let id = extract_id_from_unnecessary_link(raw_official[9].as_str()?)?;

    let joined =
        chrono::NaiveDate::parse_from_str(function_and_dates.start_date, "%d.%m.%Y").ok()?;

    let left = function_and_dates
        .end_date
        .and_then(|end_date| chrono::NaiveDate::parse_from_str(end_date, "%d.%m.%Y").ok());

    Some(GovOfficial {
        government: extract_gov_from_html(raw_official[0].as_str()?),
        function: function_and_dates.function.to_string(),
        ministry: raw_official[7].as_str()?.to_string(),
        joined,
        left,
        delegate: Delegate {
            id,
            name,
            is_active: left.is_none(),
            image_url: None, // from delegate call
            party: None,
            constituency: None,
            council: None,
            seat: None,
            mandates: todo!(),
            contact: todo!(), // from delegate call
            gender: todo!(),
        },
    })
}

#[async_trait]
impl GovOfficials for AustriaScraper {
    const FILTER_PATH: &'static str =
        "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFW_018&listeId=undefined&showAll=true";

    async fn gov_officials(&self) -> Option<Vec<GovOfficial>> {
        Some(
            self.post_gov_officials_payload()
                .await
                .ok()?
                .get("rows")?
                .as_array()?
                .iter()
                .flat_map(extract_gov_official)
                .collect(),
        )
    }

    #[inline]
    fn gov_officials_payload(&self) -> Self::Payload {
        json!({
          "REG": [
            "0"
          ],
          "R_ZEIT": [
            "AKT"
          ]
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct FunctionAndDates<'a> {
    function: &'a str,
    start_date: &'a str,
    end_date: Option<&'a str>,
}

fn extract_function_and_dates(function_text: &str) -> Option<FunctionAndDates> {
    let mut function_text = function_text.split("<br />");
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

    #[test]
    fn test_extract_single_gov_official_from_json() {
        let raw_gov = json!([
            r#"<span title="06.12.2021 - " class="zeigeTooltip">Nehammer</span>"#,
            "Mag. Werner Kogler",
            "Bundesminister<br />06.12.2021 - ",
            "134",
            "007",
            "Kogler",
            "D",
            "Bundesministerium für Kunst, Kultur, öffentlichen Dienst und Sport",
            "10.05.2023",
            "/WWER/PAD_08242/index.shtml"
        ]);
        let gov_official = super::extract_gov_official(&raw_gov).unwrap();
        assert_eq!(gov_official.government, "Nehammer");
        assert_eq!(gov_official.function, "Bundesminister");
        assert_eq!(
            gov_official.ministry,
            "Bundesministerium für Kunst, Kultur, öffentlichen Dienst und Sport"
        );
        assert_eq!(
            gov_official.joined,
            chrono::NaiveDate::from_ymd_opt(2021, 12, 6).unwrap()
        );
        assert_eq!(gov_official.left, None);
        assert_eq!(gov_official.delegate.id, 8242);
        assert_eq!(gov_official.delegate.name, "Mag. Werner Kogler");
        assert!(gov_official.delegate.is_active);
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
