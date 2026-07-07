use std::sync::Arc;
use std::time::Instant;
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::Instrument;
use dns_types::zone::store::ZoneStore;
use dns_types::zone::alias::AliasCache;
use crate::rrl::{ResponseClass, Rrl, RrlDecision};
use crate::udp::handler::handle_connection;

pub mod socket;
pub mod handler;

pub async fn udp_loop(
    socket: Arc<UdpSocket>,
    store: Arc<ZoneStore>,
    aliases: Arc<AliasCache>,
    rrl: Option<Arc<Rrl>>,
    tasks: TaskTracker,
    shutdown: CancellationToken,
) {
    let mut buf = [0u8; 512];

    loop {
        let (len, addr) = tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            received = socket.recv_from(&mut buf) => match received {
                Ok(datagram) => datagram,
                Err(_) => continue,
            },
        };

        let socket = socket.clone();
        let zones = store.load();
        let aliases = aliases.clone();
        let rrl = rrl.clone();
        let message = buf[..len].to_vec();

        let span = tracing::info_span!("dns_query", transport = "udp", client = %addr);
        
        tasks.spawn(
            async move {
                let handled = match handle_connection(&message, zones, aliases).await {
                    Ok(handled) => handled,
                    Err(error) => {
                        tracing::error!("error handling UDP query from {}: {}", addr, error);
                        
                        return;
                    }
                };

                let class = ResponseClass::from_rcode(u8::from(handled.rcode));
                let decision = match &rrl {
                    Some(rrl) => rrl.check(addr, class, handled.qname.as_ref(), Instant::now()),
                    None => RrlDecision::Send,
                };

                let response = match decision {
                    RrlDecision::Send => handled.bytes,

                    RrlDecision::Slip => {
                        match dns_types::truncated_response(&handled.bytes) {
                            Some(truncated) => {
                                telemetry::metrics().record_rate_limited("udp", "slipped", class.as_str());
                                truncated
                            }
                            None => {
                                telemetry::metrics().record_rate_limited("udp", "dropped", class.as_str());
                                return;
                            }
                        }
                    }

                    RrlDecision::Drop => {
                        telemetry::metrics().record_rate_limited("udp", "dropped", class.as_str());
                        tracing::debug!(client = %addr, "response rate limited");
                        
                        return;
                    }
                };

                let _ = socket.send_to(&response, addr).await;
            }
            .instrument(span),
        );
    }
}
