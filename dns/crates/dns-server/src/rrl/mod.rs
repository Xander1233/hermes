use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use dns_types::types::Name;

mod config;
mod decision;
mod ipnet;

pub use config::RrlConfig;
pub use decision::{ResponseClass, RrlDecision};
pub use ipnet::IpNet;

use ipnet::mask;

const ALL_CLASS: u8 = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RrlKey {
    prefix: IpAddr,
    class: u8,
    name: u64,
}

#[derive(Debug, Clone, Copy)]
struct Slot {
    tag: u64,
    tokens: f64,
    last_refill: Instant,
}

pub struct Rrl {
    config: RrlConfig,
    shards: Vec<Mutex<Box<[Option<Slot>]>>>,
    slots_per_shard: usize,
    slip_counter: AtomicU64,
    collisions: AtomicU64,
}

const SHARD_COUNT: usize = 64;

impl Rrl {
    pub fn new(config: RrlConfig) -> Self {
        let slots_per_shard = config.table_size.div_ceil(SHARD_COUNT).max(1);

        let shards = (0..SHARD_COUNT)
            .map(|_| Mutex::new(vec![None; slots_per_shard].into_boxed_slice()))
            .collect();

        Rrl {
            config,
            shards,
            slots_per_shard,
            slip_counter: AtomicU64::new(0),
            collisions: AtomicU64::new(0),
        }
    }

    pub fn config(&self) -> &RrlConfig {
        &self.config
    }

    pub fn collisions(&self) -> u64 {
        self.collisions.load(Ordering::Relaxed)
    }

    pub fn occupancy(&self) -> (usize, usize) {
        let mut occupied = 0;
        let mut total = 0;

        for shard in &self.shards {
            let guard = shard.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

            total += guard.len();
            occupied += guard.iter().filter(|slot| slot.is_some()).count();
        }

        (occupied, total)
    }

    pub fn check(
        &self,
        client: SocketAddr,
        class: ResponseClass,
        qname: Option<&Name>,
        now: Instant,
    ) -> RrlDecision {
        if !self.config.enabled {
            return RrlDecision::Send;
        }

        let address = client.ip();
        if self.config.exempt.iter().any(|net| net.contains(address)) {
            return RrlDecision::Send;
        }

        if self.config.all_per_second > 0 {
            let key = self.prefix_key(address, ALL_CLASS, 0);

            if !self.take_token(key, self.config.all_per_second, now) {
                return self.slip_decision();
            }
        }

        let rate = match class {
            ResponseClass::NoError => self.config.responses_per_second,
            ResponseClass::NxDomain => self.config.nxdomains_per_second,
            ResponseClass::Error => self.config.errors_per_second,
        };

        if rate == 0 {
            return RrlDecision::Send;
        }

        let key = self.key(address, class, qname);

        if self.take_token(key, rate, now) {
            RrlDecision::Send
        } else {
            self.slip_decision()
        }
    }

    fn key(&self, address: IpAddr, class: ResponseClass, qname: Option<&Name>) -> RrlKey {
        let name = match (class, qname) {
            (ResponseClass::NoError, Some(name)) => hash_of(name.raw()),
            _ => 0,
        };

        self.prefix_key(address, class as u8, name)
    }

    fn prefix_key(&self, address: IpAddr, class: u8, name: u64) -> RrlKey {
        let prefix_len = match address {
            IpAddr::V4(_) => self.config.ipv4_prefix_len,
            IpAddr::V6(_) => self.config.ipv6_prefix_len,
        };

        RrlKey { prefix: mask(address, prefix_len), class, name }
    }

    fn take_token(&self, key: RrlKey, rate: u32, now: Instant) -> bool {
        let hash = hash_of(&key);
        let capacity = f64::from(rate) * f64::from(self.config.window_secs.max(1));

        let shard_index = (hash % SHARD_COUNT as u64) as usize;
        let slot_index = ((hash / SHARD_COUNT as u64) % self.slots_per_shard as u64) as usize;

        let mut shard = self.shards[shard_index].lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        let slot = &mut shard[slot_index];

        match slot {
            Some(existing) if existing.tag == hash => {
                let elapsed = now.saturating_duration_since(existing.last_refill).as_secs_f64();

                existing.tokens = (existing.tokens + elapsed * f64::from(rate)).min(capacity);
                existing.last_refill = now;

                if existing.tokens >= 1.0 {
                    existing.tokens -= 1.0;

                    true
                } else {
                    false
                }
            }
            Some(_) => {
                self.collisions.fetch_add(1, Ordering::Relaxed);
                *slot = Some(Slot { tag: hash, tokens: capacity - 1.0, last_refill: now });

                true
            }
            None => {
                *slot = Some(Slot { tag: hash, tokens: capacity - 1.0, last_refill: now });

                true
            }
        }
    }

