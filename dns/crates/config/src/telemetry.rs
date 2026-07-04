use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TelemetryConfig {
    pub level: String,
    pub log_format: LogFormat,
    pub logs_dir: PathBuf,
    pub file_logging: bool,
    pub log_file_prefix: String,
    pub otlp_endpoint: Option<String>,
    pub service_name: Option<String>,
    pub trace_sample_ratio: f64,
    pub metric_export_interval_secs: u64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        TelemetryConfig {
            level: "info".to_string(),
            log_format: LogFormat::default(),
            logs_dir: PathBuf::from("logs"),
            file_logging: true,
            log_file_prefix: "hermes".to_string(),
            otlp_endpoint: None,
            service_name: None,
            trace_sample_ratio: 1.0,
            metric_export_interval_secs: 10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LogFormat {
    #[default]
    Pretty,
    Json,
}
