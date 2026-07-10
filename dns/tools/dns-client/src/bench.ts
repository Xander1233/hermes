// DNS load generator / benchmark.
//
// UDP mode multiplexes many in-flight queries over a single connected socket, matching
// responses to requests by transaction ID (so it can push high query rates without
// exhausting sockets/ports). TCP mode uses a pool of `concurrency` workers, each doing
// one connect→query→close at a time. Names and record types are randomized.
import dgram from "node:dgram";
import { encodeQuery, RECORD_TYPES, rcodeName, typeName } from "./protocol.ts";
import { sendQuery } from "./transport.ts";

interface BenchConfig {
  server: string;
  port: number;
  transport: "udp" | "tcp";
  durationMs?: number; // used when `count` is undefined
  count?: number;
  concurrency: number; // max in-flight (UDP) / worker count (TCP)
  qps?: number; // target rate; when unset, run flat-out at `concurrency`
  timeoutMs: number;
  domain: string;
  names: string[];
  types: number[];
  randomRatio: number; // fraction of queries that use a random subdomain of `domain`
}

interface Stats {
  sent: number;
  completed: number;
  /**
   * Responses that came back with TC=1 and no records.
   *
   * Counted separately because a truncated reply still carries rcode NOERROR, so without this
   * it is indistinguishable from a real answer — and under response rate limiting it is the
   * single most interesting number: the server telling the client to retry over TCP. Lumping
   * them into `completed` makes a rate-limited run look healthy.
   */
  truncated: number;
  timeouts: number;
  errors: number;
  rcodes: Map<number, number>;
  latencies: number[]; // reservoir sample of latencies (ms)
  startedAt: number;
}

const LATENCY_CAP = 200_000;

function newStats(): Stats {
  return {
    sent: 0,
    completed: 0,
    truncated: 0,
    timeouts: 0,
    errors: 0,
    rcodes: new Map(),
    latencies: [],
    startedAt: 0,
  };
}

function recordLatency(stats: Stats, ms: number): void {
  // Reservoir sampling so long/fast runs stay bounded in memory.
  if (stats.latencies.length < LATENCY_CAP) {
    stats.latencies.push(ms);
  } else {
    const j = Math.floor(Math.random() * stats.completed);
    if (j < LATENCY_CAP) stats.latencies[j] = ms;
  }
}

/**
 * Records a response, reading the rcode and the TC flag straight off the wire.
 *
 * Takes the whole message rather than a pre-extracted rcode so the header flags stay
 * available — TC lives in byte 2, the rcode in byte 3.
 */
function recordResponse(stats: Stats, response: Buffer, latencyMs: number): void {
  stats.completed++;
  recordLatency(stats, latencyMs);

  const rcode = response.length >= 4 ? response[3] & 0x0f : 0;
  stats.rcodes.set(rcode, (stats.rcodes.get(rcode) ?? 0) + 1);

  // TC is bit 9 of the flags word, so it is bit 1 of the high byte.
  if (response.length >= 3 && (response[2] & 0x02) !== 0) {
    stats.truncated++;
  }
}

/** Builds a generator that yields randomized {name, type} pairs. */
function makeGenerator(cfg: BenchConfig): () => { name: string; type: number } {
  const randomLabel = () => "b" + Math.random().toString(36).slice(2, 10);
  return () => {
    const type = cfg.types[(Math.random() * cfg.types.length) | 0];
    const useRandom = cfg.names.length === 0 || Math.random() < cfg.randomRatio;
    const name = useRandom ? `${randomLabel()}.${cfg.domain}` : cfg.names[(Math.random() * cfg.names.length) | 0];
    return { name, type };
  };
}

