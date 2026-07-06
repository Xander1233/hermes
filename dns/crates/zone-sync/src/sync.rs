use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::{Notify, Semaphore};
use tokio::task::JoinSet;
use dns_types::types::Name;
use dns_types::zone::Zones;
use dns_types::zone::store::ZoneStore;
use hermes_config::apply_zone;
use crate::error::SyncError;
use crate::model::{PersistedSnapshot, SyncOutcome, SyncStatus, ZoneDocument, ZoneRef};
use crate::source::ZoneSource;

struct TrackedZone {
    apex: String,
    version: u64,
    document: ZoneDocument,
}

struct State {
    manifest_version: Option<u64>,
    zones: HashMap<String, TrackedZone>,
    skipped: usize,
    last_success: Option<SystemTime>,
    last_error: Option<String>,
    successful: u64,
    failed: u64,
}

pub struct ZoneSync<S> {
    store: Arc<ZoneStore>,
    source: S,
    snapshot_path: Option<PathBuf>,
    fetch_concurrency: usize,
    state: Mutex<State>,
    notify: Notify,
}

impl<S: ZoneSource> ZoneSync<S> {
    pub fn new(
        store: Arc<ZoneStore>,
        source: S,
        snapshot_path: Option<PathBuf>,
        fetch_concurrency: usize,
    ) -> Arc<Self> {
        Arc::new(ZoneSync {
            store,
            source,
            snapshot_path,
            fetch_concurrency: fetch_concurrency.max(1),
            state: Mutex::new(State {
                manifest_version: None,
                zones: HashMap::new(),
                skipped: 0,
                last_success: None,
                last_error: None,
                successful: 0,
                failed: 0,
            }),
            notify: Notify::new(),
        })
    }

    pub fn trigger(&self) {
        self.notify.notify_one();
    }

    pub fn status(&self) -> SyncStatus {
        let state = self.state.lock().unwrap();

        SyncStatus {
            manifest_version: state.manifest_version,
            zones: self.store.zone_count(),
            tracked: state.zones.len(),
            skipped: state.skipped,
            last_success_secs_ago: state
                .last_success
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .map(|d| d.as_secs()),
            last_error: state.last_error.clone(),
            successful_syncs: state.successful,
            failed_syncs: state.failed,
        }
    }

    pub async fn sync_once(self: &Arc<Self>) -> Result<SyncOutcome, SyncError> {
        let manifest = match self.source.manifest().await {
            Ok(manifest) => manifest,
            Err(error) => {
                self.record_failure(&error);
                return Err(error);
            }
        };

        let (to_fetch, to_delete) = {
            let state = self.state.lock().unwrap();

            let to_fetch: Vec<ZoneRef> = manifest
                .zones
                .iter()
                .filter(|entry| match state.zones.get(&entry.id) {
                    Some(tracked) => tracked.version != entry.version,
                    None => true,
                })
                .cloned()
                .collect();

            let present: HashSet<&str> = manifest.zones.iter().map(|entry| entry.id.as_str()).collect();
            let to_delete: Vec<(String, String)> = state
                .zones
                .iter()
                .filter(|(id, _)| !present.contains(id.as_str()))
                .map(|(id, tracked)| (id.clone(), tracked.apex.clone()))
                .collect();

            (to_fetch, to_delete)
        };

        if to_fetch.is_empty() && to_delete.is_empty() {
            self.record_up_to_date(manifest.version);
            return Ok(SyncOutcome::UpToDate);
        }

        let fetched = self.fetch_zones(to_fetch).await;
        self.apply(manifest.version, fetched, to_delete, true)
    }

    async fn fetch_zones(self: &Arc<Self>, refs: Vec<ZoneRef>) -> Vec<(ZoneRef, Result<ZoneDocument, SyncError>)> {
        let semaphore = Arc::new(Semaphore::new(self.fetch_concurrency));
        let mut tasks = JoinSet::new();

        for entry in refs {
            let this = self.clone();
            let permits = semaphore.clone();

            tasks.spawn(async move {
                let _permit = permits.acquire_owned().await;
                let result = this.source.zone(&entry.id).await;

                (entry, result)
            });
        }

        let mut results = Vec::new();

        while let Some(joined) = tasks.join_next().await {
            match joined {
                Ok(pair) => results.push(pair),
                Err(error) => tracing::error!(%error, "zone fetch task failed to join"),
            }
        }

        results
    }

