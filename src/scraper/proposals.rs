use async_trait::async_trait;

use chrono::{NaiveDate, NaiveDateTime};
use common_scrapes::Document;
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    documents, error::ScraperError, legis_init::extract_topics, person_url_to_id, DatePayload,
    GeneralScrapeInfo,
};

use super::{AustriaScraper, GegenstandPaths, Scraper};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ProposalsPayload {
    pub date_payload: DatePayload,
    scrape_info: GeneralScrapeInfo,
}

impl ProposalsPayload {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_date_range(mut self, date_start: NaiveDate, date_end: NaiveDate) -> Self {
        self.date_payload.with_date_range(date_start, date_end);
        self
    }

    pub fn as_json(&self) -> Value {
        json!({"NRBR": self.scrape_info.council_type, "GP_CODE": self.scrape_info.gp, "DATUM_VON": self.date_payload.to_at_format()})
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProposalType {
    /// String in points to the answered proposals
    AB(Vec<String>),
    /// String in this enum is the type of proposal!
    Other(String),
}

impl ProposalType {
    pub fn from_str(ityp: &str, raw_proposal_content: &Value) -> Option<Self> {
        Some(match ityp {
            "AB" => {
                let answered_ids = raw_proposal_content["reference"]
                    .as_array()?
                    .iter()
                    .flat_map(|reference| Some(reference["zitation"].as_str()?.to_string()))
                    .collect::<Vec<String>>();
                ProposalType::AB(answered_ids)
            }
            _ => ProposalType::Other(ityp.to_string()),
        })
    }
}

#[async_trait]
pub trait ProposalScraper: Scraper {
    const FILTER_PATH: &'static str;
    const TO_DELEGATES_IDX: &'static str;

    fn proposals_payload(&self) -> Self::Payload;

    async fn post_proposals_payload(
        &self,
        proposal_payload: ProposalsPayload,
    ) -> Result<Value, ScraperError> {
        self.post_payload(
            proposal_payload.as_json(),
            format!(
                "{base_url}{filter}",
                base_url = Self::BASE_URL,
                filter = Self::FILTER_PATH
            ),
        )
        .await
    }

    fn raw_proposal_into_proposal(raw_proposal: Value) -> Option<Proposal>;

    fn proposal_paths(&self, proposals: Value) -> Result<GegenstandPaths, ScraperError>;

    async fn get_proposal(&self, proposal_path: &str) -> Result<Proposal, ScraperError> {
        let raw_proposal = self.get_gegenstand_info(proposal_path).await?;
        Self::raw_proposal_into_proposal(raw_proposal).ok_or(ScraperError::InvalidProposalInfoError)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub ityp: String,
    pub gp: String,
    pub title: String,
    pub proposal_type: ProposalType,
    pub description: Option<String>,
    pub issuer_ids: Vec<u64>,
    pub receiver_ids: Vec<u64>,
    pub documents: Vec<Document>,
    pub topics: Vec<String>,
    pub eurovoc_topics: Vec<String>,
    pub other_keyword_topics: Vec<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: Option<NaiveDateTime>,
}

impl ProposalScraper for AustriaScraper {
    const FILTER_PATH: &'static str = "/Filter/api/filter/data/101?js=eval&showAll=true";
    // TODO
    // const FILTER_PATH: &'static str = "/Filter/api/filter/data/101?js=eval";
    const TO_DELEGATES_IDX: &'static str = "rows";

    fn proposals_payload(&self) -> Self::Payload {
        json!({"NRBR": [self.scrape_info.council_type], "GP_CODE": self.scrape_info.gp})
    }

    fn proposal_paths(&self, proposals: Value) -> Result<GegenstandPaths, ScraperError> {
        self.gegenstand_paths(proposals, 14)
            .ok_or(ScraperError::ProposalParseError)
    }

    fn raw_proposal_into_proposal(raw_proposal: Value) -> Option<Proposal> {
        let raw_proposal_content = &raw_proposal["content"];
        // if raw_proposal_content["names"].as_array().is_none() {
        // }

        // let json = serde_json::to_string_pretty(raw_proposal_content).unwrap();
        // println!("{json}");
        let ityp = raw_proposal_content["ityp"].as_str().unwrap().to_string();

        // weird AB: ityp AB inr 13696 gp XXVII, has no delegates associated with AB, no fitting title
        // https://www.parlament.gv.at/gegenstand/XXVII/AB/13696

        let issuer_ids;
        let receiver_ids;
        if let Some(names) = raw_proposal_content["names"].as_array() {
            issuer_ids = names
                .iter()
                .filter(|name| name["funktext"] == "Eingebracht von")
                .flat_map(|name| person_url_to_id(name["url"].as_str().unwrap()))
                .collect::<Vec<u64>>();

            let receiver_text = if ityp == "AB" {
                "Beantwortet durch"
            } else {
                "Eingebracht an"
            };

            receiver_ids = names
                .iter()
                .filter(|name| name["funktext"] == receiver_text)
                .flat_map(|name| person_url_to_id(name["url"].as_str().unwrap()))
                .collect::<Vec<u64>>();
        } else {
            issuer_ids = vec![];
            receiver_ids = vec![];
        }

        let description = raw_proposal_content["description"].as_str().map(|x| x.to_string());
        let raw_documents = raw_proposal_content["documents"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let documents = match documents(&raw_documents) {
            Some(d) => d,
            None => {
                // "Dieses Dokument ist nicht öffentlich zugänglich"
                // if !raw_documents.is_empty() {
                //     log::error!("Raw documents are not empty, but documents could not be processed?: {:?}", serde_json::to_string_pretty(&raw_documents));
                // }
                vec![]
            }
        };

        let title = raw_proposal["meta"]["title"].as_str()?.to_string();
        let inr = raw_proposal_content["inr"].as_u64()?;

        let created_at = raw_proposal_content["einlangen"].as_str()?;
        let created_at = NaiveDateTime::parse_from_str(created_at, "%Y-%m-%dT%H:%M:%S").ok()?;

        let updated_at = raw_proposal_content["update"].as_str();
        let updated_at = updated_at.map(|updated_at| {
            NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%dT%H:%M:%S").unwrap()
        });

        let id = format!("{ityp}/{inr}");

        let gp = raw_proposal_content["gp_code"].as_str()?.to_string();

        let proposal_type = ProposalType::from_str(&ityp, raw_proposal_content)?;

        let topics = extract_topics(raw_proposal_content, "topics").unwrap_or_default();
        let eurovoc_topics = extract_topics(raw_proposal_content, "eurovoc").unwrap_or_default();
        let other_keyword_topics =
            extract_topics(raw_proposal_content, "headwords").unwrap_or_default();

        Some(Proposal {
            id,
            ityp,
            gp,
            title,
            description,
            issuer_ids,
            receiver_ids,
            documents,
            proposal_type,
            created_at,
            updated_at,
            topics,
            eurovoc_topics,
            other_keyword_topics,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        proposals::{Document, ProposalType, ProposalsPayload},
        scraper::{proposals::ProposalScraper, AustriaScraper},
    };

    use common_scrapes::FileType;

    #[tokio::test]
    async fn test_post_proposal_payload() {
        let scraper = AustriaScraper::default();

        let proposal_payload = ProposalsPayload::default();
        let proposals = scraper
            .post_proposals_payload(proposal_payload)
            .await
            .unwrap();

        for proposal in proposals["rows"].as_array().unwrap() {
            println!("{}", proposal[14]);
        }

        // println!("proposals: {}", serde_json::to_string_pretty(&proposals).unwrap());
    }

    #[tokio::test]
    async fn test_get_all_proposals() -> Result<(), Box<dyn std::error::Error>> {
        let scraper = AustriaScraper::default();

        let proposal_paths = match std::fs::read_to_string("prop-paths") {
            Ok(p) => {
                println!("read");
                serde_json::from_str(&p).unwrap()
            }
            Err(e) => {
                println!("e: {e:?}");
                let proposal_payload = ProposalsPayload::default();
                let proposals = scraper.post_proposals_payload(proposal_payload).await?;

                scraper.proposal_paths(proposals).unwrap()
            }
        };
        println!("scraped paths");

        // let out = serde_json::to_string(&proposal_paths).unwrap();
        // std::fs::write("prop-paths", out).unwrap();
        println!("props: {}", proposal_paths.len());
        return Ok(());
        for proposal_path in &proposal_paths {
            if proposal_path.contains("A") {
                continue;
            }
            println!("proposal_path: {}", proposal_path);
            match scraper.get_proposal(proposal_path).await {
                Ok(proposal) => {
                    // println!("proposal: {proposal:?}");
                    if proposal.ityp == "J" {
                        println!("anfrage {:?}", proposal.issuer_ids);
                    }
                }
                Err(_) => println!("could not parse: {proposal_path}"),
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_get_proposal_from_path_multiple_issuer() -> Result<(), Box<dyn std::error::Error>>
    {
        let scraper = AustriaScraper::default();

        let proposal = scraper.get_proposal("/gegenstand/XXVII/UEA/568").await?;
        // println!("proposal: {:?}", proposal.eurovoc_topics);
        assert_eq!(proposal.issuer_ids, [83153, 5679]);

        Ok(())
    }

    #[tokio::test]
    async fn test_minister_entwurf() -> Result<(), Box<dyn std::error::Error>> {
        let scraper = AustriaScraper::default();

        let proposal = scraper.get_proposal("/gegenstand/XXVII/ME/232").await?;
        println!("proposal: {proposal:?}");
        Ok(())
    }
    // /gegenstand/XXVII/ME/232
    #[tokio::test]
    async fn test_get_proposal_from_path_with_issuer_and_receiver(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let scraper = AustriaScraper::default();

        let proposal = scraper.get_proposal("/gegenstand/XXVII/J/14815").await?;
        assert_eq!(proposal.receiver_ids, [20445]);
        assert_eq!(proposal.issuer_ids, [2344]);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_proposal_from_path_with_documents() -> Result<(), Box<dyn std::error::Error>>
    {
        let scraper = AustriaScraper::default();

        let proposal = scraper.get_proposal("/gegenstand/XXVII/J/14120").await?;
        assert_eq!(proposal.receiver_ids, [55727]);
        assert_eq!(proposal.issuer_ids, [2344]);

        assert_eq!(
            proposal.documents[0],
            Document {
                title: "Anfrage (gescanntes Original)".to_string(),
                link: "/dokument/XXVII/J/14120/imfname_1518329.pdf".to_string(),
                ftype: FileType::OriginalPDF
            }
        );

        assert_eq!(
            proposal.documents[1],
            Document {
                title: "Anfrage (elektr. übermittelte Version)".to_string(),
                link: "/dokument/XXVII/J/14120/fname_1518478.pdf".to_string(),
                ftype: FileType::DigitalPDF
            }
        );

        assert_eq!(
            proposal.documents[2],
            Document {
                title: "Anfrage (elektr. übermittelte Version)".to_string(),
                link: "/dokument/XXVII/J/14120/fnameorig_1518478.html".to_string(),
                ftype: FileType::HTML
            }
        );

        Ok(())
    }


    // /gegenstand/XXVII/GO/1731
     #[tokio::test]
    async fn test_get_go_1731_proposal() -> Result<(), Box<dyn std::error::Error>> {
        let scraper = AustriaScraper::default();

        let proposal = scraper.get_proposal("/gegenstand/XXVII/GO/1731").await?;
        Ok(())
    }
    #[tokio::test]
    async fn test_get_proposal_as_answer() -> Result<(), Box<dyn std::error::Error>> {
        let scraper = AustriaScraper::default();

        let proposal = scraper.get_proposal("/gegenstand/XXVII/AB/13676").await?;
        assert_eq!(proposal.receiver_ids, [55727]);

        match proposal.proposal_type {
            ProposalType::AB(answered_ids) => {
                assert_eq!(answered_ids[0], "14120/J");
            }
            _ => {
                panic!()
            }
        }

        Ok(())
    }
}