async function benchUdp(cfg: BenchConfig, gen: () => { name: string; type: number }, stats: Stats): Promise<void> {
  const socket = dgram.createSocket(cfg.server.includes(":") ? "udp6" : "udp4");
  await new Promise<void>((resolve, reject) => {
    socket.once("error", reject);
    socket.connect(cfg.port, cfg.server, () => {
      socket.removeListener("error", reject);
      resolve();
    });
  });

  const pending = new Map<number, number>(); // id -> sentAt (ms)
  let nextId = 1;
  let stopSending = false;
  let finish!: () => void;
  const finished = new Promise<void>((r) => (finish = r));
  const cleanups: Array<() => void> = [];

  const allocId = (): number => {
    for (let i = 0; i < 0x10000; i++) {
      const id = nextId;
      nextId = (nextId + 1) & 0xffff;
      if (!pending.has(id)) return id;
    }
    return -1; // every id in flight (concurrency >= 65536)
  };

  const stopCondition = (): boolean =>
    cfg.count !== undefined ? stats.sent >= cfg.count : performance.now() - stats.startedAt >= (cfg.durationMs ?? 0);

  const maybeFinish = (): void => {
    if (stopSending && pending.size === 0) finish();
  };

  const sendOne = (): boolean => {
    const id = allocId();
    if (id < 0) return false;
    const { name, type } = gen();
    let bytes: Buffer;
    try {
      bytes = encodeQuery({ name, type, id }).bytes;
    } catch {
      stats.errors++; // e.g. an invalid generated name
      return true;
    }
    pending.set(id, performance.now());
    stats.sent++;
    socket.send(bytes, (err) => {
      if (err) {
        stats.errors++;
        pending.delete(id);
      }
    });
    return true;
  };

  const pump = (): void => {
    if (stopSending) return;
    while (pending.size < cfg.concurrency) {
      if (stopCondition()) {
        stopSending = true;
        maybeFinish();
        return;
      }
      if (!sendOne()) return;
    }
  };

  socket.on("message", (msg) => {
    if (msg.length < 4) return;
    const id = msg.readUInt16BE(0);
    const sentAt = pending.get(id);
    if (sentAt === undefined) return; // late/duplicate/unknown
    pending.delete(id);
    recordResponse(stats, msg, performance.now() - sentAt);
    if (!cfg.qps) pump();
    maybeFinish();
  });
  socket.on("error", () => { stats.errors++; });

  // Timeout sweeper.
  const sweeper = setInterval(() => {
    const cutoff = performance.now() - cfg.timeoutMs;
    for (const [id, sentAt] of pending) {
      if (sentAt <= cutoff) {
        pending.delete(id);
        stats.timeouts++;
      }
    }
    if (!cfg.qps && !stopSending) pump();
    maybeFinish();
  }, 200);
  cleanups.push(() => clearInterval(sweeper));

  // Progress line.
  const progress = setInterval(() => printProgress(cfg, stats, pending.size), 1000);
  cleanups.push(() => clearInterval(progress));

  stats.startedAt = performance.now();

  if (cfg.qps) {
    // Pace by elapsed time (catch up to qps * elapsed) so the rate stays accurate even
    // when the OS timer fires coarsely (Windows ticks at ~15 ms, not 5 ms).
    const timer = setInterval(() => {
      if (stopCondition()) {
        stopSending = true;
        maybeFinish();
        return;
      }
      const elapsedSec = (performance.now() - stats.startedAt) / 1000;
      const target = Math.floor(cfg.qps! * elapsedSec);
      while (stats.sent < target && pending.size < cfg.concurrency) {
        if (stopCondition()) { stopSending = true; break; }
        if (!sendOne()) break;
      }
    }, 5);
    cleanups.push(() => clearInterval(timer));
  } else {
    pump();
  }

  // Duration guard (count mode ends via stopCondition in pump).
  if (cfg.count === undefined) {
    const guard = setTimeout(() => { stopSending = true; maybeFinish(); }, cfg.durationMs);
    cleanups.push(() => clearTimeout(guard));
  }

  // Hard drain deadline: after sending stops, don't wait forever for stragglers.
  const drainDeadline = (cfg.durationMs ?? 0) + cfg.timeoutMs + 1000;
  const hardStop = setTimeout(() => {
    stats.timeouts += pending.size;
    pending.clear();
    finish();
  }, (cfg.count !== undefined ? cfg.timeoutMs + 60_000 : drainDeadline));
  cleanups.push(() => clearTimeout(hardStop));

  const onSigint = () => { stopSending = true; maybeFinish(); };
  process.once("SIGINT", onSigint);
  cleanups.push(() => process.removeListener("SIGINT", onSigint));

  await finished;
  for (const c of cleanups) c();
  socket.close();
}

