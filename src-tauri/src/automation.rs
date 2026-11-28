use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;

use tauri::{AppHandle, Emitter, State};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::db::Database;
use crate::harness::resolve_harness_binary;
use crate::models::AutomationItem;
use crate::DbState;

pub struct AutomationState {
    scheduler: Arc<Mutex<Option<JobScheduler>>>,
}

impl Default for AutomationState {
    fn default() -> Self {
        Self {
            scheduler: Arc::new(Mutex::new(None)),
        }
    }
}

fn normalize_cron_expr(expr: &str) -> String {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() == 5 {
        format!("0 {}", parts.join(" "))
    } else {
        expr.trim().to_string()
    }
}

pub async fn sync_scheduler(
    auto_state: &AutomationState,
    db: Arc<Mutex<Database>>,
    app: AppHandle,
) -> Result<(), String> {
    let mut guard = auto_state.scheduler.lock().await;
    if let Some(mut old_sched) = guard.take() {
        let _ = old_sched.shutdown().await;
    }

    let sched = JobScheduler::new()
        .await
        .map_err(|e| format!("Failed to create cron scheduler: {e}"))?;

    let automations = {
        let db_lock = db.lock().await;
        db_lock.list_automations().map_err(|e| e.to_string())?
    };

    for auto in automations {
        if !auto.enabled || auto.kind != "cron" {
            continue;
        }
        let expr = normalize_cron_expr(&auto.schedule);
        let auto_id = auto.id.clone();
        let db_clone = Arc::clone(&db);
        let app_clone = app.clone();

        let job_res = Job::new_async_tz(expr.as_str(), chrono::Local, move |_uuid, _lock| {
            let id = auto_id.clone();
            let db_inner = Arc::clone(&db_clone);
            let app_inner = app_clone.clone();
            Box::pin(async move {
                let _ = execute_automation(&db_inner, Some(&app_inner), &id).await;
            })
        });
        if let Ok(job) = job_res {
            let _ = sched.add(job).await;
        }
    }

    sched
        .start()
        .await
        .map_err(|e| format!("Failed to start cron scheduler: {e}"))?;
    *guard = Some(sched);
    Ok(())
}

async fn load_automation_context(
    db: &Arc<Mutex<Database>>,
) -> (
    Option<String>,
    Option<crate::models::HarnessAccountRow>,
    Option<crate::models::HarnessAccountRow>,
) {
    let db_lock = db.lock().await;
    let cwd = db_lock
        .list_projects()
        .ok()
        .and_then(|projects| projects.into_iter().next())
        .map(|p| p.cwd);
    let claude_acc = db_lock
        .get_default_harness_account("claude-code")
        .ok()
        .flatten();
    let agy_acc = db_lock
        .get_default_harness_account("agy-cli")
        .ok()
        .flatten();
    (cwd, claude_acc, agy_acc)
}

pub async fn execute_automation(
    db: &Arc<Mutex<Database>>,
    app: Option<&AppHandle>,
    id: &str,
) -> Result<AutomationItem, String> {
    let auto = {
        let db_lock = db.lock().await;
        db_lock
            .get_automation(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Automation '{id}' not found"))?
    };

    let (cwd, claude_acc, agy_acc) = load_automation_context(db).await;
    let start = Instant::now();
    let (status, summary) = run_automation_prompt(
        db,
        app,
        &auto.name,
        &auto.prompt,
        None,
        cwd.as_deref(),
        claude_acc.as_ref(),
        agy_acc.as_ref(),
    )
    .await;
    let duration_ms = start.elapsed().as_millis().max(1) as i64;

    let updated = {
        let db_lock = db.lock().await;
        db_lock
            .record_automation_run(id, &status, duration_ms, &summary)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Automation '{id}' not found after run"))?
    };

    if let Some(app_handle) = app {
        let _ = app_handle.emit("automation-run", &updated);
    }

    Ok(updated)
}

async fn run_cli_harness_once(
    db: &Arc<Mutex<Database>>,
    name: &str,
    prompt: &str,
    project_cwd: Option<&str>,
    claude_acc: Option<&crate::models::HarnessAccountRow>,
    agy_acc: Option<&crate::models::HarnessAccountRow>,
) -> Result<String, String> {
    let resolved = resolve_harness_binary("claude-code")
        .map(|bin| ("claude-code", bin, claude_acc))
        .or_else(|| resolve_harness_binary("agy-cli").map(|bin| ("agy-cli", bin, agy_acc)))
        .or_else(|| resolve_harness_binary("kiro-cli").map(|bin| ("kiro-cli", bin, None)));

    let Some((harness_id, bin, acc)) = resolved else {
        return Err(format!(
            "No CLI harness (claude/agy/kiro) found on PATH to run '{}'",
            name
        ));
    };

    let enabled_mcp_servers: Vec<crate::models::McpServer> = {
        let db_lock = db.lock().await;
        db_lock
            .list_mcp_servers()
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.enabled)
            .collect()
    };
    crate::auth::sync_profile_mcp_and_skills(harness_id, acc, &enabled_mcp_servers);

    let bin_str = bin.to_string_lossy().to_string();
    let mut cmd = crate::platform::tokio_command(&bin_str);
    if let Some(cwd) = project_cwd.filter(|c| std::path::Path::new(c).is_dir()) {
        cmd.current_dir(cwd);
    }
    crate::auth::apply_account_env(&mut cmd, harness_id, acc);
    if harness_id == "kiro-cli" {
        cmd.args(["chat", "--no-interactive", prompt]);
    } else {
        cmd.args(["-p", prompt]);
        cmd.args(crate::harness::build_permission_args(harness_id, "yolo"));
    }
    cmd.kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child_fut = cmd.output();

    match tokio::time::timeout(std::time::Duration::from_secs(300), child_fut).await {
        Ok(Ok(out)) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if text.is_empty() {
                Ok(format!("Completed '{}' via {}", name, bin.display()))
            } else {
                Ok(text)
            }
        }
        Ok(Ok(out)) => {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Err(if err.is_empty() {
                format!("Harness exited with {}", out.status)
            } else {
                err.chars().take(240).collect()
            })
        }
        Ok(Err(e)) => Err(format!("Failed to spawn {}: {e}", bin.display())),
        Err(_) => Err(format!("Automation '{}' timed out after 300s", name)),
    }
}

