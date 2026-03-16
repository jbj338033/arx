use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde_json::Value;

use crate::config::Credentials;
use crate::error::Error;

pub struct ArxClient {
    http: reqwest::Client,
    base_url: String,
}

impl ArxClient {
    pub fn new(base_url: &str, api_key: &str) -> Result<Self, Error> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {api_key}"))
                .map_err(|e| Error::Internal(e.to_string()))?,
        );

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| Error::Internal(e.to_string()))?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub fn from_credentials(server: Option<&str>) -> Result<Self, Error> {
        let creds = Credentials::load()
            .ok_or_else(|| Error::Internal("not logged in, run `arx login` first".into()))?;

        let sc = if let Some(name) = server {
            creds.servers.get(name).ok_or_else(|| {
                Error::Internal(format!("server '{name}' not found in credentials"))
            })?
        } else {
            creds
                .active_server()
                .ok_or_else(|| Error::Internal("no default server configured".into()))?
        };

        Self::new(&sc.url, &sc.key)
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.base_url)
    }

    async fn handle_response(&self, resp: reqwest::Response) -> Result<Value, Error> {
        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|e| Error::Internal(format!("invalid response: {e}")))?;

        if !status.is_success() {
            let msg = body["error"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string();
            return Err(Error::Internal(msg));
        }
        Ok(body)
    }

    pub async fn create_project(&self, name: &str, repo_url: Option<&str>) -> Result<Value, Error> {
        let mut body = serde_json::json!({"name": name});
        if let Some(url) = repo_url {
            body["repo_url"] = Value::String(url.into());
        }
        let resp = self
            .http
            .post(self.url("/projects"))
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn list_projects(&self) -> Result<Value, Error> {
        let resp = self
            .http
            .get(self.url("/projects"))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn get_project(&self, id: &str) -> Result<Value, Error> {
        let resp = self
            .http
            .get(self.url(&format!("/projects/{id}")))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn delete_project(&self, id: &str) -> Result<(), Error> {
        let resp = self
            .http
            .delete(self.url(&format!("/projects/{id}")))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        if !resp.status().is_success() {
            let body: Value = resp.json().await.unwrap_or_default();
            let msg = body["error"]
                .as_str()
                .unwrap_or("delete failed")
                .to_string();
            return Err(Error::Internal(msg));
        }
        Ok(())
    }

    pub async fn deploy_image(&self, project_id: &str, image: &str) -> Result<Value, Error> {
        let body = serde_json::json!({"image_ref": image});
        let resp = self
            .http
            .post(self.url(&format!("/projects/{project_id}/deployments")))
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn list_deployments(&self, project_id: &str) -> Result<Value, Error> {
        let resp = self
            .http
            .get(self.url(&format!("/projects/{project_id}/deployments")))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn get_deployment(
        &self,
        project_id: &str,
        deployment_id: &str,
    ) -> Result<Value, Error> {
        let resp = self
            .http
            .get(self.url(&format!(
                "/projects/{project_id}/deployments/{deployment_id}"
            )))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn rollback(&self, project_id: &str, deployment_id: &str) -> Result<Value, Error> {
        let resp = self
            .http
            .post(self.url(&format!(
                "/projects/{project_id}/deployments/{deployment_id}/rollback"
            )))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn add_domain(&self, project_id: &str, domain: &str) -> Result<Value, Error> {
        let body = serde_json::json!({"domain": domain});
        let resp = self
            .http
            .post(self.url(&format!("/projects/{project_id}/domains")))
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn list_domains(&self, project_id: &str) -> Result<Value, Error> {
        let resp = self
            .http
            .get(self.url(&format!("/projects/{project_id}/domains")))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn delete_domain(&self, project_id: &str, domain_id: &str) -> Result<(), Error> {
        let resp = self
            .http
            .delete(self.url(&format!("/projects/{project_id}/domains/{domain_id}")))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(Error::Internal("domain delete failed".into()));
        }
        Ok(())
    }

    pub async fn create_api_key(
        &self,
        name: &str,
        scope: &str,
        ttl_days: Option<i64>,
    ) -> Result<Value, Error> {
        let body = serde_json::json!({
            "name": name,
            "scope": scope,
            "ttl_days": ttl_days,
        });
        let resp = self
            .http
            .post(self.url("/auth/keys"))
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn list_api_keys(&self) -> Result<Value, Error> {
        let resp = self
            .http
            .get(self.url("/auth/keys"))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn revoke_api_key(&self, id: &str) -> Result<(), Error> {
        let resp = self
            .http
            .delete(self.url(&format!("/auth/keys/{id}")))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(Error::Internal("revoke failed".into()));
        }
        Ok(())
    }

    pub async fn set_env_var(
        &self,
        project_id: &str,
        key: &str,
        value: &str,
    ) -> Result<Value, Error> {
        let body = serde_json::json!({"vars": {key: value}});
        let resp = self
            .http
            .put(self.url(&format!("/projects/{project_id}/env")))
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn list_env_vars(&self, project_id: &str) -> Result<Value, Error> {
        let resp = self
            .http
            .get(self.url(&format!("/projects/{project_id}/env")))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn delete_env_var(&self, project_id: &str, key: &str) -> Result<(), Error> {
        let resp = self
            .http
            .delete(self.url(&format!("/projects/{project_id}/env/{key}")))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        if !resp.status().is_success() {
            let body: Value = resp.json().await.unwrap_or_default();
            let msg = body["error"]
                .as_str()
                .unwrap_or("delete failed")
                .to_string();
            return Err(Error::Internal(msg));
        }
        Ok(())
    }

    pub async fn stream_logs(&self, project_id: &str, deployment_id: &str) -> Result<(), Error> {
        let resp = self
            .http
            .get(self.url(&format!(
                "/projects/{project_id}/deployments/{deployment_id}/logs"
            )))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;

        if !resp.status().is_success() {
            let body: Value = resp.json().await.unwrap_or_default();
            let msg = body["error"].as_str().unwrap_or("logs failed").to_string();
            return Err(Error::Internal(msg));
        }

        let mut stream = resp.bytes_stream();
        use tokio_stream::StreamExt;
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| Error::Internal(e.to_string()))?;
            let text = String::from_utf8_lossy(&bytes);
            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    println!("{data}");
                }
            }
        }
        Ok(())
    }

    pub async fn list_audit_logs(&self) -> Result<Value, Error> {
        let resp = self
            .http
            .get(self.url("/audit"))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn create_database(
        &self,
        project_id: &str,
        engine: &str,
        name: Option<&str>,
    ) -> Result<Value, Error> {
        let mut body = serde_json::json!({"engine": engine});
        if let Some(n) = name {
            body["name"] = Value::String(n.into());
        }
        let resp = self
            .http
            .post(self.url(&format!("/projects/{project_id}/databases")))
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn list_databases(&self, project_id: &str) -> Result<Value, Error> {
        let resp = self
            .http
            .get(self.url(&format!("/projects/{project_id}/databases")))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn delete_database(&self, project_id: &str, db_id: &str) -> Result<(), Error> {
        let resp = self
            .http
            .delete(self.url(&format!("/projects/{project_id}/databases/{db_id}")))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        if !resp.status().is_success() {
            let body: Value = resp.json().await.unwrap_or_default();
            let msg = body["error"]
                .as_str()
                .unwrap_or("delete failed")
                .to_string();
            return Err(Error::Internal(msg));
        }
        Ok(())
    }

    pub async fn create_deploy_hook(
        &self,
        project_id: &str,
        url: &str,
        events: Option<&str>,
        secret: Option<&str>,
    ) -> Result<Value, Error> {
        let mut body = serde_json::json!({"url": url});
        if let Some(e) = events {
            body["events"] = Value::String(e.into());
        }
        if let Some(s) = secret {
            body["secret"] = Value::String(s.into());
        }
        let resp = self
            .http
            .post(self.url(&format!("/projects/{project_id}/hooks")))
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn list_deploy_hooks(&self, project_id: &str) -> Result<Value, Error> {
        let resp = self
            .http
            .get(self.url(&format!("/projects/{project_id}/hooks")))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn delete_deploy_hook(&self, project_id: &str, hook_id: &str) -> Result<(), Error> {
        let resp = self
            .http
            .delete(self.url(&format!("/projects/{project_id}/hooks/{hook_id}")))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(Error::Internal("hook delete failed".into()));
        }
        Ok(())
    }

    pub async fn deployment_diff(
        &self,
        project_id: &str,
        from: &str,
        to: &str,
    ) -> Result<Value, Error> {
        let resp = self
            .http
            .get(self.url(&format!("/projects/{project_id}/diff?from={from}&to={to}")))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }

    pub async fn health(&self) -> Result<Value, Error> {
        let resp = self
            .http
            .get(self.url("/health"))
            .send()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;
        self.handle_response(resp).await
    }
}
