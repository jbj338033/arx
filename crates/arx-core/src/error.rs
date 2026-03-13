use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("project not found: {0}")]
    ProjectNotFound(String),

    #[error("deployment not found: {0}")]
    DeploymentNotFound(String),

    #[error("project already exists: {0}")]
    ProjectAlreadyExists(String),

    #[error("invalid api key")]
    InvalidApiKey,

    #[error("api key expired")]
    ApiKeyExpired,

    #[error("api key revoked")]
    ApiKeyRevoked,

    #[error("insufficient scope: requires {required}")]
    InsufficientScope { required: String },

    #[error("build failed: {0}")]
    BuildFailed(BuildError),

    #[error("deployment failed: {0}")]
    DeploymentFailed(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Internal(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildError {
    pub code: String,
    pub phase: BuildPhase,
    pub message: String,
    pub suggestion: Option<String>,
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.phase, self.code, self.message)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildPhase {
    Setup,
    Install,
    Build,
    Package,
}

impl std::fmt::Display for BuildPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Setup => write!(f, "setup"),
            Self::Install => write!(f, "install"),
            Self::Build => write!(f, "build"),
            Self::Package => write!(f, "package"),
        }
    }
}