async fn send_discord_webhook(url: &str, message: &str) -> Result<(), String> {
    let webhook_url = url.trim();
    if !webhook_url.starts_with("http://") && !webhook_url.starts_with("https://") {
        return Err("Invalid Discord webhook URL (must start with http:// or https://)".to_string());
    }
    let content: String = message.trim().chars().take(1950).collect();
    if content.is_empty() {
        return Err("Discord webhook message is empty".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(webhook_url)
        .json(&serde_json::json!({ "content": content }))
        .send()
        .await
        .map_err(|e| format!("Discord webhook request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Discord webhook HTTP {status}: {body}"));
    }
    Ok(())
}

async fn send_discord_bot_channel(
    db: &Arc<Mutex<Database>>,
    channel_id: &str,
    message: &str,
) -> Result<(), String> {
    let cid = channel_id.trim();
    if cid.is_empty() {
        return Err("Discord channel ID is empty".to_string());
    }
    let token = {
        let db_lock = db.lock().await;
        db_lock
            .get_setting("discord_bot_token")
            .map_err(|e| e.to_string())?
            .filter(|t| !t.trim().is_empty())
            .or_else(|| crate::platform::get_env_or_dotenv("DISCORD_BOT_TOKEN"))
            .unwrap_or_default()
    };
    if token.trim().is_empty() {
        return Err("Discord bot token is not configured in Discord Bridge".to_string());
    }
    let content: String = message.trim().chars().take(1950).collect();
    if content.is_empty() {
        return Err("Discord channel message is empty".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("https://discord.com/api/v10/channels/{cid}/messages");
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bot {}", token.trim()))
        .json(&serde_json::json!({ "content": content }))
        .send()
        .await
        .map_err(|e| format!("Discord API request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Discord API HTTP {status}: {body}"));
    }
    Ok(())
}

async fn run_automation_prompt(
    db: &Arc<Mutex<Database>>,
    app: Option<&AppHandle>,
    name: &str,
    prompt: &str,
    webhook_payload: Option<&str>,
    project_cwd: Option<&str>,
    claude_acc: Option<&crate::models::HarnessAccountRow>,
    agy_acc: Option<&crate::models::HarnessAccountRow>,
) -> (String, String) {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return (
            "ERR".to_string(),
            "Automation prompt is empty".to_string(),
        );
    }

    // 1. Direct Discord Webhook: discord:webhook:<url>|<message>
    if let Some(rest) = trimmed.strip_prefix("discord:webhook:") {
        let Some((url, msg)) = rest.split_once('|') else {
            return ("ERR".to_string(), "Expected format: discord:webhook:<url>|<message>".to_string());
        };
        return match send_discord_webhook(url, msg.trim()).await {
            Ok(()) => (
                "OK".to_string(),
                format!("Sent Discord webhook message: {}", msg.trim().chars().take(160).collect::<String>()),
            ),
            Err(e) => ("ERR".to_string(), e),
        };
    }

    // 2. Direct Discord Bot Channel: discord:channel:<channel_id>|<message>
    if let Some(rest) = trimmed.strip_prefix("discord:channel:") {
        let Some((channel_id, msg)) = rest.split_once('|') else {
            return ("ERR".to_string(), "Expected format: discord:channel:<channel_id>|<message>".to_string());
        };
        return match send_discord_bot_channel(db, channel_id, msg.trim()).await {
            Ok(()) => (
                "OK".to_string(),
                format!("Sent Discord message to #{}: {}", channel_id.trim(), msg.trim().chars().take(150).collect::<String>()),
            ),
            Err(e) => ("ERR".to_string(), e),
        };
    }

    // 3. AI-generated Discord Webhook: discord-ai:webhook:<url>|<ai prompt>
    if let Some(rest) = trimmed.strip_prefix("discord-ai:webhook:") {
        let Some((url, ai_prompt)) = rest.split_once('|') else {
            return ("ERR".to_string(), "Expected format: discord-ai:webhook:<url>|<prompt>".to_string());
        };
        return match run_cli_harness_once(db, name, ai_prompt.trim(), project_cwd, claude_acc, agy_acc).await {
            Ok(generated) => match send_discord_webhook(url, &generated).await {
                Ok(()) => (
                    "OK".to_string(),
                    format!("AI -> Discord webhook: {}", generated.chars().take(180).collect::<String>()),
                ),
                Err(e) => ("ERR".to_string(), e),
            },
            Err(e) => ("ERR".to_string(), e),
        };
    }

    // 4. AI-generated Discord Bot Channel: discord-ai:channel:<channel_id>|<ai prompt>
    if let Some(rest) = trimmed.strip_prefix("discord-ai:channel:") {
        let Some((channel_id, ai_prompt)) = rest.split_once('|') else {
            return ("ERR".to_string(), "Expected format: discord-ai:channel:<channel_id>|<prompt>".to_string());
        };
        return match run_cli_harness_once(db, name, ai_prompt.trim(), project_cwd, claude_acc, agy_acc).await {
            Ok(generated) => match send_discord_bot_channel(db, channel_id, &generated).await {
                Ok(()) => (
                    "OK".to_string(),
                    format!("AI -> Discord #{}: {}", channel_id.trim(), generated.chars().take(160).collect::<String>()),
                ),
                Err(e) => ("ERR".to_string(), e),
            },
            Err(e) => ("ERR".to_string(), e),
        };
    }

    // 5. Desktop Notification: notify:<title>|<message>
    if let Some(rest) = trimmed.strip_prefix("notify:") {
        let (title, body) = match rest.split_once('|') {
            Some((t, b)) => (t.trim(), b.trim()),
            None => (name, rest.trim()),
        };
        crate::platform::emit_app_and_remote(
            app,
            "sakana:notify",
            serde_json::json!({ "title": title, "body": body }),
        );
        return ("OK".to_string(), format!("Notification: {} — {}", title, body));
    }

    // If prompt is an explicit shell command prefixed with `shell:` or `cmd:`
    if let Some(cmd_str) = trimmed
        .strip_prefix("shell:")
        .or_else(|| trimmed.strip_prefix("cmd:"))
    {
        let cmd_str = cmd_str.trim();
        #[cfg(target_os = "windows")]
        let mut cmd = {
            let mut c = Command::new("cmd");
            c.args(["/C", cmd_str]);
            c.creation_flags(0x0800_0000);
            c
        };
        #[cfg(not(target_os = "windows"))]
        let mut cmd = {
            let mut c = Command::new("sh");
            c.args(["-c", cmd_str]);
            c
        };
        if let Some(cwd) = project_cwd.filter(|c| std::path::Path::new(c).is_dir()) {
            cmd.current_dir(cwd);
        }
        if let Some(body) = webhook_payload {
            cmd.env("SAKANA_WEBHOOK_PAYLOAD", body);
        }
        cmd.kill_on_drop(true).stdin(Stdio::null());
        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(120),
            cmd.output(),
        )
        .await
        {
            Ok(res) => res,
            Err(_) => return ("ERR".to_string(), "Shell automation timed out after 120s".to_string()),
        };

        return match output {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                if out.status.success() {
                    let summary = if text.is_empty() {
                        format!("Command '{}' succeeded", cmd_str)
                    } else {
                        text.chars().take(240).collect()
                    };
                    ("OK".to_string(), summary)
                } else {
                    let summary = if err.is_empty() {
                        format!("Command exited with {}", out.status)
                    } else {
                        err.chars().take(240).collect()
                    };
                    ("ERR".to_string(), summary)
                }
            }
            Err(e) => ("ERR".to_string(), format!("Failed to spawn shell: {e}")),
        };
    }

    match run_cli_harness_once(db, name, trimmed, project_cwd, claude_acc, agy_acc).await {
        Ok(text) => ("OK".to_string(), text.chars().take(240).collect()),
        Err(err) => ("ERR".to_string(), err),
    }
}

