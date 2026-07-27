# management-backend

The control plane, in C# on ASP.NET Core (.NET 10). It's the source of truth for Hermes: zones, records, SOA data, and who owns them. The dashboard reads and writes through it, and the nameservers pull their zone data from it.

> This is a learning project. It hasn't been hardened or audited. Don't use it in production.

## What it does

It answers two kinds of caller.

The dashboard hits a versioned REST API under `/api/v1`: list and create zones, edit records, update the SOA, run a delegation check against public resolvers, and manage the signed-in user's profile. Every one of those routes needs a valid Cognito id token, which the API validates on each request. The one open route is `GET /api/v1/auth/providers`, so the login page can find out which social providers are enabled before anyone has signed in.

The DNS fleet pulls zone data over the sync contract: a manifest of zones with per-zone versions, and one endpoint per full zone. When a zone changes the backend bumps its version and (optionally) pokes each replica's `/admin/reload` so it refetches without waiting for the next poll. The contract is written up in `../dns/docs/zone-sync-contract.md`.

Storage is a single DynamoDB table (`hermes-dns` by default). One table, keyed so zones, records, and profiles share it.

## Layout

Four projects, layered so the web framework stays out of the business logic:

- `Hermes.Mgmt.Api` is the controllers, auth, request pipeline, and mapping.
- `Hermes.Mgmt.Service` is the business logic: validation, zone and record rules, delegation checks, the Cognito calls.
- `Hermes.Mgmt.Repository` is the DynamoDB access.
- `Hermes.Mgmt.Common` is the contracts and domain types shared across the layers.

Tests live under `tests/`, one project per layer plus shared support.

## Running it

You need the .NET 10 SDK.

```
cd management-backend
dotnet run --project src/Hermes.Mgmt.Api
```

It needs two things to be useful:

- DynamoDB. Point `DynamoDb:ServiceUrl` at a local DynamoDB (or DynamoDB Local in Docker) for development, or leave it null to use real AWS in `DynamoDb:Region`. Set `CreateTableIfMissing` to true if you want it to make the table for you.
- Cognito. Fill in `Cognito:Region`, `UserPoolId`, and `ClientId` for the same user pool the dashboard uses, so the tokens it issues validate here.

Config is in `src/Hermes.Mgmt.Api/appsettings.json` (and `appsettings.Development.json`). Besides Cognito and DynamoDb, the parts worth knowing:

- `ZoneDefaults` sets the nameservers, SOA timers, and per-user limits handed to new zones.
- `HermesFleet` lists the nameserver admin endpoints to notify on a change, with a shared token.
- `Delegation` is the resolvers used to check whether a zone's registrar has actually been pointed at Hermes.
- `Cache` is off by default; it can front reads with Redis.

The dashboard's `VITE_API_BASE_URL` has to match wherever this ends up listening (5080 in the sample env). Check `Properties/launchSettings.json` for the port.

Tests:

```
dotnet test
```

## How it connects

This sits in the middle. The dashboard (`../frontend`) talks to it over REST; it stores everything in DynamoDB and hands zone data to the nameservers (`../dns`) over the sync contract. It's the only piece that touches the database, and the only one that decides who is allowed to do what.

## Status

Incomplete in places, which is the point of the exercise. Expect placeholder tokens in the sample config, gaps, and decisions made to learn the shape of the problem rather than to run for real.
