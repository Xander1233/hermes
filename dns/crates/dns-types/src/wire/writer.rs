use std::collections::HashMap;
use crate::dns_message::header::Header;
use crate::dns_message::message::DNSMessage;
use crate::dns_message::question::Question;
use crate::dns_message::response_codes::ResponseCode;
use crate::types::Name;
use crate::zone::resource_record::RecordData;

const CLASS_IN: u16 = 1;

#[derive(Debug, thiserror::Error)]
pub enum WriterError {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("invalid label length: {0}")]
    InvalidLabelLength(u8),
    #[error("invalid domain name")]
    InvalidDomainName,
    #[error("invalid pointer offset: {0}")]
    InvalidPointerOffset(u16),
    #[error("RDATA length exceeds maximum allowed size")]
    RdataLengthExceeded,
    #[error("TXT record string exceeds maximum length of 255 bytes")]
    TxtStringTooLong,
    #[error("internal record type {0} cannot be serialized to the wire")]
    NonWritableRecordType(u16),
}

impl WriterError {
    pub fn to_response_code(&self) -> ResponseCode {
        match self {
            WriterError::UnexpectedEof => ResponseCode::FormErr,
            WriterError::InvalidLabelLength(_) => ResponseCode::FormErr,
            WriterError::InvalidDomainName => ResponseCode::FormErr,
            WriterError::InvalidPointerOffset(_) => ResponseCode::FormErr,
            WriterError::RdataLengthExceeded => ResponseCode::FormErr,
            WriterError::TxtStringTooLong => ResponseCode::FormErr,
            WriterError::NonWritableRecordType(_) => ResponseCode::ServFail,
        }
    }
}

pub(crate) type WriterResult<T> = Result<T, WriterError>;

#[derive(Debug)]
pub struct Writer {
    pub buffer: Vec<u8>,
    pub names: HashMap<Name, u16>
}

impl Writer {
    pub fn new() -> Self { Writer { buffer: Vec::new(), names: HashMap::new() } }

    pub fn into_bytes(self) -> Vec<u8> { self.buffer }

    #[inline]
    pub fn u16(&mut self, value: u16) {
        self.buffer.extend_from_slice(&value.to_be_bytes());
    }

