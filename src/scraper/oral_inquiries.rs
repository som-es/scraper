use async_trait::async_trait;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    convert_vienna_datetime_to_utc, error::ScraperError, extract_text_from_html,
    legis_init::extract_topics, person_url_to_id, LastUpdated,
};

use super::{AustriaScraper, Scraper};

/// An oral parliamentary question ("Mündliche Anfrage"), including all
/// exchanges (main question, main answer, supplementary questions) as they
/// appear in the stenographic protocol of the plenary session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OralInquiry {
    pub inr: u64,
    pub gp: String,
    pub title: String,
    /// The written question text submitted with the Anfrage (HTML stripped).
    pub question_text: String,
    /// Person ID of the delegate who submitted the question.
    pub asker_id: u64,
    /// Person ID of the minister/official to whom the question is directed.
    pub recipient_id: u64,
    /// Short name of the recipient's ministry (e.g. `"Bundeskanzleramt"`).
    pub recipient_ministry: Option<String>,
    /// Plenary session number in which the question was asked.
    pub plenary_session: Option<u64>,
    /// All oral exchanges: main question, main answer, and supplementary
    /// questions (Zusatzfragen) in chronological order.
    pub exchanges: Vec<OralExchange>,
    /// References to related documents, e.g. written supplement answers
    /// ("Schriftliche Ergänzung").
    pub references: Vec<OralReference>,
    pub topics: Vec<String>,
    pub eurovoc_topics: Vec<String>,
    pub other_keyword_topics: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl LastUpdated for OralInquiry {
    fn last_updated(&self) -> DateTime<Utc> {
        self.updated_at
            .unwrap_or_else(|| chrono::Local::now().to_utc())
    }
}

/// A single exchange event in the oral question procedure (Fragestunde).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OralExchange {
    pub kind: ExchangeKind,
    /// Person ID of the speaker (asker or answerer).
    /// `None` for [`ExchangeKind::FollowUpAnswerWritten`] which has no
    /// associated individual speaker.
    pub person_id: Option<u64>,
    /// References into the stenographic protocol where this exchange appears.
    pub protocol_refs: Vec<ProtocolRef>,
}

/// The role an [`OralExchange`] plays in the question-and-answer procedure.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExchangeKind {
    /// The initial oral question posed by the submitting delegate.
    MainQuestion,
    /// The minister's oral answer to the main question.
    MainAnswer,
    /// A follow-up question (Zusatzfrage) posed by any delegate.
    FollowUpQuestion,
    /// A follow-up question answered in writing rather than orally.
    FollowUpAnswerWritten,
}

/// A pointer into the stenographic protocol document for an exchange.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProtocolRef {
    pub gp: String,
    pub session_id: u64,
    /// First page of the relevant protocol section (older page-based format).
    pub page_from: Option<u64>,
    /// Last page of the relevant protocol section (older page-based format).
    pub page_to: Option<u64>,
    /// Direct URL to the protocol section (may contain an `#anchor`).
    pub url: String,
    /// Human-readable title of the protocol section.
    pub title: String,
}

/// A reference from an [`OralInquiry`] to a related document, e.g. a
/// written supplement ("Schriftliche Ergänzung der Beantwortung").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OralReference {
    /// Short label, e.g. `"Zusatz"`.
    pub text: String,
    /// Longer description of the reference type.
    pub subject: String,
    /// Zitation of the referenced document, e.g. `"Zu 384/M"`.
    pub zitation: String,
    /// Relative URL to the referenced document.
    pub url: String,
    /// Art/type code of the reference, e.g. `"ZU"`.
    pub art: String,
}

#[async_trait]
pub trait OralInquiryScraper: Scraper {
    const FILTER_PATH: &'static str;

    fn oral_inquiry_payload(&self) -> Value;

    async fn post_oral_inquiry_payload(&self) -> Result<Value, ScraperError> {
        self.post_payload(
            self.oral_inquiry_payload(),
            format!(
                "{base_url}{filter}",
                base_url = Self::BASE_URL,
                filter = Self::FILTER_PATH
            ),
        )
        .await
    }

    fn oral_inquiry_paths(&self, rows: Value) -> Result<Vec<String>, ScraperError>;

    fn raw_value_into_oral_inquiry(raw: Value) -> Option<OralInquiry>;

    async fn get_oral_inquiry(&self, path: &str) -> Result<OralInquiry, ScraperError> {
        let raw = self.cache(path).await?;

        Self::raw_value_into_oral_inquiry(raw).ok_or(ScraperError::InvalidProposalInfoError)
    }
}

const GEGENSTAND_PATH_IDX: usize = 14;

impl OralInquiryScraper for AustriaScraper {
    const FILTER_PATH: &'static str = "/Filter/api/filter/data/101?js=eval&showAll=true";

    fn oral_inquiry_payload(&self) -> Value {
        json!({
            "GP": self.scrape_info.gp,
            "NRBR": self.scrape_info.council_type,
            "DOKTYP": ["M"],
            "DATUM_VON": [
                self.scrape_info.date_start,
                self.scrape_info.date_end
            ],
        })
    }

