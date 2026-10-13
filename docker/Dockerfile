# Compatibility build entry; keep aligned with the root Dockerfile.
# syntax=docker/dockerfile:1
FROM rust:1.88.0-slim-bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml ./
COPY crates/ crates/
RUN cargo build --release --locked -j 1 -p dts-server --features postgres,engine-ape,auth-redis

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/dts-server /usr/local/bin/dts-server
RUN mkdir -p /app/data /opt/ape
VOLUME /app/data
ENV SERVER_HOST=0.0.0.0 SERVER_PORT=8080
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1
CMD ["dts-server"]
