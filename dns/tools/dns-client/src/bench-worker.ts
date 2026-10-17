// One load-generator worker. Several of these run in parallel worker threads so the client
// is not limited to a single event loop.
//
// The hot path is deliberately allocation-free:
//   * queries are encoded once into a pool at startup, then copied into a per-slot buffer and
//     stamped with a transaction id, so no Buffer is built per query;
//   * in-flight bookkeeping lives in flat typed arrays indexed by slot, not in a Map;
//   * latencies go into a bucketed histogram, so there is no reservoir sampling and no sort;
//   * counters are plain locals, copied into shared memory a few times a second.
//
// UDP spreads its traffic over several connected sockets. That is not cosmetic. The server
// binds its listeners with SO_REUSEPORT and the kernel picks a listener by hashing the
// 4-tuple, so a single client socket is one flow and would pin an entire run to one of the
// server's receive loops.
//
// TCP keeps its connections open and pipelines queries down them (RFC 7766) instead of
// paying a handshake per query.

import dgram from "node:dgram";
import net from "node:net";
import { parentPort, workerData } from "node:worker_threads";
import { BUCKETS, bucketOf } from "./histogram.ts";
import { C, COUNTER_SLOTS, CTL, F, FLOAT_SLOTS, type WorkerConfig, type WorkerInit } from "./bench-shared.ts";
import { encodeQuery } from "./protocol.ts";

const init = workerData as WorkerInit;
const cfg = init.cfg;

const counters = new Int32Array(init.counters);
const floats = new Float64Array(init.floats);
const histShared = new Uint32Array(init.hist);
const control = new Int32Array(init.control);

const cBase = cfg.index * COUNTER_SLOTS;
const fBase = cfg.index * FLOAT_SLOTS;
const hBase = cfg.index * BUCKETS;

// ---- local (non-shared) accumulators -------------------------------------------------

const hist = new Uint32Array(BUCKETS);
const rcodes = new Int32Array(16);
let sent = 0;
let completed = 0;
let truncated = 0;
let timeouts = 0;
let errors = 0;
let saturated = 0;
let inFlight = 0;
let latSum = 0;
let latMin = Infinity;
let latMax = 0;
let lagMax = 0;
let lagSum = 0;
let lagSamples = 0;

let recording = false;
let stopping = false;
let finished = false;
let drainDeadline: number | undefined;

// Pacing reference for --qps. Both are re-based when warmup ends, so the pacer does not see
// the warmup traffic as a deficit and burst to catch up.
let paceStart = 0;
let paceBase = 0;

function resetStats(): void {
  hist.fill(0);
  rcodes.fill(0);
  sent = 0;
  completed = 0;
  truncated = 0;
  timeouts = 0;
  errors = 0;
  saturated = 0;
  latSum = 0;
  latMin = Infinity;
  latMax = 0;
  lagMax = 0;
  lagSum = 0;
  lagSamples = 0;

  // Queries already in flight when warmup ended will still be answered, so carry them over
  // as sent. Zeroing outright would let `completed` exceed `sent`.
  sent = inFlight;
  paceStart = performance.now();
  paceBase = sent;
}

function record(latencyUs: number, msg: Buffer, offset: number, length: number): void {
  completed++;
  if (latencyUs < latMin) latMin = latencyUs;
  if (latencyUs > latMax) latMax = latencyUs;
  latSum += latencyUs;
  hist[bucketOf(latencyUs)]++;

  if (length >= 4) {
    rcodes[msg[offset + 3] & 0x0f]++;
    // TC is bit 9 of the flags word, so bit 1 of the high byte. A truncated reply carries no
    // records, so counting it as an answer would make a rate-limited server look healthy.
    if ((msg[offset + 2] & 0x02) !== 0) truncated++;
  }
}

