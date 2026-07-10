use chrono::NaiveDate;
use common_scrapes::AcceptedState;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    caching::{disk_caching, get_disk_cache, get_disk_cache_json},
    error::ScraperError,
    request::request_text,
};

use super::{id_to_u64, last_path_segment, EuParliamentScraper};

/// How one MEP voted in a single roll-call vote (RCV).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EuVoteChoice {
    Favor,
    Against,
    Abstention,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EuDelegateVote {
    pub delegate_id: u64,
    pub choice: EuVoteChoice,
}

/// A roll-call vote (RCV) taken during a plenary sitting.
///
/// This intentionally does *not* reuse AT's `Vote`/`HiddenVotes`/`VotedByName` types:
/// those model aggregated per-party tallies, with individual named votes only
/// reconstructed on a best-effort basis by scraping free-text minutes
/// (`extract_votes_by_name` in `legis_init/votes.rs`). The EU Open Data API instead
/// exposes the outcome of *every* MEP's vote directly and precisely as three id lists,
/// so `votes` here is a flat per-delegate record rather than a per-party one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EuRollCallVote {
    /// e.g. `"MTG-PL-2024-01-15-DEC-163199"`
    pub id: String,
    /// e.g. `"168997"` — the id shared between this vote's JSON representation
    /// (`notation_votingId`) and the DOCEO "VOT"/"RCV" XML documents' `votingId`
    /// attribute; see [`EuParliamentScraper::get_votes_for_proposal`].
    pub voting_id: String,
    pub sitting_date: NaiveDate,
    /// English-language description of what was voted on, when available (e.g. "Request
    /// by the PPE and Verts/ALE groups"). Often a procedural blurb rather than the
    /// legislative text's title; see `related_document_ids` to resolve the latter.
    pub description: Option<String>,
    pub outcome: Option<AcceptedState>,
    pub favor_count: u32,
    pub against_count: u32,
    pub abstention_count: u32,
    pub votes: Vec<EuDelegateVote>,
    /// ids of documents this vote is recorded in (minutes/verbatim report), e.g.
    /// `"eli/dl/doc/PV-9-2024-01-15-RCV-ITM-001"`.
    pub related_document_ids: Vec<String>,
}

impl EuParliamentScraper {
    /// The plenary-sitting id EP uses for a given calendar date, e.g. `2024-01-15` ->
    /// `"MTG-PL-2024-01-15"`. Only dates on which a plenary sitting was actually held
    /// will resolve to real data.
    pub fn sitting_id(date: NaiveDate) -> String {
        format!("MTG-PL-{}", date.format("%Y-%m-%d"))
    }

    /// Fetches every roll-call vote recorded during the plenary sitting `sitting_id`
    /// (see [`Self::sitting_id`]). Non-roll-call decisions (e.g. hand-count agenda
    /// votes) are skipped since they carry no per-MEP breakdown.
    ///
    /// A past sitting's decisions never change, so — unlike [`Self::get_all_pages`] in
    /// general, which is used for genuinely-live lists and deliberately isn't
    /// cached — this result is cached on disk, same as [`Self::get_one_cached`].
    pub async fn get_roll_call_votes(
        &self,
        sitting_id: &str,
    ) -> Result<Vec<EuRollCallVote>, ScraperError> {
        let cache_key = format!("/meetings/{sitting_id}/decisions");
        let rows = match get_disk_cache_json(&cache_key, self.cache_config()).await {
            Some(cached) => cached.as_array().cloned().unwrap_or_default(),
            None => {
                let rows = self
                    .get_all_pages(&format!("meetings/{sitting_id}/decisions"), "")
                    .await?;
                if let Ok(serialized) = serde_json::to_string(&rows) {
                    if let Err(e) = disk_caching(&cache_key, serialized, self.cache_config()).await
                    {
                        log::warn!("could not cache {cache_key}: {e:?}");
                    }
                }
                rows
            }
        };

        Ok(rows.iter().flat_map(raw_decision_into_vote).collect())
    }

