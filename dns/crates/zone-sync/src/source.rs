use std::future::Future;
use std::time::Duration;
use crate::error::SyncError;
use crate::model::{ZoneDocument, ZoneManifest};

pub trait ZoneSource: Send + Sync + 'static {
    fn manifest(&self) -> impl Future<Output = Result<ZoneManifest, SyncError>> + Send;

    fn zone(&self, id: &str) -> impl Future<Output = Result<ZoneDocument, SyncError>> + Send;
}

pub struct HttpZoneSource {
    client: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl HttpZoneSource {
    pub fn new(base_url: impl Into<String>, token: Option<String>, timeout: Duration) -> Result<Self, SyncError> {
        let client = reqwest::Client::builder().timeout(timeout).build()?;
        
        Ok(HttpZoneSource {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token,
        })
    }

    fn get(&self, url: reqwest::Url) -> reqwest::RequestBuilder {
        let request = self.client.get(url);
        
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    fn manifest_url(&self) -> Result<reqwest::Url, SyncError> {
        reqwest::Url::parse(&format!("{}/zones", self.base_url)).map_err(|error| SyncError::Http(format!("invalid base_url: {error}")))
    }

    fn zone_url(&self, id: &str) -> Result<reqwest::Url, SyncError> {
        let mut url = self.manifest_url()?;
        
        url.path_segments_mut()
            .map_err(|_| SyncError::Http("base_url cannot be a base".into()))?
            .push(id);
        
        Ok(url)
    }
}

impl ZoneSource for HttpZoneSource {
    async fn manifest(&self) -> Result<ZoneManifest, SyncError> {
        let url = self.manifest_url()?;
        let response = self.get(url).send().await?.error_for_status()?;
        
        Ok(response.json().await?)
    }

    async fn zone(&self, id: &str) -> Result<ZoneDocument, SyncError> {
        let url = self.zone_url(id)?;
        
        let response = self
            .get(url)
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|error| SyncError::ZoneFetch { id: id.to_string(), reason: error.to_string() })?;

        response
            .json()
            .await
            .map_err(|error| SyncError::ZoneFetch { id: id.to_string(), reason: error.to_string() })
    }
}

#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use crate::model::ZoneRef;
    use hermes_config::{RecordConfig, SoaConfig};
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    /// Scripted control plane: holds zone documents keyed by id, derives the manifest from
    /// them, and can fail the manifest or individual zones. Records which ids were fetched so
    /// tests can assert that unchanged zones are *not* refetched.
    pub struct MockSource {
        docs: Mutex<HashMap<String, ZoneDocument>>,
        failing: Mutex<HashSet<String>>,
        manifest_failing: Mutex<bool>,
        manifest_version: Mutex<Option<u64>>,
        fetches: Mutex<Vec<String>>,
    }

    /// SOA in the form `dns_types` wants, for seeding a store directly in tests.
    pub fn seed_soa(apex: &str) -> dns_types::zone::dnstype::SOA {
        dns_types::zone::dnstype::SOA {
            mname: dns_types::types::Name::from_ascii(&format!("ns1.{apex}")).unwrap(),
            rname: dns_types::types::Name::from_ascii(&format!("hostmaster.{apex}")).unwrap(),
            serial: 1,
            refresh: 3600,
            retry: 600,
            expire: 604800,
            minimum: 60,
        }
    }

    pub fn soa(apex: &str) -> SoaConfig {
        SoaConfig {
            mname: format!("ns1.{apex}"),
            rname: format!("hostmaster.{apex}"),
            serial: 1,
            refresh: 3600,
            retry: 600,
            expire: 604800,
            minimum: 60,
            ttl: 3600,
        }
    }

    pub fn a_record(name: &str, ip: &str) -> RecordConfig {
        RecordConfig {
            name: name.to_string(),
            rtype: "A".to_string(),
            ttl: 300,
            data: Some(ip.to_string()),
            preference: None,
            priority: None,
            weight: None,
            port: None,
            flags: None,
            tag: None,
        }
    }

    impl MockSource {
        pub fn new() -> Self {
            MockSource {
                docs: Mutex::new(HashMap::new()),
                failing: Mutex::new(HashSet::new()),
                manifest_failing: Mutex::new(false),
                manifest_version: Mutex::new(None),
                fetches: Mutex::new(Vec::new()),
            }
        }

        /// Adds or replaces a zone with a single A record at `www.<apex>`.
        pub fn upsert(&self, id: &str, apex: &str, version: u64, ip: &str) {
            self.upsert_doc(ZoneDocument {
                id: id.to_string(),
                apex: apex.to_string(),
                version,
                soa: soa(apex),
                records: vec![a_record(&format!("www.{apex}"), ip)],
            });
        }

        pub fn upsert_doc(&self, document: ZoneDocument) {
            self.docs.lock().unwrap().insert(document.id.clone(), document);
        }

        pub fn remove(&self, id: &str) {
            self.docs.lock().unwrap().remove(id);
        }

        pub fn fail_zone(&self, id: &str, failing: bool) {
            let mut set = self.failing.lock().unwrap();
            if failing {
                set.insert(id.to_string());
            } else {
                set.remove(id);
            }
        }

        pub fn fail_manifest(&self, failing: bool) {
            *self.manifest_failing.lock().unwrap() = failing;
        }

        pub fn set_manifest_version(&self, version: Option<u64>) {
            *self.manifest_version.lock().unwrap() = version;
        }

        /// Ids fetched since the last `clear_fetches`, sorted for stable assertions.
        pub fn fetches(&self) -> Vec<String> {
            let mut ids = self.fetches.lock().unwrap().clone();
            ids.sort();
            ids
        }

        pub fn clear_fetches(&self) {
            self.fetches.lock().unwrap().clear();
        }
    }

    impl ZoneSource for MockSource {
        async fn manifest(&self) -> Result<ZoneManifest, SyncError> {
            if *self.manifest_failing.lock().unwrap() {
                return Err(SyncError::Http("mock manifest failure".into()));
            }
            let docs = self.docs.lock().unwrap();
            let mut zones: Vec<ZoneRef> = docs
                .values()
                .map(|d| ZoneRef { id: d.id.clone(), apex: d.apex.clone(), version: d.version })
                .collect();
            zones.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(ZoneManifest { version: *self.manifest_version.lock().unwrap(), zones })
        }

        async fn zone(&self, id: &str) -> Result<ZoneDocument, SyncError> {
            self.fetches.lock().unwrap().push(id.to_string());
            if self.failing.lock().unwrap().contains(id) {
                return Err(SyncError::ZoneFetch { id: id.to_string(), reason: "mock failure".into() });
            }
            self.docs
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .ok_or_else(|| SyncError::ZoneFetch { id: id.to_string(), reason: "not found".into() })
        }
    }
}
