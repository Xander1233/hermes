use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use crate::types::Name;
use crate::zone::dnstype::SOA;
use crate::zone::resource_record::{RecordData, RecordType};
use crate::zone::rr_set::RRSet;
use crate::zone::zone_node::{LookUpResult, ZoneNode};

pub mod rr_set;
pub mod resource_record;
pub mod dnstype;
pub mod zone_node;
pub mod alias;
pub mod store;

#[derive(Clone, Default)]
pub struct Zones {
    zones: HashMap<Name, Arc<ZoneNode>>,
}

impl Zones {
    pub fn new() -> Self {
        Zones { zones: HashMap::new() }
    }

    pub fn len(&self) -> usize {
        self.zones.len()
    }

    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }

    pub fn apexes(&self) -> impl Iterator<Item = &Name> {
        self.zones.keys()
    }

    pub fn add_zone(&mut self, apex: Name, soa: SOA, soa_ttl: u32) -> &mut ZoneNode {
        let mut node = ZoneNode::new(apex.clone(), true);
        
        node.add_rr_set(
            u16::from(RecordType::SOA),
            RRSet::single(soa_ttl, RecordData::SOA(soa)),
        );
        self.zones.insert(apex.clone(), Arc::new(node));
        
        Arc::make_mut(self.zones.get_mut(&apex).expect("zone was just inserted"))
    }

    pub fn remove_zone(&mut self, apex: &Name) -> bool {
        self.zones.remove(apex).is_some()
    }

    pub fn adopt_zone(&mut self, other: &mut Zones, apex: &Name) -> bool {
        match other.zones.remove(apex) {
            Some(node) => {
                self.zones.insert(apex.clone(), node);
                
                true
            }
            None => false,
        }
    }

    pub fn has_zone(&self, apex: &Name) -> bool {
        self.zones.contains_key(apex)
    }

    pub fn get_zone(&self, apex: &Name) -> Option<&ZoneNode> {
        self.zones.get(apex).map(|zone| zone.as_ref())
    }

    pub fn get_zone_mut(&mut self, apex: &Name) -> Option<&mut ZoneNode> {
        self.zones.get_mut(apex).map(Arc::make_mut)
    }

    pub fn find_authoritative_zone(&self, qname: &Name) -> Option<&ZoneNode> {
        let mut candidate = Some(qname.clone());
        
        while let Some(name) = candidate {
            if let Some(zone) = self.zones.get(&name) {
                return Some(zone.as_ref());
            }
            
            candidate = name.parent();
        }
        
        None
    }

    pub fn alias_targets(&self) -> HashSet<Name> {
        let mut out = HashSet::new();
        
        for zone in self.zones.values() {
            zone.collect_alias_targets(&mut out);
        }
        
        out
    }

    pub fn resolve_local_addresses(&self, name: &Name) -> Option<(Vec<Ipv4Addr>, Vec<Ipv6Addr>)> {
        let zone = self.find_authoritative_zone(name)?;

        let collect = |qtype: RecordType| -> Vec<RecordData> {
            match zone.query(name, u16::from(qtype)) {
                LookUpResult::Answer(rr_set) => rr_set.records,
                _ => Vec::new(),
            }
        };

        let mut v4 = Vec::new();
        for record in collect(RecordType::A) {
            if let RecordData::A(ip) = record {
                v4.push(ip);
            }
        }
        
        let mut v6 = Vec::new();
        for record in collect(RecordType::AAAA) {
            if let RecordData::AAAA(ip) = record {
                v6.push(ip);
            }
        }

        if v4.is_empty() && v6.is_empty() {
            None
        } else {
            Some((v4, v6))
        }
    }

    pub fn upsert_rrset(&mut self, apex: &Name, owner: Name, rr_set: RRSet) -> bool {
        if !owner.is_in_zone(apex) {
            return false;
        }
        
        let Some(zone) = self.zones.get_mut(apex) else {
            return false;
        };
        
        let rtype = rr_set.rtype;
        Arc::make_mut(zone).upsert_node(&owner).add_rr_set(rtype, rr_set);
        
        true
    }

    pub fn remove_rrset(&mut self, apex: &Name, owner: &Name, rtype: u16) -> bool {
        let depth = apex.label_count();
        let Some(zone) = self.zones.get_mut(apex) else {
            return false;
        };
        
        match Arc::make_mut(zone).find_mut(owner, depth) {
            Some(node) => node.rr_sets.remove(&rtype).is_some(),
            None => false,
        }
    }

    pub fn remove_name(&mut self, apex: &Name, owner: &Name) -> bool {
        if owner.eq(apex) {
            return false;
        }
        
        let depth = apex.label_count();
        
        match self.zones.get_mut(apex) {
            Some(zone) => Arc::make_mut(zone).remove(owner, depth),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::name;
    use crate::zone::zone_node::LookUpResult;
    use std::net::Ipv4Addr;

    fn soa() -> SOA {
        SOA {
            mname: name(&["ns", "example", "com"]),
            rname: name(&["hostmaster", "example", "com"]),
            serial: 1,
            refresh: 2,
            retry: 3,
            expire: 4,
            minimum: 5,
        }
    }

    fn a(ip: [u8; 4]) -> RecordData {
        RecordData::A(Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]))
    }

    #[test]
    fn new_has_no_zones() {
        assert!(!Zones::new().has_zone(&name(&["com"])));
    }

    #[test]
    fn add_zone_seeds_soa() {
        let mut zones = Zones::new();
        zones.add_zone(name(&["example", "com"]), soa(), 3600);
        let zone = zones.get_zone(&name(&["example", "com"])).unwrap();
        assert!(zone.soa().is_some());
    }

    #[test]
    fn add_zone_returns_apex_for_immediate_use() {
        let mut zones = Zones::new();
        let apex = zones.add_zone(name(&["example", "com"]), soa(), 3600);
        apex.add_rr_set(u16::from(RecordType::A), RRSet::single(300, a([1, 2, 3, 4])));
        assert!(zones.get_zone(&name(&["example", "com"])).unwrap().rr_sets.contains_key(&1));
    }

    #[test]
    fn remove_zone_removes_it() {
        let mut zones = Zones::new();
        zones.add_zone(name(&["example", "com"]), soa(), 3600);
        assert!(zones.remove_zone(&name(&["example", "com"])));
        assert!(!zones.has_zone(&name(&["example", "com"])));
        assert!(!zones.remove_zone(&name(&["example", "com"]))); // already gone
    }

    #[test]
    fn find_authoritative_zone_picks_longest_apex() {
        let mut zones = Zones::new();
        zones.add_zone(name(&["example", "com"]), soa(), 3600);
        zones.add_zone(name(&["sub", "example", "com"]), soa(), 3600);

        assert_eq!(
            zones.find_authoritative_zone(&name(&["www", "sub", "example", "com"])).unwrap().name,
            name(&["sub", "example", "com"])
        );
        assert_eq!(
            zones.find_authoritative_zone(&name(&["www", "example", "com"])).unwrap().name,
            name(&["example", "com"])
        );
    }

    #[test]
    fn find_authoritative_zone_none_when_uncovered() {
        let mut zones = Zones::new();
        zones.add_zone(name(&["example", "com"]), soa(), 3600);
        assert!(zones.find_authoritative_zone(&name(&["example", "org"])).is_none());
    }

    #[test]
    fn find_authoritative_zone_supports_multi_label_apex() {
        let mut zones = Zones::new();
        zones.add_zone(name(&["zone", "example", "com"]), soa(), 3600);
        assert_eq!(
            zones.find_authoritative_zone(&name(&["www", "zone", "example", "com"])).unwrap().name,
            name(&["zone", "example", "com"])
        );
    }

    #[test]
    fn upsert_rrset_adds_records_resolvable_by_query() {
        let mut zones = Zones::new();
        zones.add_zone(name(&["example", "com"]), soa(), 3600);
        assert!(zones.upsert_rrset(
            &name(&["example", "com"]),
            name(&["www", "example", "com"]),
            RRSet::single(300, a([1, 2, 3, 4])),
        ));

        let zone = zones.get_zone(&name(&["example", "com"])).unwrap();
        match zone.query(&name(&["www", "example", "com"]), u16::from(RecordType::A)) {
            LookUpResult::Answer(rr) => assert_eq!(rr.records, vec![a([1, 2, 3, 4])]),
            _ => panic!("expected Answer"),
        }
    }

    #[test]
    fn upsert_rrset_rejects_owner_outside_zone() {
        let mut zones = Zones::new();
        zones.add_zone(name(&["example", "com"]), soa(), 3600);
        assert!(!zones.upsert_rrset(
            &name(&["example", "com"]),
            name(&["www", "example", "org"]),
            RRSet::single(300, a([1, 2, 3, 4])),
        ));
    }

    #[test]
    fn remove_rrset_removes_only_that_type() {
        let mut zones = Zones::new();
        zones.add_zone(name(&["example", "com"]), soa(), 3600);
        let apex = name(&["example", "com"]);
        let owner = name(&["www", "example", "com"]);
        zones.upsert_rrset(&apex, owner.clone(), RRSet::single(300, a([1, 2, 3, 4])));

        assert!(zones.remove_rrset(&apex, &owner, u16::from(RecordType::A)));
        assert!(!zones.remove_rrset(&apex, &owner, u16::from(RecordType::A))); // already gone
    }

    #[test]
    fn remove_name_drops_the_name() {
        let mut zones = Zones::new();
        zones.add_zone(name(&["example", "com"]), soa(), 3600);
        let apex = name(&["example", "com"]);
        zones.upsert_rrset(&apex, name(&["www", "example", "com"]), RRSet::single(300, a([1, 2, 3, 4])));

        assert!(zones.remove_name(&apex, &name(&["www", "example", "com"])));
        let zone = zones.get_zone(&apex).unwrap();
        assert!(matches!(
            zone.query(&name(&["www", "example", "com"]), u16::from(RecordType::A)),
            LookUpResult::NXDomain
        ));
    }

    #[test]
    fn remove_name_refuses_to_remove_apex() {
        let mut zones = Zones::new();
        zones.add_zone(name(&["example", "com"]), soa(), 3600);
        assert!(!zones.remove_name(&name(&["example", "com"]), &name(&["example", "com"])));
    }
}
