import { config } from "~/lib/config";
import { refreshSession } from "~/lib/auth/cognito.client";
import { isExpiringSoon, loadSession, type Session } from "~/lib/auth/session";
import {
  ApiError,
  type ApiProblem,
  type CreateRecordRequest,
  type CreateZoneRequest,
  type DelegationCheck,
  type DnsRecord,
  type HermesApi,
  type Soa,
  type UpdateProfileRequest,
  type UpdateRecordRequest,
  type UserProfile,
  type Zone,
  type ZoneDetail,
} from "~/lib/api/types";

let refreshInFlight: Promise<Session | null> | null = null;

async function currentSession(): Promise<Session | null> {
  const session = loadSession();
  if (!session || !isExpiringSoon(session)) return session;

  refreshInFlight ??= refreshSession().finally(() => {
    refreshInFlight = null
  });

  return refreshInFlight;
}

async function request<T>(
  path: string,
  init: RequestInit & { method?: string } = {}
): Promise<T> {
  const session = await currentSession();
  const response = await fetch(`${config.apiBaseUrl}${path}`, {
    ...init,
    headers: {
      accept: "application/json",
      ...(init.body ? { "content-type": "application/json" } : {}),
      ...(session ? { authorization: `Bearer ${session.idToken}` } : {}),
      ...init.headers,
    },
  });

  if (response.status === 204) return undefined as T;

  const text = await response.text();
  const body = text ? JSON.parse(text) : null;

  if (!response.ok) {
    throw new ApiError(
      (body as ApiProblem | null) ?? {
        title: response.statusText || "Request failed",
        status: response.status,
      }
    );
  }

  return body as T;
}

export const httpApi: HermesApi = {
  listAuthProviders: () => request<string[]>("/auth/providers"),

  getProfile: () => request<UserProfile>("/me"),
  updateProfile: (body: UpdateProfileRequest) => request<UserProfile>("/me", {
      method: "PATCH",
      body: JSON.stringify(body),
    }),

  listZones: () => request<Zone[]>("/zones"),
  createZone: (body: CreateZoneRequest) => request<ZoneDetail>("/zones", {
      method: "POST",
      body: JSON.stringify(body),
    }),
  getZone: (zoneId: string) => request<ZoneDetail>(`/zones/${encodeURIComponent(zoneId)}`),
  deleteZone: (zoneId: string) => request<void>(`/zones/${encodeURIComponent(zoneId)}`, { method: "DELETE" }),
  updateSoa: (zoneId: string, body: Soa) => request<ZoneDetail>(`/zones/${encodeURIComponent(zoneId)}/soa`, {
      method: "PUT",
      body: JSON.stringify(body),
    }),
  checkDelegation: (zoneId: string) => request<DelegationCheck>(`/zones/${encodeURIComponent(zoneId)}/delegation`),

  listRecords: (zoneId: string) => request<DnsRecord[]>(`/zones/${encodeURIComponent(zoneId)}/records`),
  createRecord: (zoneId: string, body: CreateRecordRequest) => request<DnsRecord>(`/zones/${encodeURIComponent(zoneId)}/records`, {
      method: "POST",
      body: JSON.stringify(body),
    }),
  updateRecord: (zoneId: string, recordId: string, body: UpdateRecordRequest) => request<DnsRecord>(
      `/zones/${encodeURIComponent(zoneId)}/records/${encodeURIComponent(recordId)}`,
      { method: "PUT", body: JSON.stringify(body) }
    ),
  deleteRecord: (zoneId: string, recordId: string) => request<void>(
      `/zones/${encodeURIComponent(zoneId)}/records/${encodeURIComponent(recordId)}`,
      { method: "DELETE" }
    ),
}