    /// Fetches the roll-call vote(s) that decided procedure `process_id`.
    ///
    /// There is no field linking a vote directly to a procedure id in the JSON API —
    /// and the one mechanism that gets close (`PLENARY_VOTE_RESULTS` events in the
    /// procedure's own `consists_of`, whose id reappears in a vote's
    /// `inverse_consists_of`) is unreliable: that event is simply missing from some
    /// procedures' `consists_of` even when a real roll-call vote happened on them
    /// (confirmed for `2024-0021`, "gradual roll-out of Eudamed").
    ///
    /// Instead, this resolves the link via the EP's DOCEO "VOT" documents (plenary
    /// vote list, `PV-{term}-{date}-VOT`), which are also mirrored on the Open Data API
    /// host as plain XML (unlike `www.europarl.europa.eu`, which fronts them with an
    /// AWS WAF bot challenge). Each `<vote>` entry's `<label>` carries an official
    /// reference verbatim, but *which* reference depends on what's being voted on:
    /// - a direct first-reading vote on the bare Commission proposal (e.g.
    ///   `2024-0021`) uses the procedure reference itself, e.g.
    ///   `"...C9-0010/2024 – 2024/0021(COD))"` — matched via `{year}/{inr}({ityp})`
    ///   built from the procedure's own id.
    /// - a vote on a committee report (the common case for most legislative
    ///   procedures, e.g. `2024-0003`) uses the *report's* reference instead, e.g.
    ///   `"Report: Margarida Marques (A9-0173/2024)"` — matched via
    ///   [`super::proposals::report_references`], built from the procedure's own
    ///   `created_a_realization_of` document ids.
    ///
    /// A `<vote>`'s nested `<voting votingId="...">` elements give the id(s) to filter
    /// [`Self::get_roll_call_votes`]'s result by.
    ///
    /// Candidate sitting dates come from [`super::proposals::plenary_sitting_dates`],
    /// which doesn't require `PLENARY_VOTE_RESULTS` either. A procedure can have more
    /// than one matching vote (e.g. separate first- and second-reading votes), and this
    /// returns an empty `Vec` for procedures that were never put to a recorded
    /// roll-call vote (adopted by hand vote, withdrawn, still pending, ...) or whose
    /// vote used some other reference format not covered by the two above.
    pub async fn get_votes_for_proposal(
        &self,
        process_id: &str,
    ) -> Result<Vec<EuRollCallVote>, ScraperError> {
        let raw_procedure = self
            .get_one_cached(&format!("procedures/{process_id}"))
            .await?;

        let ityp = last_path_segment(raw_procedure["process_type"].as_str().unwrap_or_default());
        let (year, inr_str) = process_id.split_once('-').unwrap_or((process_id, ""));

        let mut references = vec![format!("{year}/{inr_str}({ityp})")];
        references.extend(super::proposals::report_references(&raw_procedure));

        let sitting_dates = super::proposals::plenary_sitting_dates(&raw_procedure);

        let mut votes = Vec::new();
        for (term, date) in sitting_dates {
            let Ok(vot_entries) = self.get_vot_entries(&term, date).await else {
                continue;
            };

            let voting_ids: Vec<&str> = vot_entries
                .iter()
                .filter(|entry| references.iter().any(|r| entry.label.contains(r)))
                .flat_map(|entry| entry.voting_ids.iter().map(String::as_str))
                .collect();

            if voting_ids.is_empty() {
                continue;
            }

            let sitting_votes = self.get_roll_call_votes(&Self::sitting_id(date)).await?;
            votes.extend(
                sitting_votes
                    .into_iter()
                    .filter(|vote| voting_ids.contains(&vote.voting_id.as_str())),
            );
        }
        Ok(votes)
    }

    /// Fetches and parses the DOCEO "VOT" document for a plenary sitting — a compact
    /// per-sitting index from official procedure/report reference to `votingId`(s).
    /// Distinct from the much larger "RCV" document, which carries the actual per-MEP
    /// breakdown; that's never parsed here since `/meetings/{sitting}/decisions`
    /// already gives the same breakdown more directly as JSON.
    async fn get_vot_entries(
        &self,
        term: &str,
        date: NaiveDate,
    ) -> Result<Vec<VotEntry>, ScraperError> {
        let doc_id = format!("PV-{term}-{}-VOT", date.format("%Y-%m-%d"));
        let cache_key = format!("/eu-parliament/vot-xml/{doc_id}");

        let xml = match get_disk_cache(&cache_key, self.cache_config()).await {
            Some(cached) => cached,
            None => {
                let raw_doc = self.get_one_cached(&format!("documents/{doc_id}")).await?;
                let link_path =
                    english_xml_manifestation(&raw_doc).ok_or(ScraperError::ParseError)?;
                let url = format!("https://data.europarl.europa.eu/{link_path}");
                let xml = request_text(self.client(), url).await?;
                if let Err(e) = disk_caching(&cache_key, &xml, self.cache_config()).await {
                    log::warn!("could not cache {cache_key}: {e:?}");
                }
                xml
            }
        };

        Ok(parse_vot_xml(&xml))
    }
}

/// Finds the English-language XML manifestation link (`is_exemplified_by`) on a
/// document resource fetched via `/documents/{id}`.
fn english_xml_manifestation(raw_doc: &Value) -> Option<String> {
    raw_doc["is_realized_by"]
        .as_array()?
        .iter()
        .find(|expression| {
            expression["id"]
                .as_str()
                .is_some_and(|id| id.ends_with("/en"))
        })?
        .get("is_embodied_by")?
        .as_array()?
        .iter()
        .find(|manifestation| {
            manifestation["format"]
                .as_str()
                .is_some_and(|format| format.ends_with("/XML"))
        })?
        .get("is_exemplified_by")?
        .as_str()
        .map(ToString::to_string)
}

