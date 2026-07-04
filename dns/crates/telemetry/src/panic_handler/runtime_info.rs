use std::fmt;

#[derive(Debug, Clone)]
pub struct RuntimeInfo {
    pub os: String,
    pub distribution: Option<String>,
    pub arch: &'static str,
    pub cpu_cores: usize,
    pub total_memory_bytes: u64,
    pub hostname: Option<String>,
    pub container: ContainerRuntime,
    pub pid: u32,
    pub uid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerRuntime {
    None,
    Docker,
    Podman,
    Kubernetes,
    Lxc,
    Unknown,
}

impl ContainerRuntime {
    fn detect() -> Self {
        if std::env::var_os("KUBERNETES_SERVICE_HOST").is_some() {
            return Self::Kubernetes;
        }

        if std::path::Path::new("/.dockerenv").exists() {
            return Self::Docker;
        }

        if std::path::Path::new("/run/.containerenv").exists() {
            return Self::Podman;
        }

        if let Ok(cgroup) = std::fs::read_to_string("/proc/1/cgroup") {
            if cgroup.contains("docker") {
                return Self::Docker;
            }

            if cgroup.contains("kubepods") {
                return Self::Kubernetes;
            }

            if cgroup.contains("lxc") {
                return Self::Lxc;
            }

            if cgroup.contains("containerd") {
                return Self::Unknown;
            }
        }

        Self::None
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none detected",
            Self::Docker => "Docker",
            Self::Podman => "Podman",
            Self::Kubernetes => "Kubernetes",
            Self::Lxc => "LXC",
            Self::Unknown => "container (unknown runtime)",
        }
    }
}

impl RuntimeInfo {
    pub fn capture() -> Self {
        let os_info = os_info::get();
        let os = format!("{} {}", os_info.os_type(), os_info.version());

        let distribution = match os_info.os_type() {
            os_info::Type::Linux => read_os_release_pretty_name(),
            _ => None,
        };

        Self {
            os,
            distribution,
            arch: std::env::consts::ARCH,
            cpu_cores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
            total_memory_bytes: detect_total_memory(),
            hostname: hostname::get().ok().and_then(|h| h.into_string().ok()),
            container: ContainerRuntime::detect(),
            pid: std::process::id(),
            uid: get_uid(),
        }
    }

    pub fn fmt_redacted(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_inner(f, true)
    }

    pub fn to_redacted_string(&self) -> String {
        struct Redacted<'a>(&'a RuntimeInfo);

        impl fmt::Display for Redacted<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt_redacted(f)
            }
        }

        Redacted(self).to_string()
    }

    fn fmt_inner(&self, f: &mut fmt::Formatter<'_>, redact_hostname: bool) -> fmt::Result {
        writeln!(f, "OS:            {}", self.os)?;

        if let Some(distro) = &self.distribution {
            writeln!(f, "Distribution:  {distro}")?;
        }

        writeln!(f, "Architecture:  {}", self.arch)?;
        writeln!(f, "CPU cores:     {}", self.cpu_cores)?;

        if self.total_memory_bytes > 0 {
            writeln!(f, "Total memory:  {}", format_bytes(self.total_memory_bytes))?;
        }

        let hostname_value = match &self.hostname {
            Some(_) if redact_hostname => "<redacted>".to_string(),
            Some(h) => h.clone(),
            None => "<unknown>".to_string(),
        };

        writeln!(f, "Hostname:      {hostname_value}")?;
        writeln!(f, "Container:     {}", self.container.as_str())?;
        writeln!(f, "PID:           {}", self.pid)?;

        if let Some(uid) = self.uid {
            writeln!(f, "UID:           {uid}")?;
        }

        Ok(())
    }
}

impl fmt::Display for RuntimeInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_inner(f, false)
    }
}

fn read_os_release_pretty_name() -> Option<String> {
    let content = std::fs::read_to_string("/etc/os-release").ok()?;

    for line in content.lines() {
        if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
            return Some(value.trim_matches('"').to_string());
        }
    }

    None
}

#[cfg(unix)]
fn get_uid() -> Option<u32> {
    // SAFETY: getuid() is always safe to call.
    Some(unsafe { libc::getuid() })
}

#[cfg(not(unix))]
fn get_uid() -> Option<u32> {
    None
}

fn detect_total_memory() -> u64 {
    let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") else {
        return 0;
    };

    for line in meminfo.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") && let Some(kb) = rest.split_whitespace().next().and_then(|v| v.parse::<u64>().ok()) {
            return kb * 1024;
        }
    }

    0
}

fn format_bytes(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;

    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else {
        format!("{bytes} bytes")
    }
}
