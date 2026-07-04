use crate::panic_handler::crash_context::CrashContext;
use crate::panic_handler::ring_buffer::RECENT_LOGS;
use humantime_serde::re::humantime::format_duration;
use std::backtrace::Backtrace;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::panic::PanicHookInfo;
use std::time::Duration;
use time::OffsetDateTime;
use uuid::Uuid;

pub(crate) fn write_crash_report(ctx: &CrashContext, info: &PanicHookInfo) -> std::io::Result<()> {
    fs::create_dir_all(&ctx.crash_dir)?;

    let now = OffsetDateTime::now_utc();
    let crash_id = Uuid::new_v4();
    let timestamp = now.format(&time::format_description::well_known::Iso8601::DEFAULT).unwrap_or_else(|_| "unknown_time".into());

    let short_id = &crash_id.to_string()[..6];
    let filename = format!("crash-{}-{}.log", now.format(&time::macros::format_description!("[year][month][day]T[hour][minute][second]Z")).unwrap_or_else(|_| "unknown_time".into()), short_id);

    let path = ctx.crash_dir.join(&filename);
    let mut f = File::create(&path)?;

    writeln!(f, "{}", "=".repeat(80))?;
    writeln!(f, "HERMES DNS SERVER CRASH REPORT")?;
    writeln!(f, "{}", "=".repeat(80))?;
    writeln!(f)?;
    writeln!(f, "This file was generated automatically when the hermes process")?;
    writeln!(f, "encountered an unrecoverable error (panic). The process has terminated")?;
    writeln!(f)?;
    writeln!(f, "To report this bug, please open an issue at:")?;
    writeln!(f, "   https://github.com/Xander1233/Hermes/issues/new")?;
    writeln!(f)?;
    writeln!(f, "Please attach this file. It does not contain credentials or")?;
    writeln!(f, "customer data, but you may want to review it before sharing.")?;
    writeln!(f)?;

    let location = info.location().map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column())).unwrap_or_else(|| "<unknown>".into());
    let message = info.payload().downcast_ref::<&str>().copied().or_else(|| info.payload().downcast_ref::<String>().map(String::as_str)).unwrap_or("<non-string panic payload>");

    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("<unnamed>");

    writeln!(f, "{}", "=".repeat(80))?;
    writeln!(f, "SUMMARY")?;
    writeln!(f, "{}", "=".repeat(80))?;
    writeln!(f)?;
    writeln!(f, "Timestamp:     {}", timestamp)?;
    writeln!(f, "Crash ID:      {}", crash_id)?;
    writeln!(f, "Subsystem:     {}", ctx.mode)?;
    writeln!(f, "Thread:        {} (id: {:?})", thread_name, thread.id())?;
    writeln!(f, "Location:      {}", location)?;
    writeln!(f, "Message:       {}", message)?;
    writeln!(f)?;

    write_section(&mut f, "BUILD INFO", &ctx.build_info.to_string())?;

    let uptime = OffsetDateTime::now_utc() - ctx.start_time;
    let mut runtime_block = ctx.runtime_info.to_redacted_string();
    runtime_block.push_str(&format!("\nMode:          {}\n", ctx.mode));
    runtime_block.push_str(&format!("Uptime:        {}\n", format_duration(Duration::from_secs_f64(uptime.as_seconds_f64().max(0.0)))));
    write_section(&mut f, "RUNTIME INFO", &runtime_block)?;

    writeln!(f, "{}", "=".repeat(80))?;
    writeln!(f, "CONFIG SUMMARY (redacted)")?;
    writeln!(f, "{}", "=".repeat(80))?;
    writeln!(f)?;
    writeln!(f, "Sensitive values (passwords, tokens, keys, connection strings) are")?;
    writeln!(f, "replaced with <redacted>. The structure and non-sensitive values are")?;
    writeln!(f, "preserved to help diagnose configuration-related crashes.")?;
    writeln!(f)?;
    writeln!(f, "{}", ctx.config_redacted)?;
    writeln!(f)?;

    writeln!(f, "{}", "=".repeat(80))?;
    writeln!(f, "RECENT LOG EVENTS (last 50, oldest first)")?;
    writeln!(f, "{}", "=".repeat(80))?;
    writeln!(f)?;

    if let Some(logs) = RECENT_LOGS.get() {
        if let Ok(logs) = logs.lock() {
            if logs.iter().count() == 0 {
                writeln!(f, "<no log events captured>")?;
            } else {
                for log in logs.iter() {
                    writeln!(f, "{}", log)?;
                }
            }
        } else {
            writeln!(f, "<failed to acquire log lock>")?;
        }
    }
    writeln!(f)?;

    let backtrace = Backtrace::force_capture();
    writeln!(f, "{}", "=".repeat(80))?;
    writeln!(f, "PANIC DETAILS")?;
    writeln!(f, "{}", "=".repeat(80))?;
    writeln!(f)?;
    writeln!(f, "Panic message:")?;
    writeln!(f, "   {}", message)?;
    writeln!(f)?;
    writeln!(f, "Panic location:")?;
    if let Some(loc) = info.location() {
        writeln!(f, "   file: {}", loc.file())?;
        writeln!(f, "   line: {}", loc.line())?;
        writeln!(f, "   column: {}", loc.column())?;
    }
    writeln!(f)?;
    writeln!(f, "Backtrace:")?;
    writeln!(f, "{}", backtrace)?;
    writeln!(f)?;

    write_disclosure_footer(&mut f)?;

    f.flush()?;

    eprintln!();
    eprintln!("=== A crash report has been written to: ===");
    eprintln!("    {}", path.display());
    eprintln!();
    eprintln!("    Please review the contents before sharing.");
    eprintln!("    It should not contain secrets, but may contain");
    eprintln!("    technical details about your environment and configuration.");
    eprintln!();
    eprintln!("=== Please include it when reporting this bug. ===");
    eprintln!();

    let _ = fs::remove_file(ctx.crash_dir.join("last-crash.log"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let _ = symlink(&path, ctx.crash_dir.join("last-crash.log"));
    }

    Ok(())
}

fn write_section(f: &mut File, title: &str, content: &str) -> std::io::Result<()> {
    writeln!(f, "{}", "=".repeat(80))?;
    writeln!(f, "{}", title)?;
    writeln!(f, "{}", "=".repeat(80))?;
    writeln!(f)?;
    writeln!(f, "{}", content)?;
    writeln!(f)?;
    Ok(())
}

fn write_disclosure_footer(f: &mut File) -> std::io::Result<()> {
    let disclaimer = "This crash report is intended to help diagnose bugs. It contains:\n\n\
\
   - The exact panic message and source location\n\
   - A stack backtrace showing what code was running\n\
   - Build information (version, commit, target)\n\
   - Runtime environment (OS, architecture, memory)\n\
   - Your configuration with all secrets redacted\n\
   - The last 50 log lines before the crash\n\n\
\
This file does NOT contain:\n\n\
\
   - Passwords, API secrets, or other credentials\n\
   - HTTP request bodies or response data\n\
   - Email addresses, names, or other PII\n\n\
\
The hostname has been redacted. If your config file contains custom fields\n\
not known to this version, they appear with values intact -- review before\n\
sharing if you have non-standard config.\
    ";

    write_section(f, "WHAT'S IN THIS FILE", disclaimer)?;

    Ok(())
}
