use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use tauri::{AppHandle, State};
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use crate::db::Database;
use crate::harness::{self, HarnessState, TurnRunOptions};
use crate::models::{CreateSessionInput, RaceContestant, RaceContestantInput, RaceRun};
use crate::DbState;

#[derive(Default)]
pub struct RaceState {
    pub runs: Arc<Mutex<HashMap<String, RaceRun>>>,
}

fn run_git_cmd(cwd: &str, wsl_distro: Option<&str>, args: &[&str]) -> Result<String, String> {
    let mut cmd = if let Some((distro, linux_path)) = crate::platform::parse_wsl_path(cwd) {
        let mut all_args = Vec::with_capacity(args.len() + 2);
        all_args.push("-C");
        all_args.push(linux_path.as_str());
        all_args.extend_from_slice(args);
        crate::platform::wsl_std_command(&distro, None, "git", &all_args)
    } else if let Some(distro) = wsl_distro {
        let mut all_args = Vec::with_capacity(args.len() + 2);
        all_args.push("-C");
        all_args.push(cwd);
        all_args.extend_from_slice(args);
        crate::platform::wsl_std_command(distro, None, "git", &all_args)
    } else {
        let mut c = crate::platform::std_command("git");
        c.arg("-C").arg(cwd).args(args);
        c
    };
    let out = cmd
        .output()
        .map_err(|e| format!("git {:?} failed: {e}", args))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("git {:?} exited with {}", args, out.status)
        } else {
            stderr
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn compute_worktree_diff_stats(worktree_path: &str, wsl_distro: Option<&str>) -> (i64, i64, i64) {
    let numstat = run_git_cmd(worktree_path, wsl_distro, &["diff", "--numstat", "HEAD"]).unwrap_or_default();
    let mut files = 0i64;
    let mut adds = 0i64;
    let mut dels = 0i64;
    for line in numstat.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            files += 1;
            adds += parts[0].trim().parse::<i64>().unwrap_or(0);
            dels += parts[1].trim().parse::<i64>().unwrap_or(0);
        }
    }
    if let Ok(untracked) = run_git_cmd(
        worktree_path,
        wsl_distro,
        &["ls-files", "--others", "--exclude-standard"],
    ) {
        let fs_root = if let Some((distro, linux_path)) = crate::platform::parse_wsl_path(worktree_path) {
            crate::platform::wsl_to_unc_path(&distro, &linux_path)
        } else if let Some(distro) = wsl_distro {
            crate::platform::wsl_to_unc_path(distro, worktree_path)
        } else {
            std::path::PathBuf::from(worktree_path)
        };
        for rel in untracked.lines().map(str::trim).filter(|s| !s.is_empty()) {
            files += 1;
            if let Ok(text) = std::fs::read_to_string(fs_root.join(rel)) {
                adds += text.lines().count() as i64;
            }
        }
    }
    (files, adds, dels)
}

async fn run_verify_command(
    worktree_path: &str,
    wsl_distro: Option<&str>,
    verify_cmd: &str,
) -> (bool, String) {
    let mut cmd = if let Some((distro, linux_path)) = crate::platform::parse_wsl_path(worktree_path) {
        crate::platform::wsl_tokio_shell(&distro, &linux_path, verify_cmd)
    } else if let Some(distro) = wsl_distro {
        crate::platform::wsl_tokio_shell(distro, worktree_path, verify_cmd)
    } else if cfg!(target_os = "windows") {
        let mut c = crate::platform::tokio_command("cmd");
        c.args(["/C", verify_cmd]);
        c.current_dir(worktree_path);
        c
    } else {
        let mut c = crate::platform::tokio_command("sh");
        c.args(["-lc", verify_cmd]);
        c.current_dir(worktree_path);
        c
    };
    cmd.kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match tokio::time::timeout(Duration::from_secs(180), cmd.output()).await {
        Ok(Ok(out)) => {
            let mut combined = String::from_utf8_lossy(&out.stdout).to_string();
            let err = String::from_utf8_lossy(&out.stderr);
            if !err.trim().is_empty() {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(&err);
            }
            let clipped: String = combined.chars().take(1500).collect();
            (out.status.success(), clipped)
        }
        Ok(Err(e)) => (false, format!("Verify spawn error: {e}")),
        Err(_) => (false, "Verify command timed out after 180s".to_string()),
    }
}

