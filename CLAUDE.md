# DTS Platform — AI Coding Guide

## Project Overview

DTS Platform is a **lightweight data middle platform** built in Rust + React, serving as a unified portal for data integration, scheduling, analytics, and AI. It orchestrates four core components: DTS Platform (self-built portal), jiascheduler (distributed scheduler), SwanLake (analytics engine), and DeerFlow (AI agent).

## Architecture

```
Frontend (React SPA) → REST API / WebSocket → dts-server (actix-web)
                                                    │
                          ┌─────────────────────────┼─────────────────────────┐
                          │                         │                         │
                   ape-dts (CLI)           jiascheduler (HTTP)        DeerFlow (HTTP)
                   集成引擎                    调度引擎                   AI 智能体
                                                                          │
                                                                   SwanLake (gRPC)
                                                                    分析引擎
```

### Crate Map (9 crates)

| Crate | Layer | Purpose |
|-------|-------|---------|
| `dts-core` | Domain | Shared types + serde roundtrip tests |
| `dts-backend` | Abstraction | Repository traits + domain types (DB-agnostic) |
| `dts-backend-rdbms` | Infrastructure | Shared RDBMS: error mapping, password utils |
| `dts-backend-sqlite` | Infrastructure | SQLite repo implementations (dev default) |
| `dts-backend-postgres` | Infrastructure | PostgreSQL repo implementations (production) |
| `dts-engine` | Domain | Engine trait definition |
| `dts-engine-ape` | Infrastructure | ape-dts engine adapter (CLI subprocess) |
| `dts-scheduler-client` | Infrastructure | jiascheduler HTTP client |
| `dts-server` | Application | actix-web server, API, services, middleware |

### Layer Architecture (inside dts-server)

```
api/          → Thin handlers: param extraction, auth check, call service
services/     → Business logic: orchestration, validation, transactions
repo/         → Repository traits in dts-backend crate
db/           → Runtime backend dispatch (sqlite/postgres via feature flags)
middleware/   → JWT auth extraction + tenant isolation
ws/           → WebSocket hub for real-time task progress/logs
models/       → Request/response types, error types
```

## Commands

```bash
# Build (default: SQLite only)
cargo build

# Build with PostgreSQL
cargo build --features postgres

# Test (SQLite :memory:)
cargo test

# Test specific crate
cargo test -p dts-server

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt --all

# Coverage
cargo llvm-cov --fail-under-lines 80

# Frontend
cd web && npm run dev       # Dev server
cd web && npm run build     # Production build
cd web && npm test          # Frontend tests

# Start server
DATABASE_BACKEND=sqlite DATABASE_URL="sqlite:./data/dts.db?mode=rwc" cargo run -p dts-server
```

## Feature Flags

| Flag | Default | Effect |
|------|---------|--------|
| `sqlite` | ✅ on | Compile SQLite backend |
| `postgres` | off | Compile PostgreSQL backend |

## Code Conventions

- **Immutability**: Always create new objects, never mutate
- **Error handling**: `thiserror` for libraries, `anyhow` for app, never silent swallow
- **File size**: < 800 lines target, < 400 lines ideal
- **Function size**: < 50 lines target
- **Nesting**: < 4 levels deep
- **Unsafe**: Denied (`#![deny(unsafe_code)]`)
- **Unwrap/expect**: Denied in production (`clippy::unwrap_used = "deny"`)
- **Tests**: 80%+ coverage, AAA pattern (Arrange-Act-Assert), `#[cfg(test)]` mod + `tests/` integration tests
- **DB access**: Always through Repository traits — never raw SQL in services
- **API handlers**: Thin — just parse params, call service, serialize response

## Tech Stack

| Layer | Tech |
|-------|------|
| Backend | Rust, actix-web 4, sqlx 0.8, tokio |
| Database dev | SQLite (zero-config, file/memory) |
| Database prod | PostgreSQL |
| Auth | JWT + argon2 password hashing |
| Frontend | Vite + React + TypeScript + Tailwind CSS |
| State | Zustand (client), TanStack Query (server) |
| UI | Custom components + CSS variables |
| Engine | ape-dts (git dep, CLI subprocess mode) |

## Current Branch State

Branch: `feat/supabase-postgresql` (multi-backend: SQLite + PostgreSQL)
Phase 2 complete (engine integration + TaskScheduler + TaskRunRepo).
Multi-backend architecture active with runtime dispatch.
Frontend i18n (zh-CN/en-US) + theme system (light/dark) complete.

## Related Docs

- `docs/architecture/overview.md` — Architecture overview + Phase roadmap
- `docs/architecture/data-model.md` — Database schema
- `docs/architecture/database-backend.md` — Multi-backend design (traits, feature flags)
- `docs/architecture/engine-integration.md` — Engine integration design
- `README.md` — Project overview + evolution roadmap
