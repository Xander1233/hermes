use std::net::{Ipv4Addr, Ipv6Addr};

pub fn is_forbidden_v4(ip: Ipv4Addr) -> bool {
    ip.is_private()          // 10/8, 172.16/12, 192.168/16
        || ip.is_loopback()   // 127/8
        || ip.is_link_local() // 169.254/16
        || ip.is_unspecified() // 0.0.0.0
        || ip.is_broadcast()   // 255.255.255.255
        || ip.octets()[0] == 0 // 0/8
}

pub fn is_forbidden_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    ip.is_loopback()            // ::1
        || ip.is_unspecified()   // ::
        || (first & 0xfe00) == 0xfc00 // ULA fc00::/7
        || (first & 0xffc0) == 0xfe80 // link-local fe80::/10
}

pub fn filter_addrs(v4: Vec<Ipv4Addr>, v6: Vec<Ipv6Addr>) -> (Vec<Ipv4Addr>, Vec<Ipv6Addr>) {
    (
        v4.into_iter().filter(|ip| !is_forbidden_v4(*ip)).collect(),
        v6.into_iter().filter(|ip| !is_forbidden_v6(*ip)).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_private_v4() {
        assert!(is_forbidden_v4(Ipv4Addr::new(10, 0, 0, 1)));
        assert!(is_forbidden_v4(Ipv4Addr::new(172, 16, 5, 5)));
        assert!(is_forbidden_v4(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(is_forbidden_v4(Ipv4Addr::new(127, 0, 0, 1)));
        assert!(is_forbidden_v4(Ipv4Addr::new(169, 254, 1, 1)));
        assert!(is_forbidden_v4(Ipv4Addr::UNSPECIFIED));
    }

    #[test]
    fn allows_public_v4() {
        assert!(!is_forbidden_v4(Ipv4Addr::new(93, 184, 216, 34)));
        assert!(!is_forbidden_v4(Ipv4Addr::new(1, 1, 1, 1)));
        assert!(!is_forbidden_v4(Ipv4Addr::new(203, 0, 113, 7)));
    }

    #[test]
    fn rejects_special_v6() {
        assert!(is_forbidden_v6(Ipv6Addr::LOCALHOST));
        assert!(is_forbidden_v6(Ipv6Addr::UNSPECIFIED));
        assert!(is_forbidden_v6("fc00::1".parse().unwrap())); // ULA
        assert!(is_forbidden_v6("fd12:3456::1".parse().unwrap())); // ULA
        assert!(is_forbidden_v6("fe80::1".parse().unwrap())); // link-local
    }

    #[test]
    fn allows_public_v6() {
        assert!(!is_forbidden_v6("2606:4700:4700::1111".parse().unwrap()));
        assert!(!is_forbidden_v6("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn filter_drops_forbidden_keeps_public() {
        let (v4, v6) = filter_addrs(
            vec![Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(93, 184, 216, 34)],
            vec!["fc00::1".parse().unwrap(), "2001:db8::1".parse().unwrap()],
        );
        assert_eq!(v4, vec![Ipv4Addr::new(93, 184, 216, 34)]);
        assert_eq!(v6, vec!["2001:db8::1".parse::<Ipv6Addr>().unwrap()]);
    }
}
