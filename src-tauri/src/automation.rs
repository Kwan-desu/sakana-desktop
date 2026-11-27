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

        if let Ok(job) = Job::new_async(expr.as_str(), move |_uuid, _lock| {
            let id = auto_id.clone();
            let db_inner = Arc::clone(&db_clone);
            let app_inner = app_clone.clone();
            Box::pin(async move {
                let _ = execute_automation(&db_inner, Some(&app_inner), &id).await;
            })
        }) {
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

async fn execute_automation(
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
        &auto.name,
        &auto.prompt,
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

async fn run_automation_prompt(
    name: &str,
    prompt: &str,
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

    // If prompt is an explicit shell command prefixed with `shell:` or `cmd:`
    if let Some(cmd_str) = trimmed
        .strip_prefix("shell:")
        .or_else(|| trimmed.strip_prefix("cmd:"))
    {
        let cmd_str = cmd_str.trim();
        #[cfg(target_os = "windows")]
        let output = {
            let mut cmd = Command::new("cmd");
            if let Some(cwd) = project_cwd.filter(|c| std::path::Path::new(c).is_dir()) {
                cmd.current_dir(cwd);
            }
            cmd.args(["/C", cmd_str]).stdin(Stdio::null());
            cmd.creation_flags(0x0800_0000);
            cmd.output().await
        };
        #[cfg(not(target_os = "windows"))]
        let output = {
            let mut cmd = Command::new("sh");
            if let Some(cwd) = project_cwd.filter(|c| std::path::Path::new(c).is_dir()) {
                cmd.current_dir(cwd);
            }
            cmd.args(["-c", cmd_str])
                .stdin(Stdio::null())
                .output()
                .await
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

    // Try running via an available CLI harness (`claude` or `agy`) with a bounded timeout
    let resolved = resolve_harness_binary("claude-code")
        .map(|bin| ("claude-code", bin, claude_acc))
        .or_else(|| resolve_harness_binary("agy-cli").map(|bin| ("agy-cli", bin, agy_acc)));

    if let Some((harness_id, bin, acc)) = resolved {
        let bin_str = bin.to_string_lossy().to_string();
        let mut cmd = crate::platform::tokio_command(&bin_str);
        if let Some(cwd) = project_cwd.filter(|c| std::path::Path::new(c).is_dir()) {
            cmd.current_dir(cwd);
        }
        crate::auth::apply_account_env(&mut cmd, harness_id, acc);
        cmd.args(["-p", trimmed])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child_fut = cmd.output();

        if let Ok(Ok(out)) = tokio::time::timeout(std::time::Duration::from_secs(45), child_fut).await {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let summary = if text.is_empty() {
                    format!("Completed '{}' via {}", name, bin.display())
                } else {
                    text.chars().take(240).collect()
                };
                return ("OK".to_string(), summary);
            } else {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                return (
                    "ERR".to_string(),
                    if err.is_empty() {
                        format!("Harness exited with {}", out.status)
                    } else {
                        err.chars().take(240).collect()
                    },
                );
            }
        }
    }

    (
        "ERR".to_string(),
        format!(
            "No CLI harness (claude/agy) found on PATH to run '{}'",
            name
        ),
    )
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
        let prompt = match &payload {
            Some(body) if !body.trim().is_empty() => {
                format!("{}\n\nWebhook payload:\n{}", auto.prompt, body)
            }
            _ => auto.prompt.clone(),
        };
        let start = Instant::now();
        let (status, summary) = run_automation_prompt(
            &auto.name,
            &prompt,
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
