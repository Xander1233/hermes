# hermes-dns-client

A tiny interactive DNS query client for the Hermes DNS server. It builds the DNS query on the
wire by hand (no DNS library), sends it over UDP or TCP, and prints the answer both
**human-readable** (dig-style) and as **raw bytes** (hex + ASCII), for the query and the response.

## Setup

Requires Node.js 18+.

```sh
cd tools/dns-client
npm install
```

## Usage

### Interactive (prompts)

```sh
npm start
```

You'll be prompted for the name, record type, server, port, and transport. Press Enter to
accept the default shown in `[brackets]`. Defaults target a local Hermes: `127.0.0.1:53`, UDP.

### One-shot (arguments)

```
npm start -- <name> [type] [server] [port] [transport]
```

```sh
npm start -- www.example.com A
npm start -- alias.example.com A 127.0.0.1 53 tcp
npm start -- example.com MX
```

Supported types: `A, NS, CNAME, SOA, PTR, MX, TXT, AAAA, SRV, CAA, ANY`.

## Example

```
;; Sending 33-byte UDP query for www.example.com A to 127.0.0.1:53

;; ->>HEADER<<- opcode: QUERY, status: NOERROR, id: 8890
;; flags: qr aa rd;  QUERY: 1, ANSWER: 1, AUTHORITY: 0, ADDITIONAL: 0

;; QUESTION SECTION:
;www.example.com.        IN  A

;; ANSWER SECTION:
www.example.com.  300  IN  A  93.184.216.34

;; Query time: 8.3 ms

;; QUERY BYTES (33):
0000  22 ba 01 00 00 01 00 00  00 00 00 00 03 77 77 77   |"............www|
...

;; RESPONSE BYTES (49):
0000  22 ba 85 00 00 01 00 01  00 00 00 00 03 77 77 77   |"............www|
...
```

## Benchmark / stress test

`npm run bench` fires queries at the server and reports throughput and latency percentiles.

It is built so that the generator is not the thing being measured. Load is spread over worker
threads, queries are encoded once into a pool at startup rather than per query, in-flight state
lives in flat typed arrays instead of a Map, and latencies go into a bucketed histogram instead
of a sampled-and-sorted array. Every run ends with a **load generator health** block, so a
client-bound result is visible instead of being mistaken for a slow server.

```sh
npm run bench                                     # 10s UDP, 4 workers, 1024 in flight
npm run bench -- --duration 30 --concurrency 4096
npm run bench -- --qps 50000 --duration 30        # paced instead of flat out
npm run bench -- --transport tcp --concurrency 64 --pipeline 64
npm run bench -- --count 200000                   # exactly 200k measured queries
npm run bench -- --help                           # all options
```

Key options: `--server`, `--port`, `--transport udp|tcp`, `--duration <sec>` / `--count <n>`,
`--warmup <sec>`, `--workers <n>`, `--concurrency <n>`, `--sockets <n>` (UDP),
`--pipeline <n>` (TCP), `--qps <n>` (UDP), `--timeout <ms>`, `--domain`, `--names a,b,c`,
`--types A,AAAA,MX`, `--random-ratio <0..1>`. By default 30% of queries hit a random subdomain
(to exercise NXDOMAIN); the rest hit the shipped `example.com` zone names. The first two
seconds are warmup and are thrown away, so JIT warmup does not land in the percentiles.

UDP spreads traffic over several connected sockets per worker (`--sockets`). That is not
cosmetic: the server binds its listeners with `SO_REUSEPORT` and the kernel picks one by
hashing the 4-tuple, so a single client socket is one flow and would pin an entire run to one
of the server's receive loops. TCP keeps its connections open and pipelines queries down them
(RFC 7766) rather than paying a handshake per query. That is not only faster, it stops the
generator burning one ephemeral port per query: a connect-per-query client drains the whole
local port range in seconds and then spends the run measuring its own connect failures. A
sustained TCP run here holds well under a hundred sockets in `TIME_WAIT`.

Example output:

```
=== Benchmark results ===
transport   : udp
target      : 127.0.0.1:53
duration    : 8.04 s
sent        : 1180318
completed   : 1180318
timeouts    : 0
errors      : 0
throughput  : 146818 q/s
latency (ms): min 4.86  avg 6.59  p50 6.53  p90 7.30  p99 7.81  p99.9 8.45  max 9.72
rcodes      : NOERROR 1180318

--- load generator health ---
workers     : 4  sockets 16
event-loop  : max lag 7.0 ms, avg 0.07 ms
in-flight   : avg 872 of 1024 (85% full)
```

An `avg 0.07 ms` event loop and an in-flight window that never pinned mean this run is the
server's number, not the client's. When something does look off, the report says so in a
`notes:` block rather than leaving you to infer it.

### Reading the health block

- **event-loop lag** is the generator's own stall time. Single-digit milliseconds means the
  latencies above are the server's. Tens of milliseconds means they include time queued inside
  this process, and the run needs more `--workers`.
- **in-flight fill** says which side set the pace. Near 100% with a healthy client means the
  server was saturated, so throughput is its service rate and the latency is mostly queueing
  (Little's law: `latency ≈ concurrency / throughput`). Raising `--concurrency` from there only
  raises latency. Well under the ceiling means the server kept up and the figure is a
  client-side send rate.

Press Ctrl-C to stop early and still get the report.

## Notes

- The port defaults to `53` to match the shipped `config/server.toml`.
- TCP uses the standard 2-byte length prefix (RFC 1035 §4.2.2).
- `npm run typecheck` type-checks the sources with `tsc`.
