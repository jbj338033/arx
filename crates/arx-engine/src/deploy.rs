use arx_core::config::ArxConfig;
use arx_core::error::Error;
use arx_core::model::{Deployment, VerificationResult};

use crate::build::BuildEngine;
use crate::container::ContainerManager;

pub struct DeployEngine {
    pub containers: ContainerManager,
    pub builder: BuildEngine,
}

impl DeployEngine {
    pub fn new() -> Result<Self, Error> {
        let containers = ContainerManager::new()?;
        let builder = BuildEngine::new(containers.docker().clone());
        Ok(Self {
            containers,
            builder,
        })
    }

    pub async fn build_and_deploy(
        &self,
        deployment: &Deployment,
        context_path: &std::path::Path,
        env: Vec<String>,
        config: &ArxConfig,
    ) -> Result<DeployResult, Error> {
        let short_id = truncate_id(&deployment.id, 8);
        let image_tag = format!("arx-build-{}-{}", deployment.project_id, short_id);

        let dockerfile = config.build.dockerfile.as_deref().unwrap_or("Dockerfile");

        self.builder
            .build_dockerfile(context_path, dockerfile, &image_tag)
            .await?;

        self.deploy_image(deployment, &image_tag, env, config).await
    }

    pub async fn deploy_image(
        &self,
        deployment: &Deployment,
        image: &str,
        env: Vec<String>,
        config: &ArxConfig,
    ) -> Result<DeployResult, Error> {
        tracing::info!(deployment_id = %deployment.id, image, "pulling image");
        self.containers.pull_image(image).await?;

        let short_id = truncate_id(&deployment.id, 8);
        let container_name = format!("arx-{}-{}", deployment.project_id, short_id);
        let network_name = format!("arx-{}", deployment.project_id);
        self.containers.create_network(&network_name).await?;

        let port = config.deploy.port;
        let cpu = parse_cpu(&config.resources.cpu);
        let memory = parse_memory(&config.resources.memory);

        tracing::info!(deployment_id = %deployment.id, "starting container");
        let container_id = self
            .containers
            .create_and_start(
                &container_name,
                image,
                port,
                env,
                cpu,
                memory,
                Some(&network_name),
                None,
            )
            .await?;

        let host_port = self.containers.get_host_port(&container_id, port).await?;

        Ok(DeployResult {
            container_id,
            host_port,
        })
    }

    pub async fn verify(
        &self,
        container_id: &str,
        host_port: u16,
        health_path: Option<&str>,
    ) -> VerificationResult {
        let base = format!("http://127.0.0.1:{host_port}");
        let url = match health_path {
            Some(path) => format!("{base}{path}"),
            None => base,
        };

        for _ in 0..30 {
            if self.containers.is_running(container_id).await {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        let start = std::time::Instant::now();
        match reqwest::get(&url).await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                let preview = if body.len() > 200 {
                    let mut end = 200;
                    while !body.is_char_boundary(end) {
                        end -= 1;
                    }
                    format!("{}...", &body[..end])
                } else {
                    body
                };
                VerificationResult {
                    health_check: Some((200..400).contains(&status)),
                    http_status: Some(status),
                    response_time_ms: Some(start.elapsed().as_millis() as u64),
                    body_preview: Some(preview),
                }
            }
            Err(e) => {
                tracing::warn!("verification request failed: {e}");
                VerificationResult {
                    health_check: Some(false),
                    http_status: None,
                    response_time_ms: Some(start.elapsed().as_millis() as u64),
                    body_preview: Some(e.to_string()),
                }
            }
        }
    }

    pub async fn stop_previous(&self, container_id: &str) -> Result<(), Error> {
        tracing::info!(container_id, "stopping previous container");
        self.containers.stop_and_remove(container_id).await
    }
}

pub struct DeployResult {
    pub container_id: String,
    pub host_port: u16,
}

fn parse_cpu(s: &str) -> Option<i64> {
    s.parse::<f64>().ok().map(|v| (v * 1_000_000_000.0) as i64)
}

fn truncate_id(id: &str, max: usize) -> &str {
    &id[..id.len().min(max)]
}

fn parse_memory(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix('m') {
        num.parse::<i64>().ok().map(|v| v * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix('g') {
        num.parse::<i64>().ok().map(|v| v * 1024 * 1024 * 1024)
    } else {
        s.parse::<i64>().ok()
    }
}
