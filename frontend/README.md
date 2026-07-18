# frontend

The Hermes dashboard. A server-rendered React Router 7 app where you sign in, create zones, edit records, fix up the SOA, check delegation, and manage your account. It talks to the control plane and nothing else.

> This is a portfolio and learning project. It isn't meant to run a real product. Don't use it in production.

## The one thing worth knowing first

You can run the entire UI without a backend or an AWS account. Set `VITE_USE_MOCK_API=true` and every API call is served by an in-memory mock (`app/lib/api/mock.ts`) backed by `localStorage`, seeded with a couple of zones. Sign-in falls back to a development user. This is the fastest way to see the thing, and it's how most of it was built.

Flip `VITE_USE_MOCK_API=false` when you have the control plane running and want the real thing.

## Running it

```
npm install
npm run dev
```

The dev server comes up on http://localhost:5173.

Copy `.env.example` to `.env` and fill it in. For mock mode you only need `VITE_USE_MOCK_API=true`. For the real backend you also need `VITE_API_BASE_URL` (point it at the control plane, e.g. `http://localhost:5080/api/v1`) and the `VITE_COGNITO_*` values for your user pool. Everything prefixed `VITE_` is baked into the client bundle at build time, so never put a secret there.

Other scripts:

```
npm run build      # production build (this is the real SSR check)
npm start          # serve the build
npm run typecheck  # react-router typegen + tsc
npm run format     # prettier
```

## How auth works

Sign-in runs through Amazon Cognito. Password sign-in uses SRP directly in the browser (`amazon-cognito-identity-js`), so it never leaves for a hosted page. Social sign-in (Google and friends) uses the OAuth redirect, since federated providers only work that way. The login page renders whichever social buttons the backend reports as enabled, and degrades to just the password form if that call fails.

The session lives in `localStorage`. Tokens refresh silently at a single choke point in the API layer before they expire, so you don't get bounced to the login screen mid-edit. There's no account-linking feature: you sign in with one method per account.

## Layout

- `app/routes/` is the pages. Zones list, a zone with its records/SOA/nameservers sections, the login flow, the OAuth callback, and the profile area.
- `app/lib/api/` is the API client: a typed interface with an HTTP implementation and the mock, chosen by config.
- `app/lib/auth/` is the Cognito and session code.
- `app/components/` is the UI, built on Base UI primitives and Tailwind.

Data loading and mutations use React Router's `clientLoader` and `clientAction` with fetchers, so the routes stay declarative and the app revalidates itself after a change. `PRODUCT.md` has the design thinking behind it.

## Built with

React 19, React Router 7 (SSR), Vite, Tailwind v4, Base UI, sonner for toasts, and `amazon-cognito-identity-js`.

## How it connects

This is the only piece a person clicks on. It sends requests to the control plane (`../management-backend`) with the Cognito id token attached, and the backend authorizes every one. It never touches the nameserver or a database directly.

## Status

Unfinished in places, on purpose. Run it in mock mode and click around; that's what it's for.
