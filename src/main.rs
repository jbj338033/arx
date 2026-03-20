use arx_core::client::ArxClient;
use arx_core::error::Error;
use arx_core::output::{print_error, OutputMode};
use clap::{Parser, Subcommand};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Parser)]
#[command(name = "arx", about = "Agent-first self-hosted deployment platform")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, global = true)]
    server: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the arx server
    Server {
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
        #[arg(long, default_value_t = 8443)]
        port: u16,
        #[arg(long, default_value = "/var/lib/arx/arx.db")]
        db: String,
    },

    /// Deploy a project
    Deploy {
        #[arg(long)]
        project: String,
        #[arg(long)]
        image: Option<String>,
    },

    /// Manage projects
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },

    /// Manage environment variables
    Env {
        #[command(subcommand)]
        command: EnvCommands,
    },

    /// View deployment logs
    Logs {
        project: String,
        #[arg(long)]
        follow: bool,
    },

    /// Rollback to previous deployment
    Rollback { project: String },

    /// Manage custom domains
    Domain {
        #[command(subcommand)]
        command: DomainCommands,
    },

    /// Manage API keys
    Auth {
        #[command(subcommand)]
        command: AuthCommands,
    },

    /// View audit logs
    Audit,

    /// Manage project databases
    Db {
        #[command(subcommand)]
        command: DbCommands,
    },

    /// Compare two deployments
    Diff {
        project: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },

    /// Admin commands (run on server)
    Admin {
        #[command(subcommand)]
        command: AdminCommands,
    },

    /// Get project status
    Status { project: String },

    /// Authenticate with an arx server
    Login {
        #[arg(long)]
        url: String,
        #[arg(long)]
        key: String,
        #[arg(long)]
        name: Option<String>,
    },

    /// Start MCP server (stdio)
    Mcp,
}

#[derive(Subcommand)]
enum ProjectCommands {
    Create {
        name: String,
        #[arg(long)]
        repo: Option<String>,
    },
    List,
    Info {
        name: String,
    },
    Delete {
        name: String,
    },
}

#[derive(Subcommand)]
enum EnvCommands {
    Set {
        project: String,
        key: String,
        value: String,
    },
    Get {
        project: String,
        key: String,
    },
    Delete {
        project: String,
        key: String,
    },
}

#[derive(Subcommand)]
enum DomainCommands {
    Add { project: String, domain: String },
    List { project: String },
    Remove { project: String, domain: String },
}

#[derive(Subcommand)]
enum AuthCommands {
    Create {
        name: String,
        #[arg(long, default_value = "deploy")]
        scope: String,
        #[arg(long)]
        ttl: Option<i64>,
    },
    List,
    Revoke {
        id: String,
    },
}

#[derive(Subcommand)]
enum DbCommands {
    Create {
        engine: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: Option<String>,
    },
    List {
        project: String,
    },
    Delete {
        id: String,
        #[arg(long)]
        project: String,
    },
}

#[derive(Subcommand)]
enum AdminCommands {
    InitialPassword,
    ResetKey,
    Doctor,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "arx=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();
    let output = OutputMode::detect();

    let result = match cli.command {
        Commands::Server { host, port, db } => run_server(&host, port, &db).await,
        Commands::Admin { command } => run_admin(command).await,
        Commands::Login { url, key, name } => run_login(&url, &key, name.as_deref()).await,
        Commands::Mcp => run_mcp().await,
        cmd => run_client_command(cmd, cli.server.as_deref(), output).await,
    };

    if let Err(e) = result {
        print_error(output, &e);
        std::process::exit(1);
    }
}

