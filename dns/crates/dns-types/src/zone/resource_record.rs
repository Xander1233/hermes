use std::net::{Ipv4Addr, Ipv6Addr};
use std::vec;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use crate::wire::{Reader, ReaderResult};
use crate::zone::dnstype::SOA;
use crate::types::Name;

#[derive(Debug, Clone)]
pub struct ResourceRecord {
    pub name: Name,
    pub rtype: u16,
    pub rclass: u16,
    pub ttl: u32,

    pub data_offset: usize, // Offset in the original buffer where the RDATA starts
    pub buffer: Vec<u8>,
    
    pub record_data: RecordData
}

impl ResourceRecord {
    pub fn from_answer(name: Name, rtype: u16, rclass: u16, ttl: u32, record_data: RecordData) -> ResourceRecord {
        ResourceRecord {
            name,
            rtype,
            rclass,
            ttl,
            data_offset: 0, // Not used in this case
            buffer: Vec::new(), // Not used in this case
            record_data,
        }
    }
    
    pub fn new(name: Name, rtype: u16, rclass: u16, ttl: u32, data_offset: usize, buffer: Vec<u8>) -> ResourceRecord {
        let record = ResourceRecord {
            name,
            rtype,
            rclass,
            ttl,
            data_offset,
            buffer,
            record_data: RecordData::Unknown { rtype, data: Vec::new() }, // Placeholder, will be parsed next
        };
        
        let parsed_data = record.parse_data().unwrap_or(RecordData::Unknown { rtype, data: Vec::new() });
        
        ResourceRecord {
            record_data: parsed_data,
            ..record
        }
    }
    
    pub fn parse_data(&self) -> ReaderResult<RecordData> {
        let rdata = &self.buffer[self.data_offset..];
        match self.rtype {
            1 => Ok(RecordData::A(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]))),
            28 => {
                let addr = Ipv6Addr::new(
                    ((rdata[0] as u16) << 8) | (rdata[1] as u16),
                    ((rdata[2] as u16) << 8) | (rdata[3] as u16),
                    ((rdata[4] as u16) << 8) | (rdata[5] as u16),
                    ((rdata[6] as u16) << 8) | (rdata[7] as u16),
                    ((rdata[8] as u16) << 8) | (rdata[9] as u16),
                    ((rdata[10] as u16) << 8) | (rdata[11] as u16),
                    ((rdata[12] as u16) << 8) | (rdata[13] as u16),
                    ((rdata[14] as u16) << 8) | (rdata[15] as u16),
                );
                
                Ok(RecordData::AAAA(addr))
            }
            2 => {
                let mut reader = Reader::new_with_position(&self.buffer, self.data_offset);
                let name = reader.read_name()?;
                
                Ok(RecordData::NS(name))
            }
            5 => {
                let mut reader = Reader::new_with_position(&self.buffer, self.data_offset);
                let name = reader.read_name()?;
                
                Ok(RecordData::CNAME(name))
            }
            12 => {
                let mut reader = Reader::new_with_position(&self.buffer, self.data_offset);
                let name = reader.read_name()?;
                
                Ok(RecordData::PTR(name))
            }
            15 => {
                let mut reader = Reader::new_with_position(&self.buffer, self.data_offset);
                let preference = reader.u16()?;
                let exchange = reader.read_name()?;
                
                Ok(RecordData::MX { preference, exchange })
            }
            16 => {
                let mut reader = Reader::new_with_position(&self.buffer, self.data_offset);
                let mut txt_data = Vec::new();
                
                while reader.position() < self.buffer.len() {
                    let entry = reader.read_name()?;
                    txt_data.push(entry.raw().to_vec());
                }
                
                Ok(RecordData::TXT(txt_data))
            }
            6 => {
                let mut reader = Reader::new_with_position(&self.buffer, self.data_offset);
                let mname = reader.read_name()?;
                let rname = reader.read_name()?;
                let serial = reader.u32()?;
                let refresh = reader.u32()? as i32;
                let retry = reader.u32()? as i32;
                let expire = reader.u32()? as i32;
                let minimum = reader.u32()?;
                
                Ok(RecordData::SOA(SOA {
                    mname,
                    rname,
                    serial,
                    refresh,
                    retry,
                    expire,
                    minimum,
                }))
            }
            33 => {
                let mut reader = Reader::new_with_position(&self.buffer, self.data_offset);
                let priority = reader.u16()?;
                let weight = reader.u16()?;
                let port = reader.u16()?;
                let target = reader.read_name()?;
                
                Ok(RecordData::SRV { priority, weight, port, target })
            }
            257 => {
                let mut reader = Reader::new_with_position(&self.buffer, self.data_offset);
                let flags = reader.u8()?;
                let tag_len = reader.u8()? as usize;
                let mut tag = vec![0u8; tag_len];
                
                for i in 0..tag_len {
                    tag[i] = reader.u8()?;
                }
                
                let value_len = self.buffer.len() - reader.position();
                let mut value = vec![0u8; value_len];
                for i in 0..value_len {
                    value[i] = reader.u8()?;
                }
                
                Ok(RecordData::CAA { flags, tag, value })
            }
            _ => Ok(RecordData::Unknown {
                rtype: self.rtype,
                data: rdata.to_vec(),
            }),
        }
    }
}

