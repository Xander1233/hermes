# Linux deployment image for the Hermes DNS server.
# (Local development and testing happen on the host — Windows or otherwise — via `cargo`.)

# --- Build stage -------------------------------------------------------------
FROM rust:1.93-slim-bookworm AS builder

# aws-lc-sys (pulled in by reqwest's rustls TLS, used to reach the zone-sync control plane)
# compiles C and needs cmake/perl, which the slim image does not ship.
RUN apt-get update && apt-get install -y --no-install-recommends \
        cmake perl pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/hermes
COPY . .
RUN cargo build --release --bin hermes

# --- Runtime stage -----------------------------------------------------------
FROM debian:bookworm-slim

# Run as an unprivileged user.
RUN useradd --system --no-create-home --user-group hermes

COPY --from=builder /usr/src/hermes/target/release/hermes /usr/local/bin/hermes
COPY config /etc/hermes

USER hermes
WORKDIR /etc/hermes

# The shipped config binds 127.0.0.1:5353. For container networking, override the bind
# address in server.toml (e.g. address = "0.0.0.0"). Port 53 needs CAP_NET_BIND_SERVICE.
EXPOSE 5353/udp 5353/tcp

ENTRYPOINT ["hermes"]
CMD ["/etc/hermes/server.toml", "/etc/hermes/zones.toml"]
