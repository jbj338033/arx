use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use tokio_stream::StreamExt;

use arx_core::db;
use arx_core::model::*;

use crate::auth::{require_scope, AuthenticatedKey};
use crate::server::AppState;

pub async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub repo_url: Option<String>,
    pub default_branch: Option<String>,
}

pub async fn create_project(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Json(body): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Deploy)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let project = Project {
        id: uuid::Uuid::new_v4().to_string(),
        name: body.name,
        repo_url: body.repo_url,
        default_branch: body.default_branch,
        production_deployment_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    db::create_project(&state.pool, &project)
        .await
        .map_err(|e| (StatusCode::CONFLICT, Json(json!({"error": e.to_string()}))))?;

    Ok((StatusCode::CREATED, Json(json!(project))))
}

pub async fn list_projects(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Read)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let projects = db::list_projects(&state.pool).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    Ok(Json(json!(projects)))
}

pub async fn get_project(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Read)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let project = db::get_project(&state.pool, &id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!(project)))
}

#[derive(Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub repo_url: Option<String>,
    pub default_branch: Option<String>,
}

pub async fn update_project(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(id): Path<String>,
    Json(body): Json<UpdateProjectRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Deploy)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let project = db::update_project(
        &state.pool,
        &id,
        body.name.as_deref(),
        body.repo_url.as_deref(),
        body.default_branch.as_deref(),
    )
    .await
    .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!(project)))
}

