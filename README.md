# Hermes

Hermes is an authoritative DNS platform, built from scratch across three languages to see how a real DNS service fits together end to end: the nameserver that answers queries, the control plane that holds the data, and the dashboard someone actually clicks on.

> Heads up: this is a learning project. It hasn't been hardened, audited, or load-tested, and it isn't meant to run anything you'd miss. Please don't use it in production.

## What's in here

One repo, three projects. Each keeps its own toolchain and its own README with the details.

- `dns/` is the authoritative nameserver, written in Rust. It answers over UDP and TCP, rate-limits responses, and pulls its zone data from the control plane. It's a Cargo workspace split into small crates (`dns-types`, `dns-server`, `zone-sync`, `config`, `telemetry`, `alias-resolver`) plus the `hermes` binary that wires them together.
- `management-backend/` is the control plane, in C# on .NET 10. A versioned REST API over a single-table DynamoDB store. It owns zones, records, and SOA data, signs users in through Amazon Cognito, and publishes the snapshot the nameserver reads.
- `frontend/` is the dashboard, a server-rendered React Router 7 app. It's where you create zones, edit records, and manage your account. It only ever talks to the control plane.

## How it fits together

The control plane is the source of truth. You change a zone in the dashboard, it calls the control plane's REST API, and the control plane writes to DynamoDB and bumps a version number. The nameserver polls the control plane, sees the version move, and refetches the snapshot it serves from. The browser never talks to the nameserver, and the nameserver never touches a database.

Sign-in goes through Cognito. The dashboard authenticates the user (password or social login) and passes the id token to the control plane, which checks it on every request.

## Running it

Each project runs on its own.

dns

```
cd dns
cargo run -p hermes
```

Config lives in `dns/config`. More in `dns/README.md`.

management-backend

```
cd management-backend
dotnet run --project src/Hermes.Mgmt.Api
```

Wants DynamoDB and Cognito settings. More in `management-backend/README.md`.

frontend

```
cd frontend
npm install
npm run dev
```

Set `VITE_USE_MOCK_API=true` in `.env.local` and the whole UI runs against an in-memory mock, so you need neither the backend nor AWS. More in `frontend/README.md`.

## Built with

- dns: Rust (2024 edition), Tokio, a hand-written DNS wire codec, OpenTelemetry over OTLP.
- management-backend: C# and ASP.NET Core on .NET 10, DynamoDB, Cognito, API versioning.
- frontend: React Router 7 (SSR), Vite, Tailwind, Base UI, Cognito.

## Status

Unfinished in places, and that's the point: it's here to be read and taken apart, not shipped. Expect placeholder config, gaps, and choices made to learn something rather than to hold up in every case.

## License

MIT.