async fn run_client_command(
    cmd: Commands,
    server: Option<&str>,
    output: OutputMode,
) -> Result<(), Error> {
    let client = ArxClient::from_credentials(server)?;

    match cmd {
        Commands::Deploy { project, image } => {
            let result = if let Some(img) = &image {
                client.deploy_image(&project, img).await?
            } else {
                return Err(Error::Internal(
                    "source deploy not yet supported, use --image".into(),
                ));
            };
            print_json(output, &result);
        }

        Commands::Project { command } => match command {
            ProjectCommands::Create { name, repo } => {
                let result = client.create_project(&name, repo.as_deref()).await?;
                print_json(output, &result);
            }
            ProjectCommands::List => {
                let result = client.list_projects().await?;
                print_json(output, &result);
            }
            ProjectCommands::Info { name } => {
                let result = client.get_project(&name).await?;
                print_json(output, &result);
            }
            ProjectCommands::Delete { name } => {
                client.delete_project(&name).await?;
                print_msg(output, "project deleted");
            }
        },

        Commands::Domain { command } => match command {
            DomainCommands::Add { project, domain } => {
                let result = client.add_domain(&project, &domain).await?;
                print_json(output, &result);
            }
            DomainCommands::List { project } => {
                let result = client.list_domains(&project).await?;
                print_json(output, &result);
            }
            DomainCommands::Remove { project, domain } => {
                client.delete_domain(&project, &domain).await?;
                print_msg(output, "domain removed");
            }
        },

        Commands::Auth { command } => match command {
            AuthCommands::Create { name, scope, ttl } => {
                let result = client.create_api_key(&name, &scope, ttl).await?;
                print_json(output, &result);
            }
            AuthCommands::List => {
                let result = client.list_api_keys().await?;
                print_json(output, &result);
            }
            AuthCommands::Revoke { id } => {
                client.revoke_api_key(&id).await?;
                print_msg(output, "key revoked");
            }
        },

        Commands::Env { command } => match command {
            EnvCommands::Set {
                project,
                key,
                value,
            } => {
                let result = client.set_env_var(&project, &key, &value).await?;
                print_json(output, &result);
            }
            EnvCommands::Get { project, key: _ } => {
                let result = client.list_env_vars(&project).await?;
                print_json(output, &result);
            }
            EnvCommands::Delete { project, key } => {
                client.delete_env_var(&project, &key).await?;
                print_msg(output, "env var deleted");
            }
        },

        Commands::Logs { project, follow: _ } => {
            let proj = client.get_project(&project).await?;
            let dep_id = proj["production_deployment_id"]
                .as_str()
                .ok_or_else(|| Error::Internal("no active deployment".into()))?;
            let proj_id = proj["id"]
                .as_str()
                .ok_or_else(|| Error::Internal("invalid project".into()))?;
            client.stream_logs(proj_id, dep_id).await?;
        }

        Commands::Rollback { project } => {
            let proj = client.get_project(&project).await?;
            let proj_id = proj["id"]
                .as_str()
                .ok_or_else(|| Error::Internal("invalid project".into()))?;
            let deployments = client.list_deployments(proj_id).await?;
            let prev = deployments
                .as_array()
                .and_then(|arr| arr.iter().find(|d| d["status"] == "live"))
                .and_then(|d| d["id"].as_str())
                .ok_or_else(|| Error::Internal("no previous deployment to rollback to".into()))?;
            let result = client.rollback(proj_id, prev).await?;
            print_json(output, &result);
        }

        Commands::Audit => {
            let result = client.list_audit_logs().await?;
            print_json(output, &result);
        }

        Commands::Status { project } => {
            let result = client.get_project(&project).await?;
            print_json(output, &result);
        }

        Commands::Db { command } => match command {
            DbCommands::Create {
                engine,
                project,
                name,
            } => {
                let result = client
                    .create_database(&project, &engine, name.as_deref())
                    .await?;
                print_json(output, &result);
            }
            DbCommands::List { project } => {
                let result = client.list_databases(&project).await?;
                print_json(output, &result);
            }
            DbCommands::Delete { id, project } => {
                client.delete_database(&project, &id).await?;
                print_msg(output, "database deleted");
            }
        },

        Commands::Diff { project, from, to } => {
            let result = client.deployment_diff(&project, &from, &to).await?;
            print_json(output, &result);
        }

        _ => return Err(Error::Internal("command not yet implemented".into())),
    }

    Ok(())
}

fn print_json(output: OutputMode, value: &serde_json::Value) {
    match output {
        OutputMode::Human => {
            println!(
                "{}",
                serde_json::to_string_pretty(value).unwrap_or_default()
            );
        }
        OutputMode::Json => {
            println!("{}", serde_json::to_string(value).unwrap_or_default());
        }
    }
}

fn print_msg(output: OutputMode, msg: &str) {
    match output {
        OutputMode::Human => println!("{msg}"),
        OutputMode::Json => println!("{}", serde_json::json!({"message": msg})),
    }
}

async fn run_server(host: &str, port: u16, db_path: &str) -> Result<(), Error> {
    if let Some(parent) = std::path::Path::new(db_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    ensure_master_key()?;
    let pool = arx_core::db::connect(db_path).await?;
    ensure_initial_admin_key(&pool).await?;
    arx_api::server::run(pool, host, port).await
}

fn ensure_master_key() -> Result<(), Error> {
    let key_path = std::path::Path::new("/etc/arx/master.key");
    if key_path.exists() {
        return Ok(());
    }

    let rng = ring::rand::SystemRandom::new();
    let mut key_bytes = [0u8; 32];
    ring::rand::SecureRandom::fill(&rng, &mut key_bytes)
        .map_err(|e| Error::Internal(format!("rng failed: {e}")))?;
    let key_hex = hex::encode(key_bytes);

    let key_dir = key_path.parent().unwrap();
    if !key_dir.exists() {
        std::fs::create_dir_all(key_dir)?;
    }

    std::fs::write(key_path, &key_hex)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600));
    }

    tracing::info!("master key generated at {}", key_path.display());
    Ok(())
}