pub async fn delete_project(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Admin)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    db::delete_project(&state.pool, &id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_deployments(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(project_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Read)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let deployments = db::list_deployments(&state.pool, &project_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    Ok(Json(json!(deployments)))
}

pub async fn get_deployment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path((_project_id, deployment_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Read)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let deployment = db::get_deployment(&state.pool, &deployment_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!(deployment)))
}

#[derive(Deserialize)]
pub struct CreateDeploymentRequest {
    pub image_ref: Option<String>,
    pub git_ref: Option<String>,
    pub claimable: Option<bool>,
}

pub async fn create_deployment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(project_id): Path<String>,
    Json(body): Json<CreateDeploymentRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Deploy)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let _project = db::get_project(&state.pool, &project_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))))?;

    let (source, image_ref, git_ref) = if let Some(ref img) = body.image_ref {
        (DeploymentSource::Image, Some(img.clone()), None)
    } else if let Some(ref gr) = body.git_ref {
        (DeploymentSource::GitPush, None, Some(gr.clone()))
    } else {
        (DeploymentSource::ApiUpload, None, None)
    };

    let claim_token = if body.claimable.unwrap_or(false) {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    let deployment = Deployment {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project_id.clone(),
        status: DeploymentStatus::Pending,
        source,
        git_ref,
        git_sha: None,
        image_ref,
        container_id: None,
        url: None,
        verification_result: None,
        log_path: None,
        claim_token,
        claimed_by: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    db::create_deployment(&state.pool, &deployment)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    if let Some(image) = body.image_ref {
        let pool = state.pool.clone();
        let engine = state.engine.clone();
        let caddy = state.caddy.clone();
        let dep_id = deployment.id.clone();
        let pid = project_id.clone();
        tokio::spawn(async move {
            if let Err(e) = run_image_deploy(&pool, &engine, &caddy, &dep_id, &pid, &image).await {
                tracing::error!(deployment_id = %dep_id, "deploy failed: {e}");
                let _ =
                    db::update_deployment_status(&pool, &dep_id, DeploymentStatus::Failed).await;
            }
        });
    }

    Ok((StatusCode::CREATED, Json(json!(deployment))))
}

async fn run_image_deploy(
    pool: &sqlx::SqlitePool,
    engine: &std::sync::Arc<arx_engine::deploy::DeployEngine>,
    caddy: &Option<std::sync::Arc<arx_proxy::caddy::CaddyClient>>,
    deployment_id: &str,
    project_id: &str,
    image: &str,
) -> Result<(), arx_core::error::Error> {
    db::update_deployment_status(pool, deployment_id, DeploymentStatus::Deploying).await?;

    let config = arx_core::config::ArxConfig {
        build: Default::default(),
        deploy: Default::default(),
        resources: Default::default(),
    };

    let env = {
        let master_key = arx_core::crypto::load_master_key(
            &arx_core::config::ServerConfig::default().master_key_path,
        )?;
        let vars = db::get_env_vars(pool, project_id).await?;
        vars.into_iter()
            .filter_map(|v| {
                arx_core::crypto::decrypt(&master_key, &v.encrypted_value)
                    .ok()
                    .and_then(|plain| String::from_utf8(plain).ok())
                    .map(|val| format!("{}={}", v.key, val))
            })
            .collect::<Vec<_>>()
    };

    let result = engine
        .deploy_image(
            &db::get_deployment(pool, deployment_id).await?,
            image,
            env,
            &config,
        )
        .await?;

    db::update_deployment_container(
        pool,
        deployment_id,
        &result.container_id,
        &format!("http://127.0.0.1:{}", result.host_port),
    )
    .await?;
    db::update_deployment_status(pool, deployment_id, DeploymentStatus::Verifying).await?;

    let verification = engine
        .verify(&result.container_id, result.host_port, None)
        .await;
    let verification_json = serde_json::to_value(&verification).unwrap_or_default();
    db::update_deployment_verification(pool, deployment_id, &verification_json).await?;

    let final_status = if verification.health_check.unwrap_or(false) {
        let prev_deployments = db::list_deployments(pool, project_id).await?;
        let prev_containers: Vec<String> = prev_deployments
            .iter()
            .filter(|d| d.id != deployment_id && d.status == DeploymentStatus::Live)
            .filter_map(|d| d.container_id.clone())
            .collect();

        db::update_project_production(pool, project_id, deployment_id).await?;

        if let Some(caddy) = caddy {
            let upstream = format!("127.0.0.1:{}", result.host_port);
            let domains = db::list_domains(pool, project_id).await.unwrap_or_default();
            for domain in &domains {
                if let Err(e) = caddy.update_upstream(&domain.domain, &upstream).await {
                    tracing::warn!(domain = %domain.domain, "caddy update failed: {e}");
                }
            }
        }

        for cid in &prev_containers {
            if let Err(e) = engine.stop_previous(cid).await {
                tracing::warn!(container_id = %cid, "failed to stop previous container: {e}");
            }
        }

        DeploymentStatus::Live
    } else {
        DeploymentStatus::Failed
    };

    db::update_deployment_status(pool, deployment_id, final_status).await?;

    let event = if final_status == DeploymentStatus::Live {
        "success"
    } else {
        "failure"
    };
    if let Ok(hooks) = db::list_deploy_hooks(pool, project_id).await {
        let deployment = db::get_deployment(pool, deployment_id).await.ok();
        for hook in hooks {
            if !hook.events.split(',').any(|e| e.trim() == event) {
                continue;
            }
            let url = hook.url.clone();
            let payload = json!({
                "event": event,
                "deployment": deployment,
                "project_id": project_id,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            let secret = hook.secret.clone();
            tokio::spawn(async move {
                let client = reqwest::Client::new();
                let body = serde_json::to_string(&payload).unwrap_or_default();
                let mut req = client.post(&url).header("content-type", "application/json");
                if let Some(ref sec) = secret {
                    use hmac::{Hmac, Mac};
                    use sha2::Sha256;
                    type HmacSha256 = Hmac<Sha256>;
                    if let Ok(mut mac) = HmacSha256::new_from_slice(sec.as_bytes()) {
                        mac.update(body.as_bytes());
                        let sig = hex::encode(mac.finalize().into_bytes());
                        req = req.header("x-arx-signature", format!("sha256={sig}"));
                    }
                }
                let _ = req
                    .body(body)
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await;
            });
        }
    }

    Ok(())
}

#[derive(Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub scope: String,
    pub ttl_days: Option<i64>,
    pub allowed_ips: Option<String>,
}

pub async fn create_api_key(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Admin)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let scope = ApiScope::parse(&body.scope).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid scope"})),
        )
    })?;

    let raw_key = crate::auth::generate_api_key();
    let key_hash = crate::auth::hash_key(&raw_key);
    let key_prefix = raw_key[..15].to_string();

    let expires_at = body
        .ttl_days
        .map(|days| chrono::Utc::now() + chrono::Duration::days(days));

    let api_key = ApiKey {
        id: uuid::Uuid::new_v4().to_string(),
        name: body.name,
        key_hash,
        key_prefix,
        scope,
        allowed_ips: body.allowed_ips,
        expires_at,
        last_used_at: None,
        revoked_at: None,
        created_at: chrono::Utc::now(),
    };

    db::create_api_key(&state.pool, &api_key)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": api_key.id,
            "name": api_key.name,
            "key": raw_key,
            "key_prefix": api_key.key_prefix,
            "scope": body.scope,
            "expires_at": api_key.expires_at,
        })),
    ))
}

