use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::Instant;
use arc_swap::ArcSwap;
use crate::types::Name;
use crate::zone::resource_record::{RecordData, RecordType};
use crate::zone::rr_set::RRSet;

pub const ALIAS_MIN_TTL: u32 = 30;
pub const ALIAS_MAX_TTL: u32 = 3600;

#[derive(Debug, Clone)]
pub struct AliasEntry {
    pub v4: Vec<Ipv4Addr>,
    pub v6: Vec<Ipv6Addr>,
    pub ttl: u32,
    pub last_success: Option<Instant>,
    pub resolved: bool,
}

impl AliasEntry {
    pub fn resolved(v4: Vec<Ipv4Addr>, v6: Vec<Ipv6Addr>, ttl: u32) -> Self {
        AliasEntry {
            v4,
            v6,
            ttl: ttl.clamp(ALIAS_MIN_TTL, ALIAS_MAX_TTL),
            last_success: Some(Instant::now()),
            resolved: true,
        }
    }
}

pub struct AliasCache {
    inner: ArcSwap<HashMap<Name, AliasEntry>>,
}

impl AliasCache {
    pub fn new() -> Self {
        AliasCache { inner: ArcSwap::from_pointee(HashMap::new()) }
    }

    pub fn from_map(map: HashMap<Name, AliasEntry>) -> Self {
        AliasCache { inner: ArcSwap::from_pointee(map) }
    }

    pub fn get(&self, target: &Name) -> Option<AliasEntry> {
        self.inner.load().get(target).cloned()
    }

    pub fn store(&self, map: HashMap<Name, AliasEntry>) {
        self.inner.store(Arc::new(map));
    }

    pub fn upsert(&self, target: Name, entry: AliasEntry) {
        let mut map = (**self.inner.load()).clone();
        map.insert(target, entry);

        self.inner.store(Arc::new(map));
    }

    pub fn remove(&self, target: &Name) {
        let mut map = (**self.inner.load()).clone();

        if map.remove(target).is_some() {
            self.inner.store(Arc::new(map));
        }
    }

    pub fn len(&self) -> usize {
        self.inner.load().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.load().is_empty()
    }
}

impl Default for AliasCache {
    fn default() -> Self {
        Self::new()
    }
}

pub fn emitted_ttl(record_ttl: u32, entry_ttl: u32) -> u32 {
    record_ttl.min(entry_ttl).max(ALIAS_MIN_TTL)
}

#[derive(Debug, PartialEq, Eq)]
pub enum AliasOutcome {
    Records(RRSet),
    NoData,
    ServFail,
}

pub fn synthesize(entry: Option<&AliasEntry>, qtype: u16, record_ttl: u32) -> AliasOutcome {
    let entry = match entry {
        Some(entry) if entry.resolved => entry,
        _ => return AliasOutcome::ServFail,
    };

    let ttl = emitted_ttl(record_ttl, entry.ttl);

    if qtype == u16::from(RecordType::A) {
        if entry.v4.is_empty() {
            return AliasOutcome::NoData;
        }

        let records = entry.v4.iter().map(|ip| RecordData::A(*ip)).collect();
        AliasOutcome::Records(RRSet::new(u16::from(RecordType::A), ttl, records))
    } else if qtype == u16::from(RecordType::AAAA) {
        if entry.v6.is_empty() {
            return AliasOutcome::NoData;
        }

        let records = entry.v6.iter().map(|ip| RecordData::AAAA(*ip)).collect();
        AliasOutcome::Records(RRSet::new(u16::from(RecordType::AAAA), ttl, records))
    } else {
        AliasOutcome::NoData
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::name;

    fn v4(a: [u8; 4]) -> Ipv4Addr {
        Ipv4Addr::new(a[0], a[1], a[2], a[3])
    }

    fn a_type() -> u16 {
        u16::from(RecordType::A)
    }
    fn aaaa_type() -> u16 {
        u16::from(RecordType::AAAA)
    }

    #[test]
    fn emitted_ttl_is_min_capped_and_floored() {
        assert_eq!(emitted_ttl(300, 100), 100); // entry ttl caps
        assert_eq!(emitted_ttl(100, 300), 100); // record ttl caps
        assert_eq!(emitted_ttl(10, 10), 30); // floored to 30
        assert_eq!(emitted_ttl(3600, 50), 50);
    }

    #[test]
    fn resolved_entry_clamps_ttl() {
        assert_eq!(AliasEntry::resolved(vec![], vec![], 5).ttl, ALIAS_MIN_TTL);
        assert_eq!(AliasEntry::resolved(vec![], vec![], 99999).ttl, ALIAS_MAX_TTL);
        assert_eq!(AliasEntry::resolved(vec![], vec![], 120).ttl, 120);
    }

    #[test]
    fn synthesize_cold_is_servfail() {
        assert_eq!(synthesize(None, a_type(), 300), AliasOutcome::ServFail);
        // Present but never resolved is also SERVFAIL.
        let cold = AliasEntry { v4: vec![], v6: vec![], ttl: 60, last_success: None, resolved: false };
        assert_eq!(synthesize(Some(&cold), a_type(), 300), AliasOutcome::ServFail);
    }

    #[test]
    fn synthesize_v4_query_with_v4_data_is_records() {
        let entry = AliasEntry::resolved(vec![v4([1, 2, 3, 4])], vec![], 60);
        match synthesize(Some(&entry), a_type(), 300) {
            AliasOutcome::Records(rr) => {
                assert_eq!(rr.rtype, a_type());
                assert_eq!(rr.ttl, 60);
                assert_eq!(rr.records, vec![RecordData::A(v4([1, 2, 3, 4]))]);
            }
            other => panic!("expected Records, got {other:?}"),
        }
    }

    #[test]
    fn synthesize_wrong_family_is_nodata_not_servfail() {
        // v4 present, AAAA queried -> NODATA (resolved, just no v6).
        let entry = AliasEntry::resolved(vec![v4([1, 2, 3, 4])], vec![], 60);
        assert_eq!(synthesize(Some(&entry), aaaa_type(), 300), AliasOutcome::NoData);
    }

    #[test]
    fn synthesize_respects_record_ttl_cap() {
        let entry = AliasEntry::resolved(vec![v4([1, 2, 3, 4])], vec![], 600);
        match synthesize(Some(&entry), a_type(), 45) {
            AliasOutcome::Records(rr) => assert_eq!(rr.ttl, 45),
            other => panic!("expected Records, got {other:?}"),
        }
    }

    #[test]
    fn cache_upsert_get_remove_roundtrip() {
        let cache = AliasCache::new();
        let target = name(&["host", "example", "net"]);
        assert!(cache.get(&target).is_none());

        cache.upsert(target.clone(), AliasEntry::resolved(vec![v4([9, 9, 9, 9])], vec![], 60));
        assert_eq!(cache.get(&target).unwrap().v4, vec![v4([9, 9, 9, 9])]);
        assert_eq!(cache.len(), 1);

        cache.remove(&target);
        assert!(cache.get(&target).is_none());
        assert!(cache.is_empty());
    }
}
