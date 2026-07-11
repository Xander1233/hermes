import type { DnsRecord, RecordType } from "~/lib/api/types";

export interface RecordTypeInfo {
  type: RecordType;
  label: string;
  description: string;
  dataLabel: string;
  dataPlaceholder: string;
  extraFields: ExtraField[];
  selectable: boolean;
}

export type ExtraField = "preference" | "priority" | "weight" | "port" | "flags" | "tag";

export const RECORD_TYPE_INFO: Record<RecordType, RecordTypeInfo> = {
  A: {
    type: "A",
    label: "A",
    description: "Points a name at an IPv4 address.",
    dataLabel: "IPv4 address",
    dataPlaceholder: "192.0.2.1",
    extraFields: [],
    selectable: true,
  },
  AAAA: {
    type: "AAAA",
    label: "AAAA",
    description: "Points a name at an IPv6 address.",
    dataLabel: "IPv6 address",
    dataPlaceholder: "2001:db8::1",
    extraFields: [],
    selectable: true,
  },
  CNAME: {
    type: "CNAME",
    label: "CNAME",
    description: "Aliases this name to another name.",
    dataLabel: "Target",
    dataPlaceholder: "target.example.com",
    extraFields: [],
    selectable: true,
  },
  ALIAS: {
    type: "ALIAS",
    label: "CNAME",
    description:
      "A CNAME at the zone apex. Served as flattened A/AAAA records, because a real CNAME is not allowed alongside the SOA and NS records at the apex.",
    dataLabel: "Target",
    dataPlaceholder: "target.example.com",
    extraFields: [],
    selectable: false,
  },
  NS: {
    type: "NS",
    label: "NS",
    description: "Delegates a subdomain to other nameservers.",
    dataLabel: "Nameserver",
    dataPlaceholder: "ns1.example.net",
    extraFields: [],
    selectable: true,
  },
  MX: {
    type: "MX",
    label: "MX",
    description: "Routes mail for this name to a mail server.",
    dataLabel: "Mail server",
    dataPlaceholder: "mail.example.com",
    extraFields: ["preference"],
    selectable: true,
  },
  TXT: {
    type: "TXT",
    label: "TXT",
    description: "Arbitrary text — SPF, DKIM, domain verification.",
    dataLabel: "Content",
    dataPlaceholder: "v=spf1 include:_spf.example.com ~all",
    extraFields: [],
    selectable: true,
  },
  SRV: {
    type: "SRV",
    label: "SRV",
    description: "Advertises the host and port of a service.",
    dataLabel: "Target",
    dataPlaceholder: "sipserver.example.com",
    extraFields: ["priority", "weight", "port"],
    selectable: true,
  },
  CAA: {
    type: "CAA",
    label: "CAA",
    description:
      "Restricts which certificate authorities may issue for this name.",
    dataLabel: "Value",
    dataPlaceholder: "letsencrypt.org",
    extraFields: ["flags", "tag"],
    selectable: true,
  },
  PTR: {
    type: "PTR",
    label: "PTR",
    description: "Reverse mapping from an address to a name.",
    dataLabel: "Target",
    dataPlaceholder: "host.example.com",
    extraFields: [],
    selectable: true,
  },
};

export const SELECTABLE_RECORD_TYPES = Object.values(RECORD_TYPE_INFO)
  .filter((info) => info.selectable)
  .map((info) => info.type);

export const CAA_TAGS = ["issue", "issuewild", "iodef"] as const;

/**
 * Converts a user-entered name into an FQDN inside the zone apex. Follows these rules:
 * - Empty or `@` → the apex itself.
 * - Subdomain inside the apex with apex → return as-is.
 * - Subdomain inside the apex without apex → append the apex.
 */
export function toFqdn(input: string, apex: string): string {
  const trimmed = input.trim().replace(/\.$/, "").toLowerCase();

  if (trimmed === "" || trimmed === "@") return apex;
  if (trimmed === apex || trimmed.endsWith(`.${apex}`)) return trimmed;

  return `${trimmed}.${apex}`;
}

export function toRelativeName(fqdn: string, apex: string): string {
  if (fqdn === apex) return "@";
  return fqdn.endsWith(`.${apex}`) ? fqdn.slice(0, -(apex.length + 1)) : fqdn;
}

export function isApex(fqdn: string, apex: string): boolean {
  return fqdn === apex;
}

export function isFlattenedApexCname(record: DnsRecord, apex: string): boolean {
  return record.type === "ALIAS" && isApex(record.name, apex);
}

export function displayType(record: DnsRecord): string {
  return RECORD_TYPE_INFO[record.type].label;
}

const LABEL = /^(?!-)[a-z0-9-_]{1,63}(?<!-)$/;
const IPV4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/;

