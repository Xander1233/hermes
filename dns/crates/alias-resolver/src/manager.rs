use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;
use dns_types::types::Name;
use dns_types::zone::store::ZoneStore;
use dns_types::zone::alias::{AliasCache, AliasEntry};
use crate::error::ResolveError;
use crate::filter::filter_addrs;
use crate::upstream::{chase, Upstream};

#[derive(Debug, Clone)]
pub struct ResolverConfig {
    pub min_ttl: u32,
    pub max_ttl: u32,
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        ResolverConfig {
            min_ttl: 30,
            max_ttl: 3600,
            backoff_initial: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
        }
    }
}

struct Shared<U> {
    cache: Arc<AliasCache>,
    upstream: U,
    local: Option<Arc<ZoneStore>>,
    config: ResolverConfig,
}

struct State {
    refcounts: HashMap<Name, usize>,
    tasks: HashMap<Name, JoinHandle<()>>,
}

pub struct AliasResolver<U> {
    shared: Arc<Shared<U>>,
    state: Mutex<State>,
}

impl<U: Upstream> AliasResolver<U> {
    pub fn new(cache: Arc<AliasCache>, upstream: U, local: Option<Arc<ZoneStore>>, config: ResolverConfig) -> Arc<Self> {
        Arc::new(AliasResolver {
            shared: Arc::new(Shared { cache, upstream, local, config }),
            state: Mutex::new(State { refcounts: HashMap::new(), tasks: HashMap::new() }),
        })
    }

    pub fn sync(self: &Arc<Self>, desired: HashSet<Name>) {
        let mut state = self.state.lock().unwrap();
        let current: HashSet<Name> = state.tasks.keys().cloned().collect();

        for target in desired.difference(&current) {
            let handle = self.spawn_task(target.clone());
            state.tasks.insert(target.clone(), handle);
            state.refcounts.insert(target.clone(), 1);
        }

        for target in current.difference(&desired) {
            if let Some(handle) = state.tasks.remove(target) {
                handle.abort();
            }

            state.refcounts.remove(target);
            self.shared.cache.remove(target);
        }
    }

    pub fn register(self: &Arc<Self>, target: Name) {
        let mut state = self.state.lock().unwrap();

        let count = state.refcounts.entry(target.clone()).or_insert(0);
        *count += 1;

        if *count == 1 {
            let handle = self.spawn_task(target.clone());
            state.tasks.insert(target, handle);
        }
    }

    pub fn deregister(&self, target: &Name) {
        let mut state = self.state.lock().unwrap();

        if let Some(count) = state.refcounts.get_mut(target) {
            *count -= 1;

            if *count == 0 {
                state.refcounts.remove(target);

                if let Some(handle) = state.tasks.remove(target) {
                    handle.abort();
                }

                self.shared.cache.remove(target);
            }
        }
    }

    pub fn spawn_reconciler(self: &Arc<Self>, store: Arc<ZoneStore>, interval: Duration) -> JoinHandle<()> {
        let resolver = self.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                resolver.sync(store.load().alias_targets());
            }
        })
    }

    pub fn shutdown(&self) {
        let mut state = self.state.lock().unwrap();

        for (_, handle) in state.tasks.drain() {
            handle.abort();
        }

        state.refcounts.clear();
    }

    pub fn active_targets(&self) -> usize {
        self.state.lock().unwrap().tasks.len()
    }

    pub async fn resolve_target(&self, target: &Name) -> Result<u32, ResolveError> {
        resolve_once(&self.shared, target).await
    }

    fn spawn_task(&self, target: Name) -> JoinHandle<()> {
        let shared = self.shared.clone();

        tokio::spawn(async move {
            let mut backoff = shared.config.backoff_initial;

            loop {
                match resolve_once(&shared, &target).await {
                    Ok(ttl) => {
                        backoff = shared.config.backoff_initial;
                        tokio::time::sleep(Duration::from_secs(ttl as u64)).await;
                    }
                    Err(error) => {
                        tracing::warn!(
                            target = %target.to_display(),
                            error = %error,
                            "alias resolution failed; serving stale and backing off"
                        );

                        let wait = backoff;
                        backoff = (backoff * 2).min(shared.config.backoff_max);

                        tokio::time::sleep(wait).await;
                    }
                }
            }
        })
    }
}

