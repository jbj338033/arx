# arx

Agent-first self-hosted deployment platform. Deploy containers to your own server with a single command.

## Features

- **Image Deploy** — deploy any Docker image with zero config
- **Git Webhooks** — auto-deploy on push (GitHub, Gitea)
- **Custom Domains** — automatic TLS via Caddy reverse proxy
- **Managed Databases** — provision PostgreSQL/MySQL containers per project
- **Encrypted Env Vars** — AES-256-GCM encrypted environment variables
- **API Keys & Scopes** — granular access control (admin/deploy/read)
- **Deploy Hooks** — webhook notifications with HMAC signatures
- **Audit Logs** — track all API actions
- **Rollback** — instant rollback to previous deployments
- **MCP Server** — AI agent integration via Model Context Protocol
- **CLI & REST API** — full control from terminal or HTTP

## Quick Start

### Install (Linux x86_64)

```bash
curl -fsSL https://raw.githubusercontent.com/arx-deploy/arx/main/install.sh | sudo sh
```

### Get your admin key

```bash
sudo arx admin initial-password
```

### Login from your local machine

```bash
arx login --url http://your-server:8443 --key <admin-key>
```

### Deploy

```bash
arx project create my-app
arx deploy --project my-app --image nginx:latest
```

## CLI

```
arx server              Start the arx server
arx deploy              Deploy a project
arx project create|list|info|delete
arx env set|get|delete  Manage environment variables
arx domain add|list|remove
arx logs <project>      View deployment logs
arx rollback <project>  Rollback to previous deployment
arx db create|list|delete   Manage project databases
arx auth create|list|revoke Manage API keys
arx audit               View audit logs
arx diff                Compare two deployments
arx status <project>    Get project status
arx login               Authenticate with a server
arx admin               Server admin commands
arx mcp                 Start MCP server (stdio)
```

## REST API

All endpoints are under `/api/v1`. Authentication via `Authorization: Bearer arx_sk_...` header.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check |
| POST | `/projects` | Create project |
| GET | `/projects` | List projects |
| GET | `/projects/{id}` | Get project |
| PATCH | `/projects/{id}` | Update project |
| DELETE | `/projects/{id}` | Delete project |
| POST | `/projects/{id}/deployments` | Create deployment |
| GET | `/projects/{id}/deployments` | List deployments |
| GET | `/projects/{id}/deployments/{did}` | Get deployment |
| GET | `/projects/{id}/deployments/{did}/logs` | Stream logs (SSE) |
| POST | `/projects/{id}/deployments/{did}/promote` | Promote deployment |
| POST | `/projects/{id}/deployments/{did}/rollback` | Rollback deployment |
| PUT | `/projects/{id}/env` | Set env vars |
| GET | `/projects/{id}/env` | List env var keys |
| DELETE | `/projects/{id}/env/{key}` | Delete env var |
| POST | `/projects/{id}/domains` | Add domain |
| GET | `/projects/{id}/domains` | List domains |
| DELETE | `/projects/{id}/domains/{did}` | Delete domain |
| POST | `/projects/{id}/databases` | Create database |
| GET | `/projects/{id}/databases` | List databases |
| DELETE | `/projects/{id}/databases/{did}` | Delete database |
| POST | `/projects/{id}/hooks` | Create deploy hook |
| GET | `/projects/{id}/hooks` | List deploy hooks |
| DELETE | `/projects/{id}/hooks/{hid}` | Delete deploy hook |
| GET | `/projects/{id}/diff?from=&to=` | Compare deployments |
| POST | `/auth/keys` | Create API key |
| GET | `/auth/keys` | List API keys |
| DELETE | `/auth/keys/{id}` | Revoke API key |
| GET | `/audit` | List audit logs |
| POST | `/claim/{token}` | Claim deployment |
| POST | `/webhooks/github` | GitHub webhook |
| POST | `/webhooks/gitea` | Gitea webhook |

## Architecture

```
┌─────────┐     ┌──────────┐     ┌────────────┐     ┌───────────┐
│ CLI/API  │────▶│  arx-api │────▶│ arx-engine │────▶│  Docker   │
│          │     │  (axum)  │     │ (bollard)  │     │           │
└─────────┘     └────┬─────┘     └────────────┘     └───────────┘
                     │
                ┌────┴─────┐     ┌────────────┐
                │ arx-core │     │ arx-proxy  │────▶ Caddy
                │ (sqlite) │     │            │
                └──────────┘     └────────────┘
```

- **arx-core** — data models, SQLite database, config, encryption, HTTP client
- **arx-api** — REST API server, authentication, rate limiting, idempotency
- **arx-engine** — Docker container lifecycle, image builds, health verification
- **arx-proxy** — Caddy reverse proxy integration for custom domains + TLS

## Development

### Prerequisites

- Rust 1.75+
- Docker
- Caddy (optional, for custom domains)

### Build

```bash
cargo build
```

### Run locally

```bash
# Start server
cargo run -- server --db /tmp/arx.db

# In another terminal
cargo run -- admin initial-password
cargo run -- login --url http://localhost:8443 --key <key>
```

## License

MIT
