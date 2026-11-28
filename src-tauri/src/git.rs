use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::State;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::db::Database;
use crate::models::{DiffFile, DiffHunk, DiffLine, TraceSpan, TurnCheckpoint, WorkspaceFile};
use crate::DbState;

const EMPTY_TREE_HASH: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

async fn resolve_effective_cwd(
    db_arc: &Arc<Mutex<Database>>,
    cwd: &str,
    wsl_distro: Option<&str>,
) -> String {
    if let Some(distro) = wsl_distro.map(str::trim).filter(|s| !s.is_empty()) {
        return crate::platform::encode_wsl_path(Some(distro), cwd);
    }
    if crate::platform::parse_wsl_path(cwd).is_some() {
        return cwd.to_string();
    }
    let db = db_arc.lock().await;
    if let Ok(projects) = db.list_projects() {
        if let Some(proj) = projects.into_iter().find(|p| p.cwd == cwd) {
            return crate::platform::encode_wsl_path(proj.wsl_distro.as_deref(), cwd);
        }
    }
    cwd.to_string()
}

async fn resolve_session_wsl_distro(
    db_arc: &Arc<Mutex<Database>>,
    session_id: &str,
) -> Option<String> {
    let db = db_arc.lock().await;
    db.get_session(session_id)
        .ok()
        .flatten()
        .and_then(|s| db.get_project(&s.project_id).ok().flatten())
        .and_then(|p| p.wsl_distro)
}

fn resolve_fs_root(cwd: &str) -> PathBuf {
    if let Some((distro, linux_path)) = crate::platform::parse_wsl_path(cwd) {
        return crate::platform::wsl_to_unc_path(&distro, &linux_path);
    }
    PathBuf::from(cwd)
}

fn git_cmd(cwd: &str) -> Command {
    if let Some((distro, linux_path)) = crate::platform::parse_wsl_path(cwd) {
        return crate::platform::wsl_std_command(&distro, None, "git", &["-C", &linux_path]);
    }
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(cwd);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn run_git(cwd: &str, args: &[&str]) -> Result<String, String> {
    let output = git_cmd(cwd)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run git {}: {}", args.join(" "), e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(format!("git {} failed: {}", args.join(" "), detail));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_git_with_env(cwd: &str, args: &[&str], envs: &[(&str, &str)]) -> Result<String, String> {
    let mut cmd = if let Some((distro, linux_path)) = crate::platform::parse_wsl_path(cwd) {
        let env_pairs: Vec<String> = envs.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        let mut wsl_args: Vec<&str> = Vec::with_capacity(env_pairs.len() + 3 + args.len());
        for pair in &env_pairs {
            wsl_args.push(pair.as_str());
        }
        wsl_args.push("git");
        wsl_args.push("-C");
        wsl_args.push(&linux_path);
        wsl_args.extend_from_slice(args);
        crate::platform::wsl_std_command(&distro, None, "env", &wsl_args)
    } else {
        let mut c = git_cmd(cwd);
        c.args(args);
        for (k, v) in envs {
            c.env(k, v);
        }
        c
    };
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run git {}: {}", args.join(" "), e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} failed: {}", args.join(" "), stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_git_stdin(cwd: &str, args: &[&str], input: &str) -> Result<String, String> {
    let mut child = git_cmd(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn git {}: {}", args.join(" "), e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .map_err(|e| format!("Failed to write patch to git stdin: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for git {}: {}", args.join(" "), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} failed: {}", args.join(" "), stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn link_shared_dep(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() || dst.exists() {
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent dir for {}: {}", dst.display(), e))?;
    }

    #[cfg(target_os = "windows")]
    {
        if src.is_dir() {
            // Native Win32 directory junction — no cmd.exe, no Admin/Developer Mode required.
            junction::create(src, dst)
                .map_err(|e| format!("Failed to create directory junction {} -> {}: {}", dst.display(), src.display(), e))?;
        } else {
            std::os::windows::fs::symlink_file(src, dst)
                .map_err(|e| format!("Failed to link file {} -> {}: {}", dst.display(), src.display(), e))?;
        }
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst)
            .map_err(|e| format!("Failed to symlink {} -> {}: {}", dst.display(), src.display(), e))?;
    }

    Ok(())
}

fn validate_worktree_session_id(session_id: &str) -> Result<(), String> {
    if session_id.is_empty()
        || !session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!("Invalid worktree session_id: '{}'.", session_id));
    }
    Ok(())
}

fn validate_git_branch_name(branch: &str) -> Result<(), String> {
    let b = branch.trim();
    if b.is_empty()
        || b.starts_with('-')
        || b.contains("..")
        || b.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(format!("Invalid git branch name: '{}'.", branch));
    }
    Ok(())
}

fn validate_relative_dep(dep: &str) -> Result<PathBuf, String> {
    let p = Path::new(dep);
    if p.is_absolute() {
        return Err(format!("Shared dep '{}' must be a relative path", dep));
    }
    for comp in p.components() {
        if matches!(
            comp,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        ) {
            return Err(format!("Shared dep '{}' escapes workspace root", dep));
        }
    }
    Ok(p.to_path_buf())
}

pub async fn create_worktree_inner(
    db_arc: &Arc<Mutex<Database>>,
    session_id: String,
    project_cwd: String,
    branch: String,
    shared_deps: Vec<String>,
) -> Result<String, String> {
    validate_worktree_session_id(&session_id)?;
    validate_git_branch_name(&branch)?;
    let wsl_distro = resolve_session_wsl_distro(db_arc, &session_id).await;
    let effective_cwd = resolve_effective_cwd(db_arc, &project_cwd, wsl_distro.as_deref()).await;
    let fs_root = resolve_fs_root(&effective_cwd);
    let wt_dir = fs_root.join(".sakana").join("worktrees").join(&session_id);

    if let Some(parent) = wt_dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create worktree directory: {}", e))?;
    }

    let git_wt_arg = if let Some((_, linux_path)) = crate::platform::parse_wsl_path(&effective_cwd) {
        format!(
            "{}/.sakana/worktrees/{}",
            linux_path.trim_end_matches('/'),
            session_id
        )
    } else {
        wt_dir.to_string_lossy().to_string()
    };

    if !wt_dir.exists() {
        let add_existing = run_git(&effective_cwd, &["worktree", "add", &git_wt_arg, &branch]);
        if add_existing.is_err() {
            run_git(
                &effective_cwd,
                &["worktree", "add", "-b", &branch, &git_wt_arg],
            )?;
        }
    }

    for dep in shared_deps {
        let trimmed = dep.trim();
        if trimmed.is_empty() {
            continue;
        }
        let rel_dep = validate_relative_dep(trimmed)?;
        let src = fs_root.join(&rel_dep);
        let dst = wt_dir.join(&rel_dep);
        link_shared_dep(&src, &dst)?;
    }

    Ok(git_wt_arg)
}

#[tauri::command]
pub async fn create_worktree(
    state: State<'_, DbState>,
    session_id: String,
    project_cwd: String,
    branch: String,
    shared_deps: Vec<String>,
) -> Result<String, String> {
    create_worktree_inner(&state.0, session_id, project_cwd, branch, shared_deps).await
}

