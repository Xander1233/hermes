use std::sync::Arc;
use dns_types::{Handled, HandleError};
use dns_types::zone::Zones;
use dns_types::zone::alias::AliasCache;

pub(crate) async fn handle_connection(message: &[u8], zones: Arc<Zones>, aliases: Arc<AliasCache>) -> Result<Handled, HandleError> {
    crate::answer::answer("udp", message, &zones, &aliases)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dns_types::types::Name;
    use dns_types::zone::dnstype::SOA;
    use dns_types::zone::resource_record::RecordData;
    use dns_types::zone::rr_set::RRSet;
    use std::net::Ipv4Addr;

    fn nm(labels: &[&str]) -> Name {
        let mut raw = Vec::new();
        for label in labels {
            raw.push(label.len() as u8);
            raw.extend_from_slice(label.as_bytes());
        }
        raw.push(0);
        Name::new(raw)
    }

    fn zones() -> Zones {
        let mut zones = Zones::new();
        let apex = nm(&["example", "com"]);
        let soa = SOA {
            mname: nm(&["ns", "example", "com"]),
            rname: nm(&["hostmaster", "example", "com"]),
            serial: 1, refresh: 2, retry: 3, expire: 4, minimum: 5,
        };
        zones.add_zone(apex.clone(), soa, 3600);
        zones.upsert_rrset(&apex, nm(&["www", "example", "com"]), RRSet::single(300, RecordData::A(Ipv4Addr::new(1, 2, 3, 4))));
        zones
    }

    #[tokio::test]
    async fn answers_a_raw_udp_query() {
        // Raw query: id=0x1234, RD set, one question for www.example.com IN A.
        let query = vec![
            0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            3, b'w', b'w', b'w', 7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0,
            0x00, 0x01, // QTYPE = A
            0x00, 0x01, // QCLASS = IN
        ];

        let response = handle_connection(&query, Arc::new(zones()), Arc::new(AliasCache::new())).await.unwrap().bytes;

        // Same id echoed back, QR bit set, and exactly one answer.
        assert_eq!(&response[0..2], &[0x12, 0x34]);
        assert_ne!(response[2] & 0x80, 0, "QR bit must be set on a response");
        assert_eq!(u16::from_be_bytes([response[6], response[7]]), 1, "expected one answer");
    }
}
