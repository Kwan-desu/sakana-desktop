use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;

use chrono::Local;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use crate::models::{
    ChatMessage, HarnessAccountRow, HarnessModelOption, InlineWidgetData, McpServer, SkillItem,
    SubagentInfo, ToolCall, TraceSpan, WidgetBar, WidgetMetric,
};
use crate::DbState;

#[derive(Default)]
pub struct HarnessState {
    pub active_turns: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnTokenEvent {
    session_id: String,
    message_id: String,
    token: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnThinkingEvent {
    session_id: String,
    message_id: String,
    thinking: String,
    duration_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnToolEvent {
    session_id: String,
    message_id: String,
    tool: ToolCall,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnSubagentEvent {
    session_id: String,
    message_id: String,
    subagent: SubagentInfo,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnDoneEvent {
    session_id: String,
    message_id: String,
    message: ChatMessage,
    status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnErrorEvent {
    session_id: String,
    message_id: String,
    error: String,
}

fn harness_binary_candidates(harness: &str) -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    match harness {
        "agy-cli" => &["agy-cli.cmd", "agy-cli.exe", "agy-cli", "agy.cmd", "agy.exe", "agy"],
        "claude-code" => &["claude.cmd", "claude.exe", "claude"],
        "kiro-cli" => &["kiro-cli.cmd", "kiro-cli.exe", "kiro-cli", "kiro.cmd", "kiro.exe", "kiro"],
        _ => &[],
    }
    #[cfg(not(target_os = "windows"))]
    match harness {
        "agy-cli" => &["agy-cli", "agy"],
        "claude-code" => &["claude"],
        "kiro-cli" => &["kiro-cli", "kiro"],
        _ => &[],
    }
}

pub fn resolve_harness_binary(harness: &str) -> Option<PathBuf> {
    let candidates = harness_binary_candidates(harness);
    if candidates.is_empty() {
        return None;
    }

    let mut search_dirs: Vec<PathBuf> = Vec::new();
    if let Ok(path_var) = std::env::var("PATH") {
        search_dirs.extend(std::env::split_paths(&path_var));
    }

    if let Some(home) = dirs::home_dir() {
        search_dirs.push(home.join(".local").join("bin"));
        search_dirs.push(home.join(".npm-global").join("bin"));
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(local_app) = dirs::data_local_dir() {
            search_dirs.push(local_app.join("agy").join("bin"));
            search_dirs.push(local_app.join("Programs").join("Antigravity").join("bin"));
        }
        if let Some(roaming_app) = dirs::config_dir() {
            search_dirs.push(roaming_app.join("npm"));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        search_dirs.push(PathBuf::from("/usr/local/bin"));
        search_dirs.push(PathBuf::from("/opt/homebrew/bin"));
    }

    for dir in search_dirs {
        for bin in candidates {
            let full = dir.join(bin);
            if full.is_file() {
                return Some(full);
            }
        }
    }
    None
}

pub fn load_sakana_context(project_cwd: &str, wsl_distro: Option<&str>) -> String {
    let mut sections = Vec::new();

    if let Some(home) = dirs::home_dir() {
        for global_path in [
            home.join(".sakana").join("SAKANA.md"),
            home.join(".config").join("sakana").join("SAKANA.md"),
        ] {
            if let Ok(content) = std::fs::read_to_string(&global_path) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    sections.push(format!("# Global SAKANA.md\n{}", trimmed));
                }
            }
        }
    }

    let encoded_cwd = crate::platform::encode_wsl_path(wsl_distro, project_cwd);
    let project_root = if let Some((distro, linux_path)) = crate::platform::parse_wsl_path(&encoded_cwd) {
        crate::platform::wsl_to_unc_path(&distro, &linux_path)
    } else {
        PathBuf::from(project_cwd)
    };

    for (label, filename) in [
        ("Project SAKANA.md", "SAKANA.md"),
        ("Project CLAUDE.md", "CLAUDE.md"),
    ] {
        let project_file = project_root.join(filename);
        if let Ok(content) = std::fs::read_to_string(&project_file) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                sections.push(format!("# {}\n{}", label, trimmed));
            }
        }
    }

    sections.join("\n\n")
}

fn parse_skill_md(path: &Path, fallback_slug: &str, scope: &str) -> Option<SkillItem> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut slug = fallback_slug.to_string();
    let mut description = String::new();

    let trimmed = content.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---") {
        if let Some(end_idx) = rest.find("---") {
            let frontmatter = &rest[..end_idx];
            for line in frontmatter.lines() {
                let line = line.trim();
                if let Some(val) = line.strip_prefix("name:") {
                    let v = val.trim().trim_matches('"').trim_matches('\'');
                    if !v.is_empty() {
                        slug = v.to_string();
                    }
                } else if let Some(val) = line.strip_prefix("slug:") {
                    let v = val.trim().trim_matches('"').trim_matches('\'');
                    if !v.is_empty() {
                        slug = v.to_string();
                    }
                } else if let Some(val) = line.strip_prefix("description:") {
                    let v = val.trim().trim_matches('"').trim_matches('\'');
                    if !v.is_empty() {
                        description = v.to_string();
                    }
                }
            }
        }
    }

    if description.is_empty() {
        description = content
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with('#') && *l != "---")
            .unwrap_or("Local SKILL.md instruction")
            .chars()
            .take(140)
            .collect();
    }

    Some(SkillItem {
        slug,
        scope: scope.to_string(),
        description,
    })
}

fn scan_skill_dir(dir: &Path, scope: &str, out: &mut Vec<SkillItem>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let skill_file = path.join("SKILL.md");
            if skill_file.is_file() {
                let fallback = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("skill");
                if let Some(item) = parse_skill_md(&skill_file, fallback, scope) {
                    if !out.iter().any(|existing| existing.slug == item.slug) {
                        out.push(item);
                    }
                }
            }
        }
    }
}

pub fn discover_skills(project_cwd: Option<&str>) -> Vec<SkillItem> {
    let mut skills = Vec::new();

    if let Some(cwd) = project_cwd {
        let root = Path::new(cwd);
        scan_skill_dir(&root.join(".sakana").join("skills"), "project", &mut skills);
        scan_skill_dir(&root.join(".agents").join("skills"), "project", &mut skills);
        scan_skill_dir(&root.join("skills"), "project", &mut skills);
    }

    if let Some(home) = dirs::home_dir() {
        scan_skill_dir(&home.join(".sakana").join("skills"), "global", &mut skills);
        scan_skill_dir(&home.join(".gemini").join("config").join("skills"), "global", &mut skills);
        scan_skill_dir(&home.join(".claude").join("skills"), "global", &mut skills);
    }

    skills.sort_by(|a, b| a.slug.cmp(&b.slug));
    skills
}

#[tauri::command]
pub async fn list_skills(project_cwd: Option<String>) -> Result<Vec<SkillItem>, String> {
    Ok(discover_skills(project_cwd.as_deref()))
}

fn read_env_or_dotenv(key: &str, project_cwd: &str) -> Option<String> {
    if let Some(val) = crate::platform::get_env_or_dotenv(key) {
        return Some(val);
    }

    let env_path = Path::new(project_cwd).join(".env");
    if let Ok(content) = std::fs::read_to_string(env_path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                if k.trim() == key {
                    let cleaned = v.trim().trim_matches('"').trim_matches('\'').to_string();
                    if !cleaned.is_empty() {
                        return Some(cleaned);
                    }
                }
            }
        }
    }
    None
}