pub async fn remove_worktree_inner(
    db_arc: &Arc<Mutex<Database>>,
    session_id: String,
    project_cwd: String,
) -> Result<(), String> {
    validate_worktree_session_id(&session_id)?;
    let wsl_distro = resolve_session_wsl_distro(db_arc, &session_id).await;
    let effective_cwd = resolve_effective_cwd(db_arc, &project_cwd, wsl_distro.as_deref()).await;
    let fs_root = resolve_fs_root(&effective_cwd);
    let wt_dir = fs_root.join(".sakana").join("worktrees").join(&session_id);
    let git_wt_arg = if let Some((_, linux_path)) = crate::platform::parse_wsl_path(&effective_cwd) {
        format!(
            "{}/.sakana/worktrees/{}",
            linux_path.trim_end_matches('/'),
            session_id
        )
    } else {
        wt_dir.to_string_lossy().to_string()
    };

    let _ = run_git(&effective_cwd, &["worktree", "remove", "--force", &git_wt_arg]);
    let _ = run_git(&effective_cwd, &["worktree", "prune"]);

    if wt_dir.exists() {
        std::fs::remove_dir_all(&wt_dir).map_err(|e| {
            format!(
                "Failed to remove worktree directory {}: {}",
                wt_dir.display(),
                e
            )
        })?;
    }
    Ok(())
}

#[tauri::command]
pub async fn remove_worktree(
    state: State<'_, DbState>,
    session_id: String,
    project_cwd: String,
) -> Result<(), String> {
    remove_worktree_inner(&state.0, session_id, project_cwd).await
}

fn is_git_repo(cwd: &str) -> bool {
    !cwd.trim().is_empty()
        && run_git(cwd, &["rev-parse", "--is-inside-work-tree"])
            .map(|s| s.trim() == "true")
            .unwrap_or(false)
}

fn ensure_git_repo(cwd: &str) -> Result<(), String> {
    if !is_git_repo(cwd) {
        return Err("Selected folder is not a Git repository".to_string());
    }
    Ok(())
}

pub async fn list_branches_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    wsl_distro: Option<String>,
) -> Result<Vec<String>, String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    ensure_git_repo(&cwd)?;
    let out = run_git(&cwd, &["branch", "--format=%(refname:short)"])?;
    let branches: Vec<String> = out
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect();
    Ok(branches)
}

#[tauri::command]
pub async fn list_branches(
    state: State<'_, DbState>,
    cwd: String,
    wsl_distro: Option<String>,
) -> Result<Vec<String>, String> {
    list_branches_inner(&state.0, cwd, wsl_distro).await
}

pub async fn checkout_branch_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    branch: String,
    wsl_distro: Option<String>,
) -> Result<(), String> {
    validate_git_branch_name(&branch)?;
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    ensure_git_repo(&cwd)?;
    run_git(&cwd, &["checkout", &branch])?;
    Ok(())
}

#[tauri::command]
pub async fn checkout_branch(
    state: State<'_, DbState>,
    cwd: String,
    branch: String,
    wsl_distro: Option<String>,
) -> Result<(), String> {
    checkout_branch_inner(&state.0, cwd, branch, wsl_distro).await
}

pub async fn git_pull_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    wsl_distro: Option<String>,
) -> Result<String, String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    ensure_git_repo(&cwd)?;
    run_git(&cwd, &["pull"])
}

#[tauri::command]
pub async fn git_pull(
    state: State<'_, DbState>,
    cwd: String,
    wsl_distro: Option<String>,
) -> Result<String, String> {
    git_pull_inner(&state.0, cwd, wsl_distro).await
}

pub async fn git_push_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    wsl_distro: Option<String>,
) -> Result<String, String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    ensure_git_repo(&cwd)?;
    match run_git(&cwd, &["push"]) {
        Ok(out) => Ok(out),
        Err(_) => run_git(&cwd, &["push", "-u", "origin", "HEAD"]),
    }
}

#[tauri::command]
pub async fn git_push(
    state: State<'_, DbState>,
    cwd: String,
    wsl_distro: Option<String>,
) -> Result<String, String> {
    git_push_inner(&state.0, cwd, wsl_distro).await
}

fn resolve_git_dir(cwd: &str) -> Result<PathBuf, String> {
    let raw = run_git(cwd, &["rev-parse", "--git-dir"])?;
    let trimmed = raw.trim();
    let p = Path::new(trimmed);
    if p.is_absolute() {
        Ok(p.to_path_buf())
    } else {
        Ok(Path::new(cwd).join(p))
    }
}

fn parse_hunk_header_lines(header: &str) -> (i64, i64) {
    // Format: @@ -old_start[,old_count] +new_start[,new_count] @@ ...
    let mut old_start = 1i64;
    let mut new_start = 1i64;
    for part in header.split_whitespace() {
        if let Some(rest) = part.strip_prefix('-') {
            if let Some(num) = rest.split(',').next().and_then(|s| s.parse::<i64>().ok()) {
                old_start = num;
            }
        } else if let Some(rest) = part.strip_prefix('+') {
            if let Some(num) = rest.split(',').next().and_then(|s| s.parse::<i64>().ok()) {
                new_start = num;
            }
        }
    }
    (old_start, new_start)
}

fn parse_unified_diff(raw: &str, staged: bool, turn_id: Option<&str>) -> Vec<DiffFile> {
    let mut files: Vec<DiffFile> = Vec::new();
    let mut current_file: Option<DiffFile> = None;
    let mut current_hunk: Option<DiffHunk> = None;
    let mut old_ln = 1i64;
    let mut new_ln = 1i64;
    let mut hunk_counter = 0usize;

    let flush_hunk = |file: &mut Option<DiffFile>, hunk: &mut Option<DiffHunk>| {
        if let (Some(ref mut f), Some(h)) = (file, hunk.take()) {
            f.hunks.push(h);
        }
    };
    let flush_file = |files: &mut Vec<DiffFile>,
                      file: &mut Option<DiffFile>,
                      hunk: &mut Option<DiffHunk>| {
        flush_hunk(file, hunk);
        if let Some(f) = file.take() {
            files.push(f);
        }
    };

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            flush_file(&mut files, &mut current_file, &mut current_hunk);
            let path = rest
                .split(" b/")
                .nth(1)
                .unwrap_or_else(|| rest.split_whitespace().last().unwrap_or("unknown"))
                .to_string();
            current_file = Some(DiffFile {
                path,
                status: "modified".to_string(),
                turn_id: turn_id.map(ToString::to_string),
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
            });
            continue;
        }

        if let Some(ref mut f) = current_file {
            if line.starts_with("new file mode") {
                f.status = "added".to_string();
                continue;
            }
            if line.starts_with("deleted file mode") {
                f.status = "deleted".to_string();
                continue;
            }
            if line.starts_with("index ")
                || line.starts_with("--- ")
                || line.starts_with("+++ ")
                || line.starts_with("rename from ")
                || line.starts_with("rename to ")
                || line.starts_with("similarity index ")
            {
                continue;
            }
        }

        if line.starts_with("@@ ") {
            flush_hunk(&mut current_file, &mut current_hunk);
            hunk_counter += 1;
            let (os, ns) = parse_hunk_header_lines(line);
            old_ln = os;
            new_ln = ns;
            current_hunk = Some(DiffHunk {
                id: format!("hunk-{}-{}", if staged { "s" } else { "u" }, hunk_counter),
                header: line.to_string(),
                staged,
                discarded: None,
                lines: Vec::new(),
            });
            continue;
        }

        if let (Some(ref mut f), Some(ref mut h)) = (&mut current_file, &mut current_hunk) {
            if let Some(content) = line.strip_prefix('+') {
                f.additions += 1;
                h.lines.push(DiffLine {
                    line_type: "add".to_string(),
                    old_line: None,
                    new_line: Some(new_ln),
                    content: content.to_string(),
                });
                new_ln += 1;
            } else if let Some(content) = line.strip_prefix('-') {
                f.deletions += 1;
                h.lines.push(DiffLine {
                    line_type: "del".to_string(),
                    old_line: Some(old_ln),
                    new_line: None,
                    content: content.to_string(),
                });
                old_ln += 1;
            } else if let Some(content) = line.strip_prefix(' ') {
                h.lines.push(DiffLine {
                    line_type: "ctx".to_string(),
                    old_line: Some(old_ln),
                    new_line: Some(new_ln),
                    content: content.to_string(),
                });
                old_ln += 1;
                new_ln += 1;
            }
        }
    }

    flush_file(&mut files, &mut current_file, &mut current_hunk);
    files
}

