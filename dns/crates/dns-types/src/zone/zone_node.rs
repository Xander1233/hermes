use std::collections::{HashMap, HashSet};
use crate::types::Name;
use crate::zone::resource_record::{RecordData, RecordType};
use crate::zone::rr_set::RRSet;

#[derive(Clone)]
pub struct ZoneNode {
    pub name: Name,
    pub children: Box<Vec<ZoneNode>>,
    pub rr_sets: HashMap<u16, RRSet>,
    pub is_root: bool,
}

impl ZoneNode {
    pub fn new(name: Name, is_root: bool) -> Self {
        ZoneNode {
            name,
            children: Box::new(Vec::new()),
            rr_sets: HashMap::new(),
            is_root
        }
    }

    pub fn add_child(&mut self, child: ZoneNode) {
        self.children.push(child);
    }

    pub fn add_rr_set(&mut self, rtype: u16, rr_set: RRSet) {
        self.rr_sets.insert(rtype, rr_set);
    }

    pub fn remove(&mut self, name: &Name, depth: usize) -> bool {
        let child_depth = depth + 1;

        let suffix_index = match name.offsets().len().checked_sub(child_depth + 1) {
            Some(i) => i,
            None => return false,
        };
        let suffix = name.suffix_from(suffix_index);

        let Some(pos) = self.children.iter().position(|child| child.name.eq(&suffix)) else {
            return false;
        };

        if suffix_index == 0 {
            self.children.remove(pos);
            true
        } else {
            self.children[pos].remove(name, child_depth)
        }
    }

    pub fn find(&self, name: &Name, depth: usize) -> Option<&ZoneNode> {
        let idx = name.offsets().len().checked_sub(depth + 1)?;

        if !name.suffix_from(idx).eq(&self.name) {
            return None;
        }

        if idx == 0 {
            return Some(self);
        }

        for child in self.children.iter() {
            if let Some(found) = child.find(name, depth + 1) {
                return Some(found);
            }
        }

        None
    }

    pub fn find_mut(&mut self, name: &Name, depth: usize) -> Option<&mut ZoneNode> {
        let idx = name.offsets().len().checked_sub(depth + 1)?;

        if !name.suffix_from(idx).eq(&self.name) {
            return None;
        }

        if idx == 0 {
            return Some(self);
        }

        for child in self.children.iter_mut() {
            if let Some(found) = child.find_mut(name, depth + 1) {
                return Some(found);
            }
        }

        None
    }

    pub fn child(&self, name: &Name) -> Option<&ZoneNode> {
        self.children.iter().find(|child| child.name.eq(name))
    }

    pub fn child_wildcard(&self) -> Option<&ZoneNode> {
        self.children.iter().find(|child| child.name.is_wildcard())
    }

    pub fn soa(&self) -> Option<&RRSet> {
        self.rr_sets.get(&u16::from(RecordType::SOA))
    }

    pub fn collect_alias_targets(&self, out: &mut HashSet<Name>) {
        if let Some(alias) = self.rr_sets.get(&u16::from(RecordType::Alias)) && let Some(RecordData::Alias { target }) = alias.records.first() {
            out.insert(target.clone());
        }

        for child in self.children.iter() {
            child.collect_alias_targets(out);
        }
    }

    pub fn upsert_node(&mut self, owner: &Name) -> &mut ZoneNode {
        if owner.eq(&self.name) {
            return self;
        }

        let next_label_count = self.name.label_count() + 1;
        let child_name = owner.suffix_from(owner.label_count() - next_label_count);

        let index = match self.children.iter().position(|child| child.name.eq(&child_name)) {
            Some(index) => index,
            None => {
                self.children.push(ZoneNode::new(child_name, false));
                self.children.len() - 1
            }
        };

        self.children[index].upsert_node(owner)
    }

    pub fn query(&self, qname: &Name, qtype: u16) -> LookUpResult {
        self.walk(qname, qtype, true).0
    }

    pub fn query_classified(&self, qname: &Name, qtype: u16) -> (LookUpResult, bool) {
        self.walk(qname, qtype, true)
    }

    fn walk(&self, qname: &Name, qtype: u16, is_apex: bool) -> (LookUpResult, bool) {
        if !is_apex && let Some(ns) = self.rr_sets.get(&u16::from(RecordType::NS)) {
            return (
                LookUpResult::Referral {
                    owner: self.name.clone(),
                    ns: ns.clone(),
                    glue: Vec::new(),
                },
                false,
            );
        }

        if qname.eq(&self.name) {
            return (self.answer(qtype), false);
        }

        let next_label_count = self.name.label_count() + 1;
        let child_name = qname.suffix_from(qname.label_count() - next_label_count);

        match self.child(&child_name) {
            Some(child) => child.walk(qname, qtype, false),
            None => match self.child_wildcard() {
                Some(wildcard) => (wildcard.answer(qtype), true),
                None => (LookUpResult::NXDomain, false),
            },
        }
    }

