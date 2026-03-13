use bollard::image::BuildImageOptions;
use bollard::Docker;
use std::path::Path;
use tokio::sync::Semaphore;
use tokio_stream::StreamExt;

use arx_core::error::{BuildError, BuildPhase, Error};

static BUILD_SEMAPHORE: Semaphore = Semaphore::const_new(2);

pub struct BuildEngine {
    docker: Docker,
}

impl BuildEngine {
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }

    pub async fn build_dockerfile(
        &self,
        context_path: &Path,
        dockerfile: &str,
        image_tag: &str,
    ) -> Result<String, Error> {
        let _permit = BUILD_SEMAPHORE
            .acquire()
            .await
            .map_err(|e| Error::Internal(format!("build queue error: {e}")))?;

        tracing::info!(image_tag, "building dockerfile");

        let tar = create_tar_context(context_path)?;

        let options = BuildImageOptions {
            t: image_tag,
            dockerfile,
            rm: true,
            ..Default::default()
        };

        let mut stream = self
            .docker
            .build_image(options, None, Some(tar.into()));

        let mut last_error = None;

        while let Some(result) = stream.next().await {
            match result {
                Ok(output) => {
                    if let Some(stream_str) = &output.stream {
                        tracing::debug!("{}", stream_str.trim());
                    }
                    if let Some(err) = &output.error {
                        last_error = Some(err.clone());
                    }
                }
                Err(e) => {
                    return Err(Error::BuildFailed(parse_build_error(
                        &e.to_string(),
                        BuildPhase::Build,
                    )));
                }
            }
        }

        if let Some(err) = last_error {
            return Err(Error::BuildFailed(parse_build_error(&err, BuildPhase::Build)));
        }

        Ok(image_tag.to_string())
    }
}

fn create_tar_context(path: &Path) -> Result<Vec<u8>, Error> {
    let mut ar = tar::Builder::new(Vec::new());
    ar.append_dir_all(".", path)
        .map_err(|e| Error::Internal(format!("tar context failed: {e}")))?;
    ar.into_inner()
        .map_err(|e| Error::Internal(format!("tar finalize failed: {e}")))
}

fn parse_build_error(msg: &str, phase: BuildPhase) -> BuildError {
    let lower = msg.to_lowercase();

    let (code, suggestion) = if lower.contains("no such file or directory") {
        (
            "FILE_NOT_FOUND".to_string(),
            Some("check that the Dockerfile path and referenced files exist".to_string()),
        )
    } else if lower.contains("command not found") || lower.contains("not found: exec") {
        (
            "COMMAND_NOT_FOUND".to_string(),
            Some("ensure required build tools are installed in the base image".to_string()),
        )
    } else if lower.contains("npm err") || lower.contains("yarn error") {
        (
            "DEPENDENCY_INSTALL".to_string(),
            Some("check package.json for missing or incompatible dependencies".to_string()),
        )
    } else if lower.contains("out of memory") || lower.contains("oom") {
        (
            "OUT_OF_MEMORY".to_string(),
            Some("increase memory limit in arx.toml [resources]".to_string()),
        )
    } else if lower.contains("permission denied") {
        (
            "PERMISSION_DENIED".to_string(),
            Some("check file permissions in the build context".to_string()),
        )
    } else {
        ("BUILD_ERROR".to_string(), None)
    };

    BuildError {
        code,
        phase,
        message: msg.to_string(),
        suggestion,
    }
}
