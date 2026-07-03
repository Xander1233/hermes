use std::sync::Arc;
use arc_swap::ArcSwap;
use crate::zone::Zones;

pub struct ZoneStore {
    inner: ArcSwap<Zones>,
}

impl ZoneStore {
    pub fn new(zones: Zones) -> Self {
        ZoneStore { inner: ArcSwap::from_pointee(zones) }
    }

    pub fn empty() -> Self {
        ZoneStore::new(Zones::new())
    }

    pub fn load(&self) -> Arc<Zones> {
        self.inner.load_full()
    }

    pub fn store(&self, zones: Zones) {
        self.inner.store(Arc::new(zones));
    }
    
    pub fn zone_count(&self) -> usize {
        self.inner.load().len()
    }
}

impl Default for ZoneStore {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::name;
    use crate::zone::dnstype::SOA;

    fn soa() -> SOA {
        SOA {
            mname: name(&["ns", "example", "com"]),
            rname: name(&["hostmaster", "example", "com"]),
            serial: 1, refresh: 2, retry: 3, expire: 4, minimum: 5,
        }
    }

    #[test]
    fn swaps_snapshots_atomically() {
        let store = ZoneStore::empty();
        assert_eq!(store.zone_count(), 0);

        let mut zones = Zones::new();
        zones.add_zone(name(&["example", "com"]), soa(), 3600);
        store.store(zones);
        assert_eq!(store.zone_count(), 1);

        // A snapshot taken before the next swap keeps serving the old view.
        let before = store.load();

        let mut next = Zones::new();
        next.add_zone(name(&["example", "com"]), soa(), 3600);
        next.add_zone(name(&["another", "org"]), soa(), 3600);
        store.store(next);

        assert_eq!(before.len(), 1, "held snapshot must be unaffected by the swap");
        assert_eq!(store.zone_count(), 2);
    }

    #[test]
    fn cloning_a_snapshot_is_copy_on_write() {
        let mut zones = Zones::new();
        zones.add_zone(name(&["example", "com"]), soa(), 3600);

        // Cloning shares the zone Arcs; mutating the clone must not affect the original.
        let mut clone = zones.clone();
        clone.remove_zone(&name(&["example", "com"]));

        assert!(zones.get_zone(&name(&["example", "com"])).is_some());
        assert!(clone.get_zone(&name(&["example", "com"])).is_none());
    }
}
