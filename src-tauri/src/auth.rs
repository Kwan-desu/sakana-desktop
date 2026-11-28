use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use regex::Regex;
use tauri::State;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::db::Database;
use crate::harness::resolve_harness_binary;
use crate::models::{HarnessAccountRow, HarnessAuthAccount, HarnessLoginResponse, McpServer};
use crate::platform::{app_data_dir, get_env_or_dotenv, tokio_command};
use crate::DbState;

const AGY_ACP_APP_DATA_DIR: &str = "antigravity-cli";

pub struct ActiveLoginProc {
    pub child: Child,
    pub stdin: Option<ChildStdin>,
    pub output_buf: Arc<Mutex<String>>,
}

#[derive(Default)]
pub struct AuthState {
    pub login_procs: Arc<Mutex<HashMap<String, ActiveLoginProc>>>,
    pub last_outputs: Arc<Mutex<HashMap<String, String>>>,
}

fn home_dir_or_fallback() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

pub fn ensure_profile_dir(harness: &str, account_id: &str) -> PathBuf {
    let bucket = match harness {
        "agy-cli" => "agy",
        "kiro-cli" => "kiro",
        "claude-code" => "claude",
        _ => "other",
    };
    let prof = app_data_dir().join("profiles").join(bucket).join(account_id);
    match harness {
        "agy-cli" => {
            let _ = std::fs::create_dir_all(prof.join(".gemini").join(AGY_ACP_APP_DATA_DIR));
        }
        "kiro-cli" => {
            let _ = std::fs::create_dir_all(prof.join(".kiro"));
            let _ = std::fs::create_dir_all(prof.join(".local").join("share").join("kiro-cli"));
            let _ = std::fs::create_dir_all(prof.join(".config"));
            let _ = std::fs::create_dir_all(prof.join(".local").join("state"));
            let _ = std::fs::create_dir_all(prof.join("AppData").join("Local").join("kiro-cli"));
            let _ = std::fs::create_dir_all(prof.join("AppData").join("Roaming").join("kiro-cli"));
        }
        "claude-code" => {
            let _ = std::fs::create_dir_all(&prof);
        }
        _ => {
            let _ = std::fs::create_dir_all(&prof);
        }
    }
    prof
}

pub fn agy_profile_dir(acc: &HarnessAccountRow) -> PathBuf {
    if !acc.profile_dir.trim().is_empty() {
        PathBuf::from(&acc.profile_dir)
    } else {
        let id = if acc.id.trim().is_empty() {
            "default"
        } else {
            acc.id.trim()
        };
        ensure_profile_dir("agy-cli", id)
    }
}

pub fn agy_gemini_dir(acc: &HarnessAccountRow) -> PathBuf {
    agy_profile_dir(acc).join(".gemini")
}

fn agy_acp_data_dir(acc: &HarnessAccountRow) -> PathBuf {
    agy_gemini_dir(acc).join(AGY_ACP_APP_DATA_DIR)
}

fn write_agy_settings(acc: &HarnessAccountRow, auth_type: &str) {
    let dir = agy_acp_data_dir(acc);
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("settings.json");
    let mut root: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = root.as_object_mut() {
        obj.insert("auth".to_string(), serde_json::json!({ "type": auth_type }));
    }
    if let Ok(pretty) = serde_json::to_string_pretty(&root) {
        let _ = std::fs::write(path, format!("{pretty}\n"));
    }
}

fn kiro_data_dirs(acc: &HarnessAccountRow) -> Vec<PathBuf> {
    if acc.id != "kiro_default" && !acc.profile_dir.trim().is_empty() {
        let prof = PathBuf::from(&acc.profile_dir);
        return vec![
            prof.join(".local").join("share").join("kiro-cli"),
            prof.join("AppData").join("Local").join("kiro-cli"),
            prof.join("AppData").join("Roaming").join("kiro-cli"),
        ];
    }
    let mut dirs_vec = vec![home_dir_or_fallback()
        .join(".local")
        .join("share")
        .join("kiro-cli")];
    if let Some(local) = dirs::data_local_dir() {
        dirs_vec.push(local.join("kiro-cli"));
    }
    if let Some(roam) = dirs::config_dir() {
        dirs_vec.push(roam.join("kiro-cli"));
    }
    dirs_vec
}

fn claude_config_dir(acc: &HarnessAccountRow) -> PathBuf {
    if acc.id != "claude_default" && !acc.profile_dir.trim().is_empty() {
        PathBuf::from(&acc.profile_dir)
    } else {
        home_dir_or_fallback().join(".claude")
    }
}