fn extract_single_hunk_patch(raw_diff: &str, file_path: &str, hunk_header: &str) -> Option<String> {
    let mut in_target_file = false;
    let mut file_headers: Vec<String> = Vec::new();
    let mut capturing_hunk = false;
    let mut hunk_lines: Vec<String> = Vec::new();

    for line in raw_diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if capturing_hunk {
                break;
            }
            let path = rest
                .split(" b/")
                .nth(1)
                .unwrap_or_else(|| rest.split_whitespace().last().unwrap_or(""));
            in_target_file = path == file_path;
            file_headers.clear();
            if in_target_file {
                file_headers.push(line.to_string());
            }
            continue;
        }

        if !in_target_file {
            continue;
        }

        if line.starts_with("@@ ") {
            if capturing_hunk {
                break;
            }
            if line.trim() == hunk_header.trim() {
                capturing_hunk = true;
                hunk_lines.push(line.to_string());
            }
            continue;
        }

        if capturing_hunk {
            if line.starts_with('+')
                || line.starts_with('-')
                || line.starts_with(' ')
                || line.starts_with('\\')
            {
                hunk_lines.push(line.to_string());
            } else {
                break;
            }
        } else if line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("index ")
            || line.starts_with("new file mode")
            || line.starts_with("deleted file mode")
        {
            file_headers.push(line.to_string());
        }
    }

    if !capturing_hunk || hunk_lines.is_empty() {
        return None;
    }

    let mut patch = String::new();
    for h in file_headers {
        patch.push_str(&h);
        patch.push('\n');
    }
    for l in hunk_lines {
        patch.push_str(&l);
        patch.push('\n');
    }
    Some(patch)
}

pub async fn create_checkpoint_inner(
    db_arc: &Arc<Mutex<Database>>,
    session_id: String,
    turn_number: i64,
    label: String,
    cwd: String,
) -> Result<Option<TurnCheckpoint>, String> {
    let wsl_distro = resolve_session_wsl_distro(db_arc, &session_id).await;
    let cwd = crate::platform::encode_wsl_path(wsl_distro.as_deref(), &cwd);
    let idx_name = format!("sakana-idx-{}", Uuid::new_v4());
    let (temp_index_host, temp_idx_env) = if let Some((distro, linux_path)) = crate::platform::parse_wsl_path(&cwd) {
        let raw_git_dir = run_git(&cwd, &["rev-parse", "--git-dir"])?;
        let trimmed = raw_git_dir.trim();
        let linux_git_dir = if trimmed.starts_with('/') {
            trimmed.to_string()
        } else {
            format!("{}/{}", linux_path.trim_end_matches('/'), trimmed)
        };
        let orig_index_host = crate::platform::wsl_to_unc_path(&distro, &format!("{}/index", linux_git_dir.trim_end_matches('/')));
        let linux_temp = format!("/tmp/{}", idx_name);
        let host_temp = crate::platform::wsl_to_unc_path(&distro, &linux_temp);
        if orig_index_host.exists() {
            std::fs::copy(&orig_index_host, &host_temp)
                .map_err(|e| format!("Failed to copy WSL git index: {}", e))?;
        }
        (host_temp, linux_temp)
    } else {
        let git_dir = resolve_git_dir(&cwd)?;
        let orig_index = git_dir.join("index");
        let host_temp = std::env::temp_dir().join(&idx_name);
        if orig_index.exists() {
            std::fs::copy(&orig_index, &host_temp)
                .map_err(|e| format!("Failed to copy git index: {}", e))?;
        }
        let env_str = host_temp.to_string_lossy().to_string();
        (host_temp, env_str)
    };

    let add_res = run_git_with_env(&cwd, &["add", "-A"], &[("GIT_INDEX_FILE", &temp_idx_env)]);
    if let Err(e) = add_res {
        let _ = std::fs::remove_file(&temp_index_host);
        return Err(e);
    }

    let tree_res = run_git_with_env(
        &cwd,
        &["write-tree"],
        &[("GIT_INDEX_FILE", &temp_idx_env)],
    );
    let _ = std::fs::remove_file(&temp_index_host);
    let tree_hash = tree_res?.trim().to_string();

    let base_ref = if turn_number > 1 {
        let prev_ref = format!("refs/sakana/checkpoints/{}/{}", session_id, turn_number - 1);
        if run_git(&cwd, &["rev-parse", "--verify", &prev_ref]).is_ok() {
            prev_ref
        } else if run_git(&cwd, &["rev-parse", "--verify", "HEAD"]).is_ok() {
            "HEAD".to_string()
        } else {
            EMPTY_TREE_HASH.to_string()
        }
    } else if run_git(&cwd, &["rev-parse", "--verify", "HEAD"]).is_ok() {
        "HEAD".to_string()
    } else {
        EMPTY_TREE_HASH.to_string()
    };

    let numstat = run_git(&cwd, &["diff", "--numstat", &base_ref, &tree_hash]).unwrap_or_default();
    let mut files_changed = 0i64;
    let mut additions = 0i64;
    let mut deletions = 0i64;
    for line in numstat.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            files_changed += 1;
            additions += parts[0].trim().parse::<i64>().unwrap_or(0);
            deletions += parts[1].trim().parse::<i64>().unwrap_or(0);
        }
    }

    if files_changed == 0 {
        return Ok(None);
    }

    let ref_name = format!("refs/sakana/checkpoints/{}/{}", session_id, turn_number);
    run_git(&cwd, &["update-ref", &ref_name, &tree_hash])?;

    let checkpoint = TurnCheckpoint {
        id: format!("cp-{}", Uuid::new_v4()),
        turn_number,
        commit_hash: tree_hash,
        label,
        files_changed,
        additions,
        deletions,
    };

    let db = db_arc.lock().await;
    db.insert_checkpoint(&session_id, &checkpoint)
        .map_err(|e| e.to_string())?;

    Ok(Some(checkpoint))
}

#[tauri::command]
pub async fn create_checkpoint(
    state: State<'_, DbState>,
    session_id: String,
    turn_number: i64,
    label: String,
    cwd: String,
) -> Result<Option<TurnCheckpoint>, String> {
    create_checkpoint_inner(&state.0, session_id, turn_number, label, cwd).await
}

