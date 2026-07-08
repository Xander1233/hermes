use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::Instrument;
use dns_types::zone::store::ZoneStore;
use dns_types::zone::alias::AliasCache;
use crate::tcp::handler::handle_connection;

pub mod socket;
pub mod handler;

pub async fn tcp_loop(
    listener: TcpListener,
    store: Arc<ZoneStore>,
    aliases: Arc<AliasCache>,
    idle_timeout: Duration,
    tasks: TaskTracker,
    shutdown: CancellationToken,
) {
    loop {
        let (stream, addr) = tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            accepted = listener.accept() => match accepted {
                Ok(connection) => connection,
                Err(_) => continue,
            },
        };
        
        tracing::debug!("accepted TCP connection from {}", addr);

        let zones_clone = store.load();
        let aliases_clone = aliases.clone();
        let connection_shutdown = shutdown.clone();

        let span = tracing::info_span!("dns_connection", transport = "tcp", client = %addr);
        
        tasks.spawn(
            async move {
                if let Err(error) = handle_connection(stream, zones_clone, aliases_clone, idle_timeout, connection_shutdown).await {
                    tracing::debug!("TCP connection from {} ended: {}", addr, error);
                }
            }
            .instrument(span),
        );
    }
}