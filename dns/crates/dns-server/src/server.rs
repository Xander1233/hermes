use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use dns_types::zone::store::ZoneStore;
use dns_types::zone::alias::AliasCache;
use crate::rrl::Rrl;
use crate::{tcp, udp};

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub udp: bool,
    pub tcp: bool,
    pub tcp_backlog: i32,
    pub udp_sockets: usize,
    pub tcp_idle_timeout: std::time::Duration,
}

impl Default for ServerOptions {
    fn default() -> Self {
        ServerOptions {
            udp: true,
            tcp: true,
            tcp_backlog: 1024,
            udp_sockets: 1,
            tcp_idle_timeout: std::time::Duration::from_secs(10),
        }
    }
}

pub struct DnsServer {
    address: SocketAddr,
    options: ServerOptions,
    zones: Arc<ZoneStore>,
    aliases: Arc<AliasCache>,
    rrl: Option<Arc<Rrl>>,
}

impl DnsServer {
    pub fn new(address: SocketAddr, zones: Arc<ZoneStore>, aliases: Arc<AliasCache>) -> Self {
        DnsServer { address, options: ServerOptions::default(), zones, aliases, rrl: None }
    }

    pub fn with_options(address: SocketAddr, options: ServerOptions, zones: Arc<ZoneStore>, aliases: Arc<AliasCache>) -> Self {
        DnsServer { address, options, zones, aliases, rrl: None }
    }

    pub fn with_rate_limiting(mut self, rrl: Arc<Rrl>) -> Self {
        self.rrl = Some(rrl);
        self
    }

    pub async fn run(self) -> std::io::Result<()> {
        let shutdown = CancellationToken::new();
        let query_tasks = TaskTracker::new();
        let mut accept_loops: Vec<JoinHandle<()>> = Vec::new();

        if self.options.udp {
            let socket_count = self.udp_socket_count();

            for _ in 0..socket_count {
                let socket = Arc::new(udp::socket::create_socket(self.address)?);

                accept_loops.push(tokio::spawn(udp::udp_loop(
                    socket,
                    self.zones.clone(),
                    self.aliases.clone(),
                    self.rrl.clone(),
                    query_tasks.clone(),
                    shutdown.clone(),
                )));
            }

            tracing::info!(
                rate_limited = self.rrl.is_some(),
                "listening for UDP queries on {} ({} socket(s))",
                self.address,
                socket_count
            );
        }

        if self.options.tcp {
            let listener = tcp::socket::create_socket(self.address, self.options.tcp_backlog)?;

            tracing::info!("listening for TCP queries on {}", self.address);

            accept_loops.push(tokio::spawn(tcp::tcp_loop(
                listener,
                self.zones.clone(),
                self.aliases.clone(),
                self.options.tcp_idle_timeout,
                query_tasks.clone(),
                shutdown.clone(),
            )));
        }

        if accept_loops.is_empty() {
            tracing::warn!("both UDP and TCP are disabled; the server has nothing to serve");

            return Ok(());
        }

        wait_for_shutdown_signal().await;
        tracing::info!("shutdown signal received; no longer accepting queries, draining in-flight work");
        shutdown.cancel();

        for accept_loop in accept_loops {
            let _ = accept_loop.await;
        }

        query_tasks.close();
        query_tasks.wait().await;

        tracing::info!("in-flight work drained; shutdown complete");
        Ok(())
    }

    #[cfg(unix)]
    fn udp_socket_count(&self) -> usize {
        match self.options.udp_sockets {
            0 => std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
            n => n,
        }
    }

    #[cfg(not(unix))]
    fn udp_socket_count(&self) -> usize {
        if self.options.udp_sockets > 1 {
            tracing::warn!(
                "udp_sockets = {} requested, but SO_REUSEPORT is unavailable on this platform; using 1",
                self.options.udp_sockets
            );
        }

        1
    }
}

async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                term.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
