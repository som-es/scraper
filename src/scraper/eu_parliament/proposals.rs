use std::collections::HashSet;

use chrono::{Datelike, NaiveDate, Utc};
use common_scrapes::{Document, FileType, Proposal, ProposalType};
use regex::Regex;
use serde_json::Value;

use crate::{error::ScraperError, request::request_text};

use super::{id_to_u64, last_path_segment, EuParliamentScraper};

mod oeil;
mod procedure_list_parser;

pub use procedure_list_parser::ProcedureFeedEntry;

impl EuParliamentScraper {
    pub async fn list_procedures_feed(
        &self,
        start_date: NaiveDate,
    ) -> Result<Vec<ProcedureFeedEntry>, ScraperError> {
        let url = format!(
            "{base}/procedures/feed?timeframe=custom&start-date={start_date}",
            base = Self::BASE_URL,
        );
        let xml = request_text(self.client(), &url).await?;
        Ok(procedure_list_parser::parse_procedure_feed_xml(&xml))
    }

    pub async fn list_procedures_feed_filtered_by_year(
        &self,
        year: u32,
    ) -> Result<Vec<ProcedureFeedEntry>, ScraperError> {
        let start_date = NaiveDate::from_ymd_opt(year as i32, 1, 1).unwrap();
        Ok(self
            .list_procedures_feed(start_date)
            .await?
            .into_iter()
            .filter(|entry| entry.process_id.starts_with(&format!("{year}-")))
            .collect())
    }

    pub async fn list_procedure_ids_by_year(&self, year: u32) -> Result<Vec<String>, ScraperError> {
        let mut ids: HashSet<String> = HashSet::new();
        ids.extend(self.list_oeil_procedure_ids(year).await?);
        Ok(ids.into_iter().collect())
    }

    pub async fn list_procedure_ids(&self) -> Result<Vec<String>, ScraperError> {
        let rows = self.get_all_pages("procedures", "").await?;
        Ok(rows
            .into_iter()
            .flat_map(|row| row["process_id"].as_str().map(ToString::to_string))
            .collect())
    }

    /// Fetches one legislative procedure ("proposal" in AT terms) by its process id.
    pub async fn get_proposal(&self, process_id: &str) -> Result<Proposal, ScraperError> {
        let path = format!("procedures/{process_id}");
        let raw = self.get_one_cached(&path).await?;
        let mut proposal =
            raw_procedure_into_proposal(&raw).ok_or(ScraperError::InvalidProposalInfoError)?;

        let doc_ids = substantive_document_ids(&raw);
        self.fill_documents_and_topics(&doc_ids, &mut proposal)
            .await;

        Ok(proposal)
    }

    /// Fetches each of `doc_ids` via `/documents/{id}` and fills in `proposal.{documents,
    /// topics, eurovoc_topics, other_keyword_topics}` — none of which are available on
    /// the bare procedure resource itself. Each fetched document carries three
    /// classification arrays (deduplicated here across all fetched docs): `is_about`
    /// (EuroVoc topic ids), `isAboutSubjectMatter` (the responsible committee's policy
    /// area, e.g. `"EMPL"`), and `isAboutDirectoryCode` (an EU legal-act classification
    /// code, e.g. `"052030"`).
    async fn fill_documents_and_topics(&self, doc_ids: &[String], proposal: &mut Proposal) {
        for doc_id in doc_ids {
            let path = format!("documents/{}", last_path_segment(doc_id));
            let Ok(raw) = self.get_one_cached(&path).await else {
                continue;
            };
            proposal.documents.extend(extract_documents_from_doc(&raw));
            push_unique_last_segments(&raw["is_about"], &mut proposal.eurovoc_topics);
            push_unique_last_segments(&raw["isAboutSubjectMatter"], &mut proposal.topics);
            push_unique_last_segments(
                &raw["isAboutDirectoryCode"],
                &mut proposal.other_keyword_topics,
            );
        }
    }
}

