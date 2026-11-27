use std::collections::HashMap;
use std::sync::Arc;

use chrono::Local;
use futures_util::future::join_all;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, State};
use tokio::sync::{oneshot, watch, Mutex};
use uuid::Uuid;

use crate::db::Database;
use crate::harness::{self, HarnessState, TurnRunOptions};
use crate::models::{
    ChatMessage, CreateOfficeInput, CreateProjectInput, CreateSessionInput, HireStaffInput,
    Office, OfficeEvent, OfficeStaff, UpdateSessionInput, UpdateStaffInput,
};
use crate::DbState;

#[derive(Default)]
pub struct OfficeState {
    pub active_runs: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegationEdge {
    pub office_id: String,
    pub from_staff_id: String,
    pub to_staff_id: String,
    pub task: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeDelegationPayload {
    pub office_id: String,
    pub edges: Vec<DelegationEdge>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum OfficeAction {
    Hire {
        name: String,
        role: String,
        #[serde(default)]
        harness: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        system_prompt: Option<String>,
        #[serde(default)]
        isolated_worktree: Option<bool>,
        #[serde(default)]
        avatar_color: Option<String>,
    },
    Delegate {
        #[serde(default)]
        staff_id: Option<String>,
        #[serde(default)]
        staff_name: Option<String>,
        task: String,
    },
    Fire {
        #[serde(default)]
        staff_id: Option<String>,
        #[serde(default)]
        staff_name: Option<String>,
    },
}

const AVATAR_PALETTE: &[&str] = &[
    "#0ea5e9", "#8b5cf6", "#10b981", "#f59e0b", "#ec4899", "#06b6d4", "#6366f1",
];

fn now_ts() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        let head: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}…", head)
    }
}

fn sanitize_slug(s: &str) -> String {
    let out: String = s
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "staff".to_string()
    } else {
        trimmed.chars().take(24).collect()
    }
}

pub fn parse_office_actions(text: &str) -> Vec<OfficeAction> {
    let mut actions = Vec::new();
    let Ok(re) = Regex::new(r"(?s)```sakana-office\s*\n?(.*?)\n?```") else {
        return actions;
    };
    for cap in re.captures_iter(text) {
        let block = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        if block.is_empty() {
            continue;
        }
        if let Ok(single) = serde_json::from_str::<OfficeAction>(block) {
            actions.push(single);
        } else if let Ok(many) = serde_json::from_str::<Vec<OfficeAction>>(block) {
            actions.extend(many);
        }
    }
    actions
}

fn emit_office_updated(app: Option<&AppHandle>, office: &Office) {
    crate::platform::emit_app_and_remote(app, "office:updated", office.clone());
}

fn emit_office_event(app: Option<&AppHandle>, event: &OfficeEvent) {
    crate::platform::emit_app_and_remote(app, "office:event", event.clone());
}

fn emit_office_delegation(app: Option<&AppHandle>, office_id: &str, edges: Vec<DelegationEdge>) {
    crate::platform::emit_app_and_remote(
        app,
        "office:delegation",
        OfficeDelegationPayload {
            office_id: office_id.to_string(),
            edges,
        },
    );
}

fn build_master_system_context(office: &Office) -> String {
    let roster_lines: Vec<String> = office
        .staff
        .iter()
        .filter(|s| !s.is_master)
        .map(|s| {
            format!(
                "- id: `{}` | name: `{}` | role: `{}` | harness: `{}` | model: `{}` | status: `{}`",
                s.id, s.name, s.role, s.harness, s.model, s.status
            )
        })
        .collect();
    let roster_block = if roster_lines.is_empty() {
        "(No staff hired yet — you can hire specialists before delegating.)".to_string()
    } else {
        roster_lines.join("\n")
    };

    let rounds_label = if office.max_rounds <= 0 {
        "Unlimited (continue delegating until the task is complete)".to_string()
    } else {
        office.max_rounds.to_string()
    };

    format!(
        "You are the Master Orchestrator of Sakana Office \"{}\".\n\
        Project working directory: `{}`\n\
        Max delegation rounds: {}\n\n\
        Current Staff Roster:\n\
        {}\n\n\
        Orchestration Instructions:\n\
        When you want to hire staff, delegate tasks to staff (in parallel), or fire staff, emit one or more fenced ```sakana-office JSON blocks.\n\
        Supported actions:\n\
        1. Hire:\n\
        ```sakana-office\n\
        {{\"action\":\"hire\",\"name\":\"Backend Dev\",\"role\":\"Rust & API engineer\",\"harness\":\"claude-code\",\"model\":\"claude-sonnet-4-6\",\"isolated_worktree\":false}}\n\
        ```\n\
        2. Delegate (you may emit multiple delegate blocks in one response to run staff in parallel):\n\
        ```sakana-office\n\
        {{\"action\":\"delegate\",\"staff_id\":\"<id>\",\"staff_name\":\"<name>\",\"task\":\"Concrete instructions for this staff member\"}}\n\
        ```\n\
        3. Fire:\n\
        ```sakana-office\n\
        {{\"action\":\"fire\",\"staff_id\":\"<id>\",\"staff_name\":\"<name>\"}}\n\
        ```\n\
        When all work is complete or no further delegation is needed, provide a clear final summary to the user without any delegate blocks.",
        office.name, office.cwd, rounds_label, roster_block
    )
}

fn build_staff_system_context(office: &Office, staff: &OfficeStaff) -> String {
    let custom_prompt = if staff.system_prompt.trim().is_empty() {
        String::new()
    } else {
        format!("\nRole Instructions:\n{}\n", staff.system_prompt.trim())
    };
    format!(
        "You are {} ({}), a staff member in Sakana Office \"{}\".\n\
        Working directory: `{}`{}\n\
        Complete the assigned task thoroughly and report your findings and changes clearly.",
        staff.name, staff.role, office.name, staff.worktree_path, custom_prompt
    )
}

