pub mod error;
pub mod server;
pub mod telemetry;
pub mod resolver;
pub mod sync;
pub mod dns;

pub use error::ConfigError;
pub use server::{NetworkConfig, ServerConfig};
pub use telemetry::{LogFormat, TelemetryConfig};
pub use resolver::ResolverConfig;
pub use sync::ZoneSyncConfig;
pub use dns::{apply_zone, build_zones, DnsConfig, RecordConfig, SoaConfig, ZoneConfig};
