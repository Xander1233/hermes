// DNS load generator.
//
// The job of this file is orchestration only: parse the CLI, spread the load across worker
// threads, and merge what they report. The actual sending lives in bench-worker.ts.
//
// It is built around one rule: the client must not be the thing being measured. A single
// Node event loop encoding a fresh Buffer per query, tracking in-flight work in a Map and
// reservoir-sampling latencies tops out well below what the server can serve, and the result
// looks like a slow server. So the work is split across threads, queries are pre-encoded,
// bookkeeping is flat typed arrays, and the run reports the client's own event-loop lag so a
// client-bound result is visible instead of being mistaken for a server limit.

import os from "node:os";
import { Worker } from "node:worker_threads";
import { BUCKETS, percentileUs } from "./histogram.ts";
import {
  C,
  CONTROL_SLOTS,
  COUNTER_SLOTS,
  CTL,
  F,
  FLOAT_SLOTS,
  type WorkerConfig,
} from "./bench-shared.ts";
import { RECORD_TYPES, rcodeName, typeName } from "./protocol.ts";

interface BenchConfig {
  server: string;
  port: number;
  transport: "udp" | "tcp";
  durationMs?: number;
  count?: number;
  warmupMs: number;
  concurrency: number;
  qps: number;
  timeoutMs: number;
  workers: number;
  sockets: number;
  pipeline: number;
  poolSize: number;
  domain: string;
  names: string[];
  types: number[];
  randomRatio: number;
}

interface Totals {
  sent: number;
  completed: number;
  truncated: number;
  timeouts: number;
  errors: number;
  inFlight: number;
  rcodes: number[];
  latSumUs: number;
  latMinUs: number;
  latMaxUs: number;
  lagMaxMs: number;
  lagAvgMs: number;
  hist: Uint32Array;
  done: number;
}

// ---- aggregation ---------------------------------------------------------------------

function aggregate(
  cfg: BenchConfig,
  counters: Int32Array,
  floats: Float64Array,
  hist: Uint32Array,
): Totals {
  const totals: Totals = {
    sent: 0,
    completed: 0,
    truncated: 0,
    timeouts: 0,
    errors: 0,
    inFlight: 0,
    rcodes: new Array(16).fill(0),
    latSumUs: 0,
    latMinUs: Infinity,
    latMaxUs: 0,
    lagMaxMs: 0,
    lagAvgMs: 0,
    hist: new Uint32Array(BUCKETS),
    done: 0,
  };

  let lagSum = 0;
  let lagSamples = 0;

  for (let w = 0; w < cfg.workers; w++) {
    const c = w * COUNTER_SLOTS;
    const f = w * FLOAT_SLOTS;

    totals.sent += counters[c + C.SENT];
    totals.completed += counters[c + C.COMPLETED];
    totals.truncated += counters[c + C.TRUNCATED];
    totals.timeouts += counters[c + C.TIMEOUTS];
    totals.errors += counters[c + C.ERRORS];
    totals.inFlight += counters[c + C.INFLIGHT];
    totals.done += counters[c + C.DONE];
    for (let i = 0; i < 16; i++) totals.rcodes[i] += counters[c + C.RCODE + i];

    totals.latSumUs += floats[f + F.LAT_SUM_US];
    const min = floats[f + F.LAT_MIN_US];
    if (min > 0 && min < totals.latMinUs) totals.latMinUs = min;
    if (floats[f + F.LAT_MAX_US] > totals.latMaxUs) totals.latMaxUs = floats[f + F.LAT_MAX_US];
    if (floats[f + F.LAG_MAX_MS] > totals.lagMaxMs) totals.lagMaxMs = floats[f + F.LAG_MAX_MS];
    lagSum += floats[f + F.LAG_SUM_MS];
    lagSamples += floats[f + F.LAG_SAMPLES];

    const base = w * BUCKETS;
    for (let i = 0; i < BUCKETS; i++) totals.hist[i] += hist[base + i];
  }

  if (totals.latMinUs === Infinity) totals.latMinUs = 0;
  totals.lagAvgMs = lagSamples > 0 ? lagSum / lagSamples : 0;
  return totals;
}

