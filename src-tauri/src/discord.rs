use std::sync::Arc;

use poise::serenity_prelude as serenity;
use serenity::all::{
    ActionRowComponent, ButtonStyle, CreateActionRow, CreateButton, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateMessage, FullEvent, GatewayIntents, Interaction,
};
use serenity::gateway::ShardManager;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::db::Database;
use crate::models::{ChatMessage, CreateSessionInput, DiscordStatus, DiscordUserRule};
use crate::DbState;

const DISCORD_TOKEN_KEY: &str = "discord_bot_token";

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

#[poise::command(slash_command, description_localized("en-US", "Show Sakana workspace status"))]
async fn status(ctx: BotContext<'_>) -> Result<(), BotError> {
    let role = resolve_user_role(&ctx.data().db, ctx.author().id.get()).await;
    if role == "blocked" {
        ctx.say("Access denied (blocked by Sakana RBAC).").await?;
        return Ok(());
    }

    let (proj_count, sess_count, approval_count) = {
        let db_lock = ctx.data().db.lock().await;
        let projects = db_lock.list_projects().unwrap_or_default();
        let sessions = db_lock.list_sessions(None).unwrap_or_default();
        let approvals = db_lock.list_approvals(None).unwrap_or_default();
        (projects.len(), sessions.len(), approvals.len())
    };

    ctx.say(format!(
        "Sakana Status (role: `{}`)\n• Projects: {}\n• Sessions: {}\n• Pending Approvals: {}",
        role, proj_count, sess_count, approval_count
    ))
    .await?;
    Ok(())
}

