use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Serialize;
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, RefreshKind, System};
use tauri::{AppHandle, State};
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

#[tauri::command]
pub async fn list_quota_accounts(state: State<'_, DbState>) -> Result<Vec<QuotaAccount>, String> {
    let db = state.0.lock().await;
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
    let db = state.0.lock().await;
    db.upsert_account(&account).map_err(|e| e.to_string())?;
    db.list_accounts().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_quota_account(
    state: State<'_, DbState>,
    id: String,
) -> Result<Vec<QuotaAccount>, String> {
    let db = state.0.lock().await;
    db.delete_account(&id).map_err(|e| e.to_string())?;
    db.list_accounts().map_err(|e| e.to_string())
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

#[tauri::command]
pub async fn reveal_env_secret(key: String) -> Result<String, String> {
    let key = key.trim();
    for (k, v) in read_env_entries() {
        if k == key {
            return Ok(v);
        }
    }
    Err(format!("Secret '{}' not found", key))
}

#[tauri::command]
pub async fn set_env_secret(key: String, value: String) -> Result<Vec<EnvSecret>, String> {
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
pub async fn delete_env_secret(key: String) -> Result<Vec<EnvSecret>, String> {
    let key = key.trim().to_string();
    let mut entries = read_env_entries();
    entries.retain(|(k, _)| k != &key);
    write_env_entries(&entries)?;
    list_env_secrets().await
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

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateCheckResult, String> {
    let current_version = app.package_info().version.to_string();
    let updater = match app.updater() {
        Ok(u) => u,
        Err(err) => {
            return Ok(UpdateCheckResult {
                available: false,
                current_version,
                latest_version: None,
                body: None,
                error: Some(format!("Updater unavailable: {err}")),
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
    let updater = app
        .updater()
        .map_err(|e| format!("Updater unavailable: {e}"))?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|e| format!("Update check failed: {e}"))?
    else {
        return Err("Already on the latest version".to_string());
    };

    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("Failed to install update: {e}"))?;
    app.restart();
}
