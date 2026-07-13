import {
  ApiError,
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
import { toFqdn, validateRecord } from "~/lib/dns";

const NAMESERVERS = ["ns1.hermes-dns.net", "ns2.hermes-dns.net"];

const STORAGE_KEY = "hermes.mock-db.v1";
const LATENCY_MS = 220;

interface Db {
  profile: UserProfile;
  zones: ZoneDetail[];
  records: DnsRecord[];
  delegated: string[];
}

function now(): string {
  return new Date().toISOString();
}

function id(prefix: string): string {
  return `${prefix}_${Math.random().toString(36).slice(2, 10)}`;
}

function defaultSoa(apex: string): Soa {
  return {
    mname: NAMESERVERS[0],
    rname: `hostmaster.${apex}`,
    // YYYYMMDDnn, where nn is a sequence for multi updates on the same day. Always starts at 00 and counts up to 99 (=> 99 updates per day)
    // Prod uses the timestamp
    serial: Number(new Date().toISOString().slice(0, 10).replace(/-/g, "") + "01"),
    refresh: 3600,
    retry: 600,
    expire: 604800,
    minimum: 60,
    ttl: 3600,
  };
}

function seed(): Db {
  const created = now();
  const apex = "example.com";
  const zoneId = "zone_seed01";

  return {
    profile: {
      id: "mock-user-0001",
      email: "dev@hermes.local",
      displayName: "Dev User",
      company: "Hermes",
      timezone: "Europe/Zurich",
      notificationEmail: null,
      notifyOnZoneErrors: true,
      createdAt: created,
    },
    zones: [
      {
        id: zoneId,
        apex,
        status: "active",
        version: 7,
        nameservers: NAMESERVERS,
        recordCount: 5,
        createdAt: created,
        updatedAt: created,
        soa: defaultSoa(apex),
      },
      {
        id: "zone_seed02",
        apex: "staging.example.org",
        status: "pending",
        version: 1,
        nameservers: NAMESERVERS,
        recordCount: 1,
        createdAt: created,
        updatedAt: created,
        soa: defaultSoa("staging.example.org"),
      },
    ],
    records: [
      record(zoneId, {
        name: apex,
        type: "ALIAS",
        ttl: 300,
        data: "origin.hermes-edge.net",
        notes: "Apex points at the edge pool — flattened, see the CNAME note.",
      }),
      record(zoneId, {
        name: `www.${apex}`,
        type: "CNAME",
        ttl: 300,
        data: apex,
        notes: null,
      }),
      record(zoneId, {
        name: `api.${apex}`,
        type: "A",
        ttl: 60,
        data: "192.0.2.10",
        notes: "Blue/green: flipped by the deploy pipeline.",
      }),
      record(zoneId, {
        name: apex,
        type: "MX",
        ttl: 3600,
        data: "mail.example.com",
        preference: 10,
        notes: null,
      }),
      record(zoneId, {
        name: apex,
        type: "TXT",
        ttl: 3600,
        data: "v=spf1 include:_spf.example.com ~all",
        notes: "SPF — do not remove, breaks outbound mail.",
      }),
      record("zone_seed02", {
        name: "staging.example.org",
        type: "A",
        ttl: 300,
        data: "198.51.100.7",
        notes: null,
      }),
    ],
    delegated: [zoneId],
  };
}

function record(
  zoneId: string,
  fields: Omit<CreateRecordRequest, "data"> & { data: string | null }
): DnsRecord {
  const timestamp = now();

  return {
    id: id("rec"),
    zoneId,
    preference: null,
    priority: null,
    weight: null,
    port: null,
    flags: null,
    tag: null,
    notes: null,
    ...fields,
    createdAt: timestamp,
    updatedAt: timestamp,
  };
}

function read(): Db {
  if (typeof window === "undefined") return seed();

  const raw = window.localStorage.getItem(STORAGE_KEY);
  if (!raw) {
    const fresh = seed();
    write(fresh);

    return fresh;
  }

  try {
    return JSON.parse(raw) as Db;
  } catch {
    const fresh = seed();
    write(fresh);

    return fresh;
  }
}

function write(db: Db): void {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(db));
}

