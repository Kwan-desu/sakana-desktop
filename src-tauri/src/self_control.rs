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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiControlEvent {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dashboard_subtab: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspector_tab: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspector_open: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidebar_open: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub office_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staff_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

fn is_valid_builtin_theme(t: &str) -> bool {
    matches!(t, "paper" | "noir" | "slate" | "sand")
}

fn is_valid_section(s: &str) -> bool {
    matches!(s, "chat" | "dashboard" | "office")
}

fn is_valid_inspector_tab(s: &str) -> bool {
    matches!(
        s,
        "changes" | "files" | "preview" | "terminal" | "traces" | "team"
    )
}

fn is_valid_browser_action(s: &str) -> bool {
    matches!(s, "back" | "forward" | "reload" | "hide" | "show" | "close")
}

fn sanitize_dashboard_subtab(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return None;
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(trimmed)
}

fn sanitize_browser_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 4096 || trimmed.chars().any(|c| c.is_control()) {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("javascript:")
        || lower.starts_with("vbscript:")
        || lower.starts_with("data:")
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn get_non_empty_str(val: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = val.get(*key).and_then(|v| v.as_str()) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn get_opt_bool(val: &Value, keys: &[&str]) -> Option<bool> {
    for key in keys {
        if let Some(b) = val.get(*key).and_then(|v| v.as_bool()) {
            return Some(b);
        }
    }
    None
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

pub fn sanitize_customization(cfg: &AppCustomization) -> Result<AppCustomization, String> {
    let mut clean = cfg.clone();
    if !is_valid_builtin_theme(&clean.theme) {
        clean.theme = "paper".to_string();
    }
    clean.custom_theme_name = clean.custom_theme_name.and_then(|n| {
        let s: String = n.trim().chars().filter(|c| !c.is_control()).take(80).collect();
        if s.is_empty() { None } else { Some(s) }
    });
    clean.css_vars = clean
        .css_vars
        .into_iter()
        .filter_map(|(k, v)| {
            let k2 = sanitize_css_var_name(&k)?;
            let v2 = sanitize_css_var_value(&v)?;
            Some((k2, v2))
        })
        .collect();
    if clean.custom_css.len() > 16_384 {
        return Err("Custom CSS exceeds 16KB limit".to_string());
    }
    let lower_css = clean.custom_css.to_ascii_lowercase();
    if lower_css.contains("</style")
        || lower_css.contains("@import")
        || lower_css.contains("javascript:")
        || lower_css.contains("expression(")
    {
        return Err("Custom CSS contains disallowed directives".to_string());
    }
    clean.ui_scale = clean.ui_scale.map(|s| s.clamp(0.5, 2.0));
    Ok(clean)
}

pub fn save_customization(db: &Database, cfg: &AppCustomization) -> Result<(), String> {
    let clean = sanitize_customization(cfg)?;
    if is_valid_builtin_theme(&clean.theme) {
        db.set_theme(&clean.theme).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(&clean).map_err(|e| e.to_string())?;
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

    let teams = db.list_offices().unwrap_or_default();
    let teams_summary = if teams.is_empty() {
        "None configured yet".to_string()
    } else {
        teams
            .iter()
            .take(8)
            .map(|t| {
                let members = t
                    .staff
                    .iter()
                    .take(8)
                    .map(|s| format!("{} [{}] ({})", s.name, s.id, s.role))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("- [{}] {} — {}", t.id, t.name, members)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "# Sakana Desktop Self-Control Capability\n\
You are running inside **Sakana Desktop** and have live permission to control Sakana itself: UI navigation, layout, Agent Teams, embedded browser, session sleep/wake, toast notifications, appearance (themes, colors, fonts, custom CSS, UI scale), and **Daily Routine Automations**.\n\n\
Current Sakana state:\n\
- Base theme: `{theme}` (built-ins: `paper`, `noir`, `slate`, `sand`)\n\
- Active custom theme: `{custom_name}`\n\
- Saved custom presets: {presets}\n\
- Active CSS variable overrides: {override_count}\n\
- Agent Teams:\n{teams}\n\
- Existing automations:\n{automations}\n\n\
## How to control Sakana\n\
ONLY when the user asks you to control Sakana's UI, embedded browser, Agent Teams, sessions, appearance, or automations, include one or more fenced `sakana-control` (or legacy `sakana-action`) JSON blocks in your reply. Sakana executes them automatically the moment your reply finishes. Do NOT emit control blocks for normal coding questions.\n\n\
### Supported Actions (single JSON object or array of objects inside ```sakana-control ... ```)\n\
1. **UI Navigation & Layout** (`navigate` or `ui_navigate`):\n\
```sakana-control\n{{\"action\": \"navigate\", \"section\": \"chat\", \"dashboardSubtab\": \"overview\", \"inspectorTab\": \"preview\", \"inspectorOpen\": true, \"sidebarOpen\": true, \"splitView\": false, \"sessionId\": \"...\", \"officeId\": \"...\", \"staffId\": \"...\"}}\n```\n\
   - All fields are optional: `section` (`chat` | `dashboard`), `dashboardSubtab` (`overview` | `harnesses` | `mcp` | `automations` | `discord` | `remote`), `inspectorTab` (`changes` | `files` | `preview` | `terminal` | `traces` | `team`), `inspectorOpen`, `sidebarOpen`, `splitView`, `sessionId`, `officeId`, `staffId`.\n\
2. **Quick Shortcuts**:\n\
   - `switch_team`: `{{\"action\": \"switch_team\", \"officeId\": \"...\", \"staffId\": \"...\"}}` (opens Agent Teams inspector tab & selects team)\n\
   - `switch_agent`: `{{\"action\": \"switch_agent\", \"staffId\": \"...\", \"officeId\": \"...\"}}` (focuses a specific team agent)\n\
   - `open_browser`: `{{\"action\": \"open_browser\", \"url\": \"http://localhost:3000\"}}` (`url` optional; opens Browser/Preview inspector tab)\n\
3. **Embedded Browser Control**:\n\
   - `browser_navigate`: `{{\"action\": \"browser_navigate\", \"url\": \"http://localhost:5173\"}}`\n\
   - `browser_action`: `{{\"action\": \"browser_action\", \"browserAction\": \"reload\"}}` (`back` | `forward` | `reload` | `hide` | `show` | `close`; also accepts `command` or `op`)\n\
   - `browser_eval`: `{{\"action\": \"browser_eval\", \"script\": \"document.title\"}}`\n\
4. **Session & UI Feedback**:\n\
   - `wake_session`: `{{\"action\": \"wake_session\", \"sessionId\": \"...\"}}`\n\
   - `sleep_session`: `{{\"action\": \"sleep_session\", \"sessionId\": \"...\"}}`\n\
   - `show_toast`: `{{\"action\": \"show_toast\", \"message\": \"Build complete!\"}}`\n\
5. **Appearance & Themes**:\n\
   - `set_theme`: `{{\"action\": \"set_theme\", \"theme\": \"noir\"}}` (`paper` | `noir` | `slate` | `sand`)\n\
   - `apply_custom_theme`: `{{\"action\": \"apply_custom_theme\", \"name\": \"Cyberpunk Neon\", \"baseTheme\": \"noir\", \"savePreset\": true, \"cssVars\": {{...}}, \"customCss\": \"\"}}`\n\
   - `set_css_vars`: `{{\"action\": \"set_css_vars\", \"cssVars\": {{\"--accent-blue\": \"#10b981\"}}}}`\n\
   - `set_custom_css`: `{{\"action\": \"set_custom_css\", \"css\": \"/* css */\", \"append\": false}}`\n\
   - `set_ui_scale`: `{{\"action\": \"set_ui_scale\", \"scale\": 1.05}}` (0.85 to 1.25)\n\
   - `reset_appearance`: `{{\"action\": \"reset_appearance\"}}`\n\
6. **Automations**: `create_automation`, `toggle_automation` (`id`, `enabled`), `delete_automation` (`id`), `run_automation` (`id`).\n\n\
### Sakana Design Tokens Reference\n\
`--bg-sidebar`, `--bg-app`, `--bg-canvas`, `--bg-card`, `--bg-elevated`, `--bg-hover`, `--bg-surface`, `--border-line`, `--border-strong`, `--text-main`, `--text-body`, `--text-muted`, `--accent-blue`, `--accent-blue-bg`, `--accent-green`, `--accent-green-bg`, `--accent-yellow`, `--accent-amber`, `--accent-yellow-bg`, `--accent-red`, `--accent-red-bg`, `--accent-discord`, `--accent-discord-bg`, `--btn-solid-bg`, `--btn-solid-text`, `--scrollbar-thumb`, `--syn-kw`, `--syn-str`, `--syn-num`, `--syn-cmt`, `--syn-fn`, `--font-sans`, `--font-mono`.",
        theme = cfg.theme,
        custom_name = cfg.custom_theme_name.as_deref().unwrap_or("None"),
        presets = preset_names,
        override_count = cfg.css_vars.len(),
        teams = teams_summary,
        automations = auto_summary,
    )
}

/// Extract all ```sakana-control ... ``` and ```sakana-action ... ``` blocks from an assistant response and apply them.
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

    let Ok(re) = Regex::new(r"(?s)```sakana-(?:action|control)\s*\n(.*?)```") else {
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
    let mut ui_events: Vec<UiControlEvent> = Vec::new();
    let mut customization_changed = false;
    let mut automations_changed = false;

    for action_val in actions {
        let raw_action = action_val.get("action").and_then(|v| v.as_str());
        let raw_type = action_val
            .get("type")
            .or_else(|| action_val.get("kind"))
            .and_then(|v| v.as_str());
        let (kind, browser_subaction_override) = match (raw_type, raw_action) {
            (Some("browser_action"), Some(sub))
                if is_valid_browser_action(&sub.trim().to_ascii_lowercase()) =>
            {
                ("browser_action", Some(sub.trim().to_ascii_lowercase()))
            }
            (_, Some(act)) => (act, None),
            (Some(t), None) => (t, None),
            (None, None) => continue,
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
                if !name.is_empty() || !prompt.is_empty() {
                    let _ = (kind, schedule);
                    applied.push(format!(
                        "Blocked auto-creation of automation \"{}\" from model output (create manually in Automations)",
                        if name.is_empty() { "unnamed" } else { &name }
                    ));
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
                    applied.push(format!(
                        "Blocked auto-execution of automation {} (run manually from Automations)",
                        id
                    ));
                }
            }
            "navigate" | "ui_navigate" => {
                let section = get_non_empty_str(&action_val, &["section"])
                    .map(|s| s.to_ascii_lowercase())
                    .filter(|s| is_valid_section(s));
                let dashboard_subtab =
                    get_non_empty_str(&action_val, &["dashboardSubtab", "dashboard_subtab"])
                        .and_then(|s| sanitize_dashboard_subtab(&s));
                let inspector_tab =
                    get_non_empty_str(&action_val, &["inspectorTab", "inspector_tab"])
                        .map(|s| s.to_ascii_lowercase())
                        .filter(|s| is_valid_inspector_tab(s));
                let inspector_open =
                    get_opt_bool(&action_val, &["inspectorOpen", "inspector_open"]);
                let sidebar_open = get_opt_bool(&action_val, &["sidebarOpen", "sidebar_open"]);
                let split_view = get_opt_bool(&action_val, &["splitView", "split_view"]);
                let session_id = get_non_empty_str(&action_val, &["sessionId", "session_id"]);
                let office_id = get_non_empty_str(
                    &action_val,
                    &["officeId", "office_id", "teamId", "team_id"],
                );
                let staff_id = get_non_empty_str(
                    &action_val,
                    &["staffId", "staff_id", "agentId", "agent_id"],
                );

                let has_any = section.is_some()
                    || dashboard_subtab.is_some()
                    || inspector_tab.is_some()
                    || inspector_open.is_some()
                    || sidebar_open.is_some()
                    || split_view.is_some()
                    || session_id.is_some()
                    || office_id.is_some()
                    || staff_id.is_some();

                if has_any {
                    let mut parts = Vec::new();
                    if let Some(ref s) = section {
                        parts.push(format!("section={}", s));
                    }
                    if let Some(ref d) = dashboard_subtab {
                        parts.push(format!("dashboard={}", d));
                    }
                    if let Some(ref t) = inspector_tab {
                        parts.push(format!("inspector={}", t));
                    }
                    if let Some(ref id) = session_id {
                        parts.push(format!("session={}", id));
                    }
                    if let Some(ref id) = office_id {
                        parts.push(format!("team={}", id));
                    }
                    if let Some(ref id) = staff_id {
                        parts.push(format!("agent={}", id));
                    }
                    if parts.is_empty() {
                        parts.push("layout updated".to_string());
                    }
                    ui_events.push(UiControlEvent {
                        kind: "navigate".to_string(),
                        section,
                        dashboard_subtab,
                        inspector_tab,
                        inspector_open,
                        sidebar_open,
                        split_view,
                        session_id,
                        office_id,
                        staff_id,
                        ..Default::default()
                    });
                    applied.push(format!("Navigated UI ({})", parts.join(", ")));
                }
            }
            "browser_navigate" => {
                if let Some(url) = get_non_empty_str(&action_val, &["url"])
                    .and_then(|u| sanitize_browser_url(&u))
                {
                    applied.push(format!("Navigated browser to {}", url));
                    ui_events.push(UiControlEvent {
                        kind: "browser_navigate".to_string(),
                        url: Some(url),
                        ..Default::default()
                    });
                }
            }
            "browser_action" => {
                let op = browser_subaction_override.or_else(|| {
                    get_non_empty_str(
                        &action_val,
                        &[
                            "browserAction",
                            "browser_action",
                            "command",
                            "op",
                            "subaction",
                            "value",
                        ],
                    )
                    .map(|s| s.to_ascii_lowercase())
                    .filter(|s| is_valid_browser_action(s))
                });
                if let Some(op) = op {
                    applied.push(format!("Browser action: {}", op));
                    ui_events.push(UiControlEvent {
                        kind: "browser_action".to_string(),
                        browser_action: Some(op.clone()),
                        action: Some(op),
                        ..Default::default()
                    });
                }
            }
            "browser_eval" => {
                if let Some(script) = get_non_empty_str(&action_val, &["script", "code", "js"]) {
                    if script.len() <= 16_384 {
                        applied.push("Executed browser script".to_string());
                        ui_events.push(UiControlEvent {
                            kind: "browser_eval".to_string(),
                            script: Some(script),
                            ..Default::default()
                        });
                    }
                }
            }
            "show_toast" => {
                if let Some(raw_msg) = get_non_empty_str(&action_val, &["message", "text"]) {
                    let msg: String = raw_msg
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(280)
                        .collect();
                    if !msg.is_empty() {
                        applied.push(format!("Toast: {}", msg));
                        ui_events.push(UiControlEvent {
                            kind: "show_toast".to_string(),
                            message: Some(msg),
                            ..Default::default()
                        });
                    }
                }
            }
            "wake_session" | "sleep_session" => {
                if let Some(session_id) = get_non_empty_str(&action_val, &["sessionId", "session_id"])
                {
                    let verb = if kind == "wake_session" {
                        "Woke"
                    } else {
                        "Slept"
                    };
                    applied.push(format!("{} session {}", verb, session_id));
                    ui_events.push(UiControlEvent {
                        kind: kind.to_string(),
                        session_id: Some(session_id),
                        ..Default::default()
                    });
                }
            }
            "switch_team" => {
                let office_id = get_non_empty_str(
                    &action_val,
                    &["officeId", "office_id", "teamId", "team_id", "id"],
                );
                let staff_id = get_non_empty_str(
                    &action_val,
                    &["staffId", "staff_id", "agentId", "agent_id"],
                );
                let label = office_id
                    .as_deref()
                    .map(|id| format!("Switched to Agent Team {}", id))
                    .unwrap_or_else(|| "Opened Agent Teams panel".to_string());
                applied.push(label);
                ui_events.push(UiControlEvent {
                    kind: "navigate".to_string(),
                    section: Some("chat".to_string()),
                    inspector_tab: Some("team".to_string()),
                    inspector_open: Some(true),
                    office_id,
                    staff_id,
                    ..Default::default()
                });
            }
            "switch_agent" => {
                let staff_id = get_non_empty_str(
                    &action_val,
                    &["staffId", "staff_id", "agentId", "agent_id", "id"],
                );
                let office_id = get_non_empty_str(
                    &action_val,
                    &["officeId", "office_id", "teamId", "team_id"],
                );
                if let Some(sid) = staff_id {
                    applied.push(format!("Switched to team agent {}", sid));
                    ui_events.push(UiControlEvent {
                        kind: "navigate".to_string(),
                        section: Some("chat".to_string()),
                        inspector_tab: Some("team".to_string()),
                        inspector_open: Some(true),
                        office_id,
                        staff_id: Some(sid),
                        ..Default::default()
                    });
                }
            }
            "open_browser" => {
                let url = get_non_empty_str(&action_val, &["url"])
                    .and_then(|u| sanitize_browser_url(&u));
                if let Some(u) = url {
                    applied.push(format!("Opened browser at {}", u));
                    ui_events.push(UiControlEvent {
                        kind: "browser_navigate".to_string(),
                        section: Some("chat".to_string()),
                        inspector_tab: Some("preview".to_string()),
                        inspector_open: Some(true),
                        url: Some(u),
                        ..Default::default()
                    });
                } else {
                    applied.push("Opened browser preview".to_string());
                    ui_events.push(UiControlEvent {
                        kind: "navigate".to_string(),
                        section: Some("chat".to_string()),
                        inspector_tab: Some("preview".to_string()),
                        inspector_open: Some(true),
                        ..Default::default()
                    });
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

    for ev in &ui_events {
        crate::platform::emit_app_and_remote(app, "sakana:ui-control", ev);
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