#[repr(u16)]
#[derive(Debug, Clone, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
pub enum RecordType {
    A = 1,
    NS = 2,
    CNAME = 5,
    SOA = 6,
    PTR = 12,
    MX = 15,
    TXT = 16,
    AAAA = 28,
    SRV = 33,
    CAA = 257,
    Alias = 65401,
    Unknown = 0, // Placeholder for unknown types
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordData {
    A(Ipv4Addr),
    AAAA(Ipv6Addr),
    NS(Name),
    CNAME(Name),
    PTR(Name),
    MX { preference: u16, exchange: Name },
    TXT(Vec<Vec<u8>>),
    SOA(SOA),
    SRV { priority: u16, weight: u16, port: u16, target: Name },
    CAA { flags: u8, tag: Vec<u8>, value: Vec<u8> },
    Alias { target: Name },
    Unknown { rtype: u16, data: Vec<u8> },
}

impl RecordData {
    pub fn rtype(&self) -> u16 {
        match self {
            RecordData::A(_) => 1,
            RecordData::AAAA(_) => 28,
            RecordData::NS(_) => 2,
            RecordData::CNAME(_) => 5,
            RecordData::PTR(_) => 12,
            RecordData::MX { .. } => 15,
            RecordData::TXT(_) => 16,
            RecordData::SOA(_) => 6,
            RecordData::SRV { .. } => 33,
            RecordData::CAA { .. } => 257,
            RecordData::Alias { .. } => 65401,
            RecordData::Unknown { rtype, .. } => *rtype,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{encode, name};

    #[test]
    fn rtype_maps_each_variant() {
        assert_eq!(RecordData::A(Ipv4Addr::LOCALHOST).rtype(), 1);
        assert_eq!(RecordData::AAAA(Ipv6Addr::LOCALHOST).rtype(), 28);
        assert_eq!(RecordData::NS(name(&["a"])).rtype(), 2);
        assert_eq!(RecordData::CNAME(name(&["a"])).rtype(), 5);
        assert_eq!(RecordData::PTR(name(&["a"])).rtype(), 12);
        assert_eq!(RecordData::MX { preference: 0, exchange: name(&["a"]) }.rtype(), 15);
        assert_eq!(RecordData::TXT(vec![]).rtype(), 16);
        assert_eq!(RecordData::SRV { priority: 0, weight: 0, port: 0, target: name(&["a"]) }.rtype(), 33);
        assert_eq!(RecordData::CAA { flags: 0, tag: vec![], value: vec![] }.rtype(), 257);
    }

    #[test]
    fn rtype_unknown_returns_stored_type() {
        assert_eq!(RecordData::Unknown { rtype: 999, data: vec![] }.rtype(), 999);
    }

    fn record(rtype: u16, rdata: Vec<u8>) -> ResourceRecord {
        // data_offset 0 with the buffer holding only the RDATA.
        ResourceRecord::new(name(&["example", "com"]), rtype, 1, 300, 0, rdata)
    }

    #[test]
    fn parse_a() {
        match record(1, vec![192, 0, 2, 1]).record_data {
            RecordData::A(addr) => assert_eq!(addr, Ipv4Addr::new(192, 0, 2, 1)),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn parse_aaaa() {
        let addr = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
        match record(28, addr.octets().to_vec()).record_data {
            RecordData::AAAA(got) => assert_eq!(got, addr),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn parse_ns() {
        match record(2, encode(&["ns", "example", "com"])).record_data {
            RecordData::NS(n) => assert_eq!(n, name(&["ns", "example", "com"])),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn parse_cname() {
        match record(5, encode(&["alias", "com"])).record_data {
            RecordData::CNAME(n) => assert_eq!(n, name(&["alias", "com"])),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn parse_mx() {
        let mut rdata = vec![0, 10];
        rdata.extend_from_slice(&encode(&["mail", "com"]));
        match record(15, rdata).record_data {
            RecordData::MX { preference, exchange } => {
                assert_eq!(preference, 10);
                assert_eq!(exchange, name(&["mail", "com"]));
            }
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn parse_soa() {
        let mut rdata = encode(&["ns", "com"]);
        rdata.extend_from_slice(&encode(&["root", "com"]));
        rdata.extend_from_slice(&1u32.to_be_bytes());
        rdata.extend_from_slice(&2u32.to_be_bytes());
        rdata.extend_from_slice(&3u32.to_be_bytes());
        rdata.extend_from_slice(&4u32.to_be_bytes());
        rdata.extend_from_slice(&5u32.to_be_bytes());
        match record(6, rdata).record_data {
            RecordData::SOA(soa) => {
                assert_eq!(soa.mname, name(&["ns", "com"]));
                assert_eq!(soa.rname, name(&["root", "com"]));
                assert_eq!(soa.serial, 1);
                assert_eq!(soa.refresh, 2);
                assert_eq!(soa.retry, 3);
                assert_eq!(soa.expire, 4);
                assert_eq!(soa.minimum, 5);
            }
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn parse_srv() {
        let mut rdata = vec![0, 1, 0, 2, 0x01, 0xBB]; // priority 1, weight 2, port 443
        rdata.extend_from_slice(&encode(&["svc", "com"]));
        match record(33, rdata).record_data {
            RecordData::SRV { priority, weight, port, target } => {
                assert_eq!((priority, weight, port), (1, 2, 443));
                assert_eq!(target, name(&["svc", "com"]));
            }
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn parse_caa() {
        let mut rdata = vec![0, 5];
        rdata.extend_from_slice(b"issue");
        rdata.extend_from_slice(b"ca.com");
        match record(257, rdata).record_data {
            RecordData::CAA { flags, tag, value } => {
                assert_eq!(flags, 0);
                assert_eq!(tag, b"issue");
                assert_eq!(value, b"ca.com");
            }
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn parse_unknown_keeps_raw_rdata() {
        match record(64000, vec![9, 8, 7]).record_data {
            RecordData::Unknown { rtype, data } => {
                assert_eq!(rtype, 64000);
                assert_eq!(data, vec![9, 8, 7]);
            }
            other => panic!("got {:?}", other),
        }
    }
}