use crate::panic_handler::CrashReportConfig;
use crate::panic_handler::build_info::BuildInfo;
use crate::panic_handler::runtime_info::RuntimeInfo;
use std::path::PathBuf;
use time::OffsetDateTime;

pub struct CrashContext {
    pub crash_dir: PathBuf,
    pub config_redacted: String,
    pub build_info: BuildInfo,
    pub runtime_info: RuntimeInfo,
    pub mode: String,
    pub start_time: OffsetDateTime,
}

impl CrashContext {
    pub fn new(cfg: &CrashReportConfig) -> Self {
        Self {
            crash_dir: cfg.crash_dir.clone(),
            config_redacted: cfg.config_redacted.clone(),
            build_info: BuildInfo::CURRENT,
            runtime_info: RuntimeInfo::capture(),
            mode: cfg.mode.clone(),
            start_time: OffsetDateTime::now_utc(),
        }
    }
}