/// One `<vote>` entry from a DOCEO "VOT" document.
struct VotEntry {
    /// e.g. `"Proposal for a regulation (COM(2024)0043 - C9-0010/2024 – 2024/0021(COD))"`
    label: String,
    /// the nested `<voting votingId="...">` id(s); a `<vote>` covers e.g. both an
    /// amendment vote and the final-passage vote for the same procedure
    voting_ids: Vec<String>,
}

/// Parses a DOCEO "VOT" document's `<votes><vote>...</vote>...</votes>` list.
///
/// Schema (from a real fetched `PV-9-2024-04-25-VOT_en.xml`):
/// ```xml
/// <vote listIndex="0" dlvId="946508" indexSched="160" type="REPORT">
///   <title>Gradual roll-out of Eudamed...</title>
///   <label>Proposal for a regulation (COM(2024)0043 - C9-0010/2024 – 2024/0021(COD))</label>
///   <votings>
///     <voting type="AMENDMENT" votingId="168997" result="ADOPTED" resultType="ROLL_CALL" ...>
///       <title>Commission proposal</title>
///       <label status="rcv">C9-0010/2024 – Commission proposal</label>
///       ...
///     </voting>
///   </votings>
/// </vote>
/// ```
/// Note `<voting>` has its own nested `<label>` — only the outer `<vote>`'s `<label>`
/// (captured while `in_voting` is false) carries the procedure reference used for
/// matching in [`EuParliamentScraper::get_votes_for_proposal`].
fn parse_vot_xml(xml: &str) -> Vec<VotEntry> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut entries = Vec::new();
    let mut in_vote = false;
    let mut in_voting = false;
    let mut in_vote_label = false;
    let mut current_label = String::new();
    let mut current_voting_ids = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"vote" => {
                    in_vote = true;
                    current_label.clear();
                    current_voting_ids.clear();
                }
                b"label" if in_vote && !in_voting => {
                    in_vote_label = true;
                }
                b"voting" if in_vote => {
                    in_voting = true;
                    let voting_id = e
                        .attributes()
                        .flatten()
                        .find(|attr| attr.key.as_ref() == b"votingId")
                        .and_then(|attr| attr.decode_and_unescape_value(reader.decoder()).ok());
                    if let Some(voting_id) = voting_id {
                        current_voting_ids.push(voting_id.to_string());
                    }
                }
                _ => {}
            },
            Ok(Event::Text(e)) if in_vote_label => {
                current_label.push_str(&e.unescape().unwrap_or_default());
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"label" if in_vote_label => in_vote_label = false,
                b"voting" => in_voting = false,
                b"vote" => {
                    in_vote = false;
                    if !current_voting_ids.is_empty() {
                        entries.push(VotEntry {
                            label: std::mem::take(&mut current_label),
                            voting_ids: std::mem::take(&mut current_voting_ids),
                        });
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    entries
}

fn raw_decision_into_vote(raw: &Value) -> Option<EuRollCallVote> {
    if raw["decision_method"].as_str() != Some("def/ep-decision-methods/VOTE_ELECTRONIC_ROLLCALL") {
        return None;
    }
    let id = raw["activity_id"].as_str()?.to_string();
    let voting_id = raw["notation_votingId"].as_str()?.to_string();
    let sitting_date =
        NaiveDate::parse_from_str(raw["activity_date"].as_str()?, "%Y-%m-%d").ok()?;

    let description = raw["referenceText"]["en"]
        .as_str()
        .or_else(|| raw["activity_label"]["en"].as_str())
        .map(ToString::to_string);

    let outcome = match raw["decision_outcome"].as_str() {
        Some(o) if o.ends_with("ADOPTED") => Some(AcceptedState::Accepted),
        Some(o) if o.ends_with("REJECTED") => Some(AcceptedState::Declined),
        _ => None,
    };

    let mut votes = Vec::new();
    collect_votes(&raw["had_voter_favor"], EuVoteChoice::Favor, &mut votes);
    collect_votes(&raw["had_voter_against"], EuVoteChoice::Against, &mut votes);
    collect_votes(
        &raw["had_voter_abstention"],
        EuVoteChoice::Abstention,
        &mut votes,
    );

    let related_document_ids = raw["recorded_in_a_realization_of"]
        .as_array()
        .map(|docs| {
            docs.iter()
                .flat_map(|d| d.as_str())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();

    Some(EuRollCallVote {
        id,
        voting_id,
        sitting_date,
        description,
        outcome,
        favor_count: raw["number_of_votes_favor"].as_u64().unwrap_or_default() as u32,
        against_count: raw["number_of_votes_against"].as_u64().unwrap_or_default() as u32,
        abstention_count: raw["number_of_votes_abstention"]
            .as_u64()
            .unwrap_or_default() as u32,
        votes,
        related_document_ids,
    })
}

fn collect_votes(raw_ids: &Value, choice: EuVoteChoice, out: &mut Vec<EuDelegateVote>) {
    let Some(ids) = raw_ids.as_array() else {
        return;
    };
    out.extend(ids.iter().flat_map(|id| {
        let delegate_id = id_to_u64(id.as_str()?)?;
        Some(EuDelegateVote {
            delegate_id,
            choice,
        })
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_vot_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<file><votes>
<vote listIndex="0" dlvId="946508" indexSched="160" type="REPORT">
<title>Gradual roll-out of Eudamed</title>
<label>Proposal for a regulation (COM(2024)0043 - C9-0010/2024 &#8211; 2024/0021(COD))</label>
<votings>
<voting type="AMENDMENT" votingId="168997" listIndex="0" result="ADOPTED" resultType="ROLL_CALL">
<title>Commission proposal</title>
<label status="rcv">C9-0010/2024 &#8211; Commission proposal</label>
</voting>
</votings>
</vote>
<vote listIndex="1" dlvId="946514" indexSched="168" type="RESOLUTION">
<label>Motions for resolutions: RC-B9-0227/2024</label>
<votings>
<voting type="TITLE_BLOCK" votingId="169094" listIndex="0"></voting>
</votings>
</vote>
</votes></file>"#;

        let entries = parse_vot_xml(xml);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].label.contains("2024/0021(COD)"));
        assert_eq!(entries[0].voting_ids, vec!["168997".to_string()]);
        assert_eq!(entries[1].voting_ids, vec!["169094".to_string()]);
    }

    #[tokio::test]
    async fn test_get_roll_call_votes() {
        let scraper = EuParliamentScraper::default();
        let sitting_id = EuParliamentScraper::sitting_id(
            NaiveDate::parse_from_str("2024-01-15", "%Y-%m-%d").unwrap(),
        );
        assert_eq!(sitting_id, "MTG-PL-2024-01-15");

        let votes = scraper.get_roll_call_votes(&sitting_id).await.unwrap();
        assert!(!votes.is_empty());

        let first = &votes[0];
        assert_eq!(
            first.favor_count as usize,
            first
                .votes
                .iter()
                .filter(|v| v.choice == EuVoteChoice::Favor)
                .count()
        );
        assert!(first.outcome.is_some());
    }

    #[tokio::test]
    async fn test_get_votes_for_proposal() {
        let scraper = EuParliamentScraper::default();

        // 2024-0003: EGF mobilization for displaced workers, adopted 2024-04-24 by
        // roll-call vote 547 favor / 33 against / 49 abstention (see docs/eu-parliament-scraper.md)
        let votes = scraper.get_votes_for_proposal("2024-0003").await.unwrap();

        assert_eq!(votes.len(), 1);
        let vote = &votes[0];
        assert_eq!(
            vote.sitting_date,
            NaiveDate::from_ymd_opt(2024, 4, 24).unwrap()
        );
        assert_eq!(vote.favor_count, 547);
        assert_eq!(vote.against_count, 33);
        assert_eq!(vote.abstention_count, 49);
        assert_eq!(vote.outcome, Some(AcceptedState::Accepted));
    }

    #[tokio::test]
    async fn test_get_votes_for_proposal_missing_plenary_vote_results_event() {
        let scraper = EuParliamentScraper::default();

        // 2024-0021 ("gradual roll-out of Eudamed"): the procedure that exposed the
        // bug in the old `inverse_consists_of`-only matching — its own `consists_of`
        // has no `PLENARY_VOTE_RESULTS` event, only bare `PLENARY_VOTE` ones, yet it
        // really was decided by roll-call vote 168997 on 2024-04-25 (511/1/21;
        // confirmed via howtheyvote.eu and the DOCEO VOT/RCV XML directly).
        let votes = scraper.get_votes_for_proposal("2024-0021").await.unwrap();

        assert_eq!(votes.len(), 1);
        let vote = &votes[0];
        assert_eq!(vote.voting_id, "168997");
        assert_eq!(
            vote.sitting_date,
            NaiveDate::from_ymd_opt(2024, 4, 25).unwrap()
        );
        assert_eq!(vote.favor_count, 511);
        assert_eq!(vote.against_count, 1);
        assert_eq!(vote.abstention_count, 21);
        assert_eq!(vote.outcome, Some(AcceptedState::Accepted));
    }
}
