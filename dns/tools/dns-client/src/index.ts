import readline from "node:readline/promises";
import { stdin, stdout } from "node:process";
import {
  className,
  decodeMessage,
  encodeQuery,
  Message,
  rcodeName,
  RECORD_TYPES,
  ResourceRecord,
  typeName,
} from "./protocol.ts";
import { sendQuery, Transport } from "./transport.ts";

interface Params {
  name: string;
  type: number;
  typeLabel: string;
  server: string;
  port: number;
  transport: Transport;
}

const DEFAULTS = {
  name: "example.com",
  type: "A",
  server: "127.0.0.1",
  port: "53",
  transport: "udp",
};

const TIMEOUT_MS = 5000;

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const params = args.length > 0 ? fromArgs(args) : await prompt();

  const { id, bytes: query } = encodeQuery({ name: params.name, type: params.type });

  console.log(
    `\n;; Sending ${query.length}-byte ${params.transport.toUpperCase()} query for ` +
      `${params.name} ${params.typeLabel} to ${params.server}:${params.port}`,
  );

  const startedAt = performance.now();
  const response = await sendQuery(params.transport, params.server, params.port, query, TIMEOUT_MS);
  const elapsedMs = performance.now() - startedAt;

  const message = decodeMessage(response);

  console.log(renderHumanReadable(message, elapsedMs, id));
  console.log(`\n;; QUERY BYTES (${query.length}):\n${hexdump(query)}`);
  console.log(`\n;; RESPONSE BYTES (${response.length}):\n${hexdump(response)}`);
}

function fromArgs(args: string[]): Params {
  const [name, type = DEFAULTS.type, server = DEFAULTS.server, port = DEFAULTS.port, transport = DEFAULTS.transport] = args;
  return buildParams(name, type, server, port, transport);
}

async function prompt(): Promise<Params> {
  return stdin.isTTY ? promptInteractive() : promptFromPipe();
}

// Interactive terminal: readline handles line editing and echo.
async function promptInteractive(): Promise<Params> {
  const rl = readline.createInterface({ input: stdin, output: stdout });
  try {
    const ask = async (label: string, def: string): Promise<string> => {
      const answer = (await rl.question(`${label} [${def}]: `)).trim();
      return answer.length > 0 ? answer : def;
    };

    const name = await ask("Name", DEFAULTS.name);
    const type = await ask(`Type (${Object.keys(RECORD_TYPES).join(", ")})`, DEFAULTS.type);
    const server = await ask("Server", DEFAULTS.server);
    const port = await ask("Port", DEFAULTS.port);
    const transport = await ask("Transport (udp/tcp)", DEFAULTS.transport);
    return buildParams(name, type, server, port, transport);
  } finally {
    rl.close();
  }
}

// Piped/redirected stdin (scripting, tests): read every line up front so none are lost.
async function promptFromPipe(): Promise<Params> {
  const chunks: Buffer[] = [];
  for await (const chunk of stdin) chunks.push(chunk as Buffer);
  const lines = Buffer.concat(chunks).toString("utf8").split(/\r?\n/);

  let i = 0;
  const ask = (label: string, def: string): string => {
    const raw = (lines[i++] ?? "").trim();
    const value = raw.length > 0 ? raw : def;
    stdout.write(`${label} [${def}]: ${value}\n`);
    return value;
  };

  const name = ask("Name", DEFAULTS.name);
  const type = ask(`Type (${Object.keys(RECORD_TYPES).join(", ")})`, DEFAULTS.type);
  const server = ask("Server", DEFAULTS.server);
  const port = ask("Port", DEFAULTS.port);
  const transport = ask("Transport (udp/tcp)", DEFAULTS.transport);
  return buildParams(name, type, server, port, transport);
}

function buildParams(name: string, type: string, server: string, port: string, transport: string): Params {
  const typeLabel = type.toUpperCase();
  const typeCode = RECORD_TYPES[typeLabel];
  if (typeCode === undefined) {
    throw new Error(`unknown record type "${type}". Known types: ${Object.keys(RECORD_TYPES).join(", ")}`);
  }

  const portNumber = Number.parseInt(port, 10);
  if (!Number.isInteger(portNumber) || portNumber < 1 || portNumber > 65535) {
    throw new Error(`invalid port "${port}"`);
  }

  const transportLabel = transport.toLowerCase();
  if (transportLabel !== "udp" && transportLabel !== "tcp") {
    throw new Error(`invalid transport "${transport}" (use udp or tcp)`);
  }

  return { name, type: typeCode, typeLabel, server, port: portNumber, transport: transportLabel };
}

function renderHumanReadable(message: Message, elapsedMs: number, sentId: number): string {
  const { header } = message;
  const flags = [
    header.qr && "qr",
    header.aa && "aa",
    header.tc && "tc",
    header.rd && "rd",
    header.ra && "ra",
  ].filter(Boolean).join(" ");

  const lines: string[] = [];
  lines.push("");
  lines.push(`;; ->>HEADER<<- opcode: QUERY, status: ${rcodeName(header.rcode)}, id: ${header.id}`);
  lines.push(
    `;; flags: ${flags};  QUERY: ${header.qdcount}, ANSWER: ${header.ancount}, ` +
      `AUTHORITY: ${header.nscount}, ADDITIONAL: ${header.arcount}`,
  );
  if (header.id !== sentId) lines.push(`;; WARNING: response id ${header.id} != query id ${sentId}`);
  if (header.tc) lines.push(";; WARNING: response was truncated (retry over TCP for the full answer)");

  if (message.questions.length > 0) {
    lines.push("");
    lines.push(";; QUESTION SECTION:");
    for (const q of message.questions) {
      lines.push(`;${q.name}\t\t${className(q.class)}\t${typeName(q.type)}`);
    }
  }

  const section = (title: string, records: ResourceRecord[]): void => {
    if (records.length === 0) return;
    lines.push("");
    lines.push(`;; ${title}:`);
    for (const rr of records) {
      lines.push(`${rr.name}\t${rr.ttl}\t${className(rr.class)}\t${typeName(rr.type)}\t${rr.rdata}`);
    }
  };

  section("ANSWER SECTION", message.answers);
  section("AUTHORITY SECTION", message.authorities);
  section("ADDITIONAL SECTION", message.additionals);

  lines.push("");
  lines.push(`;; Query time: ${elapsedMs.toFixed(1)} ms`);
  return lines.join("\n");
}

/** Classic hex + ASCII dump, 16 bytes per row. */
function hexdump(buf: Buffer): string {
  const lines: string[] = [];
  for (let offset = 0; offset < buf.length; offset += 16) {
    const slice = buf.subarray(offset, offset + 16);
    const hexCells: string[] = [];
    for (let i = 0; i < 16; i++) {
      hexCells.push(i < slice.length ? slice[i].toString(16).padStart(2, "0") : "  ");
      if (i === 7) hexCells.push(""); // gap between the two 8-byte halves
    }
    const ascii = Array.from(slice, (b) => (b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : ".")).join("");
    lines.push(`${offset.toString(16).padStart(4, "0")}  ${hexCells.join(" ").padEnd(49)}  |${ascii}|`);
  }
  return lines.join("\n") || "(empty)";
}

main().catch((err: unknown) => {
  console.error(`\nerror: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
});
