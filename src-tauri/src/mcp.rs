use std::process::Stdio;
use std::time::{Duration, Instant};

use tauri::State;

use crate::harness::resolve_harness_binary;
use crate::models::McpServer;
use crate::DbState;

#[tauri::command]
pub async fn list_mcp_servers(state: State<'_, DbState>) -> Result<Vec<McpServer>, String> {
    let db = state.0.lock().await;
    db.list_mcp_servers().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, DbState>,
    name: String,
    transport: String,
    endpoint: String,
) -> Result<McpServer, String> {
    let name = name.trim().to_string();
    let transport = transport.trim().to_lowercase();
    let endpoint = endpoint.trim().to_string();

    if name.is_empty() {
        return Err("MCP server name cannot be empty".to_string());
    }
    if endpoint.is_empty() {
        return Err("MCP server endpoint cannot be empty".to_string());
    }
    let normalized_transport = if transport == "sse" { "sse" } else { "stdio" };

    let db = state.0.lock().await;
    db.add_mcp_server(&name, normalized_transport, &endpoint)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn toggle_mcp_server(
    state: State<'_, DbState>,
    id: String,
    enabled: bool,
) -> Result<Option<McpServer>, String> {
    let db = state.0.lock().await;
    db.toggle_mcp_server(&id, enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_mcp_server(state: State<'_, DbState>, id: String) -> Result<bool, String> {
    let db = state.0.lock().await;
    db.delete_mcp_server(&id).map_err(|e| e.to_string())
}

pub async fn ping_mcp_server_inner(
    db: &std::sync::Arc<tokio::sync::Mutex<crate::db::Database>>,
    id: String,
) -> Result<McpServer, String> {
    let server = {
        let db = db.lock().await;
        db.get_mcp_server(&id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("MCP server '{id}' not found"))?
    };

    let (status, latency_ms) = if server.transport == "sse"
        || server.endpoint.starts_with("http://")
        || server.endpoint.starts_with("https://")
    {
        ping_http_endpoint(&server.endpoint).await
    } else {
        ping_stdio_endpoint(&server.endpoint).await
    };

    let db = db.lock().await;
    db.update_mcp_server_ping(&id, &status, latency_ms)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("MCP server '{id}' not found"))
}

#[tauri::command]
pub async fn ping_mcp_server(
    state: State<'_, DbState>,
    id: String,
) -> Result<McpServer, String> {
    ping_mcp_server_inner(&state.0, id).await
}

async fn ping_http_endpoint(endpoint: &str) -> (String, Option<i64>) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return ("error".to_string(), None),
    };

    let start = Instant::now();
    match client.get(endpoint).send().await {
        Ok(resp) => {
            let elapsed = start.elapsed().as_millis() as i64;
            if resp.status().as_u16() < 500 {
                ("connected".to_string(), Some(elapsed.max(1)))
            } else {
                ("error".to_string(), Some(elapsed.max(1)))
            }
        }
        Err(_) => ("error".to_string(), None),
    }
}

async fn ping_stdio_endpoint(endpoint: &str) -> (String, Option<i64>) {
    let parts: Vec<&str> = endpoint.split_whitespace().collect();
    let Some(program) = parts.first() else {
        return ("error".to_string(), None);
    };

    let start = Instant::now();
    let mut cmd = crate::platform::tokio_command(program);
    cmd.args(&parts[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    if let Ok(mut child) = cmd.spawn() {
        // Attempt real MCP JSON-RPC 2.0 initialize handshake
        let init_req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"sakana-desktop","version":"2.0.0"}}}"#;
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(format!("{init_req}\n").as_bytes()).await;
            let _ = stdin.flush().await;
        }
        if let Some(stdout) = child.stdout.take() {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut lines = BufReader::new(stdout).lines();
            if let Ok(Ok(Some(line))) = tokio::time::timeout(Duration::from_millis(800), lines.next_line()).await {
                let elapsed = start.elapsed().as_millis().max(1) as i64;
                let _ = child.kill().await;
                if line.contains("jsonrpc") || line.contains("protocolVersion") || line.contains("result") {
                    return ("connected".to_string(), Some(elapsed));
                }
            }
        }
        let elapsed = start.elapsed().as_millis().max(1) as i64;
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => return ("error".to_string(), Some(elapsed)),
            Ok(_) => {
                let _ = child.kill().await;
                return ("connected".to_string(), Some(elapsed));
            }
            Err(_) => return ("error".to_string(), None),
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut shell_cmd = crate::platform::tokio_command("where");
        shell_cmd
            .arg(program)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Ok(out) = shell_cmd.output().await {
            let elapsed = start.elapsed().as_millis().max(1) as i64;
            if out.status.success() {
                return ("connected".to_string(), Some(elapsed));
            }
        }
    }

    ("error".to_string(), None)
}

