use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;

use arx_core::db;
use arx_core::model::{Deployment, DeploymentSource, DeploymentStatus};

use crate::server::AppState;

type HmacSha256 = Hmac<Sha256>;

fn verify_signature(secret: &[u8], payload: &[u8], signature: &str) -> bool {
    let hex_sig = signature.strip_prefix("sha256=").unwrap_or(signature);
    let sig_bytes = match hex::decode(hex_sig) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let mut mac = match HmacSha256::new_from_slice(secret) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(payload);
    mac.verify_slice(&sig_bytes).is_ok()
}

#[derive(Deserialize)]
struct GithubRepository {
    clone_url: Option<String>,
    html_url: Option<String>,
}

#[derive(Deserialize)]
struct GithubHeadCommit {
    id: Option<String>,
}

#[derive(Deserialize)]
struct GithubPayload {
    #[serde(rename = "ref")]
    git_ref: Option<String>,
    after: Option<String>,
    head_commit: Option<GithubHeadCommit>,
    repository: Option<GithubRepository>,
}

pub async fn github_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({"error": "missing signature"}))))?;

    let secret = std::env::var("ARX_WEBHOOK_SECRET").unwrap_or_default();
    if !verify_signature(secret.as_bytes(), &body, signature) {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid signature"}))));
    }

    let payload: GithubPayload = serde_json::from_slice(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": format!("invalid payload: {e}")}))))?;

    let repo = payload.repository.as_ref();
    let repo_url = repo
        .and_then(|r| r.clone_url.as_deref().or(r.html_url.as_deref()))
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({"error": "missing repository url"}))))?;

    let git_ref = payload.git_ref.as_deref();
    let git_sha = payload
        .after
        .as_deref()
        .or_else(|| payload.head_commit.as_ref().and_then(|c| c.id.as_deref()));

    let branch = git_ref
        .and_then(|r| r.strip_prefix("refs/heads/"))
        .unwrap_or("main");

    trigger_deployment(&state, repo_url, branch, git_sha).await
}

#[derive(Deserialize)]
struct GiteaRepository {
    clone_url: Option<String>,
    html_url: Option<String>,
}

#[derive(Deserialize)]
struct GiteaPayload {
    #[serde(rename = "ref")]
    git_ref: Option<String>,
    after: Option<String>,
    repository: Option<GiteaRepository>,
}

pub async fn gitea_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let signature = headers
        .get("x-gitea-signature")
        .or_else(|| headers.get("x-hub-signature-256"))
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({"error": "missing signature"}))))?;

    let secret = std::env::var("ARX_WEBHOOK_SECRET").unwrap_or_default();
    if !verify_signature(secret.as_bytes(), &body, signature) {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid signature"}))));
    }

    let payload: GiteaPayload = serde_json::from_slice(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": format!("invalid payload: {e}")}))))?;

    let repo = payload.repository.as_ref();
    let repo_url = repo
        .and_then(|r| r.clone_url.as_deref().or(r.html_url.as_deref()))
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({"error": "missing repository url"}))))?;

    let git_ref = payload.git_ref.as_deref();
    let git_sha = payload.after.as_deref();

    let branch = git_ref
        .and_then(|r| r.strip_prefix("refs/heads/"))
        .unwrap_or("main");

    trigger_deployment(&state, repo_url, branch, git_sha).await
}

async fn trigger_deployment(
    state: &AppState,
    repo_url: &str,
    branch: &str,
    git_sha: Option<&str>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let project = db::get_project_by_repo_url(&state.pool, repo_url)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, Json(json!({"error": "no project matches this repository"}))))?;

    let default_branch = project.default_branch.as_deref().unwrap_or("main");
    if branch != default_branch {
        return Ok(Json(json!({"status": "skipped", "reason": "branch does not match"})));
    }

    let deployment = Deployment {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project.id.clone(),
        status: DeploymentStatus::Pending,
        source: DeploymentSource::GitPush,
        git_ref: Some(branch.to_string()),
        git_sha: git_sha.map(String::from),
        image_ref: None,
        container_id: None,
        url: None,
        verification_result: None,
        log_path: None,
        claim_token: None,
        claimed_by: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    db::create_deployment(&state.pool, &deployment)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({
        "status": "triggered",
        "deployment_id": deployment.id,
        "project_id": project.id,
    })))
}
