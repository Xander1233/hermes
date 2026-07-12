export const RECORD_TYPES = [
  "A",
  "AAAA",
  "CNAME",
  "ALIAS",
  "NS",
  "MX",
  "TXT",
  "SRV",
  "CAA",
  "PTR",
] as const;

export type RecordType = (typeof RECORD_TYPES)[number];
export type ZoneStatus = "pending" | "active" | "error";

export interface Zone {
  id: string;
  apex: string;
  status: ZoneStatus;
  statusMessage?: string | null;
  version: number;
  nameservers: string[];
  recordCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface ZoneDetail extends Zone {
  soa: Soa;
}

export interface Soa {
  mname: string;
  rname: string;
  serial: number;
  refresh: number;
  retry: number;
  expire: number;
  minimum: number;
  ttl: number;
}

export interface CreateZoneRequest {
  apex: string;
}

export interface DelegationCheck {
  observedNameservers: string[];
  expectedNameservers: string[];
  delegated: boolean;
  checkedAt: string;
}

export interface DnsRecord {
  id: string;
  zoneId: string;
  name: string;
  type: RecordType;
  ttl: number;
  data: string | null;
  preference?: number | null;
  priority?: number | null;
  weight?: number | null;
  port?: number | null;
  flags?: number | null;
  tag?: string | null;
  notes?: string | null;
  createdAt: string;
  updatedAt: string;
}

export type CreateRecordRequest = Omit<
  DnsRecord,
  "id" | "zoneId" | "createdAt" | "updatedAt"
>;

export type UpdateRecordRequest = CreateRecordRequest;

export interface UserProfile {
  id: string;
  email: string;
  displayName: string;
  company?: string | null;
  timezone: string;
  notificationEmail?: string | null;
  notifyOnZoneErrors: boolean;
  createdAt: string;
}

export type UpdateProfileRequest = Pick<
  UserProfile,
  | "displayName"
  | "company"
  | "timezone"
  | "notificationEmail"
  | "notifyOnZoneErrors"
>;

export interface ApiProblem {
  title: string;
  status: number;
  detail?: string;
  errors?: Record<string, string[]>;
}

export class ApiError extends Error {
  readonly status: number;
  readonly fieldErrors: Record<string, string[]>;

  constructor(problem: ApiProblem) {
    super(problem.detail ?? problem.title);

    this.name = "ApiError";
    this.status = problem.status;
    this.fieldErrors = problem.errors ?? {};
  }
}

export interface HermesApi {
  listAuthProviders(): Promise<string[]>;

  getProfile(): Promise<UserProfile>;
  updateProfile(body: UpdateProfileRequest): Promise<UserProfile>;

  listZones(): Promise<Zone[]>;
  createZone(body: CreateZoneRequest): Promise<ZoneDetail>;
  getZone(zoneId: string): Promise<ZoneDetail>;
  deleteZone(zoneId: string): Promise<void>;
  updateSoa(zoneId: string, body: Soa): Promise<ZoneDetail>;
  checkDelegation(zoneId: string): Promise<DelegationCheck>;

  listRecords(zoneId: string): Promise<DnsRecord[]>;
  createRecord(zoneId: string, body: CreateRecordRequest): Promise<DnsRecord>;
  updateRecord(
    zoneId: string,
    recordId: string,
    body: UpdateRecordRequest
  ): Promise<DnsRecord>;
  deleteRecord(zoneId: string, recordId: string): Promise<void>;
}