    fn apply(
        &self,
        manifest_version: Option<u64>,
        fetched: Vec<(ZoneRef, Result<ZoneDocument, SyncError>)>,
        to_delete: Vec<(String, String)>,
        record_success: bool,
    ) -> Result<SyncOutcome, SyncError> {
        let mut zones: Zones = (*self.store.load()).clone();

        for (_, apex) in &to_delete {
            match Name::from_ascii(apex) {
                Ok(name) => {
                    zones.remove_zone(&name);
                }
                Err(_) => tracing::warn!(apex, "tracked zone has an unparsable apex; skipping removal"),
            }
        }

        let mut applied: Vec<ZoneDocument> = Vec::new();
        let mut skipped = 0usize;

        for (entry, result) in fetched {
            let document = match result {
                Ok(document) => document,
                Err(error) => {
                    skipped += 1;
                    tracing::warn!(id = %entry.id, apex = %entry.apex, %error, "could not fetch zone; keeping previous state");

                    continue;
                }
            };

            let mut scratch = Zones::new();
            let apex = match apply_zone(&mut scratch, &document.to_zone_config()) {
                Ok(apex) => apex,
                Err(error) => {
                    skipped += 1;
                    tracing::warn!(id = %document.id, apex = %document.apex, %error, "invalid zone data; keeping previous state");

                    continue;
                }
            };

            if let Some(previous) = self.tracked_apex(&document.id) && previous != document.apex && let Ok(name) = Name::from_ascii(&previous) {
                zones.remove_zone(&name);
            }

            zones.adopt_zone(&mut scratch, &apex);
            applied.push(document);
        }

        let (persisted, expected) = {
            let mut state = self.state.lock().unwrap();

            for (id, _) in &to_delete {
                state.zones.remove(id);
            }

            for document in &applied {
                state.zones.insert(
                    document.id.clone(),
                    TrackedZone {
                        apex: document.apex.clone(),
                        version: document.version,
                        document: document.clone(),
                    },
                );
            }

            state.manifest_version = manifest_version;
            state.skipped = skipped;
            state.last_error = (skipped > 0).then(|| format!("{skipped} zone(s) failed to apply"));

            if record_success {
                state.last_success = Some(SystemTime::now());
                state.successful += 1;
            }

            let expected: HashSet<Name> = state
                .zones
                .values()
                .filter_map(|tracked| Name::from_ascii(&tracked.apex).ok())
                .collect();

            let persisted = PersistedSnapshot {
                manifest_version: state.manifest_version,
                zones: state.zones.values().map(|tracked| tracked.document.clone()).collect(),
            };

            (persisted, expected)
        };

        let stale: Vec<Name> = zones.apexes().filter(|apex| !expected.contains(*apex)).cloned().collect();

        for apex in stale {
            tracing::info!(apex = %apex.to_display(), "dropping zone not present in the control plane manifest");

            zones.remove_zone(&apex);
        }

        let total = zones.len();
        self.store.store(zones);

        telemetry::metrics().record_zone_sync("applied", total, record_success);
        telemetry::metrics().record_zones_skipped(skipped);

        if record_success {
            self.persist(&persisted);
        }

        Ok(SyncOutcome::Applied { updated: applied.len(), deleted: to_delete.len(), skipped, total })
    }

    fn tracked_apex(&self, id: &str) -> Option<String> {
        self.state.lock().unwrap().zones.get(id).map(|tracked| tracked.apex.clone())
    }

    fn record_up_to_date(&self, manifest_version: Option<u64>) {
        {
            let mut state = self.state.lock().unwrap();
            state.manifest_version = manifest_version;
            state.last_success = Some(SystemTime::now());
            state.successful += 1;
            if state.skipped == 0 {
                state.last_error = None;
            }
        }

        // A no-op pass is still a successful sync, so it refreshes the freshness gauge.
        telemetry::metrics().record_zone_sync("up_to_date", self.store.zone_count(), true);
    }

    fn record_failure(&self, error: &SyncError) {
        {
            let mut state = self.state.lock().unwrap();
            state.failed += 1;
            state.last_error = Some(error.to_string());
        }

        telemetry::metrics().record_zone_sync("failed", self.store.zone_count(), false);
    }

