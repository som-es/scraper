use lazy_static::__Deref;
use ::scraper::{Html, Selector};
use serde_derive::{Deserialize, Serialize};

mod config;
mod document;
mod error;
mod request;
mod scraper;
mod url;

pub use document::*;

pub use common_scrapes::*;

pub use crate::scraper::*;
pub mod serde_json {
    pub use ::serde_json::*;
}

/// /person/83153 -> 83153
fn person_url_to_id(url: &str) -> Option<u64> {
    url.split('/').last().and_then(|id| id.parse::<u64>().ok())
}

pub fn extract_text_from_html(html: &str) -> String {
    let fragment = Html::parse_fragment(html);

    fragment
        .tree
        .nodes()
        .flat_map(|node| node.value().as_text().map(|text| text.deref()))
        .collect::<String>()
}

pub fn extract_text_from_html_via_name_attribute(html: &str, name_attribute: &str) -> String {
    let fragment = Html::parse_fragment(html);

    let selector = Selector::parse(&format!("[name=\"{}\"]", name_attribute)).unwrap();

    fragment
        .select(&selector)
        .flat_map(|node| Some(node.parent()?.parent()?))
        .inspect(|node| println!("node: {node:?}"))        
        .for_each(|node| {
            node.children().for_each(|node| {
                
                println!("node: {node:?}");
            });
        });
        
    fragment
        .select(&selector)
        .flat_map(|node| node.parent()?.parent()?.value().as_text().map(|text| text.to_string()))
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use crate::extract_text_from_html_via_name_attribute;

    #[test]
    fn test_extract_text_from_html_via_name_attribute() {
        let url = "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/43/fnameorig_845422.html#RU_221617";
        let html = reqwest::blocking::get(url).unwrap();
        let html = html.text().unwrap();

        let text = extract_text_from_html_via_name_attribute(&html, "RU_221617");
        println!("text: {text}")
    }
}