pub async fn list_api_keys(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Admin)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let keys = db::list_api_keys(&state.pool).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    let masked: Vec<Value> = keys
        .iter()
        .map(|k| {
            json!({
                "id": k.id,
                "name": k.name,
                "key_prefix": format!("{}...", k.key_prefix),
                "scope": k.scope.as_str(),
                "expires_at": k.expires_at,
                "last_used_at": k.last_used_at,
                "created_at": k.created_at,
            })
        })
        .collect();

    Ok(Json(json!(masked)))
}

pub async fn revoke_api_key(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Admin)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    db::revoke_api_key(&state.pool, &id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct AddDomainRequest {
    pub domain: String,
}

pub async fn add_domain(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(project_id): Path<String>,
    Json(body): Json<AddDomainRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Deploy)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let domain = Domain {
        id: uuid::Uuid::new_v4().to_string(),
        project_id,
        domain: body.domain,
        is_verified: false,
        created_at: chrono::Utc::now(),
    };

    db::create_domain(&state.pool, &domain).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    if let Some(ref caddy) = state.caddy {
        let project = db::get_project(&state.pool, &domain.project_id).await.ok();
        if let Some(proj) = project {
            if let Some(ref dep_id) = proj.production_deployment_id {
                if let Ok(dep) = db::get_deployment(&state.pool, dep_id).await {
                    if let Some(ref url) = dep.url {
                        let upstream = url.trim_start_matches("http://");
                        let _ = caddy.add_route(&domain.domain, upstream).await;
                    }
                }
            }
        }
    }

    Ok((StatusCode::CREATED, Json(json!(domain))))
}

pub async fn list_domains(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(project_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Read)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let domains = db::list_domains(&state.pool, &project_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    Ok(Json(json!(domains)))
}

pub async fn delete_domain(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path((_project_id, domain_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Admin)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    if let Some(ref caddy) = state.caddy {
        if let Ok(domain) = db::get_domain(&state.pool, &domain_id).await {
            let _ = caddy.remove_route(&domain.domain).await;
        }
    }

    db::delete_domain(&state.pool, &domain_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct AuditQuery {
    pub action: Option<String>,
    pub key_id: Option<String>,
    pub since: Option<String>,
    pub limit: Option<i64>,
}

pub async fn list_audit_logs(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    axum::extract::Query(query): axum::extract::Query<AuditQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Admin)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let since = query.since.as_deref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });
    let limit = query.limit.unwrap_or(100);

    let logs = db::list_audit_logs(
        &state.pool,
        query.key_id.as_deref(),
        query.action.as_deref(),
        since,
        limit,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    Ok(Json(json!(logs)))
}

pub async fn list_env_vars(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(project_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Read)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let vars = db::get_env_vars(&state.pool, &project_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    let keys: Vec<Value> = vars
        .iter()
        .map(|v| json!({"key": v.key, "environment": v.environment}))
        .collect();

    Ok(Json(json!(keys)))
}

#[derive(Deserialize)]
pub struct SetEnvVarsRequest {
    pub vars: std::collections::HashMap<String, String>,
}

pub async fn set_env_vars(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(project_id): Path<String>,
    Json(body): Json<SetEnvVarsRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Deploy)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let master_key = arx_core::crypto::load_master_key(
        &arx_core::config::ServerConfig::default().master_key_path,
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    for (key, value) in &body.vars {
        let encrypted = arx_core::crypto::encrypt(&master_key, value.as_bytes()).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;
        db::set_env_var(&state.pool, &project_id, key, &encrypted)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": e.to_string()})),
                )
            })?;
    }

    Ok(Json(json!({"updated": body.vars.len()})))
}

