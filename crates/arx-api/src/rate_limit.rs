use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Json, Response};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::auth::AuthenticatedKey;

#[derive(Clone)]
pub struct RateLimiter {
    windows: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
    max_requests: u64,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        let max_requests = std::env::var("ARX_RATE_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);
        Self {
            windows: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
        }
    }
}

pub async fn rate_limit_middleware(req: Request, next: Next) -> Response {
    let limiter = req.extensions().get::<RateLimiter>().cloned();

    let limiter = match limiter {
        Some(l) => l,
        None => return next.run(req).await,
    };

    let key_id = req
        .extensions()
        .get::<AuthenticatedKey>()
        .map(|a| a.key.id.clone());

    let key_id = match key_id {
        Some(k) => k,
        None => return next.run(req).await,
    };

    let now = Instant::now();
    let window = std::time::Duration::from_secs(60);

    let mut windows = limiter.windows.lock().await;

    windows.retain(|_, timestamps| {
        while timestamps
            .front()
            .is_some_and(|t| now.duration_since(*t) > window)
        {
            timestamps.pop_front();
        }
        !timestamps.is_empty()
    });

    let timestamps = windows.entry(key_id).or_default();

    if timestamps.len() as u64 >= limiter.max_requests {
        let oldest = timestamps.front().unwrap();
        let retry_after = window.saturating_sub(now.duration_since(*oldest));
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", retry_after.as_secs().to_string())],
            Json(json!({"error": "rate limit exceeded"})),
        )
            .into_response();
    }

    timestamps.push_back(now);
    drop(windows);

    next.run(req).await
}