pub async fn start_race_inner(
    app: Option<AppHandle>,
    db_arc: Arc<Mutex<Database>>,
    active_arc: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    race_runs: Arc<Mutex<HashMap<String, RaceRun>>>,
    project_id: String,
    prompt: String,
    contestants_in: Vec<RaceContestantInput>,
    verify_cmd: Option<String>,
) -> Result<RaceRun, String> {
    let prompt_trimmed = prompt.trim().to_string();
    if prompt_trimmed.is_empty() {
        return Err("Race prompt cannot be empty".to_string());
    }
    if contestants_in.len() < 2 || contestants_in.len() > 3 {
        return Err("Race mode requires 2 to 3 contestants".to_string());
    }

    let project = {
        let db = db_arc.lock().await;
        db.get_project(&project_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Project '{}' not found", project_id))?
    };

    let base_cwd = project.cwd.clone();
    let wsl_distro = project.wsl_distro.clone();
    let encoded_base_cwd = crate::platform::encode_wsl_path(wsl_distro.as_deref(), &base_cwd);
    let base_cwd_for_git = base_cwd.clone();
    let wsl_for_git = wsl_distro.clone();
    let base_branch = tokio::task::spawn_blocking(move || {
        run_git_cmd(&base_cwd_for_git, wsl_for_git.as_deref(), &["rev-parse", "--abbrev-ref", "HEAD"])
    })
    .await
    .ok()
    .and_then(Result::ok)
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "main".to_string());

    let race_id = format!("race-{}", &Uuid::new_v4().to_string()[..8]);
    let mut contestants: Vec<RaceContestant> = Vec::new();

    for (idx, c_in) in contestants_in.iter().enumerate() {
        let slug = format!("{}-{}", race_id, idx + 1);
        let branch = format!("sakana-race/{}-{}", slug, c_in.harness);
        let wt_path = match crate::git::create_worktree_inner(
            &db_arc,
            slug.clone(),
            encoded_base_cwd.clone(),
            branch.clone(),
            vec![],
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                // Clean up already created worktrees on failure
                for created in &contestants {
                    let _ = crate::git::remove_worktree_inner(
                        &db_arc,
                        created.worktree_slug.clone(),
                        encoded_base_cwd.clone(),
                    )
                    .await;
                }
                return Err(format!("Failed to create worktree for {}: {e}", c_in.harness));
            }
        };

        let session = {
            let db = db_arc.lock().await;
            db.create_session(CreateSessionInput {
                project_id: project_id.clone(),
                title: format!("Race #{}: {}", idx + 1, c_in.harness),
                harness: Some(c_in.harness.clone()),
                model: if c_in.model.trim().is_empty() {
                    None
                } else {
                    Some(c_in.model.clone())
                },
                effort: Some("medium".to_string()),
                account: c_in.account.clone(),
                permission_mode: Some("auto-edit".to_string()),
                worktree_path: Some(wt_path.clone()),
                branch: Some(branch.clone()),
                shared_deps: Some(vec![]),
                discord_channel: None,
                is_discord: Some(false),
            })
            .map_err(|e| e.to_string())?
        };

        contestants.push(RaceContestant {
            session_id: session.id,
            harness: c_in.harness.clone(),
            model: c_in.model.clone(),
            branch,
            worktree_path: wt_path,
            worktree_slug: slug,
            status: "running".to_string(),
            output: String::new(),
            duration_ms: 0,
            files_changed: 0,
            additions: 0,
            deletions: 0,
            verify_ok: None,
            verify_output: None,
            error: None,
        });
    }

    let run = RaceRun {
        id: race_id.clone(),
        project_id: project_id.clone(),
        base_branch,
        base_cwd,
        wsl_distro: wsl_distro.clone(),
        prompt: prompt_trimmed.clone(),
        status: "running".to_string(),
        winner_session_id: None,
        verify_cmd: verify_cmd.clone().filter(|s| !s.trim().is_empty()),
        created_at: Utc::now().to_rfc3339(),
        contestants: contestants.clone(),
    };

    {
        let mut map = race_runs.lock().await;
        map.insert(race_id.clone(), run.clone());
    }
    crate::platform::emit_app_and_remote(app.as_ref(), "race:update", &run);

    // Spawn each contestant concurrently
    for contestant in contestants {
        let app_c = app.clone();
        let db_c = Arc::clone(&db_arc);
        let active_c = Arc::clone(&active_arc);
        let runs_c = Arc::clone(&race_runs);
        let rid = race_id.clone();
        let sid = contestant.session_id.clone();
        let wt = contestant.worktree_path.clone();
        let wsl_c = wsl_distro.clone();
        let prompt_c = prompt_trimmed.clone();
        let vcmd = run.verify_cmd.clone();

        tokio::spawn(async move {
            let started = Instant::now();
            let (tx, rx) = oneshot::channel();
            let dispatch_res = harness::send_turn_with_options(
                app_c.clone(),
                Arc::clone(&db_c),
                Arc::clone(&active_c),
                sid.clone(),
                prompt_c,
                None,
                TurnRunOptions {
                    completion_tx: Some(tx),
                    ..Default::default()
                },
            )
            .await;

            let turn_res = match dispatch_res {
                Ok(_) => match rx.await {
                    Ok(r) => r,
                    Err(_) => Err("Contestant turn channel closed".to_string()),
                },
                Err(e) => Err(e),
            };

            let duration_ms = started.elapsed().as_millis().max(1) as i64;
            let wt_stats = wt.clone();
            let wsl_stats = wsl_c.clone();
            let (files_changed, additions, deletions) = tokio::task::spawn_blocking(move || {
                compute_worktree_diff_stats(&wt_stats, wsl_stats.as_deref())
            })
            .await
            .unwrap_or((0, 0, 0));
            let (verify_ok, verify_output) = if let Some(ref cmd_str) = vcmd {
                let (ok, out) = run_verify_command(&wt, wsl_c.as_deref(), cmd_str).await;
                (Some(ok), Some(out))
            } else {
                (None, None)
            };

            let snapshot = {
                let mut map = runs_c.lock().await;
                if let Some(r) = map.get_mut(&rid) {
                    if let Some(c) = r.contestants.iter_mut().find(|x| x.session_id == sid) {
                        c.duration_ms = duration_ms;
                        c.files_changed = files_changed;
                        c.additions = additions;
                        c.deletions = deletions;
                        c.verify_ok = verify_ok;
                        c.verify_output = verify_output;
                        match turn_res {
                            Ok(msg) => {
                                c.status = "done".to_string();
                                c.output = msg.text;
                            }
                            Err(err) => {
                                c.status = "error".to_string();
                                c.error = Some(err);
                            }
                        }
                    }
                    if r.contestants.iter().all(|c| c.status != "running") && r.status == "running" {
                        r.status = "completed".to_string();
                    }
                    Some(r.clone())
                } else {
                    None
                }
            };

            if let Some(snap) = snapshot {
                crate::platform::emit_app_and_remote(app_c.as_ref(), "race:update", &snap);
            }
        });
    }

    Ok(run)
}

