# ── Builder Stage ──
FROM rust:1.88.0-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy only Cargo files first for caching
COPY Cargo.toml Cargo.lock ./
COPY crates/dts-server/Cargo.toml crates/dts-server/
COPY rust-toolchain.toml rustfmt.toml ./

# Create dummy source for dependency pre-build
RUN mkdir -p crates/dts-server/src && \
    echo "fn main() {}" > crates/dts-server/src/main.rs && \
    echo "" > crates/dts-server/src/lib.rs

# Build dependencies (will be cached)
RUN cargo build --release -p dts-server && \
    rm -rf crates/dts-server/src target/release/deps/dts_server*

# Copy actual source
COPY crates/ crates/

# Build release binary
RUN cargo build --release -p dts-server

# ── Runtime Stage ──
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary
COPY --from=builder /app/target/release/dts-server /app/dts-server

# Copy migrations
COPY --from=builder /app/crates/dts-server/migrations /app/migrations

# Fly.io provides PORT env var
ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8080

EXPOSE 8080

CMD ["/app/dts-server"]