pub fn is_account_authenticated(acc: &HarnessAccountRow) -> bool {
    match acc.harness.as_str() {
        "agy-cli" => {
            if acc.auth_type == "gemini-api-key" {
                return get_env_or_dotenv("GEMINI_API_KEY").is_some();
            }
            if acc.auth_type == "agent-platform" {
                return get_env_or_dotenv("GOOGLE_API_KEY").is_some()
                    || get_env_or_dotenv("GOOGLE_APPLICATION_CREDENTIALS").is_some()
                    || get_env_or_dotenv("GOOGLE_CLOUD_PROJECT").is_some();
            }
            let gdir = agy_gemini_dir(acc);
            for check_dir in [gdir.join(AGY_ACP_APP_DATA_DIR), gdir.join("antigravity-acp"), gdir.clone()] {
                if !check_dir.is_dir() {
                    continue;
                }
                for name in [
                    "antigravity-oauth-token",
                    "acp_token.json",
                    "oauth_creds.json",
                    "credentials.json",
                    "auth.json",
                ] {
                    let p = check_dir.join(name);
                    if p.is_file() && p.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                        return true;
                    }
                }
                if let Ok(entries) = std::fs::read_dir(&check_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                            let lower = entry.file_name().to_string_lossy().to_ascii_lowercase();
                            if lower.contains("token") || lower.contains("oauth") {
                                return true;
                            }
                        }
                    }
                }
            }
            false
        }
        "kiro-cli" => {
            for dir in kiro_data_dirs(acc) {
                if dir.join("data.sqlite3").is_file()
                    || dir.join("accounts").join("registry.json").is_file()
                {
                    return true;
                }
            }
            false
        }
        "claude-code" => {
            if acc.auth_type == "anthropic-api-key" {
                return get_env_or_dotenv("ANTHROPIC_API_KEY").is_some();
            }
            if get_env_or_dotenv("ANTHROPIC_API_KEY").is_some() {
                return true;
            }
            let cdir = claude_config_dir(acc);
            for name in [".credentials.json", "credentials.json", "auth.json"] {
                let p = cdir.join(name);
                if p.is_file() && p.metadata().map(|m| m.len() > 2).unwrap_or(false) {
                    return true;
                }
            }
            if acc.id == "claude_default" {
                let claude_json = home_dir_or_fallback().join(".claude.json");
                if let Ok(content) = std::fs::read_to_string(claude_json) {
                    if content.contains("oauthAccount") {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

fn extract_email_from_claude_json(acc: &HarnessAccountRow) -> Option<String> {
    let path = if acc.id == "claude_default" {
        home_dir_or_fallback().join(".claude.json")
    } else {
        claude_config_dir(acc).join(".claude.json")
    };
    let content = std::fs::read_to_string(path).ok()?;
    let val: serde_json::Value = serde_json::from_str(&content).ok()?;
    val.get("oauthAccount")
        .and_then(|o| o.get("emailAddress"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn extract_email_from_agy_files(acc: &HarnessAccountRow) -> Option<String> {
    let gdir = agy_gemini_dir(acc);
    let reg_path = gdir.join(AGY_ACP_APP_DATA_DIR).join("accounts").join("registry.json");
    if let Ok(raw) = std::fs::read_to_string(&reg_path) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(active) = val
                .get("active")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && s.contains('@'))
            {
                return Some(active);
            }
            if let Some(accounts) = val.get("accounts").and_then(|v| v.as_object()) {
                for (k, v) in accounts {
                    if let Some(em) = v
                        .get("email")
                        .and_then(|e| e.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty() && s.contains('@'))
                    {
                        return Some(em);
                    }
                    if k.contains('@') {
                        return Some(k.trim().to_string());
                    }
                }
            }
        }
    }
    for path in [
        gdir.join(AGY_ACP_APP_DATA_DIR).join("antigravity-oauth-token"),
        gdir.join("antigravity-acp").join("antigravity-oauth-token"),
        gdir.join("google_accounts.json"),
    ] {
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                for candidate in [
                    val.get("email").and_then(|v| v.as_str()),
                    val.get("active").and_then(|v| v.as_str()),
                    val.get("account").and_then(|a| a.get("email")).and_then(|v| v.as_str()),
                ] {
                    if let Some(em) = candidate.map(|s| s.trim()).filter(|s| !s.is_empty() && s.contains('@')) {
                        return Some(em.to_string());
                    }
                }
            }
        }
    }
    None
}

pub fn read_agy_file_token(acc: &HarnessAccountRow) -> Option<(String, Option<String>)> {
    let dir = agy_gemini_dir(acc);
    let candidates = [
        dir.join(AGY_ACP_APP_DATA_DIR).join("antigravity-oauth-token"),
        dir.join("antigravity-acp").join("antigravity-oauth-token"),
        dir.join(AGY_ACP_APP_DATA_DIR).join("acp_token.json"),
        dir.join("oauth_creds.json"),
        dir.join("google_accounts.json"),
    ];
    for p in candidates {
        if let Ok(raw) = std::fs::read_to_string(&p) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                let token_opt = val
                    .get("token")
                    .and_then(|t| t.get("access_token"))
                    .and_then(|v| v.as_str())
                    .or_else(|| val.get("access_token").and_then(|v| v.as_str()))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                if let Some(token) = token_opt {
                    let email = val
                        .get("email")
                        .and_then(|v| v.as_str())
                        .or_else(|| val.get("account").and_then(|a| a.get("email")).and_then(|v| v.as_str()))
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .or_else(|| extract_email_from_agy_files(acc));
                    return Some((token, email));
                }
            }
        }
    }
    None
}

pub fn resolve_account_email(acc: &HarnessAccountRow) -> String {
    if !acc.email.trim().is_empty() {
        return acc.email.trim().to_string();
    }
    match acc.harness.as_str() {
        "claude-code" => extract_email_from_claude_json(acc).unwrap_or_default(),
        "agy-cli" => extract_email_from_agy_files(acc).unwrap_or_default(),
        _ => String::new(),
    }
}

fn link_dir_if_exists(src: &Path, dst: &Path) {
    if !src.exists() || dst.exists() || dst.symlink_metadata().is_ok() {
        return;
    }
    if let Some(parent) = dst.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    #[cfg(unix)]
    {
        let _ = std::os::unix::fs::symlink(src, dst);
    }
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_dir(src, dst).is_err() {
            let _ = copy_dir_recursive(src, dst);
        }
    }
}

#[cfg(windows)]
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            let _ = std::fs::copy(entry.path(), target);
        }
    }
    Ok(())
}

fn read_mcp_servers_from_file(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return serde_json::Map::new();
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return serde_json::Map::new();
    };
    val.get("mcpServers")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default()
}

