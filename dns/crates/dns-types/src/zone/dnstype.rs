use crate::types::Name;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SOA {
    pub mname: Name,
    pub rname: Name,
    pub serial: u32,
    pub refresh: i32,
    pub retry: i32,
    pub expire: i32,
    pub minimum: u32, // negative caching TTL
}