fn format_attachments(attachments: &[Value]) -> String {
    if attachments.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    for item in attachments {
        if let Some(s) = item.as_str() {
            parts.push(format!("- {}", s));
        } else if let Some(obj) = item.as_object() {
            let kind = obj
                .get("kind")
                .or_else(|| obj.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("context");
            let label = obj
                .get("label")
                .or_else(|| obj.get("path"))
                .or_else(|| obj.get("title"))
                .and_then(Value::as_str)
                .unwrap_or("attachment");
            let content = obj
                .get("content")
                .or_else(|| obj.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if content.is_empty() {
                parts.push(format!("[{}] {}", kind, label));
            } else {
                parts.push(format!("[{}] {}:\n{}", kind, label, content));
            }
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("\n\nAttached Context:\n{}", parts.join("\n"))
    }
}

fn parse_sakana_ui_widget(text: &str) -> Option<InlineWidgetData> {
    let mut search_from = 0;
    let block_inner = loop {
        let rest = text.get(search_from..)?;
        let fence_rel = rest.find("```")?;
        let after_ticks = &rest[fence_rel + 3..];
        let line_end = after_ticks.find('\n')?;
        let lang = after_ticks[..line_end].trim();
        let body_start = search_from + fence_rel + 3 + line_end + 1;
        if lang.eq_ignore_ascii_case("sakana-ui") {
            let body_rest = text.get(body_start..)?;
            let close_rel = body_rest.find("```")?;
            break body_rest[..close_rel].trim();
        }
        search_from = body_start;
    };

    if block_inner.is_empty() {
        return None;
    }

    if let Ok(val) = serde_json::from_str::<Value>(block_inner) {
        if let Some(obj) = val.as_object() {
            let title = obj
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Interactive Widget")
                .to_string();
            let subtitle = obj
                .get("subtitle")
                .and_then(Value::as_str)
                .unwrap_or("sakana-ui")
                .to_string();
            let source_code = obj
                .get("sourceCode")
                .or_else(|| obj.get("source_code"))
                .or_else(|| obj.get("html"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let metrics: Vec<WidgetMetric> = obj
                .get("metrics")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let bars: Vec<WidgetBar> = obj
                .get("bars")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            return Some(InlineWidgetData {
                title,
                subtitle,
                source_code,
                metrics,
                bars,
            });
        }
    }

    Some(InlineWidgetData {
        title: "Interactive Widget".to_string(),
        subtitle: "sakana-ui".to_string(),
        source_code: block_inner.to_string(),
        metrics: Vec::new(),
        bars: Vec::new(),
    })
}

pub(crate) struct TurnAccumulator {
    pub(crate) text: String,
    pub(crate) thinking: String,
    pub(crate) thinking_ms: i64,
    pub(crate) tools: Vec<ToolCall>,
    pub(crate) subagent: Option<SubagentInfo>,
    pub(crate) stream_error: Option<String>,
    pub(crate) input_tokens: i64,
    pub(crate) output_tokens: i64,
    pub(crate) cost_usd: f64,
    pub(crate) failover_notice: Option<String>,
}

impl TurnAccumulator {
    fn new() -> Self {
        Self {
            text: String::new(),
            thinking: String::new(),
            thinking_ms: 0,
            tools: Vec::new(),
            subagent: None,
            stream_error: None,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            failover_notice: None,
        }
    }

    fn into_message(self, message_id: String, author: String) -> ChatMessage {
        let widget = parse_sakana_ui_widget(&self.text);
        ChatMessage {
            id: message_id,
            role: "assistant".to_string(),
            author: Some(author),
            timestamp: Local::now().format("%H:%M").to_string(),
            text: self.text,
            thinking: if self.thinking.is_empty() {
                None
            } else {
                Some(self.thinking)
            },
            thinking_duration: if self.thinking_ms > 0 {
                Some(format!("{:.1}s", self.thinking_ms as f64 / 1000.0))
            } else {
                None
            },
            tools: if self.tools.is_empty() {
                None
            } else {
                Some(self.tools)
            },
            subagent: self.subagent,
            widget,
            checkpoint: None,
            failover_notice: self.failover_notice,
        }
    }
}

fn is_rate_limit_or_quota_error(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("usage limit")
        || lower.contains("quota exceeded")
        || lower.contains("quota_exceeded")
        || lower.contains("too many requests")
        || lower.contains("overloaded")
}

pub async fn get_failover_config_inner(
    db_arc: &Arc<Mutex<crate::db::Database>>,
    project_id: Option<String>,
) -> Result<crate::models::FailoverConfig, String> {
    let db = db_arc.lock().await;
    db.get_failover_config(project_id.as_deref())
        .map_err(|e| e.to_string())
}

pub async fn update_failover_config_inner(
    db_arc: &Arc<Mutex<crate::db::Database>>,
    project_id: Option<String>,
    mut config: crate::models::FailoverConfig,
) -> Result<crate::models::FailoverConfig, String> {
    config.quota_threshold_pct = config.quota_threshold_pct.clamp(10, 100);
    let db = db_arc.lock().await;
    db.save_failover_config(project_id.as_deref(), &config)
        .map_err(|e| e.to_string())?;
    Ok(config)
}

#[tauri::command]
pub async fn get_failover_config(
    db_state: State<'_, DbState>,
    project_id: Option<String>,
) -> Result<crate::models::FailoverConfig, String> {
    get_failover_config_inner(&db_state.0, project_id).await
}

#[tauri::command]
pub async fn update_failover_config(
    db_state: State<'_, DbState>,
    project_id: Option<String>,
    config: crate::models::FailoverConfig,
) -> Result<crate::models::FailoverConfig, String> {
    update_failover_config_inner(&db_state.0, project_id, config).await
}

pub(crate) fn emit_token(app: Option<&AppHandle>, session_id: &str, message_id: &str, token: &str, acc: &mut TurnAccumulator) {
    if token.is_empty() {
        return;
    }
    acc.text.push_str(token);
    crate::platform::emit_app_and_remote(
        app,
        "turn:token",
        TurnTokenEvent {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            token: token.to_string(),
        },
    );
}

pub(crate) fn emit_thinking(
    app: Option<&AppHandle>,
    session_id: &str,
    message_id: &str,
    thinking: &str,
    duration_ms: i64,
    acc: &mut TurnAccumulator,
) {
    if thinking.is_empty() {
        return;
    }
    acc.thinking.push_str(thinking);
    acc.thinking_ms = acc.thinking_ms.max(duration_ms);
    crate::platform::emit_app_and_remote(
        app,
        "turn:thinking",
        TurnThinkingEvent {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            thinking: thinking.to_string(),
            duration_ms,
        },
    );
}

pub(crate) fn emit_tool(app: Option<&AppHandle>, session_id: &str, message_id: &str, tool: ToolCall, acc: &mut TurnAccumulator) {
    if let Some(existing) = acc.tools.iter_mut().find(|t| t.id == tool.id) {
        *existing = tool.clone();
    } else {
        acc.tools.push(tool.clone());
    }
    crate::platform::emit_app_and_remote(
        app,
        "turn:tool",
        TurnToolEvent {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool,
        },
    );
}

fn process_stream_json_line(
    line: &str,
    app: Option<&AppHandle>,
    session_id: &str,
    message_id: &str,
    started: Instant,
    acc: &mut TurnAccumulator,
) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }

    let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
        emit_token(app, session_id, message_id, &format!("{}\n", line), acc);
        return;
    };

    let elapsed_ms = started.elapsed().as_millis() as i64;

    if let Some(event) = val.get("event").and_then(Value::as_str) {
        match event {
            "init" => {}
            "step_update" => {
                if let Some(su) = val.get("step_update") {
                    if let Some(thinking) = su
                        .get("thinking_delta")
                        .or_else(|| su.get("thinking"))
                        .and_then(Value::as_str)
                    {
                        let dur = su
                            .get("duration_seconds")
                            .and_then(Value::as_f64)
                            .map(|s| (s * 1000.0) as i64)
                            .unwrap_or(elapsed_ms);
                        emit_thinking(app, session_id, message_id, thinking, dur, acc);
                    }
                    if let Some(delta) = su.get("text_delta").and_then(Value::as_str) {
                        emit_token(app, session_id, message_id, delta, acc);
                    }
                    if let Some(tool_name) = su.get("tool_name").and_then(Value::as_str) {
                        let tool = ToolCall {
                            id: su
                                .get("tool_call_id")
                                .or_else(|| su.get("id"))
                                .and_then(Value::as_str)
                                .unwrap_or("tool")
                                .to_string(),
                            name: tool_name.to_string(),
                            target: su
                                .get("target")
                                .or_else(|| su.get("input"))
                                .map(|v| match v.as_str() {
                                    Some(s) => s.to_string(),
                                    None => v.to_string(),
                                })
                                .unwrap_or_default()
                                .chars()
                                .take(80)
                                .collect(),
                            status: su
                                .get("state")
                                .and_then(Value::as_str)
                                .unwrap_or("ok")
                                .to_lowercase(),
                            duration_ms: su
                                .get("duration_seconds")
                                .and_then(Value::as_f64)
                                .map(|s| (s * 1000.0) as i64)
                                .unwrap_or(elapsed_ms),
                        };
                        emit_tool(app, session_id, message_id, tool, acc);
                    }
                }
            }
            "result" => {
                if let Some(res) = val.get("result") {
                    extract_usage_from_value(res, acc);
                    let status = res.get("status").and_then(Value::as_str).unwrap_or("");
                    if status.eq_ignore_ascii_case("error") {
                        if let Some(err) = res.get("error").and_then(Value::as_str) {
                            let trimmed_err = err.trim();
                            if !trimmed_err.is_empty() {
                                acc.stream_error = Some(trimmed_err.to_string());
                            }
                        }
                    } else if acc.text.is_empty() {
                        if let Some(text) = res.get("response").and_then(Value::as_str) {
                            emit_token(app, session_id, message_id, text, acc);
                        }
                    }
                }
            }
            _ => {}
        }
        return;
    }

    let kind = val.get("type").and_then(Value::as_str).unwrap_or("");

    match kind {
        "content_block_delta" => {
            if let Some(delta) = val.get("delta") {
                if let Some(text) = delta.get("text").and_then(Value::as_str) {
                    emit_token(app, session_id, message_id, text, acc);
                }
                if let Some(thinking) = delta.get("thinking").and_then(Value::as_str) {
                    emit_thinking(app, session_id, message_id, thinking, elapsed_ms, acc);
                }
            }
        }
        "assistant" => {
            if let Some(content) = val
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(Value::as_array)
            {
                for block in content {
                    let btype = block.get("type").and_then(Value::as_str).unwrap_or("");
                    match btype {
                        "text" => {
                            if let Some(t) = block.get("text").and_then(Value::as_str) {
                                emit_token(app, session_id, message_id, t, acc);
                            }
                        }
                        "thinking" => {
                            if let Some(t) = block.get("thinking").and_then(Value::as_str) {
                                emit_thinking(app, session_id, message_id, t, elapsed_ms, acc);
                            }
                        }
                        "tool_use" => {
                            let tool = ToolCall {
                                id: block
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .unwrap_or("tool")
                                    .to_string(),
                                name: block
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("tool")
                                    .to_string(),
                                target: block
                                    .get("input")
                                    .map(|v| v.to_string())
                                    .unwrap_or_default()
                                    .chars()
                                    .take(80)
                                    .collect(),
                                status: "ok".to_string(),
                                duration_ms: elapsed_ms,
                            };
                            emit_tool(app, session_id, message_id, tool, acc);
                        }
                        _ => {}
                    }
                }
            }
        }
        "result" => {
            extract_usage_from_value(&val, acc);
            if acc.text.is_empty() {
                if let Some(res) = val.get("result").and_then(Value::as_str) {
                    emit_token(app, session_id, message_id, res, acc);
                }
            }
        }
        _ => {
            if let Some(t) = val
                .get("token")
                .or_else(|| val.get("text"))
                .or_else(|| val.get("content"))
                .and_then(Value::as_str)
            {
                emit_token(app, session_id, message_id, t, acc);
            }
        }
    }
}

fn extract_usage_from_value(val: &Value, acc: &mut TurnAccumulator) {
    let usage = val.get("usage").unwrap_or(val);
    if let Some(inp) = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(Value::as_i64)
    {
        acc.input_tokens = acc.input_tokens.max(inp);
    }
    if let Some(out) = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(Value::as_i64)
    {
        acc.output_tokens = acc.output_tokens.max(out);
    }
    if let Some(cost) = val
        .get("total_cost_usd")
        .or_else(|| val.get("cost_usd"))
        .or_else(|| usage.get("cost_usd"))
        .or_else(|| usage.get("total_cost"))
        .or_else(|| usage.get("cost"))
        .and_then(Value::as_f64)
    {
        if cost > acc.cost_usd {
            acc.cost_usd = cost;
        }
    }
}

pub fn build_permission_args(harness: &str, permission_mode: &str) -> Vec<&'static str> {
    match harness {
        "claude-code" => match permission_mode {
            "plan" => vec!["--permission-mode", "plan"],
            _ => vec!["--dangerously-skip-permissions"],
        },
        "agy-cli" => match permission_mode {
            "plan" => vec!["--mode", "plan"],
            _ => vec!["--dangerously-skip-permissions"],
        },
        _ => Vec::new(),
    }
}

fn build_mcp_config_json(servers: &[McpServer]) -> Option<Value> {
    let mut map = serde_json::Map::new();
    for s in servers {
        if !s.enabled {
            continue;
        }
        let endpoint = s.endpoint.trim();
        if endpoint.is_empty() {
            continue;
        }
        let is_http = s.transport.eq_ignore_ascii_case("http")
            || s.transport.eq_ignore_ascii_case("sse")
            || endpoint.starts_with("http://")
            || endpoint.starts_with("https://");
        let entry = if is_http {
            let kind = if s.transport.eq_ignore_ascii_case("http") {
                "http"
            } else {
                "sse"
            };
            serde_json::json!({
                "type": kind,
                "url": endpoint,
            })
        } else {
            let mut parts = endpoint.split_whitespace();
            let Some(command) = parts.next() else {
                continue;
            };
            let args: Vec<&str> = parts.collect();
            serde_json::json!({
                "command": command,
                "args": args,
            })
        };
        map.insert(s.name.clone(), entry);
    }
    if map.is_empty() {
        None
    } else {
        Some(serde_json::json!({ "mcpServers": map }))
    }
}

struct TempFileGuard {
    path: PathBuf,
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn run_cli_harness(
    bin: &Path,
    harness: &str,
    account: Option<&crate::models::HarnessAccountRow>,
    model: &str,
    effort: &str,
    permission_mode: &str,
    enabled_mcp_servers: &[McpServer],
    cwd: &str,
    full_prompt: &str,
    app: Option<&AppHandle>,
    session_id: &str,
    message_id: &str,
    started: Instant,
    acc: &mut TurnAccumulator,
    mut cancel_rx: oneshot::Receiver<()>,
) -> Result<(), String> {
    crate::auth::sync_profile_mcp_and_skills(harness, account, enabled_mcp_servers);
    let bin_str = bin.to_string_lossy().to_string();
    let mut cmd = crate::platform::tokio_command(&bin_str);
    crate::auth::apply_account_env(&mut cmd, harness, account);
    cmd.current_dir(cwd)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut _mcp_guard: Option<TempFileGuard> = None;

    match harness {
        "claude-code" => {
            cmd.args(["-p", full_prompt, "--output-format", "stream-json", "--verbose"]);
            if !model.trim().is_empty() {
                cmd.args(["--model", model.trim()]);
            }
            cmd.args(build_permission_args(harness, permission_mode));
            if let Some(mcp_cfg) = build_mcp_config_json(enabled_mcp_servers) {
                let cfg_path = std::env::temp_dir().join(format!("sakana-mcp-{}.json", message_id));
                if let Ok(json_str) = serde_json::to_string_pretty(&mcp_cfg) {
                    if std::fs::write(&cfg_path, json_str).is_ok() {
                        cmd.args(["--mcp-config", &cfg_path.to_string_lossy()]);
                        _mcp_guard = Some(TempFileGuard { path: cfg_path });
                    }
                }
            }
        }
        "agy-cli" => {
            cmd.args(["-p", full_prompt, "--output-format", "stream-json"]);
            if !model.trim().is_empty() {
                cmd.args(["--model", model.trim()]);
            }
            if matches!(effort.trim(), "low" | "medium" | "high" | "xhigh" | "max") {
                cmd.args(["--effort", effort.trim()]);
            }
            cmd.args(build_permission_args(harness, permission_mode));
        }
        "kiro-cli" => {
            cmd.args(["chat", "--no-interactive", full_prompt]);
        }
        _ => {
            cmd.args([full_prompt]);
        }
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn {} ({}): {}", harness, bin.display(), e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let mut stdout_lines = stdout.map(|s| BufReader::new(s).lines());
    let stderr_task = stderr.map(|s| {
        tokio::spawn(async move {
            let mut lines = BufReader::new(s).lines();
            let mut buf = String::new();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    buf.push_str(&line);
                    buf.push('\n');
                }
            }
            buf
        })
    });

    loop {
        tokio::select! {
            _ = &mut cancel_rx => {
                let _ = child.kill().await;
                if let Some(t) = stderr_task {
                    t.abort();
                }
                return Err("Turn cancelled by user.".to_string());
            }
            res = async {
                match stdout_lines.as_mut() {
                    Some(lines) => lines.next_line().await,
                    None => Ok(None),
                }
            } => {
                match res {
                    Ok(Some(line)) => {
                        process_stream_json_line(&line, app, session_id, message_id, started, acc);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        if let Some(t) = stderr_task {
                            t.abort();
                        }
                        return Err(format!("Error reading {} stdout: {}", harness, e));
                    }
                }
            }
        }
    }

    let stderr_buf = match stderr_task {
        Some(t) => t.await.unwrap_or_default(),
        None => String::new(),
    };

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed waiting on {}: {}", harness, e))?;

    if !status.success() && acc.text.is_empty() {
        let msg = if let Some(err) = acc.stream_error.take() {
            format!("{} failed: {}", harness, err)
        } else if stderr_buf.trim().is_empty() {
            format!("{} exited with status {}", harness, status)
        } else {
            format!("{} failed: {}", harness, stderr_buf.trim())
        };
        return Err(msg);
    }
    if acc.text.is_empty() {
        if let Some(err) = acc.stream_error.take() {
            return Err(format!("{} failed: {}", harness, err));
        }
    }

    Ok(())
}

pub(crate) fn wsl_posix_cwd(cwd: &str) -> String {
    if let Some((_, linux_path)) = crate::platform::parse_wsl_path(cwd) {
        linux_path
    } else {
        cwd.to_string()
    }
}

pub(crate) fn build_wsl_env_exports(
    harness: &str,
    account: Option<&crate::models::HarnessAccountRow>,
) -> Vec<String> {
    let mut env_exports = Vec::new();
    for key in [
        "ANTHROPIC_API_KEY",
        "GEMINI_API_KEY",
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "GOOGLE_API_KEY",
        "GOOGLE_CLOUD_PROJECT",
        "GOOGLE_CLOUD_LOCATION",
    ] {
        if let Some(val) = crate::platform::get_env_or_dotenv(key) {
            env_exports.push(format!(
                "export {}={}",
                key,
                crate::platform::shell_escape_single(&val)
            ));
        }
    }
    if harness == "agy-cli" {
        let agy_id = account
            .map(|a| a.id.trim())
            .filter(|id| !id.is_empty())
            .unwrap_or("default");
        let escaped_id = crate::platform::shell_escape_single(agy_id);
        env_exports.push("export AGY_CLI_FORCE_FILE_TOKEN_STORAGE=1".to_string());
        env_exports.push(format!(
            "_SAKANA_ORIG_HOME=\"$HOME\"; export PATH=\"$_SAKANA_ORIG_HOME/.local/bin:$_SAKANA_ORIG_HOME/.antigravity/bin:$PATH\"; _SAKANA_AGY_PROF=\"$_SAKANA_ORIG_HOME/.sakana/profiles/agy/\"{}; mkdir -p \"$_SAKANA_AGY_PROF/.gemini/antigravity-cli\" \"$_SAKANA_AGY_PROF/.gemini/config\" \"$_SAKANA_AGY_PROF/.gemini/antigravity\"; for _mc in \"$_SAKANA_ORIG_HOME/.gemini/config/mcp_config.json\" \"$_SAKANA_ORIG_HOME/.gemini/antigravity/mcp_config.json\"; do if [ -f \"$_mc\" ]; then ln -sf \"$_mc\" \"$_SAKANA_AGY_PROF/.gemini/config/mcp_config.json\"; ln -sf \"$_mc\" \"$_SAKANA_AGY_PROF/.gemini/antigravity-cli/mcp_config.json\"; ln -sf \"$_mc\" \"$_SAKANA_AGY_PROF/.gemini/antigravity/mcp_config.json\"; break; fi; done; for _sk in .gemini/config/skills .gemini/antigravity/skills .sakana/skills .agents/skills .claude/skills; do if [ -d \"$_SAKANA_ORIG_HOME/$_sk\" ] && [ ! -e \"$_SAKANA_AGY_PROF/$_sk\" ]; then mkdir -p \"$(dirname \"$_SAKANA_AGY_PROF/$_sk\")\"; ln -s \"$_SAKANA_ORIG_HOME/$_sk\" \"$_SAKANA_AGY_PROF/$_sk\"; fi; done; export HOME=\"$_SAKANA_AGY_PROF\"; export SAKANA_AGY_GEMINI_DIR=\"$_SAKANA_AGY_PROF/.gemini\"",
            escaped_id
        ));
    } else if let Some(acc) = account.filter(|a| !a.is_default && !a.id.trim().is_empty()) {
        let id = acc.id.trim();
        match harness {
            "claude-code" => {
                env_exports.push(format!(
                    "export CLAUDE_CONFIG_DIR=\"$HOME/.sakana/profiles/{}\"",
                    id
                ));
            }
            "kiro-cli" => {
                env_exports.push(format!(
                    "export XDG_DATA_HOME=\"$HOME/.sakana/profiles/{}/.local/share\"",
                    id
                ));
            }
            _ => {}
        }
    }
    env_exports
}

async fn run_wsl_cli_harness(
    distro: &str,
    harness: &str,
    account: Option<&crate::models::HarnessAccountRow>,
    model: &str,
    effort: &str,
    permission_mode: &str,
    enabled_mcp_servers: &[McpServer],
    cwd: &str,
    full_prompt: &str,
    app: Option<&AppHandle>,
    session_id: &str,
    message_id: &str,
    started: Instant,
    acc: &mut TurnAccumulator,
    mut cancel_rx: oneshot::Receiver<()>,
) -> Result<(), String> {
    let env_exports = build_wsl_env_exports(harness, account);

    let mut mcp_setup_cmd: Option<String> = None;
    let mut wsl_mcp_path: Option<String> = None;
    if harness == "claude-code" {
        if let Some(mcp_cfg) = build_mcp_config_json(enabled_mcp_servers) {
            if let Ok(json_str) = serde_json::to_string_pretty(&mcp_cfg) {
                let tmp_path = format!("/tmp/sakana-mcp-{}.json", Uuid::new_v4());
                let escaped_json = crate::platform::shell_escape_single(&json_str);
                let escaped_path = crate::platform::shell_escape_single(&tmp_path);
                mcp_setup_cmd = Some(format!(
                    "printf '%s' {} > {} && trap 'rm -f {}' EXIT",
                    escaped_json, escaped_path, escaped_path
                ));
                wsl_mcp_path = Some(tmp_path);
            }
        }
    }

    let escaped_prompt = crate::platform::shell_escape_single(full_prompt);
    let mut cli_cmd = match harness {
        "claude-code" => {
            let mut parts = vec![
                "claude".to_string(),
                "-p".to_string(),
                escaped_prompt,
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--verbose".to_string(),
            ];
            if !model.trim().is_empty() {
                parts.push("--model".to_string());
                parts.push(crate::platform::shell_escape_single(model.trim()));
            }
            for flag in build_permission_args(harness, permission_mode) {
                parts.push(flag.to_string());
            }
            if let Some(ref mcp_path) = wsl_mcp_path {
                parts.push("--mcp-config".to_string());
                parts.push(crate::platform::shell_escape_single(mcp_path));
            }
            parts.join(" ")
        }
        "agy-cli" => {
            let mut parts = vec![
                "$(command -v agy-cli || command -v agy || echo agy)".to_string(),
                "-p".to_string(),
                escaped_prompt,
                "--output-format".to_string(),
                "stream-json".to_string(),
            ];
            if !model.trim().is_empty() {
                parts.push("--model".to_string());
                parts.push(crate::platform::shell_escape_single(model.trim()));
            }
            if matches!(effort.trim(), "low" | "medium" | "high" | "xhigh" | "max") {
                parts.push("--effort".to_string());
                parts.push(crate::platform::shell_escape_single(effort.trim()));
            }
            for flag in build_permission_args(harness, permission_mode) {
                parts.push(flag.to_string());
            }
            parts.join(" ")
        }
        "kiro-cli" => format!("kiro-cli chat --no-interactive {}", escaped_prompt),
        _ => format!("echo 'Unsupported WSL harness: {}'", harness),
    };

    if let Some(setup) = mcp_setup_cmd {
        cli_cmd = format!("{}; {}", setup, cli_cmd);
    }
    if !env_exports.is_empty() {
        cli_cmd = format!("{}; {}", env_exports.join("; "), cli_cmd);
    }

    let posix_cwd = wsl_posix_cwd(cwd);
    let mut cmd = crate::platform::wsl_tokio_shell(distro, &posix_cwd, &cli_cmd);
    crate::auth::apply_account_env(&mut cmd, harness, account);
    cmd.kill_on_drop(true).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn {} in WSL ({}): {}", harness, distro, e))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut stdout_lines = stdout.map(|s| BufReader::new(s).lines());
    let stderr_task = stderr.map(|s| {
        tokio::spawn(async move {
            let mut lines = BufReader::new(s).lines();
            let mut buf = String::new();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    if !buf.is_empty() {
                        buf.push('\n');
                    }
                    buf.push_str(&line);
                }
            }
            buf
        })
    });

    loop {
        tokio::select! {
            _ = &mut cancel_rx => {
                let _ = child.kill().await;
                if let Some(t) = stderr_task {
                    t.abort();
                }
                return Err("Turn cancelled by user.".to_string());
            }
            res = async {
                match stdout_lines.as_mut() {
                    Some(lines) => lines.next_line().await,
                    None => Ok(None),
                }
            } => {
                match res {
                    Ok(Some(line)) => {
                        process_stream_json_line(&line, app, session_id, message_id, started, acc);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        if let Some(t) = stderr_task {
                            t.abort();
                        }
                        return Err(format!("Error reading WSL CLI stdout: {}", e));
                    }
                }
            }
        }
    }
    let stderr_buf = match stderr_task {
        Some(t) => t.await.unwrap_or_default(),
        None => String::new(),
    };
    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed waiting on WSL {}: {}", harness, e))?;
    if !status.success() && acc.text.is_empty() {
        let err_detail = acc
            .stream_error
            .take()
            .unwrap_or_else(|| stderr_buf.trim().to_string());
        return Err(format!(
            "WSL {} ({}) exited with {}: {}",
            harness, distro, status, err_detail
        ));
    }
    if acc.text.is_empty() {
        if let Some(err) = acc.stream_error.take() {
            return Err(format!("WSL {} ({}) failed: {}", harness, distro, err));
        }
    }
    Ok(())
}

