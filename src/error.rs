use thiserror::Error;

#[derive(Error, Clone, Debug, PartialEq, Eq)]
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
    RequestError,

    #[error("Encounterd invalid delegate information from API")]
    InvalidDelegateInfoError,

    #[error("Encounterd invalid proposal information from API")]
    InvalidProposalInfoError,
}
