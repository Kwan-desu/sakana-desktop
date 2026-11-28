use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{oneshot, Mutex, Semaphore};

use crate::db::Database;
use crate::harness::{emit_thinking, emit_token, emit_tool, TurnAccumulator};
use crate::models::{HarnessAccountRow, McpServer, ToolCall};
use crate::DbState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAwakeEvent {
    pub session_id: String,
    pub awake: bool,
}

struct AcpWorker {
    child: Child,
    stdin: ChildStdin,
    stdout_lines: Lines<BufReader<ChildStdout>>,
    acp_session_id: String,
    cwd: String,
    wsl_distro: Option<String>,
    account_id: String,
    model: String,
    effort: String,
    mode: String,
    next_rpc_id: u64,
    turns_completed: u64,
    supports_effort: bool,
    dead: bool,
}

const KEEP_AWAKE_SETTING_KEY: &str = "acp_keep_awake_sessions";

static ACP_POOL: OnceLock<Arc<Mutex<HashMap<String, Arc<Mutex<AcpWorker>>>>>> = OnceLock::new();
static ACP_SPAWN_SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();
static KEEP_AWAKE_SET: OnceLock<Arc<Mutex<HashSet<String>>>> = OnceLock::new();

fn pool() -> &'static Arc<Mutex<HashMap<String, Arc<Mutex<AcpWorker>>>>> {
    ACP_POOL.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

fn spawn_semaphore() -> &'static Arc<Semaphore> {
    ACP_SPAWN_SEMAPHORE.get_or_init(|| Arc::new(Semaphore::new(1)))
}

fn keep_awake_set() -> &'static Arc<Mutex<HashSet<String>>> {
    KEEP_AWAKE_SET.get_or_init(|| Arc::new(Mutex::new(HashSet::new())))
}

fn load_keep_awake_from_db(db: &Database) -> Vec<String> {
    let Ok(Some(raw)) = db.get_setting(KEEP_AWAKE_SETTING_KEY) else {
        return Vec::new();
    };
    let Ok(ids) = serde_json::from_str::<Vec<String>>(&raw) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    ids.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && seen.insert(s.clone()))
        .collect()
}

fn save_keep_awake_to_db(db: &Database, set: &HashSet<String>) {
    let mut ids: Vec<&String> = set.iter().collect();
    ids.sort();
    if let Ok(json) = serde_json::to_string(&ids) {
        if let Err(err) = db.set_setting(KEEP_AWAKE_SETTING_KEY, &json) {
            eprintln!("[acp_pool] failed to persist {KEEP_AWAKE_SETTING_KEY}: {err}");
        }
    }
}

async fn add_keep_awake(db_arc: &Arc<Mutex<Database>>, session_id: &str) {
    let snapshot = {
        let mut set = keep_awake_set().lock().await;
        if set.insert(session_id.to_string()) {
            Some(set.clone())
        } else {
            None
        }
    };
    if let Some(snapshot) = snapshot {
        let db = db_arc.lock().await;
        save_keep_awake_to_db(&db, &snapshot);
    }
}

async fn remove_keep_awake(db_arc: Option<&Arc<Mutex<Database>>>, session_id: &str) -> bool {
    let (removed, snapshot) = {
        let mut set = keep_awake_set().lock().await;
        let removed = set.remove(session_id);
        let snapshot = if removed { Some(set.clone()) } else { None };
        (removed, snapshot)
    };
    if let (Some(snapshot), Some(db_arc)) = (snapshot, db_arc) {
        let db = db_arc.lock().await;
        save_keep_awake_to_db(&db, &snapshot);
    }
    removed
}

fn emit_awake(app: Option<&AppHandle>, session_id: &str, awake: bool) {
    crate::platform::emit_app_and_remote(
        app,
        "session:awake-changed",
        SessionAwakeEvent {
            session_id: session_id.to_string(),
            awake,
        },
    );
}

fn map_permission_mode(permission_mode: &str) -> &'static str {
    match permission_mode {
        "plan" => "plan",
        _ => "auto",
    }
}

impl AcpWorker {
    fn is_alive(&mut self) -> bool {
        if self.dead {
            return false;
        }
        match self.child.try_wait() {
            Ok(None) => true,
            _ => {
                self.dead = true;
                false
            }
        }
    }