const COMPACTION_CONFIG_KEY: &str = "compaction_config";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionConfig {
    pub auto_compact: bool,
    pub message_threshold: usize,
    pub char_threshold: usize,
    pub keep_recent: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            auto_compact: true,
            message_threshold: 36,
            char_threshold: 32_000,
            keep_recent: 8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionCompactionState {
    summary: String,
    compacted_up_to_id: String,
    compacted_count: usize,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionResult {
    pub session_id: String,
    pub compacted_count: usize,
    pub remaining_count: usize,
    pub summary: String,
    pub message: Option<ChatMessage>,
    pub auto_triggered: bool,
}

fn compaction_setting_key(session_id: &str) -> String {
    format!("compaction:{}", session_id)
}

pub fn load_compaction_config(db: &crate::db::Database) -> CompactionConfig {
    db.get_setting(COMPACTION_CONFIG_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<CompactionConfig>(&raw).ok())
        .unwrap_or_default()
}

fn save_compaction_config(
    db: &crate::db::Database,
    cfg: &CompactionConfig,
) -> Result<(), String> {
    let json = serde_json::to_string(cfg).map_err(|e| e.to_string())?;
    db.set_setting(COMPACTION_CONFIG_KEY, &json)
        .map_err(|e| e.to_string())
}

fn load_session_compaction(
    db: &crate::db::Database,
    session_id: &str,
) -> Option<SessionCompactionState> {
    db.get_setting(&compaction_setting_key(session_id))
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<SessionCompactionState>(&raw).ok())
}

fn save_session_compaction(
    db: &crate::db::Database,
    session_id: &str,
    state: &SessionCompactionState,
) -> Result<(), String> {
    let json = serde_json::to_string(state).map_err(|e| e.to_string())?;
    db.set_setting(&compaction_setting_key(session_id), &json)
        .map_err(|e| e.to_string())
}

fn is_compaction_marker(msg: &ChatMessage) -> bool {
    msg.role == "system" && msg.author.as_deref() == Some("Context Compacted")
}

/// Load full retained memory for a session: optional compacted summary + all messages after
/// the compaction boundary (never dropping open-thread memory at an arbitrary 16-message cutoff).
fn load_session_memory_for_turn(
    db: &crate::db::Database,
    session_id: &str,
) -> (Option<String>, Vec<ChatMessage>) {
    let all_msgs = db.list_recent_messages(session_id, 400).unwrap_or_default();
    let compaction = load_session_compaction(db, session_id);
    let uncompacted: Vec<ChatMessage> = if let Some(ref st) = compaction {
        if let Some(pos) = all_msgs.iter().position(|m| m.id == st.compacted_up_to_id) {
            all_msgs[pos + 1..]
                .iter()
                .filter(|m| !is_compaction_marker(m))
                .cloned()
                .collect()
        } else {
            all_msgs
                .into_iter()
                .filter(|m| !is_compaction_marker(m))
                .collect()
        }
    } else {
        all_msgs
            .into_iter()
            .filter(|m| !is_compaction_marker(m))
            .collect()
    };
    let summary = compaction.map(|c| c.summary).filter(|s| !s.trim().is_empty());
    (summary, uncompacted)
}

fn build_extractive_compaction_summary(
    prev_summary: Option<&str>,
    to_compact: &[ChatMessage],
) -> String {
    let mut lines: Vec<String> = Vec::new();
    if let Some(prev) = prev_summary {
        let trimmed = prev.trim();
        if !trimmed.is_empty() {
            let kept: String = trimmed.chars().take(2800).collect();
            lines.push(kept);
        }
    }
    for m in to_compact {
        let text = m.text.trim();
        if text.is_empty() || is_compaction_marker(m) {
            continue;
        }
        let role_tag = match m.role.as_str() {
            "assistant" => "Assistant",
            "system" => "System",
            _ => "User",
        };
        let compact_line: String = text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let snippet: String = if compact_line.chars().count() > 220 {
            format!("{}…", compact_line.chars().take(220).collect::<String>())
        } else {
            compact_line
        };
        lines.push(format!("- [{}] {}", role_tag, snippet));
    }
    let joined = lines.join("\n");
    if joined.chars().count() > 5500 {
        let tail: String = joined
            .chars()
            .rev()
            .take(5400)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        format!("(Earlier summary condensed)\n{}", tail)
    } else {
        joined
    }
}

pub async fn compact_session_inner(
    app: Option<&AppHandle>,
    db: &Arc<Mutex<crate::db::Database>>,
    session_id: &str,
    auto_triggered: bool,
) -> Result<CompactionResult, String> {
    let (cfg, prev_compaction, uncompacted) = {
        let db_lock = db.lock().await;
        let cfg = load_compaction_config(&db_lock);
        let prev = load_session_compaction(&db_lock, session_id);
        let (_, msgs) = load_session_memory_for_turn(&db_lock, session_id);
        (cfg, prev, msgs)
    };

    let keep_recent = cfg.keep_recent.clamp(2, 30);
    if uncompacted.len() <= keep_recent {
        return Ok(CompactionResult {
            session_id: session_id.to_string(),
            compacted_count: 0,
            remaining_count: uncompacted.len(),
            summary: prev_compaction.map(|c| c.summary).unwrap_or_default(),
            message: None,
            auto_triggered,
        });
    }

    let split_idx = uncompacted.len() - keep_recent;
    let to_compact = &uncompacted[..split_idx];
    let boundary_id = to_compact
        .last()
        .map(|m| m.id.clone())
        .unwrap_or_default();

    let summary = build_extractive_compaction_summary(
        prev_compaction.as_ref().map(|c| c.summary.as_str()),
        to_compact,
    );
    let total_compacted = prev_compaction.as_ref().map(|c| c.compacted_count).unwrap_or(0)
        + to_compact.len();

    let state = SessionCompactionState {
        summary: summary.clone(),
        compacted_up_to_id: boundary_id,
        compacted_count: total_compacted,
        updated_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };

    let notice_msg = ChatMessage {
        id: format!("msg-{}", Uuid::new_v4()),
        role: "system".to_string(),
        author: Some("Context Compacted".to_string()),
        timestamp: Local::now().format("%H:%M").to_string(),
        text: format!(
            "{}Compacted {} older messages into retained memory ({} recent messages kept verbatim).\n\n{}",
            if auto_triggered { "[Auto-Compact] " } else { "" },
            to_compact.len(),
            keep_recent,
            summary.chars().take(600).collect::<String>()
        ),
        thinking: None,
        thinking_duration: None,
        tools: None,
        subagent: None,
        widget: None,
        checkpoint: None,
        failover_notice: None,
    };

    {
        let db_lock = db.lock().await;
        save_session_compaction(&db_lock, session_id, &state)?;
        let _ = db_lock.append_message(session_id, &notice_msg);
    }

    let res = CompactionResult {
        session_id: session_id.to_string(),
        compacted_count: to_compact.len(),
        remaining_count: keep_recent,
        summary,
        message: Some(notice_msg),
        auto_triggered,
    };

    crate::platform::emit_app_and_remote(app, "session:compacted", &res);
    Ok(res)
}

async fn maybe_auto_compact_session(
    app: Option<&AppHandle>,
    db: &Arc<Mutex<crate::db::Database>>,
    session_id: &str,
) {
    let should_compact = {
        let db_lock = db.lock().await;
        let cfg = load_compaction_config(&db_lock);
        if !cfg.auto_compact {
            false
        } else {
            let (_, uncompacted) = load_session_memory_for_turn(&db_lock, session_id);
            let total_chars: usize = uncompacted.iter().map(|m| m.text.len()).sum();
            uncompacted.len() >= cfg.message_threshold.max(12)
                || total_chars >= cfg.char_threshold.max(8_000)
        }
    };
    if should_compact {
        let _ = compact_session_inner(app, db, session_id, true).await;
    }
}

fn build_cli_prompt_with_history(
    sakana_ctx: &str,
    history: &[ChatMessage],
    current_prompt: &str,
) -> String {
    let prior = match history.last() {
        Some(last) if last.role == "user" => &history[..history.len() - 1],
        _ => history,
    };

    let mut history_lines = Vec::new();
    for m in prior {
        if is_compaction_marker(m) {
            continue;
        }
        let trimmed = m.text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let role_label = match m.role.as_str() {
            "assistant" => "Assistant",
            "system" => "System",
            _ => "User",
        };
        history_lines.push(format!("[{}]: {}", role_label, trimmed));
    }

    // Keep CLI prompt safely under Windows CreateProcess 32KB command-line limit
    // by trimming oldest uncompacted lines first (compacted summary in sakana_ctx is preserved).
    const MAX_HISTORY_CHARS: usize = 20_000;
    let mut kept_rev: Vec<String> = Vec::new();
    let mut used_chars: usize = 0;
    for line in history_lines.into_iter().rev() {
        let line_len = line.len() + 1;
        if used_chars + line_len > MAX_HISTORY_CHARS && !kept_rev.is_empty() {
            break;
        }
        used_chars += line_len;
        kept_rev.push(line);
    }
    kept_rev.reverse();

    let mut sections = Vec::new();
    if !sakana_ctx.trim().is_empty() {
        sections.push(sakana_ctx.trim().to_string());
    }
    if !kept_rev.is_empty() {
        sections.push(format!(
            "<conversation_history>\n{}\n</conversation_history>",
            kept_rev.join("\n")
        ));
    }
    sections.push(current_prompt.to_string());
    sections.join("\n\n---\n\n")
}

fn build_http_messages(
    system_context: &str,
    history: &[ChatMessage],
    user_prompt: &str,
) -> Vec<Value> {
    let mut messages = Vec::new();
    if !system_context.trim().is_empty() {
        messages.push(serde_json::json!({
            "role": "system",
            "content": system_context,
        }));
    }

    // If the latest persisted message is the current user turn we just appended,
    // replace it with `user_prompt` (which may include attachments/delegation context).
    let prior = match history.last() {
        Some(last) if last.role == "user" => &history[..history.len() - 1],
        _ => history,
    };

    for m in prior {
        if is_compaction_marker(m) || m.text.trim().is_empty() {
            continue;
        }
        let role = match m.role.as_str() {
            "assistant" => "assistant",
            "system" => "system",
            _ => "user",
        };
        messages.push(serde_json::json!({
            "role": role,
            "content": m.text,
        }));
    }

    messages.push(serde_json::json!({
        "role": "user",
        "content": user_prompt,
    }));
    messages
}

async fn run_http_sse_fallback(
    project_cwd: &str,
    model: &str,
    system_context: &str,
    history: &[ChatMessage],
    user_prompt: &str,
    app: Option<&AppHandle>,
    session_id: &str,
    message_id: &str,
    started: Instant,
    acc: &mut TurnAccumulator,
    mut cancel_rx: oneshot::Receiver<()>,
) -> Result<(), String> {
    let openrouter_key = read_env_or_dotenv("OPENROUTER_API_KEY", project_cwd);
    let openai_key = read_env_or_dotenv("OPENAI_API_KEY", project_cwd);
    let ollama_url = read_env_or_dotenv("OLLAMA_BASE_URL", project_cwd);

    let (endpoint, api_key, resolved_model) = if let Some(key) = openrouter_key {
        let m = if model.contains('/') {
            model.to_string()
        } else {
            format!("anthropic/{}", model)
        };
        (
            "https://openrouter.ai/api/v1/chat/completions".to_string(),
            Some(key),
            m,
        )
    } else if let Some(key) = openai_key {
        (
            "https://api.openai.com/v1/chat/completions".to_string(),
            Some(key),
            model.to_string(),
        )
    } else if let Some(base) = ollama_url {
        let base_trimmed = base.trim_end_matches('/');
        (
            format!("{}/v1/chat/completions", base_trimmed),
            None,
            model.to_string(),
        )
    } else {
        return Err(
            "No LLM API key or local CLI found. Configure OPENROUTER_API_KEY, OPENAI_API_KEY, or OLLAMA_BASE_URL in .env / environment, or install agy-cli / claude-code / kiro-cli."
                .to_string(),
        );
    };

    let messages = build_http_messages(system_context, history, user_prompt);

    let body = serde_json::json!({
        "model": resolved_model,
        "stream": true,
        "messages": messages,
    });

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let mut req = client.post(&endpoint).json(&body);
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }

    let resp = tokio::select! {
        _ = &mut cancel_rx => return Err("Turn cancelled by user.".to_string()),
        r = req.send() => r.map_err(|e| format!("HTTP request failed: {}", e))?,
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Provider returned HTTP {}: {}", status, text));
    }

    let mut stream = resp.bytes_stream();
    let mut utf8_carry: Vec<u8> = Vec::new();
    let mut buffer = String::new();

    loop {
        let chunk_opt = tokio::select! {
            _ = &mut cancel_rx => return Err("Turn cancelled by user.".to_string()),
            c = stream.next() => c,
        };

        let Some(chunk_res) = chunk_opt else {
            break;
        };
        let chunk = chunk_res.map_err(|e| format!("Stream read error: {}", e))?;
        utf8_carry.extend_from_slice(&chunk);
        match std::str::from_utf8(&utf8_carry) {
            Ok(s) => {
                buffer.push_str(s);
                utf8_carry.clear();
            }
            Err(e) => {
                let valid_up_to = e.valid_up_to();
                if valid_up_to > 0 {
                    if let Ok(valid_str) = std::str::from_utf8(&utf8_carry[..valid_up_to]) {
                        buffer.push_str(valid_str);
                    }
                    utf8_carry.drain(..valid_up_to);
                }
                if let Some(err_len) = e.error_len() {
                    let lossy = String::from_utf8_lossy(&utf8_carry[..err_len]);
                    buffer.push_str(&lossy);
                    utf8_carry.drain(..err_len);
                }
            }
        }

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if !line.starts_with("data:") {
                continue;
            }
            let payload = line.trim_start_matches("data:").trim();
            if payload == "[DONE]" {
                return Ok(());
            }
            if let Ok(json) = serde_json::from_str::<Value>(payload) {
                extract_usage_from_value(&json, acc);
                if let Some(delta) = json
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("delta"))
                {
                    if let Some(reasoning) = delta
                        .get("reasoning_content")
                        .or_else(|| delta.get("reasoning"))
                        .and_then(Value::as_str)
                    {
                        emit_thinking(
                            app,
                            session_id,
                            message_id,
                            reasoning,
                            started.elapsed().as_millis() as i64,
                            acc,
                        );
                    }
                    if let Some(content) = delta.get("content").and_then(Value::as_str) {
                        emit_token(app, session_id, message_id, content, acc);
                    }
                }
            }
        }
    }

    Ok(())
}