pub async fn hire_staff_internal(
    app: Option<&AppHandle>,
    db_arc: &Arc<Mutex<Database>>,
    input: HireStaffInput,
) -> Result<OfficeStaff, String> {
    let office = {
        let db = db_arc.lock().await;
        db.get_office(&input.office_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Office '{}' not found", input.office_id))?
    };

    let staff_id = format!("staff-{}", Uuid::new_v4());
    let effort = input.effort.unwrap_or_else(|| "medium".to_string());
    let account = input.account.unwrap_or_else(|| "default".to_string());
    let permission_mode = input.permission_mode.unwrap_or_else(|| "yolo".to_string());
    let want_isolated = input.isolated_worktree.unwrap_or(false);

    let session = {
        let db = db_arc.lock().await;
        db.create_session_for_office(
            CreateSessionInput {
                project_id: office.project_id.clone(),
                title: format!("{} · {}", office.name, input.name),
                harness: Some(input.harness.clone()),
                model: Some(input.model.clone()),
                effort: Some(effort.clone()),
                account: Some(account.clone()),
                permission_mode: Some(permission_mode.clone()),
                worktree_path: Some(office.cwd.clone()),
                branch: Some(String::new()),
                shared_deps: None,
                is_discord: None,
                discord_channel: None,
            },
            &office.id,
        )
        .map_err(|e| e.to_string())?
    };

    let mut isolated_worktree = false;
    let mut worktree_path = office.cwd.clone();
    let mut branch = String::new();

    if want_isolated {
        let wt_branch = format!(
            "office/{}/{}",
            sanitize_slug(&office.name),
            sanitize_slug(&input.name)
        );
        let wsl_distro = {
            let db = db_arc.lock().await;
            db.get_project(&office.project_id)
                .ok()
                .flatten()
                .and_then(|p| p.wsl_distro)
        };
        let encoded_cwd =
            crate::platform::encode_wsl_path(wsl_distro.as_deref(), &office.cwd);
        match crate::git::create_worktree_inner(
            &db_arc,
            session.id.clone(),
            encoded_cwd,
            wt_branch.clone(),
            Vec::new(),
        )
        .await
        {
            Ok(wt_path) => {
                let normalized_wt_path = crate::platform::parse_wsl_path(&wt_path)
                    .map(|(_, linux_path)| linux_path)
                    .unwrap_or(wt_path);
                isolated_worktree = true;
                worktree_path = normalized_wt_path.clone();
                branch = wt_branch.clone();
                let db = db_arc.lock().await;
                let _ = db.update_session(UpdateSessionInput {
                    id: session.id.clone(),
                    project_id: None,
                    title: None,
                    status: None,
                    harness: None,
                    model: None,
                    effort: None,
                    account: None,
                    permission_mode: None,
                    worktree_path: Some(normalized_wt_path),
                    branch: Some(wt_branch),
                    shared_deps: None,
                    additions: None,
                    deletions: None,
                    unread_done: None,
                });
            }
            Err(err) => {
                let evt = OfficeEvent {
                    id: format!("evt-{}", Uuid::new_v4()),
                    office_id: office.id.clone(),
                    kind: "error".to_string(),
                    from_staff_id: None,
                    from_name: Some("System".to_string()),
                    to_staff_id: Some(staff_id.clone()),
                    to_name: Some(input.name.clone()),
                    summary: format!(
                        "Worktree unavailable for {}; using shared directory",
                        input.name
                    ),
                    detail: Some(err),
                    timestamp: now_ts(),
                };
                let db = db_arc.lock().await;
                let _ = db.append_office_event(&evt);
                emit_office_event(app, &evt);
            }
        }
    }

    let palette_idx = office.staff.len() % AVATAR_PALETTE.len();
    let avatar_color = input
        .avatar_color
        .filter(|c| !c.trim().is_empty())
        .unwrap_or_else(|| AVATAR_PALETTE[palette_idx].to_string());

    let staff = OfficeStaff {
        id: staff_id,
        office_id: office.id.clone(),
        session_id: session.id,
        name: input.name,
        role: input.role,
        is_master: false,
        is_dynamic: input.is_dynamic.unwrap_or(false),
        harness: input.harness,
        model: input.model,
        effort,
        account,
        permission_mode,
        system_prompt: input.system_prompt.unwrap_or_default(),
        isolated_worktree,
        worktree_path,
        branch,
        status: "IDLE".to_string(),
        current_task: None,
        avatar_color,
        desk_order: office.staff.len() as i64,
        created_at: now_ts(),
    };

    let evt = OfficeEvent {
        id: format!("evt-{}", Uuid::new_v4()),
        office_id: office.id.clone(),
        kind: "hire".to_string(),
        from_staff_id: Some(office.master_staff_id.clone()),
        from_name: office
            .staff
            .iter()
            .find(|s| s.is_master)
            .map(|s| s.name.clone()),
        to_staff_id: Some(staff.id.clone()),
        to_name: Some(staff.name.clone()),
        summary: format!("Hired {} ({})", staff.name, staff.role),
        detail: None,
        timestamp: now_ts(),
    };

    let updated_office = {
        let db = db_arc.lock().await;
        db.insert_office_staff(&staff).map_err(|e| e.to_string())?;
        db.append_office_event(&evt).map_err(|e| e.to_string())?;
        db.get_office(&office.id).map_err(|e| e.to_string())?
    };

    emit_office_event(app, &evt);
    if let Some(ref off) = updated_office {
        emit_office_updated(app, off);
    }

    Ok(staff)
}

async fn fire_staff_internal(
    app: Option<&AppHandle>,
    db_arc: &Arc<Mutex<Database>>,
    active_turns: &Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    staff_id: &str,
) -> Result<bool, String> {
    let (staff, office_cwd, master_id, master_name) = {
        let db = db_arc.lock().await;
        let staff = db
            .get_office_staff(staff_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Staff '{}' not found", staff_id))?;
        if staff.is_master {
            return Err("Cannot fire the Master Orchestrator".to_string());
        }
        let office = db
            .get_office(&staff.office_id)
            .map_err(|e| e.to_string())?;
        let cwd = office
            .as_ref()
            .map(|o| o.cwd.clone())
            .unwrap_or_else(|| ".".to_string());
        let m_id = office.as_ref().map(|o| o.master_staff_id.clone());
        let m_name = office
            .and_then(|o| o.staff.into_iter().find(|s| s.is_master).map(|s| s.name));
        (staff, cwd, m_id, m_name)
    };

    {
        let mut active = active_turns.lock().await;
        if let Some(tx) = active.remove(&staff.session_id) {
            let _ = tx.send(());
        }
    }

    if staff.isolated_worktree && !staff.worktree_path.is_empty() && staff.worktree_path != office_cwd {
        let _ = crate::git::remove_worktree_inner(&db_arc, staff.session_id.clone(), office_cwd).await;
    }

    let evt = OfficeEvent {
        id: format!("evt-{}", Uuid::new_v4()),
        office_id: staff.office_id.clone(),
        kind: "fire".to_string(),
        from_staff_id: master_id,
        from_name: master_name,
        to_staff_id: Some(staff.id.clone()),
        to_name: Some(staff.name.clone()),
        summary: format!("Removed {} ({})", staff.name, staff.role),
        detail: None,
        timestamp: now_ts(),
    };

    let (deleted, updated_office) = {
        let db = db_arc.lock().await;
        let deleted = db
            .delete_office_staff(&staff.id)
            .map_err(|e| e.to_string())?;
        let _ = db.append_office_event(&evt);
        let off = db.get_office(&staff.office_id).map_err(|e| e.to_string())?;
        (deleted, off)
    };

    emit_office_event(app, &evt);
    if let Some(ref off) = updated_office {
        emit_office_updated(app, off);
    }

    Ok(deleted)
}

#[tauri::command]
pub async fn list_offices(db_state: State<'_, DbState>) -> Result<Vec<Office>, String> {
    let db = db_state.0.lock().await;
    db.list_offices().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_office(
    db_state: State<'_, DbState>,
    id: String,
) -> Result<Option<Office>, String> {
    let db = db_state.0.lock().await;
    db.get_office(&id).map_err(|e| e.to_string())
}

pub async fn create_office_inner(
    app: Option<&AppHandle>,
    db_arc: &Arc<Mutex<Database>>,
    input: CreateOfficeInput,
) -> Result<Office, String> {
    let office_id = format!("office-{}", Uuid::new_v4());
    let master_staff_id = format!("staff-{}", Uuid::new_v4());
    let master_name = input
        .master_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Master Orchestrator".to_string());
    let master_effort = input
        .master_effort
        .unwrap_or_else(|| "medium".to_string());
    let master_account = input
        .master_account
        .unwrap_or_else(|| "default".to_string());
    let master_permission_mode = input
        .master_permission_mode
        .unwrap_or_else(|| "yolo".to_string());
    let max_rounds = input.max_rounds.unwrap_or(0).max(0);

    let (project_id, cwd) = {
        let db = db_arc.lock().await;
        if let Some(ref pid) = input.project_id {
            if let Some(proj) = db.get_project(pid).map_err(|e| e.to_string())? {
                (proj.id, proj.cwd)
            } else {
                let target_cwd = input
                    .cwd
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| ".".to_string());
                let proj_name = std::path::Path::new(&target_cwd)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&input.name)
                    .to_string();
                let proj = db
                    .create_project(CreateProjectInput {
                        name: proj_name,
                        cwd: target_cwd.clone(),
                        repo_url: None,
                        discord_channel: None,
                        runtime: input.runtime.clone(),
                        wsl_distro: input.wsl_distro.clone(),
                    })
                    .map_err(|e| e.to_string())?;
                (proj.id, proj.cwd)
            }
        } else {
            let target_cwd = input
                .cwd
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| ".".to_string());
            let existing = db
                .list_projects()
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|p| p.cwd == target_cwd);
            if let Some(proj) = existing {
                (proj.id, proj.cwd)
            } else {
                let proj_name = std::path::Path::new(&target_cwd)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&input.name)
                    .to_string();
                let proj = db
                    .create_project(CreateProjectInput {
                        name: proj_name,
                        cwd: target_cwd,
                        repo_url: None,
                        discord_channel: None,
                        runtime: input.runtime.clone(),
                        wsl_distro: input.wsl_distro.clone(),
                    })
                    .map_err(|e| e.to_string())?;
                (proj.id, proj.cwd)
            }
        }
    };

    let now = now_ts();
    let master_session = {
        let db = db_arc.lock().await;
        db.create_session_for_office(
            CreateSessionInput {
                project_id: project_id.clone(),
                title: format!("{} · {}", input.name, master_name),
                harness: Some(input.master_harness.clone()),
                model: Some(input.master_model.clone()),
                effort: Some(master_effort.clone()),
                account: Some(master_account.clone()),
                permission_mode: Some(master_permission_mode.clone()),
                worktree_path: Some(cwd.clone()),
                branch: Some(String::new()),
                shared_deps: None,
                is_discord: None,
                discord_channel: None,
            },
            &office_id,
        )
        .map_err(|e| e.to_string())?
    };

    let office_row = Office {
        id: office_id.clone(),
        name: input.name.clone(),
        project_id,
        cwd: cwd.clone(),
        master_staff_id: master_staff_id.clone(),
        max_rounds,
        status: "IDLE".to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
        staff: Vec::new(),
        events: Vec::new(),
    };

    let master_staff = OfficeStaff {
        id: master_staff_id,
        office_id: office_id.clone(),
        session_id: master_session.id,
        name: master_name,
        role: "Master Orchestrator".to_string(),
        is_master: true,
        is_dynamic: false,
        harness: input.master_harness,
        model: input.master_model,
        effort: master_effort,
        account: master_account,
        permission_mode: master_permission_mode,
        system_prompt: String::new(),
        isolated_worktree: false,
        worktree_path: cwd,
        branch: String::new(),
        status: "IDLE".to_string(),
        current_task: None,
        avatar_color: "#f59e0b".to_string(),
        desk_order: 0,
        created_at: now.clone(),
    };

    let init_evt = OfficeEvent {
        id: format!("evt-{}", Uuid::new_v4()),
        office_id: office_id.clone(),
        kind: "system".to_string(),
        from_staff_id: None,
        from_name: Some("System".to_string()),
        to_staff_id: None,
        to_name: None,
        summary: format!("Office '{}' created", input.name),
        detail: None,
        timestamp: now,
    };

    let full_office = {
        let db = db_arc.lock().await;
        db.create_office_row(&office_row)
            .map_err(|e| e.to_string())?;
        db.insert_office_staff(&master_staff)
            .map_err(|e| e.to_string())?;
        db.append_office_event(&init_evt)
            .map_err(|e| e.to_string())?;
        db.get_office(&office_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Failed to load newly created office".to_string())?
    };

    emit_office_updated(app, &full_office);
    Ok(full_office)
}

