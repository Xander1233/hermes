# Management backend contract

This is the API the Hermes dashboard expects. It is written to be implementable as-is by the
C# management backend.

The authoritative machine-readable version of these shapes is
[`app/lib/api/types.ts`](../app/lib/api/types.ts). The in-memory mock in
[`app/lib/api/mock.ts`](../app/lib/api/mock.ts) implements the *behaviour* described here —
normalization, conflict rules, version bumps — so you can diff your implementation against
it.

---

## 1. System shape

```
Browser (this app)  ──HTTPS/JSON──▶  Management backend (C#)  ──zone-sync──▶  Hermes DNS
      │                                      │                                    │
   Cognito JWT                     owns users, permissions,                 owns nothing but
                                   metadata, notes, audit                   the live zone tree
```

Three rules that shape everything below:

1. **The frontend never talks to Hermes.** It talks only to the management backend.
2. **The management backend is the source of truth.** Hermes *pulls* from it via the
   zone-sync contract (`docs/zone-sync-contract.md` in the `hermes` repo). Hermes never
   writes back.
3. **Some shapes are deliberately identical to Hermes's** (the SOA, the record fields) so
   your translation into the zone-sync payload is a field copy, not a transformation.

### Relationship to the zone-sync contract

The backend serves two different audiences and must not confuse them:

| | This contract (dashboard) | Zone-sync contract (Hermes) |
|---|---|---|
| Consumer | Browser, per user | Hermes servers, per fleet |
| Auth | Cognito JWT, user-scoped | Shared bearer token, fleet-scoped |
| Scope | Only zones the caller owns | **All** zones, every tenant |
| Includes `notes` | Yes | **No** — strip it |
| Includes record ids | Yes | No |
| Versioning | `version` per zone, informational | `version` per zone, **drives refetch** |

`Zone.version` in this contract and the per-zone `version` in the zone-sync manifest **must
be the same counter**. That is what lets the dashboard show "published" state honestly.

### Publishing flow

On any successful write to a zone (record create/update/delete, SOA update, zone delete):

1. Persist the change.
2. Bump that zone's `version` (and, for record changes, the SOA `serial`).
3. `POST /admin/reload` on each Hermes instance with the fleet admin token, so the change
   propagates in seconds instead of waiting for the poll interval. Failure here is not fatal
   — Hermes polls anyway — but log it.

---

## 2. Conventions

- **Base URL**: everything below is relative to e.g. `https://api.hermes.example/api/v1`.
  The frontend reads it from `VITE_API_BASE_URL`.
- **Auth**: `Authorization: Bearer <Cognito id_token>` on every request. Validate the
  signature against the user pool JWKS, and the `aud`/`iss` claims. `sub` is the user id.
- **Content type**: `application/json`, UTF-8.
- **Names**: all DNS names are **lower-case FQDNs without a trailing dot**
  (`www.example.com`). The frontend normalizes before sending; normalize again server-side.
- **Timestamps**: RFC 3339 UTC (`2026-07-21T09:30:00Z`).
- **Ownership**: every zone belongs to exactly one user. A request for a zone the caller does
  not own returns **`404`**, not `403` — do not leak the existence of other tenants' zones.

### Errors

RFC 7807-shaped. `errors` maps a **field name matching the request body** to messages; the
frontend binds these directly to form inputs, so the key names matter.

```jsonc
{
  "title": "Validation failed",
  "status": 422,
  "detail": "One or more fields are invalid.",   // optional
  "errors": {
    "data": ["Not a valid IPv4 address."],
    "name": ["Must be inside example.com."]
  }
}
```

| Status | When |
|---|---|
| `400` | Malformed JSON / unparseable request |
| `401` | Missing, expired or invalid token |
| `404` | Zone or record does not exist **or is not owned by the caller** |
| `409` | Conflict — duplicate zone, CNAME/other-data clash, duplicate record |
| `422` | Validation failure (use `errors`) |
| `429` | Rate limited |

---

## 3. Profile

### `GET /me`

