// Minimal DNS message encoding/decoding (RFC 1035) — just enough to query Hermes and to
// render both a human-readable answer and the raw wire bytes.

export const RECORD_TYPES: Record<string, number> = {
  A: 1,
  NS: 2,
  CNAME: 5,
  SOA: 6,
  PTR: 12,
  MX: 15,
  TXT: 16,
  AAAA: 28,
  SRV: 33,
  CAA: 257,
  ANY: 255,
};

export const TYPE_NAMES: Record<number, string> = Object.fromEntries(
  Object.entries(RECORD_TYPES).map(([name, value]) => [value, name]),
);

export const RCODES: Record<number, string> = {
  0: "NOERROR",
  1: "FORMERR",
  2: "SERVFAIL",
  3: "NXDOMAIN",
  4: "NOTIMP",
  5: "REFUSED",
  6: "YXDOMAIN",
  7: "YXRRSET",
  8: "NXRRSET",
  9: "NOTAUTH",
  10: "NOTZONE",
};

export function typeName(type: number): string {
  return TYPE_NAMES[type] ?? `TYPE${type}`;
}

export function className(cls: number): string {
  return cls === 1 ? "IN" : `CLASS${cls}`;
}

export function rcodeName(rcode: number): string {
  return RCODES[rcode] ?? `RCODE${rcode}`;
}

export interface Header {
  id: number;
  qr: boolean;
  opcode: number;
  aa: boolean;
  tc: boolean;
  rd: boolean;
  ra: boolean;
  z: number;
  rcode: number;
  qdcount: number;
  ancount: number;
  nscount: number;
  arcount: number;
}

export interface Question {
  name: string;
  type: number;
  class: number;
}

export interface ResourceRecord {
  name: string;
  type: number;
  class: number;
  ttl: number;
  rdlength: number;
  /** Human-readable RDATA (e.g. an IP address or a target name). */
  rdata: string;
  /** Raw RDATA bytes. */
  rdataRaw: Buffer;
}

export interface Message {
  header: Header;
  questions: Question[];
  answers: ResourceRecord[];
  authorities: ResourceRecord[];
  additionals: ResourceRecord[];
}

/** Encodes a domain name into length-prefixed labels terminated by a root octet. */
export function encodeName(name: string): Buffer {
  const trimmed = name.endsWith(".") ? name.slice(0, -1) : name;
  const labels = trimmed.length === 0 ? [] : trimmed.split(".");
  const parts: Buffer[] = [];
  for (const label of labels) {
    const bytes = Buffer.from(label, "utf8");
    if (bytes.length === 0) throw new Error(`invalid name "${name}": empty label`);
    if (bytes.length > 63) throw new Error(`invalid name "${name}": label "${label}" exceeds 63 bytes`);
    parts.push(Buffer.from([bytes.length]), bytes);
  }
  parts.push(Buffer.from([0]));
  return Buffer.concat(parts);
}

export interface QueryOptions {
  name: string;
  type: number;
  id?: number;
  recursionDesired?: boolean;
}

/** Builds a single-question DNS query message. */
export function encodeQuery(opts: QueryOptions): { id: number; bytes: Buffer } {
  const id = opts.id ?? Math.floor(Math.random() * 0x10000);
  const rd = opts.recursionDesired ?? true;

  const header = Buffer.alloc(12);
  header.writeUInt16BE(id, 0);
  header.writeUInt16BE(rd ? 0x0100 : 0x0000, 2); // QR=0, opcode=0, RD flag
  header.writeUInt16BE(1, 4); // QDCOUNT
  // ANCOUNT / NSCOUNT / ARCOUNT stay 0

  const question = Buffer.alloc(4);
  question.writeUInt16BE(opts.type, 0); // QTYPE
  question.writeUInt16BE(1, 2); // QCLASS = IN

  return { id, bytes: Buffer.concat([header, encodeName(opts.name), question]) };
}

/** Cursor over a DNS message that understands name compression pointers. */
class Reader {
  constructor(private readonly buf: Buffer, public pos = 0) {}

  u8(): number {
    const value = this.buf.readUInt8(this.pos);
    this.pos += 1;
    return value;
  }

  u16(): number {
    const value = this.buf.readUInt16BE(this.pos);
    this.pos += 2;
    return value;
  }

  u32(): number {
    const value = this.buf.readUInt32BE(this.pos);
    this.pos += 4;
    return value;
  }

  take(n: number): Buffer {
    const value = this.buf.subarray(this.pos, this.pos + n);
    this.pos += n;
    return value;
  }

  /** Reads a (possibly compressed) domain name, returning it with a trailing dot. */
  name(): string {
    const labels: string[] = [];
    let pos = this.pos;
    let jumped = false;
    let guard = 0;

    while (true) {
      if (guard++ > 255) throw new Error("malformed name (pointer loop or too long)");
      const len = this.buf.readUInt8(pos);

      if ((len & 0xc0) === 0xc0) {
        const offset = ((len & 0x3f) << 8) | this.buf.readUInt8(pos + 1);
        if (!jumped) this.pos = pos + 2; // real cursor stops after the pointer
        pos = offset;
        jumped = true;
        continue;
      }

      pos += 1;
      if (len === 0) break; // root label ends the name
      labels.push(this.buf.subarray(pos, pos + len).toString("utf8"));
      pos += len;
    }

    if (!jumped) this.pos = pos;
    return labels.length === 0 ? "." : labels.join(".") + ".";
  }
}

