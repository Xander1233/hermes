mod error;
mod metrics;
mod panic_handler;

pub use error::{TelemetryError, TelemetryResult};
pub use metrics::{metrics, DnsMetrics};
pub use panic_handler::build_info::BuildInfo;
pub use panic_handler::{init_crash_reporting, CrashReportConfig};

use crate::panic_handler::ring_buffer::{BoundedRingBuffer, RingBufferLayer};
use hermes_config::{LogFormat, TelemetryConfig};
use opentelemetry::{global, KeyValue};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::{
    Resource,
    logs::SdkLoggerProvider,
    metrics::{PeriodicReader, SdkMeterProvider, Temporality},
    trace::{Sampler, SdkTracerProvider},
};
use std::time::Duration;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, Layer, Registry, prelude::*};

type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync>;

pub struct Telemetry {
    pub tracer_provider: SdkTracerProvider,
    pub logger_provider: SdkLoggerProvider,
    pub meter_provider: SdkMeterProvider,
    ring_layer: RingBufferLayer,
    _file_guard: Option<WorkerGuard>,
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        let _ = self.ring_layer.flush();
        let _ = self.tracer_provider.shutdown();
        let _ = self.logger_provider.shutdown();
        let _ = self.meter_provider.shutdown();
    }
}

pub fn init(cfg: &TelemetryConfig) -> anyhow::Result<Telemetry> {
    let service_name = cfg.service_name.clone().unwrap_or_else(|| env!("CARGO_PKG_NAME").to_string());

    let resource = Resource::builder()
        .with_service_name(service_name.clone())
        .with_attributes([
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("deployment.environment", std::env::var("ENV").unwrap_or_else(|_| "dev".into())),
        ])
        .build();

    let mut tracer_provider = SdkTracerProvider::builder().build();
    let mut logger_provider = SdkLoggerProvider::builder().build();
    let mut meter_provider = SdkMeterProvider::builder().build();

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&cfg.level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let ring_layer = RingBufferLayer::init(BoundedRingBuffer::new(50));

    let mut layers: Vec<BoxedLayer> = Vec::new();
    layers.push(ring_layer.clone().boxed());

    let file_guard = if cfg.file_logging {
        std::fs::create_dir_all(&cfg.logs_dir)?;
        
        let appender = tracing_appender::rolling::daily(&cfg.logs_dir, format!("{}.log", cfg.log_file_prefix));
        let (writer, guard) = tracing_appender::non_blocking(appender);
        
        layers.push(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(writer).boxed());
        
        Some(guard)
    } else {
        None
    };

    match cfg.log_format {
        LogFormat::Json => layers.push(tracing_subscriber::fmt::layer().json().boxed()),
        LogFormat::Pretty => layers.push(tracing_subscriber::fmt::layer().pretty().boxed()),
    }

    if let Some(endpoint) = &cfg.otlp_endpoint {
        let endpoint = endpoint.clone();

        let span_exporter = SpanExporter::builder().with_tonic().with_endpoint(endpoint.clone()).build()?;
        let ratio = cfg.trace_sample_ratio.clamp(0.0, 1.0);
        let sampler = Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(ratio)));
        
        tracer_provider = SdkTracerProvider::builder().with_resource(resource.clone()).with_sampler(sampler).with_batch_exporter(span_exporter).build();
        global::set_tracer_provider(tracer_provider.clone());
        let tracer = opentelemetry::trace::TracerProvider::tracer(&tracer_provider, service_name.clone());

        let log_exporter = LogExporter::builder().with_tonic().with_endpoint(endpoint.clone()).build()?;
        logger_provider = SdkLoggerProvider::builder().with_resource(resource.clone()).with_batch_exporter(log_exporter).build();

        let metric_exporter = MetricExporter::builder().with_tonic().with_endpoint(endpoint).with_temporality(Temporality::Cumulative).build()?;
        let interval = Duration::from_secs(cfg.metric_export_interval_secs.max(1));
        let reader = PeriodicReader::builder(metric_exporter).with_interval(interval).build();
        meter_provider = SdkMeterProvider::builder().with_resource(resource).with_reader(reader).build();
        global::set_meter_provider(meter_provider.clone());

        layers.push(tracing_opentelemetry::layer().with_tracer(tracer).boxed());
        layers.push(OpenTelemetryTracingBridge::new(&logger_provider).boxed());
    }

    tracing_subscriber::registry().with(layers).with(env_filter).init();

    if cfg.otlp_endpoint.is_none() {
        tracing::warn!("OTLP endpoint not configured; traces and metrics will not be exported");
    }

    Ok(Telemetry {
        tracer_provider,
        logger_provider,
        meter_provider,
        ring_layer,
        _file_guard: file_guard,
    })
}
