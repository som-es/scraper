use async_trait::async_trait;
use common_scrapes::{LegislativeInitiative, Vote};
use regex::Regex;
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    documents, error::ScraperError, person_url_to_id, proposals::ProposalScraper, AustriaScraper,
    GegenstandPaths, Scraper,
};

#[async_trait]
pub trait LegislativeInitiatives: Scraper {
    const FILTER_PATH: &'static str;

    fn legis_payload(&self) -> Self::Payload;

    async fn post_legis_payload(&self) -> Result<Value, ScraperError> {
        let payload = self.legis_payload();
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

    fn raw_legis_into_legis(raw_legis: Value) -> Option<LegislativeInitiative>;

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError>;

    async fn get_legis_init(
        &self,
        proposal_path: &str,
    ) -> Result<LegislativeInitiative, ScraperError> {
        let raw_proposal = self.get_gegenstand_info(proposal_path).await?;
        Self::raw_legis_into_legis(raw_proposal).ok_or(ScraperError::LegisInitParseError)
    }
}

impl LegislativeInitiatives for AustriaScraper {
    const FILTER_PATH: &'static str =
        "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFP_003&listeId=10103&showAll";

    fn legis_payload(&self) -> Self::Payload {
        json!({"GP": self.scrape_info.gp})
    }

    fn raw_legis_into_legis(raw_legis: Value) -> Option<LegislativeInitiative> {
        let description = raw_legis["meta"]["description"].as_str()?.to_string();
        let title = raw_legis["meta"]["title"].as_str()?.to_string();

        let legis_content = &raw_legis["content"];

        let ityp = legis_content["ityp"].as_str()?.to_string();
        let inr = legis_content["inr"].as_u64()?;

        let preconst = &legis_content["preconst"].as_array()?[0];

        let gp = legis_content["gp_code"].as_str()?.to_string();

        let raw_documents = preconst["documents"].as_array()?;
        let documents = documents(raw_documents)?;

        let names = legis_content["names"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let delegate_ids = names
            .iter()
            .flat_map(|name| person_url_to_id(name["url"].as_str()?))
            .collect::<Vec<u64>>();

        let votes = legis_content["vote"]["result"]
            .as_array()?
            .iter()
            .flat_map(|vote| {
                Some(Vote {
                    party: vote["text"].as_str()?.to_string(),
                    fraction: vote["fraction"].as_u64()? as u32,
                    infavor: vote["infavor"].as_bool()?,
                })
            })
            .collect::<Vec<Vote>>();

        let accepted = legis_content["vote"]["infavor"].as_bool()?;

        // let shortinfo = &legis_content["shortinfo"];

        // let teil1 = remove_html_tags(shortinfo["teil1"].as_str()?);

        // let json = serde_json::to_string_pretty(&raw_legis).unwrap();
        // println!("{json}");

        let id = format!("{inr}/{ityp}");

        Some(LegislativeInitiative {
            title,
            description,
            emphasis: vec![], // todo: extract emphasis from html
            // council: todo!(),
            gp,
            ityp,
            id,
            votes,
            documents,
            delegate_ids,
            accepted,
        })
    }

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError> {
        self.gegenstand_paths(initiatives, 12)
            .ok_or(ScraperError::ProposalParseError)
    }
}

// not working
fn remove_html_tags(html: &str) -> String {
    lazy_static::lazy_static! {
        static ref HTML_REGEX: Regex = Regex::new(r"<.*>").unwrap();
    }
    HTML_REGEX.replace_all(html, "").to_string()
}

#[cfg(test)]
mod tests {
    use crate::{
        legis_init::{LegislativeInitiatives, Vote},
        AustriaScraper,
    };

    #[tokio::test]
    async fn test_legis_init() {
        let scraper = AustriaScraper::default();

        let initiatives = scraper.post_legis_payload().await.unwrap();
        for path in scraper.legis_paths(initiatives).unwrap() {
            println!("path: {path:?}");
        }
    }

    #[tokio::test]
    async fn test_get_legis_init() {
        let scraper = AustriaScraper::default();

        let start = std::time::Instant::now();

        let initiatives = scraper.post_legis_payload().await.unwrap();
        for path in scraper.legis_paths(initiatives).unwrap() {
            //println!("path: {path:?}");
            let Ok(init) = scraper.get_legis_init(&path).await else {
                continue;
            };

            println!("id: {}, title: {}", init.id, init.title);
        }

        println!("elapsed: {:?}", start.elapsed());
    }

    #[tokio::test]
    async fn test_get_legis_init_i_1954_path() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/I/1954")
            .await
            .unwrap();

        assert_eq!(
            init.title,
            "Kraftfahrgesetz, Änderung (41. KFG-Novelle) (1954 d.B.) | Parlament Österreich"
        );
        assert_eq!(
            init.description,
            "Kraftfahrgesetz, Änderung (41. KFG-Novelle) (1954 d.B.)"
        );

        assert_eq!(init.gp, "XXVII");
        assert_eq!(init.emphasis, Vec::<String>::new());
        assert_eq!(
            init.votes,
            vec![
                Vote {
                    party: "ÖVP".to_string(),
                    fraction: 71,
                    infavor: true
                },
                Vote {
                    party: "SPÖ".to_string(),
                    fraction: 40,
                    infavor: false
                },
                Vote {
                    party: "FPÖ".to_string(),
                    fraction: 30,
                    infavor: false
                },
                Vote {
                    party: "GRÜNE".to_string(),
                    fraction: 26,
                    infavor: true
                },
                Vote {
                    party: "NEOS".to_string(),
                    fraction: 15,
                    infavor: true
                }
            ]
        );

        assert!(init.accepted);

        assert_eq!(init.id, "1954/I");
        assert_eq!(init.ityp, "I".to_string());
    }
}
