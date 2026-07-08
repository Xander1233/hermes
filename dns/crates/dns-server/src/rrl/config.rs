use super::IpNet;

#[derive(Debug, Clone)]
pub struct RrlConfig {
    pub enabled: bool,
    pub responses_per_second: u32,
    pub all_per_second: u32,
    pub nxdomains_per_second: u32,
    pub errors_per_second: u32,
    pub window_secs: u32,
    pub slip: u32,
    pub ipv4_prefix_len: u8,
    pub ipv6_prefix_len: u8,
    pub table_size: usize,
    pub exempt: Vec<IpNet>,
}

impl Default for RrlConfig {
    fn default() -> Self {
        RrlConfig {
            enabled: true,
            responses_per_second: 15,
            all_per_second: 100,
            nxdomains_per_second: 5,
            errors_per_second: 5,
            window_secs: 15,
            slip: 2,
            ipv4_prefix_len: 24,
            ipv6_prefix_len: 56,
            table_size: 8192,
            exempt: Vec::new(),
        }
    }
}