#[tauri::command]
pub async fn create_office(
    app: AppHandle,
    db_state: State<'_, DbState>,
    input: CreateOfficeInput,
) -> Result<Office, String> {
    create_office_inner(Some(&app), &db_state.0, input).await
}

pub async fn delete_office_inner(
    app: Option<&AppHandle>,
    db_arc: &Arc<Mutex<Database>>,
    active_runs: &Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
    active_turns: &Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    id: String,
) -> Result<bool, String> {
    {
        let mut runs = active_runs.lock().await;
        if let Some(tx) = runs.remove(&id) {
            let _ = tx.send(true);
        }
    }

    let office_opt = {
        let db = db_arc.lock().await;
        db.get_office(&id).map_err(|e| e.to_string())?
    };

    if let Some(office) = office_opt {
        {
            let mut active = active_turns.lock().await;
            for staff in &office.staff {
                if let Some(tx) = active.remove(&staff.session_id) {
                    let _ = tx.send(());
                }
            }
        }
        for staff in &office.staff {
            if staff.isolated_worktree
                && !staff.worktree_path.is_empty()
                && staff.worktree_path != office.cwd
            {
                let _ =
                    crate::git::remove_worktree_inner(&db_arc, staff.session_id.clone(), office.cwd.clone()).await;
            }
        }
    }

    let deleted = {
        let db = db_arc.lock().await;
        db.delete_office(&id).map_err(|e| e.to_string())?
    };

    crate::platform::emit_app_and_remote(
        app,
        "office:deleted",
        json!({ "officeId": id }),
    );
    Ok(deleted)
}

