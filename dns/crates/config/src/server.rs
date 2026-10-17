use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::error::ConfigError;
use crate::telemetry::TelemetryConfig;
use crate::resolver::ResolverConfig;
use crate::sync::ZoneSyncConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub network: NetworkConfig,
    pub telemetry: TelemetryConfig,
    pub resolver: ResolverConfig,
    pub zone_sync: ZoneSyncConfig,
    pub rate_limit: RateLimitConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub responses_per_second: u32,
    pub all_per_second: u32,
    pub nxdomains_per_second: u32,
    pub errors_per_second: u32,
    pub window_secs: u32,
    pub slip: u32,
    pub ipv4_prefix_len: u8,
    pub ipv6_prefix_len: u8,
    pub table_size: usize,
    pub exempt: Vec<String>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        RateLimitConfig {
            enabled: true,
            responses_per_second: 15,
            all_per_second: 100,
            nxdomains_per_second: 5,
            errors_per_second: 5,
            window_secs: 15,
            slip: 2,
            ipv4_prefix_len: 24,
            ipv6_prefix_len: 56,
            table_size: 8192,
            exempt: vec!["127.0.0.0/8".to_string(), "::1/128".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    pub address: IpAddr,
    pub port: u16,
    pub udp: bool,
    pub tcp: bool,
    pub workers: usize,
    pub tcp_backlog: i32,
    pub tcp_idle_timeout_secs: u64,
    pub udp_sockets: usize,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        NetworkConfig {
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 53,
            udp: true,
            tcp: true,
            workers: 0,
            tcp_backlog: 1024,
            tcp_idle_timeout_secs: 10,
            udp_sockets: 1,
        }
    }
}

impl NetworkConfig {
    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.address, self.port)
    }
}

impl ServerConfig {
    pub fn from_toml_str(contents: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(contents)?)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.display().to_string(),
            source,
        })?;
        
        Self::from_toml_str(&contents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::LogFormat;

    #[test]
    fn defaults_are_sensible() {
        let config = ServerConfig::default();
        assert!(config.network.udp && config.network.tcp);
        assert_eq!(config.network.port, 53);
        assert_eq!(config.telemetry.level, "info");
        assert!(config.telemetry.file_logging);
    }

    #[test]
    fn parses_full_config() {
        let toml = r#"
            [network]
            address = "0.0.0.0"
            port = 5353
            udp = true
            tcp = false
            workers = 4
            tcp_backlog = 512

            [telemetry]
            level = "debug"
            log_format = "json"
            otlp_endpoint = "http://localhost:4317"
            trace_sample_ratio = 0.5
        "#;
        let config = ServerConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.network.socket_addr().to_string(), "0.0.0.0:5353");
        assert!(!config.network.tcp);
        assert_eq!(config.network.workers, 4);
        assert_eq!(config.telemetry.level, "debug");
        assert_eq!(config.telemetry.log_format, LogFormat::Json);
        assert_eq!(config.telemetry.otlp_endpoint.as_deref(), Some("http://localhost:4317"));
        assert_eq!(config.telemetry.trace_sample_ratio, 0.5);
    }

    #[test]
    fn missing_sections_fall_back_to_defaults() {
        // Only the port is overridden; everything else uses defaults.
        let config = ServerConfig::from_toml_str("[network]\nport = 1053\n").unwrap();
        assert_eq!(config.network.port, 1053);
        assert!(config.network.udp);
        assert_eq!(config.telemetry.level, "info");
        assert_eq!(config.telemetry.log_format, LogFormat::Pretty);
    }

    #[test]
    fn rate_limiting_defaults_to_on() {
        // The default must protect, not expose: a config that omits the section entirely is a
        // deployment that never thought about amplification.
        let config = ServerConfig::default();
        assert!(config.rate_limit.enabled);
        assert_eq!(config.rate_limit.ipv4_prefix_len, 24);
        assert!(config.rate_limit.nxdomains_per_second < config.rate_limit.responses_per_second);
    }

    #[test]
    fn parses_the_rate_limit_section() {
        let toml = r#"
            [rate_limit]
            enabled = true
            responses_per_second = 20
            nxdomains_per_second = 3
            slip = 4
            ipv4_prefix_len = 20
            table_size = 4096
            exempt = [ "10.0.0.0/8" ]
        "#;
        let config = ServerConfig::from_toml_str(toml).unwrap();

        assert_eq!(config.rate_limit.responses_per_second, 20);
        assert_eq!(config.rate_limit.nxdomains_per_second, 3);
        assert_eq!(config.rate_limit.slip, 4);
        assert_eq!(config.rate_limit.ipv4_prefix_len, 20);
        assert_eq!(config.rate_limit.table_size, 4096);
        assert_eq!(config.rate_limit.exempt, vec!["10.0.0.0/8".to_string()]);

        // Unlisted keys still fall back to defaults.
        assert_eq!(config.rate_limit.errors_per_second, 5);
    }

    #[test]
    fn rejects_invalid_toml() {
        assert!(ServerConfig::from_toml_str("this is not valid = = toml").is_err());
    }

    #[test]
    fn shipped_server_config_is_valid() {
        // Guards the example config against parser drift. It has to be the .example file:
        // config/server.toml is gitignored local config and so absent from a clean checkout.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/server.example.toml");
        ServerConfig::load(path).expect("shipped server.example.toml must parse");
    }
}
