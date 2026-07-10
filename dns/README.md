# dns

The authoritative nameserver, written in Rust. It answers DNS queries over UDP and TCP for the zones the control plane owns, and it's the part of Hermes that faces the open internet.

> This is a learning project. It hasn't been hardened or audited and isn't built to sit on a real port 53. Don't run it in production.

## What it does

It loads zones from two places and serves whichever answer matches:

- `config/zones.toml`, for static zones you commit by hand.
- The control plane, over the zone-sync protocol, for zones people edit in the dashboard.

Sync is pull-based. Hermes polls the backend's manifest, notices which zones changed by their version number, and refetches only those. It keeps a local snapshot on disk (`data/zones-snapshot.json`) so it can still boot and answer if the backend is unreachable. The backend can also `POST /admin/reload` to skip the poll delay, but that's an optimization, not the correctness path. The full contract is in `docs/zone-sync-contract.md`.

On a public address the interesting bit is response rate limiting (RRL). An attacker can spoof a victim's source address, send small queries, and make the server spray big answers at them. RRL caps responses per client prefix and per answer class so that stops working. It's UDP-only, since TCP can't be spoofed.

Telemetry goes out over OTLP to an LGTM stack (Loki, Grafana, Tempo, Mimir). Logs also print to the console and rotate to daily files. If you don't set an OTLP endpoint, export just turns off and local logging keeps working.

## Layout

It's a Cargo workspace. Each crate does one thing:

- `hermes` is the binary. It reads config, wires everything up, and owns startup and shutdown.
- `dns-server` has the UDP and TCP listeners, the RRL limiter, and the connection handling.
- `dns-types` is the wire codec and the zone lookup tree. Parsing and building DNS messages, and answering a query against a zone, live here.
- `zone-sync` is the client that pulls from the control plane, plus the `/admin/reload` endpoint.
- `config` reads the TOML files into typed config.
- `telemetry` sets up tracing, the OTLP metrics and traces, and a panic/crash handler.
- `alias-resolver` resolves `ALIAS` records by asking an upstream resolver, since ALIAS points at a name rather than an address.

`tools/dns-client` is a small CLI for firing queries at a running server while you poke at it.

## Running it

You need a recent Rust toolchain (the workspace is on the 2024 edition, `rust-version = 1.93`).

```
cargo run -p hermes
```

Config lives in `config/`. `server.toml` is the process (bind address, transports, worker counts, telemetry, rate limiting); `zones.toml` is static DNS data.

Port 53 is the real DNS port but needs elevated privileges. For local work, set `port = 5353` in `config/server.toml` and query that instead:

```
dig @127.0.0.1 -p 5353 example.com
```

Tests:

```
cargo test
```

## How it connects

The browser never talks to this server, and this server never talks to a database. It answers DNS, and it reads zone data from the control plane (`../management-backend`). Everything about who owns what and who's allowed to change it stays on the backend.

## Status

Rough in spots, and that's fine. It exists to be read and experimented with. Expect placeholder config, missing edges, and choices made to learn how DNS actually works rather than to survive real traffic.
