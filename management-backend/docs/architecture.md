# Architecture

## The three systems

```
┌────────────────────┐        ┌──────────────────────┐       ┌────────────────────┐
│  hermes-frontend   │        │     hermes-mgmt      │       │   hermes (Rust)    │
│  React, Cognito    │───────▶│  ASP.NET 10          │◀──────│  authoritative DNS │
│                    │  JWT   │  DynamoDB + Redis?   │  pull │                    │
└────────────────────┘        └──────────────────────┘       └────────────────────┘
                                        │                              ▲
                                        └──── POST /admin/reload ──────┘
                                                  (notify)
```

The frontend and the DNS server **never** talk to each other. This service is the only writer,
and the only thing that knows both contracts.

Some shapes are identical across all three on purpose — the SOA fields, the record fields — so
translating a stored zone into what Hermes parses is a field copy rather than a transformation.

## Layers

```
Api  ──▶  Service  ──▶  Common  ◀──  Repository
 └──────────────────────────────────────┘
              (DI wiring only)
```

- Common holds the domain model, both wire contracts, the repository interfaces, and
  `AppException`. It has no dependencies.
- Repository is DynamoDB and the cache. It implements Common's interfaces.
- Service is all the business rules. It depends on the interfaces, never on DynamoDB or
  ASP.NET, and the authenticated caller reaches it as a plain `Caller` record.
- Api is the controllers, auth schemes, versioning, error middleware, and DI.

Api references Repository only to register it. That single edge is what keeps Service free of
the host and the database, and so unit-testable.

## Data model

One DynamoDB table, `PK`/`SK`:

| Item | PK | SK | GSI1PK / GSI1SK | GSI2PK / GSI2SK |
|---|---|---|---|---|
| Profile | `USER#{sub}` | `PROFILE` | — | — |
| Zone | `ZONE#{id}` | `META` | `USER#{owner}` / `APEX#{apex}` | `ZONES` / `{id}` |
| Record | `ZONE#{id}` | `REC#{recordId}` | — | — |
| Apex claim | `APEX#{apex}` | `CLAIM` | — | — |

Records share their zone's partition key. That makes reading a whole zone, which is the
zone-sync hot path, a single Query, and it puts a record write and its zone's version bump in
the same partition.

GSI1 answers "zones owned by this user", the dashboard's first screen. GSI2 answers "every
zone" for the manifest, on a constant partition key, projecting only `Apex` and `Version`. One
hot partition is fine at hundreds of zones and keeps the manifest a Query instead of a Scan;
shard it past ~10k.

The apex claim is what makes zone creation race-safe. Creating a zone is a `TransactWriteItems`
of the claim and the zone, both conditional on not already existing. A read-then-write would
let two concurrent creates of the same apex both succeed.

`RecordCount` lives denormalized on the zone item, maintained by record writes. Counting on
read would turn listing N zones into N extra queries on the screen every user loads first.

## Versioning and propagation

`Zone.Version` is one counter serving two consumers: the dashboard displays it, and Hermes
diffs it per zone to decide what to refetch. Every write bumps it and the SOA serial with a
single atomic `ADD` — not a read-modify-write, because two concurrent record edits on one zone
must both move the version or Hermes would skip refetching one of them.

A write then:

1. commits,
2. invalidates the cached manifest and zone document (before notifying, or the fleet can
   refetch fast enough to repopulate the cache with the pre-write value),
3. `POST /admin/reload` to every Hermes instance, in parallel.

Steps 2 and 3 **cannot fail the caller's write**. The data is already committed and Hermes
polls regardless, so a failure there costs latency, not correctness — and reporting an error
for a change that did happen is the worse outcome. `ZonePublisher` is the one place this
happens, so there is one place to audit.

## Delegation and activation

A new zone starts `pending`. A background scanner (`DelegationScanner`, driven by
`DelegationMonitorService`) polls the parent zone until it delegates to our nameservers, then
flips the zone to `active`.

### Zones are published from creation, not from activation

This is the important part, and it is the opposite of what "activate, then propagate" suggests.

**A zone and its records go to the DNS fleet the moment they are created.** Activation is a
status flag for the dashboard — it does not gate publication.

It has to work this way. Delegation becomes visible to the world the instant the registrar's
change propagates, which is *before* our scanner notices. If publication waited for
activation, every zone would pass through a window where the parent zone points at Hermes but
Hermes has no such zone — and every query in that window would fail. Publishing up front means
the records a user added while waiting are already live when the delegation lands, and the
switchover is seamless.

So "users can add entries before the zone is delegated" is not a concession; it is what makes
the handover work. `PendingZoneUsabilityTests` pins it, including that the zone document Hermes
receives is byte-identical either side of activation.

### What counts as delegated

