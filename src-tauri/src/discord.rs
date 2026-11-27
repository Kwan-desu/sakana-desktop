use std::sync::Arc;

use std::path::PathBuf;

use poise::serenity_prelude as serenity;
use serenity::all::{CreateAttachment, CreateMessage, FullEvent, GatewayIntents, ReactionType};
use serenity::gateway::ShardManager;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::db::Database;
use crate::models::{
    AutomationItem, ChatMessage, CreateSessionInput, DiscordSettings, DiscordStatus,
    DiscordUserRule, Project,
};
use crate::DbState;

const DISCORD_TOKEN_KEY: &str = "discord_bot_token";
const DISCORD_SETTINGS_KEY: &str = "discord_bot_settings";

pub fn load_discord_settings(db: &Database) -> DiscordSettings {
    db.get_setting(DISCORD_SETTINGS_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<DiscordSettings>(&raw).ok())
        .unwrap_or_default()
}

pub fn save_discord_settings(db: &Database, settings: &DiscordSettings) -> Result<(), String> {
    let raw = serde_json::to_string(settings).map_err(|e| e.to_string())?;
    db.set_setting(DISCORD_SETTINGS_KEY, &raw)
        .map_err(|e| e.to_string())
}

pub fn resolve_effective_discord_harness(settings: &DiscordSettings) -> String {
    let chosen = settings.default_harness.trim();
    if !chosen.is_empty() && chosen != "auto" {
        return chosen.to_string();
    }
    for candidate in ["agy-cli", "claude-code", "kiro-cli"] {
        if crate::harness::resolve_harness_binary(candidate).is_some() {
            return candidate.to_string();
        }
    }
    "agy-cli".to_string()
}

fn find_automation_by_query<'a>(
    items: &'a [AutomationItem],
    query: &str,
) -> Option<&'a AutomationItem> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return None;
    }
    items
        .iter()
        .find(|a| a.id.eq_ignore_ascii_case(&q) || a.name.eq_ignore_ascii_case(&q))
        .or_else(|| items.iter().find(|a| a.name.to_lowercase().contains(&q)))
}

fn format_automations_list(items: &[AutomationItem]) -> String {
    if items.is_empty() {
        return "No automations configured yet. Use `!auto create <name> | <cron> | <prompt>` or `/automation` to add one.".to_string();
    }
    let lines = items
        .iter()
        .map(|a| {
            let state = if a.enabled { "ON" } else { "OFF" };
            format!(
                "• **{}** (`{}` · `{}` · {} · last: {})",
                a.name, a.kind, a.schedule, state, a.last_run_status
            )
        })
        .collect::<Vec<_>>();
    format!("**Sakana Automations:**\n{}", lines.join("\n"))
}

async fn handle_automation_command(
    app: &AppHandle,
    db: &Arc<Mutex<Database>>,
    role: &str,
    channel_id: u64,
    action: &str,
    arg_text: &str,
) -> Result<String, String> {
    let act = action.trim().to_lowercase();
    if act.is_empty() || act == "list" {
        let items = {
            let db_lock = db.lock().await;
            db_lock.list_automations().unwrap_or_default()
        };
        return Ok(format_automations_list(&items));
    }

    if role != "owner" && role != "friend" {
        return Err("Only Owner or Friend can run or modify automations from Discord.".to_string());
    }

    match act.as_str() {
        "run" => {
            let target_id = {
                let db_lock = db.lock().await;
                let items = db_lock.list_automations().unwrap_or_default();
                find_automation_by_query(&items, arg_text)
                    .map(|a| a.id.clone())
                    .ok_or_else(|| format!("Automation `{}` not found. Use `!auto list` to see available automations.", arg_text.trim()))?
            };
            let updated = crate::automation::execute_automation(db, Some(app), &target_id).await?;
            let summary = updated
                .history
                .first()
                .map(|h| h.summary.as_str())
                .unwrap_or("Completed");
            Ok(format!(
                "Ran **{}** → `{}`\n{}",
                updated.name,
                updated.last_run_status,
                truncate_discord_reply(summary)
            ))
        }
        "toggle" => {
            let (target_id, next_enabled) = {
                let db_lock = db.lock().await;
                let items = db_lock.list_automations().unwrap_or_default();
                let found = find_automation_by_query(&items, arg_text)
                    .ok_or_else(|| format!("Automation `{}` not found.", arg_text.trim()))?;
                (found.id.clone(), !found.enabled)
            };
            let updated = {
                let db_lock = db.lock().await;
                db_lock
                    .toggle_automation(&target_id, next_enabled)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "Automation not found".to_string())?
            };
            let auto_state = app.state::<crate::automation::AutomationState>();
            let _ = crate::automation::sync_scheduler(&auto_state, Arc::clone(db), app.clone()).await;
            Ok(format!(
                "Automation **{}** is now **{}**.",
                updated.name,
                if updated.enabled { "ENABLED" } else { "DISABLED" }
            ))
        }
        "delete" | "rm" | "remove" => {
            let (target_id, target_name) = {
                let db_lock = db.lock().await;
                let items = db_lock.list_automations().unwrap_or_default();
                let found = find_automation_by_query(&items, arg_text)
                    .ok_or_else(|| format!("Automation `{}` not found.", arg_text.trim()))?;
                (found.id.clone(), found.name.clone())
            };
            {
                let db_lock = db.lock().await;
                db_lock
                    .delete_automation(&target_id)
                    .map_err(|e| e.to_string())?;
            };
            let auto_state = app.state::<crate::automation::AutomationState>();
            let _ = crate::automation::sync_scheduler(&auto_state, Arc::clone(db), app.clone()).await;
            Ok(format!("Deleted automation **{}**.", target_name))
        }
        "create" | "add" => {
            // Format: <name> | <schedule> | <prompt>
            let parts: Vec<&str> = arg_text.splitn(3, '|').map(|s| s.trim()).collect();
            if parts.len() < 3 || parts[0].is_empty() || parts[1].is_empty() || parts[2].is_empty() {
                return Err("Usage: `!auto create <name> | <cron schedule> | <prompt>`\nExample: `!auto create Daily Standup | 0 9 * * * | Summarize git commits from the last 24h`".to_string());
            }
            let name = parts[0];
            let schedule = parts[1];
            let raw_prompt = parts[2];
            let kind = if schedule.starts_with('/') { "webhook" } else { "cron" };
            // Route AI output back to this Discord channel automatically unless a prefix is already specified.
            let prompt = if raw_prompt.starts_with("discord:")
                || raw_prompt.starts_with("discord-ai:")
                || raw_prompt.starts_with("shell:")
                || raw_prompt.starts_with("cmd:")
                || raw_prompt.starts_with("notify:")
            {
                raw_prompt.to_string()
            } else {
                format!("discord-ai:channel:{}|{}", channel_id, raw_prompt)
            };
            let created = {
                let db_lock = db.lock().await;
                db_lock
                    .create_automation(name, kind, schedule, &prompt)
                    .map_err(|e| e.to_string())?
            };
            let auto_state = app.state::<crate::automation::AutomationState>();
            let _ = crate::automation::sync_scheduler(&auto_state, Arc::clone(db), app.clone()).await;
            Ok(format!(
                "Created {} automation **{}** (`{}`) — results will post back to this Discord chat.",
                created.kind, created.name, created.schedule
            ))
        }
        _ => Err("Unknown automation action. Try: `list`, `run <name>`, `toggle <name>`, `delete <name>`, or `create <name> | <cron> | <prompt>`.".to_string()),
    }
}