    fn oral_inquiry_paths(&self, rows: Value) -> Result<Vec<String>, ScraperError> {
        self.gegenstand_paths(rows, GEGENSTAND_PATH_IDX)
            .ok_or(ScraperError::ProposalParseError)
    }

    fn raw_value_into_oral_inquiry(raw: Value) -> Option<OralInquiry> {
        let content = raw.get("content")?;

        let inr = content["inr"].as_u64()?;
        let gp = content["gp_code"].as_str()?.to_string();
        let title = raw["meta"]["title"].as_str()?.to_string();

        // The description contains HTML (e.g. <br> tags); strip it.
        let question_text = content["description"]
            .as_str()
            .map(extract_text_from_html)
            .unwrap_or_default();

        // Identify asker and recipient from `names`.
        let names = content["names"].as_array()?;

        let asker_id = names
            .iter()
            .find(|n| n["funktext"].as_str() == Some("Eingebracht von"))
            .and_then(|n| person_url_to_id(n["url"].as_str()?))?;

        let recipient_entry = names
            .iter()
            .find(|n| n["funktext"].as_str() == Some("Eingebracht an"));

        let recipient_id = recipient_entry.and_then(|n| person_url_to_id(n["url"].as_str()?))?;

        let recipient_ministry = recipient_entry
            .and_then(|n| n["ltext"].as_str())
            .map(str::to_string);

        // Parse stages into typed exchanges.
        let stages = content["stages"].as_array().cloned().unwrap_or_default();
        let exchanges = extract_exchanges(&stages);

        // Plenary session ID: first stage that contains an NRSITZ link.
        let plenary_session = stages
            .iter()
            .find_map(|stage| extract_plenary_session_id_from_stage_text(stage["text"].as_str()?));

        // References (e.g. written supplement answers "Schriftliche Ergänzung").
        let references = content["reference"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .flat_map(|r| {
                Some(OralReference {
                    text: r["text"].as_str()?.to_string(),
                    subject: r["subject"].as_str().unwrap_or_default().to_string(),
                    zitation: r["zitation"].as_str().unwrap_or_default().to_string(),
                    url: r["url"].as_str()?.to_string(),
                    art: r["art"].as_str().unwrap_or_default().to_string(),
                })
            })
            .collect();

        let topics = extract_topics(content, "topics").unwrap_or_default();
        let eurovoc_topics = extract_topics(content, "eurovoc").unwrap_or_default();
        let other_keyword_topics = extract_topics(content, "headwords").unwrap_or_default();

        let created_at = content["einlangen"].as_str()?;
        let created_at = NaiveDateTime::parse_from_str(created_at, "%Y-%m-%dT%H:%M:%S")
            .ok()
            .and_then(convert_vienna_datetime_to_utc)?;

        let updated_at = content["update"]
            .as_str()
            .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok())
            .and_then(convert_vienna_datetime_to_utc);

        Some(OralInquiry {
            inr,
            gp,
            title,
            question_text,
            asker_id,
            recipient_id,
            recipient_ministry,
            plenary_session,
            exchanges,
            references,
            topics,
            eurovoc_topics,
            other_keyword_topics,
            created_at,
            updated_at,
        })
    }
}

