import {
  Outlet,
  redirect,
  useLoaderData,
  type ShouldRevalidateFunctionArgs,
} from "react-router";

import { api, type UserProfile } from "~/lib/api";
import { loadSession } from "~/lib/auth/session";
import { AppShell } from "~/components/app-shell";
import { Skeleton } from "~/components/ui/skeleton";

export interface AppContext {
  profile: UserProfile;
}

const PROFILE_PATH = "/profile";

export async function clientLoader() {
  if (!loadSession()) {
    throw redirect("/login");
  }
  
  const profile = await api.getProfile();
  return { profile };
}

/**
 * Fetches current session only when visiting a profile settings route
 * 
 * This will ensure that the session is refreshed, when necessary, before any profile settings action is performed.
 * Without this check, we fetch the current session on every route change, which is unnecessary and can lead to performance issues on front- and backend.
 */
export function shouldRevalidate({ formAction }: ShouldRevalidateFunctionArgs) {
  return formAction?.startsWith(PROFILE_PATH) ?? false;
}

export function HydrateFallback() {
  return (
    <div className="flex min-h-svh flex-col">
      <div className="h-14 border-b" />
      <div className="mx-auto w-full max-w-6xl flex-1 space-y-4 p-6">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-64 w-full" />
      </div>
    </div>
  );
}

export default function AppLayout() {
  const { profile } = useLoaderData<typeof clientLoader>();

  return (
    <AppShell profile={profile}>
      <Outlet context={{ profile } satisfies AppContext} />
    </AppShell>
  );
}