#[derive(Clone)]
pub struct DiscordState {
    pub running: Arc<Mutex<bool>>,
    pub bot_tag: Arc<Mutex<Option<String>>>,
    pub connected_guilds: Arc<Mutex<usize>>,
    pub last_error: Arc<Mutex<Option<String>>>,
    pub shard_manager: Arc<Mutex<Option<Arc<ShardManager>>>>,
}

impl Default for DiscordState {
    fn default() -> Self {
        Self {
            running: Arc::new(Mutex::new(false)),
            bot_tag: Arc::new(Mutex::new(None)),
            connected_guilds: Arc::new(Mutex::new(0)),
            last_error: Arc::new(Mutex::new(None)),
            shard_manager: Arc::new(Mutex::new(None)),
        }
    }
}

struct BotData {
    db: Arc<Mutex<Database>>,
    app: AppHandle,
    running: Arc<Mutex<bool>>,
    bot_tag: Arc<Mutex<Option<String>>>,
    connected_guilds: Arc<Mutex<usize>>,
    last_error: Arc<Mutex<Option<String>>>,
}

type BotError = Box<dyn std::error::Error + Send + Sync>;
type BotContext<'a> = poise::Context<'a, BotData, BotError>;

async fn resolve_user_role(db: &Arc<Mutex<Database>>, user_id: u64) -> String {
    let db_lock = db.lock().await;
    db_lock
        .get_discord_role(&user_id.to_string())
        .ok()
        .flatten()
        .unwrap_or_else(|| "blocked".to_string())
}

fn dm_project_setting_key(user_id: u64) -> String {
    format!("discord_dm_project:{}", user_id)
}

fn find_project_by_query<'a>(projects: &'a [Project], query: &str) -> Option<&'a Project> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return None;
    }
    projects
        .iter()
        .find(|p| p.id.eq_ignore_ascii_case(&q) || p.name.eq_ignore_ascii_case(&q))
        .or_else(|| projects.iter().find(|p| p.name.to_lowercase().contains(&q)))
}

fn truncate_discord_reply(reply: &str) -> String {
    if reply.chars().count() > 1850 {
        format!("{}…", reply.chars().take(1850).collect::<String>())
    } else {
        reply.to_string()
    }
}

const MAX_DISCORD_ATTACHMENT_BYTES: u64 = 20 * 1024 * 1024;

async fn download_discord_attachments(
    attachments: &[serenity::Attachment],
) -> Vec<(String, String, PathBuf)> {
    if attachments.is_empty() {
        return Vec::new();
    }
    let dir = crate::platform::app_data_dir().join("discord-attachments");
    let _ = std::fs::create_dir_all(&dir);
    let mut saved = Vec::new();
    for att in attachments.iter().take(5) {
        if u64::from(att.size) > MAX_DISCORD_ATTACHMENT_BYTES {
            continue;
        }
        let Ok(bytes) = att.download().await else {
            continue;
        };
        let safe_name: String = att
            .filename
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let short_id = &Uuid::new_v4().to_string()[..8];
        let dest = dir.join(format!("{}_{}", short_id, safe_name));
        if std::fs::write(&dest, &bytes).is_ok() {
            let ctype = att
                .content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_string());
            saved.push((att.filename.clone(), ctype, dest));
        }
    }
    saved
}