export function isValidHostname(value: string): boolean {
  const name = value.trim().replace(/\.$/, "").toLowerCase();
  if (name.length === 0 || name.length > 253) return false;
  
  return name
    .split(".")
    .every(
      (label, index) => (index === 0 && label === "*") || LABEL.test(label)
    );
}

export function isValidIpv4(value: string): boolean {
  const match = IPV4.exec(value.trim());
  if (!match) return false;

  return match.slice(1).every((octet) => {
    const n = Number(octet)
    return n >= 0 && n <= 255 && String(n) === String(Number(octet))
  });
}

export function isValidIpv6(value: string): boolean {
  const address = value.trim();
  if (!/^[0-9a-fA-F:.]+$/.test(address)) return false;
  
  const doubleColons = address.match(/::/g)?.length ?? 0;
  if (doubleColons > 1) return false;
  
  const groups = address.split(":").filter((g) => g !== "");
  if (groups.length > 8) return false;

  return groups.every((g) => /^[0-9a-fA-F]{1,4}$/.test(g) || isValidIpv4(g));
}

export const TTL_PRESETS = [
  { value: 0, label: "Auto" },
  { value: 60, label: "1 minute" },
  { value: 300, label: "5 minutes" },
  { value: 1800, label: "30 minutes" },
  { value: 3600, label: "1 hour" },
  { value: 43200, label: "12 hours" },
  { value: 86400, label: "1 day" },
] as const;

export function formatTtl(ttl: number): string {
  const preset = TTL_PRESETS.find((p) => p.value === ttl);
  if (preset) return preset.label;

  if (ttl % 3600 === 0) return `${ttl / 3600} hours`;
  if (ttl % 60 === 0) return `${ttl / 60} minutes`;
  
  return `${ttl} seconds`;
}

export function validateRecord(
  record: {
    name: string
    type: RecordType
    ttl: number
    data: string | null
    preference?: number | null
    priority?: number | null
    weight?: number | null
    port?: number | null
    flags?: number | null
    tag?: string | null
  },
  apex: string
): Record<string, string> {
  const errors: Record<string, string> = {};
  const data = record.data?.trim() ?? "";

  if (!isValidHostname(record.name)) {
    errors.name = "Not a valid host name.";
  } else if (record.name !== apex && !record.name.endsWith(`.${apex}`)) {
    errors.name = `Must be inside ${apex}.`;
  }

  if (record.ttl < 0 || record.ttl > 604800) {
    errors.ttl = "TTL must be between 0 and 604800 seconds.";
  }

  if (data === "" && record.type !== "TXT") {
    errors.data = `${RECORD_TYPE_INFO[record.type].dataLabel} is required.`;
    return errors;
  }

  switch (record.type) {
    case "A":
      if (!isValidIpv4(data)) errors.data = "Not a valid IPv4 address.";
      break;

    case "AAAA":
      if (!isValidIpv6(data)) errors.data = "Not a valid IPv6 address.";
      break;

    case "CNAME":
      if (!isValidHostname(data)) errors.data = "Not a valid target name.";
      else if (record.name === apex) errors.name = "A CNAME cannot live at the apex; it is stored as a flattened ALIAS.";
      break;

    case "ALIAS":
    case "NS":
    case "PTR":
      if (!isValidHostname(data)) errors.data = "Not a valid host name.";
      break;

    case "MX":
      if (!isValidHostname(data)) errors.data = "Not a valid mail server name.";
      if (
        record.preference == null ||
        record.preference < 0 ||
        record.preference > 65535
      )
        errors.preference = "Priority must be between 0 and 65535.";
      break;

    case "SRV":
      if (!isValidHostname(data)) errors.data = "Not a valid target name.";
      for (const field of ["priority", "weight"] as const) {
        const value = record[field];

        if (value == null || value < 0 || value > 65535) errors[field] = "Must be between 0 and 65535.";
      }

      if (record.port == null || record.port < 1 || record.port > 65535) errors.port = "Port must be between 1 and 65535.";
      break;

    case "CAA":
      if (!record.tag) errors.tag = "Tag is required.";
      if (record.flags != null && (record.flags < 0 || record.flags > 255)) errors.flags = "Flags must be between 0 and 255.";
      break;

    case "TXT":
      if (data.length > 65535) errors.data = "Too long.";
      break;
  };

  return errors;
}

export function formatRecordValue(record: DnsRecord): string {
  switch (record.type) {
    case "MX":
      return `${record.preference ?? 0} ${record.data ?? ""}`;

    case "SRV":
      return `${record.priority ?? 0} ${record.weight ?? 0} ${record.port ?? 0} ${record.data ?? ""}`;
    case "CAA":
      return `${record.flags ?? 0} ${record.tag ?? ""} "${record.data ?? ""}"`;

    default:
      return record.data ?? "";
  };
}
