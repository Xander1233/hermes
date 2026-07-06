use serde::{Deserialize, Serialize};
use hermes_config::{RecordConfig, SoaConfig, ZoneConfig};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ZoneManifest {
    #[serde(default)]
    pub version: Option<u64>,
    #[serde(default, alias = "zone")]
    pub zones: Vec<ZoneRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneRef {
    pub id: String,
    pub apex: String,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneDocument {
    pub id: String,
    pub apex: String,
    pub version: u64,
    pub soa: SoaConfig,
    #[serde(default, alias = "record")]
    pub records: Vec<RecordConfig>,
}

impl ZoneDocument {
    pub fn to_zone_config(&self) -> ZoneConfig {
        ZoneConfig {
            apex: self.apex.clone(),
            soa: self.soa.clone(),
            records: self.records.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedSnapshot {
    #[serde(default)]
    pub manifest_version: Option<u64>,
    #[serde(default)]
    pub zones: Vec<ZoneDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    UpToDate,
    Applied {
        updated: usize,
        deleted: usize,
        skipped: usize,
        total: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub manifest_version: Option<u64>,
    pub zones: usize,
    pub tracked: usize,
    pub skipped: usize,
    pub last_success_secs_ago: Option<u64>,
    pub last_error: Option<String>,
    pub successful_syncs: u64,
    pub failed_syncs: u64,
}
