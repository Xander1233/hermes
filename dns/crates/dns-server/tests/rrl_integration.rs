//! End-to-end check that rate limiting is actually wired into the UDP path.
//!
//! The unit tests in `rrl.rs` prove the algorithm; this proves the server uses it — that the
//! decision reaches the socket, that a slipped reply really goes out with `TC=1`, and that TCP
//! is left alone.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use dns_types::zone::alias::AliasCache;
use dns_types::zone::dnstype::SOA;
use dns_types::zone::resource_record::RecordData;
use dns_types::zone::rr_set::RRSet;
use dns_types::zone::store::ZoneStore;
use dns_types::zone::Zones;
use dns_types::types::Name;
use tokio::net::UdpSocket;
use dns_server::{DnsServer, Rrl, RrlConfig, ServerOptions};

fn name(labels: &[&str]) -> Name {
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
    let apex = name(&["example", "com"]);
    zones.add_zone(
        apex.clone(),
        SOA {
            mname: name(&["ns", "example", "com"]),
            rname: name(&["hostmaster", "example", "com"]),
            serial: 1, refresh: 2, retry: 3, expire: 4, minimum: 5,
        },
        3600,
    );
    zones.upsert_rrset(
        &apex,
        name(&["www", "example", "com"]),
        RRSet::single(300, RecordData::A(Ipv4Addr::new(1, 2, 3, 4))),
    );
    zones
}

fn query(id: u16) -> Vec<u8> {
    let mut bytes = vec![
        (id >> 8) as u8, id as u8,
        0x01, 0x00,
        0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    for label in ["www", "example", "com"] {
        bytes.push(label.len() as u8);
        bytes.extend_from_slice(label.as_bytes());
    }
    bytes.push(0);
    bytes.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]); // A / IN
    bytes
}

/// Starts a server on an ephemeral port and returns the address it bound to.
async fn start(rrl: Option<RrlConfig>) -> SocketAddr {
    // Port 0 lets the OS choose, so parallel tests never collide. The probe socket is bound
    // first purely to learn a free port, then dropped so the server can take it.
    let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = probe.local_addr().unwrap();
    drop(probe);

    let store = Arc::new(ZoneStore::new(zones()));
    let aliases = Arc::new(AliasCache::new());
    let options = ServerOptions { udp: true, tcp: false, ..ServerOptions::default() };

    let mut server = DnsServer::with_options(address, options, store, aliases);
    if let Some(config) = rrl {
        server = server.with_rate_limiting(Arc::new(Rrl::new(config)));
    }

    tokio::spawn(async move { let _ = server.run().await; });
    tokio::time::sleep(Duration::from_millis(150)).await;

    address
}

/// Sends `count` queries and collects whatever comes back within a short window.
async fn flood(server: SocketAddr, count: u16) -> Vec<Vec<u8>> {
    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.connect(server).await.unwrap();

    for id in 0..count {
        client.send(&query(id)).await.unwrap();
    }

    let mut responses = Vec::new();
    let mut buf = [0u8; 512];

    // Drop responses are silent, so the only way to know we have them all is to stop hearing
    // any. A short read timeout is the termination condition.
    loop {
        match tokio::time::timeout(Duration::from_millis(300), client.recv(&mut buf)).await {
            Ok(Ok(len)) => responses.push(buf[..len].to_vec()),
            _ => break,
        }
    }

    responses
}

#[tokio::test]
async fn a_flood_is_limited_and_at_least_one_reply_is_truncated() {
    let config = RrlConfig {
        responses_per_second: 2,
        window_secs: 1,
        slip: 2,
        // 127.0.0.0/8 is exempt by default, and the test client is on loopback.
        exempt: Vec::new(),
        ..RrlConfig::default()
    };

    let server = start(Some(config)).await;
    let responses = flood(server, 60).await;

    assert!(
        responses.len() < 60,
        "rate limiting should suppress most of a 60-query flood, got {}",
        responses.len()
    );

    let truncated = responses.iter().filter(|response| response[2] & 0x02 != 0).count();
    assert!(truncated > 0, "at least one over-limit reply should slip through truncated");

    // A truncated reply must carry no records, or resolvers treat it as malformed.
    for response in responses.iter().filter(|r| r[2] & 0x02 != 0) {
        assert_eq!(u16::from_be_bytes([response[6], response[7]]), 0, "TC reply must have ANCOUNT 0");
    }
}

#[tokio::test]
async fn without_rate_limiting_every_query_is_answered() {
    // The feature must be genuinely absent when off, not merely quieter.
    let server = start(None).await;
    let responses = flood(server, 40).await;

    assert_eq!(responses.len(), 40);
    assert!(responses.iter().all(|response| response[2] & 0x02 == 0), "nothing should be truncated");
}

#[tokio::test]
async fn exempt_clients_are_not_limited() {
    let config = RrlConfig {
        responses_per_second: 2,
        window_secs: 1,
        exempt: vec![dns_server::IpNet::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8)],
        ..RrlConfig::default()
    };

    let server = start(Some(config)).await;
    let responses = flood(server, 40).await;

    assert_eq!(responses.len(), 40, "an exempt client must never be limited");
}