pub async fn trigger_webhook_by_path(
    db: Arc<Mutex<Database>>,
    app: Option<AppHandle>,
    path: &str,
    payload: Option<String>,
) -> Result<Vec<String>, String> {
    let normalized = path.trim().trim_start_matches('/').to_string();
    let matching: Vec<AutomationItem> = {
        let db_lock = db.lock().await;
        db_lock
            .list_automations()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|a| {
                a.enabled
                    && a.kind == "webhook"
                    && a.schedule.trim().trim_start_matches('/') == normalized
            })
            .collect()
    };

    if matching.is_empty() {
        return Err(format!("No enabled webhook automation matches '/{}'", normalized));
    }

    let (cwd, claude_acc, agy_acc) = load_automation_context(&db).await;
    let mut triggered = Vec::new();
    for auto in matching {
        let trimmed_auto = auto.prompt.trim().to_ascii_lowercase();
        let is_shell = trimmed_auto.starts_with("shell:") || trimmed_auto.starts_with("cmd:");
        let webhook_body = payload
            .as_deref()
            .filter(|body| !body.trim().is_empty());
        // Never concatenate untrusted webhook payloads onto shell:/cmd: commands;
        // pass them safely via SAKANA_WEBHOOK_PAYLOAD env var instead.
        let prompt = if is_shell {
            auto.prompt.clone()
        } else if let Some(body) = webhook_body {
            format!("{}\n\nWebhook payload:\n{}", auto.prompt, body)
        } else {
            auto.prompt.clone()
        };
        let start = Instant::now();
        let (status, summary) = run_automation_prompt(
            &db,
            app.as_ref(),
            &auto.name,
            &prompt,
            if is_shell { webhook_body } else { None },
            cwd.as_deref(),
            claude_acc.as_ref(),
            agy_acc.as_ref(),
        )
        .await;
        let duration_ms = start.elapsed().as_millis().max(1) as i64;
        let updated = {
            let db_lock = db.lock().await;
            db_lock
                .record_automation_run(&auto.id, &status, duration_ms, &summary)
                .map_err(|e| e.to_string())?
        };
        if let Some(item) = updated {
            crate::platform::emit_app_and_remote(app.as_ref(), "automation-run", &item);
        }
        triggered.push(auto.id);
    }
    Ok(triggered)
}

