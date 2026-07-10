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

`npm run bench` fires many queries per second with randomized names and record types, then
reports throughput and latency percentiles. UDP mode multiplexes many in-flight queries over
a single socket (matched by transaction ID), so it can push tens of thousands of q/s.

```sh
npm run bench                                   # 10s, UDP, flat-out
npm run bench -- --duration 15 --concurrency 300
npm run bench -- --qps 5000 --duration 30       # rate-limited to 5000 q/s
npm run bench -- --count 100000                 # send exactly 100k queries
npm run bench -- --transport tcp --concurrency 20
npm run bench -- --help                         # all options
```

Key options: `--server`, `--port`, `--transport udp|tcp`, `--duration <sec>` / `--count <n>`,
`--concurrency <n>`, `--qps <n>` (UDP), `--timeout <ms>`, `--domain`, `--names a,b,c`,
`--types A,AAAA,MX`, `--random-ratio <0..1>`. By default 30% of queries hit a random subdomain
(to exercise NXDOMAIN); the rest hit the shipped `example.com` zone names.

Example output:

```
=== Benchmark results ===
transport   : udp
target      : 127.0.0.1:5353
duration    : 5.00 s
sent        : 224393
completed   : 224393
timeouts    : 0
errors      : 0
throughput  : 44866 q/s
latency (ms): min 0.92  avg 4.45  p50 4.18  p90 5.64  p99 8.03  max 10.79
rcodes      : NOERROR 157307, NXDOMAIN 67086
```

Press Ctrl-C to stop early and still get the report.

## Notes

- The port defaults to `53` to match the shipped `config/server.toml`.
- TCP uses the standard 2-byte length prefix (RFC 1035 §4.2.2).
- `npm run typecheck` type-checks the sources with `tsc`.
