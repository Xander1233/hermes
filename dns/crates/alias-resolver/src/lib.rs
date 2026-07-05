pub mod error;
pub mod filter;
pub mod upstream;
pub mod manager;
pub mod hickory;

pub use error::ResolveError;
pub use filter::{filter_addrs, is_forbidden_v4, is_forbidden_v6};
pub use manager::{AliasResolver, ResolverConfig};
pub use upstream::{chase, Resolved, StepAnswer, Upstream};
pub use hickory::HickoryUpstream;
