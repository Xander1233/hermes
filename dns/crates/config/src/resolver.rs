use std::net::IpAddr;
use std::time::Duration;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ResolverConfig {
    pub upstreams: Vec<String>,
    pub timeout_ms: u64,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        ResolverConfig {
            upstreams: vec!["1.1.1.1".to_string(), "1.0.0.1".to_string(), "8.8.8.8".to_string(), "8.8.4.4".to_string()],
            timeout_ms: 3000,
        }
    }
}

impl ResolverConfig {
    pub fn upstream_ips(&self) -> Vec<IpAddr> {
        self.upstreams.iter().filter_map(|s| s.trim().parse().ok()).collect()
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}