    fn persist(&self, snapshot: &PersistedSnapshot) {
        let Some(path) = &self.snapshot_path else { return };

        if let Some(parent) = path.parent() && !parent.as_os_str().is_empty() && let Err(error) = std::fs::create_dir_all(parent) {
            tracing::warn!(path = %path.display(), %error, "could not create snapshot directory");
            return;
        }

        match serde_json::to_vec_pretty(snapshot) {
            Ok(bytes) => {
                if let Err(error) = std::fs::write(path, bytes) {
                    tracing::warn!(path = %path.display(), %error, "could not write zone snapshot");
                }
            }
            Err(error) => tracing::warn!(%error, "could not serialize zone snapshot"),
        }
    }

    pub fn load_disk_snapshot(&self) -> Result<bool, SyncError> {
        let Some(path) = &self.snapshot_path else { return Ok(false) };
        if !path.exists() {
            return Ok(false);
        }

        let text = std::fs::read_to_string(path).map_err(|source| SyncError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let snapshot: PersistedSnapshot = serde_json::from_str(&text)?;

        let fetched = snapshot
            .zones
            .into_iter()
            .map(|document| {
                let entry = ZoneRef {
                    id: document.id.clone(),
                    apex: document.apex.clone(),
                    version: document.version,
                };
                (entry, Ok(document))
            })
            .collect();

        self.apply(snapshot.manifest_version, fetched, Vec::new(), false)?;
        Ok(true)
    }

    pub async fn run(self: Arc<Self>, poll_interval: Duration) {
        loop {
            tokio::select! {
                _ = self.notify.notified() => tracing::debug!("zone sync triggered"),
                _ = tokio::time::sleep(poll_interval) => {}
            }

            match self.sync_once().await {
                Ok(SyncOutcome::UpToDate) => tracing::debug!("zone sync: already up to date"),
                Ok(SyncOutcome::Applied { updated, deleted, skipped, total }) => {
                    tracing::info!(updated, deleted, skipped, total, "applied zone update");
                }
                Err(error) => tracing::warn!(%error, "zone sync failed; continuing to serve the current snapshot"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ZoneDocument;
    use crate::source::mock::{a_record, soa, MockSource};

    fn sync(source: MockSource) -> Arc<ZoneSync<MockSource>> {
        ZoneSync::new(Arc::new(ZoneStore::empty()), source, None, 8)
    }

    fn name(value: &str) -> Name {
        Name::from_ascii(value).unwrap()
    }

    /// The A record served for `www.<apex>` in the current snapshot, as a string.
    fn served_ip(sync: &Arc<ZoneSync<MockSource>>, apex: &str) -> Option<String> {
        use dns_types::zone::resource_record::{RecordData, RecordType};
        use dns_types::zone::zone_node::LookUpResult;

        let zones = sync.store.load();
        let zone = zones.get_zone(&name(apex))?;
        match zone.query(&name(&format!("www.{apex}")), u16::from(RecordType::A)) {
            LookUpResult::Answer(rr) => match rr.records.first() {
                Some(RecordData::A(ip)) => Some(ip.to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    #[tokio::test]
    async fn first_pass_fetches_every_zone() {
        let source = MockSource::new();
        source.upsert("z1", "one.test", 1, "10.0.0.1");
        source.upsert("z2", "two.test", 1, "10.0.0.2");
        source.set_manifest_version(Some(4711));

        let sync = sync(source);
        let outcome = sync.sync_once().await.unwrap();

        assert!(matches!(outcome, SyncOutcome::Applied { updated: 2, deleted: 0, skipped: 0, total: 2 }));
        assert_eq!(sync.source.fetches(), vec!["z1", "z2"]);
        assert_eq!(served_ip(&sync, "one.test").as_deref(), Some("10.0.0.1"));

        // The advisory manifest version is surfaced for health/logging.
        let status = sync.status();
        assert_eq!(status.manifest_version, Some(4711));
        assert_eq!(status.tracked, 2);
        assert_eq!(status.skipped, 0);
    }

    #[tokio::test]
    async fn zones_seeded_from_the_local_file_are_retired_once_sync_takes_over() {
        // The store starts with a zone loaded from zones.toml, which the control plane does
        // not know about. Once sync succeeds, the manifest is the complete set.
        let mut seeded = Zones::new();
        let seed_apex = name("fileonly.test");
        seeded.add_zone(seed_apex.clone(), crate::source::mock::seed_soa("fileonly.test"), 3600);
        let store = Arc::new(ZoneStore::new(seeded));

        let source = MockSource::new();
        source.upsert("z1", "one.test", 1, "10.0.0.1");
        let sync = ZoneSync::new(store, source, None, 8);

        sync.sync_once().await.unwrap();

        let zones = sync.store.load();
        assert!(zones.get_zone(&seed_apex).is_none(), "file-seeded zone must be retired");
        assert!(zones.get_zone(&name("one.test")).is_some());
        assert_eq!(zones.len(), 1);
        assert_eq!(sync.status().zones, 1);
    }

    #[tokio::test]
    async fn unchanged_zones_are_not_refetched() {
        let source = MockSource::new();
        source.upsert("z1", "one.test", 1, "10.0.0.1");
        source.upsert("z2", "two.test", 1, "10.0.0.2");
        let sync = sync(source);
        sync.sync_once().await.unwrap();

        // Nothing changed: the pass must fetch no zones at all.
        sync.source.clear_fetches();
        assert_eq!(sync.sync_once().await.unwrap(), SyncOutcome::UpToDate);
        assert!(sync.source.fetches().is_empty());
    }

    #[tokio::test]
    async fn only_the_changed_zone_is_refetched() {
        let source = MockSource::new();
        source.upsert("z1", "one.test", 1, "10.0.0.1");
        source.upsert("z2", "two.test", 1, "10.0.0.2");
        let sync = sync(source);
        sync.sync_once().await.unwrap();

        sync.source.clear_fetches();
        sync.source.upsert("z2", "two.test", 2, "10.0.0.22");
        let outcome = sync.sync_once().await.unwrap();

        assert!(matches!(outcome, SyncOutcome::Applied { updated: 1, deleted: 0, skipped: 0, total: 2 }));
        assert_eq!(sync.source.fetches(), vec!["z2"], "only the bumped zone should be fetched");
        assert_eq!(served_ip(&sync, "two.test").as_deref(), Some("10.0.0.22"));
        assert_eq!(served_ip(&sync, "one.test").as_deref(), Some("10.0.0.1"), "sibling untouched");
    }

    #[tokio::test]
    async fn zone_missing_from_manifest_is_deleted() {
        let source = MockSource::new();
        source.upsert("z1", "one.test", 1, "10.0.0.1");
        source.upsert("z2", "two.test", 1, "10.0.0.2");
        let sync = sync(source);
        sync.sync_once().await.unwrap();

        sync.source.remove("z2");
        let outcome = sync.sync_once().await.unwrap();

        assert!(matches!(outcome, SyncOutcome::Applied { updated: 0, deleted: 1, skipped: 0, total: 1 }));
        assert!(sync.store.load().get_zone(&name("two.test")).is_none());
        assert!(sync.store.load().get_zone(&name("one.test")).is_some());
    }

    #[tokio::test]
    async fn a_zone_that_fails_to_fetch_is_isolated_and_retried() {
        let source = MockSource::new();
        source.upsert("z1", "one.test", 1, "10.0.0.1");
        source.upsert("z2", "two.test", 1, "10.0.0.2");
        let sync = sync(source);
        sync.sync_once().await.unwrap();

        // Both change, but one zone's fetch fails.
        sync.source.upsert("z1", "one.test", 2, "10.0.0.11");
        sync.source.upsert("z2", "two.test", 2, "10.0.0.22");
        sync.source.fail_zone("z2", true);

        let outcome = sync.sync_once().await.unwrap();
        assert!(matches!(outcome, SyncOutcome::Applied { updated: 1, skipped: 1, total: 2, .. }));
        assert_eq!(served_ip(&sync, "one.test").as_deref(), Some("10.0.0.11"), "healthy zone still updates");
        assert_eq!(served_ip(&sync, "two.test").as_deref(), Some("10.0.0.2"), "failed zone keeps its old data");
        assert_eq!(sync.status().skipped, 1);

        // Its version was not advanced, so the next pass retries it.
        sync.source.fail_zone("z2", false);
        sync.source.clear_fetches();
        sync.sync_once().await.unwrap();
        assert_eq!(sync.source.fetches(), vec!["z2"]);
        assert_eq!(served_ip(&sync, "two.test").as_deref(), Some("10.0.0.22"));
    }

    #[tokio::test]
    async fn an_invalid_zone_is_isolated_from_the_others() {
        let source = MockSource::new();
        source.upsert("z1", "one.test", 1, "10.0.0.1");
        let sync = sync(source);
        sync.sync_once().await.unwrap();

        // A record outside its own zone: this zone must be skipped, not published.
        sync.source.upsert_doc(ZoneDocument {
            id: "zbad".into(),
            apex: "bad.test".into(),
            version: 1,
            soa: soa("bad.test"),
            records: vec![a_record("www.somewhere-else.org", "10.0.0.9")],
        });
        sync.source.upsert("z1", "one.test", 2, "10.0.0.11");

        let outcome = sync.sync_once().await.unwrap();
        assert!(matches!(outcome, SyncOutcome::Applied { updated: 1, skipped: 1, .. }));
        assert!(sync.store.load().get_zone(&name("bad.test")).is_none(), "invalid zone must not be published");
        assert_eq!(served_ip(&sync, "one.test").as_deref(), Some("10.0.0.11"), "valid zone still applies");
    }

    #[tokio::test]
    async fn apex_rename_under_a_stable_id_moves_the_zone() {
        let source = MockSource::new();
        source.upsert("z1", "old.test", 1, "10.0.0.1");
        let sync = sync(source);
        sync.sync_once().await.unwrap();
        assert!(sync.store.load().get_zone(&name("old.test")).is_some());

        // Same id, new apex.
        sync.source.upsert("z1", "new.test", 2, "10.0.0.1");
        sync.sync_once().await.unwrap();

        assert!(sync.store.load().get_zone(&name("old.test")).is_none(), "old apex must be removed");
        assert!(sync.store.load().get_zone(&name("new.test")).is_some());
        assert_eq!(sync.store.load().len(), 1);
    }

    #[tokio::test]
    async fn manifest_failure_keeps_serving_and_fetches_nothing() {
        let source = MockSource::new();
        source.upsert("z1", "one.test", 1, "10.0.0.1");
        let sync = sync(source);
        sync.sync_once().await.unwrap();

        sync.source.fail_manifest(true);
        sync.source.clear_fetches();
        assert!(sync.sync_once().await.is_err());

        assert!(sync.source.fetches().is_empty());
        assert_eq!(served_ip(&sync, "one.test").as_deref(), Some("10.0.0.1"));
        let status = sync.status();
        assert_eq!(status.failed_syncs, 1);
        assert!(status.last_error.is_some());
    }

    #[tokio::test]
    async fn disk_snapshot_roundtrip_serves_on_cold_start() {
        let dir = std::env::temp_dir().join(format!("hermes-zonesync-{}", std::process::id()));
        let path = dir.join("snapshot.json");
        let _ = std::fs::remove_file(&path);

        let source = MockSource::new();
        source.upsert("z1", "one.test", 3, "10.0.0.1");
        let first = ZoneSync::new(Arc::new(ZoneStore::empty()), source, Some(path.clone()), 8);
        first.sync_once().await.unwrap();
        assert!(path.exists());

        // Fresh instance, control plane unreachable: still boots from disk.
        let offline = MockSource::new();
        offline.fail_manifest(true);
        let second = ZoneSync::new(Arc::new(ZoneStore::empty()), offline, Some(path.clone()), 8);
        assert!(second.load_disk_snapshot().unwrap());
        assert_eq!(served_ip(&second, "one.test").as_deref(), Some("10.0.0.1"));
        assert_eq!(second.status().tracked, 1);
        assert_eq!(second.status().successful_syncs, 0, "loading from disk is not a successful sync");

        // The adopted per-zone version means an unchanged zone is not refetched afterwards.
        second.source.fail_manifest(false);
        second.source.upsert("z1", "one.test", 3, "10.0.0.1");
        second.source.clear_fetches();
        assert_eq!(second.sync_once().await.unwrap(), SyncOutcome::UpToDate);
        assert!(second.source.fetches().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
