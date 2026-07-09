use anyhow::Context;
use hermes_config::ServerConfig;
use telemetry::{CrashReportConfig, Telemetry};
use tokio::runtime::Runtime;

const CRASH_SUBDIR: &str = "crashes";
const SERVICE_MODE: &str = "dns";
const UNSERIALIZABLE_CONFIG: &str = "<unserializable config>";

pub fn init(runtime: &Runtime, config: &ServerConfig) -> anyhow::Result<Telemetry> {
    let telemetry = {
        let _runtime_guard = runtime.enter();
        telemetry::init(&config.telemetry).context("initializing telemetry")?
    };

    telemetry::init_crash_reporting(CrashReportConfig {
        crash_dir: config.telemetry.logs_dir.join(CRASH_SUBDIR),
        config_redacted: serde_json::to_string_pretty(config).unwrap_or_else(|_| UNSERIALIZABLE_CONFIG.into()),
        mode: SERVICE_MODE.to_string(),
    });

    Ok(telemetry)
}
