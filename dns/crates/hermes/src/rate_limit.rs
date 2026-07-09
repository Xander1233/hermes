use std::sync::Arc;

use dns_server::{IpNet, Rrl, RrlConfig};
use hermes_config::ServerConfig;
use opentelemetry::metrics::{ObservableCounter, ObservableGauge};

pub struct RrlObservers {
    _occupancy: ObservableGauge<f64>,
    _collisions: ObservableCounter<u64>,
}

pub fn build_limiter(server_config: &ServerConfig) -> Option<Arc<Rrl>> {
    let config = &server_config.rate_limit;

    if !config.enabled {
        tracing::warn!(
            "response rate limiting is disabled; on a public address this server can be used \
             as a DDoS amplification reflector"
        );

        return None;
    }

    let exempt = config
        .exempt
        .iter()
        .filter_map(|entry| match IpNet::parse(entry) {
            Some(net) => Some(net),
            None => {
                tracing::error!(entry, "ignoring unparseable CIDR in rate_limit.exempt");
                None
            }
        })
        .collect();

    let rrl = Rrl::new(RrlConfig {
        enabled: true,
        responses_per_second: config.responses_per_second,
        all_per_second: config.all_per_second,
        nxdomains_per_second: config.nxdomains_per_second,
        errors_per_second: config.errors_per_second,
        window_secs: config.window_secs,
        slip: config.slip,
        ipv4_prefix_len: config.ipv4_prefix_len,
        ipv6_prefix_len: config.ipv6_prefix_len,
        table_size: config.table_size,
        exempt,
    });

    tracing::info!(
        responses_per_second = config.responses_per_second,
        all_per_second = config.all_per_second,
        nxdomains_per_second = config.nxdomains_per_second,
        slip = config.slip,
        "response rate limiting enabled (UDP only)"
    );

    Some(Arc::new(rrl))
}

pub fn register_metrics(rrl: &Arc<Rrl>) -> RrlObservers {
    let meter = opentelemetry::global::meter("hermes");

    let occupancy_rrl = rrl.clone();
    let occupancy = meter
        .f64_observable_gauge("dns.rrl.table_occupancy")
        .with_description("Fraction of RRL bucket slots in use (0–1); near 1 means the table is saturated")
        .with_callback(move |observer| {
            let (used, total) = occupancy_rrl.occupancy();
            let fraction = if total == 0 { 0.0 } else { used as f64 / total as f64 };
            observer.observe(fraction, &[]);
        })
        .build();

    let collisions_rrl = rrl.clone();
    let collisions = meter
        .u64_observable_counter("dns.rrl.collisions")
        .with_description("Cumulative RRL slot collisions (unrelated clients sharing a bucket)")
        .with_callback(move |observer| observer.observe(collisions_rrl.collisions(), &[]))
        .build();

    RrlObservers { _occupancy: occupancy, _collisions: collisions }
}
