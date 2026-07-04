use std::sync::OnceLock;
use opentelemetry::metrics::{Counter, Gauge, Histogram};
use opentelemetry::{global, KeyValue};

pub struct DnsMetrics {
    queries: Counter<u64>,
    responses: Counter<u64>,
    answers: Counter<u64>,
    errors: Counter<u64>,
    rate_limited: Counter<u64>,
    duration_ms: Histogram<f64>,
    request_bytes: Histogram<u64>,
    response_bytes: Histogram<u64>,
    zone_count: Gauge<u64>,
    zone_reloads: Counter<u64>,
    zone_last_sync: Gauge<u64>,
}

impl DnsMetrics {
    fn new() -> Self {
        let meter = global::meter("hermes");
        DnsMetrics {
            queries: meter
                .u64_counter("dns.queries")
                .with_description("Number of DNS queries received")
                .build(),
            responses: meter
                .u64_counter("dns.responses")
                .with_description("Number of DNS responses sent, by response code")
                .build(),
            answers: meter
                .u64_counter("dns.answers")
                .with_description("Responses by how they were answered (answer, wildcard, cname, nxdomain, ...)")
                .build(),
            errors: meter
                .u64_counter("dns.errors")
                .with_description("Number of queries that could not be processed")
                .build(),
            rate_limited: meter
                .u64_counter("dns.rate_limited")
                .with_description("Responses suppressed by rate limiting, by action and response class")
                .build(),
            duration_ms: meter
                .f64_histogram("dns.query.duration")
                .with_unit("ms")
                .with_description("DNS query handling latency in milliseconds")
                .build(),
            request_bytes: meter
                .u64_histogram("dns.request.size")
                .with_unit("By")
                .with_description("DNS query size on the wire, in bytes")
                .build(),
            response_bytes: meter
                .u64_histogram("dns.response.size")
                .with_unit("By")
                .with_description("DNS response size on the wire, in bytes")
                .build(),
            zone_count: meter
                .u64_gauge("dns.zone.count")
                .with_description("Number of zones currently loaded and served")
                .build(),
            zone_reloads: meter
                .u64_counter("dns.zone.reloads")
                .with_description("Zone propagation passes, by result (applied, up_to_date, failed, skipped)")
                .build(),
            zone_last_sync: meter
                .u64_gauge("dns.zone.last_sync_timestamp")
                .with_unit("s")
                .with_description("Unix time of the last successful zone sync")
                .build(),
        }
    }

    pub fn record_query(&self, transport: &'static str, qtype: u16) {
        self.queries.add(1, &[
            KeyValue::new("transport", transport),
            KeyValue::new("qtype", i64::from(qtype)),
        ]);
    }

    pub fn record_response(
        &self,
        transport: &'static str,
        rcode: u8,
        kind: &'static str,
        request_bytes: usize,
        response_bytes: usize,
        duration_ms: f64,
    ) {
        let transport_kv = KeyValue::new("transport", transport);

        self.responses.add(1, &[transport_kv.clone(), KeyValue::new("rcode", i64::from(rcode))]);
        self.answers.add(1, &[transport_kv.clone(), KeyValue::new("kind", kind)]);
        self.duration_ms.record(duration_ms, &[transport_kv.clone()]);
        self.request_bytes.record(request_bytes as u64, &[transport_kv.clone()]);
        self.response_bytes.record(response_bytes as u64, &[transport_kv]);
    }

    pub fn record_error(&self, transport: &'static str) {
        self.errors.add(1, &[KeyValue::new("transport", transport)]);
    }

    pub fn record_rate_limited(&self, transport: &'static str, action: &'static str, class: &'static str) {
        self.rate_limited.add(1, &[
            KeyValue::new("transport", transport),
            KeyValue::new("action", action),
            KeyValue::new("class", class),
        ]);
    }

    pub fn record_zone_sync(&self, result: &'static str, count: usize, synced_ok: bool) {
        self.zone_reloads.add(1, &[KeyValue::new("result", result)]);
        self.zone_count.record(count as u64, &[]);
        
        if synced_ok {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            
            self.zone_last_sync.record(now, &[]);
        }
    }

    pub fn record_zones_skipped(&self, n: usize) {
        if n > 0 {
            self.zone_reloads.add(n as u64, &[KeyValue::new("result", "skipped")]);
        }
    }
}

pub fn metrics() -> &'static DnsMetrics {
    static METRICS: OnceLock<DnsMetrics> = OnceLock::new();
    METRICS.get_or_init(DnsMetrics::new)
}