#[poise::command(slash_command, description_localized("en-US", "Send a prompt to the synced Sakana project thread"))]
async fn ask(
    ctx: BotContext<'_>,
    #[description = "Prompt for Sakana"] prompt: String,
) -> Result<(), BotError> {
    let role = resolve_user_role(&ctx.data().db, ctx.author().id.get()).await;
    if role == "blocked" {
        ctx.say("Access denied (blocked by Sakana RBAC).").await?;
        return Ok(());
    }

    if let Some(reason) = crate::guard::check_catastrophic(&prompt) {
        ctx.say(format!("Prompt blocked by Sakana Guard: {}", reason)).await?;
        return Ok(());
    }

    let channel_name = ctx
        .guild_channel()
        .await
        .map(|c| format!("#{}", c.name))
        .unwrap_or_else(|| format!("#dm-{}", ctx.channel_id()));

    let (session_id, project_name, pending_approval) = {
        let db_lock = ctx.data().db.lock().await;
        let projects = db_lock.list_projects().unwrap_or_default();
        let target_project = projects
            .iter()
            .find(|p| {
                p.discord_channel
                    .as_deref()
                    .map(|c| c.eq_ignore_ascii_case(&channel_name))
                    .unwrap_or(false)
            })
            .cloned();

        let Some(proj) = target_project else {
            ctx.say(format!(
                "No Sakana project is mapped to Discord channel `{}`. Map a project's Discord channel in Sakana first.",
                channel_name
            ))
            .await?;
            return Ok(());
        };

        let sessions = db_lock.list_sessions(Some(&proj.id)).unwrap_or_default();
        let existing_discord = sessions.into_iter().find(|s| {
            s.is_discord.unwrap_or(false)
                && s.discord_channel
                    .as_deref()
                    .map(|c| c.eq_ignore_ascii_case(&channel_name))
                    .unwrap_or(false)
        });

        let session = match existing_discord {
            Some(s) => s,
            None => db_lock.create_session(CreateSessionInput {
                project_id: proj.id.clone(),
                title: format!("Discord {}", channel_name),
                harness: Some("sakana".to_string()),
                model: None,
                effort: None,
                account: None,
                permission_mode: None,
                worktree_path: Some(proj.cwd.clone()),
                branch: None,
                shared_deps: None,
                is_discord: Some(true),
                discord_channel: Some(channel_name.clone()),
            })?,
        };

        let user_msg = ChatMessage {
            id: format!("msg-{}", Uuid::new_v4()),
            role: "user".to_string(),
            author: Some(ctx.author().name.clone()),
            timestamp: chrono::Local::now().format("%H:%M").to_string(),
            text: prompt.clone(),
            thinking: None,
            thinking_duration: None,
            tools: None,
            subagent: None,
            widget: None,
            checkpoint: None,
        };
        db_lock.append_message(&session.id, &user_msg)?;

        let approval = db_lock
            .list_approvals(Some(&session.id))
            .unwrap_or_default()
            .into_iter()
            .next();

        (session.id, proj.name, approval)
    };

    let _ = ctx.data().app.emit("discord-prompt", serde_json::json!({
        "sessionId": session_id,
        "channel": channel_name,
        "author": ctx.author().name,
        "prompt": prompt,
    }));

    if let Some(approval) = pending_approval {
        let components = vec![CreateActionRow::Buttons(vec![
            CreateButton::new(format!("guard_approve:{}", approval.id))
                .label("Approve")
                .style(ButtonStyle::Success),
            CreateButton::new(format!("guard_deny:{}", approval.id))
                .label("Deny")
                .style(ButtonStyle::Danger),
        ])];
        let msg = CreateMessage::new()
            .content(format!(
                "Queued in **{}** (`{}`). Pending Terminal Guard approval for `{}`:",
                project_name, channel_name, approval.command
            ))
            .components(components);
        let _ = ctx.channel_id().send_message(&ctx.serenity_context().http, msg).await;
        ctx.say("Prompt synced and Terminal Guard prompt posted.").await?;
    } else {
        ctx.defer().await?;
        let read_only = role != "owner" && role != "friend";
        let effective_prompt = if read_only {
            format!(
                "[READ-ONLY DISCORD ROLE ({})]: Answer questions and inspect status/code only. Do NOT modify files, run mutating shell commands, or perform destructive actions.\n\n{}",
                role, prompt
            )
        } else {
            prompt.clone()
        };
        match crate::harness::run_one_shot_prompt_with_opts(
            &ctx.data().app,
            Arc::clone(&ctx.data().db),
            &session_id,
            &effective_prompt,
            read_only,
        )
        .await
        {
            Ok(reply) => {
                let truncated: String = if reply.chars().count() > 1850 {
                    format!("{}…", reply.chars().take(1850).collect::<String>())
                } else {
                    reply
                };
                ctx.say(format!("**{}** (`{}`):\n{}", project_name, channel_name, truncated))
                    .await?;
            }
            Err(err) => {
                ctx.say(format!(
                    "Synced to **{}** (`{}`), but execution failed: {}",
                    project_name, channel_name, err
                ))
                .await?;
            }
        }
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
        FullEvent::InteractionCreate { interaction } => {
            if let Interaction::Component(comp) = interaction {
                let custom_id = comp.data.custom_id.as_str();
                let (approved, approval_id) = if let Some(id) = custom_id.strip_prefix("guard_approve:") {
                    (true, id)
                } else if let Some(id) = custom_id.strip_prefix("guard_deny:") {
                    (false, id)
                } else {
                    return Ok(());
                };

                let role = resolve_user_role(&data.db, comp.user.id.get()).await;
                if role != "owner" && role != "friend" {
                    let _ = comp
                        .create_response(
                            &ctx.http,
                            CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .content("Only Owner or Friend roles can resolve Terminal Guard approvals.")
                                    .ephemeral(true),
                            ),
                        )
                        .await;
                    return Ok(());
                }

                let resolved = {
                    let db_lock = data.db.lock().await;
                    db_lock.resolve_approval(approval_id, approved).ok().flatten()
                };

                let reply_text = if let Some((session_id, command)) = resolved {
                    let _ = data.app.emit(
                        "guard-resolved",
                        serde_json::json!({
                            "approvalId": approval_id,
                            "sessionId": session_id,
                            "approved": approved,
                            "command": command,
                            "resolvedBy": format!("discord:{}", comp.user.name),
                        }),
                    );
                    if approved {
                        format!("Approved `{}`", command)
                    } else {
                        format!("Denied `{}`", command)
                    }
                } else {
                    "Approval request was already resolved or not found.".to_string()
                };

                let _ = comp
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content(reply_text)
                                .ephemeral(true),
                        ),
                    )
                    .await;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Ensure unused imports don't warn if poise macro expands differently
#[allow(dead_code)]
fn _keep_types(_: ActionRowComponent) {}

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

#[tauri::command]
pub async fn get_discord_status(
    state: State<'_, DiscordState>,
    db_state: State<'_, DbState>,
) -> Result<DiscordStatus, String> {
    Ok(build_status(&state, &db_state.0).await)
}

#[tauri::command]
pub async fn start_discord_bot(
    state: State<'_, DiscordState>,
    db_state: State<'_, DbState>,
    app: AppHandle,
    token: Option<String>,
) -> Result<DiscordStatus, String> {
    if *state.running.lock().await {
        return Ok(build_status(&state, &db_state.0).await);
    }

    let resolved_token = if let Some(t) = token {
        let trimmed = t.trim().to_string();
        if trimmed.is_empty() {
            return Err("Discord bot token cannot be empty".to_string());
        }
        let db_lock = db_state.0.lock().await;
        db_lock
            .set_setting(DISCORD_TOKEN_KEY, &trimmed)
            .map_err(|e| e.to_string())?;
        trimmed
    } else {
        let db_lock = db_state.0.lock().await;
        db_lock
            .get_setting(DISCORD_TOKEN_KEY)
            .map_err(|e| e.to_string())?
            .filter(|t| !t.trim().is_empty())
            .or_else(|| crate::platform::get_env_or_dotenv("DISCORD_BOT_TOKEN"))
            .ok_or_else(|| "No Discord bot token configured".to_string())?
    };

    *state.last_error.lock().await = None;

    let bot_data = BotData {
        db: Arc::clone(&db_state.0),
        app,
        running: Arc::clone(&state.running),
        bot_tag: Arc::clone(&state.bot_tag),
        connected_guilds: Arc::clone(&state.connected_guilds),
        last_error: Arc::clone(&state.last_error),
    };

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![status(), ask()],
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

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;
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

    Ok(build_status(&state, &db_state.0).await)
}

#[tauri::command]
pub async fn stop_discord_bot(
    state: State<'_, DiscordState>,
    db_state: State<'_, DbState>,
) -> Result<DiscordStatus, String> {
    if let Some(sm) = state.shard_manager.lock().await.take() {
        sm.shutdown_all().await;
    }
    *state.running.lock().await = false;
    *state.bot_tag.lock().await = None;
    *state.connected_guilds.lock().await = 0;

    Ok(build_status(&state, &db_state.0).await)
}

#[tauri::command]
pub async fn upsert_discord_rule(
    db_state: State<'_, DbState>,
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

    let db = db_state.0.lock().await;
    db.upsert_discord_rule(&user_id, &label, &role)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_discord_rule(
    db_state: State<'_, DbState>,
    id: String,
) -> Result<bool, String> {
    let db = db_state.0.lock().await;
    db.delete_discord_rule(&id).map_err(|e| e.to_string())
}