pub async fn get_checkpoint_diff_inner(
    db_arc: &Arc<Mutex<Database>>,
    session_id: String,
    checkpoint_id: String,
) -> Result<Vec<DiffFile>, String> {
    let (cwd, cp, all_cps) = {
        let db = db_arc.lock().await;
        let session = db
            .get_session(&session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Session {} not found", session_id))?;
        let wsl_distro = db
            .get_project(&session.project_id)
            .map_err(|e| e.to_string())?
            .and_then(|p| p.wsl_distro);
        let cp = db
            .get_checkpoint(&session_id, &checkpoint_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Checkpoint {} not found", checkpoint_id))?;
        let all_cps = db.list_checkpoints(&session_id).map_err(|e| e.to_string())?;
        let effective_cwd =
            crate::platform::encode_wsl_path(wsl_distro.as_deref(), &session.worktree_path);
        (effective_cwd, cp, all_cps)
    };

    let prev_tree = all_cps
        .iter()
        .rfind(|c| c.turn_number < cp.turn_number)
        .map(|c| c.commit_hash.clone())
        .unwrap_or_else(|| {
            if run_git(&cwd, &["rev-parse", "--verify", "HEAD"]).is_ok() {
                "HEAD".to_string()
            } else {
                EMPTY_TREE_HASH.to_string()
            }
        });

    let raw = run_git(&cwd, &["diff", "--no-color", &prev_tree, &cp.commit_hash])?;
    Ok(parse_unified_diff(&raw, false, Some(&cp.id)))
}

#[tauri::command]
pub async fn get_checkpoint_diff(
    state: State<'_, DbState>,
    session_id: String,
    checkpoint_id: String,
) -> Result<Vec<DiffFile>, String> {
    get_checkpoint_diff_inner(&state.0, session_id, checkpoint_id).await
}

pub async fn revert_to_checkpoint_inner(
    db_arc: &Arc<Mutex<Database>>,
    session_id: String,
    checkpoint_id: String,
) -> Result<(), String> {
    let (cwd, cp) = {
        let db = db_arc.lock().await;
        let session = db
            .get_session(&session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Session {} not found", session_id))?;
        let wsl_distro = db
            .get_project(&session.project_id)
            .map_err(|e| e.to_string())?
            .and_then(|p| p.wsl_distro);
        let cp = db
            .get_checkpoint(&session_id, &checkpoint_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Checkpoint {} not found", checkpoint_id))?;
        let effective_cwd =
            crate::platform::encode_wsl_path(wsl_distro.as_deref(), &session.worktree_path);
        (effective_cwd, cp)
    };

    run_git(&cwd, &["read-tree", "--reset", "-u", &cp.commit_hash])?;

    let db = db_arc.lock().await;
    db.truncate_after_checkpoint(&session_id, &checkpoint_id, cp.turn_number)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn revert_to_checkpoint(
    state: State<'_, DbState>,
    session_id: String,
    checkpoint_id: String,
) -> Result<(), String> {
    revert_to_checkpoint_inner(&state.0, session_id, checkpoint_id).await
}

pub async fn rewind_checkpoint_inner(
    db_arc: &Arc<Mutex<Database>>,
    session_id: String,
    checkpoint_id: String,
    reset_git: bool,
    auto_stash: bool,
) -> Result<crate::models::RewindResult, String> {
    let (cwd, cp) = {
        let db = db_arc.lock().await;
        let session = db
            .get_session(&session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Session {} not found", session_id))?;
        let wsl_distro = db
            .get_project(&session.project_id)
            .map_err(|e| e.to_string())?
            .and_then(|p| p.wsl_distro);
        let cp = db
            .get_checkpoint(&session_id, &checkpoint_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Checkpoint {} not found", checkpoint_id))?;
        let effective_cwd =
            crate::platform::encode_wsl_path(wsl_distro.as_deref(), &session.worktree_path);
        (effective_cwd, cp)
    };

    let mut stashed = false;
    let mut stash_ref: Option<String> = None;
    let mut did_reset_git = false;

    if reset_git && is_git_repo(&cwd) {
        if auto_stash {
            let status = run_git(&cwd, &["status", "--porcelain"]).unwrap_or_default();
            if !status.trim().is_empty() {
                let label = format!("sakana-rewind-turn-{}", cp.turn_number);
                if run_git(&cwd, &["stash", "push", "-u", "-m", &label]).is_ok() {
                    stashed = true;
                    stash_ref = run_git(&cwd, &["rev-parse", "--short", "stash@{0}"])
                        .ok()
                        .map(|s| s.trim().to_string());
                }
            }
        }
        run_git(&cwd, &["read-tree", "--reset", "-u", &cp.commit_hash])?;
        did_reset_git = true;
    }

    let db = db_arc.lock().await;
    let removed_messages = db
        .truncate_after_checkpoint_counted(&session_id, &checkpoint_id, cp.turn_number)
        .map_err(|e| e.to_string())?;
    let _ = db.insert_audit_log(
        "desktop",
        "checkpoint_rewind",
        "allowed",
        &format!(
            "Rewound session {} to Turn {} (reset_git={}, stashed={}, removed={})",
            session_id, cp.turn_number, did_reset_git, stashed, removed_messages
        ),
        None,
    );
    let session = db
        .get_session(&session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Session not found after rewind".to_string())?;

    Ok(crate::models::RewindResult {
        session,
        stashed,
        stash_ref,
        removed_messages,
        reset_git: did_reset_git,
    })
}

#[tauri::command]
pub async fn rewind_checkpoint(
    state: State<'_, DbState>,
    session_id: String,
    checkpoint_id: String,
    reset_git: Option<bool>,
    auto_stash: Option<bool>,
) -> Result<crate::models::RewindResult, String> {
    rewind_checkpoint_inner(
        &state.0,
        session_id,
        checkpoint_id,
        reset_git.unwrap_or(true),
        auto_stash.unwrap_or(true),
    )
    .await
}

pub async fn fork_from_checkpoint_inner(
    db_arc: &Arc<Mutex<Database>>,
    session_id: String,
    checkpoint_id: String,
    title: Option<String>,
) -> Result<crate::models::Session, String> {
    let db = db_arc.lock().await;
    let session = db
        .get_session(&session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Session {} not found", session_id))?;
    let cp = db
        .get_checkpoint(&session_id, &checkpoint_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Checkpoint {} not found", checkpoint_id))?;
    let upto = db
        .find_message_index_for_checkpoint(&session_id, &checkpoint_id, cp.turn_number)
        .map_err(|e| e.to_string())?
        .unwrap_or(0);
    let fork_title = title.unwrap_or_else(|| format!("Fork @ Turn {}", cp.turn_number));
    let forked = db
        .fork_session(&session_id, &fork_title, &session.branch, upto)
        .map_err(|e| e.to_string())?;
    let _ = db.insert_audit_log(
        "desktop",
        "checkpoint_fork",
        "allowed",
        &format!("Forked session {} at Turn {} -> {}", session_id, cp.turn_number, forked.id),
        None,
    );
    Ok(forked)
}

#[tauri::command]
pub async fn fork_from_checkpoint(
    state: State<'_, DbState>,
    session_id: String,
    checkpoint_id: String,
    title: Option<String>,
) -> Result<crate::models::Session, String> {
    fork_from_checkpoint_inner(&state.0, session_id, checkpoint_id, title).await
}

pub async fn get_git_diffs_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    turn_id: Option<String>,
    wsl_distro: Option<String>,
) -> Result<Vec<DiffFile>, String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    if !is_git_repo(&cwd) {
        return Ok(vec![]);
    }
    let staged_raw = run_git(&cwd, &["diff", "--cached", "--no-color"]).unwrap_or_default();
    let unstaged_raw = run_git(&cwd, &["diff", "--no-color"]).unwrap_or_default();

    let mut merged: HashMap<String, DiffFile> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for df in parse_unified_diff(&staged_raw, true, turn_id.as_deref()) {
        if !merged.contains_key(&df.path) {
            order.push(df.path.clone());
        }
        merged.insert(df.path.clone(), df);
    }

    for df in parse_unified_diff(&unstaged_raw, false, turn_id.as_deref()) {
        if let Some(existing) = merged.get_mut(&df.path) {
            existing.additions += df.additions;
            existing.deletions += df.deletions;
            existing.hunks.extend(df.hunks);
        } else {
            order.push(df.path.clone());
            merged.insert(df.path.clone(), df);
        }
    }

    if let Ok(untracked) = run_git(&cwd, &["ls-files", "--others", "--exclude-standard"]) {
        let fs_root = resolve_fs_root(&cwd);
        for rel in untracked.lines().map(str::trim).filter(|s| !s.is_empty()) {
            if merged.contains_key(rel) {
                continue;
            }
            let full = fs_root.join(rel);
            if let Ok(bytes) = std::fs::read(&full) {
                if bytes.len() <= 256 * 1024 && !bytes.contains(&0) {
                    let text = String::from_utf8_lossy(&bytes);
                    let content_lines: Vec<DiffLine> = text
                        .lines()
                        .enumerate()
                        .map(|(i, l)| DiffLine {
                            line_type: "add".to_string(),
                            old_line: None,
                            new_line: Some((i + 1) as i64),
                            content: l.to_string(),
                        })
                        .collect();
                    let count = content_lines.len() as i64;
                    order.push(rel.to_string());
                    merged.insert(
                        rel.to_string(),
                        DiffFile {
                            path: rel.to_string(),
                            status: "added".to_string(),
                            turn_id: turn_id.clone(),
                            additions: count,
                            deletions: 0,
                            hunks: vec![DiffHunk {
                                id: format!("hunk-untracked-{}", order.len()),
                                header: format!("@@ -0,0 +1,{} @@", count),
                                staged: false,
                                discarded: None,
                                lines: content_lines,
                            }],
                        },
                    );
                }
            }
        }
    }

    Ok(order
        .into_iter()
        .filter_map(|p| merged.remove(&p))
        .collect())
}

#[tauri::command]
pub async fn get_git_diffs(
    state: State<'_, DbState>,
    cwd: String,
    turn_id: Option<String>,
    wsl_distro: Option<String>,
) -> Result<Vec<DiffFile>, String> {
    get_git_diffs_inner(&state.0, cwd, turn_id, wsl_distro).await
}

pub async fn stage_hunk_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    file_path: String,
    hunk_header: String,
    stage: bool,
    wsl_distro: Option<String>,
) -> Result<(), String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    ensure_git_repo(&cwd)?;
    let raw = if stage {
        run_git(&cwd, &["diff", "--no-color", "--", &file_path])?
    } else {
        run_git(&cwd, &["diff", "--cached", "--no-color", "--", &file_path])?
    };

    if raw.trim().is_empty() {
        if stage {
            let untracked =
                run_git(&cwd, &["ls-files", "--others", "--exclude-standard", "--", &file_path])
                    .unwrap_or_default();
            if !untracked.trim().is_empty() {
                run_git(&cwd, &["add", "--", &file_path])?;
                return Ok(());
            }
        } else {
            let staged_status =
                run_git(&cwd, &["diff", "--cached", "--name-status", "--", &file_path])
                    .unwrap_or_default();
            if !staged_status.trim().is_empty() {
                run_git(&cwd, &["reset", "HEAD", "--", &file_path])?;
                return Ok(());
            }
        }
    }

    let patch = extract_single_hunk_patch(&raw, &file_path, &hunk_header)
        .ok_or_else(|| format!("Hunk '{}' not found in {}", hunk_header, file_path))?;

    if stage {
        run_git_stdin(&cwd, &["apply", "--cached", "--unidiff-zero", "-"], &patch)?;
    } else {
        run_git_stdin(
            &cwd,
            &["apply", "--cached", "-R", "--unidiff-zero", "-"],
            &patch,
        )?;
    }
    Ok(())
}

#[tauri::command]
pub async fn stage_hunk(
    state: State<'_, DbState>,
    cwd: String,
    file_path: String,
    hunk_header: String,
    stage: bool,
    wsl_distro: Option<String>,
) -> Result<(), String> {
    stage_hunk_inner(&state.0, cwd, file_path, hunk_header, stage, wsl_distro).await
}

pub async fn discard_hunk_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    file_path: String,
    hunk_header: String,
    wsl_distro: Option<String>,
) -> Result<(), String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    ensure_git_repo(&cwd)?;
    let raw = run_git(&cwd, &["diff", "--no-color", "--", &file_path])?;
    if raw.trim().is_empty() {
        let untracked =
            run_git(&cwd, &["ls-files", "--others", "--exclude-standard", "--", &file_path])
                .unwrap_or_default();
        if !untracked.trim().is_empty() {
            let full = safe_resolve_confined_path(&cwd, &file_path)?;
            if full.exists() {
                std::fs::remove_file(&full)
                    .map_err(|e| format!("Failed to remove untracked file {}: {}", file_path, e))?;
            }
            return Ok(());
        }
    }

    let patch = extract_single_hunk_patch(&raw, &file_path, &hunk_header)
        .ok_or_else(|| format!("Hunk '{}' not found in {}", hunk_header, file_path))?;

    run_git_stdin(&cwd, &["apply", "-R", "--unidiff-zero", "-"], &patch)?;
    Ok(())
}

#[tauri::command]
pub async fn discard_hunk(
    state: State<'_, DbState>,
    cwd: String,
    file_path: String,
    hunk_header: String,
    wsl_distro: Option<String>,
) -> Result<(), String> {
    discard_hunk_inner(&state.0, cwd, file_path, hunk_header, wsl_distro).await
}

pub async fn draft_commit_message_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    wsl_distro: Option<String>,
) -> Result<String, String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    ensure_git_repo(&cwd)?;
    let staged_stat = run_git(&cwd, &["diff", "--cached", "--stat"]).unwrap_or_default();
    let stat = if staged_stat.trim().is_empty() {
        run_git(&cwd, &["diff", "--stat"]).unwrap_or_default()
    } else {
        staged_stat
    };

    let mut files: Vec<String> = Vec::new();
    for line in stat.lines() {
        if let Some((path_part, _)) = line.split_once('|') {
            let trimmed = path_part.trim();
            if !trimmed.is_empty() {
                files.push(trimmed.to_string());
            }
        }
    }

    if files.is_empty() {
        return Ok("chore: update workspace".to_string());
    }

    let prefix = if files.iter().all(|f| f.ends_with(".md")) {
        "docs"
    } else if files.iter().any(|f| f.contains("test")) {
        "test"
    } else {
        "feat"
    };

    let summary = if files.len() == 1 {
        format!("{}: update {}", prefix, files[0])
    } else {
        let first = Path::new(&files[0])
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&files[0]);
        format!(
            "{}: update {} and {} more file(s)",
            prefix,
            first,
            files.len() - 1
        )
    };

    Ok(summary)
}

#[tauri::command]
pub async fn draft_commit_message(
    state: State<'_, DbState>,
    cwd: String,
    wsl_distro: Option<String>,
) -> Result<String, String> {
    draft_commit_message_inner(&state.0, cwd, wsl_distro).await
}

pub async fn git_commit_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    message: String,
    wsl_distro: Option<String>,
) -> Result<String, String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    ensure_git_repo(&cwd)?;
    let staged = run_git(&cwd, &["diff", "--cached", "--name-only"]).unwrap_or_default();
    if staged.trim().is_empty() {
        run_git(&cwd, &["add", "-A"])?;
    }
    run_git(&cwd, &["commit", "-m", &message])
}