    fn answer(&self, qtype: u16) -> LookUpResult {
        if qtype == u16::from(RecordType::Alias) {
            return LookUpResult::NoData;
        }

        if let Some(cname) = self.rr_sets.get(&u16::from(RecordType::CNAME)) {
            let RecordData::CNAME(target) = cname.records[0].clone() else {
                unreachable!("CNAME RRSet must hold CNAME record data");
            };

            return LookUpResult::CName { record: cname.clone(), target };
        }

        if let Some(rr_set) = self.rr_sets.get(&qtype) {
            return LookUpResult::Answer(rr_set.clone());
        }

        if (qtype == u16::from(RecordType::A) || qtype == u16::from(RecordType::AAAA)) && let Some(alias) = self.rr_sets.get(&u16::from(RecordType::Alias)) {
            let RecordData::Alias { target } = alias.records[0].clone() else {
                unreachable!("ALIAS RRSet must hold Alias record data");
            };

            return LookUpResult::Alias { target, ttl: alias.ttl };
        }

        LookUpResult::NoData
    }
}

pub enum LookUpResult {
    Answer(RRSet),
    CName { record: RRSet, target: Name },
    Referral { owner: Name, ns: RRSet, glue: Vec<RecordData> },
    Alias { target: Name, ttl: u32 },
    NoData,
    NXDomain
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::name;
    use crate::zone::dnstype::SOA;
    use std::net::Ipv4Addr;

    fn rtype(t: RecordType) -> u16 {
        u16::from(t)
    }

