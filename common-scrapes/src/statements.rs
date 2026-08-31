use chrono::{DateTime, Utc};
use serde_derive::{Deserialize, Serialize};

use crate::{Document, Reference};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct StatementAuthor {
    pub name: String,
    pub delegate_id: Option<u64>,
    pub party_code: Option<String>,
    pub institution: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StatementAuthorKind {
    Organisation,
    PrivatePerson,
}

impl StatementAuthorKind {
    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_uppercase().as_str() {
            "I" => Some(Self::Organisation),
            "P" => Some(Self::PrivatePerson),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            StatementAuthorKind::Organisation => "I",
            StatementAuthorKind::PrivatePerson => "P",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Statement {
    pub id: u64,
    pub inr: u64,
    pub ityp: String,
    pub gp: String,
    pub zitation: String,
    pub title: String,
    pub description: String,
    pub statement_text: Option<String>,
    pub documents: Vec<Document>,
    pub authors: Vec<StatementAuthor>,
    pub author_kind: StatementAuthorKind,
    pub references: Vec<Reference>,
    pub primary_reference: Option<Reference>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub topics: Vec<String>,
    pub eurovoc_topics: Vec<String>,
    pub other_keyword_topics: Vec<String>,
    pub supports: u64,
    pub stage: Option<String>,
    pub path: String,
}

impl Statement {
    pub fn organisation(&self) -> bool {
        self.author_kind == StatementAuthorKind::Organisation
    }
}