/// Sync global user MCP configurations + Sakana DB MCP servers + skills directories
/// into an isolated harness profile so tools & skills remain available when HOME is overridden.
pub fn sync_profile_mcp_and_skills(
    harness: &str,
    acc: Option<&HarnessAccountRow>,
    enabled_mcp_servers: &[McpServer],
) {
    let real_home = home_dir_or_fallback();

    match harness {
        "agy-cli" => {
            let prof = match acc {
                Some(account) => agy_profile_dir(account),
                None => ensure_profile_dir("agy-cli", "default"),
            };
            let prof_gemini = prof.join(".gemini");

            // 1. Link global skills directories from real HOME into isolated profile
            for rel in [
                ".gemini/config/skills",
                ".gemini/antigravity/skills",
                ".gemini/antigravity-cli/skills",
                ".sakana/skills",
                ".agents/skills",
                ".claude/skills",
            ] {
                link_dir_if_exists(&real_home.join(rel), &prof.join(rel));
            }

            // 2. Merge MCP servers from real HOME Antigravity configs + Sakana DB enabled servers
            let mut merged = serde_json::Map::new();
            for src in [
                real_home.join(".gemini").join("settings.json"),
                real_home.join(".gemini").join("antigravity").join("mcp_config.json"),
                real_home.join(".gemini").join("antigravity-cli").join("mcp_config.json"),
                real_home.join(".gemini").join("config").join("mcp_config.json"),
            ] {
                for (k, v) in read_mcp_servers_from_file(&src) {
                    merged.insert(k, v);
                }
            }

            for s in enabled_mcp_servers {
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
                    serde_json::json!({ "type": kind, "url": endpoint, "serverUrl": endpoint })
                } else {
                    let mut parts = endpoint.split_whitespace();
                    let Some(command) = parts.next() else {
                        continue;
                    };
                    let args: Vec<&str> = parts.collect();
                    serde_json::json!({ "command": command, "args": args })
                };
                merged.insert(s.name.clone(), entry);
            }

            if !merged.is_empty() {
                let mcp_payload = serde_json::json!({ "mcpServers": merged });
                if let Ok(pretty) = serde_json::to_string_pretty(&mcp_payload) {
                    for dst in [
                        prof_gemini.join("config").join("mcp_config.json"),
                        prof_gemini.join(AGY_ACP_APP_DATA_DIR).join("mcp_config.json"),
                        prof_gemini.join("antigravity").join("mcp_config.json"),
                    ] {
                        if let Some(parent) = dst.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let _ = std::fs::write(&dst, format!("{pretty}\n"));
                    }
                }

                // Also merge mcpServers into settings.json files while preserving auth and other keys
                for settings_path in [
                    prof_gemini.join(AGY_ACP_APP_DATA_DIR).join("settings.json"),
                    prof_gemini.join("settings.json"),
                ] {
                    if let Some(parent) = settings_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let mut root: serde_json::Value = std::fs::read_to_string(&settings_path)
                        .ok()
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or_else(|| serde_json::json!({}));
                    if let Some(obj) = root.as_object_mut() {
                        obj.insert(
                            "mcpServers".to_string(),
                            serde_json::Value::Object(merged.clone()),
                        );
                    }
                    if let Ok(pretty) = serde_json::to_string_pretty(&root) {
                        let _ = std::fs::write(&settings_path, format!("{pretty}\n"));
                    }
                }
            }
        }
        "kiro-cli" => {
            if let Some(account) = acc {
                if account.id != "kiro_default" && !account.profile_dir.trim().is_empty() {
                    let prof = PathBuf::from(&account.profile_dir);
                    for rel in [".kiro/skills", ".kiro/settings", ".sakana/skills", ".agents/skills"] {
                        link_dir_if_exists(&real_home.join(rel), &prof.join(rel));
                    }
                }
            }
        }
        "claude-code" => {
            if let Some(account) = acc {
                if account.id != "claude_default" && !account.profile_dir.trim().is_empty() {
                    let cdir = PathBuf::from(&account.profile_dir);
                    for rel in ["skills", "commands", "agents"] {
                        link_dir_if_exists(&real_home.join(".claude").join(rel), &cdir.join(rel));
                    }
                }
            }
        }
        _ => {}
    }
}

/// Apply isolated profile environment variables and flags when spawning a CLI harness turn.
pub fn apply_account_env(
    cmd: &mut tokio::process::Command,
    harness: &str,
    acc: Option<&HarnessAccountRow>,
) {
    // Inject saved .env API keys if present
    for key in [
        "ANTHROPIC_API_KEY",
        "GEMINI_API_KEY",
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "GOOGLE_API_KEY",
        "GOOGLE_CLOUD_PROJECT",
        "GOOGLE_CLOUD_LOCATION",
    ] {
        if let Some(val) = get_env_or_dotenv(key) {
            cmd.env(key, val);
        }
    }

    let Some(account) = acc else {
        if harness == "agy-cli" {
            let prof = ensure_profile_dir("agy-cli", "default");
            let gdir = prof.join(".gemini");
            let _ = std::fs::create_dir_all(gdir.join(AGY_ACP_APP_DATA_DIR));
            cmd.env("AGY_CLI_FORCE_FILE_TOKEN_STORAGE", "1");
            cmd.env("SAKANA_AGY_GEMINI_DIR", &gdir);
            cmd.env("HOME", &prof);
            cmd.env("USERPROFILE", &prof);
        }
        return;
    };

    match harness {
        "agy-cli" => {
            let prof = agy_profile_dir(account);
            let gdir = prof.join(".gemini");
            let _ = std::fs::create_dir_all(gdir.join(AGY_ACP_APP_DATA_DIR));
            cmd.env("AGY_CLI_FORCE_FILE_TOKEN_STORAGE", "1");
            cmd.env("SAKANA_AGY_GEMINI_DIR", &gdir);
            cmd.env("HOME", &prof);
            cmd.env("USERPROFILE", &prof);
        }
        "kiro-cli" => {
            if account.id != "kiro_default" && !account.profile_dir.trim().is_empty() {
                let prof = PathBuf::from(&account.profile_dir);
                let local_app = prof.join("AppData").join("Local");
                let roam_app = prof.join("AppData").join("Roaming");
                let _ = std::fs::create_dir_all(prof.join(".local").join("share").join("kiro-cli"));
                let _ = std::fs::create_dir_all(prof.join(".config"));
                let _ = std::fs::create_dir_all(prof.join(".local").join("state"));
                let _ = std::fs::create_dir_all(local_app.join("kiro-cli"));
                let _ = std::fs::create_dir_all(roam_app.join("kiro-cli"));
                cmd.env("HOME", &prof);
                cmd.env("USERPROFILE", &prof);
                cmd.env("LOCALAPPDATA", &local_app);
                cmd.env("APPDATA", &roam_app);
                cmd.env("XDG_CONFIG_HOME", prof.join(".config"));
                cmd.env("XDG_DATA_HOME", prof.join(".local").join("share"));
                cmd.env("XDG_STATE_HOME", prof.join(".local").join("state"));
            }
        }
        "claude-code" => {
            if account.id != "claude_default" && !account.profile_dir.trim().is_empty() {
                let cdir = PathBuf::from(&account.profile_dir);
                let _ = std::fs::create_dir_all(&cdir);
                cmd.env("CLAUDE_CONFIG_DIR", &cdir);
            }
        }
        _ => {}
    }
}

fn extract_url_and_code(text: &str) -> (String, String) {
    let mut auth_url = String::new();
    if let Ok(re_google) = Regex::new(r#"https://accounts\.google\.com/[^\s"'<>]+"#) {
        if let Some(m) = re_google.find(text) {
            auth_url = m.as_str().trim_end_matches(&[')', '.', ',', ';'][..]).to_string();
        }
    }
    if auth_url.is_empty() {
        if let Ok(re_any) = Regex::new(r#"https?://[^\s"'<>]+"#) {
            if let Some(m) = re_any.find(text) {
                auth_url = m.as_str().trim_end_matches(&[')', '.', ',', ';'][..]).to_string();
            }
        }
    }

    let mut device_code = String::new();
    if let Ok(re_code) = Regex::new(r"\b[A-Z0-9]{4}-[A-Z0-9]{4}\b") {
        if let Some(m) = re_code.find(text) {
            device_code = m.as_str().to_string();
        }
    }
    (auth_url, device_code)
}