pub async fn import_agy_mcp_servers_inner(
    db: &std::sync::Arc<tokio::sync::Mutex<crate::db::Database>>,
) -> Result<Vec<McpServer>, String> {
    let agy_bin = resolve_harness_binary("agy-cli")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "agy".to_string());

    let mut cmd = crate::platform::tokio_command(&agy_bin);
    cmd.args(["mcp", "list"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run '{} mcp list': {}", agy_bin, e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() && stdout.trim().is_empty() {
        return Err(format!(
            "agy mcp list failed: {}",
            stderr.trim()
        ));
    }

    let parsed = parse_agy_mcp_list(&stdout);
    let db = db.lock().await;
    for (name, transport, endpoint) in parsed {
        db.upsert_mcp_server_by_name(&name, &transport, &endpoint)
            .map_err(|e| e.to_string())?;
    }

    db.list_mcp_servers().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_agy_mcp_servers(state: State<'_, DbState>) -> Result<Vec<McpServer>, String> {
    import_agy_mcp_servers_inner(&state.0).await
}

fn parse_agy_mcp_list(raw: &str) -> Vec<(String, String, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // Try JSON array or object first
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let mut results = Vec::new();
            if let Some(arr) = val.as_array() {
                for item in arr {
                    if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                        let endpoint = item
                            .get("url")
                            .or_else(|| item.get("endpoint"))
                            .or_else(|| item.get("command"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("agy-mcp")
                            .to_string();
                        let transport = if endpoint.starts_with("http") {
                            "sse".to_string()
                        } else {
                            item.get("transport")
                                .and_then(|v| v.as_str())
                                .unwrap_or("stdio")
                                .to_string()
                        };
                        results.push((name.to_string(), transport, endpoint));
                    }
                }
            } else if let Some(obj) = val.get("mcpServers").and_then(|v| v.as_object()) {
                for (name, cfg) in obj {
                    let endpoint = cfg
                        .get("url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            let cmd = cfg.get("command").and_then(|v| v.as_str()).unwrap_or("");
                            let args = cfg
                                .get("args")
                                .and_then(|v| v.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|x| x.as_str())
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                })
                                .unwrap_or_default();
                            format!("{} {}", cmd, args).trim().to_string()
                        });
                    let transport = if endpoint.starts_with("http") {
                        "sse".to_string()
                    } else {
                        "stdio".to_string()
                    };
                    if !endpoint.is_empty() {
                        results.push((name.clone(), transport, endpoint));
                    }
                }
            }
            if !results.is_empty() {
                return results;
            }
        }
    }

    // Fallback line-based parser (e.g. "name: command/url" or "name | transport | endpoint")
    let mut results = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with('-')
            || line.to_lowercase().starts_with("name")
        {
            continue;
        }
        if let Some((left, right)) = line.split_once(':') {
            let name = left.trim().to_string();
            let endpoint = right.trim().to_string();
            if !name.is_empty() && !endpoint.is_empty() {
                let transport = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
                    "sse".to_string()
                } else {
                    "stdio".to_string()
                };
                results.push((name, transport, endpoint));
            }
        } else {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 2 {
                let name = cols[0].to_string();
                let endpoint = cols[1..].join(" ");
                let transport = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
                    "sse".to_string()
                } else {
                    "stdio".to_string()
                };
                results.push((name, transport, endpoint));
            }
        }
    }
    results
}
