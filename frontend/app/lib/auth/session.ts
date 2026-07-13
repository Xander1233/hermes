const STORAGE_KEY = "hermes.session";
const REFRESH_SKEW_MS = 2 * 60 * 1000

export type SignInMethod = "srp" | "oauth";

export interface Session {
  idToken: string;
  accessToken: string;
  refreshToken?: string;
  expiresAt: number;
  sub: string;
  email: string;
  method: SignInMethod;
}

export function loadSession(): Session | null {
  if (typeof window === "undefined") return null;

  const raw = window.localStorage.getItem(STORAGE_KEY);
  if (!raw) return null;

  try {
    const session = JSON.parse(raw) as Session;

    if (session.expiresAt <= Date.now() && !session.refreshToken) {
      clearSession();
      return null;
    }

    return session;
  } catch {
    clearSession();
    return null;
  }
}

export function isExpiringSoon(session: Session, skewMs = REFRESH_SKEW_MS): boolean {
  return session.expiresAt - skewMs <= Date.now();
}

export function saveSession(session: Session): void {
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(session));
}

export function clearSession(): void {
  if (typeof window === "undefined") return;
  window.localStorage.removeItem(STORAGE_KEY);
}

export function requireSession(): Session {
  const session = loadSession();
  if (!session) throw new Response("Unauthorized", { status: 401 });

  return session;
}

/** Decoded JWT without verifying authenticity. Backend will verify authenticity using cognitos current keys */
export function decodeJwtPayload(token: string): Record<string, unknown> {
  const [, payload] = token.split(".");
  if (!payload) return {};

  const json = atob(payload.replace(/-/g, "+").replace(/_/g, "/"));

  try {
    return JSON.parse(json) as Record<string, unknown>;
  } catch {
    return {};
  }
}
