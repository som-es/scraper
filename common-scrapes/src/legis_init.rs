use std::ops::Deref;

use chrono::{DateTime, NaiveDate, Utc};
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Document, Speech};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Party {
    pub name: String,
    pub code: String,
    pub color: String,
    pub fraction: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Vote {
    pub party: Party,
    pub infavor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Opinion {
    Infavor,
    Against,
    Other(String),
    #[default]
    None,
}

impl Opinion {
    pub fn invert(self) -> Self {
        match self {
            Opinion::Infavor => Opinion::Against,
            Opinion::Against => Opinion::Infavor,
            Opinion::Other(o) => Opinion::Other(o),
            Opinion::None => Opinion::None,
        }
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QaType {
    Answer,
    Question,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QaReference {
    pub qa_type: QaType,
    pub reference: Reference,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ParliamentaryQAEntity {
    pub base_info: Proposal,
    pub qa_type: QaType,
    pub references: Vec<QaReference>,
}

impl Deref for ParliamentaryQAEntity {
    type Target = Proposal;

    fn deref(&self) -> &Self::Target {
        &self.base_info
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub ityp: String,
    pub gp: String,
    pub inr: u64,
    pub title: String,
    pub proposal_type: ProposalType,
    pub description: Option<String>,
    pub issuer_ids: Vec<u64>,
    pub receiver_ids: Vec<u64>,
    pub documents: Vec<Document>,
    pub topics: Vec<String>,
    pub eurovoc_topics: Vec<String>,
    pub other_keyword_topics: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AcceptedState {
    Accepted,
    #[default]
    Declined,
    PreDeclined,
}

impl core::fmt::Display for AcceptedState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let res = match self {
            AcceptedState::Accepted => 'a',
            AcceptedState::Declined => 'd',
            AcceptedState::PreDeclined => 'p',
        };
        write!(f, "{res}")
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Serialize, Deserialize)]
pub struct ProContra {
    pub pro: Vec<String>,
    pub contra: Vec<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Stage {
    First,
    Second,
    #[default]
    Third,
    None,
}

impl core::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let res = match self {
            Stage::First => "1",
            Stage::Second => "2",
            Stage::Third => "3",
            Stage::None => "x",
        };

        write!(f, "{res}")
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum VoteType {
    ReversingProposal,
    Proposal,
    OnlyVotedByName,
    AnyNamedVote,
    #[default]
    None,
}

impl VoteType {
    pub fn is_proposal(&self) -> bool {
        matches!(self, VoteType::ReversingProposal | VoteType::Proposal)
    }

    pub fn is_named_vote(&self) -> bool {
        matches!(self, VoteType::OnlyVotedByName | VoteType::AnyNamedVote)
    }
}

impl core::fmt::Display for VoteType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let res = match self {
            VoteType::ReversingProposal => 'p',
            VoteType::Proposal => 'd',
            VoteType::OnlyVotedByName => 'o',
            VoteType::AnyNamedVote => 'n',
            VoteType::None => 'x',
        };
        write!(f, "{res}")
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct VoteStage {
    pub vote_type: VoteType,
    pub stage: Stage,
}

impl core::fmt::Display for VoteStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.vote_type, self.stage)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HiddenVotes {
    pub vote_stage: Option<VoteStage>,
    pub pro_contra_parties: Option<ProContra>,
    pub voted_by_name: bool,
    pub voted_by_name_data: Option<VotedByName>,
    pub was_declined: Option<bool>,
    pub voted_at_date: Option<NaiveDate>,
    pub voted_unanimously: bool,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct VotedByName {
    pub delegates_pro_contra: Option<ProContra>,
    pub vote_sum: usize,
    pub pro_count: usize,
    pub contra_count: usize,
    pub invalid_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Reference {
    pub gp: String,
    pub ityp: String,
    pub inr: u32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComeIntoEffectInfo {
    pub law_come_into_effect_date: Option<NaiveDate>,
    pub law_expires_on_date: Option<NaiveDate>,
    pub by_publication: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelatedDelegate {
    pub text: Option<String>,
    pub delegate_id: u64,
}

#[derive(Copy, PartialEq, Eq, Hash, Debug, Deserialize, Serialize, Clone)]
pub enum Voting {
    Amendment,  // Abänderung
    Resolution, // Entschließung
    Report,
    Law,
}

impl std::fmt::Display for Voting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Voting::Amendment => "Amendment",
            Voting::Resolution => "Resolution",
            Voting::Law => "Law",
            Voting::Report => "Report",
        };
        write!(f, "{value}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LegislativeInitiative {
    pub id: String,
    pub ityp: String,
    pub doktyp: String,
    pub inr: u64,
    pub title: String,
    pub description: String,
    pub emphasis: Option<String>,
    pub gp: String,
    // pub council: String,
    pub votes: Vec<Vote>,
    pub accepted: Option<AcceptedState>,
    pub documents: Vec<Document>,
    pub related_delegates: Vec<RelatedDelegate>,
    pub nr_plenary_activity_date: NaiveDate,
    pub br_plenary_activity_date: Option<NaiveDate>,
    pub vote_date: Option<NaiveDate>,
    pub speeches: Vec<Speech>,
    pub topics: Vec<String>,
    pub eurovoc_topics: Vec<String>,
    pub other_keyword_topics: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub requires_simple_majority: bool,
    pub proposal: Proposal,
    pub hidden_votes_info: HiddenVotes,
    pub br_hidden_votes_info: Option<HiddenVotes>,
    pub br_plenary_session: Option<u64>,
    pub plenary_session: Option<u64>,
    pub references: Option<Vec<Reference>>,
    pub status: Option<u8>,
    pub is_law: bool,
    pub law_accepted: Option<bool>,
    pub is_voteable_on: bool,
    pub is_urgent: bool,
    pub voting: Option<Voting>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Thing {
    pub id: String,
    pub issuer: String,
    pub organization: String,
    pub title: String,
    pub short_title: String,
    pub date: NaiveDate,
    pub thing_type: String,
    pub part: String,
    pub eli: String,
    pub document_url: String,
    pub documents: Vec<Document>,
}
