ALTER TABLE api_keys ADD COLUMN allowed_ips TEXT;

CREATE TABLE IF NOT EXISTS idempotency_keys (
    key TEXT NOT NULL,
    api_key_id TEXT NOT NULL,
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    status_code INTEGER NOT NULL,
    response_body TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (key, api_key_id)
);

CREATE TABLE IF NOT EXISTS managed_databases (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    engine TEXT NOT NULL,
    container_id TEXT,
    host TEXT NOT NULL DEFAULT '127.0.0.1',
    port INTEGER NOT NULL,
    database_name TEXT NOT NULL,
    username TEXT NOT NULL,
    password_encrypted BLOB NOT NULL,
    status TEXT NOT NULL DEFAULT 'creating',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS deploy_hooks (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    events TEXT NOT NULL DEFAULT 'success,failure',
    secret TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

ALTER TABLE deployments ADD COLUMN claim_token TEXT;
ALTER TABLE deployments ADD COLUMN claimed_by TEXT;
CREATE INDEX IF NOT EXISTS idx_deployments_claim ON deployments(claim_token);
