use async_trait::async_trait;
use common_scrapes::PlenarSpeech;
use serde_json::{json, Value};

use crate::{error::ScraperError, AustriaScraper, GegenstandPaths, Scraper};

pub struct Debate {
    about: String,
    speeches: Vec<PlenarSpeech>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PlenarInfo {
    description: String,
    ityp: String,
    inr: u64,
}

#[async_trait]
pub trait Plenar: Scraper {
    const FILTER_PATH: &'static str;
    fn plenar_payload(&self) -> Self::Payload;

    async fn post_plenar_payload(&self) -> Result<Value, ScraperError> {
        let payload = self.plenar_payload();
        self.post_payload(
            payload,
            format!(
                "{base_url}{filter}",
                base_url = Self::BASE_URL,
                filter = Self::FILTER_PATH
            ),
        )
        .await
    }

    fn plenar_paths(&self, plenar_infos: Value) -> Result<GegenstandPaths, ScraperError>;

    fn raw_plenar_into_plenar_info(raw_plenar: Value) -> Option<PlenarInfo>;

    async fn get_plenar(&self, plenar_path: &str) -> Result<PlenarInfo, ScraperError> {
        let raw_proposal = self.get_gegenstand_info(plenar_path).await?;
        Self::raw_plenar_into_plenar_info(raw_proposal)
            .ok_or(ScraperError::PlenarGegenstandParseError)
    }
}

impl Plenar for AustriaScraper {
    const FILTER_PATH: &'static str =
        "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFP_007&listeId=undefined&showAll=true";

    fn plenar_payload(&self) -> Self::Payload {
        json!({"MODUS": ["PLENAR"], "GP": self.scrape_info.gp, "NRBRBV": self.scrape_info.council_type})
    }

    fn plenar_paths(&self, plenar_infos: Value) -> Result<GegenstandPaths, ScraperError> {
        self.gegenstand_paths(plenar_infos, 8)
            .ok_or(ScraperError::ProposalParseError)
    }

    fn raw_plenar_into_plenar_info(raw_plenar: Value) -> Option<PlenarInfo> {
        let description = raw_plenar["meta"]["description"]
            .as_str()
            .unwrap()
            .to_string();

        // TODO: there is even more content! look in insomnia
        let plenar_content_0 = &raw_plenar["content"][0];

        let ityp = plenar_content_0["ityp"].as_str().unwrap().to_string();
        let inr = plenar_content_0["inr"].as_u64().unwrap();

        // path with no content at 1: /gegenstand/XXVII/NRSITZ/216
        let past_debates = raw_plenar["content"][1].as_array().expect("no content at 1");
        let speeches = extract_speeches(past_debates);

        Some(PlenarInfo {
            description,
            ityp,
            inr,
        })
    }
}

pub fn extract_speeches(past_debates: &[Value]) -> Vec<PlenarSpeech> {
    past_debates
        .into_iter()
        .flat_map(|past_debate| past_debate["speeches"].as_array())
        .map(|speeches| {
            speeches
                .iter()
                .flat_map(|speech| {
                    let delegate_id = speech[3].as_u64()?;
                    let ty = speech[5].as_str()?.to_string();
                    let (minutes, seconds) = speech[7].as_str()?.split_at(2);
                    let duration_in_seconds =
                        minutes.parse::<u32>().unwrap() + seconds.parse::<u32>().unwrap();
                    Some(PlenarSpeech {
                        delegate_id,
                        ty,
                        duration_in_seconds,
                        // TODO: get spoken text html urls, use content[0]
                        spoken_text_html_urls: vec![],
                    })
                })
                .collect::<Vec<_>>()
        })
        .flatten()
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use crate::{AustriaScraper, Scraper};

    use super::Plenar;

    #[tokio::test]
    async fn test_get_gegenstand_paths_plenar() {
        let scraper = AustriaScraper::default();
        let plenar_infos = scraper.post_plenar_payload().await.unwrap();

        let paths = scraper
            .plenar_paths(plenar_infos)
            .expect("Failed to get paths");
        println!("paths: {paths:?}");
    }

    #[tokio::test]
    async fn test_look_all_plenar_gegenstand_paths() {
        let scraper = AustriaScraper::default();

        let plenar_infos = scraper.post_plenar_payload().await.unwrap();

        let paths = scraper
            .plenar_paths(plenar_infos)
            .expect("Failed to get paths");
        for path in paths {
            println!("path: {path}");
            // path with no content at 1: /gegenstand/XXVII/NRSITZ/216
            let plenar_info = scraper.get_plenar(&path).await.unwrap();
            println!("plenar_info: {plenar_info:?}");
        }
    }
}
