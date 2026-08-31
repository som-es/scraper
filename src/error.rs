use std::{sync::Arc, time::Duration};

use reqwest::StatusCode;
use thiserror::Error;

use crate::eu_parliament::votes::ParseError;

#[derive(Error, Clone, Debug)]
pub enum ScraperError {
    /// 401
    #[error("Unauthorized")]
    Unauthorized,

    /// 403
    #[error("Forbidden")]
    Forbidden,

    /// 404
    #[error("Not Found")]
    NotFound,

    /// 500
    #[error("Internal Server Error")]
    InternalServerError,

    /// serde deserialize error
    #[error("Deserialize Error")]
    DeserializeError,

    /// request error
    #[error("Http Request Error")]
    RequestError(Arc<reqwest::Error>),
    #[error("Status")]
    StatusError(StatusCode),

    #[error("Encounterd invalid delegate information from API")]
    InvalidDelegateInfoError,

    #[error("Encounterd invalid proposal information from API")]
    InvalidProposalInfoError,

    #[error("Encountered invalid statement information from API")]
    InvalidStatementInfoError,

    #[error("Could not parse statement paths from API")]
    StatementPathsParseError,

    #[error("Too many requests, retry after")]
    TooManyRequests(Option<Duration>),

    #[error("Could not parse proposal information from API")]
    ProposalParseError,

    #[error("Could not parse plenar gegenstand")]
    PlenarGegenstandParseError,

    #[error("Could not parse plenar gegenstand paths from API")]
    PlenarGegenstandPathsParseError,

    #[error("Could not parse call to order gegenstand")]
    CallTorOrderGegenstandParseError,

    #[error("Could not parse legislative initiative information from API")]
    LegisInitParseError,

    #[error("Could not parse enactment")]
    NoEnactmentError,

    #[error("No vote found")]
    NoVoteFound,

    #[error("Parse Error")]
    ParseError,

    #[error("Parse Error")]
    ParseErrorVerbose(ParseError),
    #[error("Other")]
    Other(String),
}

impl PartialEq for ScraperError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::StatusError(l0), Self::StatusError(r0)) => l0 == r0,
            (Self::ParseErrorVerbose(l0), Self::ParseErrorVerbose(r0)) => l0 == r0,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}
