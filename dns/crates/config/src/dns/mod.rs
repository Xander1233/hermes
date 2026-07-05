use std::path::Path;
use serde::{Deserialize, Serialize};
use dns_types::zone::Zones;
use crate::error::ConfigError;

mod build;

pub use build::{apply_zone, build_zones};

/// The default TTL applied to an SOA or record that does not specify one, in seconds.
const DEFAULT_TTL_SECS: u32 = 3600;

fn default_ttl() -> u32 {
    DEFAULT_TTL_SECS
}

/// The initial DNS data served by the server: a set of authoritative zones.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DnsConfig {
    #[serde(default, rename = "zone", alias = "zones")]
    pub zones: Vec<ZoneConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneConfig {
    /// Zone apex (origin), e.g. "example.com" or "zone.example.com".
    pub apex: String,
    pub soa: SoaConfig,
    #[serde(default, rename = "record", alias = "records")]
    pub records: Vec<RecordConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoaConfig {
    pub mname: String,
    pub rname: String,
    pub serial: u32,
    pub refresh: i32,
    pub retry: i32,
    pub expire: i32,
    pub minimum: u32,
    #[serde(default = "default_ttl")]
    pub ttl: u32,
}

/// A single resource record. `data` carries the primary value (address, target name, or
/// text); type-specific numeric fields are supplied alongside where needed (MX, SRV, CAA).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub rtype: String,
    #[serde(default = "default_ttl")]
    pub ttl: u32,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub preference: Option<u16>,
    #[serde(default)]
    pub priority: Option<u16>,
    #[serde(default)]
    pub weight: Option<u16>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub flags: Option<u8>,
    #[serde(default)]
    pub tag: Option<String>,
}

impl DnsConfig {
    /// Parses a `DnsConfig` from a TOML string.
    pub fn from_toml_str(contents: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(contents)?)
    }

    /// Loads a `DnsConfig` from a TOML file on disk.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_toml_str(&contents)
    }

    /// Builds an in-memory [`Zones`] tree from this configuration.
    pub fn build_zones(&self) -> Result<Zones, ConfigError> {
        build_zones(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dns_types::types::Name;
    use dns_types::zone::resource_record::RecordType;
    use dns_types::zone::zone_node::LookUpResult;

    const SAMPLE: &str = r#"
        [[zone]]
        apex = "example.com"

        [zone.soa]
        mname = "ns1.example.com"
        rname = "hostmaster.example.com"
        serial = 2024010101
        refresh = 3600
        retry = 600
        expire = 604800
        minimum = 60
        ttl = 3600

        [[zone.record]]
        name = "example.com"
        type = "A"
        ttl = 300
        data = "93.184.216.34"

        [[zone.record]]
        name = "example.com"
        type = "A"
        ttl = 300
        data = "93.184.216.35"

        [[zone.record]]
        name = "www.example.com"
        type = "CNAME"
        data = "example.com"

        [[zone.record]]
        name = "example.com"
        type = "MX"
        preference = 10
        data = "mail.example.com"
    "#;

    fn atype() -> u16 {
        u16::from(RecordType::A)
    }

    #[test]
    fn parses_and_builds_zone() {
        let config = DnsConfig::from_toml_str(SAMPLE).unwrap();
        let zones = config.build_zones().unwrap();

        let apex = Name::from_ascii("example.com").unwrap();
        let zone = zones.get_zone(&apex).unwrap();
        assert!(zone.soa().is_some());

        // The two apex A records collapse into one RRSet with two records.
        match zone.query(&apex, atype()) {
            LookUpResult::Answer(rr) => assert_eq!(rr.records.len(), 2),
            _ => panic!("expected Answer"),
        }
    }

    #[test]
    fn resolves_cname_via_built_zone() {
        let zones = DnsConfig::from_toml_str(SAMPLE).unwrap().build_zones().unwrap();
        let apex = Name::from_ascii("example.com").unwrap();
        let zone = zones.get_zone(&apex).unwrap();
        match zone.query(&Name::from_ascii("www.example.com").unwrap(), atype()) {
            LookUpResult::CName { target, .. } => assert_eq!(target, apex),
            _ => panic!("expected CNAME"),
        }
    }

    #[test]
    fn rejects_record_outside_zone() {
        let toml = r#"
            [[zone]]
            apex = "example.com"
            [zone.soa]
            mname = "ns1.example.com"
            rname = "hostmaster.example.com"
            serial = 1
            refresh = 1
            retry = 1
            expire = 1
            minimum = 1
            [[zone.record]]
            name = "www.other.org"
            type = "A"
            data = "1.2.3.4"
        "#;
        let config = DnsConfig::from_toml_str(toml).unwrap();
        assert!(matches!(config.build_zones(), Err(ConfigError::RecordOutsideZone { .. })));
    }

    #[test]
    fn rejects_bad_ipv4() {
        let toml = r#"
            [[zone]]
            apex = "example.com"
            [zone.soa]
            mname = "ns1.example.com"
            rname = "hostmaster.example.com"
            serial = 1
            refresh = 1
            retry = 1
            expire = 1
            minimum = 1
            [[zone.record]]
            name = "example.com"
            type = "A"
            data = "not-an-ip"
        "#;
        let config = DnsConfig::from_toml_str(toml).unwrap();
        assert!(matches!(config.build_zones(), Err(ConfigError::InvalidRecord { .. })));
    }

    #[test]
    fn rejects_unsupported_type() {
        let toml = r#"
            [[zone]]
            apex = "example.com"
            [zone.soa]
            mname = "ns1.example.com"
            rname = "hostmaster.example.com"
            serial = 1
            refresh = 1
            retry = 1
            expire = 1
            minimum = 1
            [[zone.record]]
            name = "example.com"
            type = "WEIRD"
            data = "x"
        "#;
        let config = DnsConfig::from_toml_str(toml).unwrap();
        assert!(matches!(config.build_zones(), Err(ConfigError::UnsupportedType(_))));
    }

    #[test]
    fn empty_config_builds_no_zones() {
        let zones = DnsConfig::default().build_zones().unwrap();
        assert!(zones.find_authoritative_zone(&Name::from_ascii("example.com").unwrap()).is_none());
    }

    #[test]
    fn shipped_zones_config_parses_and_builds() {
        // Guards the example config/zones.toml against parser/record drift.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/zones.toml");
        let config = DnsConfig::load(path).expect("shipped zones.toml must parse");
        config.build_zones().expect("shipped zones.toml must build");
    }
}