function decodeHeader(r: Reader): Header {
  const id = r.u16();
  const flags = r.u16();
  return {
    id,
    qr: ((flags >> 15) & 1) === 1,
    opcode: (flags >> 11) & 0xf,
    aa: ((flags >> 10) & 1) === 1,
    tc: ((flags >> 9) & 1) === 1,
    rd: ((flags >> 8) & 1) === 1,
    ra: ((flags >> 7) & 1) === 1,
    z: (flags >> 4) & 0x7,
    rcode: flags & 0xf,
    qdcount: r.u16(),
    ancount: r.u16(),
    nscount: r.u16(),
    arcount: r.u16(),
  };
}

function decodeQuestion(r: Reader): Question {
  const name = r.name();
  const type = r.u16();
  const cls = r.u16();
  return { name, type, class: cls };
}

function formatIPv4(rdata: Buffer): string {
  return `${rdata[0]}.${rdata[1]}.${rdata[2]}.${rdata[3]}`;
}

function formatIPv6(rdata: Buffer): string {
  const groups: number[] = [];
  for (let i = 0; i < 16; i += 2) groups.push(rdata.readUInt16BE(i));

  // Collapse the longest run (>= 2) of zero groups into "::".
  let bestStart = -1;
  let bestLen = 0;
  let curStart = -1;
  let curLen = 0;
  for (let i = 0; i < 8; i++) {
    if (groups[i] === 0) {
      if (curStart < 0) curStart = i;
      curLen++;
      if (curLen > bestLen) {
        bestLen = curLen;
        bestStart = curStart;
      }
    } else {
      curStart = -1;
      curLen = 0;
    }
  }

  if (bestLen < 2) return groups.map((g) => g.toString(16)).join(":");
  const head = groups.slice(0, bestStart).map((g) => g.toString(16)).join(":");
  const tail = groups.slice(bestStart + bestLen).map((g) => g.toString(16)).join(":");
  return `${head}::${tail}`;
}

/** Renders RDATA for known types; falls back to hex for anything else. */
function formatRdata(type: number, start: number, len: number, full: Buffer): string {
  const r = new Reader(full, start);
  const rdata = full.subarray(start, start + len);
  const end = start + len;

  switch (type) {
    case RECORD_TYPES.A:
      return formatIPv4(rdata);
    case RECORD_TYPES.AAAA:
      return formatIPv6(rdata);
    case RECORD_TYPES.NS:
    case RECORD_TYPES.CNAME:
    case RECORD_TYPES.PTR:
      return r.name();
    case RECORD_TYPES.MX: {
      const preference = r.u16();
      return `${preference} ${r.name()}`;
    }
    case RECORD_TYPES.TXT: {
      const chunks: string[] = [];
      while (r.pos < end) {
        const l = r.u8();
        chunks.push(JSON.stringify(r.take(l).toString("utf8")));
      }
      return chunks.join(" ");
    }
    case RECORD_TYPES.SOA: {
      const mname = r.name();
      const rname = r.name();
      const serial = r.u32();
      const refresh = r.u32();
      const retry = r.u32();
      const expire = r.u32();
      const minimum = r.u32();
      return `${mname} ${rname} ${serial} ${refresh} ${retry} ${expire} ${minimum}`;
    }
    case RECORD_TYPES.SRV: {
      const priority = r.u16();
      const weight = r.u16();
      const port = r.u16();
      return `${priority} ${weight} ${port} ${r.name()}`;
    }
    case RECORD_TYPES.CAA: {
      const flags = r.u8();
      const tagLen = r.u8();
      const tag = r.take(tagLen).toString("ascii");
      const value = full.subarray(r.pos, end).toString("utf8");
      return `${flags} ${tag} "${value}"`;
    }
    default:
      return `\\# ${len} ${rdata.toString("hex")}`;
  }
}

function decodeRecord(r: Reader, full: Buffer): ResourceRecord {
  const name = r.name();
  const type = r.u16();
  const cls = r.u16();
  const ttl = r.u32();
  const rdlength = r.u16();
  const rdataStart = r.pos;
  const rdataRaw = r.take(rdlength);
  const rdata = formatRdata(type, rdataStart, rdlength, full);
  return { name, type, class: cls, ttl, rdlength, rdata, rdataRaw };
}

/** Parses a full DNS response message. */
export function decodeMessage(buf: Buffer): Message {
  const r = new Reader(buf);
  const header = decodeHeader(r);

  const questions: Question[] = [];
  for (let i = 0; i < header.qdcount; i++) questions.push(decodeQuestion(r));

  const readRecords = (count: number): ResourceRecord[] => {
    const records: ResourceRecord[] = [];
    for (let i = 0; i < count; i++) records.push(decodeRecord(r, buf));
    return records;
  };

  return {
    header,
    questions,
    answers: readRecords(header.ancount),
    authorities: readRecords(header.nscount),
    additionals: readRecords(header.arcount),
  };
}