#[tauri::command]
pub async fn git_commit(
    state: State<'_, DbState>,
    cwd: String,
    message: String,
    wsl_distro: Option<String>,
) -> Result<String, String> {
    git_commit_inner(&state.0, cwd, message, wsl_distro).await
}

fn parse_github_owner_repo(remote_url: &str) -> Option<(String, String)> {
    let trimmed = remote_url.trim().trim_end_matches(".git");
    if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        let mut parts = rest.splitn(2, '/');
        return Some((parts.next()?.to_string(), parts.next()?.to_string()));
    }
    if let Some(idx) = trimmed.find("github.com/") {
        let rest = &trimmed[idx + "github.com/".len()..];
        let mut parts = rest.splitn(2, '/');
        return Some((parts.next()?.to_string(), parts.next()?.to_string()));
    }
    None
}

pub async fn create_pr_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    title: String,
    body: String,
    base_branch: String,
    wsl_distro: Option<String>,
) -> Result<String, String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    ensure_git_repo(&cwd)?;
    let mut gh_cmd = if let Some((distro, linux_path)) = crate::platform::parse_wsl_path(&cwd) {
        crate::platform::wsl_std_command(
            &distro,
            Some(&linux_path),
            "gh",
            &["pr", "create", "--title", &title, "--body", &body, "--base", &base_branch],
        )
    } else {
        let mut c = Command::new("gh");
        c.current_dir(&cwd)
            .args(["pr", "create", "--title", &title, "--body", &body, "--base", &base_branch]);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        c
    };

    if let Ok(out) = gh_cmd.output() {
        if out.status.success() {
            let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !url.is_empty() {
                return Ok(url);
            }
        }
    }

    let token = crate::platform::get_env_or_dotenv("GITHUB_TOKEN")
        .ok_or_else(|| "gh CLI failed and GITHUB_TOKEN is not set in environment or .env secrets".to_string())?;
    let remote = run_git(&cwd, &["remote", "get-url", "origin"])?;
    let (owner, repo) = parse_github_owner_repo(&remote)
        .ok_or_else(|| format!("Could not parse GitHub owner/repo from remote: {}", remote.trim()))?;
    let head_branch = run_git(&cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();

    let client = Client::new();
    let url = format!("https://api.github.com/repos/{}/{}/pulls", owner, repo);
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "sakana-desktop")
        .header("Accept", "application/vnd.github+json")
        .json(&json!({
            "title": title,
            "body": body,
            "head": head_branch,
            "base": base_branch
        }))
        .send()
        .await
        .map_err(|e| format!("GitHub API request failed: {}", e))?;

    let status = resp.status();
    let json_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse GitHub API response: {}", e))?;

    if !status.is_success() {
        return Err(format!("GitHub API error ({}): {}", status, json_body));
    }

    Ok(json_body["html_url"]
        .as_str()
        .unwrap_or_default()
        .to_string())
}

