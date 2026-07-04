use std::fmt;

const fn is_truthy(value: Option<&'static str>) -> bool {
    match value {
        Some(v) => {
            let bytes = v.as_bytes();
            bytes.len() == 1 && bytes[0] == b'1'
        }
        None => false,
    }
}

const fn unwrap_or(value: Option<&'static str>, default: &'static str) -> &'static str {
    match value {
        Some(v) => v,
        None => default,
    }
}

#[derive(Debug, Clone)]
pub struct BuildInfo {
    pub version: &'static str,
    pub git_commit: &'static str,
    pub git_dirty: bool,
    pub git_branch: Option<&'static str>,
    pub build_timestamp: &'static str,
    pub rustc_version: &'static str,
    pub profile: &'static str,
    pub target: &'static str,
}

impl BuildInfo {
    pub const CURRENT: BuildInfo = BuildInfo {
        version: env!("CARGO_PKG_VERSION"),
        git_commit: unwrap_or(option_env!("HERMES_GIT_COMMIT"), "unknown"),
        git_dirty: is_truthy(option_env!("HERMES_GIT_DIRTY")),
        git_branch: option_env!("HERMES_GIT_BRANCH"),
        build_timestamp: unwrap_or(option_env!("HERMES_BUILD_TIMESTAMP"), "unknown"),
        rustc_version: unwrap_or(option_env!("HERMES_RUSTC_VERSION"), "unknown"),
        profile: unwrap_or(option_env!("HERMES_PROFILE"), "unknown"),
        target: unwrap_or(option_env!("HERMES_TARGET"), "unknown"),
    };
}

impl fmt::Display for BuildInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dirty_marker = if self.git_dirty { " (dirty)" } else { " (clean)" };
        let branch = self.git_branch.map(|b| format!(" on {b}")).unwrap_or_default();

        writeln!(f, "Version:       {}", self.version)?;
        writeln!(f, "Git commit:    {}{}{}", self.git_commit, dirty_marker, branch)?;
        writeln!(f, "Build date:    {}", self.build_timestamp)?;
        writeln!(f, "Rust version:  {}", self.rustc_version)?;
        writeln!(f, "Profile:       {}", self.profile)?;
        writeln!(f, "Target:        {}", self.target)?;
        
        Ok(())
    }
}