// ---- output --------------------------------------------------------------------------

function ms(us: number): string {
  return (us / 1000).toFixed(2);
}

function printProgress(cfg: BenchConfig, t: Totals, elapsed: number): void {
  const answered = t.completed - t.truncated;
  const qps = elapsed > 0 ? Math.round(answered / elapsed) : 0;
  const line =
    "[" +
    elapsed.toFixed(0).padStart(3) +
    "s] sent " +
    t.sent +
    "  ok " +
    t.completed +
    "  " +
    // Only shown once it happens, so a healthy run stays uncluttered, but the moment the
    // server starts rate limiting it appears rather than hiding inside `ok`.
    (t.truncated > 0 ? "trunc " + t.truncated + "  " : "") +
    "timeout " +
    t.timeouts +
    "  err " +
    t.errors +
    "  |  " +
    qps +
    " q/s  |  in-flight " +
    t.inFlight;
  process.stdout.write("\r" + line.padEnd(100));
}

function report(cfg: BenchConfig, t: Totals, elapsed: number, avgInFlight: number): void {
  const answered = t.completed - t.truncated;
  const total = t.completed;
  const avgUs = total > 0 ? t.latSumUs / total : 0;

  const rcodes = t.rcodes
    .map((n, code) => ({ n, code }))
    .filter((e) => e.n > 0)
    .sort((a, b) => b.n - a.n)
    .map((e) => rcodeName(e.code) + " " + e.n)
    .join(", ");

  process.stdout.write("\r".padEnd(102) + "\r");
  console.log("\n=== Benchmark results ===");
  console.log("transport   : " + cfg.transport);
  console.log("target      : " + cfg.server + ":" + cfg.port);
  console.log("duration    : " + elapsed.toFixed(2) + " s");
  console.log("sent        : " + t.sent);
  console.log(
    "completed   : " +
      t.completed +
      (t.truncated > 0 ? "  (answered " + answered + ", truncated " + t.truncated + ")" : ""),
  );
  console.log("timeouts    : " + t.timeouts);
  console.log("errors      : " + t.errors);
  console.log(
    // Throughput counts real answers only. A truncated reply carries no records, so treating
    // it as served would report a rate-limited server as healthy.
    "throughput  : " +
      (elapsed > 0 ? Math.round(answered / elapsed) : 0) +
      " q/s" +
      (t.truncated > 0 ? "  (answered only)" : ""),
  );
  console.log(
    "latency (ms): min " +
      ms(t.latMinUs) +
      "  avg " +
      ms(avgUs) +
      "  p50 " +
      ms(percentileUs(t.hist, total, 50)) +
      "  p90 " +
      ms(percentileUs(t.hist, total, 90)) +
      "  p99 " +
      ms(percentileUs(t.hist, total, 99)) +
      "  p99.9 " +
      ms(percentileUs(t.hist, total, 99.9)) +
      "  max " +
      ms(t.latMaxUs),
  );
  console.log("rcodes      : " + (rcodes || "(none)"));

  // The client's own vitals, so a client-bound run is obvious rather than being read as a
  // slow server.
  // On TCP the in-flight ceiling is connections times pipeline depth, not the connection count.
  const ceiling = cfg.transport === "tcp" ? cfg.concurrency * cfg.pipeline : cfg.concurrency;
  const fill = ceiling > 0 ? (avgInFlight / ceiling) * 100 : 0;
  console.log("\n--- load generator health ---");
  console.log(
    "workers     : " +
      cfg.workers +
      (cfg.transport === "udp"
        ? "  sockets " + cfg.workers * cfg.sockets
        : "  connections " + cfg.concurrency + "  pipeline " + cfg.pipeline),
  );
  console.log(
    "event-loop  : max lag " +
      t.lagMaxMs.toFixed(1) +
      " ms, avg " +
      t.lagAvgMs.toFixed(2) +
      " ms" +
      (t.lagMaxMs > 50 ? "   <-- client stalled" : ""),
  );
  console.log(
    "in-flight   : avg " + avgInFlight.toFixed(0) + " of " + ceiling + " (" + fill.toFixed(0) + "% full)",
  );

  const notes: string[] = [];

  if (t.lagMaxMs > 50) {
    notes.push(
      "The client's event loop stalled for up to " +
        t.lagMaxMs.toFixed(0) +
        " ms. Those latencies include time queued inside this process, so\n" +
        "  the run is measuring the load generator at least as much as the server. Raise --workers.",
    );
  }

  if (fill > 95 && t.lagMaxMs <= 50) {
    notes.push(
      "In-flight sat at the concurrency ceiling while the client stayed healthy, so the server was the\n" +
        "  limit: throughput above is its service rate and the latency is mostly queueing (it tracks\n" +
        "  concurrency / throughput). Raising --concurrency raises latency, not throughput.",
    );
  }

  if (fill < 40 && cfg.qps === 0 && t.lagMaxMs <= 50) {
    notes.push(
      "In-flight stayed well below the ceiling: the server answered as fast as this client could ask.\n" +
        "  Throughput here is a client-side send-rate figure. Raise --workers to push harder.",
    );
  }

  if (cfg.qps > 0) {
    const achieved = elapsed > 0 ? t.sent / elapsed : 0;
    if (achieved < cfg.qps * 0.95) {
      notes.push(
        "Target was " +
          cfg.qps +
          " q/s but only " +
          achieved.toFixed(0) +
          " q/s went out. " +
          (t.lagMaxMs > 50
            ? "The client could not keep up; raise --workers."
            : fill > 90
              ? "In-flight hit the ceiling waiting for answers, so the\n  server set the pace rather than the pacer."
              : "The client kept up, so the send path was blocked below this process."),
      );
    }
  }

  if (t.truncated > 0) {
    notes.push(
      "Truncated replies (TC=1) carry no records: the server is asking clients to retry over TCP,\n" +
        "  which is what response rate limiting does. For raw throughput, exempt this client in the\n" +
        "  server's [rate_limit] exempt list, or use --transport tcp, which is never rate limited.",
    );
  }

  if (t.timeouts > 0) {
    notes.push(
      t.timeouts +
        " queries got no answer within --timeout " +
        cfg.timeoutMs +
        " ms. At saturation that usually means\n" +
        "  responses were dropped once a socket buffer filled, rather than the server being slow.",
    );
  }

  if (notes.length > 0) console.log("\nnotes:\n" + notes.map((n) => "- " + n).join("\n"));
}

