use std::collections::HashSet;
use crate::dns_message::header::Header;
use crate::dns_message::question::Question;
use crate::dns_message::response_codes::ResponseCode;
use crate::zone::alias::{self, AliasCache, AliasOutcome};
use crate::zone::resource_record::{RecordData, RecordType, ResourceRecord};
use crate::zone::rr_set::RRSet;
use crate::wire::{Reader, ReaderResult};
use crate::types::Name;
use crate::zone::zone_node::LookUpResult;
use crate::zone::Zones;

const MAX_CNAME_CHAIN: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerKind {
    Answer,
    Wildcard,
    Cname,
    Alias,
    NoData,
    NxDomain,
    Referral,
    Refused,
    ServFail,
    FormErr,
}

impl AnswerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AnswerKind::Answer => "answer",
            AnswerKind::Wildcard => "wildcard",
            AnswerKind::Cname => "cname",
            AnswerKind::Alias => "alias",
            AnswerKind::NoData => "nodata",
            AnswerKind::NxDomain => "nxdomain",
            AnswerKind::Referral => "referral",
            AnswerKind::Refused => "refused",
            AnswerKind::ServFail => "servfail",
            AnswerKind::FormErr => "formerr",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DNSMessage {
    pub header: Header,
    pub questions: Vec<Question>,
    pub answers: Vec<ResourceRecord>,
    pub authorities: Vec<ResourceRecord>,
    pub additionals: Vec<ResourceRecord>,
}

impl DNSMessage {
    pub fn new(reader: &mut Reader) -> ReaderResult<DNSMessage> {

        let header = reader.read_header()?;
        let mut questions: Vec<Question> = Vec::with_capacity(header.qdcount as usize);
        let mut answers: Vec<ResourceRecord> = Vec::with_capacity(header.ancount as usize);
        let mut authorities: Vec<ResourceRecord> = Vec::with_capacity(header.nscount as usize);
        let mut additionals: Vec<ResourceRecord> = Vec::with_capacity(header.arcount as usize);

        for _ in 0..header.qdcount {
            questions.push(reader.read_question()?);
        }
        for _ in 0..header.ancount {
            answers.push(reader.read_record()?);
        }
        for _ in 0..header.nscount {
            authorities.push(reader.read_record()?);
        }
        for _ in 0..header.arcount {
            additionals.push(reader.read_record()?);
        }

        Ok(DNSMessage {
            header,
            questions,
            answers,
            authorities,
            additionals,
        })
    }

    pub fn handle(&self, zones: &Zones, aliases: &AliasCache) -> DNSMessage {
        self.handle_classified(zones, aliases).0
    }