export function resetMockDatabase(): void {
  if (typeof window === "undefined") return;
  window.localStorage.removeItem(STORAGE_KEY);
}

async function delay<T>(value: T): Promise<T> {
  await new Promise((resolve) => setTimeout(resolve, LATENCY_MS));
  return value;
}

function fail(
  status: number,
  title: string,
  detail?: string,
  errors?: Record<string, string[]>
): never {
  throw new ApiError({ status, title, detail, errors });
}

function findZone(db: Db, zoneId: string): ZoneDetail {
  const zone = db.zones.find((z) => z.id === zoneId);
  if (!zone) fail(404, "Zone not found", `No zone with id ${zoneId}.`);

  return zone;
}

function touchZone(db: Db, zone: ZoneDetail): void {
  zone.version += 1;
  zone.soa = { ...zone.soa, serial: zone.soa.serial + 1 };
  zone.updatedAt = now();
  zone.recordCount = db.records.filter((r) => r.zoneId === zone.id).length;
}

function normalize(
  body: CreateRecordRequest,
  apex: string
): CreateRecordRequest {
  const name = toFqdn(body.name, apex);
  const type = body.type === "CNAME" && name === apex ? "ALIAS" : body.type;

  return { ...body, name, type };
}

function assertNoConflict(
  db: Db,
  zoneId: string,
  candidate: CreateRecordRequest,
  ignoreId?: string
): void {
  const siblings = db.records.filter(
    (r) => r.zoneId === zoneId && r.name === candidate.name && r.id !== ignoreId
  );
  const isAlias = candidate.type === "CNAME" || candidate.type === "ALIAS";

  const conflicting = siblings.some(
    (r) => isAlias || r.type === "CNAME" || r.type === "ALIAS"
  );
  if (conflicting) {
    fail(409, "Conflicting record", undefined, {
      name: [
        `${candidate.name} already has records of another type; a CNAME/ALIAS must be the only record at its name.`,
      ],
    });
  }

  if (
    siblings.some((r) => r.type === candidate.type && r.data === candidate.data)
  ) {
    fail(409, "Duplicate record", undefined, {
      data: ["An identical record already exists."],
    });
  }
}

function validateOrFail(body: CreateRecordRequest, apex: string): void {
  const errors = validateRecord(body, apex);

  if (Object.keys(errors).length > 0) {
    fail(
      422,
      "Validation failed",
      undefined,
      Object.fromEntries(
        Object.entries(errors).map(([key, value]) => [key, [value]])
      )
    );
  }
}

