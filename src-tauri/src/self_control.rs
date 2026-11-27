use std::collections::HashMap;
use std::sync::Arc;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::automation::AutomationState;
use crate::db::Database;
use crate::DbState;

const CUSTOMIZATION_KEY: &str = "sakana_customization";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomThemePreset {
    pub id: String,
    pub name: String,
    pub base_theme: String,
    #[serde(default)]
    pub css_vars: HashMap<String, String>,
    #[serde(default)]
    pub custom_css: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppCustomization {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub custom_theme_name: Option<String>,
    #[serde(default)]
    pub css_vars: HashMap<String, String>,
    #[serde(default)]
    pub custom_css: String,
    #[serde(default)]
    pub ui_scale: Option<f64>,
    #[serde(default)]
    pub saved_presets: Vec<CustomThemePreset>,
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "paper".to_string()
}

impl Default for AppCustomization {
    fn default() -> Self {
        Self {
            enabled: true,
            theme: "paper".to_string(),
            custom_theme_name: None,
            css_vars: HashMap::new(),
            custom_css: String::new(),
            ui_scale: None,
            saved_presets: Vec::new(),
        }
    }
}

fn is_valid_builtin_theme(t: &str) -> bool {
    matches!(t, "paper" | "noir" | "slate" | "sand")
}

fn sanitize_css_var_name(k: &str) -> Option<String> {
    let trimmed = k.trim();
    let normalized = if trimmed.starts_with("--") {
        trimmed.to_string()
    } else {
        format!("--{}", trimmed)
    };
    if normalized.len() > 64
        || !normalized
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(normalized)
}

fn sanitize_css_var_value(v: &str) -> Option<String> {
    let trimmed = v.trim();
    if trimmed.is_empty() || trimmed.len() > 256 {
        return None;
    }
    if trimmed.contains(';') || trimmed.contains('{') || trimmed.contains('}') {
        return None;
    }
    Some(trimmed.to_string())
}

pub fn load_customization(db: &Database) -> AppCustomization {
    let theme = db.get_theme().unwrap_or_else(|_| "paper".to_string());
    let mut cfg = db
        .get_setting(CUSTOMIZATION_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<AppCustomization>(&raw).ok())
        .unwrap_or_default();
    if is_valid_builtin_theme(&theme) {
        cfg.theme = theme;
    }
    cfg
}

pub fn save_customization(db: &Database, cfg: &AppCustomization) -> Result<(), String> {
    if is_valid_builtin_theme(&cfg.theme) {
        db.set_theme(&cfg.theme).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(cfg).map_err(|e| e.to_string())?;
    db.set_setting(CUSTOMIZATION_KEY, &json)
        .map_err(|e| e.to_string())
}

/// Build system instructions injected into every AI harness turn so any selected AI
/// knows how to customize Sakana Desktop (themes, colors, CSS, UI scale) and manage
/// daily routines / automations.
pub fn build_self_control_prompt(db: &Database) -> String {
    let cfg = load_customization(db);
    if !cfg.enabled {
        return String::new();
    }

    let automations = db.list_automations().unwrap_or_default();
    let auto_summary = if automations.is_empty() {
        "None configured yet".to_string()
    } else {
        automations
            .iter()
            .take(12)
            .map(|a| {
                format!(
                    "- [{}] {} ({}, schedule: `{}`) — {}",
                    a.id,
                    a.name,
                    if a.enabled { "active" } else { "paused" },
                    a.schedule,
                    a.prompt.chars().take(80).collect::<String>()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let preset_names = if cfg.saved_presets.is_empty() {
        "None".to_string()
    } else {
        cfg.saved_presets
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };

    format!(
        "# Sakana Desktop Self-Control Capability\n\
You are running inside **Sakana Desktop** and have live permission to modify Sakana itself (themes, colors, fonts, custom CSS, UI scale) and manage **Daily Routine Automations** (including sending daily Discord messages via webhook or bot channel).\n\n\
Current Sakana state:\n\
- Base theme: `{theme}` (built-ins: `paper`, `noir`, `slate`, `sand`)\n\
- Active custom theme: `{custom_name}`\n\
- Saved custom presets: {presets}\n\
- Active CSS variable overrides: {override_count}\n\
- Existing automations:\n{automations}\n\n\
## How to apply changes to Sakana\n\
ONLY when the user asks you to customize Sakana's appearance/settings or create/manage routines/automations, include one or more fenced `sakana-action` JSON blocks in your reply. Sakana executes them automatically the moment your reply finishes. Do NOT emit `sakana-action` blocks for normal coding questions.\n\n\
### Supported Actions (single JSON object or array of objects inside ```sakana-action ... ```)\n\
1. Switch built-in theme:\n\
```sakana-action\n{{\"action\": \"set_theme\", \"theme\": \"noir\"}}\n```\n\
2. Apply a full custom color theme (and optionally save it as a preset):\n\
```sakana-action\n{{\"action\": \"apply_custom_theme\", \"name\": \"Cyberpunk Neon\", \"baseTheme\": \"noir\", \"savePreset\": true, \"cssVars\": {{\"--bg-sidebar\": \"#0b0714\", \"--bg-app\": \"#110a1f\", \"--bg-canvas\": \"#140d24\", \"--bg-card\": \"#1c1230\", \"--bg-elevated\": \"#261942\", \"--bg-hover\": \"#322154\", \"--text-main\": \"#f5ecff\", \"--text-body\": \"#c9b8e8\", \"--text-muted\": \"#8f7ab8\", \"--border-line\": \"rgba(232,121,249,0.18)\", \"--accent-blue\": \"#ff2e97\", \"--accent-blue-bg\": \"rgba(255,46,151,0.15)\", \"--btn-solid-bg\": \"#ff2e97\", \"--btn-solid-text\": \"#ffffff\"}}, \"customCss\": \"\"}}\n```\n\
3. Tweak specific CSS variables without replacing everything:\n\
```sakana-action\n{{\"action\": \"set_css_vars\", \"cssVars\": {{\"--accent-blue\": \"#10b981\", \"--font-sans\": \"'Inter', sans-serif\"}}}}\n```\n\
4. Inject custom CSS (for layout, borders, animations, fonts, etc.):\n\
```sakana-action\n{{\"action\": \"set_custom_css\", \"css\": \"/* css rules */\", \"append\": false}}\n```\n\
5. Change UI scale (0.85 to 1.25):\n\
```sakana-action\n{{\"action\": \"set_ui_scale\", \"scale\": 1.05}}\n```\n\
6. Reset custom appearance back to clean defaults:\n\
```sakana-action\n{{\"action\": \"reset_appearance\"}}\n```\n\
7. Create a Daily Routine / Cron or Webhook Automation:\n\
```sakana-action\n{{\"action\": \"create_automation\", \"name\": \"Daily Standup\", \"kind\": \"cron\", \"schedule\": \"0 9 * * *\", \"prompt\": \"discord:webhook:https://discord.com/api/webhooks/...|Good morning! Time for daily standup.\"}}\n```\n\
   - Cron schedules use 5-field format in local time (`MIN HOUR DOM MON DOW`, e.g. `0 9 * * *` = daily 9:00 AM, `30 8 * * 1-5` = weekdays 8:30 AM).\n\
   - Supported automation prompt formats:\n\
     - `discord:webhook:<URL>|<message>` — sends message directly to a Discord webhook\n\
     - `discord:channel:<CHANNEL_ID>|<message>` — sends message via Sakana's Discord bot\n\
     - `discord-ai:webhook:<URL>|<AI prompt>` — runs AI first, then posts result to Discord webhook\n\
     - `discord-ai:channel:<CHANNEL_ID>|<AI prompt>` — runs AI first, then posts result to Discord channel\n\
     - `notify:<Title>|<Message>` — triggers a desktop notification\n\
     - `shell:<command>` — runs a shell command\n\
     - Or any plain text AI prompt.\n\
8. Manage automations: `toggle_automation` (`id`, `enabled`), `delete_automation` (`id`), `run_automation` (`id`).\n\n\
### Sakana Design Tokens Reference\n\
`--bg-sidebar`, `--bg-app`, `--bg-canvas`, `--bg-card`, `--bg-elevated`, `--bg-hover`, `--bg-surface`, `--border-line`, `--border-strong`, `--text-main`, `--text-body`, `--text-muted`, `--accent-blue`, `--accent-blue-bg`, `--accent-green`, `--accent-green-bg`, `--accent-yellow`, `--accent-amber`, `--accent-yellow-bg`, `--accent-red`, `--accent-red-bg`, `--accent-discord`, `--accent-discord-bg`, `--btn-solid-bg`, `--btn-solid-text`, `--scrollbar-thumb`, `--syn-kw`, `--syn-str`, `--syn-num`, `--syn-cmt`, `--syn-fn`, `--font-sans`, `--font-mono`.",
        theme = cfg.theme,
        custom_name = cfg.custom_theme_name.as_deref().unwrap_or("None"),
        presets = preset_names,
        override_count = cfg.css_vars.len(),
        automations = auto_summary,
    )
}

/// Extract all ```sakana-action ... ``` blocks from an assistant response and apply them.
pub async fn extract_and_execute_actions(
    db_arc: &Arc<Mutex<Database>>,
    app: Option<&AppHandle>,
    assistant_text: &str,
) -> Vec<String> {
    let enabled = {
        let db = db_arc.lock().await;
        load_customization(&db).enabled
    };
    if !enabled {
        return Vec::new();
    }

    let Ok(re) = Regex::new(r"(?s)```sakana-action\s*\n(.*?)```") else {
        return Vec::new();
    };

    let mut actions: Vec<Value> = Vec::new();
    for cap in re.captures_iter(assistant_text) {
        let Some(body) = cap.get(1) else { continue };
        let raw = body.as_str().trim();
        if raw.is_empty() {
            continue;
        }
        if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
            match parsed {
                Value::Array(arr) => actions.extend(arr),
                Value::Object(_) => actions.push(parsed),
                _ => {}
            }
        }
    }

    if actions.is_empty() {
        return Vec::new();
    }

    let mut applied: Vec<String> = Vec::new();
    let mut customization_changed = false;
    let mut automations_changed = false;
    let mut to_run_ids: Vec<String> = Vec::new();

    for action_val in actions {
        let Some(kind) = action_val.get("action").and_then(|v| v.as_str()) else {
            continue;
        };
        match kind {
            "set_theme" => {
                if let Some(theme) = action_val.get("theme").and_then(|v| v.as_str()) {
                    let t = theme.trim().to_lowercase();
                    if is_valid_builtin_theme(&t) {
                        let db = db_arc.lock().await;
                        let mut cfg = load_customization(&db);
                        cfg.theme = t.clone();
                        cfg.custom_theme_name = None;
                        cfg.css_vars.clear();
                        if save_customization(&db, &cfg).is_ok() {
                            customization_changed = true;
                            applied.push(format!("Switched theme to {}", t));
                        }
                    }
                }
            }
            "apply_custom_theme" => {
                let name = action_val
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("AI Custom Theme")
                    .trim()
                    .to_string();
                let base_theme = action_val
                    .get("baseTheme")
                    .or_else(|| action_val.get("base_theme"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| is_valid_builtin_theme(s));
                let save_preset = action_val
                    .get("savePreset")
                    .or_else(|| action_val.get("save_preset"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let mut vars = HashMap::new();
                if let Some(obj) = action_val
                    .get("cssVars")
                    .or_else(|| action_val.get("css_vars"))
                    .and_then(|v| v.as_object())
                {
                    for (k, v) in obj {
                        if let (Some(nk), Some(nv)) = (
                            sanitize_css_var_name(k),
                            v.as_str().and_then(sanitize_css_var_value),
                        ) {
                            vars.insert(nk, nv);
                        }
                    }
                }
                let custom_css = action_val
                    .get("customCss")
                    .or_else(|| action_val.get("custom_css"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let db = db_arc.lock().await;
                let mut cfg = load_customization(&db);
                if let Some(bt) = base_theme {
                    cfg.theme = bt;
                }
                cfg.custom_theme_name = Some(name.clone());
                cfg.css_vars = vars.clone();
                if let Some(css) = custom_css {
                    cfg.custom_css = css;
                }
                if save_preset && !vars.is_empty() {
                    if let Some(existing) = cfg
                        .saved_presets
                        .iter_mut()
                        .find(|p| p.name.eq_ignore_ascii_case(&name))
                    {
                        existing.base_theme = cfg.theme.clone();
                        existing.css_vars = vars;
                        existing.custom_css = cfg.custom_css.clone();
                    } else {
                        cfg.saved_presets.push(CustomThemePreset {
                            id: format!("theme-{}", Uuid::new_v4()),
                            name: name.clone(),
                            base_theme: cfg.theme.clone(),
                            css_vars: vars,
                            custom_css: cfg.custom_css.clone(),
                        });
                    }
                }
                if save_customization(&db, &cfg).is_ok() {
                    customization_changed = true;
                    applied.push(format!("Applied custom theme \"{}\"", name));
                }
            }
            "set_css_vars" => {
                if let Some(obj) = action_val
                    .get("cssVars")
                    .or_else(|| action_val.get("css_vars"))
                    .and_then(|v| v.as_object())
                {
                    let db = db_arc.lock().await;
                    let mut cfg = load_customization(&db);
                    let mut count = 0usize;
                    for (k, v) in obj {
                        if let Some(nk) = sanitize_css_var_name(k) {
                            if v.is_null() {
                                cfg.css_vars.remove(&nk);
                                count += 1;
                            } else if let Some(nv) = v.as_str().and_then(sanitize_css_var_value) {
                                cfg.css_vars.insert(nk, nv);
                                count += 1;
                            }
                        }
                    }
                    if count > 0 && save_customization(&db, &cfg).is_ok() {
                        customization_changed = true;
                        applied.push(format!("Updated {} style token(s)", count));
                    }
                }
            }
            "set_custom_css" => {
                if let Some(css) = action_val.get("css").and_then(|v| v.as_str()) {
                    let append = action_val
                        .get("append")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let db = db_arc.lock().await;
                    let mut cfg = load_customization(&db);
                    if append && !cfg.custom_css.trim().is_empty() {
                        cfg.custom_css = format!("{}\n{}", cfg.custom_css, css);
                    } else {
                        cfg.custom_css = css.to_string();
                    }
                    if save_customization(&db, &cfg).is_ok() {
                        customization_changed = true;
                        applied.push("Updated custom CSS".to_string());
                    }
                }
            }
            "set_ui_scale" => {
                if let Some(scale) = action_val.get("scale").and_then(|v| v.as_f64()) {
                    let clamped = scale.clamp(0.85, 1.25);
                    let db = db_arc.lock().await;
                    let mut cfg = load_customization(&db);
                    cfg.ui_scale = Some(clamped);
                    if save_customization(&db, &cfg).is_ok() {
                        customization_changed = true;
                        applied.push(format!("Set UI scale to {:.0}%", clamped * 100.0));
                    }
                }
            }
            "reset_appearance" => {
                let db = db_arc.lock().await;
                let mut cfg = load_customization(&db);
                cfg.custom_theme_name = None;
                cfg.css_vars.clear();
                cfg.custom_css.clear();
                cfg.ui_scale = None;
                if save_customization(&db, &cfg).is_ok() {
                    customization_changed = true;
                    applied.push("Reset custom appearance to defaults".to_string());
                }
            }
            "create_automation" => {
                let name = action_val
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let kind = action_val
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .map(|k| {
                        if k.eq_ignore_ascii_case("webhook") {
                            "webhook"
                        } else {
                            "cron"
                        }
                    })
                    .unwrap_or("cron");
                let schedule = action_val
                    .get("schedule")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let prompt = action_val
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !name.is_empty() && !schedule.is_empty() && !prompt.is_empty() {
                    let db = db_arc.lock().await;
                    if db.create_automation(&name, kind, &schedule, &prompt).is_ok() {
                        automations_changed = true;
                        applied.push(format!("Created automation \"{}\" ({})", name, schedule));
                    }
                }
            }
            "toggle_automation" => {
                let id = action_val.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let enabled = action_val
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                if !id.is_empty() {
                    let db = db_arc.lock().await;
                    if let Ok(Some(item)) = db.toggle_automation(id, enabled) {
                        automations_changed = true;
                        applied.push(format!(
                            "{} automation \"{}\"",
                            if enabled { "Enabled" } else { "Paused" },
                            item.name
                        ));
                    }
                }
            }
            "delete_automation" => {
                let id = action_val.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if !id.is_empty() {
                    let db = db_arc.lock().await;
                    if db.delete_automation(id).unwrap_or(false) {
                        automations_changed = true;
                        applied.push(format!("Deleted automation {}", id));
                    }
                }
            }
            "run_automation" => {
                let id = action_val.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if !id.is_empty() {
                    to_run_ids.push(id.to_string());
                }
            }
            _ => {}
        }
    }

    if customization_changed {
        let cfg = {
            let db = db_arc.lock().await;
            load_customization(&db)
        };
        crate::platform::emit_app_and_remote(app, "sakana:customization-updated", &cfg);
    }

    if automations_changed {
        if let Some(app_handle) = app {
            let auto_state = app_handle.state::<AutomationState>();
            let _ = crate::automation::sync_scheduler(
                &auto_state,
                Arc::clone(db_arc),
                app_handle.clone(),
            )
            .await;
        }
        let items = {
            let db = db_arc.lock().await;
            db.list_automations().unwrap_or_default()
        };
        crate::platform::emit_app_and_remote(app, "sakana:automations-updated", &items);
    }

    for id in to_run_ids {
        if let Ok(item) = crate::automation::execute_automation(db_arc, app, &id).await {
            applied.push(format!("Ran automation \"{}\" ({})", item.name, item.last_run_status));
        }
    }

    if !applied.is_empty() {
        crate::platform::emit_app_and_remote(app, "sakana:actions-applied", &applied);
    }

    applied
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerMessageEvent {
    pub from_session_id: String,
    pub to_session_id: String,
    pub from_harness: String,
    pub from_title: String,
    pub message: String,
    pub expect_reply: bool,
}

#[derive(Debug, Clone)]
pub struct ParsedPeerMessage {
    pub message: String,
    pub expect_reply: bool,
}

pub fn build_split_peer_prompt(db: &Database, peer_session_id: &str) -> String {
    let Ok(Some(peer)) = db.get_session(peer_session_id) else {
        return String::new();
    };
    let recent = db.list_recent_messages(peer_session_id, 4).unwrap_or_default();
    let mut snippet = String::new();
    for m in recent.iter().rev().take(3).rev() {
        let preview: String = m.text.chars().take(240).collect();
        let preview = preview.replace('\n', " ");
        snippet.push_str(&format!("- [{}]: {}\n", m.role, preview));
    }
    let context_block = if snippet.is_empty() {
        "(No messages in peer session yet.)".to_string()
    } else {
        snippet
    };
    format!(
        "[Sakana Split-Mode Peer Bridge]\n\
You are open side-by-side in Split Mode with another live AI session:\n\
- Peer Title: {}\n\
- Peer Harness: {}\n\
- Peer Model: {}\n\
- Peer Session ID: {}\n\
Recent peer conversation:\n{}\n\
NATIVE PEER COMMUNICATION: Whenever the user asks you to talk to, ask, coordinate with, review with, or hand off to the other session/agent (or when replying to a Split-Mode message from the peer), emit a fenced `sakana-peer` JSON block:\n\
```sakana-peer\n\
{{\"message\": \"Your message to the other session\", \"expectReply\": true}}\n\
```\n\
Sakana natively delivers this block to the other session immediately and routes their reply back to you if `expectReply` is true. Never tell the user to copy/paste between panes — just emit the `sakana-peer` block directly.",
        peer.title, peer.harness, peer.model, peer.id, context_block
    )
}

pub fn extract_peer_messages(text: &str) -> Vec<ParsedPeerMessage> {
    let mut out = Vec::new();
    let Ok(re) = Regex::new(r"(?s)```sakana-peer\s*\n(.*?)\n```") else {
        return out;
    };
    for cap in re.captures_iter(text) {
        let Some(raw) = cap.get(1).map(|m| m.as_str().trim()) else {
            continue;
        };
        if let Ok(val) = serde_json::from_str::<Value>(raw) {
            let msg = val
                .get("message")
                .or_else(|| val.get("text"))
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if let Some(message) = msg {
                let expect_reply = val
                    .get("expectReply")
                    .or_else(|| val.get("expect_reply"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                out.push(ParsedPeerMessage {
                    message,
                    expect_reply,
                });
            }
        } else if !raw.is_empty() {
            out.push(ParsedPeerMessage {
                message: raw.to_string(),
                expect_reply: true,
            });
        }
    }
    out
}

#[tauri::command]
pub async fn get_app_customization(
    state: State<'_, DbState>,
) -> Result<AppCustomization, String> {
    let db = state.0.lock().await;
    Ok(load_customization(&db))
}

#[tauri::command]
pub async fn update_app_customization(
    state: State<'_, DbState>,
    app: AppHandle,
    customization: AppCustomization,
) -> Result<AppCustomization, String> {
    let mut next = customization;
    if !is_valid_builtin_theme(&next.theme) {
        next.theme = "paper".to_string();
    }
    next.css_vars = next
        .css_vars
        .into_iter()
        .filter_map(|(k, v)| Some((sanitize_css_var_name(&k)?, sanitize_css_var_value(&v)?)))
        .collect();
    if let Some(scale) = next.ui_scale {
        next.ui_scale = Some(scale.clamp(0.85, 1.25));
    }
    {
        let db = state.0.lock().await;
        save_customization(&db, &next)?;
    }
    crate::platform::emit_app_and_remote(Some(&app), "sakana:customization-updated", &next);
    Ok(next)
}

#[tauri::command]
pub async fn reset_app_customization(
    state: State<'_, DbState>,
    app: AppHandle,
) -> Result<AppCustomization, String> {
    let next = {
        let db = state.0.lock().await;
        let mut cfg = load_customization(&db);
        cfg.custom_theme_name = None;
        cfg.css_vars.clear();
        cfg.custom_css.clear();
        cfg.ui_scale = None;
        save_customization(&db, &cfg)?;
        cfg
    };
    crate::platform::emit_app_and_remote(Some(&app), "sakana:customization-updated", &next);
    Ok(next)
}