#[tauri::command]
pub async fn list_automations(state: State<'_, DbState>) -> Result<Vec<AutomationItem>, String> {
    let db = state.0.lock().await;
    db.list_automations().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_automation(
    state: State<'_, DbState>,
    auto_state: State<'_, AutomationState>,
    app: AppHandle,
    name: String,
    kind: String,
    schedule: String,
    prompt: String,
) -> Result<AutomationItem, String> {
    let name = name.trim().to_string();
    let kind = if kind.trim().eq_ignore_ascii_case("webhook") {
        "webhook".to_string()
    } else {
        "cron".to_string()
    };
    let schedule = schedule.trim().to_string();
    let prompt = prompt.trim().to_string();

    if name.is_empty() || schedule.is_empty() || prompt.is_empty() {
        return Err("Name, schedule, and prompt are required".to_string());
    }

    let item = {
        let db = state.0.lock().await;
        db.create_automation(&name, &kind, &schedule, &prompt)
            .map_err(|e| e.to_string())?
    };

    let _ = sync_scheduler(&auto_state, Arc::clone(&state.0), app).await;
    Ok(item)
}

#[tauri::command]
pub async fn toggle_automation(
    state: State<'_, DbState>,
    auto_state: State<'_, AutomationState>,
    app: AppHandle,
    id: String,
    enabled: bool,
) -> Result<Option<AutomationItem>, String> {
    let updated = {
        let db = state.0.lock().await;
        db.toggle_automation(&id, enabled)
            .map_err(|e| e.to_string())?
    };

    let _ = sync_scheduler(&auto_state, Arc::clone(&state.0), app).await;
    Ok(updated)
}

#[tauri::command]
pub async fn delete_automation(
    state: State<'_, DbState>,
    auto_state: State<'_, AutomationState>,
    app: AppHandle,
    id: String,
) -> Result<bool, String> {
    let deleted = {
        let db = state.0.lock().await;
        db.delete_automation(&id).map_err(|e| e.to_string())?
    };

    if deleted {
        let _ = sync_scheduler(&auto_state, Arc::clone(&state.0), app).await;
    }
    Ok(deleted)
}

#[tauri::command]
pub async fn run_automation_now(
    state: State<'_, DbState>,
    app: AppHandle,
    id: String,
) -> Result<AutomationItem, String> {
    execute_automation(&state.0, Some(&app), &id).await
}
