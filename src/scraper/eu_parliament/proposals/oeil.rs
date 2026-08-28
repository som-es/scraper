use quick_xml::events::Event;
use quick_xml::Reader;

use crate::error::ScraperError;
use crate::eu_parliament::EuParliamentScraper;
use crate::request::request_text;

const OEIL_EXPORT_URL: &str = "https://oeil.secure.europarl.europa.eu/oeil/en/search/export/XML";
const OEIL_PAGE_SIZE: usize = 500;

impl EuParliamentScraper {
    pub async fn list_oeil_procedure_ids(&self, year: u32) -> Result<Vec<String>, ScraperError> {
        let mut process_ids = Vec::new();
        let mut page_index = 0usize;
        loop {
            let url = format!("{OEIL_EXPORT_URL}?year={year}&pageIndex={page_index}");
            let page = parse_oeil_page(&request_text(self.client(), &url).await?);
            process_ids.extend(page.process_ids);
            page_index += 1;
            if page_index * OEIL_PAGE_SIZE >= page.total {
                break;
            }
        }
        Ok(process_ids)
    }
}

struct OeilPage {
    total: usize,
    process_ids: Vec<String>,
}

pub(crate) fn reference_to_process_id(reference: &str) -> Option<String> {
    let (identifier, _) = reference.split_once('(')?;
    let (year, number) = identifier.split_once('/')?;
    let year_ok = year.len() == 4 && year.bytes().all(|b| b.is_ascii_digit());
    let number_ok = number.len() >= 4
        && number[..4].bytes().all(|b| b.is_ascii_digit())
        && number[4..].bytes().all(|b| b.is_ascii_uppercase());
    (year_ok && number_ok).then(|| format!("{year}-{number}"))
}

fn parse_oeil_page(xml: &str) -> OeilPage {
    let mut reader = Reader::from_str(xml);
    let mut total = 0;
    let mut process_ids = Vec::new();
    let mut in_reference = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"items" => {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"count" {
                            if let Ok(value) = attr.unescape_value() {
                                total = value.trim().parse().unwrap_or(0);
                            }
                        }
                    }
                }
                b"reference" => in_reference = true,
                _ => {}
            },
            Ok(Event::End(e)) if e.name().as_ref() == b"reference" => in_reference = false,
            Ok(Event::Text(t)) if in_reference => {
                if let Ok(text) = t.unescape() {
                    if let Some(id) = reference_to_process_id(text.trim()) {
                        process_ids.push(id);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    OeilPage { total, process_ids }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<procedureList>
  <items count="3">
    <item><reference>2019/0017(COD)</reference><title>A</title></item>
    <item><reference>2019/0001B(COD)</reference><title>B</title></item>
    <item><reference>COM(2025)0498</reference><title>Commission doc</title></item>
  </items>
</procedureList>"#;

    #[test]
    fn parses_and_filters_references() {
        let page = parse_oeil_page(SAMPLE);
        assert_eq!(page.total, 3);
        assert_eq!(page.process_ids, vec!["2019-0017", "2019-0001B"]);
    }

    #[test]
    fn reference_conversion() {
        assert_eq!(
            reference_to_process_id("2019/0017(COD)").as_deref(),
            Some("2019-0017")
        );
        assert_eq!(
            reference_to_process_id("2019/0142M(NLE)").as_deref(),
            Some("2019-0142M")
        );
        assert_eq!(reference_to_process_id("COM(2025)0498"), None);
        assert_eq!(reference_to_process_id("2019/17(COD)"), None);
    }
}