async function benchTcp(cfg: BenchConfig, gen: () => { name: string; type: number }, stats: Stats): Promise<void> {
  let stop = false;
  const stopCondition = (): boolean =>
    stop || (cfg.count !== undefined ? stats.sent >= cfg.count : performance.now() - stats.startedAt >= (cfg.durationMs ?? 0));

  const onSigint = () => { stop = true; };
  process.once("SIGINT", onSigint);

  stats.startedAt = performance.now();
  const progress = setInterval(() => printProgress(cfg, stats, 0), 1000);
  if (cfg.count === undefined) setTimeout(() => { stop = true; }, cfg.durationMs);

  const worker = async (): Promise<void> => {
    while (!stopCondition()) {
      const { name, type } = gen();
      let bytes: Buffer;
      try {
        bytes = encodeQuery({ name, type }).bytes;
      } catch {
        stats.errors++;
        continue;
      }
      const t0 = performance.now();
      stats.sent++;
      try {
        const resp = await sendQuery("tcp", cfg.server, cfg.port, bytes, cfg.timeoutMs);
        recordResponse(stats, resp, performance.now() - t0);
      } catch {
        stats.timeouts++;
      }
    }
  };

  await Promise.all(Array.from({ length: cfg.concurrency }, () => worker()));
  clearInterval(progress);
  process.removeListener("SIGINT", onSigint);
}

function printProgress(cfg: BenchConfig, stats: Stats, inFlight: number): void {
  const elapsed = (performance.now() - stats.startedAt) / 1000;
  const qps = elapsed > 0 ? Math.round(stats.completed / elapsed) : 0;
  const line =
    `[${elapsed.toFixed(0).padStart(3)}s] sent ${stats.sent}  ok ${stats.completed}  ` +
    // Only shown once it happens, so a normal run stays uncluttered — but the moment the
    // server starts rate limiting, it appears rather than hiding inside `ok`.
    (stats.truncated > 0 ? `trunc ${stats.truncated}  ` : "") +
    `timeout ${stats.timeouts}  err ${stats.errors}  |  ${qps} q/s` +
    (cfg.transport === "udp" ? `  |  in-flight ${inFlight}` : "");
  process.stdout.write(`\r${line.padEnd(90)}`);
}

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const rank = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.min(sorted.length - 1, Math.max(0, rank))];
}

function report(cfg: BenchConfig, stats: Stats): void {
  const elapsed = (performance.now() - stats.startedAt) / 1000;
  const sorted = stats.latencies.slice().sort((a, b) => a - b);
  const avg = sorted.length ? sorted.reduce((a, b) => a + b, 0) / sorted.length : 0;

  const rcodes = [...stats.rcodes.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([code, n]) => `${rcodeName(code)} ${n}`)
    .join(", ");

  process.stdout.write("\r".padEnd(92) + "\r"); // clear progress line
  console.log("\n=== Benchmark results ===");
  console.log(`transport   : ${cfg.transport}`);
  console.log(`target      : ${cfg.server}:${cfg.port}`);
  console.log(`duration    : ${elapsed.toFixed(2)} s`);
  const answered = stats.completed - stats.truncated;

  console.log(`sent        : ${stats.sent}`);
  console.log(
    `completed   : ${stats.completed}` +
      (stats.truncated > 0 ? `  (answered ${answered}, truncated ${stats.truncated})` : ""),
  );
  console.log(`timeouts    : ${stats.timeouts}`);
  console.log(`errors      : ${stats.errors}`);
  console.log(
    // Throughput counts real answers only. A truncated reply carries no records — treating it
    // as served would report a rate-limited server as healthy.
    `throughput  : ${elapsed > 0 ? Math.round(answered / elapsed) : 0} q/s` +
      (stats.truncated > 0 ? `  (answered only)` : ""),
  );
  console.log(
    `latency (ms): min ${fmt(sorted[0] ?? 0)}  avg ${fmt(avg)}  p50 ${fmt(percentile(sorted, 50))}  ` +
      `p90 ${fmt(percentile(sorted, 90))}  p99 ${fmt(percentile(sorted, 99))}  max ${fmt(sorted[sorted.length - 1] ?? 0)}`,
  );
  console.log(`rcodes      : ${rcodes || "(none)"}`);

  if (stats.truncated > 0 || stats.timeouts > 0) {
    console.log(
      "\nnote: truncated replies (TC=1) carry no records — the server is asking clients to " +
        "retry over TCP,\n      which is what response rate limiting does. Timeouts here are " +
        "most likely dropped responses,\n      not a slow server. To measure real throughput, " +
        "exempt this client in the server's\n      [rate_limit] exempt list, or run with --transport tcp (never rate limited).",
    );
  }
}

function fmt(ms: number): string {
  return ms.toFixed(2);
}

// ---- CLI ----