fn to_auth_account(
    row: HarnessAccountRow,
    last_outputs: &HashMap<String, String>,
) -> HarnessAuthAccount {
    let bin_opt = resolve_harness_binary(&row.harness);
    let cli_installed = bin_opt.is_some();
    let cli_bin = bin_opt
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let authenticated = is_account_authenticated(&row);
    let email = resolve_account_email(&row);
    let status = if authenticated {
        "authenticated".to_string()
    } else if row.status == "pending_login" {
        "pending_login".to_string()
    } else {
        "unauthenticated".to_string()
    };
    let last_login_output = last_outputs.get(&row.id).cloned().unwrap_or_default();

    HarnessAuthAccount {
        id: row.id,
        harness: row.harness,
        label: row.label,
        email,
        profile_dir: row.profile_dir,
        auth_type: row.auth_type,
        is_default: row.is_default,
        authenticated,
        status,
        cli_installed,
        cli_bin,
        last_login_output,
    }
}

pub async fn list_harness_accounts_inner(
    db: &Arc<Mutex<Database>>,
    auth: &AuthState,
    harness: Option<String>,
) -> Result<Vec<HarnessAuthAccount>, String> {
    let rows = {
        let db = db.lock().await;
        db.list_harness_accounts(harness.as_deref())
            .map_err(|e| e.to_string())?
    };
    let outputs = auth.last_outputs.lock().await;
    Ok(rows
        .into_iter()
        .map(|r| to_auth_account(r, &outputs))
        .collect())
}

#[tauri::command]
pub async fn list_harness_accounts(
    db_state: State<'_, DbState>,
    auth_state: State<'_, AuthState>,
    harness: Option<String>,
) -> Result<Vec<HarnessAuthAccount>, String> {
    list_harness_accounts_inner(&db_state.0, &auth_state, harness).await
}

pub async fn create_harness_account_inner(
    db: &Arc<Mutex<Database>>,
    auth: &AuthState,
    harness: String,
    label: String,
    email: Option<String>,
    auth_type: Option<String>,
) -> Result<Vec<HarnessAuthAccount>, String> {
    let clean_label = {
        let trimmed = label.trim();
        if trimmed.is_empty() {
            let existing_count = {
                let db = db.lock().await;
                db.list_harness_accounts(Some(&harness))
                    .map(|v| v.len())
                    .unwrap_or(0)
            };
            let base = match harness.as_str() {
                "agy-cli" => "AGY Account",
                "kiro-cli" => "Kiro Account",
                "claude-code" => "Claude Account",
                _ => "Account",
            };
            format!("{base} {}", existing_count + 1)
        } else {
            trimmed.to_string()
        }
    };
    let prefix = match harness.as_str() {
        "agy-cli" => "agy",
        "kiro-cli" => "kiro",
        "claude-code" => "claude",
        _ => return Err(format!("Unsupported harness '{harness}'")),
    };
    let id = format!("{}_{}", prefix, &Uuid::new_v4().simple().to_string()[..8]);
    let prof = ensure_profile_dir(&harness, &id);
    let default_auth_type = match harness.as_str() {
        "agy-cli" => "oauth-personal",
        "kiro-cli" => "device-flow",
        "claude-code" => "claude-oauth",
        _ => "oauth",
    };
    let row = HarnessAccountRow {
        id,
        harness,
        label: clean_label,
        email: email.unwrap_or_default().trim().to_string(),
        profile_dir: prof.to_string_lossy().to_string(),
        auth_type: auth_type
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| default_auth_type.to_string()),
        is_default: false,
        status: "unauthenticated".to_string(),
    };
    {
        let db = db.lock().await;
        db.create_harness_account(&row).map_err(|e| e.to_string())?;
    }
    list_harness_accounts_inner(db, auth, None).await
}

#[tauri::command]
pub async fn create_harness_account(
    db_state: State<'_, DbState>,
    auth_state: State<'_, AuthState>,
    harness: String,
    label: String,
    email: Option<String>,
    auth_type: Option<String>,
) -> Result<Vec<HarnessAuthAccount>, String> {
    create_harness_account_inner(&db_state.0, &auth_state, harness, label, email, auth_type).await
}