Returns the caller's profile, creating it on first call from the token claims (the dashboard
has no separate registration step).

```jsonc
{
  "id": "a1b2c3d4-...",              // Cognito `sub`, immutable
  "email": "user@example.com",       // from the token; read-only here
  "displayName": "Dev User",
  "company": "Hermes",               // nullable
  "timezone": "Europe/Zurich",       // IANA
  "notificationEmail": null,         // nullable; falls back to `email`
  "notifyOnZoneErrors": true,
  "createdAt": "2026-01-04T10:00:00Z"
}
```

### `PATCH /me`

Body — all fields required, all mutable:

```jsonc
{
  "displayName": "Dev User",
  "company": null,
  "timezone": "Europe/Zurich",
  "notificationEmail": null,
  "notifyOnZoneErrors": true
}
```

Returns the updated profile. `email` and `id` are **not** accepted; email changes are a
Cognito flow, not an API call.

Validate: `displayName` 1–100 chars; `timezone` a known IANA id; `notificationEmail` a valid
address when non-null.

---

## 4. Zones

### `Zone`

```jsonc
{
  "id": "zone_7f3a91cc",             // opaque, backend-generated, stable forever
  "apex": "example.com",
  "status": "active",                // "pending" | "active" | "error"
  "statusMessage": null,             // required when status is "error"
  "version": 7,
  "nameservers": ["ns1.hermes-dns.net", "ns2.hermes-dns.net"],
  "recordCount": 5,
  "createdAt": "2026-01-04T10:00:00Z",
  "updatedAt": "2026-07-20T18:22:11Z"
}
```

`ZoneDetail` is a `Zone` plus `"soa": { … }`.

**`id` is what Hermes's zone-sync manifest uses.** Never reuse or recycle it. The apex may
change under a stable id (Hermes handles the rename); the id may not.

**`status`**
| Value | Meaning | Set by |
|---|---|---|
| `pending` | Created, but delegation not yet observed at the parent zone | On create |
| `active` | The parent zone delegates to our nameservers | Delegation check |
| `error` | The zone was rejected or failed to publish | Publish pipeline |

### `GET /zones`

Returns `Zone[]` (no `soa`) for the authenticated user only. Sorted by `apex`.

### `POST /zones`

```jsonc
{ "apex": "example.com" }
```

Returns `201` + `ZoneDetail`.

- Normalize: trim, lower-case, strip a trailing dot, IDN → punycode.
- Reject a name that is not a registrable domain, is on the public suffix list itself, or is
  already owned by **anyone** → `409` with `errors.apex`.
- Assign `nameservers` from the Hermes pool, generate a default SOA (below), `status:
  "pending"`, `version: 1`.
- Create no records. In particular do **not** auto-create the apex NS records — Hermes
  synthesizes authority from the zone's nameserver assignment.

Default SOA for a new zone:

```jsonc
{
  "mname": "ns1.hermes-dns.net",
  "rname": "hostmaster.example.com",
  "serial": 2026072101,              // YYYYMMDDnn
  "refresh": 3600,
  "retry": 600,
  "expire": 604800,
  "minimum": 60,
  "ttl": 3600
}
```

### `GET /zones/{zoneId}` → `ZoneDetail`

### `DELETE /zones/{zoneId}` → `204`

Deletes the zone and every record in it. The zone disappears from the zone-sync manifest,
which is how Hermes learns to stop serving it — there are no tombstones.

### `PUT /zones/{zoneId}/soa`

Body is the full `Soa` object. Returns the updated `ZoneDetail`.

```jsonc
{
  "mname": "ns1.hermes-dns.net",     // FQDN
  "rname": "hostmaster.example.com", // email in DNS form: `@` becomes `.`
  "serial": 2026072101,              // sent back, but server-managed — ignore the client value
  "refresh": 3600,
  "retry": 600,
  "expire": 604800,
  "minimum": 60,
  "ttl": 3600
}
```

Validation the frontend already enforces and the backend must repeat:

| Field | Rule |
|---|---|
| `mname`, `rname` | Non-empty, valid host names |
| `refresh`, `retry` | `> 0` |
| `expire` | `> refresh` |
| `minimum` | `>= 0`, and `<= 86400` is strongly advised (it is the NXDOMAIN cache time) |
| `ttl` | `0 … 604800` |

**`serial` is server-managed.** The client sends back whatever it was given; ignore it and
increment your own. A client-settable serial is a footgun — a decrease makes secondaries stop
accepting transfers.

### `GET /zones/{zoneId}/delegation`

Live check against the parent zone. Do **not** cache aggressively; the user is clicking this
while editing their registrar.

```jsonc
{
  "observedNameservers": ["ns1.oldhost.com", "ns2.oldhost.com"],
  "expectedNameservers": ["ns1.hermes-dns.net", "ns2.hermes-dns.net"],
  "delegated": false,
  "checkedAt": "2026-07-21T09:30:00Z"
}
```

`delegated` is true when the observed set covers the expected set. On the transition to true,
flip `status` from `pending` to `active` as a side effect.

Implementation note: query the *parent* zone's nameservers for the NS RRset, not a recursive
resolver — a recursive answer may be a cached copy of your own authoritative NS records and
will read as delegated when it is not.

---

## 5. Records

### `DnsRecord`

```jsonc
{
  "id": "rec_2b81f0",
  "zoneId": "zone_7f3a91cc",
  "name": "www.example.com",         // FQDN, lower-case, no trailing dot
  "type": "A",
  "ttl": 300,
  "data": "192.0.2.10",

  "preference": null,                // MX
  "priority": null,                  // SRV
  "weight": null,                    // SRV
  "port": null,                      // SRV
  "flags": null,                     // CAA
  "tag": null,                       // CAA

  "notes": "Blue/green: flipped by the deploy pipeline.",

  "createdAt": "2026-01-04T10:00:00Z",
  "updatedAt": "2026-07-20T18:22:11Z"
}
```

### Supported types

Exactly what Hermes can serve — see `build_record_data` in
`crates/config/src/dns.rs` of the `hermes` repo. **Reject anything else with `422`.**

| Type | `data` | Extra fields | Notes |
|---|---|---|---|
| `A` | IPv4 literal | — | |
| `AAAA` | IPv6 literal | — | |
| `CNAME` | target FQDN | — | Not allowed at the apex → see flattening |
| `ALIAS` | target FQDN | — | Apex-only. Never accepted directly from the UI |
| `NS` | nameserver FQDN | — | For delegating subdomains |
| `MX` | mail server FQDN | `preference` (0–65535) | |
| `TXT` | text | — | May be empty |
| `SRV` | target FQDN | `priority`, `weight` (0–65535), `port` (1–65535) | |
| `CAA` | value | `flags` (0–255), `tag` (`issue`\|`issuewild`\|`iodef`) | |
| `PTR` | target FQDN | — | |

`SOA` is **not** a record type here. It is edited through `PUT /zones/{id}/soa`. Reject
`type: "SOA"` with `422`.

TTL range: `0 … 604800`. `0` means "auto" in the UI; treat it as your platform default.

### Apex CNAME flattening — the one non-obvious rule

RFC 1034 forbids a CNAME at a name that has other records, and the apex always has SOA and
NS. So:

> **A `CNAME` whose name resolves to the zone apex is stored as an `ALIAS`.**

- The API **accepts** `{"type": "CNAME", "name": "example.com"}` and **returns**
  `{"type": "ALIAS", "name": "example.com"}`. This normalization is the backend's job; the
  frontend does the same thing locally only so it can warn the user before they submit.
- `ALIAS` is **never** valid at a non-apex name → `422`.
- The dashboard displays a stored `ALIAS` as **"CNAME"** with a small footnote, the way
  Cloudflare presents its flattened apex CNAMEs, because that is the record the user asked
  for. Do not "fix" the response to say `CNAME`; the UI needs the real stored type to know it
  should show the footnote.