// ---- CLI ------------------------------------------------------------------------------

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

const HELP = `DNS benchmark / load generator

Usage: npm run bench -- [options]

Load is spread across worker threads so the client is not the bottleneck. The report ends
with the generator's own event-loop lag and in-flight fill, which say whether the numbers
describe the server or this process.

Options:
  --server <ip>          Target server (default 127.0.0.1)
  --port <n>             Target port (default 53)
  --transport udp|tcp    Transport (default udp)
  --duration <sec>       Run for this many seconds (default 10)
  --count <n>            Send exactly this many measured queries (overrides --duration)
  --warmup <sec>         Discard this much leading traffic from the stats (default 2)
  --workers <n>          Load-generator threads (default min(4, cpus/4))
  --concurrency <n>      Total in-flight queries (udp) / connections (tcp)
                         (default 1024 udp, 32 tcp)
  --sockets <n>          UDP sockets per worker (default 4). More source ports spread the
                         load across the server's SO_REUSEPORT listeners.
  --pipeline <n>         TCP queries in flight per connection (default 32)
  --qps <n>              Target queries/second, udp only (default: flat out)
  --timeout <ms>         Per-query timeout (default 2000)
  --pool <n>             Pre-encoded queries per worker (default 4096)
  --domain <name>        Base domain for random names (default example.com)
  --names a,b,c          Comma-separated names to query (default: zone names)
  --types A,AAAA,MX      Comma-separated record types (default A,AAAA,MX,TXT)
  --random-ratio <0..1>  Fraction of queries using a random subdomain (default 0.3)
  --help                 Show this help

Examples:
  npm run bench -- --duration 15
  npm run bench -- --workers 8 --concurrency 4096      # push harder
  npm run bench -- --qps 50000 --duration 30
  npm run bench -- --transport tcp --concurrency 64 --pipeline 64
  npm run bench -- --count 200000 --names www.example.com`;