    fn a(ip: [u8; 4]) -> RecordData {
        RecordData::A(Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]))
    }

    fn soa_record() -> RecordData {
        RecordData::SOA(SOA {
            mname: name(&["ns", "example", "com"]),
            rname: name(&["hostmaster", "example", "com"]),
            serial: 1,
            refresh: 2,
            retry: 3,
            expire: 4,
            minimum: 5,
        })
    }

    /// Zone rooted at `example.com` with an apex A, a delegated subzone, a CNAME,
    /// a wildcard, and a deep name.
    fn zone() -> ZoneNode {
        let mut apex = ZoneNode::new(name(&["example", "com"]), true);
        apex.add_rr_set(rtype(RecordType::SOA), RRSet::single(3600, soa_record()));
        apex.add_rr_set(rtype(RecordType::A), RRSet::single(300, a([93, 184, 216, 34])));

        apex.upsert_node(&name(&["www", "example", "com"]))
            .add_rr_set(rtype(RecordType::A), RRSet::single(300, a([1, 2, 3, 4])));
        apex.upsert_node(&name(&["deep", "sub", "www", "example", "com"]))
            .add_rr_set(rtype(RecordType::A), RRSet::single(300, a([5, 6, 7, 8])));
        apex.upsert_node(&name(&["alias", "example", "com"]))
            .add_rr_set(rtype(RecordType::CNAME), RRSet::single(300, RecordData::CNAME(name(&["www", "example", "com"]))));
        apex.upsert_node(&name(&["*", "wild", "example", "com"]))
            .add_rr_set(rtype(RecordType::A), RRSet::single(300, a([9, 9, 9, 9])));
        apex.upsert_node(&name(&["sub", "example", "com"]))
            .add_rr_set(rtype(RecordType::NS), RRSet::single(300, RecordData::NS(name(&["ns", "sub", "example", "com"]))));
        apex
    }

    #[test]
    fn new_initialises_empty_node() {
        let node = ZoneNode::new(name(&["example", "com"]), true);
        assert_eq!(node.name, name(&["example", "com"]));
        assert!(node.children.is_empty());
        assert!(node.rr_sets.is_empty());
        assert!(node.is_root);
    }

    #[test]
    fn add_child_appends() {
        let mut node = ZoneNode::new(name(&["example", "com"]), true);
        node.add_child(ZoneNode::new(name(&["www", "example", "com"]), false));
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].name, name(&["www", "example", "com"]));
    }

    #[test]
    fn add_rr_set_inserts_by_type() {
        let mut node = ZoneNode::new(name(&["example", "com"]), true);
        node.add_rr_set(1, RRSet::new(1, 300, vec![]));
        assert_eq!(node.rr_sets[&1].ttl, 300);
    }

    #[test]
    fn upsert_node_creates_intermediate_path() {
        let mut apex = ZoneNode::new(name(&["example", "com"]), true);
        apex.upsert_node(&name(&["a", "b", "example", "com"]))
            .add_rr_set(1, RRSet::single(300, a([1, 1, 1, 1])));

        let depth = name(&["example", "com"]).label_count();
        // Both the leaf and the intermediate node exist.
        assert!(apex.find(&name(&["a", "b", "example", "com"]), depth).is_some());
        assert!(apex.find(&name(&["b", "example", "com"]), depth).is_some());
    }

    #[test]
    fn upsert_node_returns_existing_without_duplicating() {
        let mut apex = ZoneNode::new(name(&["example", "com"]), true);
        apex.upsert_node(&name(&["www", "example", "com"]));
        apex.upsert_node(&name(&["www", "example", "com"]));
        assert_eq!(apex.children.len(), 1);
    }

    #[test]
    fn soa_returns_apex_rrset() {
        assert_eq!(zone().soa().unwrap().records, vec![soa_record()]);
    }

    #[test]
    fn query_apex_answer() {
        match zone().query(&name(&["example", "com"]), rtype(RecordType::A)) {
            LookUpResult::Answer(rr) => assert_eq!(rr.records, vec![a([93, 184, 216, 34])]),
            _ => panic!("expected Answer"),
        }
    }

    #[test]
    fn query_child_answer() {
        match zone().query(&name(&["www", "example", "com"]), rtype(RecordType::A)) {
            LookUpResult::Answer(rr) => assert_eq!(rr.records, vec![a([1, 2, 3, 4])]),
            _ => panic!("expected Answer"),
        }
    }

    #[test]
    fn query_deep_child_answer() {
        match zone().query(&name(&["deep", "sub", "www", "example", "com"]), rtype(RecordType::A)) {
            LookUpResult::Answer(rr) => assert_eq!(rr.records, vec![a([5, 6, 7, 8])]),
            _ => panic!("expected Answer"),
        }
    }

    #[test]
    fn query_existing_name_wrong_type_is_nodata() {
        assert!(matches!(
            zone().query(&name(&["www", "example", "com"]), rtype(RecordType::MX)),
            LookUpResult::NoData
        ));
    }

    #[test]
    fn query_absent_name_is_nxdomain() {
        assert!(matches!(
            zone().query(&name(&["nope", "example", "com"]), rtype(RecordType::A)),
            LookUpResult::NXDomain
        ));
    }

    #[test]
    fn query_cname_returns_target() {
        match zone().query(&name(&["alias", "example", "com"]), rtype(RecordType::A)) {
            LookUpResult::CName { target, .. } => assert_eq!(target, name(&["www", "example", "com"])),
            _ => panic!("expected CName"),
        }
    }

    #[test]
    fn query_wildcard_is_synthesized() {
        match zone().query(&name(&["anything", "wild", "example", "com"]), rtype(RecordType::A)) {
            LookUpResult::Answer(rr) => assert_eq!(rr.records, vec![a([9, 9, 9, 9])]),
            _ => panic!("expected wildcard Answer"),
        }
    }

    #[test]
    fn query_below_delegation_is_referral() {
        match zone().query(&name(&["host", "sub", "example", "com"]), rtype(RecordType::A)) {
            LookUpResult::Referral { ns, .. } => {
                assert_eq!(ns.records, vec![RecordData::NS(name(&["ns", "sub", "example", "com"]))]);
            }
            _ => panic!("expected Referral"),
        }
    }

    #[test]
    fn query_supports_multi_label_apex() {
        // Apex is three labels deep: `zone.example.com`.
        let mut apex = ZoneNode::new(name(&["zone", "example", "com"]), true);
        apex.add_rr_set(rtype(RecordType::SOA), RRSet::single(3600, soa_record()));
        apex.upsert_node(&name(&["www", "zone", "example", "com"]))
            .add_rr_set(rtype(RecordType::A), RRSet::single(300, a([7, 7, 7, 7])));

        match apex.query(&name(&["www", "zone", "example", "com"]), rtype(RecordType::A)) {
            LookUpResult::Answer(rr) => assert_eq!(rr.records, vec![a([7, 7, 7, 7])]),
            _ => panic!("expected Answer for multi-label apex zone"),
        }
    }

    #[test]
    fn find_and_remove_operate_at_apex_depth() {
        let depth = name(&["example", "com"]).label_count();
        let mut z = zone();

        assert!(z.find(&name(&["www", "example", "com"]), depth).is_some());
        assert!(z.remove(&name(&["www", "example", "com"]), depth));
        assert!(z.find(&name(&["www", "example", "com"]), depth).is_none());
        // Removing `www` also drops the deeper name beneath it.
        assert!(z.find(&name(&["deep", "sub", "www", "example", "com"]), depth).is_none());
        // A sibling name is untouched.
        assert!(z.find(&name(&["alias", "example", "com"]), depth).is_some());
    }
}