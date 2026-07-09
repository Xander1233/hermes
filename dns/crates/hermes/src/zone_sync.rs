use std::sync::Arc;

use dns_types::zone::store::ZoneStore;
use hermes_config::ServerConfig;
use tokio::runtime::Runtime;
use zone_sync::{AdminState, HttpZoneSource, ZoneSync};

pub fn start(runtime: &Runtime, server_config: &ServerConfig, store: Arc<ZoneStore>) {
    let config = &server_config.zone_sync;
    if !config.is_active() {
        tracing::info!("zone sync disabled; serving zones from the local file only");
        return;
    }

    let source = match HttpZoneSource::new(&config.base_url, config.auth_token.clone(), config.request_timeout()) {
        Ok(source) => source,
        Err(error) => {
            tracing::error!(%error, "could not build zone sync client; serving zones from the local file only");
            return;
        }
    };

    let sync = ZoneSync::new(store, source, Some(config.snapshot_path.clone()), config.fetch_concurrency);

    initial_pull(runtime, &sync);

    let _runtime_guard = runtime.enter();

    tokio::spawn(sync.clone().run(config.poll_interval()));

    start_admin_listener(server_config, &sync);
}

fn initial_pull(runtime: &Runtime, sync: &Arc<ZoneSync<HttpZoneSource>>) {
    runtime.block_on(async {
        match sync.sync_once().await {
            Ok(outcome) => {
                tracing::info!(?outcome, "initial zone sync complete");
                return;
            }
            Err(error) => tracing::warn!(%error, "initial zone sync failed; trying the on-disk snapshot"),
        }

        match sync.load_disk_snapshot() {
            Ok(true) => tracing::info!(zones = sync.status().tracked, "loaded zones from disk snapshot"),
            Ok(false) => tracing::warn!("no disk snapshot; serving zones from the local file"),
            Err(error) => tracing::error!(%error, "could not read disk snapshot; serving zones from the local file"),
        }
    });
}

fn start_admin_listener(server_config: &ServerConfig, sync: &Arc<ZoneSync<HttpZoneSource>>) {
    let config = &server_config.zone_sync;
    let Some(listen) = &config.admin_listen else {
        return;
    };

    let addr = match listen.parse::<std::net::SocketAddr>() {
        Ok(addr) => addr,
        Err(error) => {
            tracing::error!(%error, listen, "invalid admin_listen address; admin endpoint disabled");
            return;
        }
    };

    if config.admin_token.is_none() {
        tracing::warn!("admin_listen is set without admin_token; POST /admin/reload will be refused");
    }

    let trigger_sync = sync.clone();
    let status_sync = sync.clone();
    let state = AdminState::new(
        Arc::new(move || trigger_sync.trigger()),
        Arc::new(move || status_sync.status()),
        config.admin_token.clone(),
    );

    tokio::spawn(async move {
        if let Err(error) = zone_sync::admin::serve(addr, state).await {
            tracing::error!(%error, "admin listener stopped");
        }
    });
}