function flush(): void {
  const nowRecording = Atomics.load(control, CTL.RECORD) === 1;
  if (nowRecording && !recording) {
    // Warmup just ended: discard everything measured while the JIT was still warming up.
    resetStats();
    recording = true;
    return;
  }
  recording = nowRecording;

  counters[cBase + C.SENT] = sent;
  counters[cBase + C.COMPLETED] = completed;
  counters[cBase + C.TRUNCATED] = truncated;
  counters[cBase + C.TIMEOUTS] = timeouts;
  counters[cBase + C.ERRORS] = errors;
  counters[cBase + C.SATURATED] = saturated;
  counters[cBase + C.INFLIGHT] = inFlight;
  for (let i = 0; i < 16; i++) counters[cBase + C.RCODE + i] = rcodes[i];

  floats[fBase + F.LAT_SUM_US] = latSum;
  floats[fBase + F.LAT_MIN_US] = latMin === Infinity ? 0 : latMin;
  floats[fBase + F.LAT_MAX_US] = latMax;
  floats[fBase + F.LAG_MAX_MS] = lagMax;
  floats[fBase + F.LAG_SUM_MS] = lagSum;
  floats[fBase + F.LAG_SAMPLES] = lagSamples;

  histShared.set(hist, hBase);
}

function finish(): void {
  if (finished) return;
  finished = true;
  flush();
  Atomics.store(counters, cBase + C.DONE, 1);
  parentPort?.postMessage("done");
}

// ---- query pool ----------------------------------------------------------------------

/** xorshift32: a few ns per call, versus Math.random() plus a string allocation per name. */
function makeRng(seed: number): () => number {
  let s = seed | 0 || 0x9e3779b9;
  return () => {
    s ^= s << 13;
    s |= 0;
    s ^= s >>> 17;
    s ^= s << 5;
    s |= 0;
    return (s >>> 0) / 4294967296;
  };
}

interface Pool {
  /** Bare DNS queries, transaction ids left at zero. */
  packets: Buffer[];
  /** The same queries with the 2-byte TCP length prefix already applied. */
  framed: Buffer[];
  maxLen: number;
  maxFramedLen: number;
}

function buildPool(config: WorkerConfig): Pool {
  const rng = makeRng(config.seed);
  const packets: Buffer[] = [];
  const framed: Buffer[] = [];
  let maxLen = 0;
  let maxFramedLen = 0;

  for (let i = 0; i < config.poolSize; i++) {
    const type = config.types[(rng() * config.types.length) | 0];
    const useRandom = config.names.length === 0 || rng() < config.randomRatio;
    const label = "b" + Math.floor(rng() * 0xffffffff).toString(36);
    const name = useRandom ? label + "." + config.domain : config.names[(rng() * config.names.length) | 0];

    let bytes: Buffer;
    try {
      bytes = encodeQuery({ name, type, id: 0 }).bytes;
    } catch {
      continue; // an unencodable generated name; leave it out of the pool
    }

    const withPrefix = Buffer.allocUnsafe(2 + bytes.length);
    withPrefix.writeUInt16BE(bytes.length, 0);
    bytes.copy(withPrefix, 2);

    packets.push(bytes);
    framed.push(withPrefix);
    if (bytes.length > maxLen) maxLen = bytes.length;
    if (withPrefix.length > maxFramedLen) maxFramedLen = withPrefix.length;
  }

  if (packets.length === 0) throw new Error("could not encode any queries for the pool");
  return { packets, framed, maxLen, maxFramedLen };
}

// ---- UDP -----------------------------------------------------------------------------