pub async fn list_races_inner(race_runs: &Arc<Mutex<HashMap<String, RaceRun>>>) -> Vec<RaceRun> {
    let map = race_runs.lock().await;
    let mut list: Vec<RaceRun> = map.values().cloned().collect();
    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    list
}

pub async fn get_race_inner(
    race_runs: &Arc<Mutex<HashMap<String, RaceRun>>>,
    race_id: &str,
) -> Option<RaceRun> {
    let map = race_runs.lock().await;
    map.get(race_id).cloned()
}

pub async fn merge_race_winner_inner(
    app: Option<&AppHandle>,
    db_arc: &Arc<Mutex<Database>>,
    race_runs: &Arc<Mutex<HashMap<String, RaceRun>>>,
    race_id: String,
    winner_session_id: String,
) -> Result<RaceRun, String> {
    let run = {
        let map = race_runs.lock().await;
        map.get(&race_id)
            .cloned()
            .ok_or_else(|| format!("Race '{}' not found", race_id))?
    };

    let winner = run
        .contestants
        .iter()
        .find(|c| c.session_id == winner_session_id)
        .ok_or_else(|| format!("Winner session '{}' not found in race", winner_session_id))?
        .clone();

    let wsl_distro = run.wsl_distro.clone();
    let encoded_base_cwd = crate::platform::encode_wsl_path(wsl_distro.as_deref(), &run.base_cwd);

    // Commit any uncommitted changes inside the winning worktree and merge into base workspace
    let winner_wt = winner.worktree_path.clone();
    let winner_harness = winner.harness.clone();
    let winner_branch = winner.branch.clone();
    let prompt_snippet: String = run.prompt.chars().take(60).collect();
    let base_cwd_merge = run.base_cwd.clone();
    let wsl_merge = wsl_distro.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let status = run_git_cmd(&winner_wt, wsl_merge.as_deref(), &["status", "--porcelain"])
            .unwrap_or_default();
        if !status.trim().is_empty() {
            run_git_cmd(&winner_wt, wsl_merge.as_deref(), &["add", "-A"])?;
            let commit_msg = format!("race({}): {}", winner_harness, prompt_snippet);
            run_git_cmd(&winner_wt, wsl_merge.as_deref(), &["commit", "-m", &commit_msg])?;
        }
        run_git_cmd(
            &base_cwd_merge,
            wsl_merge.as_deref(),
            &[
                "merge",
                "--no-ff",
                &winner_branch,
                "-m",
                &format!("Merge race winner ({})", winner_harness),
            ],
        )?;
        Ok(())
    })
    .await
    .map_err(|e| format!("Merge task panicked: {e}"))??;

    // Clean up all race worktrees and branches
    for (idx, c) in run.contestants.iter().enumerate() {
        let slug = if !c.worktree_slug.is_empty() {
            c.worktree_slug.clone()
        } else {
            format!("{}-{}", run.id, idx + 1)
        };
        let _ = crate::git::remove_worktree_inner(db_arc, slug, encoded_base_cwd.clone()).await;
        let base_cwd_del = run.base_cwd.clone();
        let wsl_del = wsl_distro.clone();
        let branch_del = c.branch.clone();
        let _ = tokio::task::spawn_blocking(move || {
            run_git_cmd(&base_cwd_del, wsl_del.as_deref(), &["branch", "-D", &branch_del])
        })
        .await;
    }

    let updated = {
        let mut map = race_runs.lock().await;
        if let Some(r) = map.get_mut(&race_id) {
            r.status = "merged".to_string();
            r.winner_session_id = Some(winner_session_id.clone());
            for c in &mut r.contestants {
                if c.session_id == winner_session_id {
                    c.status = "merged".to_string();
                }
            }
            r.clone()
        } else {
            return Err("Race vanished".to_string());
        }
    };

    crate::platform::emit_app_and_remote(app, "race:update", &updated);
    Ok(updated)
}

