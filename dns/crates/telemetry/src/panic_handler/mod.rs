use crate::panic_handler::panic_hook::install_panic_hook;
use std::path::PathBuf;

pub mod build_info;
mod crash_context;
pub mod crash_report;
pub mod panic_hook;
pub mod ring_buffer;
pub mod runtime_info;

pub struct CrashReportConfig {
    pub crash_dir: PathBuf,
    pub config_redacted: String,
    pub mode: String,
}

pub fn init_crash_reporting(cfg: CrashReportConfig) {
    tracing::debug!("initializing crash reporting");
    install_panic_hook(cfg);
    tracing::debug!("crash reporting initialized");
}