async function runUdp(pool: Pool): Promise<void> {
  const slots = cfg.concurrency;

  // The transaction id carries both the slot and a generation counter, so a response that
  // arrives after its slot was recycled is discarded rather than credited to whatever query
  // now occupies that slot. Under rate limiting that case is common.
  const slotBits = Math.max(1, 32 - Math.clz32(Math.max(1, slots - 1)));
  const slotMask = (1 << slotBits) - 1;
  const epochBits = Math.max(0, 16 - slotBits);
  const epochMask = epochBits > 0 ? (1 << epochBits) - 1 : 0;

  const sentAt = new Float64Array(slots); // 0 means the slot is free
  const epochOf = new Uint16Array(slots);
  const freeSlots = new Int32Array(slots);
  let freeTop = slots;
  for (let i = 0; i < slots; i++) freeSlots[i] = i;

  // One contiguous allocation carved into per-slot views. A slot is only rewritten once its
  // response or timeout has landed, so mutating it cannot race an in-flight send.
  const store = Buffer.allocUnsafe(slots * pool.maxLen);
  const views: Buffer[] = new Array(slots);
  for (let i = 0; i < slots; i++) views[i] = store.subarray(i * pool.maxLen, (i + 1) * pool.maxLen);

  const sockets: dgram.Socket[] = [];
  const family = cfg.server.includes(":") ? "udp6" : "udp4";

  function onMessage(msg: Buffer): void {
    if (msg.length < 4) return;
    const id = (msg[0] << 8) | msg[1];
    const slot = id & slotMask;
    if (slot >= slots) return;
    if (epochBits > 0 && epochOf[slot] !== id >>> slotBits) return; // stale, slot recycled
    const t0 = sentAt[slot];
    if (t0 === 0) return;

    sentAt[slot] = 0;
    freeSlots[freeTop++] = slot;
    inFlight--;
    record((performance.now() - t0) * 1000, msg, 0, msg.length);
  }

  for (let i = 0; i < cfg.sockets; i++) {
    const socket = dgram.createSocket({ type: family, recvBufferSize: 1 << 21, sendBufferSize: 1 << 21 });
    await new Promise<void>((resolve, reject) => {
      socket.once("error", reject);
      socket.connect(cfg.port, cfg.server, () => {
        socket.removeListener("error", reject);
        resolve();
      });
    });
    socket.on("error", () => {
      errors++;
    });
    socket.on("message", onMessage);
    sockets.push(socket);
  }

  let rr = 0;
  let poolIdx = 0;
  const poolLen = pool.packets.length;
  paceStart = performance.now();
  paceBase = 0;

  function sendOne(nowMs: number): boolean {
    if (freeTop === 0) return false;
    const slot = freeSlots[--freeTop];

    const epoch = epochBits > 0 ? (epochOf[slot] + 1) & epochMask : 0;
    epochOf[slot] = epoch;
    const id = epochBits > 0 ? (epoch << slotBits) | slot : slot;

    const packet = pool.packets[poolIdx];
    poolIdx = poolIdx + 1 < poolLen ? poolIdx + 1 : 0;

    const view = views[slot];
    packet.copy(view, 0);
    view[0] = id >>> 8;
    view[1] = id & 0xff;

    sentAt[slot] = nowMs;
    inFlight++;
    sent++;

    // No completion callback: it would allocate a closure per query, and a failed send is
    // already visible through the socket error event and the slot timing out.
    sockets[rr].send(view, 0, packet.length);
    rr = rr + 1 < sockets.length ? rr + 1 : 0;
    return true;
  }

  const limitReached = (): boolean => cfg.count > 0 && sent >= cfg.count;

  function pump(): void {
    if (stopping) return;
    const now = performance.now(); // one clock read for the whole batch
    while (inFlight < slots) {
      if (limitReached() || Atomics.load(control, CTL.STOP) === 1) {
        stopping = true;
        return;
      }
      if (!sendOne(now)) {
        saturated++;
        return;
      }
    }
    saturated++;
  }

  function pumpPaced(): void {
    if (stopping) return;
    if (limitReached() || Atomics.load(control, CTL.STOP) === 1) {
      stopping = true;
      return;
    }
    // Pace against elapsed time rather than per-tick counts, so a coarse OS timer (Windows
    // ticks near 15 ms) does not quietly cap the rate.
    const now = performance.now();
    const target = paceBase + Math.floor((cfg.qps * (now - paceStart)) / 1000);
    while (sent < target && inFlight < slots) {
      if (limitReached()) {
        stopping = true;
        return;
      }
      if (!sendOne(now)) {
        saturated++;
        return;
      }
    }
  }

  const timers: NodeJS.Timeout[] = [];

  // Timeout sweep, bounded by the per-worker concurrency: a few hundred comparisons rather
  // than a walk over a Map holding everything in flight.
  timers.push(
    setInterval(() => {
      const cutoff = performance.now() - cfg.timeoutMs;
      for (let slot = 0; slot < slots; slot++) {
        const t0 = sentAt[slot];
        if (t0 !== 0 && t0 <= cutoff) {
          sentAt[slot] = 0;
          freeSlots[freeTop++] = slot;
          inFlight--;
          timeouts++;
        }
      }
      if (!stopping && cfg.qps === 0) pump();
    }, 100),
  );

  if (cfg.qps > 0) {
    timers.push(setInterval(pumpPaced, 2));
  } else {
    // Re-arm the send loop whenever responses have freed slots.
    timers.push(setInterval(() => { if (!stopping) pump(); }, 1));
    pump();
  }

  await new Promise<void>((resolve) => {
    const done = setInterval(() => {
      if (!stopping && Atomics.load(control, CTL.STOP) === 1) stopping = true;
      if (!stopping) return;

      if (inFlight === 0) {
        clearInterval(done);
        resolve();
        return;
      }
      // Drain grace period: once the run is over, do not wait forever for stragglers.
      drainDeadline ??= performance.now() + cfg.timeoutMs + 1000;
      if (performance.now() >= drainDeadline) {
        timeouts += inFlight;
        inFlight = 0;
        clearInterval(done);
        resolve();
      }
    }, 25);
    timers.push(done);
  });

  for (const t of timers) clearInterval(t);
  for (const s of sockets) s.close();
}