fn extract_output_files(reply: &str) -> (String, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut kept_lines = Vec::new();
    for line in reply.lines() {
        let trimmed = line.trim();
        let upper = trimmed.to_ascii_uppercase();
        let extracted = if upper.starts_with("[SEND_FILE:") && trimmed.ends_with(']') {
            Some(trimmed[11..trimmed.len() - 1].trim())
        } else if upper.starts_with("[ATTACH:") && trimmed.ends_with(']') {
            Some(trimmed[8..trimmed.len() - 1].trim())
        } else {
            None
        };
        if let Some(raw_path) = extracted {
            let cleaned = raw_path.trim_matches(|c| c == '"' || c == '\'' || c == '`');
            let p = PathBuf::from(cleaned);
            if p.is_file() && files.len() < 5 {
                if let Ok(meta) = std::fs::metadata(&p) {
                    if meta.len() <= MAX_DISCORD_ATTACHMENT_BYTES {
                        files.push(p);
                        continue;
                    }
                }
            }
        }
        kept_lines.push(line);
    }
    (kept_lines.join("\n").trim().to_string(), files)
}

async fn send_discord_reply(
    ctx: &serenity::Context,
    target_msg: &serenity::Message,
    raw_reply: &str,
) {
    let (cleaned, files) = extract_output_files(raw_reply);
    let text = if cleaned.is_empty() && !files.is_empty() {
        String::new()
    } else {
        truncate_discord_reply(&cleaned)
    };
    if files.is_empty() {
        let _ = target_msg.reply(&ctx.http, text).await;
        return;
    }
    let mut builder = CreateMessage::new().reference_message(target_msg);
    if !text.is_empty() {
        builder = builder.content(text);
    }
    for path in &files {
        if let Ok(att) = CreateAttachment::path(path).await {
            builder = builder.add_file(att);
        }
    }
    let _ = target_msg.channel_id.send_message(&ctx.http, builder).await;
}

async fn finish_reaction(
    ctx: &serenity::Context,
    msg: &serenity::Message,
    react_enabled: bool,
    ok: bool,
) {
    if !react_enabled {
        return;
    }
    let _ = msg
        .delete_reaction(&ctx.http, None, ReactionType::Unicode("👀".to_string()))
        .await;
    let emoji = if ok { "✅" } else { "❌" };
    let _ = msg
        .react(&ctx.http, ReactionType::Unicode(emoji.to_string()))
        .await;
}