    #[inline]
    pub fn u32(&mut self, value: u32) {
        self.buffer.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_message(&mut self, message: &DNSMessage) -> WriterResult<()> {
        let mut header = message.header.clone();
        header.qdcount = message.questions.len() as u16;
        header.ancount = message.answers.len() as u16;
        header.nscount = message.authorities.len() as u16;
        header.arcount = message.additionals.len() as u16;

        self.write_header(&header)?;

        for q in &message.questions {
            self.write_question(q)?;
        }
        for a in &message.answers {
            self.write_record(&a.name, a.ttl, &a.record_data)?;
        }
        for ns in &message.authorities {
            self.write_record(&ns.name, ns.ttl, &ns.record_data)?;
        }
        for ar in &message.additionals {
            self.write_record(&ar.name, ar.ttl, &ar.record_data)?;
        }

        Ok(())
    }

    pub fn write_header(&mut self, header: &Header) -> WriterResult<()> {
        self.u16(header.id);
        self.u16(header.flags());
        self.u16(header.qdcount);
        self.u16(header.ancount);
        self.u16(header.nscount);
        self.u16(header.arcount);

        Ok(())
    }

    pub fn write_question(&mut self, question: &Question) -> WriterResult<()> {
        self.write_name(&question.name, true)?;
        self.u16(question.qtype);
        self.u16(question.qclass);

        Ok(())
    }

    pub fn write_record(&mut self, name: &Name, ttl: u32, data: &RecordData) -> WriterResult<()> {
        self.write_name(name, true)?;
        self.u16(data.rtype());
        self.u16(CLASS_IN);
        self.u32(ttl);

        // Reserve space for RDLENGTH, will be filled after writing RDATA
        let len_pos = self.buffer.len();
        self.u16(0);

        let rdata_start = self.buffer.len();
        self.write_rdata(data)?;
        let rdlength = (self.buffer.len() - rdata_start) as u16;

        // Fill in the RDLENGTH field
        self.buffer[len_pos..len_pos + 2].copy_from_slice(&rdlength.to_be_bytes());

        Ok(())
    }

    pub fn write_rdata(&mut self, data: &RecordData) -> WriterResult<()> {
        match data {
            RecordData::A(addr) => {
                self.buffer.extend_from_slice(&addr.octets());
            }
            RecordData::AAAA(addr) => {
                self.buffer.extend_from_slice(&addr.octets());
            }
            RecordData::NS(name)
            | RecordData::CNAME(name)
            | RecordData::PTR(name) => {
                self.write_name(name, true)?;
            }
            RecordData::MX { preference, exchange } => {
                self.u16(*preference);
                self.write_name(exchange, true)?;
            }
            RecordData::SOA(soa) => {
                self.write_name(&soa.mname, true)?;
                self.write_name(&soa.rname, true)?;
                self.u32(soa.serial);
                self.u32(soa.refresh as u32);
                self.u32(soa.retry as u32);
                self.u32(soa.expire as u32);
                self.u32(soa.minimum);
            }
            RecordData::SRV { priority, weight, port, target } => {
                self.u16(*priority);
                self.u16(*weight);
                self.u16(*port);
                self.write_name(target, false)?;
            }
            RecordData::TXT(strings) => {
                for s in strings {
                    if s.len() > 255 {
                        return Err(WriterError::TxtStringTooLong);
                    }

                    self.buffer.push(s.len() as u8);
                    self.buffer.extend_from_slice(s);
                }
            }
            RecordData::CAA { flags, tag, value } => {
                self.buffer.push(*flags);
                self.buffer.push(tag.len() as u8);
                self.buffer.extend_from_slice(tag);
                self.buffer.extend_from_slice(value);
            }
            RecordData::Unknown { data, ..} => {
                self.buffer.extend_from_slice(data);
            }
            RecordData::Alias { .. } => {
                return Err(WriterError::NonWritableRecordType(65401));
            }
        }

        Ok(())
    }

    pub fn write_name(&mut self, name: &Name, compress: bool) -> WriterResult<()> {
        for i in 0..name.offsets().len() {
            let start = name.offsets()[i] as usize;
            let label_len = name.raw()[start] as usize;

            if label_len == 0 {
                self.buffer.push(0);

                return Ok(());
            }

            let suffix = name.suffix_from(i);
            if compress && let Some(&offset) = self.names.get(&suffix) {
                self.buffer.extend_from_slice(&(0xC000 | offset).to_be_bytes());

                return Ok(());
            }

            let buffer_offset = self.buffer.len();
            if buffer_offset <= 0x3FFF {
                self.names.insert(suffix, buffer_offset as u16);
            }

            self.buffer.push(label_len as u8);
            self.buffer.extend_from_slice(&name.raw()[start + 1..start + 1 + label_len]);
        }

        self.buffer.push(0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::Reader;
    use crate::test_helpers::name;
    use crate::zone::dnstype::SOA;
    use crate::zone::resource_record::ResourceRecord;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn new_is_empty() {
        let w = Writer::new();
        assert!(w.buffer.is_empty());
        assert!(w.names.is_empty());
    }

    #[test]
    fn primitive_writes_are_big_endian() {
        let mut w = Writer::new();
        w.u16(0x3456);
        w.u32(0x789ABCDE);
        assert_eq!(w.buffer, vec![0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE]);
    }

    #[test]
    fn write_name_without_compression() {
        let mut w = Writer::new();
        w.write_name(&name(&["www", "com"]), false).unwrap();
        assert_eq!(w.buffer, vec![3, b'w', b'w', b'w', 3, b'c', b'o', b'm', 0]);
    }

    #[test]
    fn write_name_root() {
        let mut w = Writer::new();
        w.write_name(&name(&[]), false).unwrap();
        assert_eq!(w.buffer, vec![0]);
    }

    #[test]
    fn write_name_reuses_suffix_pointer_when_compressing() {
        let mut w = Writer::new();
        w.write_name(&name(&["example", "com"]), true).unwrap();
        let after_first = w.buffer.len();
        w.write_name(&name(&["www", "example", "com"]), true).unwrap();

        // Second name writes "www" then a pointer back to the "example.com" suffix at offset 0.
        assert_eq!(w.buffer[after_first..], [3, b'w', b'w', b'w', 0xC0, 0x00]);
    }

    #[test]
    fn write_name_roundtrips_through_reader() {
        let mut w = Writer::new();
        let original = name(&["www", "example", "com"]);
        w.write_name(&original, false).unwrap();

        let mut reader = Reader::new(&w.buffer);
        assert_eq!(reader.read_name().unwrap(), original);
    }

    #[test]
    fn write_header_roundtrips_through_reader() {
        let header = Header {
            id: 0xABCD,
            qr: true, opcode: 2, aa: true, tc: false, rd: true, ra: false, z: 0, rcode: ResponseCode::NXDomain,
            qdcount: 1, ancount: 0, nscount: 0, arcount: 0,
        };
        let mut w = Writer::new();
        w.write_header(&header).unwrap();

        let mut reader = Reader::new(&w.buffer);
        let decoded = reader.read_header().unwrap();
        assert_eq!(decoded.id, header.id);
        assert_eq!(decoded.flags(), header.flags());
        assert_eq!(decoded.qdcount, 1);
        assert!(decoded.qr && decoded.aa && decoded.rd);
        assert_eq!(decoded.opcode, 2);
        assert_eq!(decoded.rcode, ResponseCode::NXDomain);
    }

    #[test]
    fn write_question_roundtrips_through_reader() {
        let q = Question { name: name(&["example", "com"]), qtype: 1, qclass: 1 };
        let mut w = Writer::new();
        w.write_question(&q).unwrap();

        let mut reader = Reader::new(&w.buffer);
        let decoded = reader.read_question().unwrap();
        assert_eq!(decoded.name, q.name);
        assert_eq!(decoded.qtype, 1);
        assert_eq!(decoded.qclass, 1);
    }

    #[test]
    fn write_record_fills_rdlength_and_roundtrips() {
        let mut w = Writer::new();
        let data = RecordData::A(Ipv4Addr::new(10, 0, 0, 1));
        w.write_record(&name(&["example", "com"]), 300, &data).unwrap();

        let mut reader = Reader::new(&w.buffer);
        let rr = reader.read_record().unwrap();
        assert_eq!(rr.name, name(&["example", "com"]));
        assert_eq!(rr.rtype, 1);
        assert_eq!(rr.rclass, CLASS_IN);
        assert_eq!(rr.ttl, 300);
        match rr.record_data {
            RecordData::A(addr) => assert_eq!(addr, Ipv4Addr::new(10, 0, 0, 1)),
            other => panic!("expected A record, got {:?}", other),
        }
    }

    fn rdata_bytes(data: &RecordData) -> Vec<u8> {
        let mut w = Writer::new();
        w.write_rdata(data).unwrap();
        w.buffer
    }

    #[test]
    fn write_rdata_a() {
        assert_eq!(rdata_bytes(&RecordData::A(Ipv4Addr::new(1, 2, 3, 4))), vec![1, 2, 3, 4]);
    }

    #[test]
    fn write_rdata_aaaa() {
        let addr = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
        assert_eq!(rdata_bytes(&RecordData::AAAA(addr)), addr.octets().to_vec());
    }

    #[test]
    fn write_rdata_ns_writes_a_name() {
        let bytes = rdata_bytes(&RecordData::NS(name(&["ns", "example", "com"])));
        assert_eq!(bytes, vec![2, b'n', b's', 7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0]);
    }

    #[test]
    fn write_rdata_mx_writes_preference_then_name() {
        let bytes = rdata_bytes(&RecordData::MX { preference: 10, exchange: name(&["mail", "com"]) });
        assert_eq!(bytes, vec![0, 10, 4, b'm', b'a', b'i', b'l', 3, b'c', b'o', b'm', 0]);
    }

    #[test]
    fn write_rdata_txt_length_prefixes_each_string() {
        let bytes = rdata_bytes(&RecordData::TXT(vec![b"hi".to_vec(), b"abc".to_vec()]));
        assert_eq!(bytes, vec![2, b'h', b'i', 3, b'a', b'b', b'c']);
    }

    #[test]
    fn write_rdata_txt_rejects_oversized_string() {
        let mut w = Writer::new();
        let big = vec![b'x'; 256];
        assert!(matches!(
            w.write_rdata(&RecordData::TXT(vec![big])),
            Err(WriterError::TxtStringTooLong)
        ));
    }

    #[test]
    fn write_rdata_caa() {
        let data = RecordData::CAA { flags: 0, tag: b"issue".to_vec(), value: b"ca.com".to_vec() };
        let mut expected = vec![0, 5, b'i', b's', b's', b'u', b'e'];
        expected.extend_from_slice(b"ca.com");
        assert_eq!(rdata_bytes(&data), expected);
    }

    #[test]
    fn write_rdata_srv() {
        let data = RecordData::SRV { priority: 1, weight: 2, port: 443, target: name(&["svc", "com"]) };
        let mut expected = vec![0, 1, 0, 2, 0x01, 0xBB];
        expected.extend_from_slice(&[3, b's', b'v', b'c', 3, b'c', b'o', b'm', 0]);
        assert_eq!(rdata_bytes(&data), expected);
    }

    #[test]
    fn write_rdata_soa_roundtrips() {
        // mname/rname are written with compression, so assert on a parse roundtrip
        // rather than hand-computed pointer bytes.
        let soa = SOA {
            mname: name(&["ns", "com"]),
            rname: name(&["root", "com"]),
            serial: 1,
            refresh: 2,
            retry: 3,
            expire: 4,
            minimum: 5,
        };
        let rdata = rdata_bytes(&RecordData::SOA(soa));
        let rr = ResourceRecord::new(name(&["example", "com"]), 6, 1, 300, 0, rdata);
        match rr.record_data {
            RecordData::SOA(got) => {
                assert_eq!(got.mname, name(&["ns", "com"]));
                assert_eq!(got.rname, name(&["root", "com"]));
                assert_eq!((got.serial, got.refresh, got.retry, got.expire, got.minimum), (1, 2, 3, 4, 5));
            }
            other => panic!("expected SOA, got {:?}", other),
        }
    }

    #[test]
    fn write_rdata_unknown_copies_raw() {
        let data = RecordData::Unknown { rtype: 999, data: vec![9, 8, 7] };
        assert_eq!(rdata_bytes(&data), vec![9, 8, 7]);
    }
}