pub async fn set_default_harness_account_inner(
    db: &Arc<Mutex<Database>>,
    auth: &AuthState,
    account_id: String,
) -> Result<Vec<HarnessAuthAccount>, String> {
    {
        let db = db.lock().await;
        let acc = db
            .get_harness_account(&account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Account '{account_id}' not found"))?;
        db.set_default_harness_account(&acc.harness, &account_id)
            .map_err(|e| e.to_string())?;
    }
    list_harness_accounts_inner(db, auth, None).await
}

#[tauri::command]
pub async fn set_default_harness_account(
    db_state: State<'_, DbState>,
    auth_state: State<'_, AuthState>,
    account_id: String,
) -> Result<Vec<HarnessAuthAccount>, String> {
    set_default_harness_account_inner(&db_state.0, &auth_state, account_id).await
}

pub async fn delete_harness_account_inner(
    db: &Arc<Mutex<Database>>,
    auth: &AuthState,
    account_id: String,
) -> Result<Vec<HarnessAuthAccount>, String> {
    if matches!(account_id.as_str(), "default" | "kiro_default" | "claude_default") {
        return Err("Primary default accounts cannot be deleted.".to_string());
    }
    {
        let mut procs = auth.login_procs.lock().await;
        if let Some(mut active) = procs.remove(&account_id) {
            let _ = active.child.kill().await;
        }
    }
    {
        let db = db.lock().await;
        db.delete_harness_account(&account_id)
            .map_err(|e| e.to_string())?;
    }
    list_harness_accounts_inner(db, auth, None).await
}

pub async fn rename_harness_account_inner(
    db: &Arc<Mutex<Database>>,
    auth: &AuthState,
    account_id: String,
    label: String,
) -> Result<Vec<HarnessAuthAccount>, String> {
    let trimmed = label.trim().to_string();
    if trimmed.is_empty() {
        return Err("Account name cannot be empty.".to_string());
    }
    {
        let db = db.lock().await;
        let updated = db
            .rename_harness_account(&account_id, &trimmed)
            .map_err(|e| e.to_string())?;
        if !updated {
            return Err(format!("Account '{account_id}' not found"));
        }
    }
    list_harness_accounts_inner(db, auth, None).await
}

#[tauri::command]
pub async fn rename_harness_account(
    db_state: State<'_, DbState>,
    auth_state: State<'_, AuthState>,
    account_id: String,
    label: String,
) -> Result<Vec<HarnessAuthAccount>, String> {
    rename_harness_account_inner(&db_state.0, &auth_state, account_id, label).await
}

#[tauri::command]
pub async fn delete_harness_account(
    db_state: State<'_, DbState>,
    auth_state: State<'_, AuthState>,
    account_id: String,
) -> Result<Vec<HarnessAuthAccount>, String> {
    delete_harness_account_inner(&db_state.0, &auth_state, account_id).await
}

pub async fn start_harness_login_inner(
    db: &Arc<Mutex<Database>>,
    auth: &AuthState,
    account_id: String,
    auth_type: Option<String>,
    mode: Option<String>,
) -> Result<HarnessLoginResponse, String> {
    let mut acc = {
        let db = db.lock().await;
        db.get_harness_account(&account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Account '{account_id}' not found"))?
    };

    if let Some(at) = auth_type.filter(|s| !s.trim().is_empty()) {
        acc.auth_type = at.clone();
        let db = db.lock().await;
        let _ = db.update_harness_account(&account_id, None, None, Some(&at));
    }

    // Terminate any previous login process for this account
    {
        let mut procs = auth.login_procs.lock().await;
        if let Some(mut prev) = procs.remove(&account_id) {
            let _ = prev.child.kill().await;
        }
    }

    match acc.harness.as_str() {
        "agy-cli" => start_agy_login(db, auth, &acc, mode.as_deref()).await,
        "kiro-cli" => start_kiro_login(db, auth, &acc).await,
        "claude-code" => start_claude_login(db, auth, &acc).await,
        other => Err(format!("Unsupported harness '{other}'")),
    }
}

#[tauri::command]
pub async fn start_harness_login(
    db_state: State<'_, DbState>,
    auth_state: State<'_, AuthState>,
    account_id: String,
    auth_type: Option<String>,
    mode: Option<String>,
) -> Result<HarnessLoginResponse, String> {
    start_harness_login_inner(&db_state.0, &auth_state, account_id, auth_type, mode).await
}

async fn start_agy_login(
    db_state: &Arc<Mutex<Database>>,
    auth_state: &AuthState,
    acc: &HarnessAccountRow,
    mode: Option<&str>,
) -> Result<HarnessLoginResponse, String> {
    let clean_mode = if mode.unwrap_or("headless").eq_ignore_ascii_case("loopback") {
        "loopback"
    } else {
        "headless"
    };
    write_agy_settings(acc, &acc.auth_type);

    if acc.auth_type == "gemini-api-key" {
        let has_key = get_env_or_dotenv("GEMINI_API_KEY").is_some();
        let status = if has_key { "authenticated" } else { "unauthenticated" };
        let db = db_state.lock().await;
        let _ = db.update_harness_account(&acc.id, None, Some(status), Some(&acc.auth_type));
        return Ok(HarnessLoginResponse {
            started: true,
            account_id: acc.id.clone(),
            harness: acc.harness.clone(),
            auth_type: acc.auth_type.clone(),
            mode: "env".to_string(),
            authenticated: has_key,
            needs_code: false,
            auth_url: String::new(),
            device_code: String::new(),
            email: acc.email.clone(),
            message: if has_key {
                "Configured Gemini API key auth (GEMINI_API_KEY found)."
            } else {
                "Set GEMINI_API_KEY in Dashboard -> Settings (.env Secrets) to authenticate."
            }
            .to_string(),
        });
    }

    if acc.auth_type == "agent-platform" {
        let has_cfg = is_account_authenticated(acc);
        let status = if has_cfg { "authenticated" } else { "unauthenticated" };
        let db = db_state.lock().await;
        let _ = db.update_harness_account(&acc.id, None, Some(status), Some(&acc.auth_type));
        return Ok(HarnessLoginResponse {
            started: true,
            account_id: acc.id.clone(),
            harness: acc.harness.clone(),
            auth_type: acc.auth_type.clone(),
            mode: "env".to_string(),
            authenticated: has_cfg,
            needs_code: false,
            auth_url: String::new(),
            device_code: String::new(),
            email: acc.email.clone(),
            message: if has_cfg {
                "Configured Agent Platform (Vertex AI) auth."
            } else {
                "Set GOOGLE_API_KEY or GOOGLE_CLOUD_PROJECT in .env Secrets to authenticate."
            }
            .to_string(),
        });
    }

    let Some(bin) = resolve_harness_binary("agy-cli") else {
        return Ok(HarnessLoginResponse {
            started: false,
            account_id: acc.id.clone(),
            harness: acc.harness.clone(),
            auth_type: acc.auth_type.clone(),
            mode: clean_mode.to_string(),
            authenticated: false,
            needs_code: false,
            auth_url: String::new(),
            device_code: String::new(),
            email: acc.email.clone(),
            message: "agy CLI binary was not found on PATH. Install Antigravity CLI first.".to_string(),
        });
    };

    let gdir = agy_gemini_dir(acc);
    let _ = std::fs::create_dir_all(gdir.join(AGY_ACP_APP_DATA_DIR));
    let bin_str = bin.to_string_lossy().to_string();
    let mut cmd = tokio_command(&bin_str);
    apply_account_env(&mut cmd, "agy-cli", Some(acc));
    cmd.args([
        "acp",
        "--gemini-dir",
        &gdir.to_string_lossy(),
        "--app-data-dir",
        AGY_ACP_APP_DATA_DIR,
        "--login",
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    if clean_mode == "headless" {
        cmd.env("SSH_CONNECTION", "127.0.0.1 0 127.0.0.1 0");
        cmd.env_remove("DISPLAY");
        cmd.env_remove("WAYLAND_DISPLAY");
    }

    let mut child = cmd
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn agy login ({}): {}", bin.display(), e))?;

    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let output_buf = spawn_login_output_reader(stdout, stderr);
    let text = wait_for_initial_login_output(&output_buf, Duration::from_secs(6)).await;
    let (auth_url, device_code) = extract_url_and_code(&text);

    auth_state
        .last_outputs
        .lock()
        .await
        .insert(acc.id.clone(), text.clone());
    auth_state
        .login_procs
        .lock()
        .await
        .insert(
            acc.id.clone(),
            ActiveLoginProc {
                child,
                stdin,
                output_buf,
            },
        );

    {
        let db = db_state.lock().await;
        let _ = db.update_harness_account(&acc.id, None, Some("pending_login"), None);
    }

    Ok(HarnessLoginResponse {
        started: true,
        account_id: acc.id.clone(),
        harness: acc.harness.clone(),
        auth_type: acc.auth_type.clone(),
        mode: clean_mode.to_string(),
        authenticated: false,
        needs_code: true,
        auth_url,
        device_code,
        email: acc.email.clone(),
        message: if text.is_empty() {
            format!("AGY {clean_mode} login started for '{}'. Complete sign-in in your browser or paste the auth code below.", acc.label)
        } else {
            text
        },
    })
}

async fn start_kiro_login(
    db_state: &Arc<Mutex<Database>>,
    auth_state: &AuthState,
    acc: &HarnessAccountRow,
) -> Result<HarnessLoginResponse, String> {
    let Some(bin) = resolve_harness_binary("kiro-cli") else {
        return Ok(HarnessLoginResponse {
            started: false,
            account_id: acc.id.clone(),
            harness: acc.harness.clone(),
            auth_type: acc.auth_type.clone(),
            mode: "device-flow".to_string(),
            authenticated: false,
            needs_code: false,
            auth_url: String::new(),
            device_code: String::new(),
            email: acc.email.clone(),
            message: "kiro-cli binary was not found on PATH. Install Kiro CLI first.".to_string(),
        });
    };

    let bin_str = bin.to_string_lossy().to_string();
    let mut cmd = tokio_command(&bin_str);
    apply_account_env(&mut cmd, "kiro-cli", Some(acc));
    cmd.args(["login", "--license", "free", "--use-device-flow"])
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn kiro-cli login ({}): {}", bin.display(), e))?;

    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let output_buf = spawn_login_output_reader(stdout, stderr);
    let text = wait_for_initial_login_output(&output_buf, Duration::from_secs(6)).await;
    let (auth_url, device_code) = extract_url_and_code(&text);

    auth_state
        .last_outputs
        .lock()
        .await
        .insert(acc.id.clone(), text.clone());
    auth_state
        .login_procs
        .lock()
        .await
        .insert(
            acc.id.clone(),
            ActiveLoginProc {
                child,
                stdin,
                output_buf,
            },
        );

    {
        let db = db_state.lock().await;
        let _ = db.update_harness_account(&acc.id, None, Some("pending_login"), None);
    }

    Ok(HarnessLoginResponse {
        started: true,
        account_id: acc.id.clone(),
        harness: acc.harness.clone(),
        auth_type: acc.auth_type.clone(),
        mode: "device-flow".to_string(),
        authenticated: is_account_authenticated(acc),
        needs_code: false,
        auth_url,
        device_code,
        email: acc.email.clone(),
        message: if text.is_empty() {
            format!("Kiro device login started for '{}'. Open the URL in your browser and approve, then click Check Status.", acc.label)
        } else {
            text
        },
    })
}

async fn start_claude_login(
    db_state: &Arc<Mutex<Database>>,
    auth_state: &AuthState,
    acc: &HarnessAccountRow,
) -> Result<HarnessLoginResponse, String> {
    if acc.auth_type == "anthropic-api-key" {
        let has_key = get_env_or_dotenv("ANTHROPIC_API_KEY").is_some();
        let status = if has_key { "authenticated" } else { "unauthenticated" };
        let db = db_state.lock().await;
        let _ = db.update_harness_account(&acc.id, None, Some(status), Some(&acc.auth_type));
        return Ok(HarnessLoginResponse {
            started: true,
            account_id: acc.id.clone(),
            harness: acc.harness.clone(),
            auth_type: acc.auth_type.clone(),
            mode: "env".to_string(),
            authenticated: has_key,
            needs_code: false,
            auth_url: String::new(),
            device_code: String::new(),
            email: acc.email.clone(),
            message: if has_key {
                "Claude Code configured with ANTHROPIC_API_KEY."
            } else {
                "Set ANTHROPIC_API_KEY in Dashboard -> Settings (.env Secrets) to authenticate."
            }
            .to_string(),
        });
    }

    let Some(bin) = resolve_harness_binary("claude-code") else {
        return Ok(HarnessLoginResponse {
            started: false,
            account_id: acc.id.clone(),
            harness: acc.harness.clone(),
            auth_type: acc.auth_type.clone(),
            mode: "claude-oauth".to_string(),
            authenticated: false,
            needs_code: false,
            auth_url: String::new(),
            device_code: String::new(),
            email: acc.email.clone(),
            message: "claude CLI binary was not found on PATH. Install Claude Code CLI first (`npm i -g @anthropic-ai/claude-code`).".to_string(),
        });
    };

    let bin_str = bin.to_string_lossy().to_string();
    let mut cmd = tokio_command(&bin_str);
    apply_account_env(&mut cmd, "claude-code", Some(acc));
    cmd.args(["auth", "login"])
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn claude auth login ({}): {}", bin.display(), e))?;

    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let output_buf = spawn_login_output_reader(stdout, stderr);
    let text = wait_for_initial_login_output(&output_buf, Duration::from_secs(6)).await;
    let (auth_url, device_code) = extract_url_and_code(&text);

    auth_state
        .last_outputs
        .lock()
        .await
        .insert(acc.id.clone(), text.clone());
    auth_state
        .login_procs
        .lock()
        .await
        .insert(
            acc.id.clone(),
            ActiveLoginProc {
                child,
                stdin,
                output_buf,
            },
        );

    {
        let db = db_state.lock().await;
        let _ = db.update_harness_account(&acc.id, None, Some("pending_login"), None);
    }

    Ok(HarnessLoginResponse {
        started: true,
        account_id: acc.id.clone(),
        harness: acc.harness.clone(),
        auth_type: acc.auth_type.clone(),
        mode: "claude-oauth".to_string(),
        authenticated: is_account_authenticated(acc),
        needs_code: false,
        auth_url,
        device_code,
        email: acc.email.clone(),
        message: if text.is_empty() {
            format!("Claude login started for '{}'. Finish sign-in in the browser window that opened, then click Check Status.", acc.label)
        } else {
            text
        },
    })
}

fn spawn_login_output_reader(
    stdout: Option<tokio::process::ChildStdout>,
    stderr: Option<tokio::process::ChildStderr>,
) -> Arc<Mutex<String>> {
    let buf = Arc::new(Mutex::new(String::new()));
    if let Some(out) = stdout {
        let buf_clone = buf.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut guard = buf_clone.lock().await;
                if !guard.is_empty() {
                    guard.push('\n');
                }
                guard.push_str(&line);
            }
        });
    }
    if let Some(err) = stderr {
        let buf_clone = buf.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut guard = buf_clone.lock().await;
                if !guard.is_empty() {
                    guard.push('\n');
                }
                guard.push_str(&line);
            }
        });
    }
    buf
}

