use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};

use dns_types::types::Name;
use dns_types::zone::Zones;
use dns_types::zone::dnstype::SOA;
use dns_types::zone::resource_record::RecordData;
use dns_types::zone::rr_set::RRSet;

use super::{DnsConfig, RecordConfig, SoaConfig, ZoneConfig};
use crate::error::ConfigError;

/// Builds a [`Zones`] tree from a parsed [`DnsConfig`], validating names and records.
pub fn build_zones(config: &DnsConfig) -> Result<Zones, ConfigError> {
    let mut zones = Zones::new();
    for zone in &config.zones {
        apply_zone(&mut zones, zone)?;
    }
    Ok(zones)
}

/// Adds (or replaces) a single zone in `zones`, returning its parsed apex.
///
/// This is the unit of incremental update: applying a changed zone from a delta feed only touches
/// that zone, leaving the rest of the snapshot shared.
pub fn apply_zone(zones: &mut Zones, zone: &ZoneConfig) -> Result<Name, ConfigError> {
    let apex = parse_name(&zone.apex)?;
    zones.add_zone(apex.clone(), build_soa(&zone.soa)?, zone.soa.ttl);

    // Group records by (owner, type) so several records of the same type at the same name form
    // one RRSet — `upsert_rrset` replaces the whole set per type.
    let mut grouped: HashMap<(Name, u16), (u32, Vec<RecordData>)> = HashMap::new();
    for record in &zone.records {
        let owner = parse_name(&record.name)?;
        if !owner.is_in_zone(&apex) {
            return Err(ConfigError::RecordOutsideZone {
                owner: record.name.clone(),
                apex: zone.apex.clone(),
            });
        }

        let data = build_record_data(record)?;
        let entry = grouped.entry((owner, data.rtype())).or_insert((record.ttl, Vec::new()));
        entry.1.push(data);
    }

    for ((owner, rtype), (ttl, records)) in grouped {
        zones.upsert_rrset(&apex, owner, RRSet::new(rtype, ttl, records));
    }

    Ok(apex)
}

fn parse_name(input: &str) -> Result<Name, ConfigError> {
    Name::from_ascii(input).map_err(|source| ConfigError::InvalidName {
        name: input.to_string(),
        source,
    })
}

fn build_soa(soa: &SoaConfig) -> Result<SOA, ConfigError> {
    Ok(SOA {
        mname: parse_name(&soa.mname)?,
        rname: parse_name(&soa.rname)?,
        serial: soa.serial,
        refresh: soa.refresh,
        retry: soa.retry,
        expire: soa.expire,
        minimum: soa.minimum,
    })
}

fn invalid_record(record: &RecordConfig, reason: impl Into<String>) -> ConfigError {
    ConfigError::InvalidRecord {
        owner: record.name.clone(),
        reason: reason.into(),
    }
}

fn require_data(record: &RecordConfig) -> Result<&str, ConfigError> {
    record.data.as_deref().ok_or_else(|| {
        invalid_record(record, format!("record type '{}' requires a `data` field", record.rtype))
    })
}

fn build_record_data(record: &RecordConfig) -> Result<RecordData, ConfigError> {
    match record.rtype.to_ascii_uppercase().as_str() {
        "A" => {
            let addr: Ipv4Addr = require_data(record)?
                .parse()
                .map_err(|_| invalid_record(record, "invalid IPv4 address"))?;
            Ok(RecordData::A(addr))
        }
        "AAAA" => {
            let addr: Ipv6Addr = require_data(record)?
                .parse()
                .map_err(|_| invalid_record(record, "invalid IPv6 address"))?;
            Ok(RecordData::AAAA(addr))
        }
        "NS" => Ok(RecordData::NS(parse_name(require_data(record)?)?)),
        "CNAME" => Ok(RecordData::CNAME(parse_name(require_data(record)?)?)),
        // ALIAS (apex flattening): `data` is the target whose addresses are served for A/AAAA.
        "ALIAS" => Ok(RecordData::Alias { target: parse_name(require_data(record)?)? }),
        "PTR" => Ok(RecordData::PTR(parse_name(require_data(record)?)?)),
        "MX" => {
            let preference = record
                .preference
                .ok_or_else(|| invalid_record(record, "MX record requires a `preference` field"))?;
            Ok(RecordData::MX {
                preference,
                exchange: parse_name(require_data(record)?)?,
            })
        }
        "TXT" => Ok(RecordData::TXT(vec![require_data(record)?.as_bytes().to_vec()])),
        "SRV" => {
            let priority = record.priority.ok_or_else(|| invalid_record(record, "SRV requires `priority`"))?;
            let weight = record.weight.ok_or_else(|| invalid_record(record, "SRV requires `weight`"))?;
            let port = record.port.ok_or_else(|| invalid_record(record, "SRV requires `port`"))?;
            Ok(RecordData::SRV {
                priority,
                weight,
                port,
                target: parse_name(require_data(record)?)?,
            })
        }
        "CAA" => {
            let tag = record
                .tag
                .clone()
                .ok_or_else(|| invalid_record(record, "CAA record requires a `tag` field"))?;
            Ok(RecordData::CAA {
                flags: record.flags.unwrap_or(0),
                tag: tag.into_bytes(),
                value: require_data(record)?.as_bytes().to_vec(),
            })
        }
        "SOA" => Err(invalid_record(
            record,
            "SOA is configured via the [zone.soa] table, not as a record",
        )),
        other => Err(ConfigError::UnsupportedType(other.to_string())),
    }
}