#[tauri::command]
pub async fn delete_office(
    app: AppHandle,
    db_state: State<'_, DbState>,
    office_state: State<'_, OfficeState>,
    harness_state: State<'_, HarnessState>,
    id: String,
) -> Result<bool, String> {
    delete_office_inner(
        Some(&app),
        &db_state.0,
        &office_state.active_runs,
        &harness_state.active_turns,
        id,
    )
    .await
}

pub async fn update_office_settings_inner(
    app: Option<&AppHandle>,
    db_arc: &Arc<Mutex<Database>>,
    office_id: String,
    max_rounds: i64,
) -> Result<Office, String> {
    let clamped = max_rounds.max(0);
    let office = {
        let db = db_arc.lock().await;
        db.update_office_max_rounds(&office_id, clamped)
            .map_err(|e| e.to_string())?;
        db.get_office(&office_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Office '{}' not found", office_id))?
    };
    emit_office_updated(app, &office);
    Ok(office)
}

#[tauri::command]
pub async fn update_office_settings(
    app: AppHandle,
    db_state: State<'_, DbState>,
    office_id: String,
    max_rounds: i64,
) -> Result<Office, String> {
    update_office_settings_inner(Some(&app), &db_state.0, office_id, max_rounds).await
}

#[tauri::command]
pub async fn hire_office_staff(
    app: AppHandle,
    db_state: State<'_, DbState>,
    input: HireStaffInput,
) -> Result<OfficeStaff, String> {
    hire_staff_internal(Some(&app), &db_state.0, input).await
}

pub async fn update_office_staff_inner(
    app: Option<&AppHandle>,
    db_arc: &Arc<Mutex<Database>>,
    input: UpdateStaffInput,
) -> Result<OfficeStaff, String> {
    let (updated, updated_office) = {
        let db = db_arc.lock().await;
        let updated = db
            .update_office_staff(input.clone())
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Staff '{}' not found", input.id))?;
        let _ = db.update_session(UpdateSessionInput {
            id: updated.session_id.clone(),
            project_id: None,
            title: input.name,
            status: None,
            harness: input.harness,
            model: input.model,
            effort: input.effort,
            account: input.account,
            permission_mode: input.permission_mode,
            worktree_path: input.worktree_path,
            branch: input.branch,
            shared_deps: None,
            additions: None,
            deletions: None,
            unread_done: None,
        });
        let off = db.get_office(&updated.office_id).map_err(|e| e.to_string())?;
        (updated, off)
    };

    if let Some(ref off) = updated_office {
        emit_office_updated(app, off);
    }
    Ok(updated)
}

#[tauri::command]
pub async fn update_office_staff(
    app: AppHandle,
    db_state: State<'_, DbState>,
    input: UpdateStaffInput,
) -> Result<OfficeStaff, String> {
    update_office_staff_inner(Some(&app), &db_state.0, input).await
}

pub async fn fire_office_staff_inner(
    app: Option<&AppHandle>,
    db_arc: &Arc<Mutex<Database>>,
    active_turns: &Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    id: String,
) -> Result<bool, String> {
    fire_staff_internal(app, db_arc, active_turns, &id).await
}

#[tauri::command]
pub async fn fire_office_staff(
    app: AppHandle,
    db_state: State<'_, DbState>,
    harness_state: State<'_, HarnessState>,
    id: String,
) -> Result<bool, String> {
    fire_office_staff_inner(Some(&app), &db_state.0, &harness_state.active_turns, id).await
}

#[tauri::command]
pub async fn get_office_staff_messages(
    db_state: State<'_, DbState>,
    staff_id: String,
) -> Result<Vec<ChatMessage>, String> {
    let db = db_state.0.lock().await;
    let staff = db
        .get_office_staff(&staff_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Staff '{}' not found", staff_id))?;
    db.list_messages_for_session(&staff.session_id)
        .map_err(|e| e.to_string())
}

pub async fn send_office_message_inner(
    app: Option<AppHandle>,
    db_arc: Arc<Mutex<Database>>,
    active_turns: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    active_runs: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
    office_id: String,
    target_staff_id: String,
    prompt: String,
    attachments: Option<Vec<Value>>,
) -> Result<(), String> {
    let (office, target) = {
        let db = db_arc.lock().await;
        let office = db
            .get_office(&office_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Office '{}' not found", office_id))?;
        let target = office
            .staff
            .iter()
            .find(|s| s.id == target_staff_id)
            .cloned()
            .ok_or_else(|| format!("Staff '{}' not found in office", target_staff_id))?;
        (office, target)
    };

    if target.is_master {
        let (abort_tx, mut abort_rx) = watch::channel(false);
        {
            let mut runs = active_runs.lock().await;
            if let Some(prev_tx) = runs.insert(office_id.clone(), abort_tx) {
                let _ = prev_tx.send(true);
            }
        }

        {
            let db = db_arc.lock().await;
            let _ = db.update_office_status(&office_id, "RUNNING");
            let task_preview = truncate_str(&prompt, 120);
            let _ = db.update_staff_runtime_state(&target.id, "WORKING", Some(&task_preview));
            if let Ok(Some(off)) = db.get_office(&office_id) {
                emit_office_updated(app.as_ref(), &off);
            }
        }

        let app_clone = app.clone();
        tokio::spawn(async move {
            let mut current_prompt = prompt;
            let mut current_attachments = attachments;
            let mut aborted = false;

            let mut round: i64 = 0;
            loop {
                if office.max_rounds > 0 && round >= office.max_rounds {
                    break;
                }
                if *abort_rx.borrow() {
                    aborted = true;
                    break;
                }

                let fresh_office = {
                    let db = db_arc.lock().await;
                    match db.get_office(&office_id) {
                        Ok(Some(o)) => o,
                        _ => break,
                    }
                };
                let Some(master_staff) = fresh_office.staff.iter().find(|s| s.is_master).cloned()
                else {
                    break;
                };

                {
                    let db = db_arc.lock().await;
                    let _ = db.update_staff_runtime_state(
                        &master_staff.id,
                        "WORKING",
                        Some(&format!("Orchestrating round {}", round + 1)),
                    );
                    if let Ok(Some(off)) = db.get_office(&office_id) {
                        emit_office_updated(app_clone.as_ref(), &off);
                    }
                }

                let master_ctx = build_master_system_context(&fresh_office);
                let (comp_tx, comp_rx) = oneshot::channel();
                let turn_start = harness::send_turn_with_options(
                    app_clone.clone(),
                    db_arc.clone(),
                    active_turns.clone(),
                    master_staff.session_id.clone(),
                    current_prompt.clone(),
                    current_attachments.take(),
                    TurnRunOptions {
                        extra_system_context: Some(master_ctx),
                        completion_tx: Some(comp_tx),
                        author_override: Some(master_staff.name.clone()),
                        peer_session_id: None,
                    },
                )
                .await;

                if let Err(err) = turn_start {
                    let evt = OfficeEvent {
                        id: format!("evt-{}", Uuid::new_v4()),
                        office_id: office_id.clone(),
                        kind: "error".to_string(),
                        from_staff_id: Some(master_staff.id.clone()),
                        from_name: Some(master_staff.name.clone()),
                        to_staff_id: None,
                        to_name: None,
                        summary: format!("Master turn failed to start: {}", err),
                        detail: None,
                        timestamp: now_ts(),
                    };
                    let db = db_arc.lock().await;
                    let _ = db.append_office_event(&evt);
                    let _ = db.update_staff_runtime_state(&master_staff.id, "ERROR", None);
                    emit_office_event(app_clone.as_ref(), &evt);
                    break;
                }

                let master_result = tokio::select! {
                    res = comp_rx => res,
                    _ = abort_rx.changed() => {
                        aborted = true;
                        break;
                    }
                };

                let master_msg = match master_result {
                    Ok(Ok(msg)) => msg,
                    Ok(Err(err)) => {
                        let evt = OfficeEvent {
                            id: format!("evt-{}", Uuid::new_v4()),
                            office_id: office_id.clone(),
                            kind: "error".to_string(),
                            from_staff_id: Some(master_staff.id.clone()),
                            from_name: Some(master_staff.name.clone()),
                            to_staff_id: None,
                            to_name: None,
                            summary: format!("Master turn error: {}", err),
                            detail: None,
                            timestamp: now_ts(),
                        };
                        let db = db_arc.lock().await;
                        let _ = db.append_office_event(&evt);
                        let _ = db.update_staff_runtime_state(&master_staff.id, "ERROR", None);
                        emit_office_event(app_clone.as_ref(), &evt);
                        break;
                    }
                    Err(_) => break,
                };

                let actions = parse_office_actions(&master_msg.text);
                let mut delegations: Vec<(Option<String>, Option<String>, String)> = Vec::new();

                for action in actions {
                    match action {
                        OfficeAction::Hire {
                            name,
                            role,
                            harness,
                            model,
                            system_prompt,
                            isolated_worktree,
                            avatar_color,
                        } => {
                            let _ = hire_staff_internal(
                                app_clone.as_ref(),
                                &db_arc,
                                HireStaffInput {
                                    office_id: office_id.clone(),
                                    name,
                                    role,
                                    harness: harness.unwrap_or_else(|| master_staff.harness.clone()),
                                    model: model.unwrap_or_else(|| master_staff.model.clone()),
                                    effort: Some(master_staff.effort.clone()),
                                    account: Some(master_staff.account.clone()),
                                    permission_mode: Some(master_staff.permission_mode.clone()),
                                    system_prompt,
                                    isolated_worktree,
                                    avatar_color,
                                    is_dynamic: Some(true),
                                },
                            )
                            .await;
                        }
                        OfficeAction::Fire {
                            staff_id,
                            staff_name,
                        } => {
                            let resolved_id = {
                                let db = db_arc.lock().await;
                                db.get_office(&office_id).ok().flatten().and_then(|o| {
                                    o.staff.into_iter().find(|s| {
                                        !s.is_master
                                            && (staff_id.as_deref() == Some(s.id.as_str())
                                                || staff_name.as_deref().map(|n| n.eq_ignore_ascii_case(&s.name)).unwrap_or(false))
                                    })
                                    .map(|s| s.id)
                                })
                            };
                            if let Some(sid) = resolved_id {
                                let _ =
                                    fire_staff_internal(app_clone.as_ref(), &db_arc, &active_turns, &sid)
                                        .await;
                            }
                        }
                        OfficeAction::Delegate {
                            staff_id,
                            staff_name,
                            task,
                        } => {
                            delegations.push((staff_id, staff_name, task));
                        }
                    }
                }

                if delegations.is_empty()
                    || (fresh_office.max_rounds > 0 && round + 1 >= fresh_office.max_rounds)
                {
                    break;
                }

                let latest_office = {
                    let db = db_arc.lock().await;
                    match db.get_office(&office_id) {
                        Ok(Some(o)) => o,
                        _ => break,
                    }
                };

                let mut resolved_delegations: Vec<(OfficeStaff, String)> = Vec::new();
                for (sid_opt, sname_opt, task) in delegations {
                    if let Some(staff) = latest_office.staff.iter().find(|s| {
                        !s.is_master
                            && (sid_opt.as_deref() == Some(s.id.as_str())
                                || sname_opt
                                    .as_deref()
                                    .map(|n| n.eq_ignore_ascii_case(&s.name))
                                    .unwrap_or(false))
                    }) {
                        resolved_delegations.push((staff.clone(), task));
                    }
                }

                if resolved_delegations.is_empty() {
                    break;
                }

                let edges: Vec<DelegationEdge> = resolved_delegations
                    .iter()
                    .map(|(staff, task)| DelegationEdge {
                        office_id: office_id.clone(),
                        from_staff_id: master_staff.id.clone(),
                        to_staff_id: staff.id.clone(),
                        task: task.clone(),
                    })
                    .collect();
                emit_office_delegation(app_clone.as_ref(), &office_id, edges);

                {
                    let db = db_arc.lock().await;
                    for (staff, task) in &resolved_delegations {
                        let task_preview = truncate_str(task, 120);
                        let _ = db.update_staff_runtime_state(
                            &staff.id,
                            "WORKING",
                            Some(&task_preview),
                        );
                        let evt = OfficeEvent {
                            id: format!("evt-{}", Uuid::new_v4()),
                            office_id: office_id.clone(),
                            kind: "delegate".to_string(),
                            from_staff_id: Some(master_staff.id.clone()),
                            from_name: Some(master_staff.name.clone()),
                            to_staff_id: Some(staff.id.clone()),
                            to_name: Some(staff.name.clone()),
                            summary: format!("Delegated to {}: {}", staff.name, task_preview),
                            detail: Some(task.clone()),
                            timestamp: now_ts(),
                        };
                        let _ = db.append_office_event(&evt);
                        emit_office_event(app_clone.as_ref(), &evt);
                    }
                    if let Ok(Some(off)) = db.get_office(&office_id) {
                        emit_office_updated(app_clone.as_ref(), &off);
                    }
                }

                let wave_futures = resolved_delegations.into_iter().map(|(staff, task)| {
                    let app_f = app_clone.clone();
                    let db_f = db_arc.clone();
                    let active_f = active_turns.clone();
                    let off_f = latest_office.clone();
                    let master_id = master_staff.id.clone();
                    let master_name = master_staff.name.clone();
                    let off_id = office_id.clone();
                    async move {
                        let staff_ctx = build_staff_system_context(&off_f, &staff);
                        let (comp_tx, comp_rx) = oneshot::channel();
                        let start_res = harness::send_turn_with_options(
                            app_f.clone(),
                            db_f.clone(),
                            active_f,
                            staff.session_id.clone(),
                            task,
                            None,
                            TurnRunOptions {
                                extra_system_context: Some(staff_ctx),
                                completion_tx: Some(comp_tx),
                                author_override: Some(staff.name.clone()),
                                peer_session_id: None,
                            },
                        )
                        .await;

                        let report_text = match start_res {
                            Ok(_) => match comp_rx.await {
                                Ok(Ok(msg)) => {
                                    let db = db_f.lock().await;
                                    let _ = db.update_staff_runtime_state(&staff.id, "DONE", None);
                                    let evt = OfficeEvent {
                                        id: format!("evt-{}", Uuid::new_v4()),
                                        office_id: off_id.clone(),
                                        kind: "report".to_string(),
                                        from_staff_id: Some(staff.id.clone()),
                                        from_name: Some(staff.name.clone()),
                                        to_staff_id: Some(master_id),
                                        to_name: Some(master_name),
                                        summary: format!(
                                            "{} reported: {}",
                                            staff.name,
                                            truncate_str(&msg.text, 100)
                                        ),
                                        detail: Some(msg.text.clone()),
                                        timestamp: now_ts(),
                                    };
                                    let _ = db.append_office_event(&evt);
                                    emit_office_event(app_f.as_ref(), &evt);
                                    msg.text
                                }
                                Ok(Err(err)) => {
                                    let db = db_f.lock().await;
                                    let _ = db.update_staff_runtime_state(&staff.id, "ERROR", None);
                                    format!("ERROR: {}", err)
                                }
                                Err(_) => {
                                    let db = db_f.lock().await;
                                    let _ = db.update_staff_runtime_state(&staff.id, "IDLE", None);
                                    "Cancelled or channel closed.".to_string()
                                }
                            },
                            Err(err) => {
                                let db = db_f.lock().await;
                                let _ = db.update_staff_runtime_state(&staff.id, "ERROR", None);
                                format!("ERROR: {}", err)
                            }
                        };
                        (staff.name, staff.role, report_text)
                    }
                });

                let wave_results = tokio::select! {
                    results = join_all(wave_futures) => results,
                    _ = abort_rx.changed() => {
                        aborted = true;
                        break;
                    }
                };

                emit_office_delegation(app_clone.as_ref(), &office_id, Vec::new());

                let reports_body: Vec<String> = wave_results
                    .into_iter()
                    .map(|(name, role, text)| format!("### {} ({})\n{}", name, role, text))
                    .collect();
                current_prompt = format!(
                    "[STAFF REPORTS — WAVE {}]\n\n{}",
                    round + 1,
                    reports_body.join("\n\n")
                );
                round += 1;
            }

            {
                let mut runs = active_runs.lock().await;
                runs.remove(&office_id);
            }
            emit_office_delegation(app_clone.as_ref(), &office_id, Vec::new());

            let db = db_arc.lock().await;
            let _ = db.update_office_status(&office_id, "IDLE");
            if let Ok(Some(off)) = db.get_office(&office_id) {
                for s in &off.staff {
                    if s.is_master {
                        let status = if aborted { "IDLE" } else { "DONE" };
                        let _ = db.update_staff_runtime_state(&s.id, status, None);
                    }
                }
            }
            if let Ok(Some(off)) = db.get_office(&office_id) {
                emit_office_updated(app_clone.as_ref(), &off);
            }
        });
    } else {
        let task_preview = truncate_str(&prompt, 120);
        {
            let db = db_arc.lock().await;
            let _ = db.update_staff_runtime_state(&target.id, "WORKING", Some(&task_preview));
            if let Ok(Some(off)) = db.get_office(&office_id) {
                emit_office_updated(app.as_ref(), &off);
            }
        }

        let staff_ctx = build_staff_system_context(&office, &target);
        let (comp_tx, comp_rx) = oneshot::channel();
        harness::send_turn_with_options(
            app.clone(),
            db_arc.clone(),
            active_turns,
            target.session_id.clone(),
            prompt.clone(),
            attachments,
            TurnRunOptions {
                extra_system_context: Some(staff_ctx),
                completion_tx: Some(comp_tx),
                author_override: Some(target.name.clone()),
                peer_session_id: None,
            },
        )
        .await?;

        let forward_to_master = prompt.to_ascii_lowercase().contains("@master");
        tokio::spawn(async move {
            let res = comp_rx.await;
            let db = db_arc.lock().await;
            match res {
                Ok(Ok(msg)) => {
                    let _ = db.update_staff_runtime_state(&target.id, "DONE", None);
                    if forward_to_master {
                        if let Some(master) = office.staff.iter().find(|s| s.is_master) {
                            let report_msg = ChatMessage {
                                id: format!("msg-{}", Uuid::new_v4()),
                                role: "user".to_string(),
                                author: Some(target.name.clone()),
                                timestamp: Local::now().format("%H:%M").to_string(),
                                text: format!(
                                    "[Report from {} ({})]\n\n{}",
                                    target.name, target.role, msg.text
                                ),
                                thinking: None,
                                thinking_duration: None,
                                tools: None,
                                subagent: None,
                                widget: None,
                                checkpoint: None,
                            };
                            let _ = db.append_message(&master.session_id, &report_msg);
                            let evt = OfficeEvent {
                                id: format!("evt-{}", Uuid::new_v4()),
                                office_id: office_id.clone(),
                                kind: "report".to_string(),
                                from_staff_id: Some(target.id.clone()),
                                from_name: Some(target.name.clone()),
                                to_staff_id: Some(master.id.clone()),
                                to_name: Some(master.name.clone()),
                                summary: format!(
                                    "{} reported to {}: {}",
                                    target.name,
                                    master.name,
                                    truncate_str(&msg.text, 100)
                                ),
                                detail: Some(msg.text),
                                timestamp: now_ts(),
                            };
                            let _ = db.append_office_event(&evt);
                            emit_office_event(app.as_ref(), &evt);
                        }
                    }
                }
                _ => {
                    let _ = db.update_staff_runtime_state(&target.id, "ERROR", None);
                }
            }
            if let Ok(Some(off)) = db.get_office(&office_id) {
                emit_office_updated(app.as_ref(), &off);
            }
        });
    }

    Ok(())
}

#[tauri::command]
pub async fn send_office_message(
    app: AppHandle,
    db_state: State<'_, DbState>,
    harness_state: State<'_, HarnessState>,
    office_state: State<'_, OfficeState>,
    office_id: String,
    target_staff_id: String,
    prompt: String,
    attachments: Option<Vec<Value>>,
) -> Result<(), String> {
    send_office_message_inner(
        Some(app),
        db_state.0.clone(),
        harness_state.active_turns.clone(),
        office_state.active_runs.clone(),
        office_id,
        target_staff_id,
        prompt,
        attachments,
    )
    .await
}

pub async fn report_staff_to_master_inner(
    app: Option<&AppHandle>,
    db_arc: &Arc<Mutex<Database>>,
    office_id: String,
    staff_id: String,
    note: Option<String>,
) -> Result<OfficeEvent, String> {
    let db = db_arc.lock().await;
    let office = db
        .get_office(&office_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Office '{}' not found", office_id))?;
    let staff = office
        .staff
        .iter()
        .find(|s| s.id == staff_id)
        .cloned()
        .ok_or_else(|| format!("Staff '{}' not found", staff_id))?;
    let master = office
        .staff
        .iter()
        .find(|s| s.is_master)
        .cloned()
        .ok_or_else(|| "Master staff not found".to_string())?;

    let latest_reply = db
        .list_messages_for_session(&staff.session_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .rev()
        .find(|m| m.role == "assistant")
        .map(|m| m.text)
        .unwrap_or_else(|| "(No assistant response yet)".to_string());

    let report_body = match note.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) => format!(
            "[Manual Report from {} ({})]\nNote: {}\n\nLatest Output:\n{}",
            staff.name, staff.role, n, latest_reply
        ),
        None => format!(
            "[Manual Report from {} ({})]\n\nLatest Output:\n{}",
            staff.name, staff.role, latest_reply
        ),
    };

    let report_msg = ChatMessage {
        id: format!("msg-{}", Uuid::new_v4()),
        role: "user".to_string(),
        author: Some(staff.name.clone()),
        timestamp: Local::now().format("%H:%M").to_string(),
        text: report_body.clone(),
        thinking: None,
        thinking_duration: None,
        tools: None,
        subagent: None,
        widget: None,
        checkpoint: None,
    };
    db.append_message(&master.session_id, &report_msg)
        .map_err(|e| e.to_string())?;

    let evt = OfficeEvent {
        id: format!("evt-{}", Uuid::new_v4()),
        office_id: office.id.clone(),
        kind: "report".to_string(),
        from_staff_id: Some(staff.id.clone()),
        from_name: Some(staff.name.clone()),
        to_staff_id: Some(master.id.clone()),
        to_name: Some(master.name.clone()),
        summary: format!(
            "{} reported to {}: {}",
            staff.name,
            master.name,
            truncate_str(&latest_reply, 100)
        ),
        detail: Some(report_body),
        timestamp: now_ts(),
    };
    db.append_office_event(&evt).map_err(|e| e.to_string())?;
    let updated_office = db.get_office(&office.id).map_err(|e| e.to_string())?;

    emit_office_event(app, &evt);
    if let Some(ref off) = updated_office {
        emit_office_updated(app, off);
    }

    Ok(evt)
}

#[tauri::command]
pub async fn report_staff_to_master(
    app: AppHandle,
    db_state: State<'_, DbState>,
    office_id: String,
    staff_id: String,
    note: Option<String>,
) -> Result<OfficeEvent, String> {
    report_staff_to_master_inner(Some(&app), &db_state.0, office_id, staff_id, note).await
}

pub async fn stop_office_run_inner(
    app: Option<&AppHandle>,
    db_arc: &Arc<Mutex<Database>>,
    active_turns: &Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    active_runs: &Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
    office_id: String,
) -> Result<(), String> {
    {
        let mut runs = active_runs.lock().await;
        if let Some(tx) = runs.remove(&office_id) {
            let _ = tx.send(true);
        }
    }

    let office_opt = {
        let db = db_arc.lock().await;
        db.get_office(&office_id).map_err(|e| e.to_string())?
    };

    if let Some(office) = office_opt {
        {
            let mut active = active_turns.lock().await;
            for staff in &office.staff {
                if let Some(tx) = active.remove(&staff.session_id) {
                    let _ = tx.send(());
                }
            }
        }

        let db = db_arc.lock().await;
        let _ = db.update_office_status(&office_id, "IDLE");
        for staff in &office.staff {
            let _ = db.update_staff_runtime_state(&staff.id, "IDLE", None);
            let _ = db.update_session_status(&staff.session_id, "IDLE", Some(false));
        }
        emit_office_delegation(app, &office_id, Vec::new());
        if let Ok(Some(updated)) = db.get_office(&office_id) {
            emit_office_updated(app, &updated);
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn stop_office_run(
    app: AppHandle,
    db_state: State<'_, DbState>,
    harness_state: State<'_, HarnessState>,
    office_state: State<'_, OfficeState>,
    office_id: String,
) -> Result<(), String> {
    stop_office_run_inner(
        Some(&app),
        &db_state.0,
        &harness_state.active_turns,
        &office_state.active_runs,
        office_id,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_block_parallel_delegations_hire_and_fire() {
        let text = r#"
I will hire a QA engineer, fire the old temp, and delegate two tasks in parallel.

```sakana-office
{"action":"hire","name":"QA Lead","role":"Integration tester","harness":"claude-code","model":"claude-sonnet-4-6"}
```

```sakana-office
{"action":"fire","staff_name":"Old Temp"}
```

```sakana-office
{"action":"delegate","staff_id":"staff-1","task":"Build API endpoints"}
```

```sakana-office
{"action":"delegate","staff_name":"Frontend Dev","task":"Build UI components"}
```
"#;
        let actions = parse_office_actions(text);
        assert_eq!(actions.len(), 4);
        assert!(matches!(actions[0], OfficeAction::Hire { ref name, .. } if name == "QA Lead"));
        assert!(matches!(actions[1], OfficeAction::Fire { staff_name: Some(ref n), .. } if n == "Old Temp"));
        assert!(matches!(actions[2], OfficeAction::Delegate { staff_id: Some(ref id), .. } if id == "staff-1"));
        assert!(matches!(actions[3], OfficeAction::Delegate { staff_name: Some(ref n), .. } if n == "Frontend Dev"));
    }

    #[test]
    fn malformed_json_blocks_are_skipped_resiliently() {
        let text = r#"
```sakana-office
{not valid json
```

```sakana-office
{"action":"delegate","staff_id":"s2","task":"Valid task"}
```
"#;
        let actions = parse_office_actions(text);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], OfficeAction::Delegate { ref task, .. } if task == "Valid task"));
    }

    #[test]
    fn db_office_staff_crud_and_session_isolation() {
        let dir = std::env::temp_dir().join(format!("sakana-office-test-{}", Uuid::new_v4()));
        let db = Database::open_in_dir(&dir).expect("open db");

        let proj = db
            .create_project(CreateProjectInput {
                name: "Test Proj".to_string(),
                cwd: ".".to_string(),
                repo_url: None,
                discord_channel: None,
                runtime: None,
                wsl_distro: None,
            })
            .expect("create project");

        let regular_sess = db
            .create_session(CreateSessionInput {
                project_id: proj.id.clone(),
                title: "Regular Session".to_string(),
                harness: None,
                model: None,
                effort: None,
                account: None,
                permission_mode: None,
                worktree_path: None,
                branch: None,
                shared_deps: None,
                is_discord: None,
                discord_channel: None,
            })
            .expect("create regular session");

        let office_id = format!("office-{}", Uuid::new_v4());
        let office_sess = db
            .create_session_for_office(
                CreateSessionInput {
                    project_id: proj.id.clone(),
                    title: "Office Master Session".to_string(),
                    harness: Some("claude-code".to_string()),
                    model: Some("claude-sonnet-4-6".to_string()),
                    effort: None,
                    account: None,
                    permission_mode: None,
                    worktree_path: None,
                    branch: None,
                    shared_deps: None,
                    is_discord: None,
                    discord_channel: None,
                },
                &office_id,
            )
            .expect("create office session");

        let sessions = db.list_sessions(Some(&proj.id)).expect("list sessions");
        assert!(sessions.iter().any(|s| s.id == regular_sess.id));
        assert!(!sessions.iter().any(|s| s.id == office_sess.id));

        let master_id = format!("staff-{}", Uuid::new_v4());
        let office = Office {
            id: office_id.clone(),
            name: "Alpha Office".to_string(),
            project_id: proj.id.clone(),
            cwd: ".".to_string(),
            master_staff_id: master_id.clone(),
            max_rounds: 4,
            status: "IDLE".to_string(),
            created_at: "12:00:00".to_string(),
            updated_at: "12:00:00".to_string(),
            staff: Vec::new(),
            events: Vec::new(),
        };
        db.create_office_row(&office).expect("create office");

        let staff = OfficeStaff {
            id: master_id.clone(),
            office_id: office_id.clone(),
            session_id: office_sess.id.clone(),
            name: "Master".to_string(),
            role: "Orchestrator".to_string(),
            is_master: true,
            is_dynamic: false,
            harness: "claude-code".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            effort: "medium".to_string(),
            account: "default".to_string(),
            permission_mode: "auto".to_string(),
            system_prompt: String::new(),
            isolated_worktree: false,
            worktree_path: ".".to_string(),
            branch: String::new(),
            status: "IDLE".to_string(),
            current_task: None,
            avatar_color: "#f59e0b".to_string(),
            desk_order: 0,
            created_at: "12:00:00".to_string(),
        };
        db.insert_office_staff(&staff).expect("insert staff");

        let loaded = db.get_office(&office_id).expect("get office").expect("exists");
        assert_eq!(loaded.staff.len(), 1);
        assert_eq!(loaded.staff[0].id, master_id);

        assert!(db.delete_office(&office_id).expect("delete office"));
        assert!(db.get_office(&office_id).expect("get office").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