pub async fn delete_env_var(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path((project_id, key)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Deploy)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    db::delete_env_var(&state.pool, &project_id, &key)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn deployment_logs(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path((_project_id, deployment_id)): Path<(String, String)>,
) -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<Value>),
> {
    require_scope(&auth.key, &ApiScope::Read)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let deployment = db::get_deployment(&state.pool, &deployment_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))))?;

    let log_path = deployment.log_path.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no log file for this deployment"})),
        )
    })?;

    let content = tokio::fs::read_to_string(&log_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("failed to read log: {e}")})),
        )
    })?;

    let lines: Vec<String> = content.lines().map(String::from).collect();
    let stream = tokio_stream::iter(lines).map(|line| Ok(Event::default().data(line)));

    Ok(Sse::new(stream))
}

pub async fn promote_deployment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path((project_id, deployment_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Deploy)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let target = db::get_deployment(&state.pool, &deployment_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))))?;

    if target.project_id != project_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "deployment does not belong to project"})),
        ));
    }

    if target.container_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "deployment has no running container to promote"})),
        ));
    }

    db::update_project_production(&state.pool, &project_id, &deployment_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    db::update_deployment_status(&state.pool, &deployment_id, DeploymentStatus::Live)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    if let Some(ref caddy) = state.caddy {
        if let Some(ref url) = target.url {
            let upstream = url.trim_start_matches("http://");
            let domains = db::list_domains(&state.pool, &project_id)
                .await
                .unwrap_or_default();
            for domain in &domains {
                let _ = caddy.update_upstream(&domain.domain, upstream).await;
            }
        }
    }

    Ok(Json(json!({
        "status": "promoted",
        "deployment_id": deployment_id,
        "project_id": project_id,
    })))
}

pub async fn rollback_deployment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path((project_id, deployment_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Deploy)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let target = db::get_deployment(&state.pool, &deployment_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))))?;

    if target.project_id != project_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "deployment does not belong to project"})),
        ));
    }

    if target.status != DeploymentStatus::Live && target.container_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "deployment has no running container to promote"})),
        ));
    }

    db::update_project_production(&state.pool, &project_id, &deployment_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    db::update_deployment_status(&state.pool, &deployment_id, DeploymentStatus::Live)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    Ok(Json(json!({
        "status": "rolled_back",
        "deployment_id": deployment_id,
        "project_id": project_id,
    })))
}

#[derive(Deserialize)]
pub struct CreateDatabaseRequest {
    pub engine: String,
    pub name: Option<String>,
}

pub async fn create_database(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(project_id): Path<String>,
    Json(body): Json<CreateDatabaseRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Deploy)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let _project = db::get_project(&state.pool, &project_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))))?;

    let db_name = body
        .name
        .unwrap_or_else(|| format!("db_{}", &project_id[..8]));
    let db_manager = arx_engine::database::DatabaseManager::new(&state.engine.containers);
    let info = db_manager
        .provision(&body.engine, &project_id, &db_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    let master_key = arx_core::crypto::load_master_key(
        &arx_core::config::ServerConfig::default().master_key_path,
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    let password_encrypted = arx_core::crypto::encrypt(&master_key, info.password.as_bytes())
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    let managed_db = ManagedDatabase {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project_id.clone(),
        engine: body.engine.clone(),
        container_id: Some(info.container_id),
        host: "127.0.0.1".into(),
        port: info.port as i64,
        database_name: db_name.clone(),
        username: info.username.clone(),
        password_encrypted,
        status: "running".into(),
        created_at: chrono::Utc::now(),
    };

    db::create_managed_database(&state.pool, &managed_db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    let database_url = format!(
        "{}://{}:{}@127.0.0.1:{}/{}",
        body.engine, info.username, info.password, info.port, db_name
    );
    let encrypted_url =
        arx_core::crypto::encrypt(&master_key, database_url.as_bytes()).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;
    let _ = db::set_env_var(&state.pool, &project_id, "DATABASE_URL", &encrypted_url).await;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": managed_db.id,
            "engine": body.engine,
            "host": "127.0.0.1",
            "port": info.port,
            "database_name": db_name,
            "username": info.username,
            "status": "running",
        })),
    ))
}