#[tauri::command]
pub async fn create_pr(
    state: State<'_, DbState>,
    cwd: String,
    title: String,
    body: String,
    base_branch: String,
    wsl_distro: Option<String>,
) -> Result<String, String> {
    create_pr_inner(&state.0, cwd, title, body, base_branch, wsl_distro).await
}

fn collect_workspace_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if matches!(
            name_str.as_ref(),
            ".git" | "node_modules" | "target" | "dist" | ".sakana"
        ) {
            continue;
        }
        if path.is_dir() {
            collect_workspace_files(root, &path, out);
        } else if path.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

pub async fn list_workspace_files_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    wsl_distro: Option<String>,
) -> Result<Vec<String>, String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    let root_buf = resolve_fs_root(&cwd);
    let root = root_buf.as_path();
    if !root.exists() {
        return Err(format!("Workspace directory does not exist: {}", cwd));
    }
    let mut files = Vec::new();
    collect_workspace_files(root, root, &mut files);
    files.sort();
    Ok(files)
}

#[tauri::command]
pub async fn list_workspace_files(
    state: State<'_, DbState>,
    cwd: String,
    wsl_distro: Option<String>,
) -> Result<Vec<String>, String> {
    list_workspace_files_inner(&state.0, cwd, wsl_distro).await
}

fn detect_language(rel_path: &str) -> &'static str {
    let ext = Path::new(rel_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "ts" => "typescript",
        "tsx" => "tsx",
        "js" => "javascript",
        "jsx" => "jsx",
        "rs" => "rust",
        "py" => "python",
        "json" => "json",
        "css" | "scss" => "css",
        "html" | "htm" => "html",
        "md" | "mdx" => "markdown",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "sh" | "bash" | "zsh" => "shell",
        _ => "plaintext",
    }
}

fn strip_file_uri(raw: &str) -> &str {
    let trimmed = raw.trim();
    let stripped = trimmed.strip_prefix("file://").unwrap_or(trimmed);
    // On Windows, file:///C:/foo leaves a leading slash before the drive letter.
    if cfg!(target_os = "windows")
        && stripped.starts_with('/')
        && stripped.as_bytes().get(2) == Some(&b':')
    {
        &stripped[1..]
    } else {
        stripped
    }
}