function buildConfig(args: Record<string, string | boolean>): BenchConfig {
  const transport = (typeof args.transport === "string" ? args.transport : "udp").toLowerCase();
  if (transport !== "udp" && transport !== "tcp") throw new Error('invalid transport "' + transport + '"');

  const typeLabels = (typeof args.types === "string" ? args.types : "A,AAAA,MX,TXT")
    .split(",")
    .map((t) => t.trim().toUpperCase());
  const types = typeLabels.map((t) => {
    const code = RECORD_TYPES[t];
    if (code === undefined) throw new Error('unknown record type "' + t + '"');
    return code;
  });

  const domain = typeof args.domain === "string" ? args.domain : "example.com";
  const defaultNames = [domain, "www." + domain, "mail." + domain, "alias." + domain, "ns1." + domain];
  const names =
    typeof args.names === "string"
      ? args.names.split(",").map((n) => n.trim()).filter(Boolean)
      : defaultNames;

  const cpus = os.cpus().length || 4;
  // Deliberately conservative: a benchmark run against 127.0.0.1 shares these cores with the
  // server it is measuring, and taking most of them would slow down the thing under test.
  const defaultWorkers = Math.max(1, Math.min(4, Math.floor(cpus / 4)));
  const workers = Math.max(1, Math.min(64, num(args.workers, defaultWorkers)));

  const hasCount = typeof args.count === "string";
  const defaultConcurrency = transport === "tcp" ? 32 : 1024;
  const concurrency = Math.max(workers, num(args.concurrency, defaultConcurrency));

  return {
    server: typeof args.server === "string" ? args.server : "127.0.0.1",
    port: num(args.port, 53),
    transport,
    count: hasCount ? num(args.count, 0) : undefined,
    durationMs: hasCount ? undefined : num(args.duration, 10) * 1000,
    warmupMs: Math.max(0, num(args.warmup, 2) * 1000),
    concurrency,
    qps: typeof args.qps === "string" ? num(args.qps, 0) : 0,
    timeoutMs: num(args.timeout, 2000),
    workers,
    sockets: Math.max(1, num(args.sockets, 4)),
    pipeline: Math.max(1, num(args.pipeline, 32)),
    poolSize: Math.max(1, num(args.pool, 4096)),
    domain,
    names,
    types,
    randomRatio: Math.min(1, Math.max(0, num(args["random-ratio"], 0.3))),
  };
}

