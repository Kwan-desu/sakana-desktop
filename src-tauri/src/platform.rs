use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Serialize;
use tokio::sync::broadcast;

use crate::models::{DirEntryItem, DirListing, WslDistro};

static APP_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static EVENT_BUS: OnceLock<broadcast::Sender<RemoteEvent>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEvent {
    pub event: String,
    pub payload: serde_json::Value,
}

fn event_sender() -> &'static broadcast::Sender<RemoteEvent> {
    EVENT_BUS.get_or_init(|| {
        let (tx, _rx) = broadcast::channel(512);
        tx
    })
}

pub fn subscribe_events() -> broadcast::Receiver<RemoteEvent> {
    event_sender().subscribe()
}

pub fn broadcast_event<T: Serialize>(event: &str, payload: &T) {
    if let Ok(val) = serde_json::to_value(payload) {
        let _ = event_sender().send(RemoteEvent {
            event: event.to_string(),
            payload: val,
        });
    }
}

pub fn emit_app_and_remote<T: Serialize + Clone>(
    app: Option<&tauri::AppHandle>,
    event: &str,
    payload: T,
) {
    broadcast_event(event, &payload);
    if let Some(handle) = app {
        use tauri::Emitter;
        let _ = handle.emit(event, payload);
    }
}

/// Initialize the canonical per-OS app data directory (e.g. %APPDATA%/ai.sakana.desktop
/// on Windows or ~/.local/share/ai.sakana.desktop on Linux).
pub fn init_app_data_dir(dir: PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    let _ = APP_DATA_DIR.set(dir);
}

/// Returns the canonical app data directory, falling back to OS local data dir / "sakana".
pub fn app_data_dir() -> PathBuf {
    if let Some(dir) = APP_DATA_DIR.get() {
        return dir.clone();
    }
    let fallback = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sakana");
    let _ = std::fs::create_dir_all(&fallback);
    fallback
}

/// Returns the path to the app-managed `.env` secrets file inside `app_data_dir()`.
/// If an existing `.env` is present in the app data dir, returns it; otherwise creates/targets
/// `<app_data_dir>/.env` so installed binaries never try to write to read-only install dirs.
pub fn resolve_env_path() -> PathBuf {
    let dir = app_data_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir.join(".env")
}

/// Read all KEY=VALUE entries from the app-managed `.env` file (and optionally fallback to CWD `.env` if app `.env` does not exist yet).
pub fn read_env_entries() -> Vec<(String, String)> {
    let primary = resolve_env_path();
    let content = if primary.exists() {
        std::fs::read_to_string(&primary).ok()
    } else if let Ok(cwd) = std::env::current_dir() {
        let cwd_env = cwd.join(".env");
        if cwd_env.exists() {
            std::fs::read_to_string(&cwd_env).ok()
        } else {
            None
        }
    } else {
        None
    };

    let Some(content) = content else {
        return Vec::new();
    };

    let mut entries = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            let key = k.trim().to_string();
            let val = v.trim().trim_matches('"').trim_matches('\'').to_string();
            if !key.is_empty() {
                entries.push((key, val));
            }
        }
    }
    entries
}