// Surfaced once per worker: a wall of connect failures usually means the client ran out of
// ephemeral ports, which is a property of the load generator, not of the server.
let connErrorReported = false;

function noteConnectError(err: NodeJS.ErrnoException): void {
  if (connErrorReported) return;
  connErrorReported = true;
  const code = err.code ?? "";
  const hint =
    code === "EADDRNOTAVAIL" || code === "EADDRINUSE" || code === "ENOBUFS"
      ? " (out of ephemeral ports on this client: wait for TIME_WAIT to drain, or widen the local port range)"
      : "";
  parentPort?.postMessage("error: TCP connect failed: " + (err.message || code) + hint);
}

// ---- TCP -----------------------------------------------------------------------------

/**
 * One persistent, pipelined connection.
 *
 * The server answers a connection's queries strictly in order, so responses are matched to
 * requests FIFO rather than by transaction id.
 */
class TcpConn {
  private sock: net.Socket | null = null;
  private readBuf = Buffer.allocUnsafe(1 << 16);
  private readEnd = 0;
  private writable = true;
  private closed = false;
  private readonly pendingAt: Float64Array;
  private head = 0;
  private tail = 0;
  private depth = 0;
  private readonly scratch: Buffer;
  private poolIdx: number;
  private backoffMs = 5;

  constructor(
    private readonly pool: Pool,
    private readonly limitReached: () => boolean,
    seed: number,
  ) {
    this.pendingAt = new Float64Array(cfg.pipeline);
    this.scratch = Buffer.allocUnsafe(cfg.pipeline * pool.maxFramedLen);
    this.poolIdx = seed % pool.framed.length;
    this.connect();
  }

  private connect(): void {
    if (this.closed || stopping) return;
    const sock = net.connect({ host: cfg.server, port: cfg.port });
    this.sock = sock;
    sock.setNoDelay(true);
    sock.on("connect", () => {
      this.writable = true;
      this.backoffMs = 5;
      this.fill();
    });
    sock.on("data", (chunk) => this.onData(chunk));
    sock.on("drain", () => {
      this.writable = true;
      this.fill();
    });
    sock.on("error", (err: NodeJS.ErrnoException) => {
      errors++;
      noteConnectError(err);
      this.reset();
    });
    sock.on("close", () => this.reset());
  }

  private reset(): void {
    if (this.sock) {
      this.sock.destroy();
      this.sock = null;
    }
    // Anything still queued on a dead connection will never be answered.
    timeouts += this.depth;
    inFlight -= this.depth;
    this.depth = 0;
    this.head = 0;
    this.tail = 0;
    this.readEnd = 0;
    if (this.closed || stopping) return;
    // Back off rather than hammering. When connects fail in bulk (a client that has run out of
    // ephemeral ports is the usual cause) a tight retry loop stalls this thread's event loop,
    // and the run stops measuring the server and starts measuring the client's failure mode.
    const delay = this.backoffMs;
    this.backoffMs = Math.min(this.backoffMs * 2, 1000);
    setTimeout(() => this.connect(), delay);
  }

