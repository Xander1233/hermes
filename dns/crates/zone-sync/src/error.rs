#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("control plane request failed: {0}")]
    Http(String),

    #[error("failed to decode control plane response: {0}")]
    Decode(String),

    #[error("snapshot io error at '{path}': {source}")]
    Io { path: String, source: std::io::Error },

    #[error("invalid zone data: {0}")]
    Zone(#[from] hermes_config::ConfigError),

    #[error("failed to fetch zone '{id}': {reason}")]
    ZoneFetch { id: String, reason: String },

    #[error("invalid apex '{0}'")]
    InvalidApex(String),
}

impl From<reqwest::Error> for SyncError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_decode() {
            SyncError::Decode(error.to_string())
        } else {
            SyncError::Http(error.to_string())
        }
    }
}

impl From<serde_json::Error> for SyncError {
    fn from(error: serde_json::Error) -> Self {
        SyncError::Decode(error.to_string())
    }
}