fn parse_cross_harness_command(prompt: &str, primary_harness: &str) -> Option<(String, String, String)> {
    let trimmed = prompt.trim();
    if let Some(rest) = trimmed.strip_prefix("/verify") {
        let secondary = if primary_harness == "agy-cli" {
            "claude-code"
        } else {
            "agy-cli"
        };
        let task = if rest.trim().is_empty() {
            "Verify recent workspace changes and report any issues.".to_string()
        } else {
            rest.trim().to_string()
        };
        return Some((secondary.to_string(), "/verify".to_string(), task));
    }
    if let Some(rest) = trimmed.strip_prefix("/delegate") {
        let rest_trimmed = rest.trim();
        let mut parts = rest_trimmed.splitn(2, char::is_whitespace);
        let first = parts.next().unwrap_or("");
        let (target_harness, task) = if matches!(first, "agy-cli" | "claude-code" | "kiro-cli" | "sakana") {
            (
                first.to_string(),
                parts.next().unwrap_or("").trim().to_string(),
            )
        } else {
            let fallback = if primary_harness == "claude-code" {
                "agy-cli"
            } else {
                "claude-code"
            };
            (fallback.to_string(), rest_trimmed.to_string())
        };
        return Some((target_harness, "/delegate".to_string(), task));
    }
    None
}

#[derive(Default)]
pub struct TurnRunOptions {
    pub extra_system_context: Option<String>,
    pub completion_tx: Option<tokio::sync::oneshot::Sender<Result<ChatMessage, String>>>,
    pub author_override: Option<String>,
    pub peer_session_id: Option<String>,
}

pub async fn send_turn_inner(
    app: Option<AppHandle>,
    db_arc: Arc<Mutex<crate::db::Database>>,
    active_arc: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    session_id: String,
    prompt: String,
    attachments: Option<Vec<Value>>,
) -> Result<String, String> {
    send_turn_with_options(
        app,
        db_arc,
        active_arc,
        session_id,
        prompt,
        attachments,
        TurnRunOptions::default(),
    )
    .await
}

