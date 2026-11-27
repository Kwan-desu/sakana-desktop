use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use reqwest::Client;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, State};
use tokio::sync::Mutex;

pub struct PtySession {
    pub master: Box<dyn MasterPty + Send>,
    pub writer: Arc<std::sync::Mutex<Box<dyn Write + Send>>>,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
}

#[derive(Default)]
pub struct PtyState {
    pub sessions: Arc<Mutex<HashMap<String, PtySession>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PtyDataEvent {
    session_id: String,
    data: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PtyExitEvent {
    session_id: String,
    code: u32,
}

pub async fn pty_spawn_inner(
    app: Option<AppHandle>,
    state: &PtyState,
    session_id: String,
    cwd: String,
    cols: u16,
    rows: u16,
    wsl_distro: Option<String>,
) -> Result<(), String> {
    // Kill existing PTY for this session if present
    {
        let mut map = state.sessions.lock().await;
        if let Some(mut old) = map.remove(&session_id) {
            let _ = old.child.kill();
        }
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: rows.max(4),
            cols: cols.max(10),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to open PTY: {}", e))?;

    let parsed_wsl = crate::platform::parse_wsl_path(&cwd);
    let effective_distro = wsl_distro
        .filter(|s| !s.trim().is_empty())
        .or_else(|| parsed_wsl.as_ref().map(|(d, _)| d.clone()));
    let effective_cwd = parsed_wsl
        .as_ref()
        .map(|(_, p)| p.clone())
        .unwrap_or_else(|| cwd.clone());

    let cmd = if let Some(distro) = effective_distro {
        let mut c = CommandBuilder::new("wsl.exe");
        c.arg("-d");
        c.arg(&distro);
        if !effective_cwd.trim().is_empty() {
            c.arg("--cd");
            c.arg(&effective_cwd);
        }
        c
    } else {
        let mut c = {
            #[cfg(target_os = "windows")]
            {
                let mut ps = CommandBuilder::new("powershell.exe");
                ps.arg("-NoLogo");
                ps
            }
            #[cfg(unix)]
            {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
                CommandBuilder::new(shell)
            }
        };
        if !effective_cwd.trim().is_empty() && std::path::Path::new(&effective_cwd).exists() {
            c.cwd(&effective_cwd);
        }
        c
    };

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn PTY shell: {}", e))?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to clone PTY reader: {}", e))?;
    let writer = Arc::new(std::sync::Mutex::new(
        pair.master
            .take_writer()
            .map_err(|e| format!("Failed to take PTY writer: {}", e))?,
    ));

    {
        let mut map = state.sessions.lock().await;
        map.insert(
            session_id.clone(),
            PtySession {
                master: pair.master,
                writer,
                child,
            },
        );
    }

    let sid = session_id.clone();
    let sessions_arc = Arc::clone(&state.sessions);
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut pending_utf8: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    if !pending_utf8.is_empty() {
                        let tail = String::from_utf8_lossy(&pending_utf8).to_string();
                        crate::platform::emit_app_and_remote(
                            app.as_ref(),
                            "pty:data",
                            PtyDataEvent {
                                session_id: sid.clone(),
                                data: tail,
                            },
                        );
                    }
                    let removed = {
                        let mut map = sessions_arc.blocking_lock();
                        map.remove(&sid)
                    };
                    let code = removed
                        .and_then(|mut sess| sess.child.wait().ok())
                        .map(|s| s.exit_code())
                        .unwrap_or(0);
                    crate::platform::emit_app_and_remote(
                        app.as_ref(),
                        "pty:exit",
                        PtyExitEvent {
                            session_id: sid.clone(),
                            code,
                        },
                    );
                    break;
                }
                Ok(n) => {
                    pending_utf8.extend_from_slice(&buf[..n]);
                    let mut out = String::new();
                    loop {
                        match std::str::from_utf8(&pending_utf8) {
                            Ok(valid) => {
                                out.push_str(valid);
                                pending_utf8.clear();
                                break;
                            }
                            Err(err) => {
                                let valid_up_to = err.valid_up_to();
                                if valid_up_to > 0 {
                                    if let Ok(valid) =
                                        std::str::from_utf8(&pending_utf8[..valid_up_to])
                                    {
                                        out.push_str(valid);
                                    }
                                }
                                if let Some(invalid_len) = err.error_len() {
                                    out.push('\u{FFFD}');
                                    pending_utf8.drain(..valid_up_to + invalid_len);
                                } else {
                                    pending_utf8.drain(..valid_up_to);
                                    break;
                                }
                            }
                        }
                    }
                    if !out.is_empty() {
                        crate::platform::emit_app_and_remote(
                            app.as_ref(),
                            "pty:data",
                            PtyDataEvent {
                                session_id: sid.clone(),
                                data: out,
                            },
                        );
                    }
                }
                Err(_) => {
                    let removed = {
                        let mut map = sessions_arc.blocking_lock();
                        map.remove(&sid)
                    };
                    let code = removed
                        .and_then(|mut sess| sess.child.wait().ok())
                        .map(|s| s.exit_code())
                        .unwrap_or(1);
                    crate::platform::emit_app_and_remote(
                        app.as_ref(),
                        "pty:exit",
                        PtyExitEvent {
                            session_id: sid.clone(),
                            code,
                        },
                    );
                    break;
                }
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn pty_spawn(
    app: AppHandle,
    state: State<'_, PtyState>,
    session_id: String,
    cwd: String,
    cols: u16,
    rows: u16,
    wsl_distro: Option<String>,
) -> Result<(), String> {
    pty_spawn_inner(Some(app), &state, session_id, cwd, cols, rows, wsl_distro).await
}

#[tauri::command]
pub async fn pty_write(
    state: State<'_, PtyState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    let writer = {
        let map = state.sessions.lock().await;
        let session = map
            .get(&session_id)
            .ok_or_else(|| format!("No active PTY for session {}", session_id))?;
        Arc::clone(&session.writer)
    };
    tokio::task::spawn_blocking(move || {
        let mut w = writer
            .lock()
            .map_err(|_| "PTY writer lock poisoned".to_string())?;
        w.write_all(data.as_bytes())
            .map_err(|e| format!("Failed to write to PTY: {}", e))?;
        w.flush()
            .map_err(|e| format!("Failed to flush PTY: {}", e))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(())
}

#[tauri::command]
pub async fn pty_resize(
    state: State<'_, PtyState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let map = state.sessions.lock().await;
    if let Some(session) = map.get(&session_id) {
        session
            .master
            .resize(PtySize {
                rows: rows.max(4),
                cols: cols.max(10),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to resize PTY: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn pty_kill(state: State<'_, PtyState>, session_id: String) -> Result<(), String> {
    let mut map = state.sessions.lock().await;
    if let Some(mut session) = map.remove(&session_id) {
        let _ = session.child.kill();
    }
    Ok(())
}

fn deterministic_nl_fallback(prompt: &str, os: &str, shell: &str) -> String {
    let q = prompt.trim().to_lowercase();
    let is_windows = os.to_lowercase().contains("win") || shell.to_lowercase().contains("powershell");

    if q.contains("git status") || q.contains("changed files") || q.contains("modified files") {
        return "git status -s".to_string();
    }
    if q.contains("git log") || q.contains("recent commits") || q.contains("commit history") {
        return "git log --oneline -n 15".to_string();
    }
    if q.contains("branch") {
        return "git branch -a".to_string();
    }
    if q.contains("port") || q.contains("listening") {
        return if is_windows {
            "Get-NetTCPConnection -State Listen | Select-Object LocalAddress,LocalPort,OwningProcess".to_string()
        } else {
            "lsof -i -P -n | grep LISTEN".to_string()
        };
    }
    if q.contains("process") {
        return if is_windows {
            "Get-Process | Sort-Object CPU -Descending | Select-Object -First 15".to_string()
        } else {
            "ps aux --sort=-%cpu | head -n 15".to_string()
        };
    }
    if q.contains("disk") || q.contains("size") || q.contains("space") {
        return if is_windows {
            "Get-PSDrive -PSProvider FileSystem".to_string()
        } else {
            "df -h".to_string()
        };
    }
    if q.contains("find") && (q.contains("file") || q.contains(".ts") || q.contains(".rs")) {
        return if is_windows {
            "Get-ChildItem -Recurse -File | Select-Object -ExpandProperty FullName".to_string()
        } else {
            "find . -maxdepth 4 -not -path '*/.*'".to_string()
        };
    }
    if q.contains("search") || q.contains("grep") || q.contains("todo") {
        return if is_windows {
            "Select-String -Path * -Pattern 'TODO' -Recurse".to_string()
        } else {
            "grep -rn 'TODO' .".to_string()
        };
    }
    if q.contains("list") || q.contains("files") || q.contains("dir") {
        return if is_windows {
            "Get-ChildItem -Force".to_string()
        } else {
            "ls -la".to_string()
        };
    }

    if is_windows {
        format!("# Suggested PowerShell: Get-Help *{}*", prompt.split_whitespace().next().unwrap_or("command"))
    } else {
        format!("# Suggested shell: apropos \"{}\"", prompt.replace('"', ""))
    }
}

/// Strip markdown code fences and newlines so multi-line LLM output
/// can never auto-execute earlier lines when written to a PTY.
fn sanitize_nl_command(raw: &str) -> String {
    let stripped = raw
        .trim()
        .trim_start_matches("```bash")
        .trim_start_matches("```powershell")
        .trim_start_matches("```sh")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    stripped
        .split(['\r', '\n'])
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .replace(['\r', '\n'], " ")
        .trim()
        .to_string()
}

pub async fn translate_nl_command_inner(
    db: &std::sync::Arc<tokio::sync::Mutex<crate::db::Database>>,
    prompt: String,
    os: String,
    shell: String,
) -> Result<String, String> {
    let clean = prompt.trim().trim_start_matches('?').trim().to_string();
    if clean.is_empty() {
        return Ok(String::new());
    }

    let api_key_and_url = crate::platform::get_env_or_dotenv("OPENROUTER_API_KEY")
        .map(|k| (k, "https://openrouter.ai/api/v1/chat/completions", "openai/gpt-4o-mini"))
        .or_else(|| {
            crate::platform::get_env_or_dotenv("OPENAI_API_KEY")
                .map(|k| (k, "https://api.openai.com/v1/chat/completions", "gpt-4o-mini"))
        });

    let mut translated = None;
    if let Some((key, url, model)) = api_key_and_url {
        let client = Client::new();
        let sys = format!(
            "Translate the user's request into a single executable {} command for {}. Output ONLY the command with no markdown, backticks, or explanation.",
            shell, os
        );
        if let Ok(resp) = client
            .post(url)
            .header("Authorization", format!("Bearer {}", key))
            .json(&json!({
                "model": model,
                "messages": [
                    { "role": "system", "content": sys },
                    { "role": "user", "content": clean }
                ],
                "temperature": 0.1,
                "max_tokens": 120
            }))
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(val) = resp.json::<serde_json::Value>().await {
                    if let Some(cmd) = val["choices"][0]["message"]["content"].as_str() {
                        let sanitized = sanitize_nl_command(cmd);
                        if !sanitized.is_empty() {
                            translated = Some(sanitized);
                        }
                    }
                }
            }
        }
    }

    let cmd = sanitize_nl_command(
        &translated.unwrap_or_else(|| deterministic_nl_fallback(&clean, &os, &shell)),
    );
    let guard_cfg = {
        let db = db.lock().await;
        crate::guard::load_guard_config(&db).unwrap_or_else(|_| crate::models::GuardConfig {
            mode: "guarded".to_string(),
            catastrophic_guardrail: true,
            allowlist_prefixes: Vec::new(),
        })
    };

    match crate::guard::evaluate_command(&guard_cfg, &cmd) {
        crate::guard::GuardDecision::Block(reason) => {
            Ok(format!("# [GUARD BLOCKED: {}] {}", reason, cmd))
        }
        crate::guard::GuardDecision::RequireApproval { reason, risk } => {
            Ok(format!("# [GUARD {} risk: {}] {}", risk, reason, cmd))
        }
        crate::guard::GuardDecision::Allow => Ok(cmd),
    }
}

#[tauri::command]
pub async fn translate_nl_command(
    state: State<'_, crate::DbState>,
    prompt: String,
    os: String,
    shell: String,
) -> Result<String, String> {
    translate_nl_command_inner(&state.0, prompt, os, shell).await
}

#[cfg(test)]
mod tests {
    use super::sanitize_nl_command;

    #[test]
    fn sanitize_nl_command_strips_fences_and_newlines() {
        let multi = "```bash\n\r\nls -la\r\nrm -rf /tmp/evil\n```";
        let sanitized = sanitize_nl_command(multi);
        assert_eq!(sanitized, "ls -la");
        assert!(!sanitized.contains('\n'));
        assert!(!sanitized.contains('\r'));
    }

    #[test]
    fn sanitize_nl_command_handles_cr_only_and_blank_leading_lines() {
        let cr_only = "\r\n  git status -s  \r echo pwned";
        let sanitized = sanitize_nl_command(cr_only);
        assert_eq!(sanitized, "git status -s");
        assert!(!sanitized.contains('\r'));
        assert!(!sanitized.contains('\n'));
    }
}
