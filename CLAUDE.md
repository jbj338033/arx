# Arx

Agent-first self-hosted deployment platform.

## Build

```bash
cargo check            # type check
cargo clippy           # lint
cargo fmt --check      # format check
cargo test             # run tests
cargo build --release  # release build
```

Binary: `target/release/arx`

## Architecture

Cargo workspace with 4 crates:

- `arx-core` — models, DB (SQLite/sqlx), config, crypto (AES-256-GCM), error types, client
- `arx-api` — axum HTTP server, REST routes, auth middleware, rate limiting, idempotency, MCP server, webhook handlers
- `arx-engine` — Docker container management (bollard), image build, deploy orchestration, health verification, database provisioning
- `arx-proxy` — Caddy reverse proxy integration via admin API

Root `src/main.rs` — CLI (clap): server, client commands, admin, login, MCP

## Code Conventions

- Error messages: lowercase, no period
- `as` type assertion 금지 — 올바른 타입 사용
- KISS: 불필요한 주석/추상화 없이 깔끔하게
- `///` doc comments는 clap help용만 사용
- `pub` 최소한으로

## Key Patterns

- API keys: `arx_sk_` prefix, SHA-256 hash 저장, scope-based access control (admin > deploy > read)
- Env vars: AES-256-GCM 암호화, master key는 `/etc/arx/master.key`
- Deploy flow: pending → deploying → verifying → live/failed
- Caddy proxy: dynamic route management via admin API (`CADDY_ADMIN_URL`)

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `RUST_LOG` | Log level | `arx=info` |
| `CADDY_ADMIN_URL` | Caddy admin API URL | (optional) |
| `ARX_DB_PATH` | SQLite DB path | `/var/lib/arx/arx.db` |
| `ARX_MASTER_KEY` | Master key hex (alternative to file) | — |
| `ARX_OUTPUT` | Output format (`json`) | auto-detect |

## Database

SQLite with WAL mode. Migrations in `migrations/`. Run automatically on startup via `sqlx::migrate!`.

## Git

- Commit messages: English, lowercase, one line
- Format: `type: description`
- Types: feat, fix, refactor, style, docs, test, chore
