# syntax=docker/dockerfile:1.7

# ── Stage 1: SPA build (Node + pnpm) ───────────────────────────────────────
FROM node:22-alpine AS spa
WORKDIR /spa
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

# ── Stage 2: Rust build (cargo-chef for layer caching) ─────────────────────
FROM rust:1-slim-bookworm AS planner
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY examples ./examples
COPY replays ./replays
COPY migrations ./migrations
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1-slim-bookworm AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
COPY --from=planner /build/recipe.json recipe.json
# Cooks the dependency tree only — kept in its own layer so source edits
# don't trigger a full deps rebuild.
RUN cargo chef cook --release --recipe-path recipe.json -p server
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY examples ./examples
COPY replays ./replays
COPY migrations ./migrations
RUN cargo build --release -p server --bin rubic-server

# ── Stage 3: Runtime (slim Debian + only what we ship) ─────────────────────
FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/rubic-server /app/rubic-server
COPY --from=spa     /spa/dist                          /app/static
COPY examples       /app/examples
COPY migrations     /app/migrations

ENV RUBIC_BIND=0.0.0.0:8080 \
    RUBIC_STATIC_DIR=/app/static \
    RUBIC_MODEL=/app/examples/agent_demo.toml \
    RUBIC_DB_URL=sqlite:///tmp/rubic.db \
    RUBIC_KEY_PATH=/tmp/rubic.key \
    RUBIC_ALLOW_LIVE_AGENT=0 \
    RUBIC_PUBLIC_HOST=rubic.fly.dev \
    RUST_LOG=info

EXPOSE 8080
CMD ["/app/rubic-server"]
