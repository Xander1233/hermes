// Fixed-bucket latency histogram, HdrHistogram style.
//
// Recording a sample is O(1) with no allocation, no RNG and no sorting. That matters here:
// the previous reservoir sampler called Math.random() on every completed query and sorted up
// to 200k floats when the run ended, which is work the load generator should not be doing
// while it is supposed to be saturating a server.
//
// 64 sub-buckets per octave gives roughly 1.5% resolution. Counts live in a Uint32Array, so
// several workers' histograms merge by adding them element-wise.

export const SUB_BITS = 6;
export const SUB_COUNT = 1 << SUB_BITS;
export const MAX_OCTAVE = 26; // ~134 s expressed in microseconds
export const BUCKETS = (MAX_OCTAVE - SUB_BITS + 2) * SUB_COUNT;

/** Bucket index for a latency given in whole microseconds. */
export function bucketOf(micros: number): number {
  const v = micros > 0 ? micros | 0 : 0;
  if (v < SUB_COUNT) return v;

  const octave = 31 - Math.clz32(v);
  if (octave > MAX_OCTAVE) return BUCKETS - 1;

  return ((octave - SUB_BITS + 1) << SUB_BITS) + ((v >>> (octave - SUB_BITS)) - SUB_COUNT);
}

/** Lowest latency (µs) that falls into `idx`. */
export function bucketValue(idx: number): number {
  if (idx < SUB_COUNT) return idx;

  const g = (idx >>> SUB_BITS) - 1;
  return ((idx & (SUB_COUNT - 1)) + SUB_COUNT) * 2 ** g;
}

export function totalCount(hist: Uint32Array): number {
  let total = 0;
  for (let i = 0; i < hist.length; i++) total += hist[i];
  return total;
}

/** Percentile in microseconds. `total` is the sample count the histogram holds. */
export function percentileUs(hist: Uint32Array, total: number, p: number): number {
  if (total === 0) return 0;

  const target = Math.ceil((p / 100) * total);
  let seen = 0;
  for (let i = 0; i < hist.length; i++) {
    seen += hist[i];
    if (seen >= target) return bucketValue(i);
  }
  return bucketValue(hist.length - 1);
}
