# DTS Platform — Docker Build
#
# Multi-stage build for dts-server
#
# Build:
#   docker build -f docker/Dockerfile -t dts-server .
#
# Run (standalone):
#   docker run --rm -p 8080:8080 dts-server

# ── Stage 1: Build ────────────────────────────────────
FROM rust:1.89-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace manifests first (dependency caching)
COPY Cargo.toml Cargo.lock* ./
COPY crates/dts-core/Cargo.toml crates/dts-core/
COPY crates/dts-backend/Cargo.toml crates/dts-backend/
COPY crates/dts-backend-rdbms/Cargo.toml crates/dts-backend-rdbms/
COPY crates/dts-backend-sqlite/Cargo.toml crates/dts-backend-sqlite/
COPY crates/dts-backend-postgres/Cargo.toml crates/dts-backend-postgres/
COPY crates/dts-engine/Cargo.toml crates/dts-engine/
COPY crates/dts-engine-ape/Cargo.toml crates/dts-engine-ape/
COPY crates/dts-scheduler-client/Cargo.toml crates/dts-scheduler-client/
COPY crates/dts-deerflow-client/Cargo.toml crates/dts-deerflow-client/
COPY crates/dts-server/Cargo.toml crates/dts-server/

# Create dummy lib.rs files so cargo can resolve workspace members
RUN mkdir -p crates/dts-core/src && echo "" > crates/dts-core/src/lib.rs \
    && mkdir -p crates/dts-backend/src && echo "" > crates/dts-backend/src/lib.rs \
    && mkdir -p crates/dts-backend-rdbms/src && echo "" > crates/dts-backend-rdbms/src/lib.rs \
    && mkdir -p crates/dts-backend-sqlite/src && echo "" > crates/dts-backend-sqlite/src/lib.rs \
    && mkdir -p crates/dts-backend-postgres/src && echo "" > crates/dts-backend-postgres/src/lib.rs \
    && mkdir -p crates/dts-engine/src && echo "" > crates/dts-engine/src/lib.rs \
    && mkdir -p crates/dts-engine-ape/src && echo "" > crates/dts-engine-ape/src/lib.rs \
    && mkdir -p crates/dts-scheduler-client/src && echo "" > crates/dts-scheduler-client/src/lib.rs \
    && mkdir -p crates/dts-deerflow-client/src && echo "" > crates/dts-deerflow-client/src/lib.rs \
    && mkdir -p crates/dts-server/src && echo "fn main() {}" > crates/dts-server/src/main.rs

# Build dependencies only (cached layer)
RUN cargo build --release -p dts-server --features postgres,engine-ape \
    && rm -rf crates/*/src target/release/dts-server

# Copy actual source
COPY crates/ crates/

# Build dts-server (dependencies are cached)
RUN cargo build --release -p dts-server --features postgres,engine-ape \
    && cp target/release/dts-server /usr/local/bin/dts-server

# ── Stage 2: Runtime ──────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/dts-server /usr/local/bin/dts-server

# Data directory
RUN mkdir -p /app/data
WORKDIR /app
VOLUME /app/data

EXPOSE 8080

ENV DATABASE_BACKEND=postgres
ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8080

HEALTHCHECK --interval=30s --timeout=3s --retries=3 \
    CMD curl -f http://localhost:8080/api/v1/health || exit 1

CMD ["dts-server"]