async fn wait_for_initial_login_output(
    output_buf: &Arc<Mutex<String>>,
    max_wait: Duration,
) -> String {
    let deadline = tokio::time::Instant::now() + max_wait;
    let mut last_len = 0usize;
    let mut quiet_ticks = 0u32;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
        let snapshot = output_buf.lock().await.clone();
        if snapshot.len() != last_len {
            last_len = snapshot.len();
            quiet_ticks = 0;
        } else if !snapshot.is_empty() {
            quiet_ticks += 1;
            let (url, code) = extract_url_and_code(&snapshot);
            // Wait ~480ms of quiet after a URL/device code appears, or ~1.2s of quiet for other text
            if ((!url.is_empty() || !code.is_empty()) && quiet_ticks >= 4) || quiet_ticks >= 10 {
                return snapshot.trim().to_string();
            }
        }
    }
    output_buf.lock().await.trim().to_string()
}

pub async fn submit_harness_login_code_inner(
    db_state: &Arc<Mutex<Database>>,
    auth_state: &AuthState,
    account_id: String,
    code: String,
) -> Result<HarnessLoginResponse, String> {
    let acc = {
        let db = db_state.lock().await;
        db.get_harness_account(&account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Account '{account_id}' not found"))?
    };

    let raw = code.trim().to_string();
    if raw.is_empty() {
        return Err("Authorization code or callback URL is required.".to_string());
    }

    // Loopback callback forwarding (e.g. http://127.0.0.1:xxxxx/?code=...&state=...)
    if let Ok(parsed) = reqwest::Url::parse(&raw) {
        let is_loopback_http = parsed.scheme() == "http"
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && matches!(parsed.host_str(), Some("127.0.0.1") | Some("localhost"))
            && parsed.port().is_some();
        if is_loopback_http {
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(15))
                .build()
                .map_err(|e| e.to_string())?;
            let resp = client
                .get(parsed)
                .send()
                .await
                .map_err(|e| format!("Loopback callback forward failed: {e}"))?;
        let status_code = resp.status().as_u16();
        tokio::time::sleep(Duration::from_millis(600)).await;
        let authed = is_account_authenticated(&acc);
        if authed {
            let db = db_state.lock().await;
            let _ = db.update_harness_account(&account_id, None, Some("authenticated"), None);
        }
        return Ok(HarnessLoginResponse {
            started: true,
            account_id,
            harness: acc.harness,
            auth_type: acc.auth_type,
            mode: "loopback".to_string(),
            authenticated: authed,
            needs_code: !authed,
            auth_url: String::new(),
            device_code: String::new(),
            email: acc.email,
                message: format!("Forwarded loopback callback (HTTP {status_code})."),
            });
        }
    }

    // Feed code to running login process stdin
    let active_opt = auth_state.login_procs.lock().await.remove(&account_id);
    let Some(mut active) = active_opt else {
        let authed = is_account_authenticated(&acc);
        return Ok(HarnessLoginResponse {
            started: false,
            account_id,
            harness: acc.harness,
            auth_type: acc.auth_type,
            mode: "headless".to_string(),
            authenticated: authed,
            needs_code: false,
            auth_url: String::new(),
            device_code: String::new(),
            email: acc.email,
            message: "No active login process for this account. Click Sign In first.".to_string(),
        });
    };

    let pre_len = active.output_buf.lock().await.len();

    if let Some(mut stdin) = active.stdin.take() {
        let _ = stdin.write_all(format!("{raw}\n").as_bytes()).await;
        let _ = stdin.flush().await;
        drop(stdin);
    }

    let wait_res = tokio::time::timeout(Duration::from_secs(20), active.child.wait()).await;
    let exit_ok = matches!(wait_res, Ok(Ok(status)) if status.success());
    tokio::time::sleep(Duration::from_millis(120)).await;
    let full_output = active.output_buf.lock().await.clone();
    let post_output = if pre_len <= full_output.len() {
        full_output[pre_len..].trim().to_string()
    } else {
        full_output.trim().to_string()
    };
    if !full_output.trim().is_empty() {
        auth_state
            .last_outputs
            .lock()
            .await
            .insert(account_id.clone(), full_output.trim().to_string());
    }

    let authed = exit_ok && is_account_authenticated(&acc);
    let resolved_email = if authed {
        resolve_account_email(&acc)
    } else {
        acc.email.clone()
    };
    if authed {
        let db = db_state.lock().await;
        let _ = db.update_harness_account(
            &account_id,
            if resolved_email.is_empty() {
                None
            } else {
                Some(resolved_email.as_str())
            },
            Some("authenticated"),
            None,
        );
    }

    Ok(HarnessLoginResponse {
        started: true,
        account_id,
        harness: acc.harness,
        auth_type: acc.auth_type,
        mode: "headless".to_string(),
        authenticated: authed,
        needs_code: !authed,
        auth_url: String::new(),
        device_code: String::new(),
        email: resolved_email,
        message: if authed {
            "Signed in successfully.".to_string()
        } else if !post_output.is_empty() {
            post_output
        } else if matches!(wait_res, Err(_)) {
            "Login process timed out waiting for token exchange. Click Sign In to retry.".to_string()
        } else {
            "Login failed. Click Sign In to generate a fresh link and try again.".to_string()
        },
    })
}

