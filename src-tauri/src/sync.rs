use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, State};
use tokio::sync::{oneshot, Mutex};

use crate::db::Database;
use crate::models::{Project, Session};
use crate::DbState;

const SYNC_CONFIG_KEY: &str = "sakana_sync_config";
const MAX_SYNC_FILE_BYTES: u64 = 15 * 1024 * 1024;
const MAX_SYNC_FILES: usize = 2000;
const MTIME_GRACE_MS: i64 = 2000;

const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".svelte-kit",
    "__pycache__",
    ".venv",
    "venv",
    ".cache",
    ".turbo",
    ".parcel-cache",
    ".idea",
    ".vscode",
    "coverage",
    "out",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub peer_host_id: Option<String>,
    #[serde(default = "default_true")]
    pub sync_sessions: bool,
    #[serde(default = "default_true")]
    pub sync_files: bool,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_conflict_policy")]
    pub conflict_policy: String,
}

fn default_true() -> bool {
    true
}

fn default_interval() -> u64 {
    15
}

fn default_conflict_policy() -> String {
    "newest-wins".to_string()
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            peer_host_id: None,
            sync_sessions: true,
            sync_files: true,
            interval_secs: 15,
            conflict_policy: "newest-wins".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub running: bool,
    pub syncing: bool,
    pub peer_name: Option<String>,
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
    pub sessions_synced: usize,
    pub files_pulled: usize,
    pub files_pushed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFileEntry {
    pub rel_path: String,
    pub size: u64,
    pub mtime_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSessionsPayload {
    pub projects: Vec<Project>,
    pub sessions: Vec<Session>,
}

pub struct SyncState {
    pub status: Arc<Mutex<SyncStatus>>,
    pub stop_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    pub http_client: reqwest::Client,
}

impl Default for SyncState {
    fn default() -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .build()
            .unwrap_or_default();
        Self {
            status: Arc::new(Mutex::new(SyncStatus::default())),
            stop_tx: Arc::new(Mutex::new(None)),
            http_client,
        }
    }
}

pub fn load_sync_config(db: &Database) -> SyncConfig {
    db.get_setting(SYNC_CONFIG_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<SyncConfig>(&raw).ok())
        .unwrap_or_default()
}

pub fn save_sync_config(db: &Database, cfg: &SyncConfig) -> Result<(), String> {
    let raw = serde_json::to_string(cfg).map_err(|e| e.to_string())?;
    db.set_setting(SYNC_CONFIG_KEY, &raw)
        .map_err(|e| e.to_string())
}

fn resolve_project_root(cwd: &str, wsl_distro: Option<&str>) -> PathBuf {
    let encoded = crate::platform::encode_wsl_path(wsl_distro, cwd);
    if let Some((distro, linux_path)) = crate::platform::parse_wsl_path(&encoded) {
        crate::platform::wsl_to_unc_path(&distro, &linux_path)
    } else {
        PathBuf::from(cwd)
    }
}

fn validate_rel_path(rel_path: &str) -> Result<PathBuf, String> {
    let normalized = rel_path.replace('\\', "/");
    let p = Path::new(&normalized);
    if p.is_absolute() {
        return Err("Absolute path not allowed in sync".to_string());
    }
    let mut clean = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::Normal(seg) => clean.push(seg),
            Component::CurDir => {}
            _ => return Err(format!("Unsafe relative path in sync: {}", rel_path)),
        }
    }
    if clean.as_os_str().is_empty() {
        return Err("Empty relative path".to_string());
    }
    Ok(clean)
}

