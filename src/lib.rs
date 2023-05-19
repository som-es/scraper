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

