use thiserror::Error;

use crate::corpus::filter::error::FilterError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("search patterns must not be empty")]
    EmptyPatterns,

    #[error("regex build error: {0}")]
    RegexBuild(String),

    #[error("invalid max-count: 0 matches requested")]
    InvalidMaxCount,

    #[error("JSON output is only supported for standard search (not count or file-list modes)")]
    JsonOutputIncompatibleMode,

    #[error("JSON serialization error: {0}")]
    JsonSerialize(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ignore walk error: {0}")]
    Ignore(#[from] ignore::Error),
}

impl From<FilterError> for Error {
    fn from(e: FilterError) -> Self {
        match e {
            FilterError::RegexBuild(s) => Self::RegexBuild(s),
            FilterError::Ignore(e) => Self::Ignore(e),
        }
    }
}