pub fn list_project_file_manifest(
    cwd: &str,
    wsl_distro: Option<&str>,
) -> Result<Vec<SyncFileEntry>, String> {
    let root = resolve_project_root(cwd, wsl_distro);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for item in rd.flatten() {
            if entries.len() >= MAX_SYNC_FILES {
                break;
            }
            let path = item.path();
            let name = item.file_name().to_string_lossy().to_string();
            let Ok(meta) = item.metadata() else {
                continue;
            };
            if meta.is_dir() {
                if IGNORED_DIRS.iter().any(|&d| d.eq_ignore_ascii_case(&name)) {
                    continue;
                }
                stack.push(path);
            } else if meta.is_file() {
                if name == ".DS_Store" || name.ends_with(".tmp") || meta.len() > MAX_SYNC_FILE_BYTES {
                    continue;
                }
                let Ok(rel) = path.strip_prefix(&root) else {
                    continue;
                };
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                let mtime_ms = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                entries.push(SyncFileEntry {
                    rel_path: rel_str,
                    size: meta.len(),
                    mtime_ms,
                });
            }
        }
    }
    entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(entries)
}

pub fn b64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

pub fn b64_decode(input: &str) -> Result<Vec<u8>, String> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let clean: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if clean.len() % 4 != 0 {
        return Err("Invalid base64 length".to_string());
    }
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for chunk in clean.chunks(4) {
        let a = val(chunk[0]).ok_or_else(|| "Invalid base64 char".to_string())?;
        let b = val(chunk[1]).ok_or_else(|| "Invalid base64 char".to_string())?;
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            let c = val(chunk[2]).ok_or_else(|| "Invalid base64 char".to_string())?;
            out.push((b << 4) | (c >> 2));
            if chunk[3] != b'=' {
                let d = val(chunk[3]).ok_or_else(|| "Invalid base64 char".to_string())?;
                out.push((c << 6) | d);
            }
        }
    }
    Ok(out)
}

pub fn read_project_file_b64(
    cwd: &str,
    rel_path: &str,
    wsl_distro: Option<&str>,
) -> Result<Value, String> {
    let root = resolve_project_root(cwd, wsl_distro);
    let clean = validate_rel_path(rel_path)?;
    let full = root.join(clean);
    let meta = std::fs::metadata(&full).map_err(|e| format!("Stat failed: {e}"))?;
    if meta.len() > MAX_SYNC_FILE_BYTES {
        return Err("File exceeds 15MB sync limit".to_string());
    }
    let bytes = std::fs::read(&full).map_err(|e| format!("Read failed: {e}"))?;
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Ok(json!({
        "relPath": rel_path,
        "contentB64": b64_encode(&bytes),
        "mtimeMs": mtime_ms,
        "size": bytes.len(),
    }))
}

pub fn write_project_file_b64(
    cwd: &str,
    rel_path: &str,
    content_b64: &str,
    wsl_distro: Option<&str>,
) -> Result<bool, String> {
    let root = resolve_project_root(cwd, wsl_distro);
    let clean = validate_rel_path(rel_path)?;
    let full = root.join(clean);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Create dir failed: {e}"))?;
    }
    let bytes = b64_decode(content_b64)?;
    std::fs::write(&full, bytes).map_err(|e| format!("Write failed: {e}"))?;
    Ok(true)
}

pub fn export_sessions_payload(db: &Database) -> Result<SyncSessionsPayload, String> {
    let projects = db.list_projects().map_err(|e| e.to_string())?;
    let sessions = db.list_sessions(None).map_err(|e| e.to_string())?;
    Ok(SyncSessionsPayload { projects, sessions })
}

pub fn merge_sessions_payload(
    db: &Database,
    payload: &SyncSessionsPayload,
) -> Result<(usize, usize), String> {
    let mut proj_id_map: HashMap<String, String> = HashMap::new();
    for proj in &payload.projects {
        if let Ok(local_id) = db.upsert_synced_project(proj) {
            proj_id_map.insert(proj.id.clone(), local_id);
        }
    }
    let fallback_proj_id = db
        .list_projects()
        .ok()
        .and_then(|p| p.first().map(|x| x.id.clone()))
        .unwrap_or_else(|| "proj-default".to_string());

    let mut sessions_touched = 0usize;
    let mut new_messages = 0usize;
    for sess in &payload.sessions {
        let mapped_pid = proj_id_map
            .get(&sess.project_id)
            .cloned()
            .unwrap_or_else(|| fallback_proj_id.clone());
        match db.upsert_synced_session(sess, &mapped_pid) {
            Ok(added) => {
                sessions_touched += 1;
                new_messages += added;
            }
            Err(_) => {}
        }
    }
    Ok((sessions_touched, new_messages))
}

