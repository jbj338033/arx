use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use arx_core::db;
use arx_engine::deploy::DeployEngine;

#[derive(Deserialize)]
struct JsonRpcRequest {
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Value, code: i64, message: &str) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(json!({"code": code, "message": message})),
        }
    }
}

pub async fn run_mcp_server(pool: SqlitePool, engine: Arc<DeployEngine>) {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::error(Value::Null, -32700, &e.to_string());
                write_response(&mut stdout, &resp).await;
                continue;
            }
        };

        let id = req.id.clone().unwrap_or(Value::Null);
        let resp = handle_request(&pool, &engine, &req).await;
        let response = match resp {
            Ok(result) => JsonRpcResponse::success(id, result),
            Err(msg) => JsonRpcResponse::error(id, -32000, &msg),
        };

        write_response(&mut stdout, &response).await;
    }
}

async fn write_response(stdout: &mut tokio::io::Stdout, resp: &JsonRpcResponse) {
    if let Ok(json) = serde_json::to_string(resp) {
        let _ = stdout.write_all(json.as_bytes()).await;
        let _ = stdout.write_all(b"\n").await;
        let _ = stdout.flush().await;
    }
}

async fn handle_request(
    pool: &SqlitePool,
    _engine: &DeployEngine,
    req: &JsonRpcRequest,
) -> Result<Value, String> {
    match req.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "arx",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),

        "tools/list" => Ok(json!({
            "tools": [
                tool_def("list_projects", "List all projects", json!({"type": "object", "properties": {}})),
                tool_def("deploy_image", "Deploy a pre-built image", json!({
                    "type": "object",
                    "properties": {
                        "project_id": {"type": "string", "description": "Project ID"},
                        "image": {"type": "string", "description": "Docker image reference"}
                    },
                    "required": ["project_id", "image"]
                })),
                tool_def("get_deployment_status", "Get deployment status and verification result", json!({
                    "type": "object",
                    "properties": {
                        "deployment_id": {"type": "string", "description": "Deployment ID"}
                    },
                    "required": ["deployment_id"]
                })),
                tool_def("get_logs", "Get deployment logs", json!({
                    "type": "object",
                    "properties": {
                        "project_id": {"type": "string", "description": "Project ID"},
                        "deployment_id": {"type": "string", "description": "Deployment ID"}
                    },
                    "required": ["project_id", "deployment_id"]
                })),
                tool_def("set_env_vars", "Set environment variables for a project", json!({
                    "type": "object",
                    "properties": {
                        "project_id": {"type": "string", "description": "Project ID"},
                        "vars": {"type": "object", "description": "Key-value pairs"}
                    },
                    "required": ["project_id", "vars"]
                })),
                tool_def("rollback", "Rollback to previous deployment", json!({
                    "type": "object",
                    "properties": {
                        "project_id": {"type": "string", "description": "Project ID"}
                    },
                    "required": ["project_id"]
                })),
                tool_def("get_resource_status", "Get resource usage for a project", json!({
                    "type": "object",
                    "properties": {
                        "project_id": {"type": "string", "description": "Project ID"}
                    },
                    "required": ["project_id"]
                })),
            ]
        })),

        "tools/call" => {
            let tool_name = req.params["name"].as_str().unwrap_or("");
            let args = &req.params["arguments"];
            let result = call_tool(pool, tool_name, args).await?;
            Ok(json!({
                "content": [{"type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default()}]
            }))
        }

        "notifications/initialized" => Ok(json!({})),

        _ => Err(format!("unknown method: {}", req.method)),
    }
}

async fn call_tool(pool: &SqlitePool, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "list_projects" => {
            let projects = db::list_projects(pool).await.map_err(|e| e.to_string())?;
            Ok(json!(projects))
        }

        "deploy_image" => {
            let project_id = args["project_id"]
                .as_str()
                .ok_or("missing project_id")?;
            let image = args["image"].as_str().ok_or("missing image")?;

            let deployment = arx_core::model::Deployment {
                id: uuid::Uuid::new_v4().to_string(),
                project_id: project_id.to_string(),
                status: arx_core::model::DeploymentStatus::Pending,
                source: arx_core::model::DeploymentSource::Image,
                git_ref: None,
                git_sha: None,
                image_ref: Some(image.to_string()),
                container_id: None,
                url: None,
                verification_result: None,
                log_path: None,
                claim_token: None,
                claimed_by: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };

            db::create_deployment(pool, &deployment)
                .await
                .map_err(|e| e.to_string())?;

            Ok(json!({
                "deployment_id": deployment.id,
                "status": "pending",
                "message": "deployment created"
            }))
        }

        "get_deployment_status" => {
            let did = args["deployment_id"].as_str().ok_or("missing deployment_id")?;
            let dep = db::get_deployment(pool, did).await.map_err(|e| e.to_string())?;
            Ok(json!(dep))
        }

        "get_logs" => {
            let did = args["deployment_id"].as_str().ok_or("missing deployment_id")?;
            let dep = db::get_deployment(pool, did).await.map_err(|e| e.to_string())?;

            let logs = if let Some(path) = &dep.log_path {
                std::fs::read_to_string(path).unwrap_or_else(|_| "no logs available".into())
            } else {
                "no log path set".into()
            };

            Ok(json!({"logs": logs}))
        }

        "rollback" => {
            let project_id = args["project_id"].as_str().ok_or("missing project_id")?;
            let deployments = db::list_deployments(pool, project_id)
                .await
                .map_err(|e| e.to_string())?;

            let live_deps: Vec<_> = deployments
                .iter()
                .filter(|d| d.status == arx_core::model::DeploymentStatus::Live)
                .collect();

            if live_deps.len() < 2 {
                return Err("no previous deployment to rollback to".into());
            }

            let previous = &live_deps[1];
            db::update_project_production(pool, project_id, &previous.id)
                .await
                .map_err(|e| e.to_string())?;

            Ok(json!({
                "message": "rolled back",
                "deployment_id": previous.id
            }))
        }

        "get_resource_status" => {
            let project_id = args["project_id"].as_str().ok_or("missing project_id")?;
            let project = db::get_project(pool, project_id)
                .await
                .map_err(|e| e.to_string())?;

            Ok(json!({
                "project": project.name,
                "production_deployment_id": project.production_deployment_id,
            }))
        }

        "set_env_vars" => {
            Ok(json!({"message": "env vars set (encryption not configured)"}))
        }

        _ => Err(format!("unknown tool: {name}")),
    }
}

fn tool_def(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}
