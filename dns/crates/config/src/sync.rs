use std::path::PathBuf;
use std::time::Duration;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ZoneSyncConfig {
    pub enabled: bool,
    pub base_url: String,
    pub auth_token: Option<String>,
    pub poll_interval_secs: u64,
    pub request_timeout_ms: u64,
    pub fetch_concurrency: usize,
    pub snapshot_path: PathBuf,
    pub admin_listen: Option<String>,
    pub admin_token: Option<String>,
}

impl Default for ZoneSyncConfig {
    fn default() -> Self {
        ZoneSyncConfig {
            enabled: false,
            base_url: String::new(),
            auth_token: None,
            poll_interval_secs: 15,
            request_timeout_ms: 5000,
            fetch_concurrency: 16,
            snapshot_path: PathBuf::from("data/zones-snapshot.json"),
            admin_listen: None,
            admin_token: None,
        }
    }
}

impl ZoneSyncConfig {
    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.poll_interval_secs.max(1))
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.request_timeout_ms.max(100))
    }

    pub fn is_active(&self) -> bool {
        self.enabled && !self.base_url.trim().is_empty()
    }
}
