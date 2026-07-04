use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
    println!("cargo:rerun-if-env-changed=PROFILE");
    println!("cargo:rerun-if-env-changed=TARGET");

    let git_commit = git(&["rev-parse", "--short=8", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let git_dirty = git(&["status", "--porcelain"]).map(|s| !s.is_empty()).unwrap_or(false);
    let git_branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|s| s != "HEAD"); // detached HEAD case

    let timestamp = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(|secs| time::OffsetDateTime::from_unix_timestamp(secs).ok())
        .unwrap_or_else(time::OffsetDateTime::now_utc)
        .format(&time::format_description::well_known::Iso8601::DEFAULT)
        .unwrap_or_else(|_| "unknown".into());

    let rustc = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".into());
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".into());

    println!("cargo:rustc-env=HERMES_GIT_COMMIT={git_commit}");
    println!("cargo:rustc-env=HERMES_GIT_DIRTY={}", if git_dirty { "1" } else { "0" });
    if let Some(branch) = git_branch {
        println!("cargo:rustc-env=HERMES_GIT_BRANCH={branch}");
    }
    println!("cargo:rustc-env=HERMES_BUILD_TIMESTAMP={timestamp}");
    println!("cargo:rustc-env=HERMES_RUSTC_VERSION={rustc}");
    println!("cargo:rustc-env=HERMES_PROFILE={profile}");
    println!("cargo:rustc-env=HERMES_TARGET={target}");
}

fn git(args: &[&str]) -> Option<String> {
    Command::new("git").args(args).output().ok().filter(|o| o.status.success()).and_then(|o| String::from_utf8(o.stdout).ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}
