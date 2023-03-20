use serde_derive::{Deserialize, Serialize};
use std::{error::Error, path::Path};

use self::delegate::DelegateEndpoint;

mod delegate;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScraperConfig {
    base_url: String,
    legislatur: String,
    delegate_endpoint: DelegateEndpoint,
}

impl ScraperConfig {
    pub fn from_json<P: AsRef<Path>>(path: P) -> Result<ScraperConfig, Box<dyn Error>> {
        Ok(serde_json::from_reader(std::fs::File::open(path)?)?)
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn Error>> {
        serde_json::to_writer_pretty(std::fs::File::create(path)?, self)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{delegate::DelegateEndpoint, ScraperConfig};

    #[test]
    fn test_save_config() -> Result<(), Box<dyn std::error::Error>> {
        let config = ScraperConfig {
            base_url: "https://www.parlament.gv.at".to_string(),
            legislatur: "XXVII".to_string(),
            delegate_endpoint: DelegateEndpoint {
                delegate_url: "/person/".to_string(),
                delegate_marker: "rows".to_string(),
                deleagate_url_idx: 8,
            },
        };

        config.save("config2.json")
    }

    #[test]
    fn test_read_config() {
        let config = super::ScraperConfig::from_json("config.json").unwrap();
        assert_eq!(config.base_url, "https://www.parlament.gv.at");
        // assert_eq!(config.deputy_url, "/de/ratsmitglieder/");
    }
}