    pub fn handle_classified(&self, zones: &Zones, aliases: &AliasCache) -> (DNSMessage, AnswerKind) {
        if self.questions.len() != 1 {
            return (self.handle_error(ResponseCode::FormErr), AnswerKind::FormErr);
        }
        
        let question = &self.questions[0];
        let qtype = question.qtype;

        let mut answers: Vec<ResourceRecord> = Vec::new();
        let mut current: Name = question.name.clone();
        let mut seen_names: HashSet<Name> = HashSet::new();

        for _ in 0..MAX_CNAME_CHAIN {
            if !seen_names.insert(current.clone()) {
                return (self.handle_error(ResponseCode::ServFail), AnswerKind::ServFail);
            }

            let Some(zone) = zones.find_authoritative_zone(&current) else {
                return if answers.is_empty() {
                    (self.handle_error(ResponseCode::Refused), AnswerKind::Refused)
                } else {
                    (self.handle_success(answers, false), AnswerKind::Cname)
                };
            };

            let (result, via_wildcard) = zone.query_classified(&current, qtype);
            
            match result {
                LookUpResult::Answer(rr_set) => {
                    Self::append_rr_set(&mut answers, &current, question.qclass, &rr_set);
                    let kind = if via_wildcard { AnswerKind::Wildcard } else { AnswerKind::Answer };
                    
                    return (self.handle_success(answers, true), kind);
                }
                LookUpResult::CName { record, target } => {
                    Self::append_rr_set(&mut answers, &current, question.qclass, &record);
                    
                    if qtype == u16::from(RecordType::CNAME) {
                        return (self.handle_success(answers, true), AnswerKind::Cname);
                    }
                    current = target;
                    
                    continue;
                }
                LookUpResult::Referral { owner, ns, glue } => {
                    return (self.handle_referral(owner, question.qclass, ns, glue), AnswerKind::Referral);
                }
                LookUpResult::Alias { target, ttl } => {
                    let entry = aliases.get(&target);
                    
                    return match alias::synthesize(entry.as_ref(), qtype, ttl) {
                        AliasOutcome::Records(rr_set) => {
                            Self::append_rr_set(&mut answers, &current, question.qclass, &rr_set);
                            
                            (self.handle_success(answers, true), AnswerKind::Alias)
                        }
                        AliasOutcome::NoData => match zone.soa() {
                            Some(soa) => (self.handle_nodata(answers, &zone.name, soa.clone()), AnswerKind::NoData),
                            None => (self.handle_success(answers, true), AnswerKind::Alias),
                        },
                        AliasOutcome::ServFail => (self.handle_error(ResponseCode::ServFail), AnswerKind::ServFail),
                    };
                }
                LookUpResult::NoData => {
                    return match zone.soa() {
                        Some(soa) => (self.handle_nodata(answers, &zone.name, soa.clone()), AnswerKind::NoData),
                        None => (self.handle_success(answers, true), AnswerKind::NoData),
                    };
                }
                LookUpResult::NXDomain => {
                    return match zone.soa() {
                        Some(soa) => (self.handle_nxdomain(&zone.name, soa.clone()), AnswerKind::NxDomain),
                        None => (self.handle_error(ResponseCode::NXDomain), AnswerKind::NxDomain),
                    };
                }
            }
        }

        (self.handle_error(ResponseCode::ServFail), AnswerKind::ServFail)
    }

    fn append_rr_set(answers: &mut Vec<ResourceRecord>, owner: &Name, qclass: u16, rr_set: &RRSet) {
        for record in &rr_set.records {
            answers.push(ResourceRecord::from_answer(
                owner.clone(),
                rr_set.rtype,
                qclass,
                rr_set.ttl,
                record.clone(),
            ));
        }
    }

    fn build_response(
        &self,
        rcode: ResponseCode,
        aa: bool,
        answers: Vec<ResourceRecord>,
        authorities: Vec<ResourceRecord>,
        additionals: Vec<ResourceRecord>,
    ) -> DNSMessage {
        let mut header = self.header.clone();
        header.qr = true;
        header.aa = aa;
        header.rcode = rcode;
        header.qdcount = self.questions.len() as u16;
        header.ancount = answers.len() as u16;
        header.nscount = authorities.len() as u16;
        header.arcount = additionals.len() as u16;

        DNSMessage {
            header,
            questions: self.questions.clone(),
            answers,
            authorities,
            additionals,
        }
    }

    fn handle_error(&self, rcode: ResponseCode) -> DNSMessage {
        self.build_response(rcode, false, Vec::new(), Vec::new(), Vec::new())
    }

    fn handle_success(&self, answers: Vec<ResourceRecord>, aa: bool) -> DNSMessage {
        self.build_response(ResponseCode::NoError, aa, answers, Vec::new(), Vec::new())
    }

    fn handle_nodata(&self, answers: Vec<ResourceRecord>, apex: &Name, soa: RRSet) -> DNSMessage {
        let authority = self.soa_authority(apex, &soa);
        self.build_response(ResponseCode::NoError, true, answers, authority, Vec::new())
    }

    fn handle_nxdomain(&self, apex: &Name, soa: RRSet) -> DNSMessage {
        let authority = self.soa_authority(apex, &soa);
        self.build_response(ResponseCode::NXDomain, true, Vec::new(), authority, Vec::new())
    }

    fn handle_referral(
        &self,
        owner: Name,
        qclass: u16,
        ns: RRSet,
        glue: Vec<RecordData>,
    ) -> DNSMessage {
        let authorities = ns
            .records
            .into_iter()
            .map(|record| ResourceRecord::from_answer(owner.clone(), ns.rtype, qclass, ns.ttl, record))
            .collect();
        
        let additionals = glue
            .into_iter()
            .map(|record| {
                let rtype = record.rtype();
                ResourceRecord::from_answer(owner.clone(), rtype, qclass, 0, record)
            })
            .collect();

        self.build_response(ResponseCode::NoError, false, Vec::new(), authorities, additionals)
    }