    fn slip_decision(&self) -> RrlDecision {
        match self.config.slip {
            0 => RrlDecision::Drop,
            1 => RrlDecision::Slip,
            slip => {
                let n = self.slip_counter.fetch_add(1, Ordering::Relaxed);
                if n.is_multiple_of(u64::from(slip)) { RrlDecision::Slip } else { RrlDecision::Drop }
            }
        }
    }
}

fn hash_of<T: Hash>(value: T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);

    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn config() -> RrlConfig {
        RrlConfig {
            responses_per_second: 10,
            nxdomains_per_second: 5,
            errors_per_second: 5,
            // High enough not to interfere with the per-class tests; exercised on its own in
            // the `all_per_second` tests below.
            all_per_second: 100_000,
            // A window of 1 makes capacity == rate, so the arithmetic in tests is obvious.
            window_secs: 1,
            slip: 0,
            ..RrlConfig::default()
        }
    }

    fn client(address: &str) -> SocketAddr {
        format!("{address}:12345").parse().unwrap()
    }

    fn name(labels: &[&str]) -> Name {
        let mut raw = Vec::new();
        for label in labels {
            raw.push(label.len() as u8);
            raw.extend_from_slice(label.as_bytes());
        }
        raw.push(0);
        Name::new(raw)
    }

    /// Sends `count` responses and reports how many were allowed through.
    fn allowed(rrl: &Rrl, from: SocketAddr, class: ResponseClass, qname: Option<&Name>, count: usize, now: Instant) -> usize {
        (0..count)
            .filter(|_| rrl.check(from, class, qname, now) == RrlDecision::Send)
            .count()
    }

    #[test]
    fn under_the_limit_everything_is_sent() {
        let rrl = Rrl::new(config());
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);

        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NoError, Some(&qname), 10, now), 10);
    }

    #[test]
    fn over_the_limit_is_dropped() {
        let rrl = Rrl::new(config());
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);

        // Capacity is 10; the 11th onwards must not get through.
        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NoError, Some(&qname), 50, now), 10);
    }

    #[test]
    fn disabled_never_limits() {
        let rrl = Rrl::new(RrlConfig { enabled: false, ..config() });
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);

        // The feature must be genuinely off, not merely quiet.
        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NoError, Some(&qname), 500, now), 500);
    }

    #[test]
    fn a_random_subdomain_nxdomain_flood_is_still_limited() {
        // The test that fails if NXDOMAIN is keyed on the question name: every query carries a
        // name nobody has ever seen, which is exactly what a random-subdomain attack looks like.
        let rrl = Rrl::new(config());
        let now = Instant::now();

        let mut sent = 0;
        for i in 0..1000 {
            let qname = name(&[&format!("random{i}"), "example", "com"]);
            if rrl.check(client("192.0.2.1"), ResponseClass::NxDomain, Some(&qname), now) == RrlDecision::Send {
                sent += 1;
            }
        }

        assert_eq!(sent, 5, "NXDOMAIN must be limited per client, regardless of the name");
    }

    #[test]
    fn a_random_subdomain_flood_answered_noerror_is_still_limited() {
        // Regression test for a hole found by live testing, not by the unit tests.
        //
        // A zone with a wildcard (`*.example.com`) answers every made-up subdomain NOERROR
        // rather than NXDOMAIN. Because the NOERROR bucket is keyed on the question name — so
        // that heavy traffic to one name cannot starve another — every one of those unique
        // names got a fresh budget and the whole flood went through. `all_per_second` is the
        // coarse per-client bucket that catches it.
        let rrl = Rrl::new(RrlConfig { all_per_second: 20, ..config() });
        let now = Instant::now();

        let mut sent = 0;
        for i in 0..1000 {
            let qname = name(&[&format!("random{i}"), "example", "com"]);
            if rrl.check(client("192.0.2.1"), ResponseClass::NoError, Some(&qname), now) == RrlDecision::Send {
                sent += 1;
            }
        }

        assert_eq!(sent, 20, "a wildcard-answered random-subdomain flood must still be capped");
    }

    #[test]
    fn the_coarse_bucket_spans_every_class() {
        // Splitting a flood across response classes must not multiply the budget.
        let rrl = Rrl::new(RrlConfig { all_per_second: 10, ..config() });
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);
        let from = client("192.0.2.1");

        let mut sent = 0;
        for i in 0..300 {
            let class = match i % 3 {
                0 => ResponseClass::NoError,
                1 => ResponseClass::NxDomain,
                _ => ResponseClass::Error,
            };
            let qname = name(&[&format!("n{i}"), "example", "com"]);
            if rrl.check(from, class, Some(&qname), now) == RrlDecision::Send {
                sent += 1;
            }
        }

        assert_eq!(sent, 10);
        // And it is genuinely per-prefix, not global.
        assert_eq!(
            rrl.check(client("198.51.100.1"), ResponseClass::NoError, Some(&qname), now),
            RrlDecision::Send
        );
    }

    #[test]
    fn a_coarse_rate_of_zero_disables_the_catch_all() {
        let rrl = Rrl::new(RrlConfig { all_per_second: 0, ..config() });
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);

        // Only the per-name NOERROR limit applies.
        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NoError, Some(&qname), 50, now), 10);
    }

    #[test]
    fn two_ips_in_one_prefix_share_a_budget() {
        // The spoofing defence: an attacker forging addresses across a /24 must not get a
        // fresh budget per forged address.
        let rrl = Rrl::new(config());
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);

        let first = allowed(&rrl, client("192.0.2.1"), ResponseClass::NoError, Some(&qname), 10, now);
        let second = allowed(&rrl, client("192.0.2.99"), ResponseClass::NoError, Some(&qname), 10, now);

        assert_eq!(first, 10);
        assert_eq!(second, 0, "a second address in the same /24 must not get its own budget");
    }

    #[test]
    fn different_prefixes_are_independent() {
        let rrl = Rrl::new(config());
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);

        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NoError, Some(&qname), 10, now), 10);
        assert_eq!(allowed(&rrl, client("198.51.100.1"), ResponseClass::NoError, Some(&qname), 10, now), 10);
    }

    #[test]
    fn different_names_are_independent_for_noerror() {
        let rrl = Rrl::new(config());
        let now = Instant::now();

        let first = name(&["a", "example", "com"]);
        let second = name(&["b", "example", "com"]);

        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NoError, Some(&first), 10, now), 10);
        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NoError, Some(&second), 10, now), 10);
    }

    #[test]
    fn classes_have_separate_budgets() {
        let rrl = Rrl::new(config());
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);

        // Exhausting NOERROR must not also block NXDOMAIN, or one flood would silence
        // everything else.
        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NoError, Some(&qname), 50, now), 10);
        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NxDomain, Some(&qname), 50, now), 5);
    }

    #[test]
    fn tokens_refill_over_time() {
        let rrl = Rrl::new(config());
        let start = Instant::now();
        let qname = name(&["www", "example", "com"]);

        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NoError, Some(&qname), 50, start), 10);

        // One second at 10/s refills the whole bucket.
        let later = start + Duration::from_secs(1);
        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NoError, Some(&qname), 50, later), 10);
    }

    #[test]
    fn refill_is_capped_at_capacity() {
        let rrl = Rrl::new(config());
        let start = Instant::now();
        let qname = name(&["www", "example", "com"]);

        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NoError, Some(&qname), 10, start), 10);

        // An hour of idling must not bank an hour's worth of burst.
        let later = start + Duration::from_secs(3600);
        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NoError, Some(&qname), 100, later), 10);
    }

    #[test]
    fn slip_of_two_truncates_every_second_over_limit_response() {
        let rrl = Rrl::new(RrlConfig { slip: 2, ..config() });
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);
        let from = client("192.0.2.1");

        // Drain the bucket first.
        allowed(&rrl, from, ResponseClass::NoError, Some(&qname), 10, now);

        let mut slipped = 0;
        let mut dropped = 0;
        for _ in 0..100 {
            match rrl.check(from, ResponseClass::NoError, Some(&qname), now) {
                RrlDecision::Slip => slipped += 1,
                RrlDecision::Drop => dropped += 1,
                RrlDecision::Send => panic!("bucket was drained; nothing should be sent"),
            }
        }

        assert_eq!(slipped, 50);
        assert_eq!(dropped, 50);
    }

    #[test]
    fn slip_of_zero_never_truncates() {
        let rrl = Rrl::new(RrlConfig { slip: 0, ..config() });
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);
        let from = client("192.0.2.1");

        allowed(&rrl, from, ResponseClass::NoError, Some(&qname), 10, now);

        for _ in 0..20 {
            assert_eq!(rrl.check(from, ResponseClass::NoError, Some(&qname), now), RrlDecision::Drop);
        }
    }

    #[test]
    fn exempt_clients_are_never_limited() {
        let rrl = Rrl::new(RrlConfig {
            exempt: vec![IpNet::parse("127.0.0.0/8").unwrap()],
            ..config()
        });
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);

        assert_eq!(allowed(&rrl, client("127.0.0.1"), ResponseClass::NoError, Some(&qname), 500, now), 500);
    }

    #[test]
    fn a_rate_of_zero_disables_that_class_rather_than_blocking_it() {
        let rrl = Rrl::new(RrlConfig { nxdomains_per_second: 0, ..config() });
        let now = Instant::now();
        let qname = name(&["nope", "example", "com"]);

        assert_eq!(allowed(&rrl, client("192.0.2.1"), ResponseClass::NxDomain, Some(&qname), 100, now), 100);
    }

    #[test]
    fn the_table_does_not_grow_with_distinct_clients() {
        // The property that stops the limiter becoming the memory-exhaustion vector it exists
        // to prevent: a spoofed flood across millions of sources must not allocate per source.
        let rrl = Rrl::new(RrlConfig { table_size: 1024, ..config() });
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);

        let slots = |rrl: &Rrl| -> usize {
            rrl.shards.iter().map(|shard| shard.lock().unwrap().len()).sum()
        };

        let before = slots(&rrl);
        for i in 0..100_000u32 {
            let address = std::net::Ipv4Addr::from(i.wrapping_mul(2654435761));
            rrl.check(SocketAddr::new(IpAddr::V4(address), 53), ResponseClass::NoError, Some(&qname), now);
        }

        assert_eq!(slots(&rrl), before, "the bucket table must be fixed-size");
    }

    #[test]
    fn slot_collisions_are_counted_when_the_table_saturates() {
        // A fixed table must merge unrelated clients once it fills; that merging is exactly
        // what `collisions()` surfaces, so an operator can tell a too-small `table_size` from
        // a genuine attack rather than guessing.
        let rrl = Rrl::new(RrlConfig { table_size: 64, ..config() });
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);

        // Far more distinct client prefixes than there are slots: by pigeonhole most collide.
        for i in 0..1000u32 {
            let address = std::net::Ipv4Addr::from(i.wrapping_mul(2654435761));
            rrl.check(SocketAddr::new(IpAddr::V4(address), 53), ResponseClass::NoError, Some(&qname), now);
        }

        assert!(rrl.collisions() > 0, "a saturated fixed table must report collisions");
    }

    #[test]
    fn occupancy_reports_slots_in_use_against_a_fixed_total() {
        let rrl = Rrl::new(RrlConfig { table_size: 1024, ..config() });
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);

        let (used_before, total) = rrl.occupancy();
        assert_eq!(used_before, 0, "a fresh table is empty");
        assert!(total >= 1024, "the total is the fixed table size");

        rrl.check(client("192.0.2.1"), ResponseClass::NoError, Some(&qname), now);

        let (used_after, total_after) = rrl.occupancy();
        assert!(used_after >= 1, "a checked client occupies at least one slot");
        assert_eq!(total, total_after, "the total never changes");
    }

    #[test]
    fn ipv6_is_bucketed_by_its_own_prefix_length() {
        let rrl = Rrl::new(config());
        let now = Instant::now();
        let qname = name(&["www", "example", "com"]);

        // Same /56, different hosts — one shared budget.
        let first: SocketAddr = "[2001:db8:0:1::1]:53".parse().unwrap();
        let second: SocketAddr = "[2001:db8:0:1::99]:53".parse().unwrap();
        // A different /56 entirely.
        let third: SocketAddr = "[2001:db8:0:ff00::1]:53".parse().unwrap();

        assert_eq!(allowed(&rrl, first, ResponseClass::NoError, Some(&qname), 10, now), 10);
        assert_eq!(allowed(&rrl, second, ResponseClass::NoError, Some(&qname), 10, now), 0);
        assert_eq!(allowed(&rrl, third, ResponseClass::NoError, Some(&qname), 10, now), 10);
    }
}