pub async fn send_turn_with_options(
    app: Option<AppHandle>,
    db_arc: Arc<Mutex<crate::db::Database>>,
    active_arc: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    session_id: String,
    prompt: String,
    attachments: Option<Vec<Value>>,
    opts: TurnRunOptions,
) -> Result<String, String> {
    let (session, project_cwd, project_wsl_distro) = {
        let db = db_arc.lock().await;
        let session = db
            .get_session(&session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Session '{}' not found", session_id))?;

        let proj = db
            .get_project(&session.project_id)
            .map_err(|e| e.to_string())?;
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

        let user_msg = ChatMessage {
            id: format!("msg-{}", Uuid::new_v4()),
            role: "user".to_string(),
            author: None,
            timestamp: Local::now().format("%H:%M").to_string(),
            text: prompt.clone(),
            thinking: None,
            thinking_duration: None,
            tools: None,
            subagent: None,
            widget: None,
            checkpoint: None,
            failover_notice: None,
        };
        db.append_message(&session_id, &user_msg)
            .map_err(|e| e.to_string())?;
        db.update_session_status(&session_id, "RUNNING", Some(false))
            .map_err(|e| e.to_string())?;

        (session, proj_cwd, proj_wsl)
    };

    let message_id = format!("msg-{}", Uuid::new_v4());
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

    {
        let mut active = active_arc.lock().await;
        if let Some(prev_tx) = active.insert(session_id.clone(), cancel_tx) {
            let _ = prev_tx.send(());
        }
    }

    let app_handle = app;
    let msg_id_clone = message_id.clone();
    let sid = session_id.clone();
    let attach_text = attachments.as_deref().map(format_attachments).unwrap_or_default();
    let TurnRunOptions {
        extra_system_context,
        mut completion_tx,
        author_override,
        peer_session_id,
    } = opts;

    tokio::spawn(async move {
        let started = Instant::now();
        let mut acc = TurnAccumulator::new();

        let effective_cwd = if project_wsl_distro.is_some() {
            if !session.worktree_path.trim().is_empty() && session.worktree_path != "." {
                session.worktree_path.clone()
            } else {
                project_cwd.clone()
            }
        } else if Path::new(&session.worktree_path).exists() && session.worktree_path != "." {
            session.worktree_path.clone()
        } else {
            project_cwd.clone()
        };

        let mut sakana_ctx = load_sakana_context(&effective_cwd, project_wsl_distro.as_deref());
        if let Some(extra) = extra_system_context.as_deref() {
            let trimmed = extra.trim();
            if !trimmed.is_empty() {
                if sakana_ctx.trim().is_empty() {
                    sakana_ctx = trimmed.to_string();
                } else {
                    sakana_ctx = format!("{}\n\n{}", trimmed, sakana_ctx);
                }
            }
        }
        let cross_cmd = parse_cross_harness_command(&prompt, &session.harness);

        let (active_harness, effective_prompt) = if let Some((ref target_h, ref cmd_name, ref task)) = cross_cmd {
            let sub = SubagentInfo {
                harness: target_h.clone(),
                command: cmd_name.clone(),
                summary: format!("Delegated to {} ({})", target_h, cmd_name),
                duration_ms: 0,
            };
            acc.subagent = Some(sub.clone());
            crate::platform::emit_app_and_remote(
                app_handle.as_ref(),
                "turn:subagent",
                TurnSubagentEvent {
                    session_id: sid.clone(),
                    message_id: msg_id_clone.clone(),
                    subagent: sub,
                },
            );
            (target_h.clone(), format!("{}{}", task, attach_text))
        } else {
            (session.harness.clone(), format!("{}{}", prompt, attach_text))
        };

        // Auto-compact long threads before building turn context so open threads retain full memory.
        maybe_auto_compact_session(app_handle.as_ref(), &db_arc, &sid).await;

        let (active_account, compacted_summary, recent_history, enabled_mcp_servers, self_control_ctx, split_peer_ctx) = {
            let db = db_arc.lock().await;
            let account = db
                .get_harness_account(&session.account)
                .ok()
                .flatten()
                .filter(|a| a.harness == active_harness)
                .or_else(|| db.get_default_harness_account(&active_harness).ok().flatten());
            let (summary, history) = load_session_memory_for_turn(&db, &sid);
            let mcp_servers: Vec<McpServer> = db
                .list_mcp_servers()
                .unwrap_or_default()
                .into_iter()
                .filter(|s| s.enabled)
                .collect();
            let sc_ctx = crate::self_control::build_self_control_prompt(&db);
            let peer_ctx = peer_session_id
                .as_deref()
                .map(|pid| crate::self_control::build_split_peer_prompt(&db, pid))
                .unwrap_or_default();
            (account, summary, history, mcp_servers, sc_ctx, peer_ctx)
        };

        if let Some(summary) = compacted_summary {
            let block = format!(
                "<compacted_conversation_memory>\n{}\n</compacted_conversation_memory>",
                summary
            );
            if sakana_ctx.trim().is_empty() {
                sakana_ctx = block;
            } else {
                sakana_ctx = format!("{}\n\n{}", sakana_ctx, block);
            }
        }

        if !self_control_ctx.trim().is_empty() {
            if sakana_ctx.trim().is_empty() {
                sakana_ctx = self_control_ctx;
            } else {
                sakana_ctx = format!("{}\n\n{}", sakana_ctx, self_control_ctx);
            }
        }
        if !split_peer_ctx.trim().is_empty() {
            if sakana_ctx.trim().is_empty() {
                sakana_ctx = split_peer_ctx;
            } else {
                sakana_ctx = format!("{}\n\n{}", sakana_ctx, split_peer_ctx);
            }
        }

        let full_prompt_with_ctx =
            build_cli_prompt_with_history(&sakana_ctx, &recent_history, &effective_prompt);

        // Auto-Failover & Smart Quota Routing (pre-check + up to 3 retry hops)
        let failover_cfg = {
            let db = db_arc.lock().await;
            db.get_failover_config(Some(&session.project_id)).unwrap_or_default()
        };

        let mut cur_harness = active_harness.clone();
        let mut cur_model = session.model.clone();
        let mut cur_account = active_account.clone();
        let mut tried_accounts: Vec<String> = cur_account
            .as_ref()
            .map(|a| vec![a.id.clone()])
            .unwrap_or_default();
        let mut fallback_idx: usize = 0;
        let mut failover_hops: u8 = 0;

        // Pre-check cached QuotaAccount percentages before spawning
        if failover_cfg.enabled {
            let quota_exceeded_reason: Option<String> = {
                let db = db_arc.lock().await;
                let quotas = db.list_accounts().unwrap_or_default();
                let cur_label = cur_account.as_ref().map(|a| a.label.as_str()).unwrap_or("");
                let cur_email = cur_account.as_ref().map(|a| a.email.as_str()).unwrap_or("");
                quotas.iter().find(|q| {
                    q.harness == cur_harness
                        && ((!cur_label.is_empty() && q.label.eq_ignore_ascii_case(cur_label))
                            || (!cur_email.is_empty() && q.email.eq_ignore_ascii_case(cur_email))
                            || q.is_default.unwrap_or(false))
                }).and_then(|q| {
                    let threshold = failover_cfg.quota_threshold_pct as f64;
                    if q.five_hour_pct >= threshold {
                        Some(format!("5h limit {:.0}%", q.five_hour_pct))
                    } else if q.weekly_pct >= threshold {
                        Some(format!("weekly limit {:.0}%", q.weekly_pct))
                    } else {
                        None
                    }
                })
            };
            if let Some(reason) = quota_exceeded_reason {
                let from_h = cur_harness.clone();
                let from_acc_label = cur_account
                    .as_ref()
                    .map(|a| a.label.clone())
                    .unwrap_or_else(|| "default".to_string());
                let mut switched = false;
                if failover_cfg.rotate_accounts_same_harness {
                    let next_acc = {
                        let db = db_arc.lock().await;
                        db.list_harness_accounts(Some(&cur_harness))
                            .unwrap_or_default()
                            .into_iter()
                            .find(|a| a.status == "authenticated" && !tried_accounts.contains(&a.id))
                    };
                    if let Some(na) = next_acc {
                        tried_accounts.push(na.id.clone());
                        cur_account = Some(na);
                        switched = true;
                    }
                }
                if !switched && fallback_idx < failover_cfg.fallback_chain.len() {
                    let target = &failover_cfg.fallback_chain[fallback_idx];
                    fallback_idx += 1;
                    cur_harness = target.harness.clone();
                    if !target.model.trim().is_empty() {
                        cur_model = target.model.clone();
                    }
                    cur_account = {
                        let db = db_arc.lock().await;
                        target
                            .account
                            .as_deref()
                            .and_then(|aid| db.get_harness_account(aid).ok().flatten())
                            .or_else(|| db.get_default_harness_account(&cur_harness).ok().flatten())
                    };
                    switched = true;
                }
                if switched {
                    failover_hops += 1;
                    let to_acc_label = cur_account
                        .as_ref()
                        .map(|a| a.label.clone())
                        .unwrap_or_else(|| "default".to_string());
                    let notice = if from_h == cur_harness {
                        format!("Switched account: {} -> {} ({})", from_acc_label, to_acc_label, reason)
                    } else {
                        format!("Failover: {}/{} -> {}/{} ({})", from_h, from_acc_label, cur_harness, to_acc_label, reason)
                    };
                    acc.failover_notice = Some(notice.clone());
                    crate::platform::emit_app_and_remote(
                        app_handle.as_ref(),
                        "turn:failover",
                        crate::models::TurnFailoverEvent {
                            session_id: sid.clone(),
                            message_id: msg_id_clone.clone(),
                            from_harness: from_h,
                            from_account: from_acc_label,
                            to_harness: cur_harness.clone(),
                            to_account: to_acc_label,
                            to_model: cur_model.clone(),
                            reason,
                            hop: failover_hops,
                            notice,
                        },
                    );
                }
            }
        }

        let mut cancel_rx_opt = Some(cancel_rx);
        let mut skip_acp = false;
        let run_result = loop {
            let (resolved_model, resolved_effort) = if cur_harness == "agy-cli" {
                let (m, e) =
                    resolve_agy_model_and_effort_with_db(&db_arc, &cur_model, &session.effort)
                        .await;
                (m.unwrap_or_default(), e.unwrap_or_default())
            } else {
                (cur_model.clone(), session.effort.clone())
            };
            let attempt_cancel_rx = match cancel_rx_opt.take() {
                Some(rx) => rx,
                None => {
                    let (retry_tx, retry_rx) = oneshot::channel::<()>();
                    let mut active = active_arc.lock().await;
                    active.insert(sid.clone(), retry_tx);
                    retry_rx
                }
            };

            let attempt_res = if let Some(ref distro) = project_wsl_distro {
                if cur_harness == "sakana" {
                    run_http_sse_fallback(
                        &effective_cwd,
                        &cur_model,
                        &sakana_ctx,
                        &recent_history,
                        &effective_prompt,
                        app_handle.as_ref(),
                        &sid,
                        &msg_id_clone,
                        started,
                        &mut acc,
                        attempt_cancel_rx,
                    )
                    .await
                } else if cur_harness == "agy-cli" && cross_cmd.is_none() && !skip_acp {
                    crate::acp_pool::run_persistent_agy_turn(
                        None,
                        Some(distro.as_str()),
                        cur_account.as_ref(),
                        &resolved_model,
                        &resolved_effort,
                        &session.permission_mode,
                        &enabled_mcp_servers,
                        &effective_cwd,
                        &full_prompt_with_ctx,
                        &effective_prompt,
                        app_handle.as_ref(),
                        &sid,
                        &msg_id_clone,
                        started,
                        &mut acc,
                        attempt_cancel_rx,
                    )
                    .await
                } else {
                    run_wsl_cli_harness(
                        distro,
                        &cur_harness,
                        cur_account.as_ref(),
                        &resolved_model,
                        &resolved_effort,
                        &session.permission_mode,
                        &enabled_mcp_servers,
                        &effective_cwd,
                        &full_prompt_with_ctx,
                        app_handle.as_ref(),
                        &sid,
                        &msg_id_clone,
                        started,
                        &mut acc,
                        attempt_cancel_rx,
                    )
                    .await
                }
            } else if let Some(bin) = resolve_harness_binary(&cur_harness) {
                if cur_harness == "agy-cli" && cross_cmd.is_none() && !skip_acp {
                    crate::acp_pool::run_persistent_agy_turn(
                        Some(&bin),
                        None,
                        cur_account.as_ref(),
                        &resolved_model,
                        &resolved_effort,
                        &session.permission_mode,
                        &enabled_mcp_servers,
                        &effective_cwd,
                        &full_prompt_with_ctx,
                        &effective_prompt,
                        app_handle.as_ref(),
                        &sid,
                        &msg_id_clone,
                        started,
                        &mut acc,
                        attempt_cancel_rx,
                    )
                    .await
                } else {
                    run_cli_harness(
                        &bin,
                        &cur_harness,
                        cur_account.as_ref(),
                        &resolved_model,
                        &resolved_effort,
                        &session.permission_mode,
                        &enabled_mcp_servers,
                        &effective_cwd,
                        &full_prompt_with_ctx,
                        app_handle.as_ref(),
                        &sid,
                        &msg_id_clone,
                        started,
                        &mut acc,
                        attempt_cancel_rx,
                    )
                    .await
                }
            } else if cur_harness == "sakana" {
                run_http_sse_fallback(
                    &effective_cwd,
                    &cur_model,
                    &sakana_ctx,
                    &recent_history,
                    &effective_prompt,
                    app_handle.as_ref(),
                    &sid,
                    &msg_id_clone,
                    started,
                    &mut acc,
                    attempt_cancel_rx,
                )
                .await
            } else {
                match run_http_sse_fallback(
                    &effective_cwd,
                    &cur_model,
                    &sakana_ctx,
                    &recent_history,
                    &effective_prompt,
                    app_handle.as_ref(),
                    &sid,
                    &msg_id_clone,
                    started,
                    &mut acc,
                    attempt_cancel_rx,
                )
                .await
                {
                    Ok(()) => Ok(()),
                    Err(_) => Err(format!(
                        "Harness '{}' CLI binary not found in PATH and no LLM API key (OPENROUTER_API_KEY / OPENAI_API_KEY / OLLAMA_BASE_URL) is configured.",
                        cur_harness
                    )),
                }
            };

            match attempt_res {
                Ok(()) => break Ok(()),
                Err(err_msg)
                    if cur_harness == "agy-cli"
                        && cross_cmd.is_none()
                        && !skip_acp
                        && acc.text.is_empty()
                        && !err_msg.to_lowercase().contains("cancelled") =>
                {
                    let _ = crate::acp_pool::sleep_session_inner(app_handle.as_ref(), &sid).await;
                    skip_acp = true;
                    continue;
                }
                Err(err_msg) if failover_cfg.enabled && failover_hops < 3 && is_rate_limit_or_quota_error(&err_msg) => {
                    let from_h = cur_harness.clone();
                    let from_acc_label = cur_account
                        .as_ref()
                        .map(|a| a.label.clone())
                        .unwrap_or_else(|| "default".to_string());
                    let mut switched = false;
                    if failover_cfg.rotate_accounts_same_harness {
                        let next_acc = {
                            let db = db_arc.lock().await;
                            db.list_harness_accounts(Some(&cur_harness))
                                .unwrap_or_default()
                                .into_iter()
                                .find(|a| a.status == "authenticated" && !tried_accounts.contains(&a.id))
                        };
                        if let Some(na) = next_acc {
                            tried_accounts.push(na.id.clone());
                            cur_account = Some(na);
                            switched = true;
                        }
                    }
                    if !switched && fallback_idx < failover_cfg.fallback_chain.len() {
                        let target = &failover_cfg.fallback_chain[fallback_idx];
                        fallback_idx += 1;
                        cur_harness = target.harness.clone();
                        if !target.model.trim().is_empty() {
                            cur_model = target.model.clone();
                        }
                        cur_account = {
                            let db = db_arc.lock().await;
                            target
                                .account
                                .as_deref()
                                .and_then(|aid| db.get_harness_account(aid).ok().flatten())
                                .or_else(|| db.get_default_harness_account(&cur_harness).ok().flatten())
                        };
                        switched = true;
                    }
                    if !switched {
                        break Err(err_msg);
                    }
                    failover_hops += 1;
                    acc.text.clear();
                    acc.thinking.clear();
                    acc.stream_error = None;
                    let to_acc_label = cur_account
                        .as_ref()
                        .map(|a| a.label.clone())
                        .unwrap_or_else(|| "default".to_string());
                    let notice = if from_h == cur_harness {
                        format!("Switched account: {} -> {} (rate limit)", from_acc_label, to_acc_label)
                    } else {
                        format!("Failover: {}/{} -> {}/{} (rate limit)", from_h, from_acc_label, cur_harness, to_acc_label)
                    };
                    acc.failover_notice = Some(notice.clone());
                    crate::platform::emit_app_and_remote(
                        app_handle.as_ref(),
                        "turn:failover",
                        crate::models::TurnFailoverEvent {
                            session_id: sid.clone(),
                            message_id: msg_id_clone.clone(),
                            from_harness: from_h,
                            from_account: from_acc_label,
                            to_harness: cur_harness.clone(),
                            to_account: to_acc_label,
                            to_model: cur_model.clone(),
                            reason: "rate limit".to_string(),
                            hop: failover_hops,
                            notice,
                        },
                    );
                }
                Err(err_msg) => break Err(err_msg),
            }
        };
        let active_harness = cur_harness;

        let elapsed_ms = started.elapsed().as_millis() as i64;
        if let Some(ref mut sub) = acc.subagent {
            sub.duration_ms = elapsed_ms;
            if !acc.text.is_empty() {
                sub.summary = acc.text.lines().next().unwrap_or("Completed").chars().take(120).collect();
            }
        }

        let final_status;
        match run_result {
            Ok(()) => {
                final_status = "DONE".to_string();
                let mut input_tokens = acc.input_tokens;
                let mut output_tokens = acc.output_tokens;
                let cost_usd = acc.cost_usd;
                let author = author_override.unwrap_or_else(|| active_harness.clone());
                let msg = acc.into_message(msg_id_clone.clone(), author);
                let _ = crate::self_control::extract_and_execute_actions(
                    &db_arc,
                    app_handle.as_ref(),
                    &msg.text,
                )
                .await;
                if let Some(ref to_sid) = peer_session_id {
                    let peer_msgs = crate::self_control::extract_peer_messages(&msg.text);
                    for pm in peer_msgs {
                        crate::platform::emit_app_and_remote(
                            app_handle.as_ref(),
                            "sakana:peer-message",
                            crate::self_control::PeerMessageEvent {
                                from_session_id: sid.clone(),
                                to_session_id: to_sid.clone(),
                                from_harness: active_harness.clone(),
                                from_title: session.title.clone(),
                                message: pm.message,
                                expect_reply: pm.expect_reply,
                            },
                        );
                    }
                }
                if input_tokens == 0 && output_tokens == 0 {
                    input_tokens = (full_prompt_with_ctx.len() / 4) as i64;
                    output_tokens = (msg.text.len() / 4) as i64;
                }
                let span = TraceSpan {
                    id: format!("span-{}", Uuid::new_v4()),
                    actor: session.harness.clone(),
                    harness: active_harness.clone(),
                    action: cross_cmd
                        .as_ref()
                        .map(|(_, c, _)| format!("{} turn", c))
                        .unwrap_or_else(|| "assistant turn".to_string()),
                    duration_ms: elapsed_ms,
                    tokens: format!("~{} chars", msg.text.len()),
                    status: "ok".to_string(),
                    children: cross_cmd.as_ref().map(|(target_h, c, _)| {
                        vec![TraceSpan {
                            id: format!("span-{}", Uuid::new_v4()),
                            actor: target_h.clone(),
                            harness: target_h.clone(),
                            action: c.clone(),
                            duration_ms: elapsed_ms,
                            tokens: format!("~{} chars", msg.text.len()),
                            status: "ok".to_string(),
                            children: None,
                        }]
                    }),
                };

                {
                    let db = db_arc.lock().await;
                    let _ = db.append_message(&sid, &msg);
                    let _ = db.insert_trace_span(&sid, None, &span);
                    let _ = db.record_usage(
                        Some(&sid),
                        &active_harness,
                        &session.model,
                        input_tokens,
                        output_tokens,
                        cost_usd,
                    );
                    let _ = db.update_session_status(&sid, &final_status, Some(true));
                }

                crate::platform::emit_app_and_remote(
                    app_handle.as_ref(),
                    "turn:done",
                    TurnDoneEvent {
                        session_id: sid.clone(),
                        message_id: msg_id_clone.clone(),
                        message: msg.clone(),
                        status: final_status,
                    },
                );
                if let Some(tx) = completion_tx.take() {
                    let _ = tx.send(Ok(msg));
                }
            }
            Err(err_msg) => {
                final_status = "IDLE".to_string();
                {
                    let db = db_arc.lock().await;
                    let _ = db.update_session_status(&sid, &final_status, Some(false));
                }
                crate::platform::emit_app_and_remote(
                    app_handle.as_ref(),
                    "turn:error",
                    TurnErrorEvent {
                        session_id: sid.clone(),
                        message_id: msg_id_clone.clone(),
                        error: err_msg.clone(),
                    },
                );
                if let Some(tx) = completion_tx.take() {
                    let _ = tx.send(Err(err_msg));
                }
            }
        }

        let mut active = active_arc.lock().await;
        active.remove(&sid);
    });

    Ok(message_id)
}

#[tauri::command]
pub async fn send_turn(
    app: AppHandle,
    db_state: State<'_, DbState>,
    harness_state: State<'_, HarnessState>,
    session_id: String,
    prompt: String,
    attachments: Option<Vec<Value>>,
    peer_session_id: Option<String>,
) -> Result<String, String> {
    send_turn_with_options(
        Some(app),
        db_state.0.clone(),
        harness_state.active_turns.clone(),
        session_id,
        prompt,
        attachments,
        TurnRunOptions {
            peer_session_id,
            ..Default::default()
        },
    )
    .await
}

#[tauri::command]
pub async fn compact_session(
    app: AppHandle,
    db_state: State<'_, DbState>,
    session_id: String,
) -> Result<CompactionResult, String> {
    compact_session_inner(Some(&app), &db_state.0, &session_id, false).await
}

#[tauri::command]
pub async fn get_compaction_config(
    db_state: State<'_, DbState>,
) -> Result<CompactionConfig, String> {
    let db = db_state.0.lock().await;
    Ok(load_compaction_config(&db))
}

#[tauri::command]
pub async fn update_compaction_config(
    db_state: State<'_, DbState>,
    config: CompactionConfig,
) -> Result<CompactionConfig, String> {
    let normalized = CompactionConfig {
        auto_compact: config.auto_compact,
        message_threshold: config.message_threshold.clamp(12, 200),
        char_threshold: config.char_threshold.clamp(8_000, 120_000),
        keep_recent: config.keep_recent.clamp(2, 30),
    };
    let db = db_state.0.lock().await;
    save_compaction_config(&db, &normalized)?;
    Ok(normalized)
}

#[tauri::command]
pub async fn cancel_turn(
    db_state: State<'_, DbState>,
    harness_state: State<'_, HarnessState>,
    session_id: String,
) -> Result<(), String> {
    let tx = {
        let mut active = harness_state.active_turns.lock().await;
        active.remove(&session_id)
    };
    if let Some(sender) = tx {
        let _ = sender.send(());
    }
    let db = db_state.0.lock().await;
    db.update_session_status(&session_id, "IDLE", Some(false))
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn infer_model_family(id: &str) -> String {
    let lower = id.to_lowercase();
    if lower == "auto" {
        "auto".to_string()
    } else if lower.starts_with("gemini-") {
        "gemini".to_string()
    } else if lower.starts_with("claude-") {
        "claude".to_string()
    } else if lower.starts_with("gpt-") {
        "openai".to_string()
    } else if lower.starts_with("deepseek-") {
        "deepseek".to_string()
    } else if lower.starts_with("qwen") {
        "qwen".to_string()
    } else {
        "other".to_string()
    }
}

fn strip_effort_suffix(id: &str, label: &str) -> (String, String, Option<String>) {
    let effort_suffixes = [
        ("-xhigh", " (XHigh)", "xhigh"),
        ("-medium", " (Medium)", "medium"),
        ("-high", " (High)", "high"),
        ("-low", " (Low)", "low"),
        ("-max", " (Max)", "max"),
    ];

    for (id_sfx, label_sfx, effort) in effort_suffixes {
        if let Some(base_id) = id.strip_suffix(id_sfx) {
            let base_label = label
                .strip_suffix(label_sfx)
                .or_else(|| {
                    let lower_l = label.to_lowercase();
                    let lower_s = label_sfx.to_lowercase();
                    if lower_l.ends_with(&lower_s) {
                        Some(&label[..label.len() - label_sfx.len()])
                    } else {
                        None
                    }
                })
                .unwrap_or(label)
                .trim()
                .to_string();
            return (
                base_id.to_string(),
                base_label,
                Some(effort.to_string()),
            );
        }
    }

    (id.to_string(), label.trim().to_string(), None)
}

fn parse_agy_models_output(raw: &str) -> Vec<HarnessModelOption> {
    let mut models: Vec<HarnessModelOption> = Vec::new();
    let mut index_by_id: HashMap<String, usize> = HashMap::new();
    let effort_order = ["low", "medium", "high", "xhigh", "max"];

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("Fetching ")
            || trimmed.starts_with('#')
            || trimmed.starts_with('-')
        {
            continue;
        }

        let (raw_id, raw_label) = if let Some((left, right)) = trimmed.split_once('\t') {
            (left.trim(), right.trim())
        } else {
            let mut parts = trimmed.split_whitespace();
            let Some(first) = parts.next() else {
                continue;
            };
            let rest = parts.collect::<Vec<_>>().join(" ");
            if rest.is_empty() {
                (first, first)
            } else {
                // Use leaked/owned via strip_effort_suffix below
                let (base_id, base_label, effort_opt) = strip_effort_suffix(first, &rest);
                if let Some(&idx) = index_by_id.get(&base_id) {
                    if let Some(eff) = effort_opt {
                        if !models[idx].efforts.contains(&eff) {
                            models[idx].efforts.push(eff);
                            models[idx].efforts.sort_by_key(|e| {
                                effort_order.iter().position(|o| o == e).unwrap_or(99)
                            });
                        }
                    }
                } else {
                    let efforts = effort_opt.into_iter().collect();
                    let family = infer_model_family(&base_id);
                    let label = if base_label.is_empty() {
                        base_id.clone()
                    } else {
                        base_label
                    };
                    index_by_id.insert(base_id.clone(), models.len());
                    models.push(HarnessModelOption {
                        id: base_id,
                        label,
                        family,
                        efforts,
                    });
                }
                continue;
            }
        };

        let (base_id, base_label, effort_opt) = strip_effort_suffix(raw_id, raw_label);
        if base_id.is_empty() {
            continue;
        }
        if let Some(&idx) = index_by_id.get(&base_id) {
            if let Some(eff) = effort_opt {
                if !models[idx].efforts.contains(&eff) {
                    models[idx].efforts.push(eff);
                    models[idx].efforts.sort_by_key(|e| {
                        effort_order.iter().position(|o| o == e).unwrap_or(99)
                    });
                }
            }
        } else {
            let efforts = effort_opt.into_iter().collect();
            let family = infer_model_family(&base_id);
            let label = if base_label.is_empty() {
                base_id.clone()
            } else {
                base_label
            };
            index_by_id.insert(base_id.clone(), models.len());
            models.push(HarnessModelOption {
                id: base_id,
                label,
                family,
                efforts,
            });
        }
    }

    models
}

async fn discover_agy_models_via_acp(
    bin: &Path,
    acc: Option<&HarnessAccountRow>,
) -> Option<Vec<HarnessModelOption>> {
    let gdir = acc
        .map(crate::auth::agy_gemini_dir)
        .unwrap_or_else(|| crate::auth::ensure_profile_dir("agy-cli", "default").join(".gemini"));
    let auth_method = acc
        .map(|a| a.auth_type.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("oauth-personal");

    let mut cmd = crate::platform::tokio_command(&bin.to_string_lossy());
    crate::auth::apply_account_env(&mut cmd, "agy-cli", acc);
    cmd.args([
        "acp",
        "--gemini-dir",
        &gdir.to_string_lossy(),
        "--app-data-dir",
        "antigravity-cli",
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());

    let mut child = cmd.spawn().ok()?;
    let mut stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let mut reader = BufReader::new(stdout).lines();

    let res = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let init_req = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":1,\"clientCapabilities\":{},\"clientInfo\":{\"name\":\"sakana-probe\",\"version\":\"2.0.0\"}}}\n";
        stdin.write_all(init_req.as_bytes()).await.ok()?;
        stdin.flush().await.ok()?;
        let _init_line = reader.next_line().await.ok()??;

        let auth_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "authenticate",
            "params": { "methodId": auth_method }
        })
        .to_string()
            + "\n";
        stdin.write_all(auth_req.as_bytes()).await.ok()?;
        stdin.flush().await.ok()?;
        let auth_line = reader.next_line().await.ok()??;
        let auth_val: Value = serde_json::from_str(&auth_line).ok()?;
        if auth_val.get("error").is_some() {
            return None;
        }

        let session_req = "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"session/new\",\"params\":{\"cwd\":\".\",\"mcpServers\":[]}}\n";
        stdin.write_all(session_req.as_bytes()).await.ok()?;
        stdin.flush().await.ok()?;
        let session_line = reader.next_line().await.ok()??;
        let session_val: Value = serde_json::from_str(&session_line).ok()?;
        let arr = session_val
            .get("result")
            .and_then(|r| r.get("models"))
            .and_then(|m| m.get("availableModels"))
            .and_then(|a| a.as_array())?;

        let mut lines = String::new();
        for item in arr {
            let id = item.get("modelId").and_then(|v| v.as_str()).unwrap_or("").trim();
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or(id).trim();
            if !id.is_empty() {
                lines.push_str(id);
                lines.push('\t');
                lines.push_str(name);
                lines.push('\n');
            }
        }
        let parsed = parse_agy_models_output(&lines);
        if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        }
    })
    .await
    .ok()
    .flatten();

    let _ = child.kill().await;
    res
}