/// Route a prompt from either a private DM or a bound guild channel/thread into a Sakana session,
/// persist the user message, emit `discord-prompt`, and execute the turn.
async fn dispatch_discord_prompt(
    app: &AppHandle,
    db: &Arc<Mutex<Database>>,
    user_id: u64,
    author_name: &str,
    role: &str,
    is_dm: bool,
    channel_label: &str,
    prompt: &str,
) -> Result<(String, String), String> {
    let (session_id, target_label) = {
        let db_lock = db.lock().await;
        let dc_cfg = load_discord_settings(&db_lock);
        let default_harness = resolve_effective_discord_harness(&dc_cfg);
        let default_model = if dc_cfg.default_model.trim().is_empty() {
            None
        } else {
            Some(dc_cfg.default_model.trim().to_string())
        };
        let default_effort = if dc_cfg.default_effort.trim().is_empty() {
            None
        } else {
            Some(dc_cfg.default_effort.trim().to_string())
        };
        let projects = db_lock.list_projects().unwrap_or_default();
        let all_sessions = db_lock.list_sessions(None).unwrap_or_default();

        let (session, label) = if is_dm {
            // Private DM: allowed for owner, friend, and guest.
            // Resolve user's preferred DM project, or fall back to the first project.
            let saved_pid = db_lock
                .get_setting(&dm_project_setting_key(user_id))
                .ok()
                .flatten();
            let target_proj = saved_pid
                .as_deref()
                .and_then(|pid| projects.iter().find(|p| p.id == pid))
                .or_else(|| projects.first())
                .cloned()
                .ok_or_else(|| {
                    "No Sakana projects exist yet. Create a project in Sakana first.".to_string()
                })?;

            let dm_chan = format!("#dm-{}", user_id);
            let existing_dm = all_sessions.into_iter().find(|s| {
                s.is_discord.unwrap_or(false)
                    && s.project_id == target_proj.id
                    && s.discord_channel
                        .as_deref()
                        .map(|c| c.eq_ignore_ascii_case(&dm_chan))
                        .unwrap_or(false)
            });

            let sess = match existing_dm {
                Some(s) => s,
                None => db_lock
                    .create_session(CreateSessionInput {
                        project_id: target_proj.id.clone(),
                        title: format!("DM · {} ({})", author_name, role),
                        harness: Some(default_harness.clone()),
                        model: default_model.clone(),
                        effort: default_effort.clone(),
                        account: None,
                        permission_mode: Some("yolo".to_string()),
                        worktree_path: Some(target_proj.cwd.clone()),
                        branch: None,
                        shared_deps: None,
                        is_discord: Some(true),
                        discord_channel: Some(dm_chan),
                    })
                    .map_err(|e| e.to_string())?,
            };
            (sess, format!("{} (DM)", target_proj.name))
        } else {
            // Guild channel: first check if a specific thread/session is bound to this channel.
            if let Some(bound_session) = all_sessions.iter().find(|s| {
                s.is_discord.unwrap_or(false)
                    && s.discord_channel
                        .as_deref()
                        .map(|c| c.eq_ignore_ascii_case(channel_label))
                        .unwrap_or(false)
            }) {
                let proj_name = projects
                    .iter()
                    .find(|p| p.id == bound_session.project_id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| bound_session.title.clone());
                (bound_session.clone(), proj_name)
            } else {
                // Otherwise check if a project is bound to this channel.
                let target_project = projects
                    .iter()
                    .find(|p| {
                        p.discord_channel
                            .as_deref()
                            .map(|c| c.eq_ignore_ascii_case(channel_label))
                            .unwrap_or(false)
                    })
                    .cloned()
                    .ok_or_else(|| {
                        format!(
                            "No Sakana project or thread is mapped to Discord channel `{}`. Right-click a project or thread in Sakana to link it, or run `/bind <project>` here.",
                            channel_label
                        )
                    })?;

                let existing_discord = all_sessions.into_iter().find(|s| {
                    s.project_id == target_project.id
                        && s.is_discord.unwrap_or(false)
                        && s.discord_channel
                            .as_deref()
                            .map(|c| c.eq_ignore_ascii_case(channel_label))
                            .unwrap_or(false)
                });

                let sess = match existing_discord {
                    Some(s) => s,
                    None => db_lock
                        .create_session(CreateSessionInput {
                            project_id: target_project.id.clone(),
                            title: format!("Discord {}", channel_label),
                            harness: Some(default_harness.clone()),
                            model: default_model.clone(),
                            effort: default_effort.clone(),
                            account: None,
                            permission_mode: Some("yolo".to_string()),
                            worktree_path: Some(target_project.cwd.clone()),
                            branch: None,
                            shared_deps: None,
                            is_discord: Some(true),
                            discord_channel: Some(channel_label.to_string()),
                        })
                        .map_err(|e| e.to_string())?,
                };
                (sess, target_project.name)
            }
        };

        let user_msg = ChatMessage {
            id: format!("msg-{}", Uuid::new_v4()),
            role: "user".to_string(),
            author: Some(author_name.to_string()),
            timestamp: chrono::Local::now().format("%H:%M").to_string(),
            text: prompt.to_string(),
            thinking: None,
            thinking_duration: None,
            tools: None,
            subagent: None,
            widget: None,
            checkpoint: None,
        };
        db_lock
            .append_message(&session.id, &user_msg)
            .map_err(|e| e.to_string())?;

        (session.id, label)
    };

    let _ = app.emit(
        "discord-prompt",
        serde_json::json!({
            "sessionId": session_id,
            "channel": channel_label.trim_start_matches('#'),
            "author": author_name,
            "prompt": prompt,
        }),
    );

    let read_only = role != "owner" && role != "friend";
    let discord_file_hint = "[Discord Output Hint: If the user asks you to send an image or file back in Discord, include a line `[SEND_FILE: /absolute/path/to/file]` in your response.]";
    let effective_prompt = if read_only {
        format!(
            "[READ-ONLY DISCORD ROLE ({})]: Answer questions and inspect status/code only. Do NOT modify files, run mutating shell commands, or perform destructive actions.\n{}\n\n{}",
            role, discord_file_hint, prompt
        )
    } else {
        format!("{}\n\n{}", discord_file_hint, prompt)
    };

    let reply = crate::harness::run_one_shot_prompt_with_opts(
        app,
        Arc::clone(db),
        &session_id,
        &effective_prompt,
        read_only,
    )
    .await?;

    Ok((target_label, reply))
}

#[poise::command(slash_command, description_localized("en-US", "Show Sakana workspace status"))]
async fn status(ctx: BotContext<'_>) -> Result<(), BotError> {
    let role = resolve_user_role(&ctx.data().db, ctx.author().id.get()).await;
    if role == "blocked" {
        ctx.say("Access denied (blocked by Sakana RBAC).").await?;
        return Ok(());
    }

    let (proj_count, sess_count, discord_bound) = {
        let db_lock = ctx.data().db.lock().await;
        let projects = db_lock.list_projects().unwrap_or_default();
        let sessions = db_lock.list_sessions(None).unwrap_or_default();
        let bound = projects.iter().filter(|p| p.discord_channel.is_some()).count();
        (projects.len(), sessions.len(), bound)
    };

    ctx.say(format!(
        "Sakana Status (role: `{}`)\n• Projects: {} ({} linked to Discord)\n• Sessions: {}",
        role, proj_count, discord_bound, sess_count
    ))
    .await?;
    Ok(())
}

#[poise::command(slash_command, description_localized("en-US", "List Sakana projects and their Discord channels"))]
async fn projects(ctx: BotContext<'_>) -> Result<(), BotError> {
    let role = resolve_user_role(&ctx.data().db, ctx.author().id.get()).await;
    if role == "blocked" {
        ctx.say("Access denied (blocked by Sakana RBAC).").await?;
        return Ok(());
    }

    let lines = {
        let db_lock = ctx.data().db.lock().await;
        let projs = db_lock.list_projects().unwrap_or_default();
        let active_dm_pid = db_lock
            .get_setting(&dm_project_setting_key(ctx.author().id.get()))
            .ok()
            .flatten();
        if projs.is_empty() {
            vec!["No projects configured in Sakana yet.".to_string()]
        } else {
            projs
                .iter()
                .map(|p| {
                    let ch = p
                        .discord_channel
                        .as_deref()
                        .unwrap_or("unlinked");
                    let dm_mark = if active_dm_pid.as_deref() == Some(p.id.as_str()) {
                        " *(active DM)*"
                    } else {
                        ""
                    };
                    format!("• **{}** — `{}`{}", p.name, ch, dm_mark)
                })
                .collect::<Vec<_>>()
        }
    };

    ctx.say(format!("**Sakana Projects:**\n{}", lines.join("\n")))
        .await?;
    Ok(())
}

