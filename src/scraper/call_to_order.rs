use async_trait::async_trait;
use scraper::{Html, Selector};
use serde_json::Value;

use crate::{error::ScraperError, AustriaScraper, Scraper};

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
    ) -> Result<CallToOrderInfo, ScraperError> {
        let raw_call_to_order = self.get_gegenstand_info(call_to_order_path).await?;
        Self::extract_raw_call_to_order(raw_call_to_order)
            .ok_or(ScraperError::CallTorOrderGegenstandParseError)
    }

    fn extract_raw_call_to_order(raw_call_to_order: Value) -> Option<CallToOrderInfo>;
}

impl CallToOrder for AustriaScraper {
    fn extract_raw_call_to_order(raw_call_to_order: Value) -> Option<CallToOrderInfo> {
        let description = raw_call_to_order["meta"]["description"]
            .as_str()?
            .to_string();

        let call_to_order_content = &raw_call_to_order["content"];

        let ityp = call_to_order_content["ityp"].as_str()?.to_string();
        let inr = call_to_order_content["inr"].as_u64()?;

        let name = call_to_order_content["names"].as_array()?.get(0)?;
        let receiver = name["name"].as_str()?.to_string();
        let receiver_id = name["url"].as_str()?.split('/').nth(2)?.to_string();

        let links_to =
            if let Some(reference) = call_to_order_content["reference"].as_array()?.get(0) {
                let zitation = reference["zitation"].as_str()?;
                let zitation = zitation.split('/').collect::<Vec<&str>>();
                Some(format!("{}/{}", zitation[1], zitation[0]))
            } else {
                None
            };

        // protection layer against API changes?

        Some(CallToOrderInfo {
            description,
            ityp,
            inr,
            receiver,
            receiver_id: receiver_id.parse::<u64>().unwrap(),
            links_to,
        })
    }
}

pub fn extract_call_to_order_paths_from_stages(stages: &Value) -> Option<()> {
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

    }
    Some(())
}

pub fn extract_call_to_order_info(text: &str) {
    let fragment = Html::parse_fragment(text);
    let selector = Selector::parse("[href]").unwrap();

    for val in fragment.select(&selector) {
        println!("val: {:?}", val.value().attrs().collect::<Vec<_>>())
    }
        

}

#[cfg(test)]
mod tests {
    use crate::{call_to_order::CallToOrder, AustriaScraper};

    use super::extract_call_to_order_info;

    #[test]
    fn test_extract_call_to_order_info_from_text() {
        let text = r#"Ordnungsruf erteilt an die Abgeordnete <a href="/person/83101" target="_blank">Sigrid Maurer, BA</a> (<a href="/gegenstand/XXVII/GO/1239">1239/GO</a>)"#;
        extract_call_to_order_info(text);
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