#[tauri::command]
pub async fn submit_harness_login_code(
    db_state: State<'_, DbState>,
    auth_state: State<'_, AuthState>,
    account_id: String,
    code: String,
) -> Result<HarnessLoginResponse, String> {
    submit_harness_login_code_inner(&db_state.0, &auth_state, account_id, code).await
}

pub async fn check_harness_auth_inner(
    db_state: &Arc<Mutex<Database>>,
    auth_state: &AuthState,
    account_id: String,
) -> Result<HarnessAuthAccount, String> {
    let mut acc = {
        let db = db_state.lock().await;
        db.get_harness_account(&account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Account '{account_id}' not found"))?
    };

    let mut live_authed = is_account_authenticated(&acc);
    let mut detected_email = acc.email.clone();

    match acc.harness.as_str() {
        "agy-cli" => {
            if detected_email.is_empty() {
                if let Some(em) = extract_email_from_agy_files(&acc) {
                    detected_email = em;
                }
            }
        }
        "kiro-cli" => {
            if let Some(bin) = resolve_harness_binary("kiro-cli") {
                let bin_str = bin.to_string_lossy().to_string();
                let mut cmd = tokio_command(&bin_str);
                apply_account_env(&mut cmd, "kiro-cli", Some(&acc));
                cmd.args(["whoami", "--format", "json"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                if let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(5), cmd.output()).await {
                    if out.status.success() {
                        live_authed = true;
                        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                            if let Some(email) = val.get("email").and_then(|v| v.as_str()) {
                                if !email.trim().is_empty() {
                                    detected_email = email.trim().to_string();
                                }
                            }
                        }
                    }
                }
            }
        }
        "claude-code" => {
            if let Some(bin) = resolve_harness_binary("claude-code") {
                let bin_str = bin.to_string_lossy().to_string();
                let mut cmd = tokio_command(&bin_str);
                apply_account_env(&mut cmd, "claude-code", Some(&acc));
                cmd.args(["auth", "status"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                if let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(5), cmd.output()).await {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    if out.status.success() {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&stdout) {
                            if val.get("loggedIn").and_then(|v| v.as_bool()).unwrap_or(false) {
                                live_authed = true;
                            }
                            if let Some(email) = val.get("email").and_then(|v| v.as_str()) {
                                if !email.trim().is_empty() {
                                    detected_email = email.trim().to_string();
                                }
                            }
                        } else if stdout.to_ascii_lowercase().contains("logged in") {
                            live_authed = true;
                        }
                    }
                }
            }
            if detected_email.is_empty() {
                if let Some(em) = extract_email_from_claude_json(&acc) {
                    detected_email = em;
                }
            }
        }
        _ => {}
    }

    let new_status = if live_authed {
        "authenticated"
    } else {
        "unauthenticated"
    };
    acc.status = new_status.to_string();
    acc.email = detected_email.clone();

    {
        let db = db_state.lock().await;
        let _ = db.update_harness_account(
            &account_id,
            if detected_email.is_empty() { None } else { Some(detected_email.as_str()) },
            Some(new_status),
            None,
        );
    }

    let outputs = auth_state.last_outputs.lock().await;
    Ok(to_auth_account(acc, &outputs))
}

#[tauri::command]
pub async fn check_harness_auth(
    db_state: State<'_, DbState>,
    auth_state: State<'_, AuthState>,
    account_id: String,
) -> Result<HarnessAuthAccount, String> {
    check_harness_auth_inner(&db_state.0, &auth_state, account_id).await
}

pub async fn logout_harness_account_inner(
    db_state: &Arc<Mutex<Database>>,
    auth_state: &AuthState,
    account_id: String,
) -> Result<Vec<HarnessAuthAccount>, String> {
    let acc = {
        let db = db_state.lock().await;
        db.get_harness_account(&account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Account '{account_id}' not found"))?
    };

    {
        let mut procs = auth_state.login_procs.lock().await;
        if let Some(mut active) = procs.remove(&account_id) {
            let _ = active.child.kill().await;
        }
    }

    match acc.harness.as_str() {
        "agy-cli" => {
            let gdir = agy_gemini_dir(&acc);
            for dir in [gdir.join(AGY_ACP_APP_DATA_DIR), gdir.join("antigravity-acp"), gdir.clone()] {
                for fname in ["acp_token.json", "antigravity-oauth-token", "oauth_creds.json"] {
                    let p = dir.join(fname);
                    if p.is_file() {
                        let _ = std::fs::remove_file(p);
                    }
                }
            }
            let reg = gdir.join(AGY_ACP_APP_DATA_DIR).join("accounts").join("registry.json");
            if reg.is_file() {
                let _ = std::fs::remove_file(reg);
            }
        }
        "kiro-cli" => {
            if let Some(bin) = resolve_harness_binary("kiro-cli") {
                let bin_str = bin.to_string_lossy().to_string();
                let mut cmd = tokio_command(&bin_str);
                apply_account_env(&mut cmd, "kiro-cli", Some(&acc));
                cmd.args(["logout"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                let _ = tokio::time::timeout(Duration::from_secs(5), cmd.output()).await;
            }
        }
        "claude-code" => {
            if let Some(bin) = resolve_harness_binary("claude-code") {
                let bin_str = bin.to_string_lossy().to_string();
                let mut cmd = tokio_command(&bin_str);
                apply_account_env(&mut cmd, "claude-code", Some(&acc));
                cmd.args(["auth", "logout"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                let _ = tokio::time::timeout(Duration::from_secs(5), cmd.output()).await;
            }
            let cred = claude_config_dir(&acc).join(".credentials.json");
            if cred.is_file() {
                let _ = std::fs::remove_file(cred);
            }
        }
        _ => {}
    }

    {
        let db = db_state.lock().await;
        let _ = db.update_harness_account(&account_id, None, Some("unauthenticated"), None);
    }
    auth_state.last_outputs.lock().await.remove(&account_id);
    list_harness_accounts_inner(db_state, auth_state, None).await
}

#[tauri::command]
pub async fn logout_harness_account(
    db_state: State<'_, DbState>,
    auth_state: State<'_, AuthState>,
    account_id: String,
) -> Result<Vec<HarnessAuthAccount>, String> {
    logout_harness_account_inner(&db_state.0, &auth_state, account_id).await
}

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("Only http(s) URLs are allowed.".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        // Use rundll32 FileProtocolHandler instead of `cmd /C start` so '&' in OAuth query strings is never truncated by cmd.exe
        let mut cmd = tokio::process::Command::new("rundll32");
        cmd.args(["url.dll,FileProtocolHandler", trimmed]);
        cmd.creation_flags(0x0800_0000);
        cmd.spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        tokio::process::Command::new("open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        tokio::process::Command::new("xdg-open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