fn push_unique_last_segments(value: &Value, out: &mut Vec<String>) {
    for entry in value.as_array().cloned().unwrap_or_default() {
        let Some(segment) = entry.as_str().map(last_path_segment) else {
            continue;
        };
        if !out.iter().any(|existing| existing == segment) {
            out.push(segment.to_string());
        }
    }
}

/// Ids of the actual legislative texts belonging to a procedure — the original
/// proposal/draft report, the committee report, and (once voted) the final adopted
/// text — analogous to AT's `documents` on a `Proposal`/`LegislativeInitiative`,
/// including its specially-prioritized "Gesetzestext" (final law text; see
/// `extract_plenar_documents` and its use in `legis_init.rs`).
///
/// `created_a_realization_of` on the procedure resource links directly to these, and is
/// far more precise than trying to derive them from `consists_of` events: those events
/// mostly reference plenary *minutes*/*verbatim reports* of when the procedure was
/// discussed or voted on, not the legislative text being discussed.
fn substantive_document_ids(raw_procedure: &Value) -> Vec<String> {
    let primary: Vec<String> = raw_procedure["created_a_realization_of"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .flat_map(|id| id.as_str().map(ToString::to_string))
        .collect();

    if !primary.is_empty() {
        return primary;
    }

    // Fallback for procedures that don't (yet) have a `created_a_realization_of` link
    // (e.g. very early-stage ones with no report/adopted text yet). This only reaches
    // minutes/verbatim-report documents, which are noisier and not the legislative text
    // itself, so it's capped and only used as a last resort.
    raw_procedure["consists_of"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .flat_map(|event| {
            event["recorded_in_a_realization_of"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .flat_map(|id| id.as_str().map(ToString::to_string))
        .rev()
        .take(3)
        .collect()
}

/// Timeline event types that can plausibly carry a plenary vote. Used by
/// [`plenary_sitting_dates`] to skip fetching a sitting's "VOT" document for dates that
/// were only ever a debate, referral, committee step, etc. — none of which are ever
/// voted on by themselves, so there's nothing a VOT-document lookup could find there.
const VOTE_RELEVANT_ACTIVITY_TYPES: &[&str] = &[
    "def/ep-activities/PLENARY_VOTE",
    "def/ep-activities/PLENARY_VOTE_RESULTS",
    "def/ep-activities/PLENARY_ADOPT_POSITION",
    "def/ep-activities/PLENARY_AMEND_PROPOSAL",
    "def/ep-activities/PLENARY_AMEND",
    "def/ep-activities/PLENARY_DECISION",
];

/// Finds every distinct `(parliamentary term, plenary sitting date)` pair a procedure
/// was actually voted (or vote-adjacently decided) on in plenary, derived from any
/// `PV-`/`CRE-` minutes/verbatim-report document id referenced by a
/// [`VOTE_RELEVANT_ACTIVITY_TYPES`] event (these ids embed both term and date, e.g.
/// `"eli/dl/doc/PV-9-2024-04-25-ITM-007-01"` -> term `"9"`, date `2024-04-25`).
///
/// This intentionally does *not* require specifically `PLENARY_VOTE_RESULTS`: that
/// event is missing from some procedures' `consists_of` even when a real roll-call vote
/// happened on them (confirmed for `2024-0021`) — but it also doesn't treat *every*
/// timeline event as a candidate, since most (`PLENARY_DEBATE`, `REFERRAL`,
/// `COMMITTEE_*`, ...) can never be a vote and would just cost an extra, always-empty
/// "VOT" document fetch in [`EuParliamentScraper::get_votes_for_proposal`].
pub(crate) fn plenary_sitting_dates(raw_procedure: &Value) -> Vec<(String, NaiveDate)> {
    let doc_id_regex = Regex::new(r"^eli/dl/doc/(?:PV|CRE)-(\d+)-(\d{4}-\d{2}-\d{2})").unwrap();
    let mut seen = HashSet::new();

    raw_procedure["consists_of"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter(|event| {
            event["had_activity_type"]
                .as_str()
                .is_some_and(|t| VOTE_RELEVANT_ACTIVITY_TYPES.contains(&t))
        })
        .flat_map(|event| {
            event["recorded_in_a_realization_of"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .flat_map(|id| id.as_str().map(ToString::to_string))
        .flat_map(|id| {
            let captures = doc_id_regex.captures(&id)?;
            let term = captures.get(1)?.as_str().to_string();
            let date = NaiveDate::parse_from_str(captures.get(2)?.as_str(), "%Y-%m-%d").ok()?;
            Some((term, date))
        })
        .filter(|pair| seen.insert(pair.clone()))
        .collect()
}

/// Builds official committee-report reference strings (e.g. `"A9-0173/2024"`) from the
/// procedure's own `created_a_realization_of` document ids (e.g.
/// `"eli/dl/doc/A-9-2024-0173"`).
///
/// Needed because a DOCEO "VOT" document's `<vote><label>` uses *this* format — not the
/// procedure's own `{year}/{inr}({ityp})` reference — for any vote taken on a committee
/// report rather than directly on a Commission proposal (i.e. most legislative votes;
/// `"2024/0021(COD)"`-style procedure references only showed up on `2024-0021`'s
/// first-reading vote on the bare Commission proposal, not on report-based votes like
/// `2024-0003`'s). See [`EuParliamentScraper::get_votes_for_proposal`].
pub(crate) fn report_references(raw_procedure: &Value) -> Vec<String> {
    let doc_id_regex = Regex::new(r"^eli/dl/doc/([A-Z]+)-(\d+)-(\d{4})-(\d+)$").unwrap();

    raw_procedure["created_a_realization_of"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .flat_map(|id| id.as_str().map(ToString::to_string))
        .flat_map(|id| {
            let captures = doc_id_regex.captures(&id)?;
            let prefix = captures.get(1)?.as_str();
            let term = captures.get(2)?.as_str();
            let year = captures.get(3)?.as_str();
            let number = captures.get(4)?.as_str();
            Some(format!("{prefix}{term}-{number}/{year}"))
        })
        .collect()
}

fn raw_procedure_into_proposal(raw: &Value) -> Option<Proposal> {
    let process_id = raw["process_id"].as_str()?.to_string();
    let ityp = last_path_segment(raw["process_type"].as_str()?).to_string();

    let (year, inr_str) = process_id.split_once('-')?;
    let inr = inr_str.trim_start_matches('0').parse::<u64>().unwrap_or(0);

    let title = raw["process_title"]["en"]
        .as_str()
        .or_else(|| raw["label"].as_str())
        .unwrap_or_default()
        .to_string();

    let events = raw["consists_of"].as_array().cloned().unwrap_or_default();
    let participations = raw["had_participation"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    // parliamentary term, roughly analogous to AT's `gp` (e.g. "XXVII"); falls back to
    // the process id's year when no participation/event carries a term reference yet.
    let gp = participations
        .iter()
        .find_map(|p| p["parliamentary_term"].as_str())
        .map(|term| {
            last_path_segment(term)
                .trim_start_matches("ep-")
                .to_string()
        })
        .unwrap_or_else(|| year.to_string());

    let issuer_ids = participations
        .iter()
        .filter(|p| p["participation_role"].as_str() == Some("def/ep-roles/RAPPORTEUR"))
        .flat_map(|p| {
            p["had_participant_person"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .flat_map(|person| id_to_u64(person.as_str()?))
        .collect::<Vec<u64>>();

    let dates: Vec<NaiveDate> = events
        .iter()
        .chain(&participations)
        .flat_map(|e| e["activity_date"].as_str())
        .flat_map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .collect();
    let created_at = dates.iter().min()?.and_hms_opt(0, 0, 0)?.and_utc();
    let updated_at = dates
        .iter()
        .max()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc())
        .filter(|updated_at| *updated_at != created_at);

    Some(Proposal {
        id: process_id,
        ityp: ityp.clone(),
        gp,
        inr,
        title,
        proposal_type: ProposalType::Other(ityp),
        // no free-text summary is exposed on the procedure resource itself
        description: None,
        issuer_ids,
        // EU procedures involve committees/rapporteurs on the issuing side and the
        // Commission/Council as addressees, not individual MEPs; there is no clean
        // per-person analogue to AT's `receiver_ids`
        receiver_ids: vec![],
        // filled in by `get_proposal` from the fetched documents (see
        // `FetchedProposalInfo`), since none of these are available on the bare
        // procedure resource itself
        documents: vec![],
        topics: vec![],
        eurovoc_topics: vec![],
        other_keyword_topics: vec![],
        created_at,
        updated_at,
    })
}

pub(crate) fn extract_documents_from_doc(raw: &Value) -> Vec<Document> {
    let title = raw["title_dcterms"]["en"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // `is_realized_by` has one entry per translation of the document; without this
    // filter we'd emit one `Document` per language (20+ near-duplicates, e.g. a French
    // .docx mislabeled with the English title) for every file type.
    let english_expression = raw["is_realized_by"].as_array().and_then(|expressions| {
        expressions.iter().find(|expression| {
            expression["id"]
                .as_str()
                .is_some_and(|id| id.ends_with("/en"))
        })
    });

    let Some(english_expression) = english_expression else {
        return vec![];
    };

    english_expression["is_embodied_by"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .flat_map(|manifestation| {
            let link_path = manifestation["is_exemplified_by"].as_str()?;
            let link = format!("https://data.europarl.europa.eu/{link_path}");
            let format_uri = manifestation["format"].as_str().unwrap_or_default();
            let ftype = if format_uri.ends_with("/PDF") {
                FileType::DigitalPDF
            } else if format_uri.ends_with("/HTML") {
                FileType::HTML
            } else {
                FileType::Unknown
            };
            Some(Document {
                title: title.clone(),
                link,
                ftype,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[tokio::test]
    async fn test_list_procedure_ids() {
        let scraper = EuParliamentScraper::default();
        let ids = scraper.list_procedure_ids().await.unwrap();
        let mut id_count = HashMap::<&str, u32>::new();
        for id in &ids {
            id_count
                .entry(id.split('-').next().unwrap())
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }
        dbg!(id_count);
        // assert!(!ids.is_empty());
        // assert!(ids.iter().all(|id| id.starts_with("2024-")));
    }

    #[tokio::test]
    async fn test_list_procedure_ids_by_year() {
        let scraper = EuParliamentScraper::default();
        let ids = scraper.list_procedure_ids_by_year(2026).await.unwrap();
        let ids_by_feed = scraper
            .list_procedures_feed_filtered_by_year(2026)
            .await
            .unwrap();
        dbg!(ids.len(), ids_by_feed.len());
        // assert!(!ids.is_empty());
        // assert!(ids.iter().all(|id| id.starts_with("2024-")));
    }

    #[tokio::test]
    async fn test_get_proposal() {
        let scraper = EuParliamentScraper::default();
        let proposal = scraper.get_proposal("2024-0003").await.unwrap();
        assert_eq!(proposal.ityp, "BUD");
        assert_eq!(proposal.inr, 3);
        assert!(proposal.title.to_lowercase().contains("globalisation"));

        // the actual adopted text (final law/decision), committee report, and original
        // draft report should all be present, each as an English PDF/DOCX/XML
        assert!(!proposal.documents.is_empty());
        assert!(proposal
            .documents
            .iter()
            .any(|doc| doc.ftype == common_scrapes::FileType::DigitalPDF
                && doc.link.contains("TA-9-2024-0334")));
        assert!(proposal
            .documents
            .iter()
            .any(|doc| doc.link.contains("A-9-2024-0173")));

        assert!(proposal.eurovoc_topics.contains(&"763".to_string()));
        assert!(proposal.topics.contains(&"EMPL".to_string()));
        assert!(proposal
            .other_keyword_topics
            .contains(&"052030".to_string()));
    }
}

#[cfg(test)]
mod feed_smoke_test {
    use super::*;

    #[tokio::test]
    async fn test_list_procedures_feed_live() {
        let scraper = EuParliamentScraper::default();
        let entries = scraper
            .list_procedures_feed(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap())
            .await
            .unwrap();
        assert!(!entries.is_empty());
        dbg!(&entries.len());
    }
}

#[cfg(test)]
mod all_ids_smoke_test {
    use super::*;

    #[tokio::test]
    async fn test_list_all_procedure_ids_union() {
        let scraper = EuParliamentScraper::default();
        let ids = scraper.list_procedure_ids_by_year(2026).await.unwrap();
        let sorted = ids.windows(2).all(|w| w[0] < w[1]);
        assert!(sorted, "ids must be sorted and unique");
        assert!(
            ids.contains(&"2026-2792".to_string()),
            "known 2026 procedure present"
        );
        assert!(ids.iter().all(|id| id.starts_with("2026-")));
        dbg!(ids.len(), &ids[..3.min(ids.len())]);
    }
}

#[cfg(test)]
mod howtheyvote_comparison {
    use super::*;
    use regex::Regex;
    use std::io::Read;

    const HOWTHEYVOTE_VOTES_URL: &str =
        "https://github.com/HowTheyVote/data/releases/latest/download/votes.csv.gz";
    const TERM_9_START_YEAR: u32 = 2019;
    const MIN_HOWTHEYVOTE_COVERAGE: f64 = 0.98;

    fn base_procedure_id(id: &str) -> String {
        match id.split_once('-') {
            Some((year, number)) => {
                let digits: String = number.chars().take_while(|c| c.is_ascii_digit()).collect();
                format!("{year}-{digits}")
            }
            None => id.to_string(),
        }
    }

    async fn howtheyvote_procedure_ids(scraper: &EuParliamentScraper) -> HashSet<String> {
        let bytes = scraper
            .client()
            .get(HOWTHEYVOTE_VOTES_URL)
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        let mut csv = String::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_string(&mut csv)
            .unwrap();

        let reference = Regex::new(r"(\d{4})/(\d{4})[A-Z]?\([A-Z]+\)").unwrap();
        reference
            .captures_iter(&csv)
            .filter_map(|caps| {
                let year = caps.get(1)?.as_str();
                let number = caps.get(2)?.as_str();
                (year.parse::<u32>().ok()? >= TERM_9_START_YEAR).then(|| format!("{year}-{number}"))
            })
            .collect()
    }

    #[tokio::test]
    async fn scraper_covers_howtheyvote_procedures() {
        let scraper = EuParliamentScraper::default();

        let ours: HashSet<String> = scraper
            .list_procedure_ids_by_year(TERM_9_START_YEAR)
            .await
            .unwrap()
            .iter()
            .map(|id| base_procedure_id(id))
            .collect();
        let howtheyvote: HashSet<String> = howtheyvote_procedure_ids(&scraper)
            .await
            .iter()
            .map(|id| base_procedure_id(id))
            .collect();

        let covered = howtheyvote.iter().filter(|id| ours.contains(*id)).count();
        let coverage = covered as f64 / howtheyvote.len() as f64;

        dbg!(ours.len(), howtheyvote.len(), covered, coverage);

        assert!(
            ours.len() >= howtheyvote.len(),
            "our enumeration ({}) should be a superset of HowTheyVote's voted procedures ({})",
            ours.len(),
            howtheyvote.len(),
        );
        assert!(
            coverage >= MIN_HOWTHEYVOTE_COVERAGE,
            "only {covered}/{} ({coverage:.1}) of HowTheyVote procedures found in our enumeration",
            howtheyvote.len(),
        );
    }
}