// ---- run ------------------------------------------------------------------------------

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    console.log(HELP);
    return;
  }

  const cfg = buildConfig(args);

  const counters = new Int32Array(new SharedArrayBuffer(cfg.workers * COUNTER_SLOTS * 4));
  const floats = new Float64Array(new SharedArrayBuffer(cfg.workers * FLOAT_SLOTS * 8));
  const hist = new Uint32Array(new SharedArrayBuffer(cfg.workers * BUCKETS * 4));
  const control = new Int32Array(new SharedArrayBuffer(CONTROL_SLOTS * 4));

  console.log("=== DNS benchmark ===");
  console.log("target      : " + cfg.server + ":" + cfg.port + " (" + cfg.transport + ")");
  console.log(
    cfg.count !== undefined
      ? "count       : " + cfg.count + " measured queries"
      : "duration    : " + (cfg.durationMs ?? 0) / 1000 + " s" + (cfg.warmupMs > 0 ? " (+ " + cfg.warmupMs / 1000 + " s warmup)" : ""),
  );
  console.log(
    "workers     : " +
      cfg.workers +
      (cfg.transport === "udp"
        ? "  in-flight " + cfg.concurrency + "  sockets " + cfg.workers * cfg.sockets
        : "  connections " + cfg.concurrency + "  pipeline " + cfg.pipeline) +
      (cfg.qps > 0 ? "  target " + cfg.qps + " q/s" : ""),
  );
  console.log("types       : " + cfg.types.map(typeName).join(", "));
  console.log(
    "names       : " +
      (cfg.randomRatio > 0 ? Math.round(cfg.randomRatio * 100) + "% random *." + cfg.domain + ", " : "") +
      (cfg.names.join(", ") || "(random only)"),
  );
  console.log("");

  // Split the load. Each worker owns its share of the in-flight budget, the rate and the
  // query count, so no coordination is needed once they are running.
  const workers: Worker[] = [];
  const exits: Promise<void>[] = [];
  const base = Math.floor(cfg.concurrency / cfg.workers);
  const countBase = cfg.count !== undefined ? Math.floor(cfg.count / cfg.workers) : 0;

  for (let i = 0; i < cfg.workers; i++) {
    const wcfg: WorkerConfig = {
      index: i,
      server: cfg.server,
      port: cfg.port,
      transport: cfg.transport,
      concurrency: base + (i < cfg.concurrency % cfg.workers ? 1 : 0),
      qps: cfg.qps > 0 ? Math.floor(cfg.qps / cfg.workers) : 0,
      count: cfg.count !== undefined ? countBase + (i < cfg.count % cfg.workers ? 1 : 0) : 0,
      timeoutMs: cfg.timeoutMs,
      sockets: cfg.sockets,
      pipeline: cfg.pipeline,
      poolSize: cfg.poolSize,
      seed: 0x9e3779b9 ^ (i * 0x85ebca6b),
      domain: cfg.domain,
      names: cfg.names,
      types: cfg.types,
      randomRatio: cfg.randomRatio,
    };

    const worker = new Worker(new URL("./bench-worker.ts", import.meta.url), {
      workerData: { cfg: wcfg, counters: counters.buffer, floats: floats.buffer, hist: hist.buffer, control: control.buffer },
    });
    worker.on("message", (m: unknown) => {
      if (typeof m === "string" && m.startsWith("error:")) console.error("\nworker " + i + " " + m);
    });
    worker.on("error", (e) => console.error("\nworker " + i + " failed: " + e.message));
    workers.push(worker);
    exits.push(new Promise<void>((resolve) => worker.once("exit", () => resolve())));
  }

  const stop = (): void => {
    Atomics.store(control, CTL.STOP, 1);
  };
  process.once("SIGINT", stop);

  // Warmup runs at full tilt but its samples are thrown away, so JIT warmup and connection
  // setup do not land in the percentiles.
  if (cfg.warmupMs > 0) await new Promise((r) => setTimeout(r, cfg.warmupMs));
  Atomics.store(control, CTL.RECORD, 1);
  const measureStart = performance.now();

  if (cfg.durationMs !== undefined) setTimeout(stop, cfg.durationMs);

  let inFlightSum = 0;
  let inFlightSamples = 0;
  let lastProgress = 0;

  const sampler = setInterval(() => {
    const t = aggregate(cfg, counters, floats, hist);
    inFlightSum += t.inFlight;
    inFlightSamples++;

    const elapsed = (performance.now() - measureStart) / 1000;
    if (elapsed - lastProgress >= 1) {
      lastProgress = elapsed;
      printProgress(cfg, t, elapsed);
    }
  }, 200);

  await Promise.all(exits);
  clearInterval(sampler);
  process.removeListener("SIGINT", stop);

  const elapsed = (performance.now() - measureStart) / 1000;
  const totals = aggregate(cfg, counters, floats, hist);
  report(cfg, totals, elapsed, inFlightSamples > 0 ? inFlightSum / inFlightSamples : 0);
}

main().catch((err: unknown) => {
  console.error("\nerror: " + (err instanceof Error ? err.message : String(err)));
  process.exit(1);
});