/// Persist KEY=VALUE entries to the app-managed `.env` file.
pub fn write_env_entries(entries: &[(String, String)]) -> Result<(), String> {
    let path = resolve_env_path();
    let mut out = String::new();
    for (k, v) in entries {
        out.push_str(&format!("{k}={v}\n"));
    }
    std::fs::write(&path, out).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

/// Look up an environment variable from the process environment first, then app-managed `.env`.
pub fn get_env_or_dotenv(key: &str) -> Option<String> {
    if let Ok(val) = std::env::var(key) {
        let trimmed = val.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    for (k, v) in read_env_entries() {
        if k == key {
            let trimmed = v.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

/// Build a cross-platform `tokio::process::Command` that:
/// - Wraps `.cmd` / `.bat` shims via `cmd /C` on Windows
/// - Sets `CREATE_NO_WINDOW` (`0x0800_0000`) on Windows so no console window flashes
pub fn tokio_command(program: &str) -> tokio::process::Command {
    #[cfg(target_os = "windows")]
    {
        let lower = program.to_ascii_lowercase();
        let mut cmd = if lower.ends_with(".cmd") || lower.ends_with(".bat") {
            let mut c = tokio::process::Command::new("cmd");
            c.arg("/C").arg(program);
            c
        } else {
            tokio::process::Command::new(program)
        };
        cmd.creation_flags(0x0800_0000);
        cmd
    }
    #[cfg(not(target_os = "windows"))]
    {
        tokio::process::Command::new(program)
    }
}

/// Build a cross-platform `std::process::Command` with `.cmd`/`.bat` handling and `CREATE_NO_WINDOW` on Windows.
pub fn std_command(program: &str) -> std::process::Command {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let lower = program.to_ascii_lowercase();
        let mut cmd = if lower.ends_with(".cmd") || lower.ends_with(".bat") {
            let mut c = std::process::Command::new("cmd");
            c.arg("/C").arg(program);
            c
        } else {
            std::process::Command::new(program)
        };
        cmd.creation_flags(0x0800_0000);
        cmd
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new(program)
    }
}

/// Resolve the path to the built frontend `dist/` directory for serving over LAN.
pub fn resolve_frontend_dist(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(res) = resource_dir {
        candidates.push(res.join("dist"));
        candidates.push(res.to_path_buf());
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("dist"));
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.join("dist"));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("dist"));
            candidates.push(exe_dir.join("resources").join("dist"));
        }
    }
    candidates
        .into_iter()
        .find(|p| p.join("index.html").is_file())
}

