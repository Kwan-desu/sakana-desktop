use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Serialize;
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, RefreshKind, System};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::Mutex;
use uuid::Uuid;

struct TelemetrySampler {
    sys: System,
    last_cpu_sample: Option<Instant>,
}

static TELEMETRY_SAMPLER: OnceLock<Mutex<TelemetrySampler>> = OnceLock::new();

use crate::models::{EnvSecret, QuotaAccount, SystemTelemetry};
use crate::platform::{read_env_entries, write_env_entries};
use crate::DbState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub available: bool,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub body: Option<String>,
    pub error: Option<String>,
}

fn mask_secret_value(val: &str) -> (String, String) {
    let len = val.chars().count();
    if len <= 4 {
        return ("••••".to_string(), format!("{len} chars"));
    }
    let prefix: String = val.chars().take(3).collect();
    let suffix: String = val
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    (format!("{prefix}••••{suffix}"), format!("{len} chars"))
}

#[tauri::command]
pub async fn get_system_telemetry(app: AppHandle) -> Result<SystemTelemetry, String> {
    let sampler = TELEMETRY_SAMPLER.get_or_init(|| {
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        Mutex::new(TelemetrySampler {
            sys,
            last_cpu_sample: None,
        })
    });

    let (cpu_brand, cpu_cores, cpu_usage_pct, memory_used_bytes, memory_total_bytes) = {
        let mut guard = sampler.lock().await;
        let needs_baseline = match guard.last_cpu_sample {
            None => true,
            Some(prev) => prev.elapsed() > Duration::from_secs(5),
        };
        if needs_baseline {
            guard.sys.refresh_cpu_usage();
            tokio::time::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL).await;
        } else if let Some(prev) = guard.last_cpu_sample {
            let min_interval = sysinfo::MINIMUM_CPU_UPDATE_INTERVAL;
            if prev.elapsed() < min_interval {
                tokio::time::sleep(min_interval - prev.elapsed()).await;
            }
        }
        guard.sys.refresh_cpu_usage();
        guard.sys.refresh_memory();
        guard.last_cpu_sample = Some(Instant::now());

        let brand = guard
            .sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .unwrap_or_else(|| "Unknown CPU".to_string());
        (
            brand,
            guard.sys.cpus().len(),
            guard.sys.global_cpu_usage(),
            guard.sys.used_memory(),
            guard.sys.total_memory(),
        )
    };

    let disks = Disks::new_with_refreshed_list();
    let mut seen_mounts: HashSet<PathBuf> = HashSet::new();
    let mut disk_total: u64 = 0;
    let mut disk_avail: u64 = 0;
    for disk in disks.list() {
        if disk.is_removable() || disk.total_space() == 0 {
            continue;
        }
        if seen_mounts.insert(disk.mount_point().to_path_buf()) {
            disk_total = disk_total.saturating_add(disk.total_space());
            disk_avail = disk_avail.saturating_add(disk.available_space());
        }
    }

    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);

    Ok(SystemTelemetry {
        os_name: System::name().unwrap_or_else(|| std::env::consts::OS.to_string()),
        os_version: System::os_version().unwrap_or_else(|| "unknown".to_string()),
        host_name: System::host_name().unwrap_or_else(|| "localhost".to_string()),
        cpu_brand,
        cpu_cores,
        cpu_usage_pct,
        memory_used_bytes,
        memory_total_bytes,
        disk_used_bytes: disk_total.saturating_sub(disk_avail),
        disk_total_bytes: disk_total,
        uptime_secs: System::uptime(),
        autostart_enabled,
    })
}

fn read_claude_recent_tokens_7d() -> i64 {
    let Some(home) = dirs::home_dir() else {
        return 0;
    };
    let stats_path = home.join(".claude").join("stats-cache.json");
    let Ok(content) = std::fs::read_to_string(stats_path) else {
        return 0;
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
        return 0;
    };
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(7))
        .format("%Y-%m-%d")
        .to_string();
    let mut total: i64 = 0;
    if let Some(days) = val.get("dailyModelTokens").and_then(|v| v.as_array()) {
        for day in days {
            let date = day.get("date").and_then(|v| v.as_str()).unwrap_or("");
            if date >= cutoff.as_str() {
                if let Some(map) = day.get("tokensByModel").and_then(|v| v.as_object()) {
                    for v in map.values() {
                        total += v.as_i64().unwrap_or(0);
                    }
                }
            }
        }
    }
    total
}

