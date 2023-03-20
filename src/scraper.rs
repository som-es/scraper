/*pub trait Scraper {

}*/

use std::{path::Path, error::Error};
use crate::config::ScraperConfig;

mod delegates;

pub struct Scraper {
    config: ScraperConfig
}

impl Scraper {
    pub fn from_json() {

    }
}