async fn call_peer_rpc(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    command: &str,
    args: Value,
) -> Result<Value, String> {
    let endpoint = format!("{}/api/rpc", url.trim_end_matches('/'));
    let resp = client
        .post(&endpoint)
        .bearer_auth(token)
        .json(&json!({ "command": command, "args": args }))
        .send()
        .await
        .map_err(|e| format!("Peer RPC '{}' failed: {}", command, e))?;

    let status = resp.status();
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Invalid peer JSON for '{}': {}", command, e))?;

    if !status.is_success() || !body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        let err = body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown peer RPC error");
        return Err(format!("Peer RPC '{}': {}", command, err));
    }
    Ok(body.get("data").cloned().unwrap_or(Value::Null))
}

pub async fn run_sync_cycle(
    db_arc: &Arc<Mutex<Database>>,
    status_arc: &Arc<Mutex<SyncStatus>>,
    http_client: &reqwest::Client,
    app: Option<&AppHandle>,
) -> Result<SyncStatus, String> {
    let (cfg, peer) = {
        let db = db_arc.lock().await;
        let cfg = load_sync_config(&db);
        let peer_id = cfg
            .peer_host_id
            .clone()
            .ok_or_else(|| "No sync peer host selected".to_string())?;
        let peer = db
            .get_remote_host(&peer_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Selected sync peer host not found".to_string())?;
        (cfg, peer)
    };

    {
        let mut st = status_arc.lock().await;
        st.syncing = true;
        st.peer_name = Some(peer.name.clone());
        st.last_error = None;
        crate::platform::emit_app_and_remote(app, "sakana:sync-status", &*st);
    }

    let mut sessions_synced = 0usize;
    let mut new_msgs_total = 0usize;
    let mut files_pulled = 0usize;
    let mut files_pushed = 0usize;

    let cycle_res: Result<(), String> = async {
        let mut remote_projects: Vec<Project> = Vec::new();

        // 1. Bidirectional Chat Sessions & History Sync
        if cfg.sync_sessions {
            let remote_val =
                call_peer_rpc(http_client, &peer.url, &peer.token, "sync_get_sessions", json!({}))
                    .await?;
            let remote_payload: SyncSessionsPayload =
                serde_json::from_value(remote_val).map_err(|e| e.to_string())?;
            remote_projects = remote_payload.projects.clone();

            let local_payload = {
                let db = db_arc.lock().await;
                let (touched, added) = merge_sessions_payload(&db, &remote_payload)?;
                sessions_synced += touched;
                new_msgs_total += added;
                export_sessions_payload(&db)?
            };

            let _ = call_peer_rpc(
                http_client,
                &peer.url,
                &peer.token,
                "sync_push_sessions",
                serde_json::to_value(&local_payload).map_err(|e| e.to_string())?,
            )
            .await?;
        }

        // 2. Bidirectional Workspace File Sync for matched projects
        if cfg.sync_files {
            if remote_projects.is_empty() {
                if let Ok(val) =
                    call_peer_rpc(http_client, &peer.url, &peer.token, "list_projects", json!({}))
                        .await
                {
                    remote_projects = serde_json::from_value(val).unwrap_or_default();
                }
            }

            let local_projects = {
                let db = db_arc.lock().await;
                db.list_projects().map_err(|e| e.to_string())?
            };

            for lp in &local_projects {
                let Some(rp) = remote_projects
                    .iter()
                    .find(|r| r.id == lp.id || r.name.eq_ignore_ascii_case(&lp.name))
                else {
                    continue;
                };

                let lp_cwd = lp.cwd.clone();
                let lp_wsl = lp.wsl_distro.clone();
                let local_manifest = tokio::task::spawn_blocking(move || {
                    list_project_file_manifest(&lp_cwd, lp_wsl.as_deref())
                })
                .await
                .map_err(|e| e.to_string())??;

                let remote_manifest_val = call_peer_rpc(
                    http_client,
                    &peer.url,
                    &peer.token,
                    "sync_list_file_manifest",
                    json!({
                        "cwd": rp.cwd,
                        "wslDistro": rp.wsl_distro,
                    }),
                )
                .await?;
                let remote_manifest: Vec<SyncFileEntry> =
                    serde_json::from_value(remote_manifest_val).unwrap_or_default();

                let local_map: HashMap<String, SyncFileEntry> = local_manifest
                    .into_iter()
                    .map(|e| (e.rel_path.clone(), e))
                    .collect();
                let remote_map: HashMap<String, SyncFileEntry> = remote_manifest
                    .into_iter()
                    .map(|e| (e.rel_path.clone(), e))
                    .collect();

                // Pull missing or newer files from peer PC
                for (path, r_entry) in &remote_map {
                    let should_pull = match local_map.get(path) {
                        None => true,
                        Some(l_entry) => match cfg.conflict_policy.as_str() {
                            "remote-wins" => r_entry.mtime_ms != l_entry.mtime_ms || r_entry.size != l_entry.size,
                            "local-wins" => false,
                            _ => r_entry.mtime_ms > l_entry.mtime_ms + MTIME_GRACE_MS,
                        },
                    };
                    if should_pull {
                        let file_val = call_peer_rpc(
                            http_client,
                            &peer.url,
                            &peer.token,
                            "sync_read_file_b64",
                            json!({
                                "cwd": rp.cwd,
                                "relPath": path,
                                "wslDistro": rp.wsl_distro,
                            }),
                        )
                        .await?;
                        if let Some(b64) = file_val.get("contentB64").and_then(|v| v.as_str()) {
                            let cwd_c = lp.cwd.clone();
                            let rel_c = path.clone();
                            let b64_c = b64.to_string();
                            let wsl_c = lp.wsl_distro.clone();
                            tokio::task::spawn_blocking(move || {
                                write_project_file_b64(&cwd_c, &rel_c, &b64_c, wsl_c.as_deref())
                            })
                            .await
                            .map_err(|e| e.to_string())??;
                            files_pulled += 1;
                        }
                    }
                }

                // Push missing or newer local files to peer PC
                for (path, l_entry) in &local_map {
                    let should_push = match remote_map.get(path) {
                        None => true,
                        Some(r_entry) => match cfg.conflict_policy.as_str() {
                            "local-wins" => l_entry.mtime_ms != r_entry.mtime_ms || l_entry.size != r_entry.size,
                            "remote-wins" => false,
                            _ => l_entry.mtime_ms > r_entry.mtime_ms + MTIME_GRACE_MS,
                        },
                    };
                    if should_push {
                        let cwd_c = lp.cwd.clone();
                        let rel_c = path.clone();
                        let wsl_c = lp.wsl_distro.clone();
                        let file_val = tokio::task::spawn_blocking(move || {
                            read_project_file_b64(&cwd_c, &rel_c, wsl_c.as_deref())
                        })
                        .await
                        .map_err(|e| e.to_string())??;
                        if let Some(b64) = file_val.get("contentB64").and_then(|v| v.as_str()) {
                            let _ = call_peer_rpc(
                                http_client,
                                &peer.url,
                                &peer.token,
                                "sync_write_file_b64",
                                json!({
                                    "cwd": rp.cwd,
                                    "relPath": path,
                                    "contentB64": b64,
                                    "wslDistro": rp.wsl_distro,
                                }),
                            )
                            .await?;
                            files_pushed += 1;
                        }
                    }
                }
            }
        }

        Ok(())
    }
    .await;

    let final_status = {
        let mut st = status_arc.lock().await;
        st.syncing = false;
        match &cycle_res {
            Ok(()) => {
                st.last_sync_at = Some(Utc::now().to_rfc3339());
                st.last_error = None;
                st.sessions_synced = sessions_synced;
                st.files_pulled = files_pulled;
                st.files_pushed = files_pushed;
            }
            Err(e) => {
                st.last_error = Some(e.clone());
            }
        }
        st.clone()
    };

    crate::platform::emit_app_and_remote(app, "sakana:sync-status", &final_status);
    if cycle_res.is_ok() && (new_msgs_total > 0 || files_pulled > 0) {
        crate::platform::emit_app_and_remote(
            app,
            "sakana:sync-updated",
            &json!({
                "newMessages": new_msgs_total,
                "filesPulled": files_pulled,
                "filesPushed": files_pushed,
            }),
        );
    }

    cycle_res.map(|_| final_status)
}

pub async fn restart_auto_sync_loop(
    sync_state: &SyncState,
    db_arc: Arc<Mutex<Database>>,
    app: AppHandle,
) {
    // Stop previous loop if running
    if let Some(tx) = sync_state.stop_tx.lock().await.take() {
        let _ = tx.send(());
    }

    let cfg = {
        let db = db_arc.lock().await;
        load_sync_config(&db)
    };

    if !cfg.enabled || cfg.peer_host_id.is_none() {
        let mut st = sync_state.status.lock().await;
        st.running = false;
        crate::platform::emit_app_and_remote(Some(&app), "sakana:sync-status", &*st);
        return;
    }

    let interval_secs = cfg.interval_secs.clamp(5, 3600);
    let (tx, mut rx) = oneshot::channel::<()>();
    *sync_state.stop_tx.lock().await = Some(tx);

    {
        let mut st = sync_state.status.lock().await;
        st.running = true;
        crate::platform::emit_app_and_remote(Some(&app), "sakana:sync-status", &*st);
    }

    let status_arc = Arc::clone(&sync_state.status);
    let client = sync_state.http_client.clone();

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            tokio::select! {
                _ = &mut rx => {
                    break;
                }
                _ = ticker.tick() => {
                    let _ = run_sync_cycle(&db_arc, &status_arc, &client, Some(&app)).await;
                }
            }
        }
        let mut st = status_arc.lock().await;
        st.running = false;
        crate::platform::emit_app_and_remote(Some(&app), "sakana:sync-status", &*st);
    });
}

