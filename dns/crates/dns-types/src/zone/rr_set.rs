use crate::zone::resource_record::RecordData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RRSet {
    pub rtype: u16,
    pub ttl: u32,
    pub records: Vec<RecordData>
}

impl RRSet {
    pub fn new(rtype: u16, ttl: u32, records: Vec<RecordData>) -> Self {
        RRSet { rtype, ttl, records }
    }

    pub fn single(ttl: u32, record: RecordData) -> Self {
        RRSet { rtype: record.rtype(), ttl, records: vec![record] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn new_sets_fields() {
        let set = RRSet::new(1, 300, vec![RecordData::A(Ipv4Addr::LOCALHOST)]);
        assert_eq!(set.rtype, 1);
        assert_eq!(set.ttl, 300);
        assert_eq!(set.records.len(), 1);
    }

    #[test]
    fn single_derives_type_from_record() {
        let set = RRSet::single(60, RecordData::A(Ipv4Addr::new(1, 2, 3, 4)));
        assert_eq!(set.rtype, 1);
        assert_eq!(set.records, vec![RecordData::A(Ipv4Addr::new(1, 2, 3, 4))]);
    }
}
