use std::net::SocketAddr;
use std::sync::Arc;

use reqwest::Client;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::net::TcpListener;

use arx_api::rate_limit::RateLimiter;
use arx_api::server::{create_router, AppState};
use arx_core::model::ApiScope;

// 32-byte test master key (AES-256-GCM requires 32 bytes)
const TEST_MASTER_KEY: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";

struct TestEnv {
    client: Client,
    base_url: String,
    admin_key: String,
    deploy_key: String,
    read_key: String,
    _tmpdir: TempDir,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestEnv {
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn get(&self, path: &str, key: &str) -> reqwest::Response {
        self.client
            .get(self.url(path))
            .bearer_auth(key)
            .send()
            .await
            .unwrap()
    }

    async fn post(&self, path: &str, key: &str, body: Value) -> reqwest::Response {
        self.client
            .post(self.url(path))
            .bearer_auth(key)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn post_with_idempotency(
        &self,
        path: &str,
        key: &str,
        body: Value,
        ikey: &str,
    ) -> reqwest::Response {
        self.client
            .post(self.url(path))
            .bearer_auth(key)
            .header("idempotency-key", ikey)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn patch(&self, path: &str, key: &str, body: Value) -> reqwest::Response {
        self.client
            .patch(self.url(path))
            .bearer_auth(key)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn put(&self, path: &str, key: &str, body: Value) -> reqwest::Response {
        self.client
            .put(self.url(path))
            .bearer_auth(key)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn delete(&self, path: &str, key: &str) -> reqwest::Response {
        self.client
            .delete(self.url(path))
            .bearer_auth(key)
            .send()
            .await
            .unwrap()
    }
}

async fn insert_api_key(pool: &sqlx::SqlitePool, name: &str, scope: ApiScope) -> String {
    let raw = arx_api::auth::generate_api_key();
    let hash = arx_api::auth::hash_key(&raw);
    let key = arx_core::model::ApiKey {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        key_hash: hash,
        key_prefix: raw[..15].to_string(),
        scope,
        allowed_ips: None,
        expires_at: None,
        last_used_at: None,
        revoked_at: None,
        created_at: chrono::Utc::now(),
    };
    arx_core::db::create_api_key(pool, &key).await.unwrap();
    raw
}

async fn setup() -> TestEnv {
    std::env::set_var("ARX_MASTER_KEY", TEST_MASTER_KEY);
    std::env::set_var("ARX_RATE_LIMIT", "10000");

    let tmpdir = TempDir::new().unwrap();
    let db_path = tmpdir.path().join("arx-test.db");
    let db_url = format!("sqlite:{}", db_path.display());

    let pool = arx_core::db::connect(&db_url).await.unwrap();

    let admin_key = insert_api_key(&pool, "test-admin", ApiScope::Admin).await;
    let deploy_key = insert_api_key(&pool, "test-deploy", ApiScope::Deploy).await;
    let read_key = insert_api_key(&pool, "test-read", ApiScope::Read).await;

    let engine =
        Arc::new(arx_engine::deploy::DeployEngine::new().expect("deploy engine init failed"));

    let state = AppState {
        pool,
        engine,
        caddy: None,
        rate_limiter: RateLimiter::new(),
    };

    let app = create_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let _ = rx.await;
        })
        .await
        .unwrap();
    });

    TestEnv {
        client: Client::new(),
        base_url: format!("http://127.0.0.1:{}/api/v1", port),
        admin_key,
        deploy_key,
        read_key,
        _tmpdir: tmpdir,
        _shutdown: tx,
    }
}

// ── Health ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_check() {
    let env = setup().await;
    let res = env.client.get(env.url("/health")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

// ── Auth ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn auth_missing_token() {
    let env = setup().await;
    let res = env.client.get(env.url("/projects")).send().await.unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn auth_invalid_prefix() {
    let env = setup().await;
    let res = env
        .client
        .get(env.url("/projects"))
        .bearer_auth("invalid_token_without_prefix")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn auth_unknown_token() {
    let env = setup().await;
    let fake = arx_api::auth::generate_api_key();
    let res = env
        .client
        .get(env.url("/projects"))
        .bearer_auth(&fake)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ── Scope enforcement ─────────────────────────────────────────────────────────

#[tokio::test]
async fn read_key_cannot_create_project() {
    let env = setup().await;
    let res = env
        .post(
            "/projects",
            &env.read_key.clone(),
            json!({"name": "forbidden"}),
        )
        .await;
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn deploy_key_cannot_delete_project() {
    let env = setup().await;
    // create a project first with admin
    let created = env
        .post(
            "/projects",
            &env.admin_key.clone(),
            json!({"name": "scope-test"}),
        )
        .await
        .json::<Value>()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let res = env
        .delete(&format!("/projects/{id}"), &env.deploy_key.clone())
        .await;
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn read_key_cannot_delete_api_key() {
    let env = setup().await;
    let res = env
        .delete("/auth/keys/some-id", &env.read_key.clone())
        .await;
    assert_eq!(res.status(), 403);
}

// ── Projects ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn project_create_list_get() {
    let env = setup().await;
    let admin = env.admin_key.clone();

    // create
    let res = env
        .post(
            "/projects",
            &admin,
            json!({"name": "my-app", "repo_url": "https://github.com/org/repo", "default_branch": "main"}),
        )
        .await;
    assert_eq!(res.status(), 201);
    let project: Value = res.json().await.unwrap();
    assert_eq!(project["name"], "my-app");
    let id = project["id"].as_str().unwrap().to_string();

    // list
    let list = env.get("/projects", &admin).await;
    assert_eq!(list.status(), 200);
    let projects: Value = list.json().await.unwrap();
    assert!(projects.as_array().unwrap().iter().any(|p| p["id"] == id));

    // get by id
    let get = env.get(&format!("/projects/{id}"), &admin).await;
    assert_eq!(get.status(), 200);
    let got: Value = get.json().await.unwrap();
    assert_eq!(got["id"], id);
    assert_eq!(got["name"], "my-app");
}

#[tokio::test]
async fn project_duplicate_name_rejected() {
    let env = setup().await;
    let admin = env.admin_key.clone();
    env.post("/projects", &admin, json!({"name": "dupe"})).await;
    let res = env.post("/projects", &admin, json!({"name": "dupe"})).await;
    assert_eq!(res.status(), 409);
}

#[tokio::test]
async fn project_update() {
    let env = setup().await;
    let admin = env.admin_key.clone();
    let created: Value = env
        .post("/projects", &admin, json!({"name": "update-me"}))
        .await
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let res = env
        .patch(
            &format!("/projects/{id}"),
            &admin,
            json!({"name": "updated-name", "default_branch": "develop"}),
        )
        .await;
    assert_eq!(res.status(), 200);
    let updated: Value = res.json().await.unwrap();
    assert_eq!(updated["name"], "updated-name");
    assert_eq!(updated["default_branch"], "develop");
}

#[tokio::test]
async fn project_not_found() {
    let env = setup().await;
    let res = env
        .get("/projects/nonexistent-id", &env.admin_key.clone())
        .await;
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn project_delete() {
    let env = setup().await;
    let admin = env.admin_key.clone();
    let created: Value = env
        .post("/projects", &admin, json!({"name": "delete-me"}))
        .await
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let res = env.delete(&format!("/projects/{id}"), &admin).await;
    assert_eq!(res.status(), 204);

    let get = env.get(&format!("/projects/{id}"), &admin).await;
    assert_eq!(get.status(), 404);
}

// ── Deployments ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn deployment_create_claimable_and_list() {
    let env = setup().await;
    let admin = env.admin_key.clone();

    let project: Value = env
        .post("/projects", &admin, json!({"name": "deploy-test"}))
        .await
        .json()
        .await
        .unwrap();
    let pid = project["id"].as_str().unwrap();

    // create a claimable deployment (no image → stays pending, no Docker needed)
    let res = env
        .post(
            &format!("/projects/{pid}/deployments"),
            &admin,
            json!({"claimable": true}),
        )
        .await;
    assert_eq!(res.status(), 201);
    let dep: Value = res.json().await.unwrap();
    assert_eq!(dep["status"], "pending");
    assert!(dep["claim_token"].is_string());
    let did = dep["id"].as_str().unwrap();

    // list
    let list: Value = env
        .get(&format!("/projects/{pid}/deployments"), &admin)
        .await
        .json()
        .await
        .unwrap();
    assert!(list.as_array().unwrap().iter().any(|d| d["id"] == did));

    // get single
    let got: Value = env
        .get(&format!("/projects/{pid}/deployments/{did}"), &admin)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(got["id"], did);
}

#[tokio::test]
async fn deployment_claim() {
    let env = setup().await;
    let admin = env.admin_key.clone();

    let project: Value = env
        .post("/projects", &admin, json!({"name": "claim-test"}))
        .await
        .json()
        .await
        .unwrap();
    let pid = project["id"].as_str().unwrap();

    let dep: Value = env
        .post(
            &format!("/projects/{pid}/deployments"),
            &admin,
            json!({"claimable": true}),
        )
        .await
        .json()
        .await
        .unwrap();
    let token = dep["claim_token"].as_str().unwrap();

    // claim it
    let res = env
        .post(&format!("/claim/{token}"), &admin, json!({}))
        .await;
    assert_eq!(res.status(), 200);
    let claimed: Value = res.json().await.unwrap();
    assert_eq!(claimed["status"], "claimed");

    // claiming again should conflict
    let res2 = env
        .post(&format!("/claim/{token}"), &admin, json!({}))
        .await;
    assert_eq!(res2.status(), 409);
}

#[tokio::test]
async fn deployment_on_missing_project() {
    let env = setup().await;
    let res = env
        .post(
            "/projects/no-such-project/deployments",
            &env.admin_key.clone(),
            json!({"claimable": false}),
        )
        .await;
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn deployment_diff() {
    let env = setup().await;
    let admin = env.admin_key.clone();

    let project: Value = env
        .post("/projects", &admin, json!({"name": "diff-test"}))
        .await
        .json()
        .await
        .unwrap();
    let pid = project["id"].as_str().unwrap();

    let d1: Value = env
        .post(
            &format!("/projects/{pid}/deployments"),
            &admin,
            json!({"git_ref": "v1.0.0"}),
        )
        .await
        .json()
        .await
        .unwrap();
    let d2: Value = env
        .post(
            &format!("/projects/{pid}/deployments"),
            &admin,
            json!({"git_ref": "v2.0.0"}),
        )
        .await
        .json()
        .await
        .unwrap();

    let from = d1["id"].as_str().unwrap();
    let to = d2["id"].as_str().unwrap();

    let res = env
        .client
        .get(env.url(&format!("/projects/{pid}/diff")))
        .bearer_auth(&admin)
        .query(&[("from", from), ("to", to)])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let diff: Value = res.json().await.unwrap();
    assert_eq!(diff["from"], from);
    assert_eq!(diff["to"], to);
    let changes = diff["changes"].as_array().unwrap();
    assert!(changes.iter().any(|c| c["field"] == "git_ref"));
}

// ── API Keys ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn api_key_create_list_revoke() {
    let env = setup().await;
    let admin = env.admin_key.clone();

    // create a new deploy key via API
    let res = env
        .post(
            "/auth/keys",
            &admin,
            json!({"name": "ci-key", "scope": "deploy"}),
        )
        .await;
    assert_eq!(res.status(), 201);
    let body: Value = res.json().await.unwrap();
    assert!(body["key"].as_str().unwrap().starts_with("arx_sk_"));
    let key_id = body["id"].as_str().unwrap().to_string();

    // list
    let list: Value = env.get("/auth/keys", &admin).await.json().await.unwrap();
    let arr = list.as_array().unwrap();
    assert!(arr.iter().any(|k| k["id"] == key_id));

    // revoke
    let del = env.delete(&format!("/auth/keys/{key_id}"), &admin).await;
    assert_eq!(del.status(), 204);

    // revoked key should be forbidden on use
    let new_key = body["key"].as_str().unwrap().to_string();
    let forbidden = env.get("/projects", &new_key).await;
    assert_eq!(forbidden.status(), 403);
}

#[tokio::test]
async fn api_key_ttl_and_scope_returned() {
    let env = setup().await;
    let res = env
        .post(
            "/auth/keys",
            &env.admin_key.clone(),
            json!({"name": "ttl-key", "scope": "read", "ttl_days": 30}),
        )
        .await;
    assert_eq!(res.status(), 201);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["scope"], "read");
    assert!(body["expires_at"].is_string());
}

#[tokio::test]
async fn api_key_invalid_scope_rejected() {
    let env = setup().await;
    let res = env
        .post(
            "/auth/keys",
            &env.admin_key.clone(),
            json!({"name": "bad-scope", "scope": "superuser"}),
        )
        .await;
    assert_eq!(res.status(), 400);
}

// ── Domains ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn domain_add_list_delete() {
    let env = setup().await;
    let admin = env.admin_key.clone();

    let project: Value = env
        .post("/projects", &admin, json!({"name": "domain-proj"}))
        .await
        .json()
        .await
        .unwrap();
    let pid = project["id"].as_str().unwrap();

    // add
    let res = env
        .post(
            &format!("/projects/{pid}/domains"),
            &admin,
            json!({"domain": "app.example.com"}),
        )
        .await;
    assert_eq!(res.status(), 201);
    let domain: Value = res.json().await.unwrap();
    assert_eq!(domain["domain"], "app.example.com");
    assert_eq!(domain["is_verified"], false);
    let did = domain["id"].as_str().unwrap().to_string();

    // list
    let list: Value = env
        .get(&format!("/projects/{pid}/domains"), &admin)
        .await
        .json()
        .await
        .unwrap();
    assert!(list.as_array().unwrap().iter().any(|d| d["id"] == did));

    // delete
    let del = env
        .delete(&format!("/projects/{pid}/domains/{did}"), &admin)
        .await;
    assert_eq!(del.status(), 204);

    let list2: Value = env
        .get(&format!("/projects/{pid}/domains"), &admin)
        .await
        .json()
        .await
        .unwrap();
    assert!(list2.as_array().unwrap().iter().all(|d| d["id"] != did));
}

// ── Env vars ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn env_vars_set_list_delete() {
    let env = setup().await;
    let admin = env.admin_key.clone();

    let project: Value = env
        .post("/projects", &admin, json!({"name": "env-proj"}))
        .await
        .json()
        .await
        .unwrap();
    let pid = project["id"].as_str().unwrap();

    // set
    let res = env
        .put(
            &format!("/projects/{pid}/env"),
            &admin,
            json!({"vars": {"DATABASE_URL": "postgres://localhost/mydb", "PORT": "3000"}}),
        )
        .await;
    assert_eq!(res.status(), 200);
    let updated: Value = res.json().await.unwrap();
    assert_eq!(updated["updated"], 2);

    // list (only keys returned, not values)
    let list: Value = env
        .get(&format!("/projects/{pid}/env"), &admin)
        .await
        .json()
        .await
        .unwrap();
    let arr = list.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert!(arr.iter().any(|v| v["key"] == "DATABASE_URL"));
    assert!(arr.iter().any(|v| v["key"] == "PORT"));

    // delete one
    let del = env
        .delete(&format!("/projects/{pid}/env/PORT"), &admin)
        .await;
    assert_eq!(del.status(), 204);

    let list2: Value = env
        .get(&format!("/projects/{pid}/env"), &admin)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(list2.as_array().unwrap().len(), 1);
    assert!(list2.as_array().unwrap().iter().all(|v| v["key"] != "PORT"));
}

// ── Deploy hooks ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn deploy_hooks_create_list_delete() {
    let env = setup().await;
    let admin = env.admin_key.clone();

    let project: Value = env
        .post("/projects", &admin, json!({"name": "hooks-proj"}))
        .await
        .json()
        .await
        .unwrap();
    let pid = project["id"].as_str().unwrap();

    // create
    let res = env
        .post(
            &format!("/projects/{pid}/hooks"),
            &admin,
            json!({"url": "https://hooks.example.com/deploy", "events": "success", "secret": "s3cr3t"}),
        )
        .await;
    assert_eq!(res.status(), 201);
    let hook: Value = res.json().await.unwrap();
    assert_eq!(hook["events"], "success");
    let hid = hook["id"].as_str().unwrap().to_string();

    // list
    let list: Value = env
        .get(&format!("/projects/{pid}/hooks"), &admin)
        .await
        .json()
        .await
        .unwrap();
    assert!(list.as_array().unwrap().iter().any(|h| h["id"] == hid));

    // delete
    let del = env
        .delete(&format!("/projects/{pid}/hooks/{hid}"), &admin)
        .await;
    assert_eq!(del.status(), 204);

    let list2: Value = env
        .get(&format!("/projects/{pid}/hooks"), &admin)
        .await
        .json()
        .await
        .unwrap();
    assert!(list2.as_array().unwrap().iter().all(|h| h["id"] != hid));
}

// ── Audit logs ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn audit_logs_accessible_to_admin() {
    let env = setup().await;
    let res = env.get("/audit", &env.admin_key.clone()).await;
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn audit_logs_forbidden_to_read_key() {
    let env = setup().await;
    let res = env.get("/audit", &env.read_key.clone()).await;
    assert_eq!(res.status(), 403);
}

// ── Idempotency ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn idempotency_same_key_returns_same_response() {
    let env = setup().await;
    let admin = env.admin_key.clone();
    let ikey = uuid::Uuid::new_v4().to_string();

    let r1 = env
        .post_with_idempotency("/projects", &admin, json!({"name": "idem-proj"}), &ikey)
        .await;
    assert_eq!(r1.status(), 201);
    let b1: Value = r1.json().await.unwrap();

    let r2 = env
        .post_with_idempotency("/projects", &admin, json!({"name": "idem-proj"}), &ikey)
        .await;
    assert_eq!(r2.status(), 201);
    let b2: Value = r2.json().await.unwrap();

    // same idempotency key → same project id
    assert_eq!(b1["id"], b2["id"]);
}
