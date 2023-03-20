use serde_derive::{Serialize, Deserialize};

mod config;
mod scraper;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Delegate {
    name: String,
    party: String,
    constituency: String,
}