# Zone Sync Contract

How the management backend (C#) propagates zone changes to the Hermes authoritative DNS server.

This document specifies the API the backend must implement and the endpoint Hermes exposes.
It is only a contract; nothing here says how the backend should store or authorize data.

---

## 1. Model: pull + notify, diffed per zone

```
[Frontend] → [C# backend]  ← source of truth (zones, records, owners, permissions)
                   │
                   │ (1) commit change, bump that zone's version (and SOA serial)
                   │
                   │ (2) optional: POST /admin/reload  ─────────────┐
                   ↓                                                │
        [Hermes replica(s)] ──(3) GET /zones ── diff ── GET /zones/{id} for changed only
                   │
                   └─ atomically swap the new snapshot into the query path
```

- Pull is what keeps things correct. Hermes polls, which works with any number of replicas
  without the backend tracking them, survives restarts and scale-out on its own, and shrugs off
  a dropped notification.
- Notify is only an optimization that removes the poll delay.
- Per-zone versions mean a pass fetches only what actually changed, deletion needs no
  tombstones, and one tenant's broken zone cannot block another tenant's update.
- The backend is the single source of truth. Hermes is a cache of it, with a local disk
  snapshot so it can still boot and serve when the backend is unreachable.

---

## 2. Endpoints the backend must implement

Base URL is configured in Hermes as `zone_sync.base_url`
(e.g. `http://backend.internal.svc/internal/dns`). Hermes appends the paths below.

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `{base_url}/zones` | Manifest: every zone with its per-zone version |
| `GET` | `{base_url}/zones/{id}` | One complete zone |

### Authentication

If `zone_sync.auth_token` is configured, Hermes sends it on **every** request:

```http
Authorization: Bearer <auth_token>
```

Any non-2xx is a failure. A failed manifest fetch aborts the pass (Hermes keeps serving what it
has); a failed *zone* fetch only skips that zone.

---

### 2.1 `GET /zones` — the manifest

Polled every `poll_interval_secs` (default 15) by every replica, so keep it cheap: it should be
one indexed read returning `id`, `apex`, and `version` per zone.

**Response `200 OK`**

```json
{
  "version": 4711,
  "zones": [
    { "id": "z_a1b2c3", "apex": "example.com",  "version": 12 },
    { "id": "z_d4e5f6", "apex": "customer.io",  "version": 3  },
    { "id": "z_g7h8i9", "apex": "shop.example", "version": 88 }
  ]
}
```

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `version` | u64 | — | **Advisory only.** Surfaced in `/healthz` for humans; never used for diffing |
| `zones[].id` | string | yes | Opaque, stable identifier. Used to fetch the zone |
| `zones[].apex` | string | yes | Zone origin — how the zone is keyed when serving |
| `zones[].version` | u64 | yes | **Must change whenever anything in the zone changes** |

> **The manifest must be complete.** It is the authoritative list of everything Hermes should
> serve. Any zone Hermes holds that is *not* in the manifest is dropped — that is how deletion
> works, and it is also what retires the bootstrap zones loaded from the local `zones.toml`
> once the control plane takes over. Never return a partial manifest.

**`version` semantics.** Hermes only tests it for *equality* against what it already applied:
equal → skip, different → refetch. It does not need to be globally ordered, and there is no
reserved value. The SOA serial is the natural choice, since you have to bump it anyway.

---

### 2.2 `GET /zones/{id}` — one zone

Called only for zones whose `version` differs from what Hermes has. `{id}` is percent-encoded,
so opaque ids may contain any characters.

**Response `200 OK`**

```json
{
  "id": "z_a1b2c3",
  "apex": "example.com",
  "version": 12,
  "soa": {
    "mname": "ns1.example.com",
    "rname": "hostmaster.example.com",
    "serial": 12,
    "refresh": 3600,
    "retry": 600,
    "expire": 604800,
    "minimum": 60,
    "ttl": 3600
  },
  "records": [
    { "name": "example.com",     "type": "A",     "ttl": 300,  "data": "93.184.216.34" },
    { "name": "www.example.com", "type": "CNAME", "ttl": 300,  "data": "example.com" },
    { "name": "example.com",     "type": "MX",    "ttl": 3600, "data": "mail.example.com", "preference": 10 },
    { "name": "example.com",     "type": "TXT",   "ttl": 3600, "data": "v=spf1 -all" }
  ]
}
```

The document must contain the zone's **complete** record set — there is no record-level
patching. The `version` you return here should match the manifest; Hermes records what the
document says.

Hermes fetches up to `zone_sync.fetch_concurrency` zones in parallel (default 16).

---

## 3. Schemas

### 3.1 Zone document

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `id` | string | yes | Must match the manifest id |
| `apex` | string | yes | Any depth (`example.com`, `zone.example.com`) |
| `version` | u64 | yes | Recorded by Hermes for the next diff |
| `soa` | Soa | yes | A zone cannot exist without an SOA |
| `records` | Record[] | — | Defaults to `[]`. Also accepted as `record` |

### 3.2 Soa

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `mname` | string | yes | Primary nameserver |
| `rname` | string | yes | Responsible-party mailbox |
| `serial` | u32 | yes | **Bump on every change to the zone** |
| `refresh` | i32 | yes | |
| `retry` | i32 | yes | |
| `expire` | i32 | yes | |
| `minimum` | u32 | yes | Negative-cache TTL — keep low (e.g. `60`) |
| `ttl` | u32 | — | TTL of the SOA record itself. Default `3600` |

### 3.3 Record

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `name` | string | yes | Owner name. **Must be the apex or below it** |
| `type` | string | yes | See table below (case-insensitive) |
| `ttl` | u32 | — | Default `3600` |
| `data` | string | Per type | The primary value |
| `preference` | u16 | MX | |
| `priority`, `weight`, `port` | u16 | SRV | |
| `flags` | u8 | — | CAA, default `0` |
| `tag` | string | CAA | |

### 3.4 Record types

| `type` | `data` holds | Extra required | Example |
| --- | --- | --- | --- |
| `A` | IPv4 address | — | `"93.184.216.34"` |
| `AAAA` | IPv6 address | — | `"2606:4700::1111"` |
| `NS` | nameserver name | — | `"ns1.example.com"` |
| `CNAME` | target name | — | `"example.com"` |
| `ALIAS` | target name | — | `"lb.provider.net"` — apex flattening, see §7 |
| `PTR` | target name | — | `"host.example.com"` |
| `MX` | exchange name | `preference` | `"mail.example.com"`, `preference: 10` |
| `TXT` | text string | — | `"v=spf1 -all"` |
| `SRV` | target name | `priority`,`weight`,`port` | `"svc.example.com"` |
| `CAA` | value | `tag` (`flags` optional) | `"letsencrypt.org"`, `tag: "issue"` |

`SOA` is **not** valid as a record `type` — use the zone's `soa` object.
Multiple records with the same `name` + `type` merge into one RRset (they share a TTL; the
first one wins).

---

## 4. Failure isolation

Each zone is validated **in isolation** and only merged once it builds cleanly. A zone that
fails — because the fetch errored or because its data is invalid — is:

- **skipped**, keeping whatever it was serving before (or staying absent if it never applied),
- counted in `/healthz` as `skipped`,
- **retried on the next pass**, because Hermes does not advance its recorded version.

Every other zone in the same pass still applies, and the publish is still a single atomic swap,
so a query never sees a half-applied state.

**This means one tenant's broken zone cannot block another tenant's update** — but that zone
stays stale until fixed, so validate on the backend before committing.

Causes of a per-zone rejection:

| Cause | Example |
| --- | --- |
| Record outside its zone | `name: "www.other.org"` inside zone `example.com` |
| Invalid domain name | empty label, label > 63 bytes, name > 255 bytes |
| Missing `data` | `type: "A"` with no `data` |
| Bad address | `type: "A"`, `data: "not-an-ip"` |
| Missing type-specific field | `MX` without `preference` |
| `SOA` used as a record type | `type: "SOA"` |
| Unknown record type | `type: "WEIRD"` |

### Renaming an apex

Because the `id` is stable, changing a zone's `apex` is expressible: return the same `id` with a
new `apex` and a bumped `version`. Hermes removes the old apex and publishes the new one in the
same swap.

---

## 5. Notify: making propagation immediate

Optional. Without it, changes land within one poll interval (default 15s).

Hermes exposes an admin listener at `zone_sync.admin_listen` (e.g. `0.0.0.0:9098`).
**After committing a change**, POST to each replica:

```http
POST http://hermes:9098/admin/reload
Authorization: Bearer <admin_token>
```

| Status | Meaning |
| --- | --- |
| `202 Accepted` | Reload triggered; Hermes polls immediately |
| `401 Unauthorized` | Missing/incorrect token — **also returned when no `admin_token` is configured** (fails closed) |

Fire-and-forget: it only asks Hermes to poll now. **Never treat a failed ping as a failed
change** — the poll loop picks it up anyway. Best-effort fan-out to replicas (e.g. via the
headless Service endpoints) is fine.

> Bind `admin_listen` to an internal address and restrict it with a NetworkPolicy.
> It is not a public API.

---

## 6. Health

```http
GET http://hermes:9098/healthz
```

```json
{
  "manifest_version": 4711,
  "zones": 42,
  "tracked": 42,
  "skipped": 0,
  "last_success_secs_ago": 3,
  "last_error": null,
  "successful_syncs": 128,
  "failed_syncs": 0
}
```

| Field | Meaning |
| --- | --- |
| `zones` | Zones currently being served |
| `tracked` | Zones known from the manifest and applied successfully |
| `skipped` | Zones that failed on the last pass and are serving stale/absent data |
| `last_error` | Last failure, including `"N zone(s) failed to apply"` |

`skipped > 0` or a growing `last_success_secs_ago` means propagation is degraded even though
DNS is still answering.

---

## 7. ALIAS (apex flattening)

`CNAME` is illegal at a zone apex. To point `example.com` itself at a hostname, send
`type: "ALIAS"`. Hermes resolves the target in the background and synthesizes `A`/`AAAA` at the
apex; all other types at that name (`MX`, `TXT`, `NS`, …) are served normally.

```json
{ "name": "example.com", "type": "ALIAS", "ttl": 60, "data": "lb.provider.net" }
```

Notes for the backend:
- Only consulted for `A`/`AAAA` queries. A static `A` at the same name takes precedence.
- Until the target first resolves, that name answers **SERVFAIL** (never an empty NOERROR).
- Targets resolving only to private/loopback/link-local/ULA space are rejected.

---

## 8. Example C# DTOs

Illustrative shapes for `System.Text.Json` — not an implementation.

```csharp
// GET /zones
public sealed record ZoneManifestDto(
    [property: JsonPropertyName("version")] ulong? Version,
    [property: JsonPropertyName("zones")]   IReadOnlyList<ZoneRefDto> Zones);

public sealed record ZoneRefDto(
    [property: JsonPropertyName("id")]      string Id,
    [property: JsonPropertyName("apex")]    string Apex,
    [property: JsonPropertyName("version")] ulong Version);

// GET /zones/{id}
public sealed record ZoneDocumentDto(
    [property: JsonPropertyName("id")]      string Id,
    [property: JsonPropertyName("apex")]    string Apex,
    [property: JsonPropertyName("version")] ulong Version,
    [property: JsonPropertyName("soa")]     SoaDto Soa,
    [property: JsonPropertyName("records")] IReadOnlyList<RecordDto> Records);

public sealed record SoaDto(
    [property: JsonPropertyName("mname")]   string MName,
    [property: JsonPropertyName("rname")]   string RName,
    [property: JsonPropertyName("serial")]  uint Serial,
    [property: JsonPropertyName("refresh")] int Refresh,
    [property: JsonPropertyName("retry")]   int Retry,
    [property: JsonPropertyName("expire")]  int Expire,
    [property: JsonPropertyName("minimum")] uint Minimum,
    [property: JsonPropertyName("ttl")]     uint Ttl = 3600);

public sealed record RecordDto(
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("type")] string Type,
    [property: JsonPropertyName("ttl")]  uint Ttl,
    [property: JsonPropertyName("data")] string? Data,
    [property: JsonPropertyName("preference")] ushort? Preference = null,
    [property: JsonPropertyName("priority")]   ushort? Priority = null,
    [property: JsonPropertyName("weight")]     ushort? Weight = null,
    [property: JsonPropertyName("port")]       ushort? Port = null,
    [property: JsonPropertyName("flags")]      byte?   Flags = null,
    [property: JsonPropertyName("tag")]        string? Tag = null);
```

Serialize with `JsonIgnoreCondition.WhenWritingNull` so unused type-specific fields are omitted.

---

## 9. Operational guidance

**Bump the zone's `version` and SOA serial on every change.** They can be the same number.
If the version doesn't change, Hermes will not refetch the zone.

**TTL bounds real propagation, not sync speed.** A record with `ttl: 3600` stays cached in
resolvers for an hour no matter how fast you push. Use `60`–`300` for user-editable records.

**Keep `minimum` low.** It controls how long `NXDOMAIN` is cached — a user who creates a record
right after someone looked it up waits that long. `60` is a good default.

**Validate before committing**, at minimum: apex ownership (a TXT challenge or registrar
delegation check — otherwise a user can claim `google.com`), no `CNAME` at an apex, no `CNAME`
alongside other types at the same name, label/name length limits, and a cap on records per zone.

**Multiple replicas** need no coordination: each polls independently and converges within one
poll interval. Eventual consistency is expected and fine for DNS.

**Scaling past ~10k zones.** The manifest is fetched on every poll, so it grows with zone count
(~60 bytes/zone). Two escape hatches when that becomes significant: support `ETag` /
`If-None-Match` on `GET /zones` so unchanged polls cost nothing, or add a batch endpoint so cold
start doesn't need one request per zone. Neither is needed at hundreds of zones.

---

## 10. Hermes-side configuration reference

```toml
[zone_sync]
enabled = true
base_url = "http://backend.internal.svc/internal/dns"
auth_token = "…"                 # sent as: Authorization: Bearer …
poll_interval_secs = 15
request_timeout_ms = 5000
fetch_concurrency = 16           # zones fetched in parallel when the manifest shows changes
snapshot_path = "data/zones-snapshot.json"   # cold-start fallback
admin_listen = "0.0.0.0:9098"    # POST /admin/reload, GET /healthz
admin_token = "…"                # required for /admin/reload
```

With `enabled = false`, zones come only from the local `zones.toml` and none of this applies.

**Startup order:** local `zones.toml` seeds the store → the first successful manifest pull
replaces it entirely → if the control plane is unreachable, the on-disk snapshot is used (with
its per-zone versions, so the next poll only refetches what changed) → if that is missing too,
the file-loaded zones keep serving.
