pub type TelemetryResult<T> = Result<T, TelemetryError>;

#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("panic hook error: {0}")]
    PanicHook(String),
}

impl TelemetryError {
    pub fn panic_hook(msg: impl Into<String>) -> Self {
        TelemetryError::PanicHook(msg.into())
    }
}
