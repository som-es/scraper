use async_trait::async_trait;

use chrono::{NaiveDate, NaiveDateTime};
use common_scrapes::Document;
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    documents, error::ScraperError, extract_from_gegenstand_url, extract_text_from_html,
    legis_init::extract_topics, person_url_to_id, proposals::ProposalsPayload,
};

use super::{AustriaScraper, Scraper};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MinistrialProposal {
    pub id: u64,
    pub inr: u64,
    pub delegate_ids: Vec<u64>,
    pub ityp: String,
    pub gp: String,
    pub title: String,
    pub description: String,
    pub emphasis: Option<String>,
    pub documents: Vec<Document>,
    pub created_at: NaiveDateTime,
    pub updated_at: Option<NaiveDateTime>,
    pub topics: Vec<String>,
    pub eurovoc_topics: Vec<String>,
    pub other_keyword_topics: Vec<String>,
    pub gegenstand_info: GegenstandInfo,
    pub legis_gp: Option<String>,
    pub legis_init_inr: Option<u32>,
    pub legis_init_ityp: Option<String>,
}

#[async_trait]
pub trait MinistrialProposalScraper: Scraper {
    const FILTER_PATH: &'static str;
    const TO_DELEGATES_IDX: &'static str;

    fn ministrial_payload(&self) -> Self::Payload;

    async fn post_ministrial_proposals_payload(
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

    fn raw_ministrial_prop_into_ministrial_prop(
        raw_proposal: Value,
        gegenstand_info: GegenstandInfo,
    ) -> Option<MinistrialProposal>;

    fn ministrial_prop_paths(&self, proposals: Value) -> Result<Vec<GegenstandInfo>, ScraperError>;

    async fn get_ministrial_proposal(
        &self,
        gegenstand_info: GegenstandInfo,
    ) -> Result<MinistrialProposal, ScraperError> {
        let raw_proposal = self.get_gegenstand_info(&gegenstand_info.path).await?;
        Self::raw_ministrial_prop_into_ministrial_prop(raw_proposal, gegenstand_info)
            .ok_or(ScraperError::InvalidProposalInfoError)
    }
}

const GEGENSTAND_PATH_IDX: usize = 7;
const RESSORT_SHORTFORM_IDX: usize = 6;
const RESSORT_IDX: usize = 16;
const DUE_TO_IDX: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GegenstandInfo {
    pub path: String,
    pub ressort: String,
    pub ressort_shortform: String,
    pub due_to: NaiveDate,
}

impl AustriaScraper {
    pub fn ministiral_paths(&self, gegenstaende: Value) -> Option<Vec<GegenstandInfo>> {
        Some(
            gegenstaende[Self::LIST_ENTRIES]
                .as_array()?
                .iter()
                .flat_map(|gegenstand| {
                    let geg = gegenstand.as_array()?;
                    let path = geg[GEGENSTAND_PATH_IDX].as_str()?.to_string();
                    let ressort = geg[RESSORT_IDX].as_str()?.to_string();
                    let ressort_shortform = geg[RESSORT_SHORTFORM_IDX].as_str()?.to_string();
                    let due_to = geg[DUE_TO_IDX].as_str()?.to_string();
                    let due_to = NaiveDate::parse_from_str(&due_to, "%d.%m.%Y ").ok()?;
                    Some(GegenstandInfo {
                        path,
                        ressort,
                        ressort_shortform,
                        due_to,
                    })
                })
                .collect(),
        )
    }
}

impl MinistrialProposalScraper for AustriaScraper {
    const FILTER_PATH: &'static str = "/Filter/api/filter/data/81?js=eval&showAll=true";
    const TO_DELEGATES_IDX: &'static str = "rows";

    fn ministrial_payload(&self) -> Self::Payload {
        json!({"GP_CODE": self.scrape_info.gp, "DOKTYP": "MEG"})
    }

    fn ministrial_prop_paths(&self, proposals: Value) -> Result<Vec<GegenstandInfo>, ScraperError> {
        // println!("{}", serde_json::to_string_pretty(&proposals).unwrap());
        self.ministiral_paths(proposals)
            .ok_or(ScraperError::ProposalParseError)
    }

