use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::error::Error;
use crate::model::*;

pub async fn connect(db_path: &str) -> Result<SqlitePool, Error> {
    let options = SqliteConnectOptions::from_str(db_path)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .map_err(|e| Error::Internal(format!("migration failed: {e}")))?;

    Ok(pool)
}

pub async fn create_project(pool: &SqlitePool, project: &Project) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO projects (id, name, repo_url, default_branch, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&project.id)
    .bind(&project.name)
    .bind(&project.repo_url)
    .bind(&project.default_branch)
    .bind(dt_str(project.created_at))
    .bind(dt_str(project.updated_at))
    .execute(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.message().contains("UNIQUE") => {
            Error::ProjectAlreadyExists(project.name.clone())
        }
        _ => Error::Database(e),
    })?;
    Ok(())
}

pub async fn get_project(pool: &SqlitePool, id: &str) -> Result<Project, Error> {
    sqlx::query_as::<_, ProjectRow>(
        "SELECT id, name, repo_url, default_branch, production_deployment_id, created_at, updated_at
         FROM projects WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .map(Into::into)
    .ok_or_else(|| Error::ProjectNotFound(id.into()))
}

pub async fn get_project_by_name(pool: &SqlitePool, name: &str) -> Result<Project, Error> {
    sqlx::query_as::<_, ProjectRow>(
        "SELECT id, name, repo_url, default_branch, production_deployment_id, created_at, updated_at
         FROM projects WHERE name = ?",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?
    .map(Into::into)
    .ok_or_else(|| Error::ProjectNotFound(name.into()))
}

pub async fn list_projects(pool: &SqlitePool) -> Result<Vec<Project>, Error> {
    let rows = sqlx::query_as::<_, ProjectRow>(
        "SELECT id, name, repo_url, default_branch, production_deployment_id, created_at, updated_at
         FROM projects ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn delete_project(pool: &SqlitePool, id: &str) -> Result<(), Error> {
    let result = sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::ProjectNotFound(id.into()));
    }
    Ok(())
}

pub async fn update_project(
    pool: &SqlitePool,
    id: &str,
    name: Option<&str>,
    repo_url: Option<&str>,
    default_branch: Option<&str>,
) -> Result<Project, Error> {
    if let Some(n) = name {
        sqlx::query("UPDATE projects SET name = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(n)
            .bind(id)
            .execute(pool)
            .await?;
    }
    if let Some(r) = repo_url {
        sqlx::query("UPDATE projects SET repo_url = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(r)
            .bind(id)
            .execute(pool)
            .await?;
    }
    if let Some(b) = default_branch {
        sqlx::query(
            "UPDATE projects SET default_branch = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(b)
        .bind(id)
        .execute(pool)
        .await?;
    }
    get_project(pool, id).await
}

pub async fn update_project_production(
    pool: &SqlitePool,
    project_id: &str,
    deployment_id: &str,
) -> Result<(), Error> {
    sqlx::query(
        "UPDATE projects SET production_deployment_id = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(deployment_id)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn create_deployment(pool: &SqlitePool, deployment: &Deployment) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO deployments (id, project_id, status, source, git_ref, git_sha, image_ref, log_path, claim_token, claimed_by, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&deployment.id)
    .bind(&deployment.project_id)
    .bind(deployment.status.as_str())
    .bind(deployment.source.as_str())
    .bind(&deployment.git_ref)
    .bind(&deployment.git_sha)
    .bind(&deployment.image_ref)
    .bind(&deployment.log_path)
    .bind(&deployment.claim_token)
    .bind(&deployment.claimed_by)
    .bind(dt_str(deployment.created_at))
    .bind(dt_str(deployment.updated_at))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_deployment(pool: &SqlitePool, id: &str) -> Result<Deployment, Error> {
    sqlx::query_as::<_, DeploymentRow>(
        "SELECT id, project_id, status, source, git_ref, git_sha, image_ref, container_id, url, verification_result, log_path, claim_token, claimed_by, created_at, updated_at
         FROM deployments WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .map(TryInto::try_into)
    .transpose()?
    .ok_or_else(|| Error::DeploymentNotFound(id.into()))
}

pub async fn list_deployments(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<Vec<Deployment>, Error> {
    let rows = sqlx::query_as::<_, DeploymentRow>(
        "SELECT id, project_id, status, source, git_ref, git_sha, image_ref, container_id, url, verification_result, log_path, claim_token, claimed_by, created_at, updated_at
         FROM deployments WHERE project_id = ? ORDER BY created_at DESC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(TryInto::try_into).collect()
}

pub async fn update_deployment_status(
    pool: &SqlitePool,
    id: &str,
    status: DeploymentStatus,
) -> Result<(), Error> {
    sqlx::query("UPDATE deployments SET status = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(status.as_str())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_deployment_container(
    pool: &SqlitePool,
    id: &str,
    container_id: &str,
    url: &str,
) -> Result<(), Error> {
    sqlx::query(
        "UPDATE deployments SET container_id = ?, url = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(container_id)
    .bind(url)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_deployment_verification(
    pool: &SqlitePool,
    id: &str,
    result: &serde_json::Value,
) -> Result<(), Error> {
    sqlx::query(
        "UPDATE deployments SET verification_result = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(result.to_string())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn create_api_key(pool: &SqlitePool, key: &ApiKey) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO api_keys (id, name, key_hash, key_prefix, scope, allowed_ips, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&key.id)
    .bind(&key.name)
    .bind(&key.key_hash)
    .bind(&key.key_prefix)
    .bind(key.scope.as_str())
    .bind(&key.allowed_ips)
    .bind(key.expires_at.map(dt_str))
    .bind(dt_str(key.created_at))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_api_key_by_hash(pool: &SqlitePool, hash: &str) -> Result<ApiKey, Error> {
    sqlx::query_as::<_, ApiKeyRow>(
        "SELECT id, name, key_hash, key_prefix, scope, allowed_ips, expires_at, last_used_at, revoked_at, created_at
         FROM api_keys WHERE key_hash = ?",
    )
    .bind(hash)
    .fetch_optional(pool)
    .await?
    .map(Into::into)
    .ok_or(Error::InvalidApiKey)
}

pub async fn list_api_keys(pool: &SqlitePool) -> Result<Vec<ApiKey>, Error> {
    let rows = sqlx::query_as::<_, ApiKeyRow>(
        "SELECT id, name, key_hash, key_prefix, scope, allowed_ips, expires_at, last_used_at, revoked_at, created_at
         FROM api_keys WHERE revoked_at IS NULL ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn revoke_api_key(pool: &SqlitePool, id: &str) -> Result<(), Error> {
    sqlx::query("UPDATE api_keys SET revoked_at = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn touch_api_key(pool: &SqlitePool, id: &str) -> Result<(), Error> {
    sqlx::query("UPDATE api_keys SET last_used_at = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn create_audit_log(pool: &SqlitePool, log: &AuditLog) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO audit_logs (id, api_key_id, action, resource, ip, timestamp)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&log.id)
    .bind(&log.api_key_id)
    .bind(&log.action)
    .bind(&log.resource)
    .bind(&log.ip)
    .bind(dt_str(log.timestamp))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_audit_logs(
    pool: &SqlitePool,
    key_id: Option<&str>,
    action: Option<&str>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    limit: i64,
) -> Result<Vec<AuditLog>, Error> {
    let mut query = String::from(
        "SELECT id, api_key_id, action, resource, ip, timestamp FROM audit_logs WHERE 1=1",
    );
    if key_id.is_some() {
        query.push_str(" AND api_key_id = ?");
    }
    if action.is_some() {
        query.push_str(" AND action = ?");
    }
    if since.is_some() {
        query.push_str(" AND timestamp >= ?");
    }
    query.push_str(" ORDER BY timestamp DESC LIMIT ?");

    let mut q = sqlx::query_as::<_, AuditLogRow>(&query);
    if let Some(kid) = key_id {
        q = q.bind(kid);
    }
    if let Some(act) = action {
        q = q.bind(act);
    }
    if let Some(s) = since {
        q = q.bind(dt_str(s));
    }
    q = q.bind(limit);

    let rows = q.fetch_all(pool).await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn create_domain(pool: &SqlitePool, domain: &Domain) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO domains (id, project_id, domain, is_verified, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&domain.id)
    .bind(&domain.project_id)
    .bind(&domain.domain)
    .bind(domain.is_verified)
    .bind(dt_str(domain.created_at))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_domains(pool: &SqlitePool, project_id: &str) -> Result<Vec<Domain>, Error> {
    let rows = sqlx::query_as::<_, DomainRow>(
        "SELECT id, project_id, domain, is_verified, created_at FROM domains WHERE project_id = ? ORDER BY created_at",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn get_domain(pool: &SqlitePool, id: &str) -> Result<Domain, Error> {
    sqlx::query_as::<_, DomainRow>(
        "SELECT id, project_id, domain, is_verified, created_at FROM domains WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .map(Into::into)
    .ok_or_else(|| Error::Internal(format!("domain not found: {id}")))
}

pub async fn delete_domain(pool: &SqlitePool, id: &str) -> Result<(), Error> {
    sqlx::query("DELETE FROM domains WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_env_var(
    pool: &SqlitePool,
    project_id: &str,
    key: &str,
    encrypted_value: &[u8],
) -> Result<(), Error> {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO env_vars (id, project_id, key, encrypted_value)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(project_id, environment, key) DO UPDATE SET encrypted_value = excluded.encrypted_value",
    )
    .bind(&id)
    .bind(project_id)
    .bind(key)
    .bind(encrypted_value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_env_vars(pool: &SqlitePool, project_id: &str) -> Result<Vec<EnvVar>, Error> {
    let rows = sqlx::query_as::<_, EnvVarRow>(
        "SELECT id, project_id, environment, key, encrypted_value FROM env_vars WHERE project_id = ? ORDER BY key",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn delete_env_var(pool: &SqlitePool, project_id: &str, key: &str) -> Result<(), Error> {
    let result = sqlx::query("DELETE FROM env_vars WHERE project_id = ? AND key = ?")
        .bind(project_id)
        .bind(key)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::Internal(format!("env var not found: {key}")));
    }
    Ok(())
}

pub async fn get_project_by_repo_url(pool: &SqlitePool, repo_url: &str) -> Result<Project, Error> {
    sqlx::query_as::<_, ProjectRow>(
        "SELECT id, name, repo_url, default_branch, production_deployment_id, created_at, updated_at
         FROM projects WHERE repo_url = ?",
    )
    .bind(repo_url)
    .fetch_optional(pool)
    .await?
    .map(Into::into)
    .ok_or_else(|| Error::ProjectNotFound(repo_url.into()))
}

pub async fn get_idempotency_key(
    pool: &SqlitePool,
    key: &str,
    api_key_id: &str,
) -> Result<Option<(i64, String)>, Error> {
    let row = sqlx::query_as::<_, IdempotencyRow>(
        "SELECT status_code, response_body FROM idempotency_keys WHERE key = ? AND api_key_id = ?",
    )
    .bind(key)
    .bind(api_key_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.status_code, r.response_body)))
}

pub async fn save_idempotency_key(
    pool: &SqlitePool,
    key: &str,
    api_key_id: &str,
    method: &str,
    path: &str,
    status_code: i64,
    response_body: &str,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT OR IGNORE INTO idempotency_keys (key, api_key_id, method, path, status_code, response_body)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(key)
    .bind(api_key_id)
    .bind(method)
    .bind(path)
    .bind(status_code)
    .bind(response_body)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn cleanup_idempotency_keys(pool: &SqlitePool) -> Result<(), Error> {
    sqlx::query("DELETE FROM idempotency_keys WHERE created_at < datetime('now', '-24 hours')")
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn create_managed_database(pool: &SqlitePool, db: &ManagedDatabase) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO managed_databases (id, project_id, engine, container_id, host, port, database_name, username, password_encrypted, status, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&db.id)
    .bind(&db.project_id)
    .bind(&db.engine)
    .bind(&db.container_id)
    .bind(&db.host)
    .bind(db.port)
    .bind(&db.database_name)
    .bind(&db.username)
    .bind(&db.password_encrypted)
    .bind(&db.status)
    .bind(dt_str(db.created_at))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_managed_database(pool: &SqlitePool, id: &str) -> Result<ManagedDatabase, Error> {
    sqlx::query_as::<_, ManagedDatabaseRow>(
        "SELECT id, project_id, engine, container_id, host, port, database_name, username, password_encrypted, status, created_at
         FROM managed_databases WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .map(Into::into)
    .ok_or_else(|| Error::Internal(format!("database not found: {id}")))
}

pub async fn list_managed_databases(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<Vec<ManagedDatabase>, Error> {
    let rows = sqlx::query_as::<_, ManagedDatabaseRow>(
        "SELECT id, project_id, engine, container_id, host, port, database_name, username, password_encrypted, status, created_at
         FROM managed_databases WHERE project_id = ? ORDER BY created_at DESC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn update_managed_database_status(
    pool: &SqlitePool,
    id: &str,
    status: &str,
    container_id: Option<&str>,
) -> Result<(), Error> {
    sqlx::query("UPDATE managed_databases SET status = ?, container_id = COALESCE(?, container_id) WHERE id = ?")
        .bind(status)
        .bind(container_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_managed_database(
    pool: &SqlitePool,
    id: &str,
) -> Result<ManagedDatabase, Error> {
    let db = get_managed_database(pool, id).await?;
    sqlx::query("DELETE FROM managed_databases WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(db)
}

pub async fn create_deploy_hook(pool: &SqlitePool, hook: &DeployHook) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO deploy_hooks (id, project_id, url, events, secret, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&hook.id)
    .bind(&hook.project_id)
    .bind(&hook.url)
    .bind(&hook.events)
    .bind(&hook.secret)
    .bind(dt_str(hook.created_at))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_deploy_hooks(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<Vec<DeployHook>, Error> {
    let rows = sqlx::query_as::<_, DeployHookRow>(
        "SELECT id, project_id, url, events, secret, created_at FROM deploy_hooks WHERE project_id = ? ORDER BY created_at",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn delete_deploy_hook(pool: &SqlitePool, id: &str) -> Result<(), Error> {
    let result = sqlx::query("DELETE FROM deploy_hooks WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::Internal(format!("hook not found: {id}")));
    }
    Ok(())
}

pub async fn get_deployment_by_claim_token(
    pool: &SqlitePool,
    token: &str,
) -> Result<Deployment, Error> {
    sqlx::query_as::<_, DeploymentRow>(
        "SELECT id, project_id, status, source, git_ref, git_sha, image_ref, container_id, url, verification_result, log_path, claim_token, claimed_by, created_at, updated_at
         FROM deployments WHERE claim_token = ?",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?
    .map(TryInto::try_into)
    .transpose()?
    .ok_or_else(|| Error::DeploymentNotFound(token.into()))
}

pub async fn claim_deployment(
    pool: &SqlitePool,
    token: &str,
    claimed_by: &str,
) -> Result<(), Error> {
    sqlx::query("UPDATE deployments SET claimed_by = ?, updated_at = datetime('now') WHERE claim_token = ? AND claimed_by IS NULL")
        .bind(claimed_by)
        .bind(token)
        .execute(pool)
        .await
        .map(|r| {
            if r.rows_affected() == 0 {
                Err(Error::Internal("deployment already claimed or not found".into()))
            } else {
                Ok(())
            }
        })?
}

#[derive(sqlx::FromRow)]
struct ProjectRow {
    id: String,
    name: String,
    repo_url: Option<String>,
    default_branch: Option<String>,
    production_deployment_id: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<ProjectRow> for Project {
    fn from(r: ProjectRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            repo_url: r.repo_url,
            default_branch: r.default_branch,
            production_deployment_id: r.production_deployment_id,
            created_at: parse_dt(&r.created_at),
            updated_at: parse_dt(&r.updated_at),
        }
    }
}

#[derive(sqlx::FromRow)]
struct DeploymentRow {
    id: String,
    project_id: String,
    status: String,
    source: String,
    git_ref: Option<String>,
    git_sha: Option<String>,
    image_ref: Option<String>,
    container_id: Option<String>,
    url: Option<String>,
    verification_result: Option<String>,
    log_path: Option<String>,
    claim_token: Option<String>,
    claimed_by: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<DeploymentRow> for Deployment {
    type Error = Error;

    fn try_from(r: DeploymentRow) -> Result<Self, Error> {
        let status = match r.status.as_str() {
            "pending" => DeploymentStatus::Pending,
            "building" => DeploymentStatus::Building,
            "deploying" => DeploymentStatus::Deploying,
            "verifying" => DeploymentStatus::Verifying,
            "live" => DeploymentStatus::Live,
            "failed" => DeploymentStatus::Failed,
            s => return Err(Error::Internal(format!("unknown deployment status: {s}"))),
        };
        let source = match r.source.as_str() {
            "git_push" => DeploymentSource::GitPush,
            "api_upload" => DeploymentSource::ApiUpload,
            "image" => DeploymentSource::Image,
            s => return Err(Error::Internal(format!("unknown deployment source: {s}"))),
        };
        let verification_result = r
            .verification_result
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|e| Error::Internal(format!("invalid verification json: {e}")))?;

        Ok(Self {
            id: r.id,
            project_id: r.project_id,
            status,
            source,
            git_ref: r.git_ref,
            git_sha: r.git_sha,
            image_ref: r.image_ref,
            container_id: r.container_id,
            url: r.url,
            verification_result,
            log_path: r.log_path,
            claim_token: r.claim_token,
            claimed_by: r.claimed_by,
            created_at: parse_dt(&r.created_at),
            updated_at: parse_dt(&r.updated_at),
        })
    }
}

#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    id: String,
    name: String,
    key_hash: String,
    key_prefix: String,
    scope: String,
    allowed_ips: Option<String>,
    expires_at: Option<String>,
    last_used_at: Option<String>,
    revoked_at: Option<String>,
    created_at: String,
}

impl From<ApiKeyRow> for ApiKey {
    fn from(r: ApiKeyRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            key_hash: r.key_hash,
            key_prefix: r.key_prefix,
            scope: ApiScope::parse(&r.scope).unwrap_or(ApiScope::Read),
            allowed_ips: r.allowed_ips,
            expires_at: r.expires_at.as_deref().map(parse_dt),
            last_used_at: r.last_used_at.as_deref().map(parse_dt),
            revoked_at: r.revoked_at.as_deref().map(parse_dt),
            created_at: parse_dt(&r.created_at),
        }
    }
}

#[derive(sqlx::FromRow)]
struct AuditLogRow {
    id: String,
    api_key_id: String,
    action: String,
    resource: String,
    ip: String,
    timestamp: String,
}

impl From<AuditLogRow> for AuditLog {
    fn from(r: AuditLogRow) -> Self {
        Self {
            id: r.id,
            api_key_id: r.api_key_id,
            action: r.action,
            resource: r.resource,
            ip: r.ip,
            timestamp: parse_dt(&r.timestamp),
        }
    }
}

#[derive(sqlx::FromRow)]
struct DomainRow {
    id: String,
    project_id: String,
    domain: String,
    is_verified: bool,
    created_at: String,
}

impl From<DomainRow> for Domain {
    fn from(r: DomainRow) -> Self {
        Self {
            id: r.id,
            project_id: r.project_id,
            domain: r.domain,
            is_verified: r.is_verified,
            created_at: parse_dt(&r.created_at),
        }
    }
}

#[derive(sqlx::FromRow)]
struct EnvVarRow {
    id: String,
    project_id: String,
    environment: String,
    key: String,
    encrypted_value: Vec<u8>,
}

impl From<EnvVarRow> for EnvVar {
    fn from(r: EnvVarRow) -> Self {
        Self {
            id: r.id,
            project_id: r.project_id,
            environment: r.environment,
            key: r.key,
            encrypted_value: r.encrypted_value,
        }
    }
}

#[derive(sqlx::FromRow)]
struct IdempotencyRow {
    status_code: i64,
    response_body: String,
}

#[derive(sqlx::FromRow)]
struct ManagedDatabaseRow {
    id: String,
    project_id: String,
    engine: String,
    container_id: Option<String>,
    host: String,
    port: i64,
    database_name: String,
    username: String,
    password_encrypted: Vec<u8>,
    status: String,
    created_at: String,
}

impl From<ManagedDatabaseRow> for ManagedDatabase {
    fn from(r: ManagedDatabaseRow) -> Self {
        Self {
            id: r.id,
            project_id: r.project_id,
            engine: r.engine,
            container_id: r.container_id,
            host: r.host,
            port: r.port,
            database_name: r.database_name,
            username: r.username,
            password_encrypted: r.password_encrypted,
            status: r.status,
            created_at: parse_dt(&r.created_at),
        }
    }
}

#[derive(sqlx::FromRow)]
struct DeployHookRow {
    id: String,
    project_id: String,
    url: String,
    events: String,
    secret: Option<String>,
    created_at: String,
}

impl From<DeployHookRow> for DeployHook {
    fn from(r: DeployHookRow) -> Self {
        Self {
            id: r.id,
            project_id: r.project_id,
            url: r.url,
            events: r.events,
            secret: r.secret,
            created_at: parse_dt(&r.created_at),
        }
    }
}

fn dt_str(dt: chrono::DateTime<chrono::Utc>) -> String {
    dt.to_rfc3339()
}

fn parse_dt(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").map(|ndt| ndt.and_utc())
        })
        .unwrap_or_default()
}