export const mockApi: HermesApi = {
  async listAuthProviders() {
    return delay([ "Google", "SignInWithApple" ])
  },

  async getProfile() {
    return delay(read().profile);
  },

  async updateProfile(body: UpdateProfileRequest) {
    const db = read();
    db.profile = { ...db.profile, ...body };

    write(db);

    return delay(db.profile);
  },

  async listZones() {
    const db = read();
    const zones: Zone[] = db.zones.map(({ soa: _soa, ...zone }) => ({
      ...zone,
      recordCount: db.records.filter((r) => r.zoneId === zone.id).length,
    }));

    return delay(zones);
  },

  async createZone(body: CreateZoneRequest) {
    const db = read();

    const apex = body.apex.trim().toLowerCase().replace(/\.$/, "");
    if (!/^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(apex)) {
      fail(422, "Validation failed", undefined, {
        apex: ["Not a valid domain name."],
      });
    }

    if (db.zones.some((z) => z.apex === apex)) {
      fail(409, "Zone already exists", undefined, {
        apex: ["You already manage this zone."],
      });
    }

    const timestamp = now();
    const zone: ZoneDetail = {
      id: id("zone"),
      apex,
      status: "pending",
      version: 1,
      nameservers: NAMESERVERS,
      recordCount: 0,
      createdAt: timestamp,
      updatedAt: timestamp,
      soa: defaultSoa(apex),
    };

    db.zones.push(zone);
    write(db);
    
    return delay(zone);
  },

  async getZone(zoneId: string) {
    const db = read();
    const zone = findZone(db, zoneId);
    zone.recordCount = db.records.filter((r) => r.zoneId === zoneId).length;

    return delay(zone);
  },

  async deleteZone(zoneId: string) {
    const db = read();

    findZone(db, zoneId);
    db.zones = db.zones.filter((z) => z.id !== zoneId);
    db.records = db.records.filter((r) => r.zoneId !== zoneId);
    db.delegated = db.delegated.filter((zid) => zid !== zoneId);

    write(db);

    return delay(undefined);
  },

  async updateSoa(zoneId: string, body: Soa) {
    const db = read();
    const zone = findZone(db, zoneId);
    const errors: Record<string, string[]> = {};

    if (!body.mname.trim()) errors.mname = ["Primary nameserver is required."];
    if (!body.rname.trim()) errors.rname = ["Responsible party is required."];
    if (body.refresh <= 0) errors.refresh = ["Must be greater than zero."];
    if (body.retry <= 0) errors.retry = ["Must be greater than zero."];
    if (body.expire <= body.refresh) errors.expire = ["Expire must be greater than refresh."];
    if (body.minimum < 0) errors.minimum = ["Must not be negative."];
    if (Object.keys(errors).length > 0) fail(422, "Validation failed", undefined, errors);

    zone.soa = { ...body };
    zone.version += 1;
    zone.updatedAt = now();

    write(db);

    return delay(zone);
  },

  async checkDelegation(zoneId: string) {
    const db = read();
    const zone = findZone(db, zoneId);
    const delegated = db.delegated.includes(zoneId);

    if (delegated && zone.status === "pending") {
      zone.status = "active";
      write(db);
    }

    const check: DelegationCheck = {
      observedNameservers: delegated ? zone.nameservers : [],
      expectedNameservers: zone.nameservers,
      delegated,
      checkedAt: now(),
    };

    return delay(check);
  },

  async listRecords(zoneId: string) {
    const db = read();
    findZone(db, zoneId);

    const records = db.records
      .filter((r) => r.zoneId === zoneId)
      .sort(
        (a, b) => a.name.localeCompare(b.name) || a.type.localeCompare(b.type)
      );

    return delay(records);
  },

  async createRecord(zoneId: string, body: CreateRecordRequest) {
    const db = read();
    const zone = findZone(db, zoneId);
    const normalized = normalize(body, zone.apex);

    validateOrFail(normalized, zone.apex);
    assertNoConflict(db, zoneId, normalized);

    const created = record(zoneId, normalized);
    db.records.push(created);

    touchZone(db, zone);
    write(db);

    return delay(created);
  },

  async updateRecord(
    zoneId: string,
    recordId: string,
    body: UpdateRecordRequest
  ) {
    const db = read();
    const zone = findZone(db, zoneId);
    const existing = db.records.find(
      (r) => r.id === recordId && r.zoneId === zoneId
    );

    if (!existing) fail(404, "Record not found");

    const normalized = normalize(body, zone.apex);
    validateOrFail(normalized, zone.apex);
    assertNoConflict(db, zoneId, normalized, recordId);

    Object.assign(existing, normalized, { updatedAt: now() });
    
    touchZone(db, zone);
    write(db);

    return delay(existing);
  },

  async deleteRecord(zoneId: string, recordId: string) {
    const db = read();
    const zone = findZone(db, zoneId);
    const before = db.records.length;
    db.records = db.records.filter(
      (r) => !(r.id === recordId && r.zoneId === zoneId)
    );

    if (db.records.length === before) fail(404, "Record not found");

    touchZone(db, zone);
    write(db);

    return delay(undefined);
  },
}

export function markDelegated(zoneId: string): void {
  const db = read();

  if (!db.delegated.includes(zoneId)) {
    db.delegated.push(zoneId);
    write(db);
  }
}