async fn resolve_once<U: Upstream>(shared: &Shared<U>, target: &Name) -> Result<u32, ResolveError> {
    let local = shared.local.as_ref().map(|store| store.load());
    let resolved = chase(&shared.upstream, local.as_deref(), target).await?;
    let (v4, v6) = filter_addrs(resolved.v4, resolved.v6);

    if v4.is_empty() && v6.is_empty() {
        return Err(ResolveError::NoAddresses);
    }

    let ttl = resolved.ttl.clamp(shared.config.min_ttl, shared.config.max_ttl);
    shared.cache.upsert(target.clone(), AliasEntry::resolved(v4, v6, ttl));

    Ok(ttl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upstream::mock::MockUpstream;
    use std::net::Ipv4Addr;

    fn name(labels: &[&str]) -> Name {
        Name::from_ascii(&labels.join(".")).unwrap()
    }

    fn resolver(up: MockUpstream) -> Arc<AliasResolver<MockUpstream>> {
        AliasResolver::new(Arc::new(AliasCache::new()), up, None, ResolverConfig::default())
    }

    #[tokio::test]
    async fn cold_start_populates_cache() {
        let up = MockUpstream::new();
        let target = name(&["lb", "provider", "net"]);
        up.addrs(target.clone(), vec![Ipv4Addr::new(203, 0, 113, 7)], vec![], 120);

        let r = resolver(up);
        // Cache is cold before the first resolution.
        assert!(r.shared.cache.get(&target).is_none());

        let ttl = r.resolve_target(&target).await.unwrap();
        assert_eq!(ttl, 120);
        let entry = r.shared.cache.get(&target).unwrap();
        assert_eq!(entry.v4, vec![Ipv4Addr::new(203, 0, 113, 7)]);
        assert!(entry.resolved);
    }

    #[tokio::test]
    async fn refresh_updates_addresses() {
        let up = MockUpstream::new();
        let target = name(&["lb", "provider", "net"]);
        up.addrs(target.clone(), vec![Ipv4Addr::new(203, 0, 113, 7)], vec![], 120);

        let r = resolver(up);
        r.resolve_target(&target).await.unwrap();

        // The target's addresses change; a refresh should pick them up.
        r.shared.upstream.addrs(target.clone(), vec![Ipv4Addr::new(203, 0, 113, 8)], vec![], 60);
        r.resolve_target(&target).await.unwrap();

        assert_eq!(r.shared.cache.get(&target).unwrap().v4, vec![Ipv4Addr::new(203, 0, 113, 8)]);
    }

    #[tokio::test]
    async fn upstream_failure_serves_stale() {
        let up = MockUpstream::new();
        let target = name(&["lb", "provider", "net"]);
        up.addrs(target.clone(), vec![Ipv4Addr::new(203, 0, 113, 7)], vec![], 120);

        let r = resolver(up);
        r.resolve_target(&target).await.unwrap();

        // Upstream now fails; the previous (stale) entry must remain.
        r.shared.upstream.set_error(target.clone());
        let err = r.resolve_target(&target).await.unwrap_err();
        assert!(matches!(err, ResolveError::Upstream(_)));

        let entry = r.shared.cache.get(&target).unwrap();
        assert_eq!(entry.v4, vec![Ipv4Addr::new(203, 0, 113, 7)]); // unchanged
        assert!(entry.resolved);
    }

    #[tokio::test]
    async fn all_private_addresses_are_rejected() {
        let up = MockUpstream::new();
        let target = name(&["internal", "corp", "net"]);
        up.addrs(target.clone(), vec![Ipv4Addr::new(10, 0, 0, 5)], vec![], 120);

        let r = resolver(up);
        let err = r.resolve_target(&target).await.unwrap_err();
        assert!(matches!(err, ResolveError::NoAddresses));
        assert!(r.shared.cache.get(&target).is_none()); // stays cold -> SERVFAIL at query time
    }

    #[tokio::test]
    async fn sync_starts_and_stops_tasks() {
        let up = MockUpstream::new();
        let a = name(&["a", "provider", "net"]);
        let b = name(&["b", "provider", "net"]);
        up.addrs(a.clone(), vec![Ipv4Addr::new(1, 1, 1, 1)], vec![], 120);
        up.addrs(b.clone(), vec![Ipv4Addr::new(2, 2, 2, 2)], vec![], 120);

        let r = resolver(up);
        r.sync(HashSet::from([a.clone(), b.clone()]));
        assert_eq!(r.active_targets(), 2);

        // Drop `b`: its task stops and its cache entry is removed.
        r.sync(HashSet::from([a.clone()]));
        assert_eq!(r.active_targets(), 1);
        assert!(r.shared.cache.get(&b).is_none());

        r.shutdown();
        assert_eq!(r.active_targets(), 0);
    }
}
