use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArxConfig {
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub deploy: DeployConfig,
    #[serde(default)]
    pub resources: ResourceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    pub command: Option<String>,
    pub output: Option<String>,
    pub dockerfile: Option<String>,
    #[serde(default)]
    pub cache_paths: Vec<String>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            command: None,
            output: None,
            dockerfile: None,
            cache_paths: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    pub health_check: Option<String>,
    #[serde(default = "default_deploy_type")]
    pub r#type: DeployType,
}

impl Default for DeployConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            health_check: None,
            r#type: DeployType::Server,
        }
    }
}

fn default_port() -> u16 {
    3000
}

fn default_deploy_type() -> DeployType {
    DeployType::Server
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployType {
    Server,
    Static,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    #[serde(default = "default_cpu")]
    pub cpu: String,
    #[serde(default = "default_memory")]
    pub memory: String,
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            cpu: default_cpu(),
            memory: default_memory(),
        }
    }
}

fn default_cpu() -> String {
    "0.5".into()
}

fn default_memory() -> String {
    "512m".into()
}

impl ArxConfig {
    pub fn load(dir: &Path) -> Option<Self> {
        let path = dir.join("arx.toml");
        let content = std::fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub data_dir: String,
    pub db_path: String,
    pub log_dir: String,
    pub master_key_path: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".into(),
            port: 8443,
            data_dir: "/var/lib/arx".into(),
            db_path: "/var/lib/arx/arx.db".into(),
            log_dir: "/var/lib/arx/logs".into(),
            master_key_path: "/etc/arx/master.key".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub default: Option<String>,
    #[serde(default)]
    pub servers: std::collections::HashMap<String, ServerCredential>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCredential {
    pub url: String,
    pub key: String,
}

impl Credentials {
    pub fn load() -> Option<Self> {
        let path = dirs_path().join("credentials.toml");
        let content = std::fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }

    pub fn active_server(&self) -> Option<&ServerCredential> {
        let name = self.default.as_deref()?;
        self.servers.get(name)
    }
}

fn dirs_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.config"))
        .join("arx")
}

mod dirs {
    use std::path::PathBuf;

    pub fn config_dir() -> Option<PathBuf> {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".config"))
    }
}