async fn discover_agy_models_via_file_token(
    acc: Option<&HarnessAccountRow>,
) -> Option<Vec<HarnessModelOption>> {
    let default_acc = HarnessAccountRow {
        id: "default".to_string(),
        harness: "agy-cli".to_string(),
        label: "Primary AGY".to_string(),
        email: String::new(),
        profile_dir: String::new(),
        auth_type: "oauth-personal".to_string(),
        is_default: true,
        status: "authenticated".to_string(),
    };
    let effective_acc = acc.unwrap_or(&default_acc);
    let (access_token, _) = crate::auth::read_agy_file_token(effective_acc)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .ok()?;

    let lca_resp = client
        .post("https://daily-cloudcode-pa.googleapis.com/v1internal:loadCodeAssist")
        .header("Authorization", format!("Bearer {access_token}"))
        .header("User-Agent", "antigravity/1.5.18 windows/amd64")
        .json(&serde_json::json!({ "metadata": { "ideType": "ANTIGRAVITY" } }))
        .send()
        .await
        .ok()?;
    if !lca_resp.status().is_success() {
        return None;
    }
    let lca_val: Value = lca_resp.json().await.ok()?;
    let project = lca_val
        .get("cloudaicompanionProject")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let body = match project {
        Some(p) => serde_json::json!({ "project": p }),
        None => serde_json::json!({}),
    };
    let models_resp = client
        .post("https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels")
        .header("Authorization", format!("Bearer {access_token}"))
        .header("User-Agent", "antigravity/1.5.18 windows/amd64")
        .json(&body)
        .send()
        .await
        .ok()?;
    if !models_resp.status().is_success() {
        return None;
    }
    let val: Value = models_resp.json().await.ok()?;
    let models_obj = val.get("models").and_then(|v| v.as_object())?;

    let mut image_gen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(arr) = val.get("imageGenerationModelIds").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(s) = item.as_str() {
                image_gen_ids.insert(s.to_string());
            }
        }
    }

    let mut ordered_ids: Vec<String> = Vec::new();
    let mut push_unique = |id: &str| {
        if !id.is_empty() && !ordered_ids.iter().any(|x| x == id) {
            ordered_ids.push(id.to_string());
        }
    };

    if let Some(sorts) = val.get("agentModelSorts").and_then(|v| v.as_array()) {
        for sort in sorts {
            if let Some(groups) = sort.get("groups").and_then(|v| v.as_array()) {
                for g in groups {
                    if let Some(ids) = g.get("modelIds").and_then(|v| v.as_array()) {
                        for id_val in ids {
                            if let Some(s) = id_val.as_str() {
                                push_unique(s);
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(tiered) = val.get("tieredModelIds") {
        if let Some(arr) = tiered.as_array() {
            for id_val in arr {
                if let Some(s) = id_val.as_str() {
                    push_unique(s);
                }
            }
        } else if let Some(obj) = tiered.as_object() {
            for arr_val in obj.values() {
                if let Some(arr) = arr_val.as_array() {
                    for id_val in arr {
                        if let Some(s) = id_val.as_str() {
                            push_unique(s);
                        }
                    }
                } else if let Some(s) = arr_val.as_str() {
                    push_unique(s);
                }
            }
        }
    }
    if models_obj.contains_key("gemini-4-hydrogen-sonar") {
        push_unique("gemini-4-hydrogen-sonar");
    }

    let mut out: Vec<HarnessModelOption> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for raw_id in ordered_ids {
        if raw_id.starts_with("tab_") || image_gen_ids.contains(&raw_id) {
            continue;
        }
        let Some(meta) = models_obj.get(&raw_id) else {
            continue;
        };
        let raw_display = meta
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        let (norm_id, label, efforts) = match raw_id.as_str() {
            "gemini-4-hydrogen-sonar" => (
                "gemini-4-hydrogen".to_string(),
                "Gemini 4 Hydrogen".to_string(),
                vec!["low".into(), "medium".into(), "high".into(), "xhigh".into()],
            ),
            "gemini-3.8-flash-tiered" => (
                "gemini-3.8-flash".to_string(),
                "Gemini 3.8 Flash".to_string(),
                vec!["low".into(), "medium".into(), "high".into()],
            ),
            _ if raw_id.starts_with("gemini-2.5-") => continue,
            _ if raw_display == "Gemini 3.5 Flash Lite" => continue,
            _ => {
                if raw_display.is_empty() {
                    continue;
                }
                if let Some(parsed_one) = parse_agy_models_output(&format!("{raw_id}\t{raw_display}")).into_iter().next() {
                    (parsed_one.id, parsed_one.label, parsed_one.efforts)
                } else {
                    (raw_id.clone(), raw_display.to_string(), vec![])
                }
            }
        };

        if seen_ids.insert(norm_id.clone()) {
            let family = infer_model_family(&norm_id);
            out.push(HarnessModelOption {
                id: norm_id,
                label,
                family,
                efforts,
            });
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

async fn discover_agy_models(acc: Option<&HarnessAccountRow>) -> Option<Vec<HarnessModelOption>> {
    if let Some(bin) = resolve_harness_binary("agy-cli") {
        if let Some(via_acp) = discover_agy_models_via_acp(&bin, acc).await {
            return Some(via_acp);
        }
        let mut cmd = crate::platform::tokio_command(&bin.to_string_lossy());
        crate::auth::apply_account_env(&mut cmd, "agy-cli", acc);
        cmd.arg("models")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Ok(Ok(output)) =
            tokio::time::timeout(std::time::Duration::from_secs(5), cmd.output()).await
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let parsed = parse_agy_models_output(&stdout);
                if !parsed.is_empty() {
                    return Some(parsed);
                }
            }
        }
    }

    discover_agy_models_via_file_token(acc).await
}

pub fn resolve_agy_model_and_effort(
    model: &str,
    effort: &str,
    catalog: &[HarnessModelOption],
) -> (Option<String>, Option<String>) {
    let trimmed_model = model.trim();
    let (base_id, _, suffix_effort) = strip_effort_suffix(trimmed_model, "");
    let matched = if !trimmed_model.is_empty() {
        catalog
            .iter()
            .find(|m| m.id == trimmed_model)
            .or_else(|| catalog.iter().find(|m| m.id == base_id))
            .or_else(|| catalog.first())
    } else {
        catalog.first()
    };

    let Some(entry) = matched else {
        return (None, None);
    };

    let resolved_effort = if entry.efforts.is_empty() {
        None
    } else {
        let req = effort.trim();
        let candidate = if !req.is_empty() {
            req
        } else if let Some(ref sfx) = suffix_effort {
            sfx.as_str()
        } else {
            ""
        };
        if let Some(found) = entry.efforts.iter().find(|e| e.as_str() == candidate) {
            Some(found.clone())
        } else if let Some(med) = entry.efforts.iter().find(|e| e.as_str() == "medium") {
            Some(med.clone())
        } else {
            entry.efforts.first().cloned()
        }
    };

    (Some(entry.id.clone()), resolved_effort)
}

pub(crate) async fn resolve_agy_model_and_effort_with_db(
    db: &Arc<Mutex<crate::db::Database>>,
    model: &str,
    effort: &str,
) -> (Option<String>, Option<String>) {
    let catalog = list_harness_models_with_db(db, false)
        .await
        .ok()
        .and_then(|mut m| m.remove("agy-cli"))
        .unwrap_or_else(fallback_agy_models);
    resolve_agy_model_and_effort(model, effort, &catalog)
}

fn opt(id: &str, label: &str, family: &str, efforts: &[&str]) -> HarnessModelOption {
    HarnessModelOption {
        id: id.to_string(),
        label: label.to_string(),
        family: family.to_string(),
        efforts: efforts.iter().map(|s| s.to_string()).collect(),
    }
}

fn fallback_agy_models() -> Vec<HarnessModelOption> {
    vec![
        opt("gemini-4-hydrogen", "Gemini 4 Hydrogen", "gemini", &["low", "medium", "high", "xhigh"]),
        opt("gemini-3.8-flash", "Gemini 3.8 Flash", "gemini", &["low", "medium", "high"]),
        opt("claude-sonnet-4-6", "Claude Sonnet 4.6", "claude", &[]),
        opt("claude-opus-4-6-thinking", "Claude Opus 4.6 (Thinking)", "claude", &[]),
        opt("gpt-oss-120b", "GPT-OSS 120B", "openai", &["medium"]),
    ]
}

fn fallback_claude_models() -> Vec<HarnessModelOption> {
    vec![
        opt("claude-sonnet-4-6", "Claude Sonnet 4.6", "claude", &["low", "medium", "high"]),
        opt("claude-opus-4-6", "Claude Opus 4.6", "claude", &["low", "medium", "high", "max"]),
        opt("claude-haiku-4-5", "Claude Haiku 4.5", "claude", &["low", "medium", "high"]),
    ]
}

fn fallback_kiro_models() -> Vec<HarnessModelOption> {
    vec![
        opt("auto", "Auto", "auto", &[]),
        opt("claude-sonnet-5.5", "Claude Sonnet 5.5", "claude", &["low", "medium", "high"]),
        opt("claude-opus-5.5", "Claude Opus 5.5", "claude", &["low", "medium", "high"]),
        opt("claude-haiku-5.5", "Claude Haiku 5.5", "claude", &["low", "medium", "high"]),
        opt("gpt-5.6-sol", "GPT-5.6 Sol", "openai", &["low", "medium", "high"]),
        opt("gpt-5.6-terra", "GPT-5.6 Terra", "openai", &["low", "medium", "high"]),
        opt("deepseek-3.2", "DeepSeek 3.2", "deepseek", &["low", "medium", "high"]),
        opt("qwen3-coder-next", "Qwen3 Coder Next", "qwen", &["low", "medium", "high"]),
    ]
}

fn fallback_sakana_models() -> Vec<HarnessModelOption> {
    vec![
        opt("gemini-4-carbon", "Gemini 4 Carbon", "gemini", &["low", "medium", "high"]),
        opt("gemini-4-hydrogen", "Gemini 4 Hydrogen", "gemini", &["low", "medium", "high", "xhigh"]),
        opt("gemini-4-argon", "Gemini 4 Argon", "gemini", &["low", "medium", "high"]),
        opt("gemini-4-neon", "Gemini 4 Neon", "gemini", &["low", "medium", "high", "xhigh"]),
        opt("gemini-3.8-flash", "Gemini 3.8 Flash", "gemini", &["low", "medium", "high"]),
        opt("gemini-3.5-flash", "Gemini 3.5 Flash", "gemini", &["low", "medium", "high"]),
        opt("gemini-3.1-pro", "Gemini 3.1 Pro", "gemini", &["low", "medium", "high"]),
        opt("gemini-2.5-pro", "Gemini 2.5 Pro", "gemini", &["low", "medium", "high"]),
        opt("gemini-flash", "Gemini Flash", "gemini", &["low", "medium", "high"]),
        opt("auto", "Auto", "auto", &[]),
    ]
}

pub const HARNESS_MODELS_CACHE_KEY: &str = "harness_models_cache_v5";
const HARNESS_MODELS_CACHE_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct HarnessModelsCache {
    fetched_at: u64,
    #[serde(default)]
    account_fingerprint: String,
    models: HashMap<String, Vec<HarnessModelOption>>,
}

fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn compute_account_fingerprint(
    agy: Option<&HarnessAccountRow>,
    claude: Option<&HarnessAccountRow>,
    kiro: Option<&HarnessAccountRow>,
) -> String {
    let fmt = |a: Option<&HarnessAccountRow>| -> String {
        a.map(|r| format!("{}:{}:{}", r.id, r.auth_type, r.status))
            .unwrap_or_else(|| "none".to_string())
    };
    format!("agy={}|claude={}|kiro={}", fmt(agy), fmt(claude), fmt(kiro))
}

async fn discover_claude_models(acc: Option<&HarnessAccountRow>) -> Option<Vec<HarnessModelOption>> {
    // 1. If an Anthropic API key is configured, query /v1/models for exact key entitlements
    let use_api_key = acc.map(|a| a.auth_type == "anthropic-api-key").unwrap_or(false)
        || crate::platform::get_env_or_dotenv("ANTHROPIC_API_KEY").is_some();
    if use_api_key {
        if let Some(api_key) = crate::platform::get_env_or_dotenv("ANTHROPIC_API_KEY") {
            if let Ok(client) = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
            {
                if let Ok(resp) = client
                    .get("https://api.anthropic.com/v1/models")
                    .header("x-api-key", api_key.trim())
                    .header("anthropic-version", "2023-06-01")
                    .send()
                    .await
                {
                    if resp.status().is_success() {
                        if let Ok(val) = resp.json::<Value>().await {
                            if let Some(items) = val.get("data").and_then(|v| v.as_array()) {
                                let mut models = Vec::new();
                                for item in items {
                                    let id = item
                                        .get("id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .trim()
                                        .to_string();
                                    if id.is_empty() {
                                        continue;
                                    }
                                    let label = item
                                        .get("display_name")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .unwrap_or_else(|| id.clone());
                                    let efforts = if id.contains("opus") {
                                        vec!["low".into(), "medium".into(), "high".into(), "max".into()]
                                    } else {
                                        vec!["low".into(), "medium".into(), "high".into()]
                                    };
                                    models.push(HarnessModelOption {
                                        id,
                                        label,
                                        family: "claude".to_string(),
                                        efforts,
                                    });
                                }
                                if !models.is_empty() {
                                    return Some(models);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Otherwise inspect `claude auth status` + `.claude.json` subscription tier
    let mut tier_hint = String::new();
    if let Some(bin) = resolve_harness_binary("claude-code") {
        let mut cmd = crate::platform::tokio_command(&bin.to_string_lossy());
        crate::auth::apply_account_env(&mut cmd, "claude-code", acc);
        cmd.args(["auth", "status"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Ok(Ok(out)) = tokio::time::timeout(std::time::Duration::from_secs(5), cmd.output()).await {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if let Ok(val) = serde_json::from_str::<Value>(&stdout) {
                    if let Some(sub) = val
                        .get("subscriptionType")
                        .or_else(|| val.get("plan"))
                        .or_else(|| val.get("tier"))
                        .and_then(|v| v.as_str())
                    {
                        tier_hint = sub.to_ascii_lowercase();
                    }
                }
            }
        }
    }

    if tier_hint.is_empty() {
        let claude_json_path = match acc {
            Some(a) if a.id != "claude_default" && !a.profile_dir.trim().is_empty() => {
                PathBuf::from(&a.profile_dir).join(".claude.json")
            }
            _ => dirs::home_dir().unwrap_or_default().join(".claude.json"),
        };
        if let Ok(content) = std::fs::read_to_string(claude_json_path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                if let Some(sub) = val
                    .get("oauthAccount")
                    .and_then(|o| {
                        o.get("subscriptionType")
                            .or_else(|| o.get("rateLimitTier"))
                            .or_else(|| o.get("billingType"))
                    })
                    .and_then(|v| v.as_str())
                {
                    tier_hint = sub.to_ascii_lowercase();
                }
            }
        }
    }

    if tier_hint.contains("free") {
        return Some(vec![
            opt("claude-sonnet-4-6", "Claude Sonnet 4.6", "claude", &["low", "medium", "high"]),
            opt("claude-haiku-4-5", "Claude Haiku 4.5", "claude", &["low", "medium", "high"]),
        ]);
    }

    None
}

fn parse_kiro_models_output(raw: &str) -> Vec<HarnessModelOption> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if trimmed.starts_with('[') {
        if let Ok(parsed) = serde_json::from_str::<Vec<HarnessModelOption>>(trimmed) {
            return parsed;
        }
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(trimmed) {
            return ids
                .into_iter()
                .filter(|id| !id.trim().is_empty())
                .map(|id| {
                    let family = infer_model_family(&id);
                    HarnessModelOption {
                        label: id.clone(),
                        id,
                        family,
                        efforts: vec![
                            "low".to_string(),
                            "medium".to_string(),
                            "high".to_string(),
                        ],
                    }
                })
                .collect();
        }
    }
    parse_agy_models_output(raw)
}

async fn discover_kiro_models(acc: Option<&HarnessAccountRow>) -> Option<Vec<HarnessModelOption>> {
    let bin = resolve_harness_binary("kiro-cli")?;
    let mut cmd = crate::platform::tokio_command(&bin.to_string_lossy());
    crate::auth::apply_account_env(&mut cmd, "kiro-cli", acc);
    cmd.arg("models")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = tokio::time::timeout(std::time::Duration::from_secs(5), cmd.output())
        .await
        .ok()?
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed = parse_kiro_models_output(&stdout);
    if parsed.is_empty() {
        None
    } else {
        Some(parsed)
    }
}

fn pick_discovered_or_fallback(
    discovered: Option<Vec<HarnessModelOption>>,
    fallback: Vec<HarnessModelOption>,
) -> Vec<HarnessModelOption> {
    match discovered {
        Some(list) if !list.is_empty() => list,
        _ => fallback,
    }
}

fn resolve_catalog_with_fallbacks(
    mut cached: HashMap<String, Vec<HarnessModelOption>>,
) -> HashMap<String, Vec<HarnessModelOption>> {
    let mut out = HashMap::new();
    out.insert(
        "agy-cli".to_string(),
        pick_discovered_or_fallback(cached.remove("agy-cli"), fallback_agy_models()),
    );
    out.insert(
        "claude-code".to_string(),
        pick_discovered_or_fallback(cached.remove("claude-code"), fallback_claude_models()),
    );
    out.insert(
        "kiro-cli".to_string(),
        pick_discovered_or_fallback(cached.remove("kiro-cli"), fallback_kiro_models()),
    );
    out.insert(
        "sakana".to_string(),
        pick_discovered_or_fallback(cached.remove("sakana"), fallback_sakana_models()),
    );
    out
}

pub async fn list_harness_models_with_db(
    db: &Arc<Mutex<crate::db::Database>>,
    refresh: bool,
) -> Result<HashMap<String, Vec<HarnessModelOption>>, String> {
    let (agy_acc, claude_acc, kiro_acc, cached_raw) = {
        let db_lock = db.lock().await;
        let agy = db_lock.get_default_harness_account("agy-cli").ok().flatten();
        let claude = db_lock.get_default_harness_account("claude-code").ok().flatten();
        let kiro = db_lock.get_default_harness_account("kiro-cli").ok().flatten();
        let raw = if !refresh {
            db_lock
                .get_setting(HARNESS_MODELS_CACHE_KEY)
                .map_err(|e| e.to_string())?
        } else {
            None
        };
        (agy, claude, kiro, raw)
    };

    let fingerprint = compute_account_fingerprint(agy_acc.as_ref(), claude_acc.as_ref(), kiro_acc.as_ref());
    let now = now_epoch_secs();

    if !refresh {
        if let Some(json_str) = cached_raw {
            if let Ok(cache) = serde_json::from_str::<HarnessModelsCache>(&json_str) {
                let not_expired = now.saturating_sub(cache.fetched_at) < HARNESS_MODELS_CACHE_TTL_SECS;
                let same_accounts = cache.account_fingerprint == fingerprint;
                if not_expired && same_accounts {
                    return Ok(resolve_catalog_with_fallbacks(cache.models));
                }
            }
        }
    }

    let (agy_opt, claude_opt, kiro_opt) = tokio::join!(
        discover_agy_models(agy_acc.as_ref()),
        discover_claude_models(claude_acc.as_ref()),
        discover_kiro_models(kiro_acc.as_ref()),
    );
    let mut discovered = HashMap::new();
    if let Some(agy) = agy_opt {
        discovered.insert("agy-cli".to_string(), agy);
    }
    if let Some(claude) = claude_opt {
        discovered.insert("claude-code".to_string(), claude);
    }
    if let Some(kiro) = kiro_opt {
        discovered.insert("kiro-cli".to_string(), kiro);
    }

    let catalog = resolve_catalog_with_fallbacks(discovered);
    let cache_entry = HarnessModelsCache {
        fetched_at: now,
        account_fingerprint: fingerprint,
        models: catalog.clone(),
    };
    if let Ok(json_str) = serde_json::to_string(&cache_entry) {
        let db_lock = db.lock().await;
        let _ = db_lock.set_setting(HARNESS_MODELS_CACHE_KEY, &json_str);
    }

    Ok(catalog)
}

#[tauri::command]
pub async fn list_harness_models(
    state: State<'_, DbState>,
    refresh: Option<bool>,
) -> Result<HashMap<String, Vec<HarnessModelOption>>, String> {
    list_harness_models_with_db(&state.0, refresh.unwrap_or(false)).await
}

/// Run a bounded one-shot prompt for remote callers (e.g. Discord `/ask`), persisting
/// the assistant reply into the session and returning the response text.
pub async fn run_one_shot_prompt(
    app: &AppHandle,
    db: Arc<Mutex<crate::db::Database>>,
    session_id: &str,
    prompt: &str,
) -> Result<String, String> {
    run_one_shot_prompt_with_opts(app, db, session_id, prompt, false).await
}

/// Run a bounded one-shot prompt with explicit options (e.g. `read_only` for Discord guests),
/// respecting the session's model, effort, account, WSL project runtime, and permission mode.
pub async fn run_one_shot_prompt_with_opts(
    app: &AppHandle,
    db: Arc<Mutex<crate::db::Database>>,
    session_id: &str,
    prompt: &str,
    read_only: bool,
) -> Result<String, String> {
    let (session, effective_cwd, proj_wsl, effective_harness, effective_model, effective_effort, account, enabled_mcp_servers) = {
        let db_lock = db.lock().await;
        let session = db_lock
            .get_session(session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Session '{}' not found", session_id))?;
        let proj = db_lock
            .get_project(&session.project_id)
            .map_err(|e| e.to_string())?;
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
        let effective_cwd = if proj_wsl.is_some() {
            if !session.worktree_path.trim().is_empty() && session.worktree_path != "." {
                session.worktree_path.clone()
            } else {
                proj_cwd
            }
        } else if Path::new(&session.worktree_path).exists() && session.worktree_path != "." {
            session.worktree_path.clone()
        } else {
            proj_cwd
        };

        // Resolve harness/model/effort: Discord sessions historically defaulted to "sakana",
        // which skipped installed CLI harnesses. Respect DiscordSettings and auto-detect
        // connected CLI harnesses unless explicitly forced to "sakana" (API-key mode).
        let dc_cfg = crate::discord::load_discord_settings(&db_lock);
        let raw_harness = session.harness.trim().to_string();
        let effective_harness = if raw_harness.is_empty() || raw_harness == "auto" || (raw_harness == "sakana" && session.is_discord.unwrap_or(false) && dc_cfg.default_harness != "sakana") {
            crate::discord::resolve_effective_discord_harness(&dc_cfg)
        } else {
            raw_harness
        };
        let effective_model = if !session.model.trim().is_empty() {
            session.model.trim().to_string()
        } else if session.is_discord.unwrap_or(false) && !dc_cfg.default_model.trim().is_empty() {
            dc_cfg.default_model.trim().to_string()
        } else {
            String::new()
        };
        let effective_effort = if !session.effort.trim().is_empty() {
            session.effort.trim().to_string()
        } else if session.is_discord.unwrap_or(false) && !dc_cfg.default_effort.trim().is_empty() {
            dc_cfg.default_effort.trim().to_string()
        } else {
            String::new()
        };

        let account = db_lock
            .get_harness_account(&session.account)
            .ok()
            .flatten()
            .filter(|a| a.harness == effective_harness)
            .or_else(|| db_lock.get_default_harness_account(&effective_harness).ok().flatten());
        let enabled_mcp_servers: Vec<McpServer> = db_lock
            .list_mcp_servers()
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.enabled)
            .collect();
        (session, effective_cwd, proj_wsl, effective_harness, effective_model, effective_effort, account, enabled_mcp_servers)
    };

    let one_shot_timeout_secs: u64 = std::env::var("SAKANA_ONE_SHOT_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&s| s >= 30)
        .unwrap_or(600);

    // Auto-compact long Discord threads and retain full conversation memory across turns.
    maybe_auto_compact_session(Some(app), &db, session_id).await;
    let (compacted_summary, recent_history) = {
        let db_lock = db.lock().await;
        load_session_memory_for_turn(&db_lock, session_id)
    };
    let mut sakana_ctx = load_sakana_context(&effective_cwd, proj_wsl.as_deref());
    if let Some(summary) = compacted_summary {
        let block = format!(
            "<compacted_conversation_memory>\n{}\n</compacted_conversation_memory>",
            summary
        );
        if sakana_ctx.trim().is_empty() {
            sakana_ctx = block;
        } else {
            sakana_ctx = format!("{}\n\n{}", sakana_ctx, block);
        }
    }
    let full_prompt_with_history = build_cli_prompt_with_history(&sakana_ctx, &recent_history, prompt);

    let permission_mode = if read_only {
        "plan"
    } else {
        session.permission_mode.as_str()
    };

    // Mark session RUNNING so desktop/remote threads immediately display live progress
    {
        let db_lock = db.lock().await;
        let _ = db_lock.update_session_status(session_id, "RUNNING", Some(false));
    }

    let message_id = format!("msg-{}", Uuid::new_v4());
    let started = Instant::now();
    let mut acc = TurnAccumulator::new();
    let (_cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let mut author_name = effective_harness.clone();

    let run_outcome: Result<(), String> = if let Some(ref distro) =
        proj_wsl.filter(|_| effective_harness != "sakana")
    {
        let (resolved_model, resolved_effort) = if effective_harness == "agy-cli" {
            let (m, e) =
                resolve_agy_model_and_effort_with_db(&db, &effective_model, &effective_effort)
                    .await;
            (m.unwrap_or_default(), e.unwrap_or_default())
        } else {
            (effective_model.clone(), effective_effort.clone())
        };
        match tokio::time::timeout(
            std::time::Duration::from_secs(one_shot_timeout_secs),
            run_wsl_cli_harness(
                distro,
                &effective_harness,
                account.as_ref(),
                &resolved_model,
                &resolved_effort,
                permission_mode,
                &enabled_mcp_servers,
                &effective_cwd,
                &full_prompt_with_history,
                Some(app),
                session_id,
                &message_id,
                started,
                &mut acc,
                cancel_rx,
            ),
        )
        .await
        {
            Ok(res) => res,
            Err(_) => Err(format!(
                "WSL harness request timed out after {}s.",
                one_shot_timeout_secs
            )),
        }
    } else if let Some((active_harness, bin)) = (effective_harness != "sakana")
        .then(|| {
            resolve_harness_binary(&effective_harness)
                .map(|b| (effective_harness.as_str(), b))
                .or_else(|| resolve_harness_binary("agy-cli").map(|b| ("agy-cli", b)))
                .or_else(|| resolve_harness_binary("claude-code").map(|b| ("claude-code", b)))
                .or_else(|| resolve_harness_binary("kiro-cli").map(|b| ("kiro-cli", b)))
        })
        .flatten()
    {
        author_name = active_harness.to_string();
        let (resolved_model, resolved_effort) = if active_harness == "agy-cli" {
            let (m, e) =
                resolve_agy_model_and_effort_with_db(&db, &effective_model, &effective_effort)
                    .await;
            (m.unwrap_or_default(), e.unwrap_or_default())
        } else {
            (effective_model.clone(), effective_effort.clone())
        };
        let run_cwd = if Path::new(&effective_cwd).is_dir() {
            effective_cwd.clone()
        } else {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        };
        match tokio::time::timeout(
            std::time::Duration::from_secs(one_shot_timeout_secs),
            run_cli_harness(
                &bin,
                active_harness,
                account.as_ref(),
                &resolved_model,
                &resolved_effort,
                permission_mode,
                &enabled_mcp_servers,
                &run_cwd,
                &full_prompt_with_history,
                Some(app),
                session_id,
                &message_id,
                started,
                &mut acc,
                cancel_rx,
            ),
        )
        .await
        {
            Ok(res) => res,
            Err(_) => Err(format!(
                "Harness request timed out after {}s.",
                one_shot_timeout_secs
            )),
        }
    } else {
        author_name = "Sakana".to_string();
        let http_model = if !effective_model.is_empty() {
            effective_model.clone()
        } else if !session.model.trim().is_empty() {
            session.model.trim().to_string()
        } else {
            "gpt-4o-mini".to_string()
        };
        match tokio::time::timeout(
            std::time::Duration::from_secs(one_shot_timeout_secs),
            run_http_sse_fallback(
                &effective_cwd,
                &http_model,
                &sakana_ctx,
                &recent_history,
                prompt,
                Some(app),
                session_id,
                &message_id,
                started,
                &mut acc,
                cancel_rx,
            ),
        )
        .await
        {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(
                "Prompt queued in Sakana, but no CLI harness or API key is configured on this host."
                    .to_string(),
            ),
            Err(_) => Err(format!(
                "API request timed out after {}s.",
                one_shot_timeout_secs
            )),
        }
    };

    if let Err(err_msg) = run_outcome {
        if acc.text.trim().is_empty() {
            acc.text = err_msg;
        } else {
            acc.text.push_str(&format!("\n\n**Error:** {}", err_msg));
        }
    }
    if acc.text.trim().is_empty() {
        acc.text = "Completed with no output.".to_string();
    }

    let reply_text = acc.text.clone();
    let assistant_msg = acc.into_message(message_id.clone(), author_name);
    {
        let db_lock = db.lock().await;
        let _ = db_lock.append_message(session_id, &assistant_msg);
        let _ = db_lock.update_session_status(session_id, "DONE", Some(true));
    }
    crate::platform::emit_app_and_remote(
        Some(app),
        "turn:done",
        TurnDoneEvent {
            session_id: session_id.to_string(),
            message_id,
            message: assistant_msg,
            status: "DONE".to_string(),
        },
    );
    Ok(reply_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agy_stream_json_step_update_and_avoids_duplicate_result() {
        let mut acc = TurnAccumulator::new();
        let started = Instant::now();

        let lines = [
            r#"{"event":"init","conversation_id":"c1","init":{"cwd":"."}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"c1","step_index":0,"state":"DONE","step_type":"user_input"}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"c1","step_index":1,"state":"ACTIVE","step_type":"agent_response","text_delta":"ok"}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"c1","step_index":1,"state":"DONE","step_type":"agent_response","text_delta":"\n","duration_seconds":1.2}}"#,
            r#"{"event":"result","result":{"conversation_id":"c1","status":"SUCCESS","response":"ok\n","duration_seconds":2.0,"num_turns":1}}"#,
        ];

        for line in lines {
            process_stream_json_line(line, None, "s1", "m1", started, &mut acc);
        }

        assert_eq!(acc.text, "ok\n");
        assert!(acc.stream_error.is_none());
    }

    #[test]
    fn parses_agy_stream_json_result_fallback_when_no_deltas() {
        let mut acc = TurnAccumulator::new();
        let started = Instant::now();

        let lines = [
            r#"{"event":"init","conversation_id":"c2"}"#,
            r#"{"event":"result","result":{"conversation_id":"c2","status":"SUCCESS","response":"hello from result"}}"#,
        ];

        for line in lines {
            process_stream_json_line(line, None, "s1", "m1", started, &mut acc);
        }

        assert_eq!(acc.text, "hello from result");
        assert!(acc.stream_error.is_none());
    }

    #[test]
    fn parses_agy_stream_json_error_result() {
        let mut acc = TurnAccumulator::new();
        let started = Instant::now();

        let line = r#"{"event":"result","result":{"conversation_id":"","status":"ERROR","response":"","error":"invalid model selection"}}"#;
        process_stream_json_line(line, None, "s1", "m1", started, &mut acc);

        assert!(acc.text.is_empty());
        assert_eq!(acc.stream_error.as_deref(), Some("invalid model selection"));
    }

    #[test]
    fn parses_sakana_ui_json_and_raw_html_blocks() {
        let json_md = "Here is a chart:\n```sakana-ui\n{\"title\":\"Perf\",\"subtitle\":\"Bench\",\"metrics\":[{\"label\":\"P95\",\"value\":\"12ms\",\"delta\":\"-2ms\",\"ok\":true}],\"bars\":[]}\n```\nDone.";
        let w = parse_sakana_ui_widget(json_md).expect("should parse json widget");
        assert_eq!(w.title, "Perf");
        assert_eq!(w.subtitle, "Bench");
        assert_eq!(w.metrics.len(), 1);
        assert_eq!(w.metrics[0].label, "P95");

        let html_md = "```sakana-ui\n<div>Hello</div>\n```";
        let w2 = parse_sakana_ui_widget(html_md).expect("should parse html widget");
        assert_eq!(w2.source_code, "<div>Hello</div>");

        assert!(parse_sakana_ui_widget("plain markdown ```rust\nfn main() {}\n```").is_none());
    }

    #[test]
    fn maps_permission_modes_to_cli_flags() {
        assert_eq!(
            build_permission_args("claude-code", "yolo"),
            vec!["--dangerously-skip-permissions"]
        );
        assert_eq!(
            build_permission_args("claude-code", "auto-edit"),
            vec!["--dangerously-skip-permissions"]
        );
        assert_eq!(
            build_permission_args("claude-code", "plan"),
            vec!["--permission-mode", "plan"]
        );
        assert_eq!(
            build_permission_args("agy-cli", "plan"),
            vec!["--mode", "plan"]
        );
        assert_eq!(
            build_permission_args("agy-cli", "yolo"),
            vec!["--dangerously-skip-permissions"]
        );
    }

    #[test]
    fn builds_mcp_config_only_for_enabled_servers() {
        let servers = vec![
            McpServer {
                id: "1".to_string(),
                name: "fs".to_string(),
                transport: "stdio".to_string(),
                endpoint: "npx -y @modelcontextprotocol/server-filesystem .".to_string(),
                enabled: true,
                status: "connected".to_string(),
                latency_ms: None,
            },
            McpServer {
                id: "2".to_string(),
                name: "disabled".to_string(),
                transport: "sse".to_string(),
                endpoint: "http://localhost:8080/sse".to_string(),
                enabled: false,
                status: "idle".to_string(),
                latency_ms: None,
            },
        ];
        let cfg = build_mcp_config_json(&servers).expect("mcp config");
        let map = cfg["mcpServers"].as_object().unwrap();
        assert!(map.contains_key("fs"));
        assert!(!map.contains_key("disabled"));
        assert_eq!(map["fs"]["command"], "npx");
    }

    #[test]
    fn builds_multi_turn_http_messages_without_duplicating_current_user_prompt() {
        let history = vec![
            ChatMessage {
                id: "1".to_string(),
                role: "user".to_string(),
                author: None,
                timestamp: "10:00".to_string(),
                text: "Hi".to_string(),
                thinking: None,
                thinking_duration: None,
                tools: None,
                subagent: None,
                widget: None,
                checkpoint: None,
                failover_notice: None,
            },
            ChatMessage {
                id: "2".to_string(),
                role: "assistant".to_string(),
                author: Some("sakana".to_string()),
                timestamp: "10:01".to_string(),
                text: "Hello!".to_string(),
                thinking: None,
                thinking_duration: None,
                tools: None,
                subagent: None,
                widget: None,
                checkpoint: None,
                failover_notice: None,
            },
            ChatMessage {
                id: "3".to_string(),
                role: "user".to_string(),
                author: None,
                timestamp: "10:02".to_string(),
                text: "Next".to_string(),
                thinking: None,
                thinking_duration: None,
                tools: None,
                subagent: None,
                widget: None,
                checkpoint: None,
                failover_notice: None,
            },
        ];
        let msgs = build_http_messages("sys", &history, "Next\n\nAttached Context:\n- file.rs");
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[1]["content"], "Hi");
        assert_eq!(msgs[2]["content"], "Hello!");
        assert_eq!(msgs[3]["content"], "Next\n\nAttached Context:\n- file.rs");
    }

    #[tokio::test]
    async fn list_harness_models_with_db_uses_cache_when_not_refreshing() {
        let dir = std::env::temp_dir().join(format!("sakana-models-test-{}", Uuid::new_v4()));
        let db = crate::db::Database::open_in_dir(&dir).unwrap();
        let mut cached = HashMap::new();
        cached.insert(
            "agy-cli".to_string(),
            vec![opt("custom-sub-model", "Custom Sub", "gemini", &["high"])],
        );
        let agy_acc = db.get_default_harness_account("agy-cli").ok().flatten();
        let claude_acc = db.get_default_harness_account("claude-code").ok().flatten();
        let kiro_acc = db.get_default_harness_account("kiro-cli").ok().flatten();
        let cache_entry = HarnessModelsCache {
            fetched_at: now_epoch_secs(),
            account_fingerprint: compute_account_fingerprint(
                agy_acc.as_ref(),
                claude_acc.as_ref(),
                kiro_acc.as_ref(),
            ),
            models: cached,
        };
        db.set_setting(
            HARNESS_MODELS_CACHE_KEY,
            &serde_json::to_string(&cache_entry).unwrap(),
        )
        .unwrap();
        let db_arc = Arc::new(Mutex::new(db));

        let res = list_harness_models_with_db(&db_arc, false).await.unwrap();
        let agy = res.get("agy-cli").unwrap();
        assert!(agy.iter().any(|m| m.id == "custom-sub-model"));
        assert!(!agy.iter().any(|m| m.id == "gemini-4-hydrogen"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn list_harness_models_with_db_refreshes_expired_cache() {
        let dir = std::env::temp_dir().join(format!("sakana-models-ttl-test-{}", Uuid::new_v4()));
        let db = crate::db::Database::open_in_dir(&dir).unwrap();
        let mut stale = HashMap::new();
        stale.insert(
            "agy-cli".to_string(),
            vec![opt("stale-only-model", "Stale", "gemini", &["high"])],
        );
        let expired_entry = HarnessModelsCache {
            fetched_at: 1,
            account_fingerprint: String::new(),
            models: stale,
        };
        db.set_setting(
            HARNESS_MODELS_CACHE_KEY,
            &serde_json::to_string(&expired_entry).unwrap(),
        )
        .unwrap();
        let db_arc = Arc::new(Mutex::new(db));

        let res = list_harness_models_with_db(&db_arc, false).await.unwrap();
        let agy = res.get("agy-cli").unwrap();
        assert!(!agy.iter().any(|m| m.id == "stale-only-model"));
        assert!(agy.iter().any(|m| m.id == "gemini-4-hydrogen"));

        let raw_updated = db_arc
            .lock()
            .await
            .get_setting(HARNESS_MODELS_CACHE_KEY)
            .unwrap()
            .unwrap();
        let parsed: HarnessModelsCache = serde_json::from_str(&raw_updated).unwrap();
        assert!(parsed.fetched_at > 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn builds_cli_prompt_with_conversation_history_and_excludes_current_turn() {
        let history = vec![
            ChatMessage {
                id: "1".to_string(),
                role: "user".to_string(),
                author: None,
                timestamp: "10:00".to_string(),
                text: "First question".to_string(),
                thinking: None,
                thinking_duration: None,
                tools: None,
                subagent: None,
                widget: None,
                checkpoint: None,
                failover_notice: None,
            },
            ChatMessage {
                id: "2".to_string(),
                role: "assistant".to_string(),
                author: Some("sakana".to_string()),
                timestamp: "10:01".to_string(),
                text: "First answer".to_string(),
                thinking: None,
                thinking_duration: None,
                tools: None,
                subagent: None,
                widget: None,
                checkpoint: None,
                failover_notice: None,
            },
            ChatMessage {
                id: "3".to_string(),
                role: "user".to_string(),
                author: None,
                timestamp: "10:02".to_string(),
                text: "Follow up".to_string(),
                thinking: None,
                thinking_duration: None,
                tools: None,
                subagent: None,
                widget: None,
                checkpoint: None,
                failover_notice: None,
            },
        ];

        let prompt = build_cli_prompt_with_history("Project SAKANA.md", &history, "Follow up");
        assert!(prompt.contains("Project SAKANA.md"));
        assert!(prompt.contains("<conversation_history>"));
        assert!(prompt.contains("[User]: First question"));
        assert!(prompt.contains("[Assistant]: First answer"));
        assert!(!prompt.contains("[User]: Follow up"));
        assert!(prompt.ends_with("Follow up"));

        // Single-turn with no prior history should omit <conversation_history>.
        let single = build_cli_prompt_with_history("", &history[2..], "Follow up");
        assert!(!single.contains("<conversation_history>"));
        assert_eq!(single, "Follow up");
    }

    #[test]
    fn resolve_agy_model_and_effort_handles_efforts_suffixes_and_stale_ids() {
        let catalog = fallback_agy_models();

        // No-effort model omits --effort even when high is requested.
        assert_eq!(
            resolve_agy_model_and_effort("claude-sonnet-4-6", "high", &catalog),
            (Some("claude-sonnet-4-6".to_string()), None)
        );

        // Restricted model clamps unsupported effort to "medium".
        assert_eq!(
            resolve_agy_model_and_effort("gpt-oss-120b", "high", &catalog),
            (Some("gpt-oss-120b".to_string()), Some("medium".to_string()))
        );

        // Model without xhigh clamps xhigh to "medium".
        assert_eq!(
            resolve_agy_model_and_effort("gemini-3.8-flash", "xhigh", &catalog),
            (Some("gemini-3.8-flash".to_string()), Some("medium".to_string()))
        );

        // Effort-suffixed ID strips suffix and resolves base model + effort.
        assert_eq!(
            resolve_agy_model_and_effort("gpt-oss-120b-medium", "", &catalog),
            (Some("gpt-oss-120b".to_string()), Some("medium".to_string()))
        );

        // Stale / foreign model ID falls back to catalog.first().
        assert_eq!(
            resolve_agy_model_and_effort("gemini-4-neon", "high", &catalog),
            (Some("gemini-4-hydrogen".to_string()), Some("high".to_string()))
        );

        // Empty catalog returns (None, None).
        assert_eq!(
            resolve_agy_model_and_effort("gemini-4-hydrogen", "high", &[]),
            (None, None)
        );
    }
}

