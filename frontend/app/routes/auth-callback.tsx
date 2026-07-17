import { Link, redirect, useLoaderData } from "react-router";
import { RiLoader4Line } from "@remixicon/react";

import { adoptOauthTokens, exchangeCode } from "~/lib/auth/cognito.client";
import { Alert, AlertDescription, AlertTitle } from "~/components/ui/alert";
import { Button } from "~/components/ui/button";

import type { Route } from "./+types/auth-callback";

export function meta() {
  return [{ title: "Signing in — Hermes DNS" }];
}

/**
 * Cognito redirects here with `?code=&state=` after a social sign-in.
 *
 * The exchange has to happen client-side because the PKCE verifier lives in `sessionStorage`.
 */
export async function clientLoader({ request }: Route.ClientLoaderArgs) {
  const url = new URL(request.url);

  const oauthError = url.searchParams.get("error");
  if (oauthError) {
    return { error: url.searchParams.get("error_description") ?? oauthError };
  }

  const code = url.searchParams.get("code");
  const state = url.searchParams.get("state");
  if (!code || !state) {
    return { error: "The sign-in response was missing its authorization code." };
  }

  try {
    adoptOauthTokens(await exchangeCode(code, state));
  } catch (cause) {
    return { error: cause instanceof Error ? cause.message : "Sign-in failed." };
  }

  throw redirect("/");
}

export function HydrateFallback() {
  return <Pending />;
}

function Pending() {
  return (
    <div className="flex min-h-svh items-center justify-center gap-2 text-sm text-muted-foreground">
      <RiLoader4Line className="size-4 animate-spin" />
      Completing sign-in…
    </div>
  );
}

export default function AuthCallback() {
  const { error } = useLoaderData<typeof clientLoader>();

  if (!error) return <Pending />;

  return (
    <div className="flex min-h-svh items-center justify-center p-6">
      <Alert variant="destructive" className="max-w-md">
        <AlertTitle>Sign-in failed</AlertTitle>
        <AlertDescription className="space-y-3">
          <p>{error}</p>
          <Button variant="outline" size="sm" render={<Link to="/login" />}>
            Back to sign in
          </Button>
        </AlertDescription>
      </Alert>
    </div>
  );
}
