use async_trait::async_trait;
use scraper::{Html, Selector};
use serde_json::Value;

use crate::{AustriaScraper, Scraper};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct CallToOrderInfo {
    pub description: String,
    pub ityp: String,
    pub inr: u64,
    pub receiver: String,
    pub receiver_id: u64,
    pub links_to: Option<String>,
}

#[async_trait]
pub trait CallToOrder: Scraper {
    async fn get_call_to_order(
        &self,
        call_to_order_path: &str,
    ) -> Result<CallToOrderInfo, Box<dyn std::error::Error>> {
        let raw_call_to_order = self.get_gegenstand_info(call_to_order_path).await?;
        let call_to_order = Self::extract_raw_call_to_order(raw_call_to_order)?;
        Ok(call_to_order)
    }

    fn extract_raw_call_to_order(
        raw_call_to_order: Value,
    ) -> Result<CallToOrderInfo, CallToOrderError>;
}

impl CallToOrder for AustriaScraper {
    fn extract_raw_call_to_order(
        raw_call_to_order: Value,
    ) -> Result<CallToOrderInfo, CallToOrderError> {
        let description = raw_call_to_order["meta"]["description"]
            .as_str()
            .ok_or(CallToOrderError::DescriptionNotAString)?
            .to_string();

        let call_to_order_content = &raw_call_to_order["content"];

        let ityp = call_to_order_content["ityp"]
            .as_str()
            .ok_or(CallToOrderError::ItypNotAString)?
            .to_string();
        let inr = call_to_order_content["inr"]
            .as_u64()
            .ok_or(CallToOrderError::InrNotANumber)?;

        let name = call_to_order_content["names"]
            .as_array()
            .ok_or(CallToOrderError::NamesNotAnArray)?
            .get(0)
            .ok_or(CallToOrderError::NamesArrayIsEmpty)?;
        let receiver = name["name"]
            .as_str()
            .ok_or(CallToOrderError::ReceiverNameNotAString)?
            .to_string();
        let receiver_id = name["url"]
            .as_str()
            .ok_or(CallToOrderError::ReceiverUrlNotAString)?
            .split('/')
            .nth(2)
            .ok_or(CallToOrderError::ReceiverUrlWrongFormat)?
            .to_string();

        let links_to = if let Some(reference) = call_to_order_content["reference"]
            .as_array()
            .ok_or(CallToOrderError::ReferenceNotAnArray)?
            .get(0)
        {
            let zitation = reference["zitation"]
                .as_str()
                .ok_or(CallToOrderError::ZitationNotAString)?;
            let zitation = zitation.split('/').collect::<Vec<&str>>();
            Some(format!(
                "{}/{}",
                zitation
                    .get(1)
                    .ok_or(CallToOrderError::ZitationWrongFormat)?,
                zitation
                    .get(0)
                    .ok_or(CallToOrderError::ZitationWrongFormat)?
            ))
        } else {
            None
        };

        // protection layer against API changes?

        Ok(CallToOrderInfo {
            description,
            ityp,
            inr,
            receiver,
            receiver_id: receiver_id.parse::<u64>().unwrap(),
            links_to,
        })
    }
}

use thiserror::Error;

#[derive(Error, Clone, Debug, PartialEq, Eq)]
pub enum CallToOrderError {
    #[error("The received 'description' from the remote API is not a string.")]
    DescriptionNotAString,
    #[error("The received 'ityp' from the remote API is not a string.")]
    ItypNotAString,
    #[error("The received 'inr' from the remote API is not a number/u64.")]
    InrNotANumber,
    #[error("The received 'names' from the remote API is not an array of names.")]
    NamesNotAnArray,
    #[error("The received 'names' is empty, as the first element was not gettable.")]
    NamesArrayIsEmpty,
    #[error("The call to order receiver name is not a string.")]
    ReceiverNameNotAString,
    #[error("The received 'url' from the remote API is not a string.")]
    ReceiverUrlNotAString,
    #[error("The received 'url' from the remote API probably has a defunct format.")]
    ReceiverUrlWrongFormat,
    #[error("The received 'reference' from the remote API is not an array of references.")]
    ReferenceNotAnArray,
    #[error("The received 'reference' is empty, as the first element was not gettable.")]
    ReferenceArrayIsEmpty,
    #[error("The received 'zitation' from the remote API is not a string.")]
    ZitationNotAString,
    #[error("The received 'zitation' from the remote API probably has a defunct format.")]
    ZitationWrongFormat,
}

