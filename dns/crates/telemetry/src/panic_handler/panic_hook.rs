use crate::panic_handler::CrashReportConfig;
use crate::panic_handler::crash_context::CrashContext;
use crate::panic_handler::crash_report::write_crash_report;
use std::backtrace::Backtrace;
use std::panic;
use std::panic::PanicHookInfo;
use std::sync::Arc;

pub fn install_panic_hook(cfg: CrashReportConfig) {
    let ctx = Arc::new(CrashContext::new(&cfg));
    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |info| {
        tracing_log_panic(info);

        let _ = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            if let Err(e) = write_crash_report(&ctx, info) {
                eprintln!("Failed to write crash report: {e}");
            }
        }));

        default_hook(info);
    }));
}

fn tracing_log_panic(info: &PanicHookInfo) {
    let location = info.location().map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column())).unwrap_or_else(|| "<unknown>".into());
    let payload = info.payload().downcast_ref::<&str>().copied().or_else(|| info.payload().downcast_ref::<String>().map(String::as_str)).unwrap_or("<non-string panic payload>");

    let backtrace = Backtrace::force_capture();

    tracing::error!(
        target: "panic",
        location = %location,
        backtrace = %backtrace,
        "PANIC: {}", payload,
    );
}
