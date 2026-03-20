use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sqlx::SqlitePool;

use arx_core::db;

use crate::auth::AuthenticatedKey;

pub async fn idempotency_middleware(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    if method != axum::http::Method::POST && method != axum::http::Method::PUT {
        return next.run(req).await;
    }

    let idem_key = req
        .headers()
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let idem_key = match idem_key {
        Some(k) => k,
        None => return next.run(req).await,
    };

    let pool = req.extensions().get::<SqlitePool>().cloned();
    let api_key_id = req
        .extensions()
        .get::<AuthenticatedKey>()
        .map(|a| a.key.id.clone());

    let (pool, api_key_id) = match (pool, api_key_id) {
        (Some(p), Some(k)) => (p, k),
        _ => return next.run(req).await,
    };

    let path = req.uri().path().to_string();

    if let Ok(Some((status_code, body))) =
        db::get_idempotency_key(&pool, &idem_key, &api_key_id).await
    {
        let status =
            StatusCode::from_u16(status_code as u16).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        return (status, [("content-type", "application/json")], body).into_response();
    }

    let resp = next.run(req).await;
    let status = resp.status();

    let (parts, body) = resp.into_parts();
    let bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };

    let body_str = String::from_utf8_lossy(&bytes).to_string();
    let _ = db::save_idempotency_key(
        &pool,
        &idem_key,
        &api_key_id,
        method.as_str(),
        &path,
        status.as_u16() as i64,
        &body_str,
    )
    .await;

    Response::from_parts(parts, Body::from(bytes))
}