/// Classify `stages` entries into [`OralExchange`] values.
///
/// Recognised stage text patterns (case-insensitive):
/// - `"mündliche frage … gestellt"` → [`ExchangeKind::MainQuestion`]
/// - `"beantwortung der mündlichen anfrage"` → [`ExchangeKind::MainAnswer`]
/// - `"zusatzfrage … gestellt"` → [`ExchangeKind::FollowUpQuestion`]
/// - `"zusatzfrage schriftlich beantwortet"` → [`ExchangeKind::FollowUpAnswerWritten`]
fn extract_exchanges(stages: &[Value]) -> Vec<OralExchange> {
    let mut exchanges = Vec::new();

    for stage in stages {
        let text = match stage["text"].as_str() {
            Some(t) => t,
            None => continue,
        };
        let lower = text.to_lowercase();

        let kind = if lower.contains("mündliche frage") && lower.contains("gestellt") {
            ExchangeKind::MainQuestion
        } else if lower.contains("beantwortung der mündlichen anfrage") {
            ExchangeKind::MainAnswer
        } else if lower.contains("zusatzfrage schriftlich beantwortet") {
            ExchangeKind::FollowUpAnswerWritten
        } else if lower.contains("zusatzfrage") && lower.contains("gestellt") {
            ExchangeKind::FollowUpQuestion
        } else {
            continue;
        };

        // Extract person_id from the first `/person/` link in the stage text.
        let person_id = extract_person_id_from_stage_text(text);

        // The `fsth` array contains protocol document references.
        let protocol_refs = stage["fsth"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .flat_map(|fsth| {
                Some(ProtocolRef {
                    gp: fsth["gp_code"].as_str()?.to_string(),
                    session_id: fsth["sitzung_id"].as_u64()?,
                    page_from: fsth["fund_von"].as_u64(),
                    page_to: fsth["fund_bis"].as_u64(),
                    url: fsth["url"].as_str()?.to_string(),
                    title: fsth["title"].as_str().unwrap_or_default().to_string(),
                })
            })
            .collect();

        exchanges.push(OralExchange {
            kind,
            person_id,
            protocol_refs,
        });
    }

    exchanges
}

/// Extract the first `/person/{id}` ID from an HTML stage text string.
fn extract_person_id_from_stage_text(text: &str) -> Option<u64> {
    let prefix = "/person/";
    let start = text.find(prefix)? + prefix.len();
    let end = text[start..].find('"').map(|i| start + i)?;
    text[start..end].parse::<u64>().ok()
}

/// Extract the plenary session number from a stage text containing an NRSITZ link.
///
/// Matches patterns like `href="/gegenstand/XXVII/NRSITZ/274"`.
fn extract_plenary_session_id_from_stage_text(text: &str) -> Option<u64> {
    let nrsitz_marker = "NRSITZ/";
    let nrsitz_pos = text.find(nrsitz_marker)?;
    let start = nrsitz_pos + nrsitz_marker.len();
    let end = text[start..].find('"').map(|i| start + i)?;
    text[start..end].parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AustriaScraper;

    #[tokio::test]
    async fn test_get_oral_inquiry_xxvii_384() {
        let scraper = AustriaScraper::default();
        let inquiry = scraper
            .get_oral_inquiry("/gegenstand/XXVII/M/384")
            .await
            .unwrap();

        assert_eq!(inquiry.inr, 384);
        assert_eq!(inquiry.gp, "XXVII");
        assert_eq!(inquiry.asker_id, 83059);
        assert_eq!(inquiry.recipient_id, 2136);
        assert_eq!(
            inquiry.recipient_ministry.as_deref(),
            Some("Bundeskanzleramt")
        );
        assert_eq!(inquiry.plenary_session, Some(274));

        // Main question + main answer + 2 Zusatzfragen + 1 schriftlich beantwortet
        assert!(inquiry.exchanges.len() >= 4);

        // First exchange is the main question
        assert_eq!(inquiry.exchanges[0].kind, ExchangeKind::MainQuestion);
        assert_eq!(inquiry.exchanges[0].person_id, Some(83059));

        // Second exchange is the main answer
        assert_eq!(inquiry.exchanges[1].kind, ExchangeKind::MainAnswer);
        assert_eq!(inquiry.exchanges[1].person_id, Some(2136));

        // Has a written supplement reference
        assert!(!inquiry.references.is_empty());
        assert_eq!(inquiry.references[0].art, "ZU");
    }

    #[tokio::test]
    async fn test_get_oral_inquiry_xxviii_90() {
        let scraper = AustriaScraper::default();
        let inquiry = scraper
            .get_oral_inquiry("/gegenstand/XXVIII/M/90")
            .await
            .unwrap();

        assert_eq!(inquiry.inr, 90);
        assert_eq!(inquiry.gp, "XXVIII");
        assert_eq!(inquiry.asker_id, 5650);
        assert_eq!(inquiry.recipient_id, 2309);
        assert_eq!(inquiry.plenary_session, Some(79));

        // XXVIII uses anchor-based protocol URLs — verify refs are populated
        let main_q = inquiry
            .exchanges
            .iter()
            .find(|e| e.kind == ExchangeKind::MainQuestion)
            .expect("main question exchange");
        assert!(!main_q.protocol_refs.is_empty());
        assert!(main_q.protocol_refs[0].url.contains('#'));
    }

    #[tokio::test]
    async fn test_get_oral_inquiry_xxii_171() {
        let scraper = AustriaScraper::default();
        let inquiry = scraper
            .get_oral_inquiry("/gegenstand/XXII/M/171")
            .await
            .unwrap();

        assert_eq!(inquiry.inr, 171);
        assert_eq!(inquiry.gp, "XXII");
        assert_eq!(inquiry.asker_id, 8241);
        assert_eq!(inquiry.recipient_id, 1546);
        assert_eq!(inquiry.plenary_session, Some(160));

        // XXII uses page-based protocol URLs — verify page_from is set
        let main_q = inquiry
            .exchanges
            .iter()
            .find(|e| e.kind == ExchangeKind::MainQuestion)
            .expect("main question exchange");
        assert!(!main_q.protocol_refs.is_empty());
        assert!(main_q.protocol_refs[0].page_from.is_some());
    }

    #[tokio::test]
    async fn test_list_oral_inquiries_xxviii() {
        let scraper = AustriaScraper::default();
        let rows = scraper.post_oral_inquiry_payload().await.unwrap();
        let paths = scraper.oral_inquiry_paths(rows).unwrap();
        assert!(!paths.is_empty());
        println!("Found {} oral inquiry paths (XXVIII)", paths.len());
    }
}