  private onData(chunk: Buffer): void {
    if (this.readEnd + chunk.length > this.readBuf.length) {
      if (this.readEnd + chunk.length > this.readBuf.length * 4) {
        const bigger = Buffer.allocUnsafe((this.readEnd + chunk.length) * 2);
        this.readBuf.copy(bigger, 0, 0, this.readEnd);
        this.readBuf = bigger;
      } else {
        this.readBuf.copyWithin(0, 0, this.readEnd);
      }
    }
    chunk.copy(this.readBuf, this.readEnd);
    this.readEnd += chunk.length;

    let pos = 0;
    const now = performance.now();
    while (this.readEnd - pos >= 2) {
      const len = (this.readBuf[pos] << 8) | this.readBuf[pos + 1];
      if (this.readEnd - pos - 2 < len) break;

      if (this.depth > 0) {
        const t0 = this.pendingAt[this.head];
        this.head = this.head + 1 < cfg.pipeline ? this.head + 1 : 0;
        this.depth--;
        inFlight--;
        record((now - t0) * 1000, this.readBuf, pos + 2, len);
      }
      pos += 2 + len;
    }

    if (pos > 0) {
      this.readBuf.copyWithin(0, pos, this.readEnd);
      this.readEnd -= pos;
    }
    this.fill();
  }

  /** Tops the pipeline back up, coalescing the whole batch into a single write. */
  fill(): void {
    const sock = this.sock;
    if (!sock || !this.writable || stopping || this.closed) return;

    let off = 0;
    const now = performance.now();
    while (this.depth < cfg.pipeline) {
      if (this.limitReached() || Atomics.load(control, CTL.STOP) === 1) {
        stopping = true;
        break;
      }

      const packet = this.pool.framed[this.poolIdx];
      this.poolIdx = this.poolIdx + 1 < this.pool.framed.length ? this.poolIdx + 1 : 0;
      packet.copy(this.scratch, off);
      off += packet.length;

      this.pendingAt[this.tail] = now;
      this.tail = this.tail + 1 < cfg.pipeline ? this.tail + 1 : 0;
      this.depth++;
      inFlight++;
      sent++;
    }

    if (off === 0) {
      if (this.depth >= cfg.pipeline) saturated++;
      return;
    }
    this.writable = sock.write(this.scratch.subarray(0, off));
  }

  get outstanding(): number {
    return this.depth;
  }

  close(): void {
    this.closed = true;
    if (this.sock) {
      this.sock.destroy();
      this.sock = null;
    }
  }
}

async function runTcp(pool: Pool): Promise<void> {
  const limitReached = (): boolean => cfg.count > 0 && sent >= cfg.count;
  const conns: TcpConn[] = [];
  for (let i = 0; i < cfg.concurrency; i++) conns.push(new TcpConn(pool, limitReached, i * 7919));

  const timers: NodeJS.Timeout[] = [];
  timers.push(
    setInterval(() => {
      if (!stopping && Atomics.load(control, CTL.STOP) === 1) stopping = true;
      if (!stopping) for (const c of conns) c.fill();
    }, 20),
  );

  await new Promise<void>((resolve) => {
    const done = setInterval(() => {
      if (!stopping && Atomics.load(control, CTL.STOP) === 1) stopping = true;
      if (!stopping) return;

      let outstanding = 0;
      for (const c of conns) outstanding += c.outstanding;
      if (outstanding === 0) {
        clearInterval(done);
        resolve();
        return;
      }
      drainDeadline ??= performance.now() + cfg.timeoutMs + 1000;
      if (performance.now() >= drainDeadline) {
        timeouts += outstanding;
        inFlight -= outstanding;
        clearInterval(done);
        resolve();
      }
    }, 25);
    timers.push(done);
  });

  for (const t of timers) clearInterval(t);
  for (const c of conns) c.close();
}

// ---- entry ---------------------------------------------------------------------------

async function main(): Promise<void> {
  const pool = buildPool(cfg);
  const flusher = setInterval(flush, 100);

  // Event-loop delay is the load generator's own vital sign. When this climbs, the numbers
  // describe this client rather than the server.
  let lagAt = performance.now();
  const lagTimer = setInterval(() => {
    const now = performance.now();
    const lag = now - lagAt - 20;
    if (lag > 0) {
      if (lag > lagMax) lagMax = lag;
      lagSum += lag;
    }
    lagSamples++;
    lagAt = now;
  }, 20);

  try {
    if (cfg.transport === "udp") await runUdp(pool);
    else await runTcp(pool);
  } finally {
    clearInterval(flusher);
    clearInterval(lagTimer);
    finish();
  }
}

main().catch((err: unknown) => {
  errors++;
  finish();
  parentPort?.postMessage("error: " + (err instanceof Error ? err.message : String(err)));
});