#[tauri::command]
pub async fn start_race(
    app: AppHandle,
    db_state: State<'_, DbState>,
    harness_state: State<'_, HarnessState>,
    race_state: State<'_, RaceState>,
    project_id: String,
    prompt: String,
    contestants: Vec<RaceContestantInput>,
    verify_cmd: Option<String>,
) -> Result<RaceRun, String> {
    start_race_inner(
        Some(app),
        Arc::clone(&db_state.0),
        Arc::clone(&harness_state.active_turns),
        Arc::clone(&race_state.runs),
        project_id,
        prompt,
        contestants,
        verify_cmd,
    )
    .await
}

#[tauri::command]
pub async fn list_races(race_state: State<'_, RaceState>) -> Result<Vec<RaceRun>, String> {
    Ok(list_races_inner(&race_state.runs).await)
}

#[tauri::command]
pub async fn get_race(
    race_state: State<'_, RaceState>,
    race_id: String,
) -> Result<Option<RaceRun>, String> {
    Ok(get_race_inner(&race_state.runs, &race_id).await)
}

#[tauri::command]
pub async fn merge_race_winner(
    app: AppHandle,
    db_state: State<'_, DbState>,
    race_state: State<'_, RaceState>,
    race_id: String,
    winner_session_id: String,
) -> Result<RaceRun, String> {
    merge_race_winner_inner(
        Some(&app),
        &db_state.0,
        &race_state.runs,
        race_id,
        winner_session_id,
    )
    .await
}
