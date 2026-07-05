#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("CNAME loop detected while resolving alias target")]
    Loop,
    #[error("CNAME hop limit exceeded")]
    HopLimit,
    #[error("no usable addresses for alias target")]
    NoAddresses,
    #[error("upstream resolver error: {0}")]
    Upstream(String),
}
