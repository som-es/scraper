use serde_derive::{Deserialize, Serialize};

mod config;
mod error;
mod request;
mod scraper;
mod url;
mod document;

pub use document::*;

pub use crate::scraper::*;
pub mod serde_json {
    pub use ::serde_json::*;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Delegate {
    pub id: u64,
    pub name: String,
    pub is_active: bool,
    pub party: Option<String>,
    pub constituency: Option<String>,
    pub council: Option<String>,
    pub seat: Option<Seat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Seat {
    pub row: u16,
    pub col: u16,
}