async fn fetch_agy_live_quota(acc: &crate::models::HarnessAccountRow) -> Option<(f64, String)> {
    let (access_token, _) = crate::auth::read_agy_file_token(acc)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
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
    let lca_val: serde_json::Value = lca_resp.json().await.ok()?;
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
    let val: serde_json::Value = models_resp.json().await.ok()?;
    let models_obj = val.get("models").and_then(|v| v.as_object())?;

    let mut active_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(sorts) = val.get("agentModelSorts").and_then(|v| v.as_array()) {
        for sort in sorts {
            if let Some(groups) = sort.get("groups").and_then(|v| v.as_array()) {
                for g in groups {
                    if let Some(ids) = g.get("modelIds").and_then(|v| v.as_array()) {
                        for id_val in ids {
                            if let Some(s) = id_val.as_str() {
                                if !s.is_empty() && !s.starts_with("tab_") {
                                    active_ids.insert(s.to_string());
                                }
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
                    if !s.is_empty() && !s.starts_with("tab_") {
                        active_ids.insert(s.to_string());
                    }
                }
            }
        } else if let Some(obj) = tiered.as_object() {
            for arr_val in obj.values() {
                if let Some(arr) = arr_val.as_array() {
                    for id_val in arr {
                        if let Some(s) = id_val.as_str() {
                            if !s.is_empty() && !s.starts_with("tab_") {
                                active_ids.insert(s.to_string());
                            }
                        }
                    }
                } else if let Some(s) = arr_val.as_str() {
                    if !s.is_empty() && !s.starts_with("tab_") {
                        active_ids.insert(s.to_string());
                    }
                }
            }
        }
    }

    let mut min_rem: Option<f64> = None;
    let mut next_reset: Option<String> = None;
    let now = chrono::Utc::now();

    for id in &active_ids {
        let Some(meta) = models_obj.get(id) else {
            continue;
        };
        if let Some(qi) = meta.get("quotaInfo") {
            let reset_str = qi
                .get("resetTime")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());
            let rem_opt = qi
                .get("remainingFraction")
                .and_then(|v| v.as_f64())
                .or_else(|| if reset_str.is_some() { Some(0.0) } else { None });
            if let Some(rem) = rem_opt {
                if min_rem.map(|m| rem < m).unwrap_or(true) {
                    min_rem = Some(rem);
                    if let Some(ts) = reset_str {
                        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
                            let mins = (dt.with_timezone(&chrono::Utc) - now).num_minutes().max(0);
                            let h = mins / 60;
                            let m = mins % 60;
                            next_reset = Some(if h > 0 { format!("{h}h {m}m") } else { format!("{m}m") });
                        }
                    }
                }
            }
        }
    }

    let rem = min_rem?;
    let used_pct = (((1.0 - rem) * 100.0).clamp(0.0, 100.0) * 10.0).round() / 10.0;
    Some((used_pct, next_reset.unwrap_or_else(|| "5h".to_string())))
}

#[tauri::command]
pub async fn list_quota_accounts(state: State<'_, DbState>) -> Result<Vec<QuotaAccount>, String> {
    let (mut accounts, harness_rows, db_windows) = {
        let db = state.0.lock().await;
        let mut accounts = db.list_accounts().map_err(|e| e.to_string())?;
        let harness_rows = db.list_harness_accounts(None).unwrap_or_default();

        for row in &harness_rows {
            let authed = crate::auth::is_account_authenticated(row);
            if !authed {
                continue;
            }
            let email = crate::auth::resolve_account_email(row);
            let display_email = if !email.is_empty() {
                email
            } else if row.auth_type.contains("api-key") {
                "API Key".to_string()
            } else {
                "Authenticated".to_string()
            };

            if let Some(existing) = accounts.iter_mut().find(|q| {
                q.harness == row.harness
                    && (q.id == row.id || q.label.eq_ignore_ascii_case(&row.label))
            }) {
                if existing.email.trim().is_empty() || existing.email == "Authenticated" {
                    existing.email = display_email;
                }
                existing.is_default = Some(row.is_default);
            } else {
                accounts.push(QuotaAccount {
                    id: row.id.clone(),
                    harness: row.harness.clone(),
                    label: row.label.clone(),
                    email: display_email,
                    five_hour_pct: 0.0,
                    five_hour_reset: "5h".to_string(),
                    weekly_pct: 0.0,
                    weekly_reset: "7d".to_string(),
                    is_default: Some(row.is_default),
                });
            }
        }

        let mut windows = std::collections::HashMap::new();
        for q in &accounts {
            windows
                .entry(q.harness.clone())
                .or_insert_with(|| db.summarize_harness_usage_windows(&q.harness).unwrap_or((0, None, 0, None)));
        }
        (accounts, harness_rows, windows)
    };

    for q in &mut accounts {
        let (t5h, r5h, t7d, r7d) = db_windows
            .get(&q.harness)
            .cloned()
            .unwrap_or((0, None, 0, None));
        let eff_7d = if t7d == 0 && q.harness == "claude-code" {
            read_claude_recent_tokens_7d()
        } else {
            t7d
        };

        const BUDGET_5H: f64 = 500_000.0;
        const BUDGET_7D: f64 = 5_000_000.0;

        if t5h > 0 || q.five_hour_pct == 0.0 {
            let pct_5h = ((t5h as f64 / BUDGET_5H) * 100.0).clamp(0.0, 100.0);
            q.five_hour_pct = (pct_5h * 10.0).round() / 10.0;
        }
        if eff_7d > 0 || q.weekly_pct == 0.0 {
            let pct_7d = ((eff_7d as f64 / BUDGET_7D) * 100.0).clamp(0.0, 100.0);
            q.weekly_pct = (pct_7d * 10.0).round() / 10.0;
        }
        q.five_hour_reset = r5h.unwrap_or_else(|| "5h".to_string());
        q.weekly_reset = r7d.unwrap_or_else(|| "7d".to_string());

        if q.harness == "agy-cli" {
            if let Some(row) = harness_rows.iter().find(|r| {
                r.harness == "agy-cli" && (r.id == q.id || r.label.eq_ignore_ascii_case(&q.label))
            }) {
                if let Some((live_pct, live_reset)) = fetch_agy_live_quota(row).await {
                    q.five_hour_pct = live_pct;
                    q.five_hour_reset = live_reset;
                }
            }
        }
    }

    Ok(accounts)
}

pub async fn upsert_quota_account_inner(
    db: &std::sync::Arc<tokio::sync::Mutex<crate::db::Database>>,
    id: Option<String>,
    harness: Option<String>,
    provider: Option<String>,
    label: String,
    email: Option<String>,
    plan: Option<String>,
    five_hour_pct: Option<f64>,
    five_hour_reset: Option<String>,
    weekly_pct: Option<f64>,
    weekly_reset: Option<String>,
    is_default: Option<bool>,
) -> Result<Vec<QuotaAccount>, String> {
    let account = QuotaAccount {
        id: id.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| Uuid::new_v4().to_string()),
        harness: harness.or(provider).unwrap_or_else(|| "claude-code".to_string()),
        label,
        email: email.or(plan).unwrap_or_default(),
        five_hour_pct: five_hour_pct.unwrap_or(0.0),
        five_hour_reset: five_hour_reset.unwrap_or_else(|| "5h".to_string()),
        weekly_pct: weekly_pct.unwrap_or(0.0),
        weekly_reset: weekly_reset.unwrap_or_else(|| "7d".to_string()),
        is_default,
    };
    let db = db.lock().await;
    db.upsert_account(&account).map_err(|e| e.to_string())?;
    db.list_accounts().map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn upsert_quota_account(
    state: State<'_, DbState>,
    id: Option<String>,
    harness: Option<String>,
    provider: Option<String>,
    label: String,
    email: Option<String>,
    plan: Option<String>,
    five_hour_pct: Option<f64>,
    five_hour_reset: Option<String>,
    weekly_pct: Option<f64>,
    weekly_reset: Option<String>,
    is_default: Option<bool>,
) -> Result<Vec<QuotaAccount>, String> {
    upsert_quota_account_inner(
        &state.0,
        id,
        harness,
        provider,
        label,
        email,
        plan,
        five_hour_pct,
        five_hour_reset,
        weekly_pct,
        weekly_reset,
        is_default,
    )
    .await
}

pub async fn delete_quota_account_inner(
    db: &std::sync::Arc<tokio::sync::Mutex<crate::db::Database>>,
    id: String,
) -> Result<Vec<QuotaAccount>, String> {
    let db = db.lock().await;
    db.delete_account(&id).map_err(|e| e.to_string())?;
    db.list_accounts().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_quota_account(
    state: State<'_, DbState>,
    id: String,
) -> Result<Vec<QuotaAccount>, String> {
    delete_quota_account_inner(&state.0, id).await
}

#[tauri::command]
pub async fn list_env_secrets() -> Result<Vec<EnvSecret>, String> {
    let entries = read_env_entries();
    Ok(entries
        .into_iter()
        .map(|(key, val)| {
            let (masked, hint) = mask_secret_value(&val);
            EnvSecret {
                key,
                value: masked,
                hint,
            }
        })
        .collect())
}

pub async fn reveal_env_secret_inner(key: String) -> Result<String, String> {
    let key = key.trim();
    for (k, v) in read_env_entries() {
        if k == key {
            return Ok(v);
        }
    }
    Err(format!("Secret '{}' not found", key))
}

#[tauri::command]
pub async fn reveal_env_secret(key: String) -> Result<String, String> {
    reveal_env_secret_inner(key).await
}

pub async fn set_env_secret_inner(key: String, value: String) -> Result<Vec<EnvSecret>, String> {
    let key = key.trim().to_string();
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Secret key must be non-empty alphanumeric/underscore".to_string());
    }

    let mut entries = read_env_entries();
    if let Some((_, existing_val)) = entries.iter_mut().find(|(k, _)| k == &key) {
        *existing_val = value;
    } else {
        entries.push((key, value));
    }
    write_env_entries(&entries)?;
    list_env_secrets().await
}

#[tauri::command]
pub async fn set_env_secret(key: String, value: String) -> Result<Vec<EnvSecret>, String> {
    set_env_secret_inner(key, value).await
}

pub async fn delete_env_secret_inner(key: String) -> Result<Vec<EnvSecret>, String> {
    let key = key.trim().to_string();
    let mut entries = read_env_entries();
    entries.retain(|(k, _)| k != &key);
    write_env_entries(&entries)?;
    list_env_secrets().await
}

#[tauri::command]
pub async fn delete_env_secret(key: String) -> Result<Vec<EnvSecret>, String> {
    delete_env_secret_inner(key).await
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let manager = app.autolaunch();
    if enabled {
        manager
            .enable()
            .map_err(|e| format!("Failed to enable autostart: {e}"))?;
    } else {
        let _ = manager.disable();
    }
    Ok(manager.is_enabled().unwrap_or(enabled))
}

fn build_updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let mut builder = app.updater_builder();
    if let Some(token) = crate::platform::get_env_or_dotenv("GITHUB_TOKEN")
        .or_else(|| crate::platform::get_env_or_dotenv("GH_TOKEN"))
    {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            builder = builder
                .header("Authorization", format!("Bearer {trimmed}"))
                .map_err(|e| format!("Invalid GitHub token header: {e}"))?;
        }
    }
    builder
        .build()
        .map_err(|e| format!("Updater unavailable: {e}"))
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateCheckResult, String> {
    let current_version = app.package_info().version.to_string();
    let updater = match build_updater(&app) {
        Ok(u) => u,
        Err(err) => {
            return Ok(UpdateCheckResult {
                available: false,
                current_version,
                latest_version: None,
                body: None,
                error: Some(err),
            });
        }
    };

    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateCheckResult {
            available: true,
            current_version,
            latest_version: Some(update.version),
            body: update.body,
            error: None,
        }),
        Ok(None) => Ok(UpdateCheckResult {
            available: false,
            current_version,
            latest_version: None,
            body: None,
            error: None,
        }),
        Err(err) => Ok(UpdateCheckResult {
            available: false,
            current_version,
            latest_version: None,
            body: None,
            error: Some(err.to_string()),
        }),
    }
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = build_updater(&app)?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|e| format!("Update check failed: {e}"))?
    else {
        return Err("Already on the latest version".to_string());
    };

    let mut downloaded: u64 = 0;
    let app_for_progress = app.clone();
    update
        .download_and_install(
            move |chunk_length, content_length| {
                downloaded = downloaded.saturating_add(chunk_length as u64);
                let _ = app_for_progress.emit(
                    "update:progress",
                    serde_json::json!({
                        "downloaded": downloaded,
                        "total": content_length,
                    }),
                );
            },
            || {},
        )
        .await
        .map_err(|e| format!("Failed to install update: {e}"))?;
    app.restart();
}
