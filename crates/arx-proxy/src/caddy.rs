use arx_core::error::Error;
use serde_json::json;

pub struct CaddyClient {
    base_url: String,
    client: reqwest::Client,
}

impl CaddyClient {
    pub fn new(admin_url: &str) -> Self {
        Self {
            base_url: admin_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn add_route(&self, domain: &str, upstream: &str) -> Result<(), Error> {
        let route = json!({
            "@id": format!("arx-{domain}"),
            "match": [{"host": [domain]}],
            "handle": [{
                "handler": "reverse_proxy",
                "upstreams": [{"dial": upstream}]
            }],
            "terminal": true
        });

        let url = format!("{}/config/apps/http/servers/arx/routes", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&route)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("caddy api error: {e}")))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Internal(format!("caddy route add failed: {body}")));
        }
        Ok(())
    }

    pub async fn remove_route(&self, domain: &str) -> Result<(), Error> {
        let url = format!("{}/id/arx-{domain}", self.base_url);
        let resp = self
            .client
            .delete(&url)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("caddy api error: {e}")))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Internal(format!(
                "caddy route remove failed: {body}"
            )));
        }
        Ok(())
    }

    pub async fn update_upstream(&self, domain: &str, upstream: &str) -> Result<(), Error> {
        let _ = self.remove_route(domain).await;
        self.add_route(domain, upstream).await
    }

    pub async fn ensure_server(&self) -> Result<(), Error> {
        let config = json!({
            "apps": {
                "http": {
                    "servers": {
                        "arx": {
                            "listen": [":443", ":80"],
                            "routes": []
                        }
                    }
                }
            }
        });

        let url = format!("{}/load", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&config)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("caddy api error: {e}")))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!("caddy server init: {body}");
        }
        Ok(())
    }
}
