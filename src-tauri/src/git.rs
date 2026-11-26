use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use reqwest::Client;
use serde_json::json;
use tauri::State;
use uuid::Uuid;

use crate::models::{DiffFile, DiffHunk, DiffLine, TraceSpan, TurnCheckpoint, WorkspaceFile};
use crate::DbState;

const EMPTY_TREE_HASH: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

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
    let mut cmd = git_cmd(cwd);
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
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
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let src_str = src.to_string_lossy().to_string();
        let dst_str = dst.to_string_lossy().to_string();
        let mut cmd = Command::new("cmd");
        cmd.creation_flags(CREATE_NO_WINDOW);
        if src.is_dir() {
            cmd.args(["/C", "mklink", "/J", &dst_str, &src_str]);
        } else {
            cmd.args(["/C", "mklink", &dst_str, &src_str]);
        }
        let out = cmd
            .output()
            .map_err(|e| format!("Failed to run mklink for {}: {}", dst_str, e))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(format!("mklink failed for {}: {}", dst_str, err));
        }
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst)
            .map_err(|e| format!("Failed to symlink {} -> {}: {}", dst.display(), src.display(), e))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn create_worktree(
    session_id: String,
    project_cwd: String,
    branch: String,
    shared_deps: Vec<String>,
) -> Result<String, String> {
    let wt_dir = Path::new(&project_cwd)
        .join(".sakana")
        .join("worktrees")
        .join(&session_id);

    if let Some(parent) = wt_dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create worktree directory: {}", e))?;
    }

    let wt_str = wt_dir.to_string_lossy().to_string();
    if !wt_dir.exists() {
        let add_existing = run_git(&project_cwd, &["worktree", "add", &wt_str, &branch]);
        if add_existing.is_err() {
            run_git(&project_cwd, &["worktree", "add", "-b", &branch, &wt_str])?;
        }
    }

    for dep in shared_deps {
        let trimmed = dep.trim();
        if trimmed.is_empty() {
            continue;
        }
        let src = Path::new(&project_cwd).join(trimmed);
        let dst = wt_dir.join(trimmed);
        link_shared_dep(&src, &dst)?;
    }

    Ok(wt_str)
}