async fn ensure_initial_admin_key(pool: &sqlx::SqlitePool) -> Result<(), Error> {
    let keys = arx_core::db::list_api_keys(pool).await?;
    if !keys.is_empty() {
        return Ok(());
    }

    let raw_key = arx_api::auth::generate_api_key();
    let key_hash = arx_api::auth::hash_key(&raw_key);
    let key_prefix = raw_key[..raw_key.len().min(15)].to_string();

    let api_key = arx_core::model::ApiKey {
        id: uuid::Uuid::new_v4().to_string(),
        name: "initial-admin".into(),
        key_hash,
        key_prefix,
        scope: arx_core::model::ApiScope::Admin,
        allowed_ips: None,
        expires_at: None,
        last_used_at: None,
        revoked_at: None,
        created_at: chrono::Utc::now(),
    };

    arx_core::db::create_api_key(pool, &api_key).await?;

    let key_dir = std::path::Path::new("/etc/arx");
    if key_dir.exists() || std::fs::create_dir_all(key_dir).is_ok() {
        let key_path = key_dir.join("initial-admin-key");
        if std::fs::write(&key_path, &raw_key).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
            }
            tracing::info!("initial admin key written to {}", key_path.display());
        }
    }

    tracing::info!(
        "initial admin key created (prefix: {}...)",
        &raw_key[..raw_key.len().min(15)]
    );
    Ok(())
}

async fn run_admin(command: AdminCommands) -> Result<(), Error> {
    match command {
        AdminCommands::InitialPassword => {
            let path = std::path::Path::new("/etc/arx/initial-admin-key");
            match std::fs::read_to_string(path) {
                Ok(key) => {
                    println!("{key}");
                    Ok(())
                }
                Err(_) => Err(Error::Internal(format!(
                    "no initial admin key found at {}",
                    path.display()
                ))),
            }
        }
        AdminCommands::ResetKey => {
            let db_path =
                std::env::var("ARX_DB_PATH").unwrap_or_else(|_| "/var/lib/arx/arx.db".into());
            let pool = arx_core::db::connect(&db_path).await?;

            let keys = arx_core::db::list_api_keys(&pool).await?;
            for key in &keys {
                if key.scope == arx_core::model::ApiScope::Admin {
                    arx_core::db::revoke_api_key(&pool, &key.id).await?;
                }
            }

            let raw_key = arx_api::auth::generate_api_key();
            let key_hash = arx_api::auth::hash_key(&raw_key);
            let key_prefix = raw_key[..raw_key.len().min(15)].to_string();

            let api_key = arx_core::model::ApiKey {
                id: uuid::Uuid::new_v4().to_string(),
                name: "admin-reset".into(),
                key_hash,
                key_prefix,
                scope: arx_core::model::ApiScope::Admin,
                allowed_ips: None,
                expires_at: None,
                last_used_at: None,
                revoked_at: None,
                created_at: chrono::Utc::now(),
            };

            arx_core::db::create_api_key(&pool, &api_key).await?;

            let key_path = std::path::Path::new("/etc/arx/initial-admin-key");
            if let Some(parent) = key_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(key_path, &raw_key)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600));
            }

            println!("{raw_key}");
            Ok(())
        }
        AdminCommands::Doctor => run_doctor().await,
    }
}

