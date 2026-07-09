use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use alias_resolver::{AliasResolver, HickoryUpstream, ResolverConfig};
use dns_types::types::Name;
use dns_types::zone::alias::AliasCache;
use dns_types::zone::store::ZoneStore;
use hermes_config::ServerConfig;
use tokio::runtime::Runtime;

const WARMUP_TIMEOUT: Duration = Duration::from_secs(5);
const RECONCILE_INTERVAL: Duration = Duration::from_secs(30);

pub fn start(
    runtime: &Runtime,
    server_config: &ServerConfig,
    store: Arc<ZoneStore>,
    aliases: Arc<AliasCache>,
) -> Option<Arc<AliasResolver<HickoryUpstream>>> {
    let targets = store.load().alias_targets();

    if targets.is_empty() && !server_config.zone_sync.is_active() {
        return None;
    }

    let upstream_ips = server_config.resolver.upstream_ips();
    if upstream_ips.is_empty() {
        tracing::warn!("zones use ALIAS records but no resolver upstreams are configured; ALIAS names will SERVFAIL");
        return None;
    }

    let upstream = {
        let _runtime_guard = runtime.enter();

        match HickoryUpstream::new(&upstream_ips, server_config.resolver.timeout()) {
            Ok(upstream) => upstream,
            Err(error) => {
                tracing::error!(%error, "failed to build ALIAS resolver; ALIAS names will SERVFAIL");

                return None;
            }
        }
    };

    let resolver = AliasResolver::new(aliases, upstream, Some(store.clone()), ResolverConfig::default());

    warm_up(runtime, &resolver, &targets);

    let _runtime_guard = runtime.enter();
    resolver.sync(targets);
    resolver.spawn_reconciler(store, RECONCILE_INTERVAL);

    Some(resolver)
}

fn warm_up(runtime: &Runtime, resolver: &AliasResolver<HickoryUpstream>, targets: &HashSet<Name>) {
    if targets.is_empty() {
        return;
    }

    tracing::info!(targets = targets.len(), "warming up ALIAS targets");

    runtime.block_on(async {
        for target in targets {
            let _ = tokio::time::timeout(WARMUP_TIMEOUT, resolver.resolve_target(target)).await;
        }
    });
}