pub async fn list_databases(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(project_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Read)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let dbs = db::list_managed_databases(&state.pool, &project_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    let result: Vec<Value> = dbs
        .iter()
        .map(|d| {
            json!({
                "id": d.id,
                "engine": d.engine,
                "host": d.host,
                "port": d.port,
                "database_name": d.database_name,
                "username": d.username,
                "status": d.status,
                "created_at": d.created_at,
            })
        })
        .collect();

    Ok(Json(json!(result)))
}

pub async fn delete_database(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path((_project_id, db_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Admin)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let managed = db::delete_managed_database(&state.pool, &db_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))))?;

    if let Some(ref cid) = managed.container_id {
        let _ = state.engine.stop_previous(cid).await;
    }

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct CreateDeployHookRequest {
    pub url: String,
    pub events: Option<String>,
    pub secret: Option<String>,
}

pub async fn create_deploy_hook(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(project_id): Path<String>,
    Json(body): Json<CreateDeployHookRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Deploy)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let hook = DeployHook {
        id: uuid::Uuid::new_v4().to_string(),
        project_id,
        url: body.url,
        events: body.events.unwrap_or_else(|| "success,failure".into()),
        secret: body.secret,
        created_at: chrono::Utc::now(),
    };

    db::create_deploy_hook(&state.pool, &hook)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    Ok((StatusCode::CREATED, Json(json!(hook))))
}

pub async fn list_deploy_hooks(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(project_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Read)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let hooks = db::list_deploy_hooks(&state.pool, &project_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    Ok(Json(json!(hooks)))
}

pub async fn delete_deploy_hook(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path((_project_id, hook_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Admin)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    db::delete_deploy_hook(&state.pool, &hook_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))))?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct DiffQuery {
    pub from: String,
    pub to: String,
}

pub async fn deployment_diff(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(_project_id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<DiffQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_scope(&auth.key, &ApiScope::Read)
        .map_err(|s| (s, Json(json!({"error": "insufficient scope"}))))?;

    let from = db::get_deployment(&state.pool, &query.from)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("from: {e}")})),
            )
        })?;
    let to = db::get_deployment(&state.pool, &query.to)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("to: {e}")})),
            )
        })?;

    let mut changes = Vec::new();

    if from.status != to.status {
        changes.push(
            json!({"field": "status", "from": from.status.as_str(), "to": to.status.as_str()}),
        );
    }
    if from.image_ref != to.image_ref {
        changes.push(json!({"field": "image_ref", "from": from.image_ref, "to": to.image_ref}));
    }
    if from.source != to.source {
        changes.push(
            json!({"field": "source", "from": from.source.as_str(), "to": to.source.as_str()}),
        );
    }
    if from.git_ref != to.git_ref {
        changes.push(json!({"field": "git_ref", "from": from.git_ref, "to": to.git_ref}));
    }
    if from.git_sha != to.git_sha {
        changes.push(json!({"field": "git_sha", "from": from.git_sha, "to": to.git_sha}));
    }

    let from_env = db::get_env_vars(&state.pool, &from.project_id)
        .await
        .unwrap_or_default();
    let to_env = db::get_env_vars(&state.pool, &to.project_id)
        .await
        .unwrap_or_default();
    let from_keys: std::collections::HashSet<&str> =
        from_env.iter().map(|v| v.key.as_str()).collect();
    let to_keys: std::collections::HashSet<&str> = to_env.iter().map(|v| v.key.as_str()).collect();
    let added: Vec<&str> = to_keys.difference(&from_keys).copied().collect();
    let removed: Vec<&str> = from_keys.difference(&to_keys).copied().collect();
    if !added.is_empty() || !removed.is_empty() {
        changes.push(json!({"field": "env_vars", "added": added, "removed": removed}));
    }

    Ok(Json(json!({
        "from": query.from,
        "to": query.to,
        "changes": changes,
    })))
}

pub async fn claim_deployment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedKey>,
    Path(token): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let deployment = db::get_deployment_by_claim_token(&state.pool, &token)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()}))))?;

    if deployment.claimed_by.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({"error": "deployment already claimed"})),
        ));
    }

    db::claim_deployment(&state.pool, &token, &auth.key.id)
        .await
        .map_err(|e| (StatusCode::CONFLICT, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({
        "status": "claimed",
        "deployment_id": deployment.id,
        "claimed_by": auth.key.id,
    })))
}
