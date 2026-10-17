// Layout of the shared memory the benchmark workers write and the main thread reads.
//
// Every worker owns its own stripe of each array, so the hot path never contends on a cache
// line and never needs an atomic. Workers accumulate into plain local variables and copy the
// totals into their stripe a few times a second; the main thread just sums the stripes.

import { BUCKETS } from "./histogram.ts";

/** Int32 counters, one stripe per worker. */
export const C = {
  SENT: 0,
  COMPLETED: 1,
  TRUNCATED: 2,
  TIMEOUTS: 3,
  ERRORS: 4,
  /** Times the send loop stopped because in-flight hit the ceiling rather than the clock. */
  SATURATED: 5,
  INFLIGHT: 6,
  DONE: 7,
  /** 16 slots, one per rcode 0..15. */
  RCODE: 8,
} as const;
export const COUNTER_SLOTS = 24;

/** Float64 accumulators, one stripe per worker. */
export const F = {
  LAT_SUM_US: 0,
  LAT_MIN_US: 1,
  LAT_MAX_US: 2,
  /** Event-loop delay: the load generator's own health, not the server's. */
  LAG_MAX_MS: 3,
  LAG_SUM_MS: 4,
  LAG_SAMPLES: 5,
} as const;
export const FLOAT_SLOTS = 8;

/** Control words every worker polls. */
export const CTL = {
  STOP: 0,
  /** 0 during warmup, 1 once samples should count. */
  RECORD: 1,
} as const;
export const CONTROL_SLOTS = 4;

export const HIST_SLOTS = BUCKETS;

export interface WorkerConfig {
  index: number;
  server: string;
  port: number;
  transport: "udp" | "tcp";
  /** Per-worker share. */
  concurrency: number;
  /** Per-worker share; 0 means flat out. */
  qps: number;
  /** Per-worker share; 0 means run until stopped. */
  count: number;
  timeoutMs: number;
  sockets: number;
  pipeline: number;
  poolSize: number;
  seed: number;
  domain: string;
  names: string[];
  types: number[];
  randomRatio: number;
}

export interface WorkerInit {
  cfg: WorkerConfig;
  counters: SharedArrayBuffer;
  floats: SharedArrayBuffer;
  hist: SharedArrayBuffer;
  control: SharedArrayBuffer;
}
