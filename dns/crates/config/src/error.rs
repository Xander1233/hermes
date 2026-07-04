use dns_types::types::NameError;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file '{path}': {source}")]
    Io { path: String, source: std::io::Error },

    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("invalid domain name '{name}': {source}")]
    InvalidName { name: String, source: NameError },

    #[error("invalid record for '{owner}': {reason}")]
    InvalidRecord { owner: String, reason: String },

    #[error("record '{owner}' is not inside zone '{apex}'")]
    RecordOutsideZone { owner: String, apex: String },

    #[error("unsupported record type '{0}'")]
    UnsupportedType(String),
}