#[poise::command(slash_command, description_localized("en-US", "Bind this channel (or your DM) to a Sakana project"))]
async fn bind(
    ctx: BotContext<'_>,
    #[description = "Project name or ID"] project: String,
) -> Result<(), BotError> {
    let role = resolve_user_role(&ctx.data().db, ctx.author().id.get()).await;
    if role == "blocked" {
        ctx.say("Access denied (blocked by Sakana RBAC).").await?;
        return Ok(());
    }

    let is_dm = ctx.guild_id().is_none();
    if !is_dm && role != "owner" {
        ctx.say("Only Owner can bind server channels to projects. In a private DM, any allowed user can switch their active project.").await?;
        return Ok(());
    }

    let result = {
        let db_lock = ctx.data().db.lock().await;
        let projs = db_lock.list_projects().unwrap_or_default();
        if let Some(target) = find_project_by_query(&projs, &project) {
            if is_dm {
                let _ = db_lock.set_setting(&dm_project_setting_key(ctx.author().id.get()), &target.id);
                Ok(format!("Private DM is now switched to project **{}** (`{}`).", target.name, target.cwd))
            } else {
                let channel_name = ctx
                    .guild_channel()
                    .await
                    .map(|c| format!("#{}", c.name))
                    .unwrap_or_else(|| format!("#{}", ctx.channel_id()));
                let _ = db_lock.set_project_discord_channel(&target.id, Some(&channel_name));
                Ok(format!("Linked Discord channel `{}` to project **{}**.", channel_name, target.name))
            }
        } else {
            Err(format!("Project `{}` not found. Use `/projects` to see available projects.", project))
        }
    };

    match result {
        Ok(msg) => { ctx.say(msg).await?; }
        Err(msg) => { ctx.say(msg).await?; }
    }
    Ok(())
}

#[poise::command(slash_command, description_localized("en-US", "Send a prompt to the synced Sakana project thread or private DM"))]
async fn ask(
    ctx: BotContext<'_>,
    #[description = "Prompt for Sakana"] prompt: String,
) -> Result<(), BotError> {
    let role = resolve_user_role(&ctx.data().db, ctx.author().id.get()).await;
    if role == "blocked" {
        ctx.say("Access denied (blocked by Sakana RBAC).").await?;
        return Ok(());
    }

    let is_dm = ctx.guild_id().is_none();
    let channel_label = if is_dm {
        format!("#dm-{}", ctx.author().id.get())
    } else {
        ctx.guild_channel()
            .await
            .map(|c| format!("#{}", c.name))
            .unwrap_or_else(|| format!("#{}", ctx.channel_id()))
    };

    ctx.defer().await?;

    match dispatch_discord_prompt(
        &ctx.data().app,
        &ctx.data().db,
        ctx.author().id.get(),
        &ctx.author().name,
        &role,
        is_dm,
        &channel_label,
        &prompt,
    )
    .await
    {
        Ok((_target_label, raw_reply)) => {
            let (cleaned, files) = extract_output_files(&raw_reply);
            let mut reply_builder = poise::CreateReply::default();
            if !cleaned.is_empty() {
                reply_builder = reply_builder.content(truncate_discord_reply(&cleaned));
            }
            for path in &files {
                if let Ok(att) = CreateAttachment::path(path).await {
                    reply_builder = reply_builder.attachment(att);
                }
            }
            ctx.send(reply_builder).await?;
        }
        Err(err) => {
            ctx.say(err).await?;
        }
    }

    Ok(())
}

#[poise::command(slash_command, description_localized("en-US", "List, run, create, toggle, or delete Sakana automations"))]
async fn automation(
    ctx: BotContext<'_>,
    #[description = "Action: list, run, create, toggle, or delete"] action: String,
    #[description = "Automation name/id, or for create: name | cron | prompt"] args: Option<String>,
) -> Result<(), BotError> {
    let role = resolve_user_role(&ctx.data().db, ctx.author().id.get()).await;
    if role == "blocked" {
        ctx.say("Access denied (blocked by Sakana RBAC).").await?;
        return Ok(());
    }

    ctx.defer().await?;
    let arg_text = args.unwrap_or_default();
    match handle_automation_command(
        &ctx.data().app,
        &ctx.data().db,
        &role,
        ctx.channel_id().get(),
        &action,
        &arg_text,
    )
    .await
    {
        Ok(msg) => { ctx.say(msg).await?; }
        Err(err) => { ctx.say(err).await?; }
    }
    Ok(())
}