async fn run_doctor() -> Result<(), Error> {
    let db_path = std::env::var("ARX_DB_PATH").unwrap_or_else(|_| "/var/lib/arx/arx.db".into());
    let mut checks: Vec<(&str, bool, String)> = Vec::new();

    let docker_ok = match bollard::Docker::connect_with_local_defaults() {
        Ok(d) => d.ping().await.is_ok(),
        Err(_) => false,
    };
    checks.push((
        "docker daemon",
        docker_ok,
        if docker_ok {
            "connected".into()
        } else {
            "cannot connect to docker".into()
        },
    ));

    let pool = arx_core::db::connect(&db_path).await;
    let db_ok = pool.is_ok();
    checks.push((
        "sqlite database",
        db_ok,
        if db_ok {
            format!("ok ({db_path})")
        } else {
            format!("cannot open {db_path}")
        },
    ));

    let key_path = std::path::Path::new("/etc/arx/master.key");
    let key_ok = key_path.exists();
    let key_msg = if key_ok {
        let perms = std::fs::metadata(key_path)
            .map(|m| format!("{:o}", m.permissions().mode()))
            .unwrap_or_default();
        format!("exists (mode: {perms})")
    } else {
        "missing".into()
    };
    checks.push(("master key", key_ok, key_msg));

    let caddy_url = std::env::var("CADDY_ADMIN_URL");
    let caddy_ok = if let Ok(ref url) = caddy_url {
        reqwest::get(url).await.is_ok()
    } else {
        false
    };
    checks.push((
        "caddy proxy",
        caddy_ok || caddy_url.is_err(),
        if caddy_url.is_err() {
            "not configured (optional)".into()
        } else if caddy_ok {
            "connected".into()
        } else {
            "configured but unreachable".into()
        },
    ));

    if let Ok(ref pool) = pool {
        let failed = sqlx::query_as::<_, (String, String, String)>(
            "SELECT id, project_id, created_at FROM deployments WHERE status = 'failed' ORDER BY created_at DESC LIMIT 5"
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        if failed.is_empty() {
            checks.push(("recent failures", true, "none".into()));
        } else {
            let msg = failed
                .iter()
                .map(|(id, pid, ts)| {
                    format!(
                        "{} (project: {}, at: {})",
                        &id[..id.len().min(8)],
                        &pid[..pid.len().min(8)],
                        ts
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            checks.push(("recent failures", false, msg));
        }

        if docker_ok {
            let docker = bollard::Docker::connect_with_local_defaults().unwrap();
            let containers = docker
                .list_containers::<String>(Some(bollard::container::ListContainersOptions {
                    filters: std::collections::HashMap::from([(
                        "name".into(),
                        vec!["arx-".into()],
                    )]),
                    ..Default::default()
                }))
                .await
                .unwrap_or_default();
            let running_ids: std::collections::HashSet<String> =
                containers.iter().filter_map(|c| c.id.clone()).collect();

            let db_containers = sqlx::query_as::<_, (String,)>(
                "SELECT container_id FROM deployments WHERE container_id IS NOT NULL AND status = 'live'"
            )
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            let db_ids: std::collections::HashSet<String> =
                db_containers.into_iter().map(|r| r.0).collect();

            let orphaned = running_ids.difference(&db_ids).count();
            let missing = db_ids.difference(&running_ids).count();
            let consistent = orphaned == 0 && missing == 0;
            let msg = if consistent {
                "consistent".into()
            } else {
                format!("{orphaned} orphaned containers, {missing} missing containers")
            };
            checks.push(("container consistency", consistent, msg));
        }
    }

    println!("\narx doctor\n{}\n", "=".repeat(40));
    for (name, ok, msg) in &checks {
        let status = if *ok { "PASS" } else { "FAIL" };
        println!("  [{status}] {name}: {msg}");
    }
    println!();

    let all_ok = checks.iter().all(|(_, ok, _)| *ok);
    if !all_ok {
        println!("some checks failed, review the output above");
    } else {
        println!("all checks passed");
    }

    Ok(())
}

async fn run_login(url: &str, key: &str, name: Option<&str>) -> Result<(), Error> {
    let client = ArxClient::new(url, key)?;
    client
        .health()
        .await
        .map_err(|_| Error::Internal(format!("cannot connect to {url}, check url and key")))?;

    let server_name = name.unwrap_or("default").to_string();
    let config_dir = config_dir();
    std::fs::create_dir_all(&config_dir)?;

    let creds_path = config_dir.join("credentials.toml");
    let mut creds = if creds_path.exists() {
        let content = std::fs::read_to_string(&creds_path)?;
        toml::from_str::<arx_core::config::Credentials>(&content).unwrap_or_else(|_| {
            arx_core::config::Credentials {
                default: None,
                servers: std::collections::HashMap::new(),
            }
        })
    } else {
        arx_core::config::Credentials {
            default: None,
            servers: std::collections::HashMap::new(),
        }
    };

    creds.servers.insert(
        server_name.clone(),
        arx_core::config::ServerCredential {
            url: url.to_string(),
            key: key.to_string(),
        },
    );

    if creds.default.is_none() {
        creds.default = Some(server_name.clone());
    }

    let content = toml::to_string_pretty(&creds)
        .map_err(|e| Error::Internal(format!("serialize error: {e}")))?;
    std::fs::write(&creds_path, content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&creds_path, std::fs::Permissions::from_mode(0o600));
    }

    println!("logged in to {url} as '{server_name}'");
    Ok(())
}

fn config_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".config/arx"))
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/arx"))
}

async fn run_mcp() -> Result<(), Error> {
    let db_path = std::env::var("ARX_DB_PATH").unwrap_or_else(|_| "/var/lib/arx/arx.db".into());
    let pool = arx_core::db::connect(&db_path).await?;
    let engine = std::sync::Arc::new(
        arx_engine::deploy::DeployEngine::new()
            .map_err(|e| Error::Internal(format!("engine init failed: {e}")))?,
    );

    arx_api::mcp::run_mcp_server(pool, engine).await;
    Ok(())
}
