use async_trait::async_trait;
use serde_json::Value;

use crate::{error::ScraperError, Scraper};

pub struct CallToOrderInfo {
    pub description: String,
    pub ityp: String,
    pub inr: u64,
    pub person_name: String,
    pub person_id: u64,
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

    fn extract_raw_call_to_order(raw_call_to_order: Value) -> Option<CallToOrderInfo> {
        let description = raw_call_to_order["meta"]["description"]
            .as_str()?
            .to_string();

        let call_to_order_content = &raw_call_to_order["content"];

        let ityp = call_to_order_content["ityp"].as_str()?.to_string();
        let inr = call_to_order_content["inr"].as_u64()?;

        let name = call_to_order_content["names"].as_array()?.get(0)?;
        let person_name = name["name"].as_str()?.to_string();
        let person_id = name["url"].as_str()?.to_string();

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
            person_name,
            person_id: person_id.parse::<u64>().unwrap(),
            links_to,
        })
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_call_to_order() {}
}
