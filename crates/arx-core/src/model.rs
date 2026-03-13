use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub repo_url: Option<String>,
    pub default_branch: Option<String>,
    pub production_deployment_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub id: String,
    pub project_id: String,
    pub status: DeploymentStatus,
    pub source: DeploymentSource,
    pub git_ref: Option<String>,
    pub git_sha: Option<String>,
    pub image_ref: Option<String>,
    pub container_id: Option<String>,
    pub url: Option<String>,
    pub verification_result: Option<serde_json::Value>,
    pub log_path: Option<String>,
    pub claim_token: Option<String>,
    pub claimed_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus {
    Pending,
    Building,
    Deploying,
    Verifying,
    Live,
    Failed,
}

impl DeploymentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Building => "building",
            Self::Deploying => "deploying",
            Self::Verifying => "verifying",
            Self::Live => "live",
            Self::Failed => "failed",
        }
    }

    pub fn can_transition_to(&self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Pending, Self::Building)
                | (Self::Pending, Self::Deploying) // image deploy skips build
                | (Self::Building, Self::Deploying)
                | (Self::Building, Self::Failed)
                | (Self::Deploying, Self::Verifying)
                | (Self::Deploying, Self::Failed)
                | (Self::Verifying, Self::Live)
                | (Self::Verifying, Self::Failed)
        )
    }
}

impl std::fmt::Display for DeploymentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentSource {
    GitPush,
    ApiUpload,
    Image,
}

impl DeploymentSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GitPush => "git_push",
            Self::ApiUpload => "api_upload",
            Self::Image => "image",
        }
    }
}

impl std::fmt::Display for DeploymentSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub id: String,
    pub project_id: String,
    pub environment: String,
    pub key: String,
    pub encrypted_value: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Domain {
    pub id: String,
    pub project_id: String,
    pub domain: String,
    pub is_verified: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub scope: ApiScope,
    pub allowed_ips: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiScope {
    Admin,
    Deploy,
    DeployProject(String),
    Read,
}

impl ApiScope {
    pub fn can_access(&self, required: &ApiScope) -> bool {
        match self {
            ApiScope::Admin => true,
            ApiScope::Deploy => matches!(required, ApiScope::Deploy | ApiScope::Read),
            ApiScope::DeployProject(proj) => match required {
                ApiScope::DeployProject(req_proj) => proj == req_proj,
                ApiScope::Read => true,
                _ => false,
            },
            ApiScope::Read => matches!(required, ApiScope::Read),
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            Self::Admin => "admin".into(),
            Self::Deploy => "deploy".into(),
            Self::DeployProject(p) => format!("deploy:{p}"),
            Self::Read => "read".into(),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(Self::Admin),
            "deploy" => Some(Self::Deploy),
            "read" => Some(Self::Read),
            s if s.starts_with("deploy:") => Some(Self::DeployProject(s[7..].to_string())),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: String,
    pub api_key_id: String,
    pub action: String,
    pub resource: String,
    pub ip: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub health_check: Option<bool>,
    pub http_status: Option<u16>,
    pub response_time_ms: Option<u64>,
    pub body_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedDatabase {
    pub id: String,
    pub project_id: String,
    pub engine: String,
    pub container_id: Option<String>,
    pub host: String,
    pub port: i64,
    pub database_name: String,
    pub username: String,
    pub password_encrypted: Vec<u8>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployHook {
    pub id: String,
    pub project_id: String,
    pub url: String,
    pub events: String,
    pub secret: Option<String>,
    pub created_at: DateTime<Utc>,
}