    async fn write_json(&mut self, val: &Value) -> Result<(), String> {
        let mut line = serde_json::to_string(val).map_err(|e| e.to_string())?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("ACP stdin write failed: {e}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| format!("ACP stdin flush failed: {e}"))?;
        Ok(())
    }

    async fn next_id(&mut self) -> u64 {
        let id = self.next_rpc_id;
        self.next_rpc_id += 1;
        id
    }

    async fn rpc_call(&mut self, method: &str, params: Value, timeout_secs: u64) -> Result<Value, String> {
        let id = self.next_id().await;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_json(&req).await?;

        let res = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
            loop {
                let line = self
                    .stdout_lines
                    .next_line()
                    .await
                    .map_err(|e| format!("ACP stdout read failed: {e}"))?;
                let Some(line) = line else {
                    self.dead = true;
                    return Err("ACP process closed stdout unexpectedly".to_string());
                };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                    continue;
                };
                if msg.get("id").and_then(Value::as_u64) == Some(id) {
                    if let Some(err) = msg.get("error") {
                        let err_msg = err
                            .get("message")
                            .and_then(Value::as_str)
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| err.to_string());
                        return Err(format!("ACP {method} error: {err_msg}"));
                    }
                    return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
                }
            }
        })
        .await;

        match res {
            Ok(inner) => inner,
            Err(_) => {
                self.dead = true;
                Err(format!("ACP {method} timed out after {timeout_secs}s"))
            }
        }
    }

    async fn spawn_and_init(
        bin: Option<&Path>,
        wsl_distro: Option<&str>,
        account: Option<&HarnessAccountRow>,
        enabled_mcp_servers: &[McpServer],
        cwd: &str,
        model: &str,
        effort: &str,
        permission_mode: &str,
    ) -> Result<Self, String> {
        let _spawn_permit = spawn_semaphore()
            .acquire()
            .await
            .map_err(|e| format!("ACP spawn gate closed: {e}"))?;

        let auth_method = account
            .map(|a| a.auth_type.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("oauth-personal");
        let account_id = account
            .map(|a| a.id.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "default".to_string());

        let (mut child, session_cwd) = if let Some(distro) = wsl_distro {
            let env_exports = crate::harness::build_wsl_env_exports("agy-cli", account);
            let posix_cwd = crate::harness::wsl_posix_cwd(cwd);
            let mut cli_cmd = r#"$(command -v agy-cli || command -v agy || echo agy) acp --dangerously-skip-permissions --gemini-dir "$SAKANA_AGY_GEMINI_DIR" --app-data-dir antigravity-cli"#.to_string();
            if !env_exports.is_empty() {
                cli_cmd = format!("{}; {}", env_exports.join("; "), cli_cmd);
            }
            let mut cmd = crate::platform::wsl_tokio_shell(distro, &posix_cwd, &cli_cmd);
            crate::auth::apply_account_env(&mut cmd, "agy-cli", account);
            cmd.kill_on_drop(true)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            let c = cmd
                .spawn()
                .map_err(|e| format!("Failed to spawn agy acp in WSL ({distro}): {e}"))?;
            (c, posix_cwd)
        } else {
            let bin_path = bin.ok_or_else(|| "agy-cli binary not found".to_string())?;
            crate::auth::sync_profile_mcp_and_skills("agy-cli", account, enabled_mcp_servers);
            let gdir = account
                .map(crate::auth::agy_gemini_dir)
                .unwrap_or_else(|| crate::auth::ensure_profile_dir("agy-cli", "default").join(".gemini"));
            let mut cmd = crate::platform::tokio_command(&bin_path.to_string_lossy());
            crate::auth::apply_account_env(&mut cmd, "agy-cli", account);
            cmd.args([
                "acp",
                "--dangerously-skip-permissions",
                "--gemini-dir",
                &gdir.to_string_lossy(),
                "--app-data-dir",
                "antigravity-cli",
            ])
            .current_dir(cwd)
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
            let c = cmd
                .spawn()
                .map_err(|e| format!("Failed to spawn agy acp ({}): {e}", bin_path.display()))?;
            (c, cwd.to_string())
        };

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to open ACP stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to open ACP stdout".to_string())?;

        let mut worker = Self {
            child,
            stdin,
            stdout_lines: BufReader::new(stdout).lines(),
            acp_session_id: String::new(),
            cwd: cwd.to_string(),
            wsl_distro: wsl_distro.map(|s| s.to_string()),
            account_id,
            model: String::new(),
            effort: String::new(),
            mode: String::new(),
            next_rpc_id: 1,
            turns_completed: 0,
            supports_effort: false,
            dead: false,
        };

        worker
            .rpc_call(
                "initialize",
                serde_json::json!({
                    "protocolVersion": 1,
                    "clientCapabilities": {},
                    "clientInfo": { "name": "sakana", "version": env!("CARGO_PKG_VERSION") }
                }),
                45,
            )
            .await?;

        worker
            .rpc_call(
                "authenticate",
                serde_json::json!({ "methodId": auth_method }),
                45,
            )
            .await?;

        let sess_res = worker
            .rpc_call(
                "session/new",
                serde_json::json!({
                    "cwd": session_cwd,
                    "mcpServers": [],
                }),
                60,
            )
            .await?;

        let acp_sid = sess_res
            .get("sessionId")
            .and_then(Value::as_str)
            .map(|s| s.to_string())
            .ok_or_else(|| "ACP session/new did not return sessionId".to_string())?;
        worker.acp_session_id = acp_sid;

        // Record initial defaults returned by ACP so sync_config only sends changes
        if let Some(opts) = sess_res.get("configOptions").and_then(Value::as_array) {
            for opt in opts {
                let id = opt.get("id").and_then(Value::as_str).unwrap_or("");
                let cur = opt.get("currentValue").and_then(Value::as_str).unwrap_or("");
                match id {
                    "model" => worker.model = cur.to_string(),
                    "thought_level" => {
                        worker.supports_effort = true;
                        worker.effort = cur.to_string();
                    }
                    "mode" => worker.mode = cur.to_string(),
                    _ => {}
                }
            }
        }

        worker.sync_config(model, effort, permission_mode).await?;
        Ok(worker)
    }

    async fn sync_config(
        &mut self,
        model: &str,
        effort: &str,
        permission_mode: &str,
    ) -> Result<(), String> {
        let trimmed_model = model.trim();
        if !trimmed_model.is_empty() && self.model != trimmed_model {
            match self
                .rpc_call(
                    "session/set_config_option",
                    serde_json::json!({
                        "sessionId": self.acp_session_id,
                        "configId": "model",
                        "value": trimmed_model,
                    }),
                    20,
                )
                .await
            {
                Ok(_) => self.model = trimmed_model.to_string(),
                Err(e) if self.dead => return Err(e),
                Err(_) => {}
            }
        }

        let trimmed_effort = effort.trim();
        if self.supports_effort
            && matches!(trimmed_effort, "low" | "medium" | "high" | "xhigh")
            && self.effort != trimmed_effort
        {
            match self
                .rpc_call(
                    "session/set_config_option",
                    serde_json::json!({
                        "sessionId": self.acp_session_id,
                        "configId": "thought_level",
                        "value": trimmed_effort,
                    }),
                    20,
                )
                .await
            {
                Ok(_) => self.effort = trimmed_effort.to_string(),
                Err(e) if self.dead => return Err(e),
                Err(_) => {}
            }
        }

        let target_mode = map_permission_mode(permission_mode);
        if self.mode != target_mode {
            match self
                .rpc_call(
                    "session/set_config_option",
                    serde_json::json!({
                        "sessionId": self.acp_session_id,
                        "configId": "mode",
                        "value": target_mode,
                    }),
                    20,
                )
                .await
            {
                Ok(_) => self.mode = target_mode.to_string(),
                Err(e) if self.dead => return Err(e),
                Err(_) => {}
            }
        }

        Ok(())
    }

    async fn handle_server_request(&mut self, msg: &Value) {
        let Some(req_id) = msg.get("id").cloned() else {
            return;
        };
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        if method == "session/request_permission" {
            let options = msg
                .get("params")
                .and_then(|p| p.get("options"))
                .and_then(Value::as_array);
            let chosen_id = options
                .and_then(|arr| {
                    arr.iter()
                        .find(|o| {
                            o.get("kind")
                                .and_then(Value::as_str)
                                .map(|k| k.contains("allow"))
                                .unwrap_or(false)
                        })
                        .or_else(|| arr.first())
                        .and_then(|o| o.get("optionId").and_then(Value::as_str))
                })
                .unwrap_or("allow_once");
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "outcome": {
                        "outcome": "selected",
                        "optionId": chosen_id,
                    }
                }
            });
            let _ = self.write_json(&resp).await;
        } else {
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "error": { "code": -32601, "message": format!("Unsupported client method: {method}") }
            });
            let _ = self.write_json(&resp).await;
        }
    }

    async fn run_prompt(
        &mut self,
        prompt_text: &str,
        app: Option<&AppHandle>,
        session_id: &str,
        message_id: &str,
        started: Instant,
        acc: &mut TurnAccumulator,
        mut cancel_rx: oneshot::Receiver<()>,
    ) -> Result<(), String> {
        let req_id = self.next_id().await;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": "session/prompt",
            "params": {
                "sessionId": self.acp_session_id,
                "prompt": [{ "type": "text", "text": prompt_text }],
            }
        });
        self.write_json(&req).await?;

        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    let cancel_notif = serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": "session/cancel",
                        "params": { "sessionId": self.acp_session_id }
                    });
                    let _ = self.write_json(&cancel_notif).await;
                    // Drain briefly so the worker stays clean for future turns; if it stalls, mark dead.
                    let drained = tokio::time::timeout(Duration::from_secs(3), async {
                        while let Ok(Some(line)) = self.stdout_lines.next_line().await {
                            if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                                if v.get("id").and_then(Value::as_u64) == Some(req_id) {
                                    return true;
                                }
                            }
                        }
                        false
                    })
                    .await
                    .unwrap_or(false);
                    if !drained {
                        self.dead = true;
                    }
                    return Err("Turn cancelled by user.".to_string());
                }
                line_res = self.stdout_lines.next_line() => {
                    match line_res {
                        Ok(Some(line)) => {
                            let trimmed = line.trim();
                            if trimmed.is_empty() {
                                continue;
                            }
                            let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                                continue;
                            };

                            // Final response for our prompt request
                            if msg.get("id").and_then(Value::as_u64) == Some(req_id) {
                                if let Some(err) = msg.get("error") {
                                    let err_msg = err
                                        .get("message")
                                        .and_then(Value::as_str)
                                        .map(|s| s.to_string())
                                        .unwrap_or_else(|| err.to_string());
                                    return Err(format!("agy-cli failed: {err_msg}"));
                                }
                                if let Some(usage) = msg.get("result").and_then(|r| r.get("usage")) {
                                    if let Some(inp) = usage.get("inputTokens").and_then(Value::as_i64) {
                                        acc.input_tokens = acc.input_tokens.max(inp);
                                    }
                                    if let Some(out) = usage.get("outputTokens").and_then(Value::as_i64) {
                                        acc.output_tokens = acc.output_tokens.max(out);
                                    }
                                }
                                self.turns_completed += 1;
                                return Ok(());
                            }

                            // Server-to-client request (has both id and method)
                            if msg.get("id").is_some() && msg.get("method").is_some() {
                                self.handle_server_request(&msg).await;
                                continue;
                            }

                            // Streaming session/update notification
                            if msg.get("method").and_then(Value::as_str) == Some("session/update") {
                                if let Some(update) = msg.get("params").and_then(|p| p.get("update")) {
                                    handle_session_update(update, app, session_id, message_id, started, acc);
                                }
                            }
                        }
                        Ok(None) => {
                            self.dead = true;
                            return Err("agy-cli ACP process exited unexpectedly".to_string());
                        }
                        Err(e) => {
                            self.dead = true;
                            return Err(format!("Error reading agy-cli ACP stdout: {e}"));
                        }
                    }
                }
            }
        }
    }
}

