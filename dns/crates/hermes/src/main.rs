use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use dns_server::{DnsServer, ServerOptions};
use dns_types::zone::alias::AliasCache;
use dns_types::zone::store::ZoneStore;
use hermes_config::{DnsConfig, NetworkConfig, ServerConfig};
use telemetry::BuildInfo;

mod alias;
mod observability;
mod rate_limit;
mod runtime;
mod zone_sync;

const DEFAULT_SERVER_CONFIG: &str = "config/server.toml";
const DEFAULT_ZONES_CONFIG: &str = "config/zones.toml";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("hermes: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let (server_path, zones_path) = config_paths();

    let server_config = ServerConfig::load(&server_path)
        .with_context(|| format!("loading server config from {}", server_path.display()))?;

    let dns_config = DnsConfig::load(&zones_path)
        .with_context(|| format!("loading zones from {}", zones_path.display()))?;

    let runtime = runtime::build(&server_config.network)?;
    // let _telemetry = observability::init(&runtime, &server_config)?;

    let build = BuildInfo::CURRENT;
    tracing::info!(version = build.version, commit = build.git_commit, profile = build.profile, "starting Hermes");

    let zones = dns_config.build_zones().context("building the zone tree from the config file")?;
    let store = Arc::new(ZoneStore::new(zones));
    tracing::info!(zones = dns_config.zones.len(), path = %zones_path.display(), "loaded zones from file");

    zone_sync::start(&runtime, &server_config, store.clone());

    let aliases = Arc::new(AliasCache::new());
    let _alias_resolver = alias::start(&runtime, &server_config, store.clone(), aliases.clone());

    let mut server = DnsServer::with_options(
        server_config.network.socket_addr(),
        server_options(&server_config.network),
        store,
        aliases,
    );

    let mut _rrl_observers = None;
    if let Some(rrl) = rate_limit::build_limiter(&server_config) {
        _rrl_observers = Some(rate_limit::register_metrics(&rrl));
        server = server.with_rate_limiting(rrl);
    }

    tracing::info!(address = %server_config.network.socket_addr(), "serving DNS");
    runtime.block_on(server.run()).context("the DNS server stopped unexpectedly")?;
    Ok(())
}

fn config_paths() -> (PathBuf, PathBuf) {
    let mut args = std::env::args().skip(1);
    let server = args.next().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_SERVER_CONFIG));
    let zones = args.next().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_ZONES_CONFIG));

    (server, zones)
}

fn server_options(network: &NetworkConfig) -> ServerOptions {
    ServerOptions {
        udp: network.udp,
        tcp: network.tcp,
        tcp_backlog: network.tcp_backlog,
        udp_sockets: network.udp_sockets,
        tcp_idle_timeout: Duration::from_secs(network.tcp_idle_timeout_secs.max(1)),
    }
}
