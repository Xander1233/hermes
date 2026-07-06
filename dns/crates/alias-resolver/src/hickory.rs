use std::net::IpAddr;
use std::time::Duration;
use hickory_resolver::TokioResolver;
use hickory_resolver::config::{NameServerConfig, ResolverConfig, ResolverOpts};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::proto::rr::{RecordRef, RecordType};
use hickory_resolver::proto::rr::rdata::{A, AAAA, CNAME};
use dns_types::types::Name;
use crate::error::ResolveError;
use crate::upstream::{StepAnswer, Upstream};

pub struct HickoryUpstream {
    resolver: TokioResolver,
}

impl HickoryUpstream {
    pub fn new(upstreams: &[IpAddr], timeout: Duration) -> Result<Self, ResolveError> {
        if upstreams.is_empty() {
            return Err(ResolveError::Upstream("no upstream recursors configured".into()));
        }

        let name_servers: Vec<NameServerConfig> = upstreams.iter().map(|ip| NameServerConfig::udp_and_tcp(*ip)).collect();
        let config = ResolverConfig::from_parts(None, Vec::new(), name_servers);

        let mut opts = ResolverOpts::default();
        opts.timeout = timeout;

        let mut builder = TokioResolver::builder_with_config(config, TokioRuntimeProvider::default());
        *builder.options_mut() = opts;
        let resolver = builder.build().map_err(|e| ResolveError::Upstream(format!("failed to build resolver: {e}")))?;

        Ok(HickoryUpstream { resolver })
    }
}

fn to_hickory(name: &Name) -> Result<hickory_resolver::proto::rr::Name, ResolveError> {
    hickory_resolver::proto::rr::Name::from_ascii(name.to_display()).map_err(|e| ResolveError::Upstream(format!("invalid name: {e}")))
}

fn from_hickory(name: &hickory_resolver::proto::rr::Name) -> Result<Name, ResolveError> {
    Name::from_ascii(&name.to_ascii()).map_err(|e| ResolveError::Upstream(format!("invalid cname target: {e:?}")))
}

impl Upstream for HickoryUpstream {
    async fn resolve_step(&self, name: &Name) -> Result<StepAnswer, ResolveError> {
        let hname = to_hickory(name)?;

        if let Ok(lookup) = self.resolver.lookup(hname.clone(), RecordType::CNAME).await {
            for record in lookup.answers() {
                if let Ok(cname) = RecordRef::<CNAME>::try_from(record) {
                    return Ok(StepAnswer {
                        cname: Some(from_hickory(&cname.data().0)?),
                        v4: Vec::new(),
                        v6: Vec::new(),
                        ttl: cname.ttl(),
                    });
                }
            }
        }

        let mut v4 = Vec::new();
        let mut v6 = Vec::new();
        let mut ttl = u32::MAX;

        if let Ok(lookup) = self.resolver.lookup(hname.clone(), RecordType::A).await {
            for record in lookup.answers() {
                if let Ok(a) = RecordRef::<A>::try_from(record) {
                    v4.push(a.data().0);
                    ttl = ttl.min(a.ttl());
                }
            }
        }
        
        if let Ok(lookup) = self.resolver.lookup(hname, RecordType::AAAA).await {
            for record in lookup.answers() {
                if let Ok(aaaa) = RecordRef::<AAAA>::try_from(record) {
                    v6.push(aaaa.data().0);
                    ttl = ttl.min(aaaa.ttl());
                }
            }
        }

        if v4.is_empty() && v6.is_empty() {
            return Err(ResolveError::NoAddresses);
        }

        Ok(StepAnswer { cname: None, v4, v6, ttl: if ttl == u32::MAX { 300 } else { ttl } })
    }
}