fn expand_native_tilde(stripped: &str) -> String {
    if stripped == "~" || stripped.starts_with("~/") || stripped.starts_with("~\\") {
        if let Some(home) = dirs::home_dir() {
            if stripped == "~" {
                return home.to_string_lossy().to_string();
            }
            let rest = stripped[2..].trim_start_matches(['/', '\\']);
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    stripped.to_string()
}

fn resolve_wsl_home(distro: &str) -> Option<String> {
    let out = crate::platform::wsl_std_command(distro, Some("/"), "sh", &["-lc", "printf '%s' \"$HOME\""])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Resolve a path for local desktop inspection/opening (supports WSL `~/` and external local paths).
pub fn resolve_path_for_open(cwd: &str, rel_path: &str) -> Result<PathBuf, String> {
    let stripped = strip_file_uri(rel_path);
    if stripped.is_empty() {
        return Err("File path is empty".to_string());
    }
    if let Some((distro, linux_cwd)) = crate::platform::parse_wsl_path(cwd) {
        let linux_target = if stripped == "~" || stripped.starts_with("~/") {
            let home = resolve_wsl_home(&distro)
                .ok_or_else(|| format!("Failed to resolve WSL home directory for {}", distro))?;
            if stripped == "~" {
                home
            } else {
                format!("{}/{}", home.trim_end_matches('/'), &stripped[2..])
            }
        } else if stripped.starts_with('/') {
            stripped.to_string()
        } else {
            format!(
                "{}/{}",
                linux_cwd.trim_end_matches('/'),
                stripped.trim_start_matches("./")
            )
        };
        let unc = crate::platform::wsl_to_unc_path(&distro, &linux_target);
        return if unc.exists() {
            unc.canonicalize().or(Ok(unc))
        } else {
            Ok(unc)
        };
    }

    let expanded = expand_native_tilde(stripped);
    let candidate = PathBuf::from(&expanded);
    if candidate.is_absolute() {
        return if candidate.exists() {
            candidate.canonicalize().or(Ok(candidate))
        } else {
            Ok(candidate)
        };
    }

    let root_buf = resolve_fs_root(cwd);
    let root = root_buf.canonicalize().unwrap_or(root_buf);
    let joined = root.join(&candidate);
    if joined.exists() {
        joined.canonicalize().or(Ok(joined))
    } else {
        Ok(joined)
    }
}

/// Strictly resolve a path confined within the workspace root (for Remote RPC & git discard).
fn safe_resolve_confined_path(cwd: &str, rel_path: &str) -> Result<PathBuf, String> {
    let stripped = strip_file_uri(rel_path);
    if stripped.is_empty() {
        return Err("File path is empty".to_string());
    }
    if stripped == "~" || stripped.starts_with("~/") || stripped.starts_with("~\\") {
        return Err("Path escapes workspace root".to_string());
    }
    let root_buf = resolve_fs_root(cwd);
    let root = root_buf
        .canonicalize()
        .map_err(|e| format!("Invalid workspace cwd {}: {}", cwd, e))?;
    let candidate = Path::new(stripped);
    if candidate.is_absolute() {
        return Err("Absolute paths are not allowed outside workspace".to_string());
    }
    for comp in candidate.components() {
        if matches!(
            comp,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        ) {
            return Err("Path traversal is not allowed".to_string());
        }
    }
    let joined = root.join(candidate);
    if let Ok(meta) = joined.symlink_metadata() {
        if meta.file_type().is_symlink() {
            let canon = joined
                .canonicalize()
                .map_err(|_| "Dangling or invalid symlink is not allowed".to_string())?;
            if !canon.starts_with(&root) {
                return Err("Path escapes workspace root".to_string());
            }
            return Ok(canon);
        }
    }
    if joined.exists() {
        let canon = joined
            .canonicalize()
            .map_err(|e| format!("Invalid path {}: {}", rel_path, e))?;
        if !canon.starts_with(&root) {
            return Err("Path escapes workspace root".to_string());
        }
        Ok(canon)
    } else {
        // Verify nearest existing ancestor does not escape root via symlink
        let mut ancestor = joined.parent();
        while let Some(p) = ancestor {
            if p.exists() {
                let canon_parent = p
                    .canonicalize()
                    .map_err(|e| format!("Invalid parent path for {}: {}", rel_path, e))?;
                if !canon_parent.starts_with(&root) {
                    return Err("Path escapes workspace root".to_string());
                }
                break;
            }
            ancestor = p.parent();
        }
        Ok(joined)
    }
}

pub async fn read_workspace_file_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    rel_path: String,
    wsl_distro: Option<String>,
) -> Result<WorkspaceFile, String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    let full = resolve_path_for_open(&cwd, &rel_path)?;
    let bytes = std::fs::read(&full)
        .map_err(|e| format!("Failed to read {}: {}", rel_path, e))?;
    let content = String::from_utf8_lossy(&bytes);
    let lines = content.lines().map(ToString::to_string).collect();
    Ok(WorkspaceFile {
        path: rel_path.replace('\\', "/"),
        language: detect_language(&rel_path).to_string(),
        lines,
    })
}

pub async fn read_workspace_file_confined_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    rel_path: String,
    wsl_distro: Option<String>,
) -> Result<WorkspaceFile, String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    let full = safe_resolve_confined_path(&cwd, &rel_path)?;
    let bytes = std::fs::read(&full)
        .map_err(|e| format!("Failed to read {}: {}", rel_path, e))?;
    let content = String::from_utf8_lossy(&bytes);
    let lines = content.lines().map(ToString::to_string).collect();
    Ok(WorkspaceFile {
        path: rel_path.replace('\\', "/"),
        language: detect_language(&rel_path).to_string(),
        lines,
    })
}

#[tauri::command]
pub async fn read_workspace_file(
    state: State<'_, DbState>,
    cwd: String,
    rel_path: String,
    wsl_distro: Option<String>,
) -> Result<WorkspaceFile, String> {
    read_workspace_file_inner(&state.0, cwd, rel_path, wsl_distro).await
}

pub async fn write_workspace_file_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    rel_path: String,
    content: String,
    wsl_distro: Option<String>,
) -> Result<(), String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    let full = resolve_path_for_open(&cwd, &rel_path)?;
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent dirs for {}: {}", rel_path, e))?;
    }
    std::fs::write(&full, content.as_bytes())
        .map_err(|e| format!("Failed to write {}: {}", rel_path, e))?;
    Ok(())
}

pub async fn write_workspace_file_confined_inner(
    db_arc: &Arc<Mutex<Database>>,
    cwd: String,
    rel_path: String,
    content: String,
    wsl_distro: Option<String>,
) -> Result<(), String> {
    let cwd = resolve_effective_cwd(db_arc, &cwd, wsl_distro.as_deref()).await;
    let full = safe_resolve_confined_path(&cwd, &rel_path)?;
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent dirs for {}: {}", rel_path, e))?;
    }
    std::fs::write(&full, content.as_bytes())
        .map_err(|e| format!("Failed to write {}: {}", rel_path, e))?;
    Ok(())
}

#[tauri::command]
pub async fn write_workspace_file(
    state: State<'_, DbState>,
    cwd: String,
    rel_path: String,
    content: String,
    wsl_distro: Option<String>,
) -> Result<(), String> {
    write_workspace_file_inner(&state.0, cwd, rel_path, content, wsl_distro).await
}

