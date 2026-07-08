use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const IPV4_BITS: u8 = 32;
const IPV6_BITS: u8 = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpNet {
    network: IpAddr,
    prefix_len: u8,
}

impl IpNet {
    pub fn new(address: IpAddr, prefix_len: u8) -> Self {
        IpNet { network: mask(address, prefix_len), prefix_len }
    }

    pub fn parse(input: &str) -> Option<Self> {
        let (address, prefix) = match input.trim().split_once('/') {
            Some((address, prefix)) => (address, Some(prefix)),
            None => (input.trim(), None),
        };

        let address: IpAddr = address.parse().ok()?;
        let max = if address.is_ipv4() { IPV4_BITS } else { IPV6_BITS };

        let prefix_len = match prefix {
            Some(prefix) => prefix.trim().parse::<u8>().ok()?,
            None => max,
        };

        (prefix_len <= max).then(|| IpNet::new(address, prefix_len))
    }

    pub fn contains(&self, address: IpAddr) -> bool {
        mask(address, self.prefix_len) == self.network
    }
}

pub(super) fn mask(address: IpAddr, prefix_len: u8) -> IpAddr {
    match address {
        IpAddr::V4(v4) => {
            let bits = u32::from(v4);
            let keep = if prefix_len >= IPV4_BITS { u32::MAX } else { !(u32::MAX >> prefix_len) };
            
            IpAddr::V4(Ipv4Addr::from(bits & keep))
        }
        IpAddr::V6(v6) => {
            let bits = u128::from(v6);
            let keep = if prefix_len >= IPV6_BITS { u128::MAX } else { !(u128::MAX >> prefix_len) };
            
            IpAddr::V6(Ipv6Addr::from(bits & keep))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr_parsing_and_matching() {
        let net = IpNet::parse("10.0.0.0/8").unwrap();
        assert!(net.contains("10.1.2.3".parse().unwrap()));
        assert!(!net.contains("11.0.0.1".parse().unwrap()));
        // A v4 net must not swallow v6 traffic.
        assert!(!net.contains("2001:db8::1".parse().unwrap()));

        // A bare address is a single host.
        let host = IpNet::parse("192.0.2.7").unwrap();
        assert!(host.contains("192.0.2.7".parse().unwrap()));
        assert!(!host.contains("192.0.2.8".parse().unwrap()));

        assert!(IpNet::parse("not-an-address").is_none());
        assert!(IpNet::parse("10.0.0.0/33").is_none());
    }
}