fn handle_session_update(
    update: &Value,
    app: Option<&AppHandle>,
    session_id: &str,
    message_id: &str,
    started: Instant,
    acc: &mut TurnAccumulator,
) {
    let kind = update
        .get("sessionUpdate")
        .and_then(Value::as_str)
        .unwrap_or("");
    let elapsed_ms = started.elapsed().as_millis() as i64;

    match kind {
        "agent_message_chunk" => {
            if let Some(text) = update
                .get("content")
                .and_then(|c| c.get("text"))
                .and_then(Value::as_str)
            {
                emit_token(app, session_id, message_id, text, acc);
            }
        }
        "agent_thought_chunk" => {
            if let Some(text) = update
                .get("content")
                .and_then(|c| c.get("text"))
                .and_then(Value::as_str)
            {
                emit_thinking(app, session_id, message_id, text, elapsed_ms, acc);
            }
        }
        "tool_call" | "tool_call_update" => {
            let id = update
                .get("toolCallId")
                .or_else(|| update.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string();
            let name = update
                .get("title")
                .or_else(|| update.get("name"))
                .or_else(|| update.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string();
            let target = update
                .get("locations")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(|loc| loc.get("path").and_then(Value::as_str))
                .map(|s| s.to_string())
                .or_else(|| {
                    update.get("rawInput").map(|v| match v.as_str() {
                        Some(s) => s.to_string(),
                        None => v.to_string(),
                    })
                })
                .unwrap_or_default()
                .chars()
                .take(80)
                .collect();
            let raw_status = update
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("running")
                .to_ascii_lowercase();
            let status = match raw_status.as_str() {
                "completed" | "done" | "ok" | "success" => "ok".to_string(),
                "failed" | "error" => "error".to_string(),
                _ => "running".to_string(),
            };
            emit_tool(
                app,
                session_id,
                message_id,
                ToolCall {
                    id,
                    name,
                    target,
                    status,
                    duration_ms: elapsed_ms,
                },
                acc,
            );
        }
        _ => {}
    }
}

/// Execute a turn on a persistent `agy-cli` ACP worker for `session_id`.
/// Keeps the worker process alive after the turn finishes so subsequent messages are instant
/// and preserve prompt caching until the user puts the thread to sleep.
pub(crate) async fn run_persistent_agy_turn(
    bin: Option<&Path>,
    wsl_distro: Option<&str>,
    account: Option<&HarnessAccountRow>,
    model: &str,
    effort: &str,
    permission_mode: &str,
    enabled_mcp_servers: &[McpServer],
    cwd: &str,
    cold_prompt: &str,
    warm_prompt: &str,
    app: Option<&AppHandle>,
    session_id: &str,
    message_id: &str,
    started: Instant,
    acc: &mut TurnAccumulator,
    mut cancel_rx: oneshot::Receiver<()>,
) -> Result<(), String> {
    let account_id = account
        .map(|a| a.id.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string());

    // Check if we already have a healthy matching worker in the pool
    let existing_arc = {
        let map = pool().lock().await;
        map.get(session_id).cloned()
    };

    let worker_arc = if let Some(arc) = existing_arc {
        let reusable = {
            let mut w = arc.lock().await;
            w.is_alive()
                && w.cwd == cwd
                && w.wsl_distro.as_deref() == wsl_distro
                && w.account_id == account_id
        };
        if reusable {
            arc
        } else {
            // Stale or dead worker — evict and respawn cleanly
            evict_worker_inner(app, session_id).await;
            let fresh = tokio::select! {
                _ = &mut cancel_rx => return Err("Turn cancelled by user".to_string()),
                res = AcpWorker::spawn_and_init(
                    bin,
                    wsl_distro,
                    account,
                    enabled_mcp_servers,
                    cwd,
                    model,
                    effort,
                    permission_mode,
                ) => res?,
            };
            let new_arc = Arc::new(Mutex::new(fresh));
            {
                let mut map = pool().lock().await;
                map.insert(session_id.to_string(), new_arc.clone());
            }
            emit_awake(app, session_id, true);
            new_arc
        }
    } else {
        let fresh = tokio::select! {
            _ = &mut cancel_rx => return Err("Turn cancelled by user".to_string()),
            res = AcpWorker::spawn_and_init(
                bin,
                wsl_distro,
                account,
                enabled_mcp_servers,
                cwd,
                model,
                effort,
                permission_mode,
            ) => res?,
        };
        let new_arc = Arc::new(Mutex::new(fresh));
        {
            let mut map = pool().lock().await;
            map.insert(session_id.to_string(), new_arc.clone());
        }
        emit_awake(app, session_id, true);
        new_arc
    };

    let res = {
        let mut w = worker_arc.lock().await;
        if let Err(e) = w.sync_config(model, effort, permission_mode).await {
            drop(w);
            evict_worker_inner(app, session_id).await;
            return Err(e);
        }
        let prompt_to_send = if w.turns_completed == 0 {
            cold_prompt
        } else {
            warm_prompt
        };
        w.run_prompt(
            prompt_to_send,
            app,
            session_id,
            message_id,
            started,
            acc,
            cancel_rx,
        )
        .await
    };

    let is_dead = { worker_arc.lock().await.dead };
    if is_dead {
        evict_worker_inner(app, session_id).await;
    }

    res
}

async fn evict_worker_inner(app: Option<&AppHandle>, session_id: &str) -> bool {
    let removed = {
        let mut map = pool().lock().await;
        map.remove(session_id)
    };
    if let Some(arc) = removed {
        let mut w = arc.lock().await;
        w.dead = true;
        let _ = w.child.kill().await;
        let keep_awake = { keep_awake_set().lock().await.contains(session_id) };
        if !keep_awake {
            emit_awake(app, session_id, false);
        }
        true
    } else {
        false
    }
}

pub async fn list_awake_sessions_inner() -> Vec<String> {
    let entries: Vec<(String, Arc<Mutex<AcpWorker>>)> = {
        let map = pool().lock().await;
        map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    };
    let mut awake: HashSet<String> = HashSet::new();
    let mut dead_ids = Vec::new();
    for (sid, arc) in entries {
        if let Ok(mut w) = arc.try_lock() {
            if w.is_alive() {
                awake.insert(sid);
            } else {
                dead_ids.push(sid);
            }
        } else {
            // Currently busy executing a turn -> definitely awake
            awake.insert(sid);
        }
    }
    if !dead_ids.is_empty() {
        let mut map = pool().lock().await;
        for sid in dead_ids {
            map.remove(&sid);
        }
    }
    {
        let keep = keep_awake_set().lock().await;
        for sid in keep.iter() {
            awake.insert(sid.clone());
        }
    }
    let mut result: Vec<String> = awake.into_iter().collect();
    result.sort();
    result
}

async fn sleep_session_with_db(
    app: Option<&AppHandle>,
    db_arc: Option<&Arc<Mutex<Database>>>,
    session_id: &str,
) -> bool {
    let removed_worker = {
        let mut map = pool().lock().await;
        map.remove(session_id)
    };
    let had_worker = if let Some(arc) = removed_worker {
        let mut w = arc.lock().await;
        w.dead = true;
        let _ = w.child.kill().await;
        true
    } else {
        false
    };
    let removed_keep_awake = remove_keep_awake(db_arc, session_id).await;
    if had_worker || removed_keep_awake {
        emit_awake(app, session_id, false);
        true
    } else {
        false
    }
}

pub async fn sleep_session_inner(app: Option<&AppHandle>, session_id: &str) -> bool {
    let db_arc = app
        .and_then(|a| a.try_state::<DbState>())
        .map(|s| Arc::clone(&s.0));
    sleep_session_with_db(app, db_arc.as_ref(), session_id).await
}

pub async fn sleep_all_sessions_inner(app: Option<&AppHandle>) -> usize {
    let drained: Vec<(String, Arc<Mutex<AcpWorker>>)> = {
        let mut map = pool().lock().await;
        map.drain().collect()
    };
    let count = drained.len();
    for (sid, arc) in drained {
        let mut w = arc.lock().await;
        w.dead = true;
        let _ = w.child.kill().await;
        emit_awake(app, &sid, false);
    }
    count
}

pub async fn wake_session_inner(
    app: Option<&AppHandle>,
    db_arc: &Arc<Mutex<Database>>,
    session_id: &str,
) -> Result<bool, String> {
    // Already awake?
    {
        let existing = { pool().lock().await.get(session_id).cloned() };
        if let Some(arc) = existing {
            let alive = { arc.lock().await.is_alive() };
            if alive {
                add_keep_awake(db_arc, session_id).await;
                emit_awake(app, session_id, true);
                return Ok(true);
            }
        }
    }

    let (session, project_cwd, project_wsl_distro, account, enabled_mcp_servers) = {
        let db = db_arc.lock().await;
        let session = db
            .get_session(session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Session '{session_id}' not found"))?;
        let proj = db.get_project(&session.project_id).map_err(|e| e.to_string())?;
        let proj_cwd = proj
            .as_ref()
            .map(|p| p.cwd.clone())
            .unwrap_or_else(|| session.worktree_path.clone());
        let proj_wsl = proj.and_then(|p| {
            if p.runtime.as_deref() == Some("wsl") || p.wsl_distro.is_some() {
                p.wsl_distro.filter(|s| !s.trim().is_empty())
            } else {
                None
            }
        });
        let acc = db
            .get_harness_account(&session.account)
            .ok()
            .flatten()
            .filter(|a| a.harness == session.harness)
            .or_else(|| db.get_default_harness_account(&session.harness).ok().flatten());
        let mcps: Vec<McpServer> = db
            .list_mcp_servers()
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.enabled)
            .collect();
        (session, proj_cwd, proj_wsl, acc, mcps)
    };

    if session.harness != "agy-cli" {
        return Err(format!(
            "Persistent awake sessions are currently supported for agy-cli (thread uses '{}')",
            session.harness
        ));
    }

    let effective_cwd = if project_wsl_distro.is_some() {
        if !session.worktree_path.trim().is_empty() && session.worktree_path != "." {
            session.worktree_path.clone()
        } else {
            project_cwd
        }
    } else if Path::new(&session.worktree_path).exists() && session.worktree_path != "." {
        session.worktree_path.clone()
    } else {
        project_cwd
    };

    let (resolved_model, resolved_effort) =
        crate::harness::resolve_agy_model_and_effort_with_db(db_arc, &session.model, &session.effort)
            .await;
    let model_str = resolved_model.unwrap_or_else(|| session.model.clone());
    let effort_str = resolved_effort.unwrap_or_else(|| session.effort.clone());

    let bin = if project_wsl_distro.is_some() {
        None
    } else {
        Some(
            crate::harness::resolve_harness_binary("agy-cli")
                .ok_or_else(|| "agy-cli binary not found in PATH".to_string())?,
        )
    };

    let worker = AcpWorker::spawn_and_init(
        bin.as_deref(),
        project_wsl_distro.as_deref(),
        account.as_ref(),
        &enabled_mcp_servers,
        &effective_cwd,
        &model_str,
        &effort_str,
        &session.permission_mode,
    )
    .await?;

    {
        let mut map = pool().lock().await;
        map.insert(session_id.to_string(), Arc::new(Mutex::new(worker)));
    }
    add_keep_awake(db_arc, session_id).await;
    emit_awake(app, session_id, true);
    Ok(true)
}

/// Load persisted `acp_keep_awake_sessions` on startup, populate `KEEP_AWAKE_SET`, emit
/// `session:awake-changed` (`awake: true`) for valid sessions, then spawn a background task
/// that waits 1500ms, prunes deleted session IDs from SQLite, and wakes each remaining
/// session sequentially via `wake_session_inner` (serialized by `ACP_SPAWN_SEMAPHORE`).
pub fn spawn_auto_wake_task(app: AppHandle, db: Arc<Mutex<Database>>) {
    let mut preloaded = false;
    let mut initial_ids: Vec<String> = Vec::new();
    let mut had_stale = false;

    if let Ok(db_guard) = db.try_lock() {
        let saved_ids = load_keep_awake_from_db(&db_guard);
        let mut valid_ids = Vec::new();
        for sid in saved_ids {
            match db_guard.get_session(&sid) {
                Ok(Some(_)) => valid_ids.push(sid),
                Ok(None) => had_stale = true,
                Err(_) => valid_ids.push(sid),
            }
        }
        drop(db_guard);

        if let Ok(mut set) = keep_awake_set().try_lock() {
            for sid in &valid_ids {
                set.insert(sid.clone());
            }
            preloaded = true;
        }
        if preloaded {
            for sid in &valid_ids {
                emit_awake(Some(&app), sid, true);
            }
            initial_ids = valid_ids;
        }
    }

    tauri::async_runtime::spawn(async move {
        let (ids_to_wake, needs_prune) = if preloaded {
            (initial_ids, had_stale)
        } else {
            let (valid_ids, stale) = {
                let db_guard = db.lock().await;
                let saved_ids = load_keep_awake_from_db(&db_guard);
                let mut valid = Vec::new();
                let mut stale = false;
                for sid in saved_ids {
                    match db_guard.get_session(&sid) {
                        Ok(Some(_)) => valid.push(sid),
                        Ok(None) => stale = true,
                        Err(_) => valid.push(sid),
                    }
                }
                (valid, stale)
            };
            {
                let mut set = keep_awake_set().lock().await;
                for sid in &valid_ids {
                    set.insert(sid.clone());
                }
            }
            for sid in &valid_ids {
                emit_awake(Some(&app), sid, true);
            }
            (valid_ids, stale)
        };

        if ids_to_wake.is_empty() && !needs_prune {
            return;
        }

        tokio::time::sleep(Duration::from_millis(1500)).await;

        // Prune deleted session IDs from SQLite and KEEP_AWAKE_SET
        let (remaining_ids, pruned_ids) = {
            let db_guard = db.lock().await;
            let saved_ids = load_keep_awake_from_db(&db_guard);
            let mut valid = Vec::new();
            let mut pruned = Vec::new();
            for sid in saved_ids {
                match db_guard.get_session(&sid) {
                    Ok(Some(_)) => valid.push(sid),
                    Ok(None) => pruned.push(sid),
                    Err(_) => valid.push(sid),
                }
            }
            if !pruned.is_empty() || needs_prune {
                let valid_set: HashSet<String> = valid.iter().cloned().collect();
                save_keep_awake_to_db(&db_guard, &valid_set);
            }
            (valid, pruned)
        };

        if !pruned_ids.is_empty() {
            {
                let mut set = keep_awake_set().lock().await;
                for sid in &pruned_ids {
                    set.remove(sid);
                }
            }
            for sid in &pruned_ids {
                emit_awake(Some(&app), sid, false);
            }
        }

        // Wake each remaining session sequentially; AcpWorker::spawn_and_init acquires ACP_SPAWN_SEMAPHORE.
        for sid in remaining_ids {
            let still_wanted = { keep_awake_set().lock().await.contains(&sid) };
            if !still_wanted {
                continue;
            }
            if let Err(err) = wake_session_inner(Some(&app), &db, &sid).await {
                eprintln!("[acp_pool] auto-wake failed for session '{sid}': {err}");
            }
        }
    });
}

#[tauri::command]
pub async fn list_awake_sessions() -> Result<Vec<String>, String> {
    Ok(list_awake_sessions_inner().await)
}

#[tauri::command]
pub async fn sleep_session(
    app: AppHandle,
    db_state: State<'_, DbState>,
    session_id: String,
) -> Result<bool, String> {
    Ok(sleep_session_with_db(Some(&app), Some(&db_state.0), &session_id).await)
}

#[tauri::command]
pub async fn sleep_all_sessions(
    app: AppHandle,
    db_state: State<'_, DbState>,
) -> Result<usize, String> {
    let kept_ids: Vec<String> = {
        let mut set = keep_awake_set().lock().await;
        if set.is_empty() {
            Vec::new()
        } else {
            set.drain().collect()
        }
    };
    if !kept_ids.is_empty() {
        let db = db_state.0.lock().await;
        save_keep_awake_to_db(&db, &HashSet::new());
    }
    let pool_count = sleep_all_sessions_inner(Some(&app)).await;
    for sid in &kept_ids {
        emit_awake(Some(&app), sid, false);
    }
    Ok(pool_count.max(kept_ids.len()))
}

#[tauri::command]
pub async fn wake_session(
    app: AppHandle,
    db_state: State<'_, DbState>,
    session_id: String,
) -> Result<bool, String> {
    wake_session_inner(Some(&app), &db_state.0, &session_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_permission_mode_maps_plan_and_auto() {
        assert_eq!(map_permission_mode("plan"), "plan");
        assert_eq!(map_permission_mode("yolo"), "auto");
        assert_eq!(map_permission_mode("default"), "auto");
    }

    #[tokio::test]
    async fn spawn_semaphore_serializes_permits() {
        let sem = spawn_semaphore();
        assert!(sem.available_permits() >= 1);
        let before = sem.available_permits();
        let permit = sem.acquire().await.expect("acquire permit");
        assert_eq!(sem.available_permits(), before - 1);
        drop(permit);
        assert_eq!(sem.available_permits(), before);
    }

    #[tokio::test]
    async fn keep_awake_persists_and_unions_with_list_awake_sessions() {
        let tmp_dir = std::env::temp_dir().join(format!("sakana-acp-test-{}", uuid::Uuid::new_v4()));
        let db = Database::open_in_dir(&tmp_dir).expect("open temp db");
        let db_arc = Arc::new(Mutex::new(db));
        let test_sid = format!("test-keep-awake-{}", uuid::Uuid::new_v4());

        add_keep_awake(&db_arc, &test_sid).await;
        {
            let db_guard = db_arc.lock().await;
            let saved = load_keep_awake_from_db(&db_guard);
            assert!(saved.contains(&test_sid));
        }

        let awake = list_awake_sessions_inner().await;
        assert!(awake.contains(&test_sid));

        let removed = remove_keep_awake(Some(&db_arc), &test_sid).await;
        assert!(removed);
        {
            let db_guard = db_arc.lock().await;
            let saved = load_keep_awake_from_db(&db_guard);
            assert!(!saved.contains(&test_sid));
        }

        let awake_after = list_awake_sessions_inner().await;
        assert!(!awake_after.contains(&test_sid));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