    fn raw_ministrial_prop_into_ministrial_prop(
        raw_ministrial: Value,
        gegenstand_info: GegenstandInfo,
    ) -> Option<MinistrialProposal> {
        let raw_ministrial_content = &raw_ministrial["content"];
        let ityp = raw_ministrial_content["ityp"].as_str().unwrap().to_string();

        // println!("{}", serde_json::to_string_pretty(&raw_ministrial).unwrap());

        let description = raw_ministrial_content["description"].as_str()?.to_string();
        let raw_documents = raw_ministrial_content["documents"]
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

        let title = raw_ministrial["meta"]["title"].as_str()?.to_string();
        let inr = raw_ministrial_content["inr"].as_u64()?;

        let created_at = raw_ministrial_content["einlangen"].as_str()?;
        let created_at = NaiveDateTime::parse_from_str(created_at, "%Y-%m-%dT%H:%M:%S").ok()?;

        let updated_at = raw_ministrial_content["update"].as_str();
        let updated_at = updated_at.map(|updated_at| {
            NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%dT%H:%M:%S").unwrap()
        });

        let id = inr;

        let gp = raw_ministrial_content["gp_code"].as_str()?.to_string();

        let emphasis = if let Some(shortinfo) = raw_ministrial_content.get("shortinfo") {
            let teil1 = extract_text_from_html(shortinfo["teil1"].as_str()?);
            let teil2 = extract_text_from_html(shortinfo["teil2"].as_str().unwrap_or_default());
            Some(format!("{teil1} {teil2}"))
        } else {
            None
        };

        let topics = extract_topics(raw_ministrial_content, "topics").unwrap_or_default();
        let eurovoc_topics = extract_topics(raw_ministrial_content, "eurovoc").unwrap_or_default();
        let other_keyword_topics =
            extract_topics(raw_ministrial_content, "headwords").unwrap_or_default();

        let names = raw_ministrial_content["names"].as_array()?;
        let delegate_ids = names
            .iter()
            .flat_map(|name| person_url_to_id(name["url"].as_str()?))
            .collect::<Vec<u64>>();

        let (mut legis_gp, mut legis_init_inr, mut legis_init_ityp) = (None, None, None);
        if let Some(entry) = raw_ministrial_content["stages"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .last()
        {
            let text = entry["text"].as_str().unwrap();
            if text.contains("Regierungsvorlage") {
                let rv = extract_from_gegenstand_url(text).unwrap();
                legis_gp = Some(rv.0);
                legis_init_ityp = Some(rv.1);
                legis_init_inr = Some(rv.2.unwrap());
            }
        }

        Some(MinistrialProposal {
            id,
            inr,
            ityp,
            gp,
            title,
            emphasis,
            description,
            delegate_ids,
            documents,
            created_at,
            updated_at,
            topics,
            eurovoc_topics,
            other_keyword_topics,
            gegenstand_info,
            legis_gp,
            legis_init_inr,
            legis_init_ityp,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{proposals::ProposalsPayload, AustriaScraper};

    use super::MinistrialProposalScraper;

    #[tokio::test]
    async fn test_ministrial_prop_me_343() {
        let scraper = AustriaScraper::default();

        // scraper.get_ministrial_proposal("/gegenstand/XXVII/ME/248");

        let proposal_payload = ProposalsPayload::default();
        let proposals = scraper
            .post_ministrial_proposals_payload(proposal_payload)
            .await
            .unwrap();
        for path in scraper.ministrial_prop_paths(proposals).unwrap() {
            // println!("path: {path:?}");
            let ministrial = scraper.get_ministrial_proposal(path).await.unwrap();
            println!(
                "shortform: {}",
                ministrial.gegenstand_info.ressort_shortform
            );
            // println!("{:?} {:?} {:?}", ministrial.legis_gp, ministrial.legis_init_inr, ministrial.legis_init_ityp);
        }

        // println!("ministrial: {ministrial:?}");
    }
}