#[tauri::command]
pub async fn remove_worktree(session_id: String, project_cwd: String) -> Result<(), String> {
    let wt_dir = Path::new(&project_cwd)
        .join(".sakana")
        .join("worktrees")
        .join(&session_id);
    let wt_str = wt_dir.to_string_lossy().to_string();

    let _ = run_git(&project_cwd, &["worktree", "remove", "--force", &wt_str]);
    let _ = run_git(&project_cwd, &["worktree", "prune"]);

    if wt_dir.exists() {
        std::fs::remove_dir_all(&wt_dir)
            .map_err(|e| format!("Failed to remove worktree directory {}: {}", wt_str, e))?;
    }
    Ok(())
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

#[tauri::command]
pub async fn list_branches(cwd: String) -> Result<Vec<String>, String> {
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
pub async fn checkout_branch(cwd: String, branch: String) -> Result<(), String> {
    ensure_git_repo(&cwd)?;
    run_git(&cwd, &["checkout", &branch])?;
    Ok(())
}

#[tauri::command]
pub async fn git_pull(cwd: String) -> Result<String, String> {
    ensure_git_repo(&cwd)?;
    run_git(&cwd, &["pull"])
}

#[tauri::command]
pub async fn git_push(cwd: String) -> Result<String, String> {
    ensure_git_repo(&cwd)?;
    match run_git(&cwd, &["push"]) {
        Ok(out) => Ok(out),
        Err(_) => run_git(&cwd, &["push", "-u", "origin", "HEAD"]),
    }
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

#[tauri::command]
pub async fn create_checkpoint(
    state: State<'_, DbState>,
    session_id: String,
    turn_number: i64,
    label: String,
    cwd: String,
) -> Result<Option<TurnCheckpoint>, String> {
    let git_dir = resolve_git_dir(&cwd)?;
    let orig_index = git_dir.join("index");
    let temp_index = std::env::temp_dir().join(format!("sakana-idx-{}", Uuid::new_v4()));

    if orig_index.exists() {
        std::fs::copy(&orig_index, &temp_index)
            .map_err(|e| format!("Failed to copy git index: {}", e))?;
    }

    let temp_idx_str = temp_index.to_string_lossy().to_string();
    let add_res = run_git_with_env(&cwd, &["add", "-A"], &[("GIT_INDEX_FILE", &temp_idx_str)]);
    if let Err(e) = add_res {
        let _ = std::fs::remove_file(&temp_index);
        return Err(e);
    }

    let tree_res = run_git_with_env(
        &cwd,
        &["write-tree"],
        &[("GIT_INDEX_FILE", &temp_idx_str)],
    );
    let _ = std::fs::remove_file(&temp_index);
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

    let db = state.0.lock().await;
    db.insert_checkpoint(&session_id, &checkpoint)
        .map_err(|e| e.to_string())?;

    Ok(Some(checkpoint))
}

#[tauri::command]
pub async fn get_checkpoint_diff(
    state: State<'_, DbState>,
    session_id: String,
    checkpoint_id: String,
) -> Result<Vec<DiffFile>, String> {
    let (cwd, cp, all_cps) = {
        let db = state.0.lock().await;
        let session = db
            .get_session(&session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Session {} not found", session_id))?;
        let cp = db
            .get_checkpoint(&session_id, &checkpoint_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Checkpoint {} not found", checkpoint_id))?;
        let all_cps = db.list_checkpoints(&session_id).map_err(|e| e.to_string())?;
        (session.worktree_path, cp, all_cps)
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
pub async fn revert_to_checkpoint(
    state: State<'_, DbState>,
    session_id: String,
    checkpoint_id: String,
) -> Result<(), String> {
    let (cwd, cp) = {
        let db = state.0.lock().await;
        let session = db
            .get_session(&session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Session {} not found", session_id))?;
        let cp = db
            .get_checkpoint(&session_id, &checkpoint_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Checkpoint {} not found", checkpoint_id))?;
        (session.worktree_path, cp)
    };

    run_git(&cwd, &["read-tree", "--reset", "-u", &cp.commit_hash])?;

    let db = state.0.lock().await;
    db.truncate_after_checkpoint(&session_id, &checkpoint_id, cp.turn_number)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_git_diffs(
    cwd: String,
    turn_id: Option<String>,
) -> Result<Vec<DiffFile>, String> {
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
pub async fn stage_hunk(
    cwd: String,
    file_path: String,
    hunk_header: String,
    stage: bool,
) -> Result<(), String> {
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
pub async fn discard_hunk(
    cwd: String,
    file_path: String,
    hunk_header: String,
) -> Result<(), String> {
    ensure_git_repo(&cwd)?;
    let raw = run_git(&cwd, &["diff", "--no-color", "--", &file_path])?;
    if raw.trim().is_empty() {
        let untracked =
            run_git(&cwd, &["ls-files", "--others", "--exclude-standard", "--", &file_path])
                .unwrap_or_default();
        if !untracked.trim().is_empty() {
            let full = safe_resolve_path(&cwd, &file_path)?;
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
pub async fn draft_commit_message(cwd: String) -> Result<String, String> {
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
pub async fn git_commit(cwd: String, message: String) -> Result<String, String> {
    ensure_git_repo(&cwd)?;
    let staged = run_git(&cwd, &["diff", "--cached", "--name-only"]).unwrap_or_default();
    if staged.trim().is_empty() {
        run_git(&cwd, &["add", "-A"])?;
    }
    run_git(&cwd, &["commit", "-m", &message])
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

#[tauri::command]
pub async fn create_pr(
    cwd: String,
    title: String,
    body: String,
    base_branch: String,
) -> Result<String, String> {
    ensure_git_repo(&cwd)?;
    let mut gh_cmd = Command::new("gh");
    gh_cmd
        .current_dir(&cwd)
        .args(["pr", "create", "--title", &title, "--body", &body, "--base", &base_branch]);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        gh_cmd.creation_flags(0x0800_0000);
    }

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
        .unwrap_or("PR created")
        .to_string())
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

#[tauri::command]
pub async fn list_workspace_files(cwd: String) -> Result<Vec<String>, String> {
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

fn safe_resolve_path(cwd: &str, rel_path: &str) -> Result<PathBuf, String> {
    let root_buf = resolve_fs_root(cwd);
    let root = root_buf
        .canonicalize()
        .map_err(|e| format!("Invalid workspace cwd {}: {}", cwd, e))?;
    let clean_rel = rel_path.trim_start_matches(['/', '\\']);
    let joined = root.join(clean_rel);
    if joined.exists() {
        let canon = joined
            .canonicalize()
            .map_err(|e| format!("Invalid path {}: {}", rel_path, e))?;
        if !canon.starts_with(&root) {
            return Err("Path escapes workspace root".to_string());
        }
        Ok(canon)
    } else {
        for comp in Path::new(clean_rel).components() {
            if matches!(comp, std::path::Component::ParentDir) {
                return Err("Path traversal is not allowed".to_string());
            }
        }
        Ok(joined)
    }
}

#[tauri::command]
pub async fn read_workspace_file(cwd: String, rel_path: String) -> Result<WorkspaceFile, String> {
    let full = safe_resolve_path(&cwd, &rel_path)?;
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
pub async fn write_workspace_file(
    cwd: String,
    rel_path: String,
    content: String,
) -> Result<(), String> {
    let full = safe_resolve_path(&cwd, &rel_path)?;
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent dirs for {}: {}", rel_path, e))?;
    }
    std::fs::write(&full, content.as_bytes())
        .map_err(|e| format!("Failed to write {}: {}", rel_path, e))?;
    Ok(())
}

#[tauri::command]
pub async fn list_trace_spans(
    state: State<'_, DbState>,
    session_id: String,
) -> Result<Vec<TraceSpan>, String> {
    let db = state.0.lock().await;
    db.list_trace_spans(&session_id).map_err(|e| e.to_string())
}
