use std::collections::HashSet;
use std::future::Future;
use std::net::{Ipv4Addr, Ipv6Addr};
use dns_types::types::Name;
use dns_types::zone::Zones;
use crate::error::ResolveError;

pub const MAX_HOPS: usize = 8;

const LOCAL_TTL: u32 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepAnswer {
    pub cname: Option<Name>,
    pub v4: Vec<Ipv4Addr>,
    pub v6: Vec<Ipv6Addr>,
    pub ttl: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub v4: Vec<Ipv4Addr>,
    pub v6: Vec<Ipv6Addr>,
    pub ttl: u32,
}

pub trait Upstream: Send + Sync + 'static {
    fn resolve_step(&self, name: &Name) -> impl Future<Output = Result<StepAnswer, ResolveError>> + Send;
}

pub async fn chase<U: Upstream>(
    upstream: &U,
    local: Option<&Zones>,
    target: &Name,
) -> Result<Resolved, ResolveError> {
    let mut current = target.clone();
    let mut seen: HashSet<Name> = HashSet::new();

    for _ in 0..MAX_HOPS {
        if !seen.insert(current.clone()) {
            return Err(ResolveError::Loop);
        }

        if let Some(zones) = local && let Some((v4, v6)) = zones.resolve_local_addresses(&current) {
            return Ok(Resolved { v4, v6, ttl: LOCAL_TTL });
        }

        let step = upstream.resolve_step(&current).await?;
        if let Some(next) = step.cname {
            current = next;
            continue;
        }

        if step.v4.is_empty() && step.v6.is_empty() {
            return Err(ResolveError::NoAddresses);
        }
        
        return Ok(Resolved { v4: step.v4, v6: step.v6, ttl: step.ttl });
    }

    Err(ResolveError::HopLimit)
}

#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// A scripted upstream: maps a name to a canned step answer, or an error. Records how many
    /// times each name was queried so tests can assert refresh behavior.
    pub struct MockUpstream {
        answers: Mutex<HashMap<Name, Result<StepAnswer, ()>>>,
        pub calls: Mutex<HashMap<Name, usize>>,
    }

    impl MockUpstream {
        pub fn new() -> Self {
            MockUpstream { answers: Mutex::new(HashMap::new()), calls: Mutex::new(HashMap::new()) }
        }

        pub fn set(&self, name: Name, answer: StepAnswer) {
            self.answers.lock().unwrap().insert(name, Ok(answer));
        }

        pub fn set_error(&self, name: Name) {
            self.answers.lock().unwrap().insert(name, Err(()));
        }

        pub fn cname(&self, name: Name, target: Name) {
            self.set(name, StepAnswer { cname: Some(target), v4: vec![], v6: vec![], ttl: 60 });
        }

        pub fn addrs(&self, name: Name, v4: Vec<Ipv4Addr>, v6: Vec<Ipv6Addr>, ttl: u32) {
            self.set(name, StepAnswer { cname: None, v4, v6, ttl });
        }
    }

    impl Upstream for MockUpstream {
        async fn resolve_step(&self, name: &Name) -> Result<StepAnswer, ResolveError> {
            *self.calls.lock().unwrap().entry(name.clone()).or_insert(0) += 1;
            match self.answers.lock().unwrap().get(name) {
                Some(Ok(answer)) => Ok(answer.clone()),
                Some(Err(())) => Err(ResolveError::Upstream("mock failure".into())),
                None => Err(ResolveError::NoAddresses),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::mock::MockUpstream;

    fn name(labels: &[&str]) -> Name {
        Name::from_ascii(&labels.join(".")).unwrap()
    }

    #[tokio::test]
    async fn follows_cname_chain_to_addresses() {
        let up = MockUpstream::new();
        up.cname(name(&["a", "net"]), name(&["b", "net"]));
        up.addrs(name(&["b", "net"]), vec![Ipv4Addr::new(1, 2, 3, 4)], vec![], 120);

        let resolved = chase(&up, None, &name(&["a", "net"])).await.unwrap();
        assert_eq!(resolved.v4, vec![Ipv4Addr::new(1, 2, 3, 4)]);
        assert_eq!(resolved.ttl, 120);
    }

    #[tokio::test]
    async fn detects_cname_loop() {
        let up = MockUpstream::new();
        up.cname(name(&["a", "net"]), name(&["b", "net"]));
        up.cname(name(&["b", "net"]), name(&["a", "net"]));

        let err = chase(&up, None, &name(&["a", "net"])).await.unwrap_err();
        assert!(matches!(err, ResolveError::Loop));
    }

    #[tokio::test]
    async fn enforces_hop_limit() {
        let up = MockUpstream::new();
        // A long non-repeating chain: h0 -> h1 -> ... longer than MAX_HOPS.
        for i in 0..(MAX_HOPS + 2) {
            up.cname(name(&[&format!("h{i}"), "net"]), name(&[&format!("h{}", i + 1), "net"]));
        }
        let err = chase(&up, None, &name(&["h0", "net"])).await.unwrap_err();
        assert!(matches!(err, ResolveError::HopLimit));
    }

    #[tokio::test]
    async fn empty_terminal_is_no_addresses() {
        let up = MockUpstream::new();
        up.addrs(name(&["a", "net"]), vec![], vec![], 60);
        let err = chase(&up, None, &name(&["a", "net"])).await.unwrap_err();
        assert!(matches!(err, ResolveError::NoAddresses));
    }
}
