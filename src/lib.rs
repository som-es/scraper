use serde_derive::{Deserialize, Serialize};

mod config;
mod error;
mod request;
mod scraper;
mod url;

pub use crate::scraper::*;
pub mod serde_json {
    pub use ::serde_json::*;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Delegate {
    pub id: String,
    pub name: String,
    pub party: String,
    pub constituency: String,
    pub council: String,
    pub seat: Seat,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Seat {
    pub row: u16,
    pub col: u16,
}