    fn soa_authority(&self, apex: &Name, soa: &RRSet) -> Vec<ResourceRecord> {
        let qclass = self.questions.first().map(|q| q.qclass).unwrap_or(1);
        
        vec![ResourceRecord::from_answer(
            apex.clone(),
            u16::from(RecordType::SOA),
            qclass,
            soa.ttl,
            soa.records[0].clone(),
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::Writer;
    use crate::test_helpers::name;
    use crate::zone::resource_record::{RecordData, ResourceRecord};
    use std::net::Ipv4Addr;
    use crate::dns_message::response_codes::ResponseCode;

    fn answer(n: crate::types::Name, ttl: u32, data: RecordData) -> ResourceRecord {
        // Build directly: write_message only reads name/ttl/record_data of answers.
        ResourceRecord {
            name: n,
            rtype: data.rtype(),
            rclass: 1,
            ttl,
            data_offset: 0,
            buffer: Vec::new(),
            record_data: data,
        }
    }

    #[test]
    fn message_roundtrips_through_writer_and_reader() {
        let header = Header {
            id: 0x4269,
            qr: true, opcode: 0, aa: true, tc: false, rd: true, ra: true, z: 0, rcode: ResponseCode::NoError,
            // Counts are intentionally wrong; write_message must recompute them.
            qdcount: 99, ancount: 99, nscount: 99, arcount: 99,
        };
        let message = DNSMessage {
            header,
            questions: vec![Question { name: name(&["example", "com"]), qtype: 1, qclass: 1 }],
            answers: vec![
                answer(name(&["example", "com"]), 300, RecordData::A(Ipv4Addr::new(93, 184, 216, 34))),
            ],
            authorities: vec![
                answer(name(&["example", "com"]), 600, RecordData::NS(name(&["ns", "example", "com"]))),
            ],
            additionals: vec![],
        };

        let mut writer = Writer::new();
        writer.write_message(&message).unwrap();

        let mut reader = Reader::new(&writer.buffer);
        let decoded = DNSMessage::new(&mut reader).unwrap();

        // Counts are derived from the actual section lengths.
        assert_eq!(decoded.header.qdcount, 1);
        assert_eq!(decoded.header.ancount, 1);
        assert_eq!(decoded.header.nscount, 1);
        assert_eq!(decoded.header.arcount, 0);
        assert_eq!(decoded.header.id, 0x4269);

        assert_eq!(decoded.questions.len(), 1);
        assert_eq!(decoded.questions[0].name, name(&["example", "com"]));

        assert_eq!(decoded.answers.len(), 1);
        assert_eq!(decoded.answers[0].ttl, 300);
        match &decoded.answers[0].record_data {
            RecordData::A(addr) => assert_eq!(*addr, Ipv4Addr::new(93, 184, 216, 34)),
            other => panic!("expected A, got {:?}", other),
        }

        assert_eq!(decoded.authorities.len(), 1);
        match &decoded.authorities[0].record_data {
            RecordData::NS(n) => assert_eq!(*n, name(&["ns", "example", "com"])),
            other => panic!("expected NS, got {:?}", other),
        }
    }

    #[test]
    fn empty_message_has_zero_counts() {
        let message = DNSMessage {
            header: Header {
                id: 1,
                qr: false, opcode: 0, aa: false, tc: false, rd: false, ra: false, z: 0, rcode: ResponseCode::NoError,
                qdcount: 0, ancount: 0, nscount: 0, arcount: 0,
            },
            questions: vec![],
            answers: vec![],
            authorities: vec![],
            additionals: vec![],
        };
        let mut writer = Writer::new();
        writer.write_message(&message).unwrap();

        let mut reader = Reader::new(&writer.buffer);
        let decoded = DNSMessage::new(&mut reader).unwrap();
        assert!(decoded.questions.is_empty());
        assert!(decoded.answers.is_empty());
    }

    // ---- Query answering flow (`handle` / `handle_message`) ----

    mod flow {
        use super::*;
        use crate::zone::dnstype::SOA;
        use crate::zone::rr_set::RRSet;
        use crate::zone::resource_record::RecordType;
        use crate::zone::Zones;
        use crate::zone::alias::AliasCache;
        use std::sync::Arc;

        fn no_aliases() -> AliasCache {
            AliasCache::new()
        }

        fn soa() -> SOA {
            SOA {
                mname: name(&["ns", "example", "com"]),
                rname: name(&["hostmaster", "example", "com"]),
                serial: 1, refresh: 2, retry: 3, expire: 4, minimum: 5,
            }
        }

        fn a(ip: [u8; 4]) -> RecordData {
            RecordData::A(Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]))
        }

        fn atype() -> u16 { u16::from(RecordType::A) }

        /// Zone `example.com` with an apex A, a `www` A, an `alias` CNAME -> www, and a
        /// delegated `sub` subzone.
        fn zones() -> Zones {
            let mut zones = Zones::new();
            let apex = name(&["example", "com"]);
            zones.add_zone(apex.clone(), soa(), 3600);
            zones.upsert_rrset(&apex, apex.clone(), RRSet::single(300, a([93, 184, 216, 34])));
            zones.upsert_rrset(&apex, name(&["www", "example", "com"]), RRSet::single(300, a([1, 2, 3, 4])));
            zones.upsert_rrset(&apex, name(&["alias", "example", "com"]), RRSet::single(300, RecordData::CNAME(name(&["www", "example", "com"]))));
            zones.upsert_rrset(&apex, name(&["sub", "example", "com"]), RRSet::single(300, RecordData::NS(name(&["ns", "sub", "example", "com"]))));
            zones
        }

        fn query(qname: Name, qtype: u16) -> DNSMessage {
            DNSMessage {
                header: Header {
                    id: 0x1234,
                    qr: false, opcode: 0, aa: false, tc: false, rd: true, ra: false, z: 0,
                    rcode: ResponseCode::NoError,
                    qdcount: 1, ancount: 0, nscount: 0, arcount: 0,
                },
                questions: vec![Question { name: qname, qtype, qclass: 1 }],
                answers: vec![], authorities: vec![], additionals: vec![],
            }
        }

        #[test]
        fn answers_existing_record_authoritatively() {
            let resp = query(name(&["www", "example", "com"]), atype()).handle(&zones(), &no_aliases());
            assert_eq!(resp.header.rcode, ResponseCode::NoError);
            assert!(resp.header.qr && resp.header.aa);
            assert_eq!(resp.answers.len(), 1);
            assert_eq!(resp.answers[0].name, name(&["www", "example", "com"]));
            assert_eq!(resp.answers[0].record_data, a([1, 2, 3, 4]));
        }

        #[test]
        fn nxdomain_carries_soa_authority() {
            let resp = query(name(&["nope", "example", "com"]), atype()).handle(&zones(), &no_aliases());
            assert_eq!(resp.header.rcode, ResponseCode::NXDomain);
            assert!(resp.answers.is_empty());
            assert_eq!(resp.authorities.len(), 1);
            assert_eq!(resp.authorities[0].name, name(&["example", "com"]));
            assert!(matches!(resp.authorities[0].record_data, RecordData::SOA(_)));
        }

        #[test]
        fn nodata_returns_soa_and_no_answers() {
            let resp = query(name(&["www", "example", "com"]), u16::from(RecordType::MX)).handle(&zones(), &no_aliases());
            assert_eq!(resp.header.rcode, ResponseCode::NoError);
            assert!(resp.answers.is_empty());
            assert_eq!(resp.authorities.len(), 1);
            assert!(matches!(resp.authorities[0].record_data, RecordData::SOA(_)));
        }

        #[test]
        fn cname_chain_is_followed_to_the_final_answer() {
            let resp = query(name(&["alias", "example", "com"]), atype()).handle(&zones(), &no_aliases());
            assert_eq!(resp.header.rcode, ResponseCode::NoError);
            assert!(resp.header.aa);
            assert_eq!(resp.answers.len(), 2);
            // First the CNAME (owned by the alias), then the target's A record.
            assert_eq!(resp.answers[0].name, name(&["alias", "example", "com"]));
            assert_eq!(resp.answers[0].record_data, RecordData::CNAME(name(&["www", "example", "com"])));
            assert_eq!(resp.answers[1].name, name(&["www", "example", "com"]));
            assert_eq!(resp.answers[1].record_data, a([1, 2, 3, 4]));
        }

        #[test]
        fn cname_query_returns_only_the_cname() {
            let resp = query(name(&["alias", "example", "com"]), u16::from(RecordType::CNAME)).handle(&zones(), &no_aliases());
            assert_eq!(resp.answers.len(), 1);
            assert!(matches!(resp.answers[0].record_data, RecordData::CNAME(_)));
        }

        #[test]
        fn delegation_produces_a_non_authoritative_referral() {
            let resp = query(name(&["host", "sub", "example", "com"]), atype()).handle(&zones(), &no_aliases());
            assert_eq!(resp.header.rcode, ResponseCode::NoError);
            assert!(!resp.header.aa);
            assert!(resp.answers.is_empty());
            assert_eq!(resp.authorities.len(), 1);
            assert_eq!(resp.authorities[0].name, name(&["sub", "example", "com"]));
            assert!(matches!(resp.authorities[0].record_data, RecordData::NS(_)));
        }

        fn kind_of(qname: Name, qtype: u16, zones: &Zones) -> AnswerKind {
            query(qname, qtype).handle_classified(zones, &no_aliases()).1
        }

        #[test]
        fn answer_kinds_are_classified() {
            let zones = zones();
            // Exact match, delegation, non-existent name, wrong type, out-of-zone.
            assert_eq!(kind_of(name(&["www", "example", "com"]), atype(), &zones), AnswerKind::Answer);
            assert_eq!(kind_of(name(&["host", "sub", "example", "com"]), atype(), &zones), AnswerKind::Referral);
            assert_eq!(kind_of(name(&["nope", "example", "com"]), atype(), &zones), AnswerKind::NxDomain);
            assert_eq!(kind_of(name(&["www", "example", "com"]), u16::from(RecordType::MX), &zones), AnswerKind::NoData);
            assert_eq!(kind_of(name(&["example", "org"]), atype(), &zones), AnswerKind::Refused);
            // A CNAME requested directly is terminal; chased to an A it becomes a plain Answer.
            assert_eq!(kind_of(name(&["alias", "example", "com"]), u16::from(RecordType::CNAME), &zones), AnswerKind::Cname);
            assert_eq!(kind_of(name(&["alias", "example", "com"]), atype(), &zones), AnswerKind::Answer);
        }

        #[test]
        fn a_wildcard_answer_is_classified_as_wildcard() {
            // The distinction the whole enum exists for: a wildcard answers every distinct
            // made-up name NOERROR, which is what defeats per-name rate limiting.
            let mut zones = zones();
            let apex = name(&["example", "com"]);
            zones.upsert_rrset(&apex, name(&["*", "example", "com"]), RRSet::single(300, a([9, 9, 9, 9])));

            // An exact name still classifies as an exact answer...
            assert_eq!(kind_of(name(&["www", "example", "com"]), atype(), &zones), AnswerKind::Answer);
            // ...while a name that only the wildcard can satisfy is flagged.
            assert_eq!(kind_of(name(&["anything", "example", "com"]), atype(), &zones), AnswerKind::Wildcard);
        }

        #[test]
        fn multiple_questions_are_rejected() {
            let mut msg = query(name(&["www", "example", "com"]), atype());
            msg.questions.push(Question { name: name(&["www", "example", "com"]), qtype: atype(), qclass: 1 });
            assert_eq!(msg.handle(&zones(), &no_aliases()).header.rcode, ResponseCode::FormErr);
        }

        #[test]
        fn out_of_zone_name_is_refused() {
            let resp = query(name(&["example", "org"]), atype()).handle(&zones(), &no_aliases());
            assert_eq!(resp.header.rcode, ResponseCode::Refused);
        }

        #[test]
        fn cname_loop_is_detected() {
            let mut zones = Zones::new();
            let apex = name(&["example", "com"]);
            zones.add_zone(apex.clone(), soa(), 3600);
            zones.upsert_rrset(&apex, name(&["a", "example", "com"]), RRSet::single(300, RecordData::CNAME(name(&["b", "example", "com"]))));
            zones.upsert_rrset(&apex, name(&["b", "example", "com"]), RRSet::single(300, RecordData::CNAME(name(&["a", "example", "com"]))));

            let resp = query(name(&["a", "example", "com"]), atype()).handle(&zones, &no_aliases());
            assert_eq!(resp.header.rcode, ResponseCode::ServFail);
        }

        #[test]
        fn end_to_end_bytes_in_bytes_out() {
            // Serialize a query, run the full flow, and parse the response bytes back.
            let mut writer = Writer::new();
            writer.write_message(&query(name(&["www", "example", "com"]), atype())).unwrap();
            let query_bytes = writer.into_bytes();

            let response_bytes = crate::handle_message(&query_bytes, Arc::new(zones()), Arc::new(no_aliases())).unwrap();

            let mut reader = Reader::new(&response_bytes);
            let resp = DNSMessage::new(&mut reader).unwrap();
            assert!(resp.header.qr);
            assert_eq!(resp.header.ancount, 1);
            assert_eq!(resp.answers[0].record_data, a([1, 2, 3, 4]));
        }

        // ---- ALIAS synthesis at the apex ----

        use crate::zone::alias::AliasEntry;
        use crate::zone::resource_record::RecordData as RD;
        use std::net::Ipv4Addr as V4;

        /// A zone whose apex has an ALIAS (no static apex A) plus a normal MX and TXT.
        fn alias_zone() -> Zones {
            let mut zones = Zones::new();
            let apex = name(&["shop", "example", "com"]);
            zones.add_zone(apex.clone(), soa(), 3600);
            zones.upsert_rrset(&apex, apex.clone(), RRSet::single(60, RD::Alias { target: name(&["lb", "provider", "net"]) }));
            zones.upsert_rrset(&apex, apex.clone(), RRSet::single(3600, RD::MX { preference: 10, exchange: name(&["mail", "example", "com"]) }));
            zones
        }

        fn aaaa() -> u16 { u16::from(RecordType::AAAA) }

        #[test]
        fn alias_a_query_synthesizes_addresses() {
            let cache = AliasCache::new();
            cache.upsert(name(&["lb", "provider", "net"]), AliasEntry::resolved(vec![V4::new(203, 0, 113, 7)], vec![], 120));

            let resp = query(name(&["shop", "example", "com"]), atype()).handle(&alias_zone(), &cache);
            assert_eq!(resp.header.rcode, ResponseCode::NoError);
            assert!(resp.header.aa);
            assert_eq!(resp.answers.len(), 1);
            // Answer is owned by the apex (flattened), not the target.
            assert_eq!(resp.answers[0].name, name(&["shop", "example", "com"]));
            assert_eq!(resp.answers[0].record_data, RD::A(V4::new(203, 0, 113, 7)));
            // Emitted TTL = min(record 60, entry 120) = 60.
            assert_eq!(resp.answers[0].ttl, 60);
        }

        #[test]
        fn alias_aaaa_query_with_only_v4_is_nodata_with_soa() {
            let cache = AliasCache::new();
            cache.upsert(name(&["lb", "provider", "net"]), AliasEntry::resolved(vec![V4::new(203, 0, 113, 7)], vec![], 120));

            let resp = query(name(&["shop", "example", "com"]), aaaa()).handle(&alias_zone(), &cache);
            assert_eq!(resp.header.rcode, ResponseCode::NoError);
            assert!(resp.answers.is_empty());
            assert_eq!(resp.authorities.len(), 1);
            assert!(matches!(resp.authorities[0].record_data, RecordData::SOA(_)));
        }

        #[test]
        fn alias_cold_target_is_servfail() {
            // Empty cache: target never resolved.
            let resp = query(name(&["shop", "example", "com"]), atype()).handle(&alias_zone(), &AliasCache::new());
            assert_eq!(resp.header.rcode, ResponseCode::ServFail);
        }

        #[test]
        fn alias_does_not_affect_other_types() {
            // MX at the same apex is served normally regardless of the ALIAS.
            let resp = query(name(&["shop", "example", "com"]), u16::from(RecordType::MX)).handle(&alias_zone(), &AliasCache::new());
            assert_eq!(resp.header.rcode, ResponseCode::NoError);
            assert_eq!(resp.answers.len(), 1);
            assert!(matches!(resp.answers[0].record_data, RecordData::MX { .. }));
        }
    }
}