- Hermes resolves the target's A/AAAA addresses in the background and answers apex A/AAAA
  queries with them. Nothing extra is required from the backend beyond emitting the `ALIAS`
  record in the zone-sync payload.

### `GET /zones/{zoneId}/records` → `DnsRecord[]`

Sorted by `name`, then `type`.

### `POST /zones/{zoneId}/records` → `201` + `DnsRecord`

### `PUT /zones/{zoneId}/records/{recordId}` → `200` + `DnsRecord`

Both take the same body — the full record, no partial updates:

```jsonc
{
  "name": "www.example.com",
  "type": "A",
  "ttl": 300,
  "data": "192.0.2.10",
  "preference": null,
  "priority": null,
  "weight": null,
  "port": null,
  "flags": null,
  "tag": null,
  "notes": "Optional free-form note, or null"
}
```

The frontend always sends every field, using `null` for the ones the type does not use.

### `DELETE /zones/{zoneId}/records/{recordId}` → `204`

### Record validation

Per type, as in the table above, plus these cross-record rules:

1. **In-zone**: `name` must equal the apex or end with `.<apex>` → `422` on `errors.name`.
2. **CNAME exclusivity** (RFC 1034 §3.6.2): a name holding a `CNAME`/`ALIAS` may hold no
   other record, and a name holding any other record may not gain a `CNAME`/`ALIAS` →
   `409` on `errors.name`.
3. **Duplicates**: same `name` + `type` + `data` → `409` on `errors.data`.
4. **RRset TTLs**: all records sharing a name and type form one RRset and must share a TTL.
   Hermes takes one TTL per RRset, so if you allow a mismatch it silently picks one. Either
   reject it, or apply the new TTL to the whole RRset — the mock does the latter implicitly
   by grouping. **Decide explicitly and document it.**
5. **Wildcards**: `*` is allowed as the left-most label only.

### `notes`

Purely management-side metadata, the equivalent of Cloudflare's record comments.

- Max 1000 characters. Nullable.
- Editable on create and update.
- **Never** included in the zone-sync payload — it is not DNS data. Strip it when building
  the zone document for Hermes.

---

## 6. Building the zone-sync payload

For reference, this is how a stored zone maps onto what Hermes fetches
(`GET /zones/{id}` on the *zone-sync* endpoint — a different endpoint from this contract's,
even though the path happens to look similar):

```jsonc
{
  "id": "zone_7f3a91cc",             // same id as this contract
  "apex": "example.com",
  "version": 7,                      // same counter as Zone.version
  "soa": { /* the Soa object, verbatim */ },
  "records": [
    // per record: name, type, ttl, data + the type-specific fields.
    // NO id, NO notes, NO createdAt/updatedAt.
    { "name": "example.com", "type": "ALIAS", "ttl": 300, "data": "origin.hermes-edge.net" },
    { "name": "example.com", "type": "MX",    "ttl": 3600, "data": "mail.example.com",
      "preference": 10 }
  ]
}
```

The manifest (`GET /zones` on the zone-sync endpoint) lists **every** zone across **all**
tenants with its `id`, `apex` and `version` — Hermes refetches only the ones whose version
moved, and deletes any it tracks that the manifest no longer lists.

---

## 7. Open decisions for the backend

Things this contract deliberately leaves to you. The frontend does not depend on the outcome,
but pick one and write it down:

1. **RRset TTL mismatch** — reject, or coerce the whole set? (See rule 4 above.)
2. **Zone limits per user** — how many zones, how many records per zone. If you enforce a
   cap, return `422` with `errors.apex` / `errors.name` so the message reaches the form.
3. **Audit log** — who changed what, when. Not currently surfaced in the UI, but `notes`
   plus an audit trail is what users will eventually ask for.
4. **Zone transfer / import** — pasting a BIND zone file is the usual first feature request
   after this UI ships.
5. **Delegation re-checks** — the UI only checks on demand. A background job that flips
   `pending` → `active` (and emails when `notifyOnZoneErrors` is set) is probably wanted.