function parseArgs(argv: string[]): Record<string, string | boolean> {
  const out: Record<string, string | boolean> = {};
  for (let i = 0; i < argv.length; i++) {
    let a = argv[i];
    if (!a.startsWith("--")) continue;
    a = a.slice(2);
    const eq = a.indexOf("=");
    if (eq >= 0) {
      out[a.slice(0, eq)] = a.slice(eq + 1);
    } else if (i + 1 < argv.length && !argv[i + 1].startsWith("--")) {
      out[a] = argv[++i];
    } else {
      out[a] = true;
    }
  }
  return out;
}

function num(v: string | boolean | undefined, def: number): number {
  if (typeof v !== "string") return def;
  const n = Number(v);
  return Number.isFinite(n) ? n : def;
}

const HELP = `DNS benchmark / stress test

Usage: npm run bench -- [options]

Options:
  --server <ip>          Target server (default 127.0.0.1)
  --port <n>             Target port (default 5353)
  --transport udp|tcp    Transport (default udp)
  --duration <sec>       Run for this many seconds (default 10)
  --count <n>            Send exactly this many queries (overrides --duration)
  --concurrency <n>      Max in-flight (udp) / worker count (tcp) (default 200 udp, 50 tcp)
  --qps <n>              Target queries/second (udp only; default: flat-out)
  --timeout <ms>         Per-query timeout (default 2000)
  --domain <name>        Base domain for random names (default example.com)
  --names a,b,c          Comma-separated names to query (default: zone names)
  --types A,AAAA,MX      Comma-separated record types (default A,AAAA,MX,TXT)
  --random-ratio <0..1>  Fraction of queries using a random subdomain (default 0.3)
  --help                 Show this help

Examples:
  npm run bench -- --duration 15 --concurrency 300
  npm run bench -- --qps 5000 --duration 30 --types A,AAAA
  npm run bench -- --count 100000 --names www.example.com,mail.example.com`;

function buildConfig(args: Record<string, string | boolean>): BenchConfig {
  const transport = (typeof args.transport === "string" ? args.transport : "udp").toLowerCase();
  if (transport !== "udp" && transport !== "tcp") throw new Error(`invalid transport "${transport}"`);

  const typeLabels = (typeof args.types === "string" ? args.types : "A,AAAA,MX,TXT").split(",").map((t) => t.trim().toUpperCase());
  const types = typeLabels.map((t) => {
    const code = RECORD_TYPES[t];
    if (code === undefined) throw new Error(`unknown record type "${t}"`);
    return code;
  });

  const domain = typeof args.domain === "string" ? args.domain : "example.com";
  const defaultNames = [domain, `www.${domain}`, `mail.${domain}`, `alias.${domain}`, `ns1.${domain}`];
  const names = typeof args.names === "string" ? args.names.split(",").map((n) => n.trim()).filter(Boolean) : defaultNames;

  const hasCount = typeof args.count === "string";
  const defaultConcurrency = transport === "tcp" ? 50 : 200;

  return {
    server: typeof args.server === "string" ? args.server : "127.0.0.1",
    port: num(args.port, 53),
    transport,
    count: hasCount ? num(args.count, 0) : undefined,
    durationMs: hasCount ? undefined : num(args.duration, 10) * 1000,
    concurrency: num(args.concurrency, defaultConcurrency),
    qps: typeof args.qps === "string" ? num(args.qps, 0) : undefined,
    timeoutMs: num(args.timeout, 2000),
    domain,
    names,
    types,
    randomRatio: Math.min(1, Math.max(0, num(args["random-ratio"], 0.3))),
  };
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    console.log(HELP);
    return;
  }

  const cfg = buildConfig(args);
  const stats = newStats();
  const gen = makeGenerator(cfg);

  console.log("=== DNS benchmark ===");
  console.log(`target      : ${cfg.server}:${cfg.port} (${cfg.transport})`);
  console.log(cfg.count !== undefined ? `count       : ${cfg.count}` : `duration    : ${(cfg.durationMs ?? 0) / 1000} s`);
  console.log(`concurrency : ${cfg.concurrency}${cfg.qps ? `  target qps  : ${cfg.qps}` : ""}`);
  console.log(`types       : ${cfg.types.map(typeName).join(", ")}`);
  console.log(`names       : ${cfg.randomRatio > 0 ? `${Math.round(cfg.randomRatio * 100)}% random *.${cfg.domain}, ` : ""}${cfg.names.join(", ") || "(random only)"}`);
  console.log("");

  if (cfg.transport === "udp") {
    await benchUdp(cfg, gen, stats);
  } else {
    await benchTcp(cfg, gen, stats);
  }

  report(cfg, stats);
}

main().catch((err: unknown) => {
  console.error(`\nerror: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
});
