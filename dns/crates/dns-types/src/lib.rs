use std::sync::Arc;
use crate::dns_message::response_codes::ResponseCode;
use crate::zone::Zones;
use crate::zone::alias::AliasCache;

pub(crate) mod wire;
pub mod zone;
pub mod types;
pub mod dns_message;

pub use dns_message::message::AnswerKind;
// The wire codec is crate-internal, but its error types are part of the parsing API surface.
pub use wire::{ReaderError, WriterError};

#[derive(Debug, thiserror::Error)]
pub enum HandleError {
    #[error("failed to parse query: {0}")]
    Parse(#[from] ReaderError),

    #[error("failed to serialize response: {0}")]
    Serialize(#[from] WriterError),
}

impl HandleError {
    pub fn to_response_code(&self) -> ResponseCode {
        match self {
            HandleError::Parse(error) => error.to_response_code(),
            HandleError::Serialize(error) => error.to_response_code(),
        }
    }
}

pub struct Handled {
    pub qname: Option<types::Name>,
    pub qtype: u16,
    pub rcode: ResponseCode,
    pub answer_kind: AnswerKind,
    pub answer_count: usize,
    pub bytes: Vec<u8>,
}

pub fn handle_message_detailed(message: &[u8], zones: &Zones, aliases: &AliasCache) -> Result<Handled, HandleError> {
    let mut reader = wire::Reader::new(message);
    let query = dns_message::message::DNSMessage::new(&mut reader)?;

    let (response, answer_kind) = query.handle_classified(zones, aliases);

    let mut writer = wire::Writer::new();
    writer.write_message(&response)?;

    Ok(Handled {
        qname: query.questions.first().map(|q| q.name.clone()),
        qtype: query.questions.first().map(|q| q.qtype).unwrap_or(0),
        rcode: response.header.rcode,
        answer_kind,
        answer_count: response.answers.len(),
        bytes: writer.into_bytes(),
    })
}

pub fn handle_message(message: &[u8], zones: Arc<Zones>, aliases: Arc<AliasCache>) -> Result<Vec<u8>, HandleError> {
    Ok(handle_message_detailed(message, &zones, &aliases)?.bytes)
}

pub fn truncated_response(response: &[u8]) -> Option<Vec<u8>> {
    let mut reader = wire::Reader::new(response);
    let header = reader.read_header().ok()?;

    for _ in 0..header.qdcount {
        reader.read_question().ok()?;
    }

    let mut bytes = response.get(..reader.position())?.to_vec();

    bytes[2] |= 0x02;
    bytes[6..12].fill(0);

    Some(bytes)
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use crate::types::Name;

    /// Encode labels into DNS wire format (length-prefixed labels + root octet).
    /// `encode(&["www", "example", "com"])` => `3www7example3com0`; `encode(&[])` => `0`.
    pub fn encode(labels: &[&str]) -> Vec<u8> {
        let mut raw = Vec::new();
        for label in labels {
            raw.push(label.len() as u8);
            raw.extend_from_slice(label.as_bytes());
        }
        raw.push(0);
        raw
    }

    /// Build a `Name` from presentation labels (no trailing empty label needed).
    pub fn name(labels: &[&str]) -> Name {
        Name::new(encode(labels))
    }
}

#[cfg(test)]
mod truncation_tests {
    use super::*;
    use crate::zone::Zones;
    use crate::zone::dnstype::SOA;
    use crate::zone::resource_record::RecordData;
    use crate::zone::rr_set::RRSet;
    use std::net::Ipv4Addr;

    fn zones() -> Zones {
        let mut zones = Zones::new();
        let apex = test_helpers::name(&["example", "com"]);
        let soa = SOA {
            mname: test_helpers::name(&["ns", "example", "com"]),
            rname: test_helpers::name(&["hostmaster", "example", "com"]),
            serial: 1, refresh: 2, retry: 3, expire: 4, minimum: 5,
        };
        zones.add_zone(apex.clone(), soa, 3600);
        zones.upsert_rrset(
            &apex,
            test_helpers::name(&["www", "example", "com"]),
            RRSet::single(300, RecordData::A(Ipv4Addr::new(1, 2, 3, 4))),
        );
        zones
    }

    /// A query for `www.example.com IN A`, id 0x1234.
    fn query() -> Vec<u8> {
        let mut bytes = vec![
            0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        bytes.extend_from_slice(&test_helpers::encode(&["www", "example", "com"]));
        bytes.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]); // QTYPE=A, QCLASS=IN
        bytes
    }

    fn answered() -> Vec<u8> {
        handle_message_detailed(&query(), &zones(), &AliasCache::new()).unwrap().bytes
    }

    #[test]
    fn truncating_sets_the_tc_bit() {
        let truncated = truncated_response(&answered()).unwrap();
        assert_ne!(truncated[2] & 0x02, 0, "TC must be set");
    }

    #[test]
    fn truncating_keeps_the_id_and_question_but_drops_the_records() {
        let full = answered();
        let truncated = truncated_response(&full).unwrap();

        // Same transaction id, or the resolver will not match the reply to its query.
        assert_eq!(&truncated[0..2], &full[0..2]);

        // The question is retained, so the resolver knows what this is a reply to.
        assert_eq!(u16::from_be_bytes([truncated[4], truncated[5]]), 1, "QDCOUNT must stay 1");

        // Counts must match the now-absent sections, or the message reads as malformed rather
        // than as truncated.
        assert_eq!(u16::from_be_bytes([truncated[6], truncated[7]]), 0, "ANCOUNT");
        assert_eq!(u16::from_be_bytes([truncated[8], truncated[9]]), 0, "NSCOUNT");
        assert_eq!(u16::from_be_bytes([truncated[10], truncated[11]]), 0, "ARCOUNT");
    }

    #[test]
    fn a_truncated_reply_is_smaller_than_the_answer_it_replaces() {
        // The whole point: it must not be worth reflecting at a spoofed victim.
        let full = answered();
        let truncated = truncated_response(&full).unwrap();
        assert!(truncated.len() < full.len(), "{} !< {}", truncated.len(), full.len());
    }

    #[test]
    fn a_truncated_reply_reparses_as_a_valid_message() {
        let truncated = truncated_response(&answered()).unwrap();

        let mut reader = wire::Reader::new(&truncated);
        let header = reader.read_header().expect("header must parse");

        assert!(header.tc);
        assert_eq!(header.qdcount, 1);
        assert_eq!(header.ancount, 0);
        reader.read_question().expect("question must parse");
    }

    #[test]
    fn garbage_returns_none_rather_than_panicking() {
        // The caller drops the response in this case; it must never take the task down.
        assert!(truncated_response(&[]).is_none());
        assert!(truncated_response(&[0x00, 0x01]).is_none());
        // A header claiming a question that is not there.
        assert!(truncated_response(&[0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).is_none());
    }
}

