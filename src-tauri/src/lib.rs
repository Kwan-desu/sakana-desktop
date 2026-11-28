pub mod acp_pool;
pub mod auth;
pub mod automation;
pub mod db;
pub mod discord;
pub mod git;
pub mod harness;
pub mod mcp;
pub mod models;
pub mod office;
pub mod platform;
pub mod pty;
pub mod race;
pub mod remote;
pub mod self_control;
pub mod sync;
pub mod telemetry;
pub mod tray;
pub mod voice;

use std::sync::Arc;

use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::Mutex;

use crate::db::Database;
use crate::models::{
    AttachmentItem, BootstrapState, CreateProjectInput, CreateSessionInput, Project, Session,
    UpdateSessionInput,
};

pub struct DbState(pub Arc<Mutex<Database>>);

#[tauri::command]
async fn get_bootstrap_state(state: State<'_, DbState>) -> Result<BootstrapState, String> {
    let db = state.0.lock().await;
    db.get_bootstrap_state().map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_projects(state: State<'_, DbState>) -> Result<Vec<Project>, String> {
    let db = state.0.lock().await;
    db.list_projects().map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_project(
    state: State<'_, DbState>,
    input: CreateProjectInput,
) -> Result<Project, String> {
    let db = state.0.lock().await;
    db.create_project(input).map_err(|e| e.to_string())
}

#[tauri::command]
async fn rename_project(
    state: State<'_, DbState>,
    id: String,
    name: String,
) -> Result<bool, String> {
    let db = state.0.lock().await;
    db.rename_project(&id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
async fn reorder_projects(
    state: State<'_, DbState>,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    let db = state.0.lock().await;
    db.reorder_projects(&ordered_ids).map_err(|e| e.to_string())
}

#[tauri::command]
async fn reorder_sessions(
    state: State<'_, DbState>,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    let db = state.0.lock().await;
    db.reorder_sessions(&ordered_ids).map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_project_discord_channel(
    state: State<'_, DbState>,
    id: String,
    discord_channel: Option<String>,
) -> Result<bool, String> {
    let db = state.0.lock().await;
    db.set_project_discord_channel(&id, discord_channel.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_session_discord_channel(
    state: State<'_, DbState>,
    id: String,
    discord_channel: Option<String>,
) -> Result<bool, String> {
    let db = state.0.lock().await;
    db.set_session_discord_channel(&id, discord_channel.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_file_external(
    cwd: String,
    rel_path: String,
    wsl_distro: Option<String>,
) -> Result<(), String> {
    let raw_trimmed = rel_path.trim();
    if raw_trimmed.is_empty() {
        return Err("File path is empty".to_string());
    }
    let encoded_cwd = crate::platform::encode_wsl_path(wsl_distro.as_deref(), &cwd);
    let full_path = crate::git::resolve_path_for_open(&encoded_cwd, raw_trimmed)?;
    if !full_path.exists() {
        return Err(format!("File not found: {}", full_path.display()));
    }
    let target = full_path.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // Avoid `cmd /C start` which interprets shell metacharacters (&, |, %, ^).
        std::process::Command::new("explorer.exe")
            .arg(&target)
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|e| format!("Failed to open file externally: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&target)
            .spawn()
            .map_err(|e| format!("Failed to open file externally: {e}"))?;
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&target)
            .spawn()
            .map_err(|e| format!("Failed to open file externally: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
async fn delete_project(state: State<'_, DbState>, id: String) -> Result<bool, String> {
    let db = state.0.lock().await;
    db.delete_project(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_sessions(
    state: State<'_, DbState>,
    project_id: Option<String>,
) -> Result<Vec<Session>, String> {
    let db = state.0.lock().await;
    db.list_sessions(project_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_session(
    state: State<'_, DbState>,
    input: CreateSessionInput,
) -> Result<Session, String> {
    let db = state.0.lock().await;
    db.create_session(input).map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_session(
    state: State<'_, DbState>,
    input: UpdateSessionInput,
) -> Result<Option<Session>, String> {
    let db = state.0.lock().await;
    db.update_session(input).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_session(
    app: tauri::AppHandle,
    state: State<'_, DbState>,
    id: String,
) -> Result<bool, String> {
    acp_pool::sleep_session_inner(Some(&app), &id).await;
    let db = state.0.lock().await;
    db.delete_session(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn fork_session(
    state: State<'_, DbState>,
    source_id: String,
    title: String,
    branch: String,
    up_to_index: usize,
) -> Result<Session, String> {
    let db = state.0.lock().await;
    db.fork_session(&source_id, &title, &branch, up_to_index)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_theme(state: State<'_, DbState>, theme: String) -> Result<(), String> {
    let db = state.0.lock().await;
    db.set_theme(&theme).map_err(|e| e.to_string())
}

#[tauri::command]
async fn pick_project_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder.map(|p| p.to_string()));
    });
    rx.await.map_err(|e| e.to_string())
}

const MAX_ATTACHMENT_PREVIEW_BYTES: u64 = 5 * 1024 * 1024;
const BASE64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode_base64(input: &[u8]) -> String {
    let mut out = String::with_capacity(((input.len() + 2) / 3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64_TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(BASE64_TABLE[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(BASE64_TABLE[((n >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(BASE64_TABLE[(n & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let cleaned: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if cleaned.len() % 4 != 0 {
        return Err("Invalid base64 length".to_string());
    }
    let mut out = Vec::with_capacity((cleaned.len() / 4) * 3);
    for (idx, chunk) in cleaned.chunks_exact(4).enumerate() {
        let is_last = idx == (cleaned.len() / 4) - 1;
        let mut vals = [0u8; 4];
        let mut pad = 0usize;
        for i in 0..4 {
            let b = chunk[i];
            if b == b'=' {
                if !is_last || i < 2 {
                    return Err("Invalid base64 padding".to_string());
                }
                pad += 1;
                vals[i] = 0;
            } else {
                if pad > 0 {
                    return Err("Invalid base64 padding".to_string());
                }
                vals[i] = match b {
                    b'A'..=b'Z' => b - b'A',
                    b'a'..=b'z' => b - b'a' + 26,
                    b'0'..=b'9' => b - b'0' + 52,
                    b'+' => 62,
                    b'/' => 63,
                    _ => return Err(format!("Invalid base64 byte: {}", b)),
                };
            }
        }
        let n = ((vals[0] as u32) << 18)
            | ((vals[1] as u32) << 12)
            | ((vals[2] as u32) << 6)
            | (vals[3] as u32);
        out.push(((n >> 16) & 0xFF) as u8);
        if pad < 2 {
            out.push(((n >> 8) & 0xFF) as u8);
        }
        if pad < 1 {
            out.push((n & 0xFF) as u8);
        }
    }
    Ok(out)
}

fn classify_attachment(name: &str) -> (&'static str, Option<&'static str>) {
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("png") => ("image", Some("image/png")),
        Some("jpg") | Some("jpeg") => ("image", Some("image/jpeg")),
        Some("gif") => ("image", Some("image/gif")),
        Some("webp") => ("image", Some("image/webp")),
        Some("bmp") => ("image", Some("image/bmp")),
        Some("svg") => ("image", Some("image/svg+xml")),
        Some("pdf") => ("pdf", None),
        _ => ("file", None),
    }
}

fn sanitize_attachment_name(name: &str) -> String {
    let base = std::path::Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("attachment");
    let sanitized: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches(|c| c == '.' || c == '_');
    if trimmed.is_empty() {
        "attachment".to_string()
    } else {
        trimmed.to_string()
    }
}

#[tauri::command]
async fn pick_attachment_files(app: tauri::AppHandle) -> Result<Vec<AttachmentItem>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_files(move |files| {
        let paths = files.map(|list| {
            list.into_iter()
                .filter_map(|p| p.into_path().ok())
                .collect::<Vec<_>>()
        });
        let _ = tx.send(paths);
    });
    let Some(paths) = rx.await.map_err(|e| e.to_string())? else {
        return Ok(Vec::new());
    };

    let mut items = Vec::with_capacity(paths.len());
    for path in paths {
        let meta = std::fs::metadata(&path)
            .map_err(|e| format!("Failed to read metadata for {}: {}", path.display(), e))?;
        let size_bytes = meta.len();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment")
            .to_string();
        let (kind, mime) = classify_attachment(&name);
        let preview_data_url = if kind == "image" && size_bytes <= MAX_ATTACHMENT_PREVIEW_BYTES {
            let bytes = std::fs::read(&path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
            Some(format!(
                "data:{};base64,{}",
                mime.unwrap_or("image/png"),
                encode_base64(&bytes)
            ))
        } else {
            None
        };
        items.push(AttachmentItem {
            path: path.to_string_lossy().to_string(),
            name,
            size_bytes,
            kind: kind.to_string(),
            preview_data_url,
        });
    }
    Ok(items)
}

pub fn save_clipboard_attachment_inner(
    name: String,
    _mime: String,
    data_base64: String,
) -> Result<AttachmentItem, String> {
    let raw_b64 = if let Some(rest) = data_base64.strip_prefix("data:") {
        rest.split_once(";base64,")
            .map(|(_, b64)| b64)
            .unwrap_or(data_base64.as_str())
    } else {
        data_base64.as_str()
    };
    let bytes = decode_base64(raw_b64)?;
    let sanitized_name = sanitize_attachment_name(&name);
    let attachments_dir = platform::app_data_dir().join("attachments");
    std::fs::create_dir_all(&attachments_dir).map_err(|e| e.to_string())?;

    let dest = attachments_dir.join(format!("{}-{}", uuid::Uuid::new_v4(), sanitized_name));
    std::fs::write(&dest, &bytes).map_err(|e| e.to_string())?;

    let size_bytes = bytes.len() as u64;
    let (kind, mime) = classify_attachment(&sanitized_name);
    let preview_data_url = if kind == "image" && size_bytes <= MAX_ATTACHMENT_PREVIEW_BYTES {
        Some(format!(
            "data:{};base64,{}",
            mime.unwrap_or("image/png"),
            encode_base64(&bytes)
        ))
    } else {
        None
    };

    Ok(AttachmentItem {
        path: dest.to_string_lossy().to_string(),
        name: sanitized_name,
        size_bytes,
        kind: kind.to_string(),
        preview_data_url,
    })
}

#[tauri::command]
async fn save_clipboard_attachment(
    name: String,
    base64_data: String,
) -> Result<AttachmentItem, String> {
    save_clipboard_attachment_inner(name, String::new(), base64_data)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(auth::AuthState::default())
        .manage(harness::HarnessState::default())
        .manage(pty::PtyState::default())
        .manage(automation::AutomationState::default())
        .manage(discord::DiscordState::default())
        .manage(remote::RemoteState::default())
        .manage(office::OfficeState::default())
        .manage(sync::SyncState::default())
        .manage(race::RaceState::default())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(move |app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| platform::app_data_dir());
            platform::init_app_data_dir(data_dir.clone());
            let db = match Database::open_in_dir(&data_dir) {
                Ok(db) => db,
                Err(err) => {
                    eprintln!(
                        "Failed to initialize Sakana SQLite database at {}: {}",
                        data_dir.display(),
                        err
                    );
                    return Err(err.into());
                }
            };
            let db_arc = Arc::new(Mutex::new(db));
            app.manage(DbState(Arc::clone(&db_arc)));

            tray::create_tray(app.handle())?;
            let app_handle = app.handle().clone();
            let db_for_sched = Arc::clone(&db_arc);
            tauri::async_runtime::spawn(async move {
                let auto_state = app_handle.state::<automation::AutomationState>();
                let _ = automation::sync_scheduler(&auto_state, Arc::clone(&db_for_sched), app_handle.clone()).await;
                let discord_state = app_handle.state::<discord::DiscordState>();
                discord::maybe_auto_start_discord_bot(&discord_state, &db_for_sched, app_handle.clone()).await;
                let sync_state = app_handle.state::<sync::SyncState>();
                sync::restart_auto_sync_loop(&sync_state, db_for_sched, app_handle.clone()).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bootstrap_state,
            list_projects,
            create_project,
            rename_project,
            set_project_discord_channel,
            set_session_discord_channel,
            open_file_external,
            reorder_projects,
            reorder_sessions,
            delete_project,
            list_sessions,
            create_session,
            update_session,
            delete_session,
            fork_session,
            set_theme,
            pick_project_folder,
            pick_attachment_files,
            save_clipboard_attachment,
            harness::send_turn,
            harness::cancel_turn,
            acp_pool::list_awake_sessions,
            acp_pool::sleep_session,
            acp_pool::sleep_all_sessions,
            acp_pool::wake_session,
            harness::compact_session,
            harness::get_compaction_config,
            harness::update_compaction_config,
            harness::list_skills,
            harness::list_harness_models,
            auth::list_harness_accounts,
            auth::create_harness_account,
            auth::set_default_harness_account,
            auth::rename_harness_account,
            auth::delete_harness_account,
            auth::start_harness_login,
            auth::submit_harness_login_code,
            auth::check_harness_auth,
            auth::logout_harness_account,
            auth::open_external_url,
            git::create_worktree,
            git::remove_worktree,
            git::list_branches,
            git::checkout_branch,
            git::git_pull,
            git::git_push,
            git::create_checkpoint,
            git::get_checkpoint_diff,
            git::revert_to_checkpoint,
            git::get_git_diffs,
            git::stage_hunk,
            git::discard_hunk,
            git::draft_commit_message,
            git::git_commit,
            git::create_pr,
            git::github_auth_status,
            git::github_save_token,
            git::github_list_repos,
            git::github_clone_repo,
            git::list_workspace_files,
            git::read_workspace_file,
            git::write_workspace_file,
            git::list_trace_spans,
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
            pty::translate_nl_command,
            mcp::list_mcp_servers,
            mcp::add_mcp_server,
            mcp::toggle_mcp_server,
            mcp::delete_mcp_server,
            mcp::ping_mcp_server,
            mcp::import_agy_mcp_servers,
            automation::list_automations,
            automation::create_automation,
            automation::toggle_automation,
            automation::delete_automation,
            automation::run_automation_now,
            discord::get_discord_status,
            discord::start_discord_bot,
            discord::stop_discord_bot,
            discord::upsert_discord_rule,
            discord::delete_discord_rule,
            discord::get_discord_settings,
            discord::update_discord_settings,
            remote::get_remote_status,
            remote::start_remote_server,
            remote::stop_remote_server,
            remote::list_remote_hosts,
            remote::upsert_remote_host,
            remote::delete_remote_host,
            remote::test_remote_host,
            remote::remote_rpc,
            remote::connect_remote_events,
            remote::disconnect_remote_events,
            sync::get_sync_config,
            sync::update_sync_config,
            sync::get_sync_status,
            sync::sync_now,
            platform::list_wsl_distros,
            platform::list_directories,
            telemetry::get_system_telemetry,
            telemetry::list_quota_accounts,
            telemetry::upsert_quota_account,
            telemetry::delete_quota_account,
            telemetry::list_env_secrets,
            telemetry::reveal_env_secret,
            telemetry::set_env_secret,
            telemetry::delete_env_secret,
            telemetry::set_autostart,
            telemetry::check_for_updates,
            telemetry::install_update,
            office::list_offices,
            office::get_office,
            office::create_office,
            office::delete_office,
            office::update_office_settings,
            office::hire_office_staff,
            office::update_office_staff,
            office::fire_office_staff,
            office::get_office_staff_messages,
            office::send_office_message,
            office::report_staff_to_master,
            office::stop_office_run,
            self_control::get_app_customization,
            self_control::update_app_customization,
            self_control::reset_app_customization,
            voice::get_voice_config,
            voice::update_voice_config,
            voice::transcribe_audio,
            harness::get_failover_config,
            harness::update_failover_config,
            git::rewind_checkpoint,
            git::fork_from_checkpoint,
            race::start_race,
            race::list_races,
            race::get_race,
            race::merge_race_winner,
            telemetry::search_history,
            telemetry::get_telemetry_summary,
            telemetry::list_audit_logs,
            telemetry::record_audit_log,
            office::list_office_blueprints,
            office::save_office_blueprint,
            office::delete_office_blueprint,
            office::create_office_from_blueprint,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Sakana")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                tauri::async_runtime::block_on(acp_pool::sleep_all_sessions_inner(None));
            }
        });
}
