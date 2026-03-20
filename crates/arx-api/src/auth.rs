use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Json, Response};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::net::IpAddr;

use arx_core::db;
use arx_core::model::{ApiKey, ApiScope};

#[derive(Clone)]
pub struct AuthenticatedKey {
    pub key: ApiKey,
}

pub async fn auth_middleware(mut req: Request, next: Next) -> Response {
    let pool = match req.extensions().get::<SqlitePool>().cloned() {
        Some(p) => p,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "server misconfigured"})),
            )
                .into_response();
        }
    };

    let token = match extract_token(&req) {
        Ok(t) => t,
        Err(status) => {
            return (
                status,
                Json(serde_json::json!({"error": status.canonical_reason().unwrap_or("unauthorized")})),
            )
                .into_response();
        }
    };

    let key = match validate_token(&pool, &token).await {
        Ok(k) => k,
        Err(status) => {
            return (
                status,
                Json(serde_json::json!({"error": status.canonical_reason().unwrap_or("unauthorized")})),
            )
                .into_response();
        }
    };

    if let Some(ref allowed) = key.allowed_ips {
        match extract_client_ip(&req) {
            Some(ip) if check_ip_allowed(ip, allowed) => {}
            _ => {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({"error": "ip not allowed"})),
                )
                    .into_response();
            }
        }
    }

    let pool2 = pool.clone();
    let key_id = key.id.clone();
    tokio::spawn(async move {
        let _ = db::touch_api_key(&pool2, &key_id).await;
    });

    req.extensions_mut().insert(AuthenticatedKey { key });
    next.run(req).await
}

fn extract_token(req: &Request) -> Result<String, StatusCode> {
    let header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if !token.starts_with("arx_sk_") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(token.to_string())
}

async fn validate_token(pool: &SqlitePool, token: &str) -> Result<ApiKey, StatusCode> {
    let hash = hash_key(token);
    let key = db::get_api_key_by_hash(pool, &hash)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    if key.revoked_at.is_some() {
        return Err(StatusCode::FORBIDDEN);
    }

    if let Some(expires) = key.expires_at {
        if expires < chrono::Utc::now() {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    Ok(key)
}

pub fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn generate_api_key() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    format!("arx_sk_{}", hex::encode(bytes))
}

pub fn require_scope(key: &ApiKey, required: &ApiScope) -> Result<(), StatusCode> {
    if key.scope.can_access(required) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

fn extract_client_ip(req: &Request) -> Option<IpAddr> {
    req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse::<IpAddr>().ok())
        .or_else(|| {
            req.extensions()
                .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                .map(|ci| ci.0.ip())
        })
}

fn check_ip_allowed(ip: IpAddr, allowed: &str) -> bool {
    use ipnet::IpNet;
    for entry in allowed.split(',') {
        let entry = entry.trim();
        if let Ok(net) = entry.parse::<IpNet>() {
            if net.contains(&ip) {
                return true;
            }
        } else if let Ok(addr) = entry.parse::<IpAddr>() {
            if addr == ip {
                return true;
            }
        }
    }
    false
}
