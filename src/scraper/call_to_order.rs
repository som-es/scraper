use serde_json::Value;

use crate::Scraper;

pub struct CallToOrderInfo {
    pub description: String,
    pub ityp: String,
    pub inr: u64,
    pub person_name: String,
    pub person_id: u64,
    pub links_to: Option<String>,
}

pub trait CallToOrder: Scraper {
    fn extract_raw_call_to_order(raw_call_to_order: Value) -> Option<CallToOrderInfo> {
        let description = raw_call_to_order["meta"]["description"].as_str()?.to_string();

        let call_to_order_content = &raw_call_to_order["content"];

        let ityp = call_to_order_content["ityp"].as_str()?.to_string();
        let inr = call_to_order_content["inr"].as_u64()?;

        let name = call_to_order_content["names"].as_array()?.get(0)?;
        let person_name = name["name"].as_str()?.to_string();
        let person_id = name["url"].as_str()?.to_string();
        
        let links_to = if let Some(reference) = call_to_order_content["reference"].as_array()?.get(0) {
            let zitation = reference["zitation"].as_str()?;
            let zitation = zitation.split('/').collect::<Vec<&str>>();
            Some(format!("{}/{}", zitation[1], zitation[0]))
        } else {
            None
        };
        
        // protection layer against API changes?

        todo!()
    }
}


