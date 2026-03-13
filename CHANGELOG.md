# Changelog

## v0.1.0

Initial release.

### Features

- Image-based deployment with Docker
- Git webhook integration (GitHub, Gitea)
- Custom domain management with automatic TLS via Caddy
- Managed database provisioning (PostgreSQL, MySQL)
- AES-256-GCM encrypted environment variables
- API key authentication with scoped access control (admin/deploy/read)
- IP allowlist per API key
- Deploy webhook notifications with HMAC signatures
- Audit logging for all API actions
- Deployment rollback and promotion
- Deployment diff comparison
- Claimable deployments with claim tokens
- Request idempotency support
- Rate limiting
- SSE log streaming
- MCP server for AI agent integration
- CLI with human and JSON output modes
- Health check verification on deploy
- `arx doctor` server diagnostics