/// Parse Windows WSL UNC paths (`\\wsl.localhost\Ubuntu\home\user\repo` or `\\wsl$\Ubuntu\...`)
/// or explicit `wsl://<distro>/<path>` URIs into `(distro, linux_posix_path)`.
pub fn parse_wsl_path(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("wsl://") {
        let mut parts = rest.splitn(2, '/');
        let distro = parts.next()?.trim();
        if distro.is_empty() {
            return None;
        }
        let linux_path = format!("/{}", parts.next().unwrap_or("").trim_start_matches('/'));
        return Some((distro.to_string(), linux_path));
    }
    let lower = trimmed.to_ascii_lowercase();
    let rest = if lower.starts_with(r"\\wsl.localhost\") {
        &trimmed[r"\\wsl.localhost\".len()..]
    } else if lower.starts_with(r"\\wsl$\") {
        &trimmed[r"\\wsl$\".len()..]
    } else {
        return None;
    };
    let normalized = rest.replace('\\', "/");
    let mut parts = normalized.splitn(2, '/');
    let distro = parts.next()?.trim();
    if distro.is_empty() {
        return None;
    }
    let linux_path = format!("/{}", parts.next().unwrap_or("").trim_start_matches('/'));
    Some((distro.to_string(), linux_path))
}

/// Convert a WSL distro + POSIX path into a Windows UNC path (`\\wsl.localhost\<distro>\...`).
pub fn wsl_to_unc_path(distro: &str, linux_path: &str) -> PathBuf {
    let clean = linux_path.trim().trim_start_matches('/');
    if clean.is_empty() {
        PathBuf::from(format!(r"\\wsl.localhost\{}", distro))
    } else {
        PathBuf::from(format!(
            r"\\wsl.localhost\{}\{}",
            distro,
            clean.replace('/', r"\")
        ))
    }
}

/// Encode a POSIX `cwd` with an optional WSL distro into `wsl://<distro>/<path>`
/// unless `cwd` is already a WSL URI/UNC path or `distro` is empty/None.
pub fn encode_wsl_path(distro: Option<&str>, cwd: &str) -> String {
    if let Some(d) = distro.map(str::trim).filter(|s| !s.is_empty()) {
        if parse_wsl_path(cwd).is_none() {
            return format!("wsl://{}/{}", d, cwd.trim_start_matches('/'));
        }
    }
    cwd.to_string()
}

/// Build a `std::process::Command` that runs inside a WSL2 distro.
pub fn wsl_std_command(distro: &str, cwd: Option<&str>, program: &str, args: &[&str]) -> std::process::Command {
    let mut cmd = std_command("wsl.exe");
    cmd.arg("-d").arg(distro);
    if let Some(dir) = cwd {
        if !dir.trim().is_empty() {
            cmd.arg("--cd").arg(dir);
        }
    }
    cmd.arg("--").arg(program).args(args);
    cmd
}

/// Build a `tokio::process::Command` that runs a login shell command inside a WSL2 distro.
pub fn wsl_tokio_shell(distro: &str, cwd: &str, shell_cmd: &str) -> tokio::process::Command {
    let mut cmd = tokio_command("wsl.exe");
    cmd.arg("-d").arg(distro);
    if !cwd.trim().is_empty() {
        cmd.arg("--cd").arg(cwd);
    }
    cmd.arg("--").arg("bash").arg("-lc").arg(shell_cmd);
    cmd
}

/// Decode output from `wsl.exe` which emits UTF-16LE on Windows.
fn decode_wsl_output(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes.iter().skip(1).step_by(2).take(16).filter(|&&b| b == 0).count() > 4 {
        let mut u16s = Vec::with_capacity(bytes.len() / 2);
        for chunk in bytes.chunks_exact(2) {
            u16s.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }
        String::from_utf16_lossy(&u16s)
            .trim_start_matches('\u{feff}')
            .replace('\0', "")
    } else {
        String::from_utf8_lossy(bytes).replace('\0', "")
    }
}

pub fn detect_wsl_distros() -> Vec<WslDistro> {
    #[cfg(not(target_os = "windows"))]
    {
        Vec::new()
    }
    #[cfg(target_os = "windows")]
    {
        let Ok(out) = std_command("wsl.exe").args(["-l", "-v"]).output() else {
            return Vec::new();
        };
        if !out.status.success() {
            return Vec::new();
        }
        let text = decode_wsl_output(&out.stdout);
        let mut distros = Vec::new();
        for (idx, raw_line) in text.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || (idx == 0 && line.to_ascii_uppercase().contains("NAME")) {
                continue;
            }
            let is_default = line.starts_with('*');
            let clean = line.trim_start_matches('*').trim();
            let cols: Vec<&str> = clean.split_whitespace().collect();
            if cols.len() >= 3 {
                let version = cols[cols.len() - 1].parse::<u32>().unwrap_or(2);
                let state = cols[cols.len() - 2].to_string();
                let name = cols[..cols.len() - 2].join(" ");
                if !name.is_empty() && !name.starts_with("docker-desktop") {
                    distros.push(WslDistro {
                        name,
                        state,
                        version,
                        is_default,
                    });
                }
            }
        }
        distros
    }
}

pub fn list_directories_impl(
    path: Option<&str>,
    wsl_distro: Option<&str>,
) -> Result<DirListing, String> {
    if let Some(distro) = wsl_distro.filter(|s| !s.trim().is_empty()) {
        let target = match path.map(|s| s.trim()).filter(|s| !s.is_empty()) {
            Some(p) => p.to_string(),
            None => {
                let out = wsl_std_command(distro, None, "bash", &["-lc", "printf '%s' \"$HOME\""])
                    .output()
                    .map_err(|e| format!("Failed to query WSL home: {e}"))?;
                let home = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if home.is_empty() { "/".to_string() } else { home }
            }
        };
        let unc = wsl_to_unc_path(distro, &target);
        let mut entries = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&unc) {
            for entry in rd.flatten() {
                let ft = match entry.file_type() {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                if !ft.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let child_posix = if target == "/" {
                    format!("/{name}")
                } else {
                    format!("{}/{name}", target.trim_end_matches('/'))
                };
                let is_git_repo = entry.path().join(".git").exists();
                entries.push(DirEntryItem {
                    name,
                    path: child_posix,
                    is_dir: true,
                    is_git_repo,
                });
            }
        } else {
            let script = format!(
                "cd {} 2>/dev/null && for d in */; do [ -d \"$d\" ] || continue; d=${{d%/}}; [ -e \"$d/.git\" ] && g=1 || g=0; printf '%s\t%s\n' \"$d\" \"$g\"; done",
                shell_escape_single(&target)
            );
            let out = wsl_std_command(distro, None, "bash", &["-lc", &script])
                .output()
                .map_err(|e| format!("Failed to list WSL dir: {e}"))?;
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if let Some((name, g)) = line.split_once('\t') {
                    let child_posix = if target == "/" {
                        format!("/{name}")
                    } else {
                        format!("{}/{name}", target.trim_end_matches('/'))
                    };
                    entries.push(DirEntryItem {
                        name: name.to_string(),
                        path: child_posix,
                        is_dir: true,
                        is_git_repo: g.trim() == "1",
                    });
                }
            }
        }
        entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let parent = if target == "/" {
            None
        } else {
            let trimmed = target.trim_end_matches('/');
            match trimmed.rfind('/') {
                Some(0) => Some("/".to_string()),
                Some(idx) => Some(trimmed[..idx].to_string()),
                None => Some("/".to_string()),
            }
        };
        return Ok(DirListing {
            path: target,
            parent,
            entries,
            os: "linux".to_string(),
            wsl_distro: Some(distro.to_string()),
        });
    }

    let target = match path.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => dirs::home_dir()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    let canonical = std::fs::canonicalize(&target).unwrap_or(target);
    let mut entries = Vec::new();
    let rd = std::fs::read_dir(&canonical)
        .map_err(|e| format!("Failed to read directory {}: {e}", canonical.display()))?;
    for entry in rd.flatten() {
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let full = entry.path();
        let is_git_repo = full.join(".git").exists();
        entries.push(DirEntryItem {
            name,
            path: full.to_string_lossy().to_string(),
            is_dir: true,
            is_git_repo,
        });
    }
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let parent = canonical
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .filter(|p| !p.is_empty());
    Ok(DirListing {
        path: canonical.to_string_lossy().to_string(),
        parent,
        entries,
        os: std::env::consts::OS.to_string(),
        wsl_distro: None,
    })
}

pub fn shell_escape_single(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

#[tauri::command]
pub async fn list_wsl_distros() -> Result<Vec<WslDistro>, String> {
    tokio::task::spawn_blocking(detect_wsl_distros)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_directories(
    path: Option<String>,
    wsl_distro: Option<String>,
) -> Result<DirListing, String> {
    tokio::task::spawn_blocking(move || list_directories_impl(path.as_deref(), wsl_distro.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::encode_wsl_path;

    #[test]
    fn encode_wsl_path_encodes_posix_path_with_distro() {
        assert_eq!(
            encode_wsl_path(Some("Ubuntu"), "/home/user/repo"),
            "wsl://Ubuntu/home/user/repo"
        );
        assert_eq!(
            encode_wsl_path(Some("  Ubuntu-22.04  "), "home/user/repo"),
            "wsl://Ubuntu-22.04/home/user/repo"
        );
    }

    #[test]
    fn encode_wsl_path_skips_when_distro_missing_or_empty() {
        assert_eq!(encode_wsl_path(None, "/home/user/repo"), "/home/user/repo");
        assert_eq!(encode_wsl_path(Some(""), "/home/user/repo"), "/home/user/repo");
        assert_eq!(encode_wsl_path(Some("   "), "/home/user/repo"), "/home/user/repo");
    }

    #[test]
    fn encode_wsl_path_preserves_existing_wsl_paths() {
        assert_eq!(
            encode_wsl_path(Some("Ubuntu"), "wsl://Debian/home/user/repo"),
            "wsl://Debian/home/user/repo"
        );
        assert_eq!(
            encode_wsl_path(Some("Ubuntu"), r"\\wsl.localhost\Debian\home\user\repo"),
            r"\\wsl.localhost\Debian\home\user\repo"
        );
        assert_eq!(
            encode_wsl_path(Some("Ubuntu"), r"\\wsl$\Debian\home\user\repo"),
            r"\\wsl$\Debian\home\user\repo"
        );
    }
}