pub fn extract_call_to_order_paths_from_stages(
    stages: &Value,
) -> Option<Vec<HtmlCallToOrderExtraction>> {
    let mut call_to_orders_urls = vec![];
    for stage in stages.as_array()? {
        let Some(text) = stage.get("text") else {
            continue;
        };
        let text = text.as_str()?;

        if !text.contains("Ordnungsruf") {
            continue;
        }

        if !text.contains("erteilt") {
            continue;
        }

        if let Some(call_to_order_urls) = extract_call_to_order_urls(text) {
            call_to_orders_urls.push(call_to_order_urls);
        }
    }
    Some(call_to_orders_urls)
}

pub type HtmlCallToOrderExtraction = (String, String, String);

pub fn extract_call_to_order_urls(text: &str) -> Option<HtmlCallToOrderExtraction> {
    let fragment = Html::parse_fragment(text);
    let selector = Selector::parse("[href]").unwrap();

    let mut hrefs = fragment.select(&selector);

    let person_href = hrefs.next()?;
    let (_, person_path) = person_href
        .value()
        .attrs()
        .find(|(tag, _)| *tag == "href")?;

    let person_name = person_href.inner_html();

    let (_, call_to_order_gegenstand_path) = hrefs
        .next()?
        .value()
        .attrs()
        .find(|(tag, _)| *tag == "href")?;

    Some((
        person_path.to_string(),
        person_name,
        call_to_order_gegenstand_path.to_string(),
    ))
}

pub fn extract_call_to_order_id_from_path(gegenstand_path: &str) -> (Option<&str>, Option<&str>) {
    let mut components = gegenstand_path.split('/').rev();
    (components.next(), components.next())
}

#[cfg(test)]
mod tests {
    use crate::{
        call_to_order::{extract_call_to_order_id_from_path, CallToOrder},
        AustriaScraper,
    };

    use super::extract_call_to_order_urls;

    #[test]
    fn test_extract_call_to_order_id_from_path() {
        let (inr, ityp) = extract_call_to_order_id_from_path("/gegenstand/XXVII/GO/1239");
        assert_eq!(inr, Some("1239"));
        assert_eq!(ityp, Some("GO"));
    }

    #[test]
    fn test_extract_call_to_order_info_from_text() {
        let text = r#"Ordnungsruf erteilt an die Abgeordnete <a href="/person/83101" target="_blank">Sigrid Maurer, BA</a> (<a href="/gegenstand/XXVII/GO/1239">1239/GO</a>)"#;
        let (person_path, person_name, call_to_order_gegenstand_path) =
            extract_call_to_order_urls(text).unwrap();
        assert_eq!(person_path, "/person/83101");
        assert_eq!(call_to_order_gegenstand_path, "/gegenstand/XXVII/GO/1239");
        assert_eq!(person_name, "Sigrid Maurer, BA");
    }

    #[tokio::test]
    async fn test_call_to_order() {
        let scraper = AustriaScraper::default();
        let call_to_order = scraper
            .get_call_to_order("/gegenstand/XXVII/GO/1238")
            .await
            .unwrap();
        assert_eq!(
            call_to_order.description,
            "Präsident Ing. Hofer - Abg. Mag. Gerstl (1238/GO)"
        );
        assert_eq!(call_to_order.ityp, "GO");
        assert_eq!(call_to_order.inr, 1238);
        assert_eq!(call_to_order.receiver, "Mag. Wolfgang Gerstl");
        assert_eq!(call_to_order.receiver_id, 67199);
        assert_eq!(call_to_order.links_to, Some("NRSITZ/211".into()));
        println!("call_to_order: {call_to_order:?}")
    }
}