`DelegationPolicy.IsDelegated` requires **every** expected nameserver to be present. A
registrar mid-edit commonly shows a mix of old and new, and treating a partial match as
delegated would mark a domain live while half its queries still go to the previous provider.
Extra nameservers beyond ours are tolerated — some registrars add their own.

The lookup reads the **authority** section of an NS query, not just the answer section. Once
Hermes is reachable, a recursive resolver can answer an NS query from the zone's own apex NS
records — our copy — which would read as delegated for a domain the registrar was never pointed
at. The parent's referral is the only trustworthy signal.

### Scheduling

`NextDelegationCheckAt` on the zone doubles as the sort key of a **sparse GSI3**:

- The mapper writes `GSI3PK`/`GSI3SK` **only while a zone is pending**. Since a zone write is a
  full `PutItem`, activation removes it from the index with no cleanup step. The index holds
  only what is actually awaiting delegation.
- The sort key is the due time in ISO-8601 UTC, which sorts lexicographically the same way it
  sorts chronologically — so "which zones are due" is `GSI3PK = PENDING AND GSI3SK <= now`, a
  bounded range query rather than a scan.

The interval backs off with the zone's age: every 2 minutes for the first hour, 15 minutes for
the first day, hourly for a week, then every 6 hours. Registrar changes usually land within
minutes to a couple of days; without the backoff a domain nobody ever delegated would be
re-queried at the fast rate forever, against public resolvers that will eventually start
refusing us.

### Several replicas

Every replica runs the scanner and they all see the same due zones. Before doing a DNS lookup,
a replica **claims** the zone with `TryClaimDelegationCheckAsync` — a conditional write that
only succeeds if `NextDelegationCheckAt` still holds the value that replica read. Losing that
race is normal and costs nothing; the winner does the lookup.

Claiming happens *before* the lookup rather than after, so a crash mid-check does not leave the
zone hammering the resolver on every subsequent pass.

### Demotion

A zone that loses its delegation is **not** demoted back to `pending`. A transient resolver
failure would otherwise flap the status, and it would change nothing operationally — the zone
stays published either way. Worth revisiting if it turns out users want the warning.

## SOA serials

`NextSerial` returns `max(current + 1, YYYYMMDD01)`.

Serials must never decrease: a secondary that sees a lower serial than it holds refuses the
transfer and keeps serving stale data until the zone is expired by hand. The `max()` covers
midnight, clock skew, and more than 99 changes in a day — where the bare `YYYYMMDDnn` scheme
overflows its counter and wraps backwards.

Clients round-trip the serial back to us and it is **always ignored**.

## Apex CNAME flattening

RFC 1034 §3.6.2 forbids a CNAME at a name holding other data, and the apex always holds SOA and
NS. Hermes's answer is the `ALIAS` record, which it resolves and flattens into A/AAAA answers.

So: **a CNAME whose name resolves to the apex is stored as an ALIAS.**

- The API accepts `{"type":"CNAME","name":"example.com"}` and returns
  `{"type":"ALIAS",...}` — the dashboard needs the real stored type to know it should render
  the flattening footnote.
- `ALIAS` submitted directly is a 422. It is not a type a user picks; accepting it would allow
  an ALIAS at a non-apex name, which Hermes resolves but which is not valid DNS.
- `ALIAS` below the apex is rejected in validation too.

## Authentication

Two independent schemes, because there are two unrelated callers:

| | Dashboard | Zone sync |
|---|---|---|
| Scheme | `Cognito` (JWT bearer) | `ZoneSync` (static bearer) |
| Caller | A user | The DNS fleet |
| Scope | Only their own zones | Every zone, every tenant |
| Rotation | Cognito | Redeploy both sides |

They never share a policy. A user credential on `/sync/v1` and a sync token on `/api/v1` both
return 401, and there are tests for both directions — the sync API exposes every tenant's data,
so that boundary is the one most worth pinning.

The static token is compared in fixed time; ordinary string equality leaks its prefix to anyone
who can measure the response.

## Errors

Services throw `AppException`; one middleware turns it into the RFC 7807 body the dashboard
binds to. Keys in `errors` match request-body property names — that is what lets the frontend
highlight the offending input.

Unexpected exceptions become an opaque 500. A DynamoDB error string can carry table names and
key structure.

| Status | When |
|---|---|
| 401 | Missing/invalid token |
| 404 | Does not exist, **or is not owned by the caller** |
| 409 | Duplicate zone, CNAME/other-data clash, duplicate record |
| 422 | Validation failure, with `errors` |

## Name expansion

`ToFqdn` follows the BIND convention: a name without a trailing dot is relative to the origin.
`@` and empty mean the apex, `www` becomes `www.example.com` — and `www.other.com` becomes
`www.other.com.example.com`.

That last one surprises people, which is why the dashboard's record dialog shows the resulting
FQDN under the name field. It also means a record can never land outside its zone through this
path; the in-zone check in `DnsValidator` is defence in depth for other callers.
