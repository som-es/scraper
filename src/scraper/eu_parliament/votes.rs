use chrono::NaiveDate;
use common_scrapes::{AcceptedState, Reference};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use regex::Regex;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    caching::{disk_caching, get_disk_cache, get_disk_cache_json},
    error::ScraperError,
    eu_parliament::votes::{
        roll_call_parser::parse_roll_call_votes_xml,
        vot_parsers::{parse_vot_xml, VotEntry, VotingRecord},
    },
    request::request_text,
};

use super::{id_to_u64, last_path_segment, EuParliamentScraper};

mod roll_call_parser;
mod vot_parsers;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Xml(String),
    Number(String),
    MissingAttribute(&'static str),
    NoResult,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Xml(m) => write!(f, "xml error: {m}"),
            ParseError::Number(m) => write!(f, "invalid number: {m}"),
            ParseError::MissingAttribute(a) => write!(f, "missing required attribute: {a}"),
            ParseError::NoResult => write!(f, "no RollCallVote.Result element found"),
        }
    }
}

impl std::error::Error for ParseError {}

/// How one MEP voted in a single roll-call vote (RCV).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EuVoteChoice {
    For,
    Against,
    Abstention,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EuDelegateVote {
    pub delegate_id: u64,
    pub choice: EuVoteChoice,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EuRollCallVote {
    pub voting_id: Option<u64>,
    pub description: Option<String>,
    pub favor_count: u32,
    pub against_count: u32,
    pub abstention_count: u32,
    pub votes: Vec<EuDelegateVote>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Vote {
    pub roll_call_vote: Option<EuRollCallVote>,
    pub meta: Option<VotingRecord>,
    pub sitting_date: NaiveDate,
    pub is_main: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Votes {
    pub votes: Vec<Vote>,
    pub references: Vec<Reference>,
}

pub fn reference_from_report(reference: &str) -> Option<Reference> {
    let regex = Regex::new(r"^([A-Z]+)(\d+)-(\d+)/(\d{4})$").unwrap();
    let captures = regex.captures(reference)?;
    Some(Reference {
        gp: captures.get(2)?.as_str().to_string(),
        ityp: captures.get(1)?.as_str().to_string(),
        inr: captures.get(3)?.as_str().parse::<u32>().ok()?
            + captures.get(4)?.as_str().parse::<u32>().ok()? * 10000,
    })
}

const MAIN_VOTE_INDICATORS: [&str; 42] = [
    "draft council decision",
    "projet de decision du conseil",
    "projet de reglement du conseil",
    "projet du conseil",
    "projet de recommendation",
    "projet commun",
    "motion for resolution",
    "motion for resolution (as a whole)",
    "motion for resolution (text as a whole)",
    "motion for a resolution",
    "motion for a resolution (as a whole)",
    "motion for a resolution (text as a whole)",
    "proposition de rejet",
    "proposition de rejet de la position du conseil",
    "proposal for rejection",
    "proposition de resolution",
    "proposition de resolution (ensemble du texte)",
    "proposition de la resolution",
    "proposition de la resolution (ensemble du texte)",
    "proposition de recommandation",
    "resolution",
    "proposals for decision",
    "proposition de decision",
    "propositions de decision",
    "proposal for a decision",
    "proposal for a decision (as a whole)",
    "proposition de decision (ensemble du texte)",
    "commission proposal",
    "proposition de la commission",
    "proposition de commission",
    "proposition de la commission et amendement",
    "proposition de la commission et amendements",
    "proposition de la commission au conseil",
    "provisional agreement",
    "accord provisoire",
    "accord provisoire (proposition de la commission - non modifiee)",
    "final vote",
    "decision d'engager des negociations interinstitutionnelles",
    "projet de decision du conseil europeen",
    "projet de decision",
    "demande de decision d'urgence",
    "request for an urgent decision",
];

impl EuParliamentScraper {
    const VOTE_BASE_URL: &str = "https://www.europarl.europa.eu/doceo/document/";

    /// The plenary-sitting id EP uses for a given calendar date, e.g. `2024-01-15` ->
    /// `"MTG-PL-2024-01-15"`. Only dates on which a plenary sitting was actually held
    /// will resolve to real data.
    pub fn sitting_id(date: NaiveDate) -> String {
        format!("MTG-PL-{}", date.format("%Y-%m-%d"))
    }

    pub async fn get_votes_for_proposal(&self, process_id: &str) -> Result<Votes, ScraperError> {
        let raw_procedure = self
            .get_one_cached(&format!("procedures/{process_id}"))
            .await?;

        let ityp = last_path_segment(raw_procedure["process_type"].as_str().unwrap_or_default());
        let (year, inr_str) = process_id.split_once('-').unwrap_or((process_id, ""));

        let mut references = vec![format!("{year}/{inr_str}({ityp})")];
        let report_references = super::proposals::report_references(&raw_procedure);

        let parsed_references = report_references
            .iter()
            .flat_map(|reference| reference_from_report(reference))
            .collect::<Vec<_>>();

        references.extend(report_references);
        dbg!(&references);
        let sitting_dates = super::proposals::plenary_sitting_dates(&raw_procedure);

        let mut votes = Vec::new();
        for (term, date) in sitting_dates {
            let vot_entries = match self.get_vot_entries(&term, date).await {
                Ok(vot_entries) => vot_entries,
                Err(e) => {
                    log::error!("cannot parse vot entries because of: {e:?}");
                    continue;
                }
            };

            let voting_records = vot_entries
                .into_iter()
                .filter(|entry| references.iter().any(|r| entry.label.contains(r)))
                .flat_map(|entry| entry.votings.into_iter().map(|voting| voting))
                .collect::<Vec<_>>();

            let roll_call_votes = self
                .get_roll_call_entries(&term, date)
                .await
                .unwrap_or_default();

            if voting_records.is_empty() {
                votes.extend(
                    roll_call_votes
                        .into_iter()
                        .filter(|entry| {
                            references.iter().any(|r| {
                                entry
                                    .description
                                    .as_ref()
                                    .unwrap_or(&String::new())
                                    .contains(r)
                            })
                        })
                        .map(|roll_call_vote| Vote {
                            is_main: roll_call_vote.description.as_ref().map(|description| {
                                description.split(" - ").any(|part| {
                                    let part = deunicode::deunicode(&part.to_lowercase());
                                    for indicator in MAIN_VOTE_INDICATORS {
                                        if part.contains(indicator) {
                                            return true;
                                        }
                                    }
                                    MAIN_VOTE_INDICATORS.contains(&part.as_str())
                                })
                            }),
                            roll_call_vote: Some(roll_call_vote),
                            meta: None,
                            sitting_date: date,
                        }),
                );
            } else {
                votes.extend(voting_records.into_iter().map(|voting_record| {
                    Vote {
                        roll_call_vote: roll_call_votes
                            .iter()
                            .find(|roll_call_vote| {
                                roll_call_vote.voting_id == Some(voting_record.voting_id)
                            })
                            .cloned(),
                        sitting_date: date,
                        is_main: voting_record.title.as_ref().map(|title| {
                            MAIN_VOTE_INDICATORS.contains(&title.to_lowercase().as_str())
                        }),
                        meta: Some(voting_record),
                    }
                }));
            }
        }
        Ok(Votes {
            votes,
            references: parsed_references,
        })
    }

    async fn get_xml_data(
        &self,
        term: &str,
        date: NaiveDate,
        vote_file: VoteFile,
    ) -> Result<String, ScraperError> {
        let doc_id = format!(
            "PV-{term}-{}-{}",
            date.format("%Y-%m-%d"),
            vote_file.as_str()
        );
        let cache_key = format!("/eu-parliament/{}-xml/{doc_id}", vote_file.as_str());

        let xml = match get_disk_cache(&cache_key, self.cache_config()).await {
            Some(cached) => cached,
            None => {
                // let url = format!("{}{doc_id}.xml", Self::VOTE_BASE_URL);
                let raw_doc = self.get_one_cached(&format!("documents/{doc_id}")).await?;
                let link_path =
                    english_xml_manifestation(&raw_doc).ok_or(ScraperError::ParseError)?;
                let url = format!("https://data.europarl.europa.eu/{link_path}");
                let xml = request_text(self.client(), &url).await?;
                if let Err(e) = disk_caching(&cache_key, &xml, self.cache_config()).await {
                    log::warn!("could not cache {cache_key}: {e:?}");
                }
                xml
            }
        };

        Ok(xml)
    }

    async fn get_vot_entries(
        &self,
        term: &str,
        date: NaiveDate,
    ) -> Result<Vec<VotEntry>, ScraperError> {
        let xml = self.get_xml_data(term, date, VoteFile::Vote).await?;
        Ok(parse_vot_xml(&xml))
    }

    async fn get_roll_call_entries(
        &self,
        term: &str,
        date: NaiveDate,
    ) -> Result<Vec<EuRollCallVote>, ScraperError> {
        let xml = self.get_xml_data(term, date, VoteFile::RollCall).await?;
        parse_roll_call_votes_xml(&xml).map_err(|e| ScraperError::ParseErrorVerbose(e))
    }
}

fn english_xml_manifestation(raw_doc: &Value) -> Option<String> {
    raw_doc["is_realized_by"]
        .as_array()?
        .iter()
        .find(|expression| {
            expression["id"]
                .as_str()
                .is_some_and(|id| id.ends_with("/en") || id.ends_with("/fr"))
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
enum VoteFile {
    Vote,
    RollCall,
}

impl VoteFile {
    pub fn as_str(&self) -> &str {
        match self {
            VoteFile::Vote => "VOT",
            VoteFile::RollCall => "RCV",
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::eu_parliament::votes::vot_parsers::VoteResult;

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
<voting type="TITLE_BLOCK" votingId="169094" listIndex="0" result="ADOPTED" resultType="ROLL_CALL"></voting>
</votings>
</vote>
</votes></file>"#;

        let entries = parse_vot_xml(xml);
        dbg!(&entries);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].label.contains("2024/0021(COD)"));
        assert_eq!(entries[0].votings[0].voting_id, 168997);
        assert_eq!(entries[1].votings[0].voting_id, 169094);
    }

    #[tokio::test]
    async fn test_get_roll_call_votes() {
        let scraper = EuParliamentScraper::default();
        let sitting_id = EuParliamentScraper::sitting_id(
            NaiveDate::parse_from_str("2024-01-15", "%Y-%m-%d").unwrap(),
        );
        assert_eq!(sitting_id, "MTG-PL-2024-01-15");

        // let votes = scraper.get_roll_call_votes(&sitting_id).await.unwrap();
        let votes: Vec<EuRollCallVote> = vec![];
        assert!(!votes.is_empty());

        let first = &votes[0];
        assert_eq!(
            first.favor_count as usize,
            first
                .votes
                .iter()
                .filter(|v| v.choice == EuVoteChoice::For)
                .count()
        );
        // assert!(first.outcome.is_some());
    }

    #[tokio::test]
    async fn test_get_votes_for_proposal_exact() {
        let _ = simple_logger::init_with_level(log::Level::Info);
        let scraper = EuParliamentScraper::default();

        // 2024-0003: EGF mobilization for displaced workers, adopted 2024-04-24 by
        // roll-call vote 547 favor / 33 against / 49 abstention (see docs/eu-parliament-scraper.md)
        let votes = scraper.get_votes_for_proposal("2024-0003").await.unwrap();

        dbg!(&votes);

        assert_eq!(votes.votes.len(), 1);
        let vote = &votes.votes[0];
        assert_eq!(
            vote.sitting_date,
            NaiveDate::from_ymd_opt(2024, 4, 24).unwrap()
        );
        let meta = vote.meta.as_ref().unwrap();
        assert_eq!(meta.favor_count, Some(547));
        assert_eq!(meta.against_count, Some(33));
        assert_eq!(meta.abstention_count, Some(49));
        assert_eq!(meta.result, VoteResult::Adopted);
    }

    #[tokio::test]
    async fn test_get_votes_for_proposal_missing_plenary_vote_results_event() {
        let scraper = EuParliamentScraper::default();

        let votes = scraper.get_votes_for_proposal("2024-0021").await.unwrap();

        assert_eq!(votes.votes.len(), 2);
        let vote = &votes.votes[1];
        let meta = vote.meta.as_ref().unwrap();
        assert!(vote.is_main.unwrap());
        assert_eq!(meta.voting_id, 168997);
        assert_eq!(
            vote.sitting_date,
            NaiveDate::from_ymd_opt(2024, 4, 25).unwrap()
        );
        assert_eq!(meta.favor_count, Some(511));
        assert_eq!(meta.against_count, Some(1));
        assert_eq!(meta.abstention_count, Some(21));
        assert_eq!(meta.result, VoteResult::Adopted);
    }

    #[tokio::test]
    async fn test_get_votes_for_2024_2006() {
        let scraper = EuParliamentScraper::default();

        let votes = scraper.get_votes_for_proposal("2024-2006").await.unwrap();

        assert!(votes.votes.iter().any(|vote| vote.is_main == Some(true)));
        dbg!(votes);
        // assert_eq!(votes.len(), 1);
    }
    // 2026-2589
    #[tokio::test]
    async fn test_get_votes_for_2026_2589() {
        let scraper = EuParliamentScraper::default();

        let votes = scraper.get_votes_for_proposal("2026-2589").await.unwrap();
        assert_eq!(votes.votes.len(), 0);
    }

    #[tokio::test]
    async fn test_get_votes_for_2025_2145() {
        let scraper = EuParliamentScraper::default();

        let votes = scraper.get_votes_for_proposal("2025-2145").await.unwrap();
        assert!(!votes.votes.is_empty())
    }

    #[tokio::test]
    async fn test_all_votes() {
        let scraper = EuParliamentScraper::default();
        let mut prods_with_votes = 0;
        for year in [2019, 2020, 2021, 2022, 2023, 2024, 2025, 2026]
            .iter()
            .rev()
        {
            dbg!(year);
            let prods = scraper
                .list_procedures_feed_filtered_by_year(*year)
                .await
                .unwrap();
            dbg!(prods.len());
            for procedure in prods {
                match scraper.get_votes_for_proposal(&procedure.process_id).await {
                    Err(e) => {
                        println!("failed on procedure: {procedure:?} because of {e:?}")
                    }
                    Ok(e) => {
                        if e.votes.is_empty() {
                            println!("procedure has no votes {procedure:?}")
                        } else {
                            // println!("procedure has votes {procedure:?}");
                            prods_with_votes += 1;
                        }
                    }
                }
            }

            println!("parsed prods with votes (current total): {prods_with_votes}");
        }
        println!("parsed prods with votes (total): {prods_with_votes}");
    }
}