#[tauri::command]
pub async fn get_sync_config(db_state: State<'_, DbState>) -> Result<SyncConfig, String> {
    let db = db_state.0.lock().await;
    Ok(load_sync_config(&db))
}

#[tauri::command]
pub async fn update_sync_config(
    app: AppHandle,
    db_state: State<'_, DbState>,
    sync_state: State<'_, SyncState>,
    config: SyncConfig,
) -> Result<SyncConfig, String> {
    let mut normalized = config;
    normalized.interval_secs = normalized.interval_secs.clamp(5, 3600);
    if !matches!(
        normalized.conflict_policy.as_str(),
        "newest-wins" | "local-wins" | "remote-wins"
    ) {
        normalized.conflict_policy = "newest-wins".to_string();
    }
    {
        let db = db_state.0.lock().await;
        save_sync_config(&db, &normalized)?;
    }
    restart_auto_sync_loop(&sync_state, Arc::clone(&db_state.0), app).await;
    Ok(normalized)
}

#[tauri::command]
pub async fn get_sync_status(sync_state: State<'_, SyncState>) -> Result<SyncStatus, String> {
    Ok(sync_state.status.lock().await.clone())
}

#[tauri::command]
pub async fn sync_now(
    app: AppHandle,
    db_state: State<'_, DbState>,
    sync_state: State<'_, SyncState>,
) -> Result<SyncStatus, String> {
    run_sync_cycle(
        &db_state.0,
        &sync_state.status,
        &sync_state.http_client,
        Some(&app),
    )
    .await
}
