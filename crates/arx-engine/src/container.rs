use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::HostConfig;
use bollard::Docker;
use std::collections::HashMap;
use tokio_stream::StreamExt;

use arx_core::error::Error;

pub struct ContainerManager {
    docker: Docker,
}

impl ContainerManager {
    pub fn new() -> Result<Self, Error> {
        let docker =
            Docker::connect_with_local_defaults().map_err(|e| Error::Internal(e.to_string()))?;
        Ok(Self { docker })
    }

    pub fn docker(&self) -> &Docker {
        &self.docker
    }

    pub async fn pull_image(&self, image: &str) -> Result<(), Error> {
        let opts = CreateImageOptions {
            from_image: image,
            ..Default::default()
        };

        let mut stream = self.docker.create_image(Some(opts), None, None);
        while let Some(result) = stream.next().await {
            result.map_err(|e| Error::Internal(format!("image pull failed: {e}")))?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_and_start(
        &self,
        name: &str,
        image: &str,
        port: u16,
        env: Vec<String>,
        cpu: Option<i64>,
        memory: Option<i64>,
        network: Option<&str>,
        volumes: Option<Vec<String>>,
    ) -> Result<String, Error> {
        let mut port_bindings = HashMap::new();
        port_bindings.insert(
            format!("{port}/tcp"),
            Some(vec![bollard::models::PortBinding {
                host_ip: Some("127.0.0.1".into()),
                host_port: Some("0".into()), // random host port
            }]),
        );

        let host_config = HostConfig {
            port_bindings: Some(port_bindings),
            nano_cpus: cpu.map(|c| c * 1_000_000_000),
            memory,
            network_mode: network.map(String::from),
            binds: volumes,
            restart_policy: Some(bollard::models::RestartPolicy {
                name: Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED),
                maximum_retry_count: None,
            }),
            ..Default::default()
        };

        let mut exposed_ports = HashMap::new();
        exposed_ports.insert(format!("{port}/tcp"), HashMap::new());

        let config = Config {
            image: Some(image.to_string()),
            env: Some(env),
            exposed_ports: Some(exposed_ports),
            host_config: Some(host_config),
            ..Default::default()
        };

        let opts = CreateContainerOptions {
            name,
            platform: None,
        };

        let response = self
            .docker
            .create_container(Some(opts), config)
            .await
            .map_err(|e| Error::DeploymentFailed(format!("container create failed: {e}")))?;

        self.docker
            .start_container(&response.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| Error::DeploymentFailed(format!("container start failed: {e}")))?;

        Ok(response.id)
    }

    pub async fn stop_and_remove(&self, container_id: &str) -> Result<(), Error> {
        let _ = self
            .docker
            .stop_container(container_id, Some(StopContainerOptions { t: 10 }))
            .await;

        self.docker
            .remove_container(
                container_id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| Error::Internal(format!("container remove failed: {e}")))?;

        Ok(())
    }

    pub async fn get_host_port(&self, container_id: &str, port: u16) -> Result<u16, Error> {
        let info = self
            .docker
            .inspect_container(container_id, None)
            .await
            .map_err(|e| Error::Internal(format!("inspect failed: {e}")))?;

        let bindings = info
            .network_settings
            .and_then(|ns| ns.ports)
            .ok_or_else(|| Error::Internal("no port bindings found".into()))?;

        let key = format!("{port}/tcp");
        let host_port = bindings
            .get(&key)
            .and_then(|v| v.as_ref())
            .and_then(|v| v.first())
            .and_then(|b| b.host_port.as_ref())
            .and_then(|p| p.parse::<u16>().ok())
            .ok_or_else(|| Error::Internal(format!("port {port} not bound")))?;

        Ok(host_port)
    }

    pub async fn is_running(&self, container_id: &str) -> bool {
        self.docker
            .inspect_container(container_id, None)
            .await
            .map(|info| info.state.and_then(|s| s.running).unwrap_or(false))
            .unwrap_or(false)
    }

    pub async fn create_network(&self, name: &str) -> Result<(), Error> {
        let config = bollard::network::CreateNetworkOptions {
            name: name.to_string(),
            driver: "bridge".to_string(),
            ..Default::default()
        };
        let _ = self.docker.create_network(config).await;
        Ok(())
    }
}
