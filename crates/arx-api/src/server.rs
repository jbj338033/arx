use axum::middleware;
use axum::routing::{delete, get, patch, post, put};
use axum::Router;
use sqlx::SqlitePool;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use crate::auth::auth_middleware;
use crate::idempotency::idempotency_middleware;
use crate::rate_limit::{rate_limit_middleware, RateLimiter};
use crate::routes;
use crate::webhook;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub engine: Arc<arx_engine::deploy::DeployEngine>,
    pub caddy: Option<Arc<arx_proxy::caddy::CaddyClient>>,
    pub rate_limiter: RateLimiter,
}

pub fn create_router(state: AppState) -> Router {
    let pool = state.pool.clone();
    let rate_limiter = state.rate_limiter.clone();
    let authed = Router::new()
        .route("/projects", post(routes::create_project))
        .route("/projects", get(routes::list_projects))
        .route("/projects/{id}", get(routes::get_project))
        .route("/projects/{id}", patch(routes::update_project))
        .route("/projects/{id}", delete(routes::delete_project))
        .route("/projects/{id}/deployments", post(routes::create_deployment))
        .route("/projects/{id}/deployments", get(routes::list_deployments))
        .route(
            "/projects/{id}/deployments/{did}",
            get(routes::get_deployment),
        )
        .route("/projects/{id}/domains", post(routes::add_domain))
        .route("/projects/{id}/domains", get(routes::list_domains))
        .route(
            "/projects/{id}/domains/{did}",
            delete(routes::delete_domain),
        )
        .route("/auth/keys", post(routes::create_api_key))
        .route("/auth/keys", get(routes::list_api_keys))
        .route("/auth/keys/{id}", delete(routes::revoke_api_key))
        .route("/projects/{id}/env", get(routes::list_env_vars))
        .route("/projects/{id}/env", put(routes::set_env_vars))
        .route("/projects/{id}/env/{key}", delete(routes::delete_env_var))
        .route(
            "/projects/{id}/deployments/{did}/logs",
            get(routes::deployment_logs),
        )
        .route(
            "/projects/{id}/deployments/{did}/promote",
            post(routes::promote_deployment),
        )
        .route(
            "/projects/{id}/deployments/{did}/rollback",
            post(routes::rollback_deployment),
        )
        .route("/audit", get(routes::list_audit_logs))
        .route("/projects/{id}/databases", post(routes::create_database))
        .route("/projects/{id}/databases", get(routes::list_databases))
        .route(
            "/projects/{id}/databases/{did}",
            delete(routes::delete_database),
        )
        .route("/projects/{id}/hooks", post(routes::create_deploy_hook))
        .route("/projects/{id}/hooks", get(routes::list_deploy_hooks))
        .route(
            "/projects/{id}/hooks/{hid}",
            delete(routes::delete_deploy_hook),
        )
        .route("/projects/{id}/diff", get(routes::deployment_diff))
        .route("/claim/{token}", post(routes::claim_deployment))
        .with_state(state.clone())
        .layer(axum::Extension(rate_limiter))
        .layer(axum::Extension(pool))
        .route_layer(middleware::from_fn(idempotency_middleware))
        .route_layer(middleware::from_fn(rate_limit_middleware))
        .route_layer(middleware::from_fn(auth_middleware));

    let public = Router::new()
        .route("/health", get(routes::health))
        .route("/webhooks/github", post(webhook::github_webhook))
        .route("/webhooks/gitea", post(webhook::gitea_webhook))
        .with_state(state.clone());

    Router::new()
        .nest("/api/v1", public)
        .nest("/api/v1", authed)
        .layer(TraceLayer::new_for_http())
}

pub async fn run(pool: SqlitePool, host: &str, port: u16) -> Result<(), arx_core::error::Error> {
    let engine = Arc::new(
        arx_engine::deploy::DeployEngine::new()
            .map_err(|e| arx_core::error::Error::Internal(format!("engine init failed: {e}")))?,
    );

    let caddy = match std::env::var("CADDY_ADMIN_URL") {
        Ok(url) => {
            let client = arx_proxy::caddy::CaddyClient::new(&url);
            if let Err(e) = client.ensure_server().await {
                tracing::warn!("caddy init failed (continuing without proxy): {e}");
                None
            } else {
                tracing::info!("caddy proxy enabled at {url}");
                Some(Arc::new(client))
            }
        }
        Err(_) => None,
    };

    let rate_limiter = RateLimiter::new();
    let state = AppState { pool: pool.clone(), engine, caddy, rate_limiter };

    let cleanup_pool = pool.clone();
    tokio::spawn(async move {
        let _ = arx_core::db::cleanup_idempotency_keys(&cleanup_pool).await;
    });

    let app = create_router(state);
    let addr = format!("{host}:{port}");

    tracing::info!("arx server listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| arx_core::error::Error::Internal(format!("bind failed: {e}")))?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(|e| arx_core::error::Error::Internal(format!("server error: {e}")))?;

    Ok(())
}