pub async fn list_trace_spans_inner(
    db_arc: &Arc<Mutex<Database>>,
    session_id: String,
) -> Result<Vec<TraceSpan>, String> {
    let db = db_arc.lock().await;
    db.list_trace_spans(&session_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_trace_spans(
    state: State<'_, DbState>,
    session_id: String,
) -> Result<Vec<TraceSpan>, String> {
    list_trace_spans_inner(&state.0, session_id).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubAuthStatus {
    pub authenticated: bool,
    pub login: String,
    pub name: String,
    pub avatar_url: String,
    pub source: String,
    pub gh_installed: bool,
    pub default_clone_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubRepo {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub description: String,
    pub clone_url: String,
    pub html_url: String,
    pub private: bool,
    pub default_branch: String,
    pub language: String,
    pub updated_at: String,
    pub stargazers_count: u64,
}

fn default_github_clone_dir() -> String {
    let base = dirs::home_dir()
        .map(|h| h.join("Projects"))
        .unwrap_or_else(|| PathBuf::from("."));
    base.to_string_lossy().to_string()
}

fn is_gh_cli_installed() -> bool {
    let mut cmd = Command::new("gh");
    cmd.arg("--version");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.output().map(|o| o.status.success()).unwrap_or(false)
}

fn read_gh_cli_token() -> Option<String> {
    let mut cmd = Command::new("gh");
    cmd.args(["auth", "token"]);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let tok = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if tok.is_empty() { None } else { Some(tok) }
}

fn resolve_github_token() -> Option<(String, &'static str)> {
    if let Some(tok) = crate::platform::get_env_or_dotenv("GITHUB_TOKEN") {
        let t = tok.trim().to_string();
        if !t.is_empty() {
            return Some((t, "token"));
        }
    }
    if let Some(tok) = read_gh_cli_token() {
        return Some((tok, "gh-cli"));
    }
    None
}

fn base64_encode_bytes(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i];
        let b1 = if i + 1 < input.len() { input[i + 1] } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] } else { 0 };
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if i + 1 < input.len() {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < input.len() {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

async fn fetch_github_user_with_token(token: &str, source: &str) -> Result<GitHubAuthStatus, String> {
    let client = Client::new();
    let resp = client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "sakana-desktop")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("GitHub API request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("GitHub authentication failed ({status}): {body}"));
    }

    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse GitHub user response: {e}"))?;
    let login = val["login"].as_str().unwrap_or_default().to_string();
    let name = val["name"].as_str().unwrap_or(&login).to_string();
    let avatar_url = val["avatar_url"].as_str().unwrap_or_default().to_string();

    Ok(GitHubAuthStatus {
        authenticated: !login.is_empty(),
        login,
        name,
        avatar_url,
        source: source.to_string(),
        gh_installed: is_gh_cli_installed(),
        default_clone_dir: default_github_clone_dir(),
    })
}

#[tauri::command]
pub async fn github_auth_status() -> Result<GitHubAuthStatus, String> {
    let gh_installed = is_gh_cli_installed();
    let default_clone_dir = default_github_clone_dir();
    let Some((token, source)) = resolve_github_token() else {
        return Ok(GitHubAuthStatus {
            authenticated: false,
            login: String::new(),
            name: String::new(),
            avatar_url: String::new(),
            source: "none".to_string(),
            gh_installed,
            default_clone_dir,
        });
    };
    match fetch_github_user_with_token(&token, source).await {
        Ok(status) => Ok(status),
        Err(_) => Ok(GitHubAuthStatus {
            authenticated: false,
            login: String::new(),
            name: String::new(),
            avatar_url: String::new(),
            source: "none".to_string(),
            gh_installed,
            default_clone_dir,
        }),
    }
}

#[tauri::command]
pub async fn github_save_token(token: String) -> Result<GitHubAuthStatus, String> {
    let trimmed = token.trim().to_string();
    if trimmed.is_empty() {
        let _ = crate::telemetry::delete_env_secret_inner("GITHUB_TOKEN".to_string()).await;
        std::env::remove_var("GITHUB_TOKEN");
        return github_auth_status().await;
    }
    let status = fetch_github_user_with_token(&trimmed, "token").await?;
    crate::telemetry::set_env_secret_inner("GITHUB_TOKEN".to_string(), trimmed.clone()).await?;
    std::env::set_var("GITHUB_TOKEN", trimmed);
    Ok(status)
}

#[tauri::command]
pub async fn github_list_repos(query: Option<String>) -> Result<Vec<GitHubRepo>, String> {
    let (token, _) = resolve_github_token()
        .ok_or_else(|| "Not signed in to GitHub. Connect your GitHub account in Settings first.".to_string())?;
    let client = Client::new();
    let resp = client
        .get("https://api.github.com/user/repos?sort=updated&per_page=100&affiliation=owner,collaborator,organization_member")
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "sakana-desktop")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch GitHub repos: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("GitHub API error ({status}): {body}"));
    }

    let raw: Vec<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse GitHub repos: {e}"))?;

    let q = query.unwrap_or_default().trim().to_ascii_lowercase();
    let mut repos: Vec<GitHubRepo> = raw
        .into_iter()
        .map(|v| GitHubRepo {
            id: v["id"].as_u64().unwrap_or(0),
            name: v["name"].as_str().unwrap_or_default().to_string(),
            full_name: v["full_name"].as_str().unwrap_or_default().to_string(),
            description: v["description"].as_str().unwrap_or_default().to_string(),
            clone_url: v["clone_url"].as_str().unwrap_or_default().to_string(),
            html_url: v["html_url"].as_str().unwrap_or_default().to_string(),
            private: v["private"].as_bool().unwrap_or(false),
            default_branch: v["default_branch"].as_str().unwrap_or("main").to_string(),
            language: v["language"].as_str().unwrap_or_default().to_string(),
            updated_at: v["updated_at"].as_str().unwrap_or_default().to_string(),
            stargazers_count: v["stargazers_count"].as_u64().unwrap_or(0),
        })
        .filter(|r| !r.clone_url.is_empty())
        .collect();

    if !q.is_empty() {
        repos.retain(|r| {
            r.full_name.to_ascii_lowercase().contains(&q)
                || r.description.to_ascii_lowercase().contains(&q)
                || r.language.to_ascii_lowercase().contains(&q)
        });
    }

    Ok(repos)
}

#[tauri::command]
pub async fn github_clone_repo(
    clone_url: String,
    repo_name: String,
    target_parent_dir: Option<String>,
) -> Result<String, String> {
    let clean_url = clone_url.trim().to_string();
    if clean_url.is_empty() {
        return Err("Repository clone URL is required.".to_string());
    }
    if !(clean_url.starts_with("https://") || clean_url.starts_with("git@")) {
        return Err("Repository clone URL must start with https:// or git@".to_string());
    }
    let clean_name: String = repo_name
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    if clean_name.is_empty() {
        return Err("Invalid repository name.".to_string());
    }

    let parent_str = target_parent_dir
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(default_github_clone_dir);
    let parent = PathBuf::from(&parent_str);
    std::fs::create_dir_all(&parent)
        .map_err(|e| format!("Failed to create parent directory {}: {e}", parent.display()))?;

    let dest = parent.join(&clean_name);
    if dest.exists() {
        if dest.join(".git").exists() {
            return Ok(dest.to_string_lossy().to_string());
        }
        return Err(format!(
            "Directory '{}' already exists and is not a git repository.",
            dest.display()
        ));
    }

    let token_opt = resolve_github_token().map(|(t, _)| t);
    let dest_str = dest.to_string_lossy().to_string();

    let output = tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new("git");
        if let Some(tok) = token_opt {
            if clean_url.starts_with("https://github.com/") {
                let basic = base64_encode_bytes(format!("x-access-token:{tok}").as_bytes());
                cmd.env("GIT_CONFIG_COUNT", "1")
                    .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
                    .env("GIT_CONFIG_VALUE_0", format!("AUTHORIZATION: basic {basic}"));
            }
        }
        cmd.args(["clone", "--", &clean_url, &dest_str])
            .env("GIT_TERMINAL_PROMPT", "0");
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        cmd.output()
    })
    .await
    .map_err(|e| format!("Clone task failed: {e}"))?
    .map_err(|e| format!("Failed to run git clone: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(format!("git clone failed: {detail}"));
    }

    Ok(dest.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::resolve_effective_cwd;
    use crate::db::Database;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn resolve_effective_cwd_encodes_wsl_distro() {
        let dir = std::env::temp_dir().join(format!("sakana-git-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(Mutex::new(Database::open_in_dir(&dir).unwrap()));
        let resolved = resolve_effective_cwd(&db, "/home/user/repo", Some("Ubuntu")).await;
        assert_eq!(resolved, "wsl://Ubuntu/home/user/repo");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