async fn handle_event(
    ctx: &serenity::Context,
    event: &FullEvent,
    _framework: poise::FrameworkContext<'_, BotData, BotError>,
    data: &BotData,
) -> Result<(), BotError> {
    match event {
        FullEvent::Ready { data_about_bot } => {
            *data.running.lock().await = true;
            *data.bot_tag.lock().await = Some(data_about_bot.user.tag());
            *data.connected_guilds.lock().await = data_about_bot.guilds.len();
            *data.last_error.lock().await = None;
        }
        FullEvent::Message { new_message } => {
            if new_message.author.bot {
                return Ok(());
            }
            let content = new_message.content.trim();
            if content.is_empty() && new_message.attachments.is_empty() {
                return Ok(());
            }

            let user_id = new_message.author.id.get();
            let is_dm = new_message.guild_id.is_none();

            if is_dm {
                let role = resolve_user_role(&data.db, user_id).await;
                if role == "blocked" {
                    let _ = new_message
                        .reply(&ctx.http, "Access denied (blocked by Sakana RBAC).")
                        .await;
                    return Ok(());
                }

                let react_enabled = {
                    let db_lock = data.db.lock().await;
                    load_discord_settings(&db_lock).react_with_emojis
                };

                // Quick DM command: !project <name> to switch active project
                if let Some(rest) = content.strip_prefix("!project") {
                    if react_enabled {
                        let _ = new_message.react(&ctx.http, ReactionType::Unicode("👀".to_string())).await;
                    }
                    let query = rest.trim();
                    let reply = {
                        let db_lock = data.db.lock().await;
                        let projs = db_lock.list_projects().unwrap_or_default();
                        if query.is_empty() {
                            let list = projs
                                .iter()
                                .map(|p| format!("• **{}**", p.name))
                                .collect::<Vec<_>>()
                                .join("\n");
                            format!("Available projects (use `!project <name>` to switch):\n{}", list)
                        } else if let Some(target) = find_project_by_query(&projs, query) {
                            let _ = db_lock.set_setting(&dm_project_setting_key(user_id), &target.id);
                            format!("Switched DM to project **{}** (`{}`).", target.name, target.cwd)
                        } else {
                            format!("Project `{}` not found.", query)
                        }
                    };
                    let _ = new_message.reply(&ctx.http, reply).await;
                    finish_reaction(ctx, new_message, react_enabled, true).await;
                    return Ok(());
                }

                // Quick DM command: !auto / !automation <list|run|toggle|delete|create> ...
                if let Some(rest) = content
                    .strip_prefix("!automation")
                    .or_else(|| content.strip_prefix("!auto"))
                {
                    if react_enabled {
                        let _ = new_message.react(&ctx.http, ReactionType::Unicode("👀".to_string())).await;
                    }
                    let _ = new_message.channel_id.broadcast_typing(&ctx.http).await;
                    let trimmed = rest.trim();
                    let (action, arg_text) = match trimmed.split_once(char::is_whitespace) {
                        Some((a, b)) => (a.trim(), b.trim()),
                        None => (trimmed, ""),
                    };
                    match handle_automation_command(
                        &data.app,
                        &data.db,
                        &role,
                        new_message.channel_id.get(),
                        action,
                        arg_text,
                    )
                    .await
                    {
                        Ok(reply) => {
                            let _ = new_message.reply(&ctx.http, reply).await;
                            finish_reaction(ctx, new_message, react_enabled, true).await;
                        }
                        Err(err) => {
                            let _ = new_message.reply(&ctx.http, err).await;
                            finish_reaction(ctx, new_message, react_enabled, false).await;
                        }
                    }
                    return Ok(());
                }

                if react_enabled {
                    let _ = new_message.react(&ctx.http, ReactionType::Unicode("👀".to_string())).await;
                }
                let _ = new_message.channel_id.broadcast_typing(&ctx.http).await;
                let saved_files = download_discord_attachments(&new_message.attachments).await;
                let full_prompt = if saved_files.is_empty() {
                    content.to_string()
                } else {
                    let list = saved_files
                        .iter()
                        .map(|(name, ctype, path)| {
                            format!("- {} ({}): {}", name, ctype, path.display())
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    if content.is_empty() {
                        format!("The user sent the following Discord attachment(s):\n{}", list)
                    } else {
                        format!("{}\n\n[Discord Attachments saved locally]:\n{}", content, list)
                    }
                };
                let channel_label = format!("#dm-{}", user_id);
                match dispatch_discord_prompt(
                    &data.app,
                    &data.db,
                    user_id,
                    &new_message.author.name,
                    &role,
                    true,
                    &channel_label,
                    &full_prompt,
                )
                .await
                {
                    Ok((_, reply)) => {
                        send_discord_reply(ctx, new_message, &reply).await;
                        finish_reaction(ctx, new_message, react_enabled, true).await;
                    }
                    Err(err) => {
                        let _ = new_message.reply(&ctx.http, err).await;
                        finish_reaction(ctx, new_message, react_enabled, false).await;
                    }
                }
            } else {
                // Guild channel: only respond if the channel is explicitly bound to a Sakana project or thread.
                let channel_name = match new_message.channel_id.to_channel(&ctx.http).await {
                    Ok(serenity::Channel::Guild(gc)) => format!("#{}", gc.name),
                    _ => return Ok(()),
                };

                let is_bound = {
                    let db_lock = data.db.lock().await;
                    let projects = db_lock.list_projects().unwrap_or_default();
                    let sessions = db_lock.list_sessions(None).unwrap_or_default();
                    projects.iter().any(|p| {
                        p.discord_channel
                            .as_deref()
                            .map(|c| c.eq_ignore_ascii_case(&channel_name))
                            .unwrap_or(false)
                    }) || sessions.iter().any(|s| {
                        s.is_discord.unwrap_or(false)
                            && s.discord_channel
                                .as_deref()
                                .map(|c| c.eq_ignore_ascii_case(&channel_name))
                                .unwrap_or(false)
                    })
                };

                if !is_bound {
                    return Ok(());
                }

                let role = resolve_user_role(&data.db, user_id).await;
                if role == "blocked" {
                    return Ok(());
                }

                let react_enabled = {
                    let db_lock = data.db.lock().await;
                    load_discord_settings(&db_lock).react_with_emojis
                };

                // Support !auto / !automation directly inside bound guild channels
                if let Some(rest) = content
                    .strip_prefix("!automation")
                    .or_else(|| content.strip_prefix("!auto"))
                {
                    if react_enabled {
                        let _ = new_message.react(&ctx.http, ReactionType::Unicode("👀".to_string())).await;
                    }
                    let _ = new_message.channel_id.broadcast_typing(&ctx.http).await;
                    let trimmed = rest.trim();
                    let (action, arg_text) = match trimmed.split_once(char::is_whitespace) {
                        Some((a, b)) => (a.trim(), b.trim()),
                        None => (trimmed, ""),
                    };
                    match handle_automation_command(
                        &data.app,
                        &data.db,
                        &role,
                        new_message.channel_id.get(),
                        action,
                        arg_text,
                    )
                    .await
                    {
                        Ok(reply) => {
                            let _ = new_message.reply(&ctx.http, reply).await;
                            finish_reaction(ctx, new_message, react_enabled, true).await;
                        }
                        Err(err) => {
                            let _ = new_message.reply(&ctx.http, err).await;
                            finish_reaction(ctx, new_message, react_enabled, false).await;
                        }
                    }
                    return Ok(());
                }

                if react_enabled {
                    let _ = new_message.react(&ctx.http, ReactionType::Unicode("👀".to_string())).await;
                }
                let _ = new_message.channel_id.broadcast_typing(&ctx.http).await;
                let saved_files = download_discord_attachments(&new_message.attachments).await;
                let full_prompt = if saved_files.is_empty() {
                    content.to_string()
                } else {
                    let list = saved_files
                        .iter()
                        .map(|(name, ctype, path)| {
                            format!("- {} ({}): {}", name, ctype, path.display())
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    if content.is_empty() {
                        format!("The user sent the following Discord attachment(s):\n{}", list)
                    } else {
                        format!("{}\n\n[Discord Attachments saved locally]:\n{}", content, list)
                    }
                };
                match dispatch_discord_prompt(
                    &data.app,
                    &data.db,
                    user_id,
                    &new_message.author.name,
                    &role,
                    false,
                    &channel_name,
                    &full_prompt,
                )
                .await
                {
                    Ok((_, reply)) => {
                        send_discord_reply(ctx, new_message, &reply).await;
                        finish_reaction(ctx, new_message, react_enabled, true).await;
                    }
                    Err(err) => {
                        let _ = new_message.reply(&ctx.http, err).await;
                        finish_reaction(ctx, new_message, react_enabled, false).await;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

async fn build_status(state: &DiscordState, db: &Arc<Mutex<Database>>) -> DiscordStatus {
    let has_token = {
        let db_lock = db.lock().await;
        db_lock
            .get_setting(DISCORD_TOKEN_KEY)
            .ok()
            .flatten()
            .map(|t| !t.trim().is_empty())
            .unwrap_or(false)
            || std::env::var("DISCORD_BOT_TOKEN")
                .map(|t| !t.trim().is_empty())
                .unwrap_or(false)
    };

    DiscordStatus {
        running: *state.running.lock().await,
        bot_tag: state.bot_tag.lock().await.clone(),
        has_token,
        connected_guilds: *state.connected_guilds.lock().await,
        last_error: state.last_error.lock().await.clone(),
    }
}

pub async fn get_discord_status_inner(
    state: &DiscordState,
    db: &Arc<Mutex<Database>>,
) -> Result<DiscordStatus, String> {
    Ok(build_status(state, db).await)
}

#[tauri::command]
pub async fn get_discord_status(
    state: State<'_, DiscordState>,
    db_state: State<'_, DbState>,
) -> Result<DiscordStatus, String> {
    get_discord_status_inner(&state, &db_state.0).await
}

pub async fn start_discord_bot_inner(
    state: &DiscordState,
    db: &Arc<Mutex<Database>>,
    app: Option<AppHandle>,
    token: Option<String>,
) -> Result<DiscordStatus, String> {
    if *state.running.lock().await {
        return Ok(build_status(state, db).await);
    }

    let app = app.ok_or_else(|| "Discord bot requires an AppHandle".to_string())?;

    let resolved_token = if let Some(t) = token {
        let trimmed = t.trim().to_string();
        if trimmed.is_empty() {
            return Err("Discord bot token cannot be empty".to_string());
        }
        let db_lock = db.lock().await;
        db_lock
            .set_setting(DISCORD_TOKEN_KEY, &trimmed)
            .map_err(|e| e.to_string())?;
        trimmed
    } else {
        let db_lock = db.lock().await;
        db_lock
            .get_setting(DISCORD_TOKEN_KEY)
            .map_err(|e| e.to_string())?
            .filter(|t| !t.trim().is_empty())
            .or_else(|| crate::platform::get_env_or_dotenv("DISCORD_BOT_TOKEN"))
            .ok_or_else(|| "No Discord bot token configured".to_string())?
    };

    *state.last_error.lock().await = None;

    let bot_data = BotData {
        db: Arc::clone(db),
        app,
        running: Arc::clone(&state.running),
        bot_tag: Arc::clone(&state.bot_tag),
        connected_guilds: Arc::clone(&state.connected_guilds),
        last_error: Arc::clone(&state.last_error),
    };

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![status(), projects(), bind(), ask(), automation()],
            event_handler: |ctx, event, framework, data| {
                Box::pin(handle_event(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(bot_data)
            })
        })
        .build();

    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::DIRECT_MESSAGES;
    let mut client = serenity::ClientBuilder::new(&resolved_token, intents)
        .framework(framework)
        .await
        .map_err(|e| format!("Failed to initialize Discord client: {e}"))?;

    *state.shard_manager.lock().await = Some(Arc::clone(&client.shard_manager));
    *state.running.lock().await = true;

    let running_flag = Arc::clone(&state.running);
    let last_err = Arc::clone(&state.last_error);
    let shard_slot = Arc::clone(&state.shard_manager);

    tokio::spawn(async move {
        if let Err(err) = client.start().await {
            *last_err.lock().await = Some(err.to_string());
        }
        *running_flag.lock().await = false;
        *shard_slot.lock().await = None;
    });

    Ok(build_status(state, db).await)
}

#[tauri::command]
pub async fn start_discord_bot(
    state: State<'_, DiscordState>,
    db_state: State<'_, DbState>,
    app: AppHandle,
    token: Option<String>,
) -> Result<DiscordStatus, String> {
    start_discord_bot_inner(&state, &db_state.0, Some(app), token).await
}

pub async fn stop_discord_bot_inner(
    state: &DiscordState,
    db: &Arc<Mutex<Database>>,
) -> Result<DiscordStatus, String> {
    if let Some(sm) = state.shard_manager.lock().await.take() {
        sm.shutdown_all().await;
    }
    *state.running.lock().await = false;
    *state.bot_tag.lock().await = None;
    *state.connected_guilds.lock().await = 0;

    Ok(build_status(state, db).await)
}

#[tauri::command]
pub async fn stop_discord_bot(
    state: State<'_, DiscordState>,
    db_state: State<'_, DbState>,
) -> Result<DiscordStatus, String> {
    stop_discord_bot_inner(&state, &db_state.0).await
}

pub async fn upsert_discord_rule_inner(
    db: &Arc<Mutex<Database>>,
    user_id: String,
    label: String,
    role: String,
) -> Result<DiscordUserRule, String> {
    let user_id = user_id.trim().to_string();
    let label = label.trim().to_string();
    let role = role.trim().to_lowercase();

    if user_id.is_empty() || label.is_empty() {
        return Err("User ID and label are required".to_string());
    }
    if !matches!(role.as_str(), "owner" | "friend" | "guest" | "blocked") {
        return Err("Role must be one of: owner, friend, guest, blocked".to_string());
    }

    let db = db.lock().await;
    db.upsert_discord_rule(&user_id, &label, &role)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn upsert_discord_rule(
    db_state: State<'_, DbState>,
    user_id: String,
    label: String,
    role: String,
) -> Result<DiscordUserRule, String> {
    upsert_discord_rule_inner(&db_state.0, user_id, label, role).await
}

pub async fn delete_discord_rule_inner(
    db: &Arc<Mutex<Database>>,
    id: String,
) -> Result<bool, String> {
    let db = db.lock().await;
    db.delete_discord_rule(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_discord_rule(
    db_state: State<'_, DbState>,
    id: String,
) -> Result<bool, String> {
    delete_discord_rule_inner(&db_state.0, id).await
}

pub async fn get_discord_settings_inner(
    db: &Arc<Mutex<Database>>,
) -> Result<DiscordSettings, String> {
    let db_lock = db.lock().await;
    Ok(load_discord_settings(&db_lock))
}

#[tauri::command]
pub async fn get_discord_settings(
    db_state: State<'_, DbState>,
) -> Result<DiscordSettings, String> {
    get_discord_settings_inner(&db_state.0).await
}

pub async fn maybe_auto_start_discord_bot(
    state: &DiscordState,
    db: &Arc<Mutex<Database>>,
    app: AppHandle,
) {
    let (should_start, has_token) = {
        let db_lock = db.lock().await;
        let cfg = load_discord_settings(&db_lock);
        let tok = db_lock
            .get_setting(DISCORD_TOKEN_KEY)
            .ok()
            .flatten()
            .filter(|t| !t.trim().is_empty())
            .or_else(|| crate::platform::get_env_or_dotenv("DISCORD_BOT_TOKEN"))
            .is_some();
        (cfg.auto_start, tok)
    };
    if !should_start || !has_token {
        return;
    }
    if let Err(err) = start_discord_bot_inner(state, db, Some(app), None).await {
        *state.last_error.lock().await = Some(format!("Auto-start failed: {err}"));
    }
}

pub async fn update_discord_settings_inner(
    db: &Arc<Mutex<Database>>,
    settings: DiscordSettings,
) -> Result<DiscordSettings, String> {
    let normalized = DiscordSettings {
        default_harness: if settings.default_harness.trim().is_empty() {
            "auto".to_string()
        } else {
            settings.default_harness.trim().to_string()
        },
        default_model: settings.default_model.trim().to_string(),
        default_effort: settings.default_effort.trim().to_string(),
        react_with_emojis: settings.react_with_emojis,
        auto_start: settings.auto_start,
    };
    let db_lock = db.lock().await;
    save_discord_settings(&db_lock, &normalized)?;
    Ok(normalized)
}

#[tauri::command]
pub async fn update_discord_settings(
    db_state: State<'_, DbState>,
    settings: DiscordSettings,
) -> Result<DiscordSettings, String> {
    update_discord_settings_inner(&db_state.0, settings).await
}
