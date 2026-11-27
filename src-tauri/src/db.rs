use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::models::{
    AutomationHistoryItem, AutomationItem, BootstrapState, ChatMessage, CreateProjectInput,
    CreateSessionInput, DiscordUserRule, HarnessAccountRow, McpServer, Office, OfficeEvent,
    OfficeStaff, PendingApproval, Project, QuotaAccount, RemoteHost, Session, Thread, TraceSpan,
    TurnCheckpoint, UpdateSessionInput, UpdateStaffInput, UpsertRemoteHostInput,
};

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open_default() -> Result<Self> {
        let db_dir = crate::platform::app_data_dir();
        Self::open_in_dir(&db_dir)
    }

    pub fn open_in_dir(dir: &std::path::Path) -> Result<Self> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create db dir: {}", dir.display()))?;
        let db_path = dir.join("sakana.db");
        Self::open_path(db_path)
    }

    pub fn open_path(path: PathBuf) -> Result<Self> {
        let conn = Connection::open(&path)
            .with_context(|| format!("failed to open sqlite db: {}", path.display()))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;",
        )?;
        let db = Self { conn };
        db.init_schema()?;
        db.seed_if_empty()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        const SCHEMA_VERSION: i64 = 1;
        let current_version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);

        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS settings_kv (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                cwd TEXT NOT NULL,
                repo_url TEXT,
                discord_channel TEXT,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                title TEXT NOT NULL,
                status TEXT NOT NULL,
                harness TEXT NOT NULL,
                model TEXT NOT NULL,
                effort TEXT NOT NULL,
                account TEXT NOT NULL,
                permission_mode TEXT NOT NULL,
                worktree_path TEXT NOT NULL,
                branch TEXT NOT NULL,
                shared_deps_json TEXT NOT NULL,
                additions INTEGER NOT NULL DEFAULT 0,
                deletions INTEGER NOT NULL DEFAULT 0,
                time_ago TEXT NOT NULL,
                is_discord INTEGER NOT NULL DEFAULT 0,
                discord_channel TEXT,
                unread_done INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                role TEXT NOT NULL,
                author TEXT,
                timestamp TEXT NOT NULL,
                text TEXT NOT NULL,
                thinking TEXT,
                thinking_duration TEXT,
                tools_json TEXT,
                subagent_json TEXT,
                widget_json TEXT,
                checkpoint_json TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS checkpoints (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                turn_number INTEGER NOT NULL,
                commit_hash TEXT NOT NULL,
                label TEXT NOT NULL,
                files_changed INTEGER NOT NULL DEFAULT 0,
                additions INTEGER NOT NULL DEFAULT 0,
                deletions INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS trace_spans (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                parent_id TEXT,
                actor TEXT NOT NULL,
                harness TEXT NOT NULL,
                action TEXT NOT NULL,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                tokens TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS approvals (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                command TEXT NOT NULL,
                cwd TEXT NOT NULL,
                reason TEXT NOT NULL,
                risk TEXT NOT NULL,
                requested_by TEXT NOT NULL,
                resolved INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS accounts (
                id TEXT PRIMARY KEY,
                harness TEXT NOT NULL,
                label TEXT NOT NULL,
                email TEXT NOT NULL,
                five_hour_pct REAL NOT NULL DEFAULT 0,
                five_hour_reset TEXT NOT NULL,
                weekly_pct REAL NOT NULL DEFAULT 0,
                weekly_reset TEXT NOT NULL,
                is_default INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS usage (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                harness TEXT NOT NULL,
                model TEXT NOT NULL,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                cost_usd REAL NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS mcp_servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                transport TEXT NOT NULL,
                endpoint TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                status TEXT NOT NULL,
                latency_ms INTEGER
            );

            CREATE TABLE IF NOT EXISTS automations (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                schedule TEXT NOT NULL,
                prompt TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                last_run_status TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS automation_runs (
                id TEXT PRIMARY KEY,
                automation_id TEXT NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
                ts TEXT NOT NULL,
                status TEXT NOT NULL,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                summary TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS discord_rbac (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                label TEXT NOT NULL,
                role TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS harness_accounts (
                id TEXT PRIMARY KEY,
                harness TEXT NOT NULL,
                label TEXT NOT NULL,
                email TEXT NOT NULL DEFAULT '',
                profile_dir TEXT NOT NULL DEFAULT '',
                auth_type TEXT NOT NULL DEFAULT 'oauth-personal',
                is_default INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'unauthenticated',
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS remote_hosts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                url TEXT NOT NULL,
                token TEXT NOT NULL,
                os TEXT NOT NULL DEFAULT '',
                hostname TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'unknown',
                last_seen_at TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS offices (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                cwd TEXT NOT NULL,
                master_staff_id TEXT NOT NULL DEFAULT '',
                max_rounds INTEGER NOT NULL DEFAULT 6,
                status TEXT NOT NULL DEFAULT 'idle',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS office_staff (
                id TEXT PRIMARY KEY,
                office_id TEXT NOT NULL REFERENCES offices(id) ON DELETE CASCADE,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                role TEXT NOT NULL,
                is_master INTEGER NOT NULL DEFAULT 0,
                is_dynamic INTEGER NOT NULL DEFAULT 0,
                harness TEXT NOT NULL,
                model TEXT NOT NULL,
                effort TEXT NOT NULL,
                account TEXT NOT NULL,
                permission_mode TEXT NOT NULL,
                system_prompt TEXT NOT NULL DEFAULT '',
                isolated_worktree INTEGER NOT NULL DEFAULT 0,
                worktree_path TEXT NOT NULL DEFAULT '',
                branch TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'idle',
                current_task TEXT,
                avatar_color TEXT NOT NULL DEFAULT '#6366f1',
                desk_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS office_events (
                id TEXT PRIMARY KEY,
                office_id TEXT NOT NULL REFERENCES offices(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                from_staff_id TEXT,
                from_name TEXT,
                to_staff_id TEXT,
                to_name TEXT,
                summary TEXT NOT NULL,
                detail TEXT,
                timestamp TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_office_staff_office ON office_staff(office_id, desk_order);
            CREATE INDEX IF NOT EXISTS idx_office_events_office ON office_events(office_id, timestamp);
            ",
        )?;
        // Idempotent migrations for native WSL support on projects
        let _ = self.conn.execute(
            "ALTER TABLE projects ADD COLUMN runtime TEXT NOT NULL DEFAULT 'native'",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE projects ADD COLUMN wsl_distro TEXT",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE projects ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE sessions ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
            [],
        );
        // Idempotently add office_id column to sessions
        let has_office_id = {
            let mut stmt = self.conn.prepare("PRAGMA table_info(sessions)")?;
            let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
            let mut found = false;
            for col in cols {
                if col? == "office_id" {
                    found = true;
                    break;
                }
            }
            found
        };
        if !has_office_id {
            self.conn
                .execute("ALTER TABLE sessions ADD COLUMN office_id TEXT", [])?;
        }
        if current_version < SCHEMA_VERSION {
            self.conn
                .execute_batch(&format!("PRAGMA user_version = {};", SCHEMA_VERSION))?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        let v: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        Ok(v)
    }

    fn seed_if_empty(&self) -> Result<()> {
        if self.get_setting("theme")?.is_none() {
            self.set_setting("theme", "paper")?;
        }

        // Remove any previously auto-seeded dummy session on startup
        self.conn.execute(
            "DELETE FROM sessions WHERE title = 'Welcome to Sakana Desktop'",
            [],
        )?;

        // Seed default CLI harness accounts if missing
        let now = Utc::now().to_rfc3339();
        let defaults = [
            ("default", "agy-cli", "Primary AGY", "oauth-personal"),
            ("kiro_default", "kiro-cli", "Primary Kiro", "device-flow"),
            ("claude_default", "claude-code", "Primary Claude", "claude-oauth"),
        ];
        for (id, harness, label, auth_type) in defaults {
            self.conn.execute(
                "INSERT OR IGNORE INTO harness_accounts
                 (id, harness, label, email, profile_dir, auth_type, is_default, status, created_at)
                 VALUES (?1, ?2, ?3, '', '', ?4, 1, 'unauthenticated', ?5)",
                params![id, harness, label, auth_type, now],
            )?;
        }

        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let val = self
            .conn
            .query_row(
                "SELECT value FROM settings_kv WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(val)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings_kv (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_theme(&self) -> Result<String> {
        Ok(self
            .get_setting("theme")?
            .unwrap_or_else(|| "paper".to_string()))
    }

    pub fn set_theme(&self, theme: &str) -> Result<()> {
        self.set_setting("theme", theme)
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, cwd, repo_url, discord_channel, runtime, wsl_distro
             FROM projects
             ORDER BY sort_order ASC, created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                cwd: row.get(2)?,
                repo_url: row.get(3)?,
                discord_channel: row.get(4)?,
                runtime: row.get(5)?,
                wsl_distro: row.get(6)?,
            })
        })?;

        let mut projects = Vec::new();
        for row in rows {
            projects.push(row?);
        }
        Ok(projects)
    }

    pub fn create_project(&self, input: CreateProjectInput) -> Result<Project> {
        let id = format!("proj-{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();
        let (cwd, runtime, wsl_distro) = if let Some((parsed_distro, parsed_linux)) =
            crate::platform::parse_wsl_path(&input.cwd)
        {
            (
                parsed_linux,
                "wsl".to_string(),
                input.wsl_distro.or(Some(parsed_distro)),
            )
        } else {
            let rt = input
                .runtime
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| {
                    if input.wsl_distro.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false) {
                        "wsl".to_string()
                    } else {
                        "native".to_string()
                    }
                });
            (input.cwd, rt, input.wsl_distro.filter(|s| !s.trim().is_empty()))
        };
        self.conn.execute(
            "INSERT INTO projects (id, name, cwd, repo_url, discord_channel, runtime, wsl_distro, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                input.name,
                cwd,
                input.repo_url,
                input.discord_channel,
                runtime,
                wsl_distro,
                now
            ],
        )?;
        Ok(Project {
            id,
            name: input.name,
            cwd,
            repo_url: input.repo_url,
            discord_channel: input.discord_channel,
            runtime: Some(runtime),
            wsl_distro,
        })
    }

    pub fn get_project(&self, id: &str) -> Result<Option<Project>> {
        let project = self
            .conn
            .query_row(
                "SELECT id, name, cwd, repo_url, discord_channel, runtime, wsl_distro
                 FROM projects
                 WHERE id = ?1",
                params![id],
                |row| {
                    Ok(Project {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        cwd: row.get(2)?,
                        repo_url: row.get(3)?,
                        discord_channel: row.get(4)?,
                        runtime: row.get(5)?,
                        wsl_distro: row.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(project)
    }

    pub fn list_remote_hosts(&self) -> Result<Vec<RemoteHost>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, url, token, os, hostname, status, last_seen_at
             FROM remote_hosts
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(RemoteHost {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                token: row.get(3)?,
                os: row.get(4)?,
                hostname: row.get(5)?,
                status: row.get(6)?,
                last_seen_at: row.get(7)?,
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn get_remote_host(&self, id: &str) -> Result<Option<RemoteHost>> {
        self.conn
            .query_row(
                "SELECT id, name, url, token, os, hostname, status, last_seen_at
                 FROM remote_hosts WHERE id = ?1",
                params![id],
                |row| {
                    Ok(RemoteHost {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        url: row.get(2)?,
                        token: row.get(3)?,
                        os: row.get(4)?,
                        hostname: row.get(5)?,
                        status: row.get(6)?,
                        last_seen_at: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn upsert_remote_host(&self, input: UpsertRemoteHostInput) -> Result<RemoteHost> {
        let id = input
            .id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("host-{}", &Uuid::new_v4().simple().to_string()[..8]));
        let now = Utc::now().to_rfc3339();
        let clean_url = input.url.trim().trim_end_matches('/').to_string();
        let clean_name = if input.name.trim().is_empty() {
            clean_url.clone()
        } else {
            input.name.trim().to_string()
        };
        let clean_token = input.token.trim().to_string();
        self.conn.execute(
            "INSERT INTO remote_hosts (id, name, url, token, os, hostname, status, last_seen_at, created_at)
             VALUES (?1, ?2, ?3, ?4, '', '', 'unknown', '', ?5)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, url = excluded.url, token = excluded.token",
            params![id, clean_name, clean_url, clean_token, now],
        )?;
        self.get_remote_host(&id)?
            .ok_or_else(|| anyhow::anyhow!("Failed to read back remote host"))
    }

    pub fn update_remote_host_status(
        &self,
        id: &str,
        status: &str,
        os: Option<&str>,
        hostname: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE remote_hosts SET status = ?2, last_seen_at = ?3,
                os = COALESCE(NULLIF(?4, ''), os),
                hostname = COALESCE(NULLIF(?5, ''), hostname)
             WHERE id = ?1",
            params![id, status, now, os.unwrap_or(""), hostname.unwrap_or("")],
        )?;
        Ok(())
    }

    pub fn delete_remote_host(&self, id: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM remote_hosts WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    pub fn set_project_discord_channel(
        &self,
        id: &str,
        discord_channel: Option<&str>,
    ) -> Result<bool> {
        let normalized = discord_channel
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                if s.starts_with('#') {
                    s.to_string()
                } else {
                    format!("#{}", s)
                }
            });
        let changed = self.conn.execute(
            "UPDATE projects SET discord_channel = ?2 WHERE id = ?1",
            params![id, normalized],
        )?;
        Ok(changed > 0)
    }

    pub fn rename_project(&self, id: &str, name: &str) -> Result<bool> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Ok(false);
        }
        let changed = self.conn.execute(
            "UPDATE projects SET name = ?2 WHERE id = ?1",
            params![id, trimmed],
        )?;
        Ok(changed > 0)
    }

    pub fn reorder_projects(&self, ordered_ids: &[String]) -> Result<()> {
        for (idx, id) in ordered_ids.iter().enumerate() {
            self.conn.execute(
                "UPDATE projects SET sort_order = ?2 WHERE id = ?1",
                params![id, idx as i64],
            )?;
        }
        Ok(())
    }

    pub fn reorder_sessions(&self, ordered_ids: &[String]) -> Result<()> {
        for (idx, id) in ordered_ids.iter().enumerate() {
            self.conn.execute(
                "UPDATE sessions SET sort_order = ?2 WHERE id = ?1",
                params![id, idx as i64],
            )?;
        }
        Ok(())
    }

    /// Upsert a project received via Cross-Device Sync.
    /// Matches by id or case-insensitive name so an existing local project's `cwd` is preserved.
    pub fn upsert_synced_project(&self, project: &Project) -> Result<String> {
        if let Some(existing) = self.get_project(&project.id)? {
            return Ok(existing.id);
        }
        let by_name: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM projects WHERE LOWER(name) = LOWER(?1) LIMIT 1",
                params![project.name],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing_id) = by_name {
            return Ok(existing_id);
        }
        let now = Utc::now().to_rfc3339();
        let runtime = project.runtime.clone().unwrap_or_else(|| "local".to_string());
        self.conn.execute(
            "INSERT INTO projects (id, name, cwd, repo_url, discord_channel, runtime, wsl_distro, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                project.id,
                project.name,
                project.cwd,
                project.repo_url,
                project.discord_channel,
                runtime,
                project.wsl_distro,
                now
            ],
        )?;
        Ok(project.id.clone())
    }

    /// Idempotently merge a synced session and its chat history into SQLite.
    /// Returns the number of newly inserted messages.
    pub fn upsert_synced_session(&self, session: &Session, mapped_project_id: &str) -> Result<usize> {
        let now = Utc::now().to_rfc3339();
        let shared_deps_json = serde_json::to_string(&session.shared_deps)?;
        let is_discord_int: i64 = if session.is_discord.unwrap_or(false) { 1 } else { 0 };
        let unread_done_int: i64 = if session.unread_done.unwrap_or(false) { 1 } else { 0 };

        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM sessions WHERE id = ?1",
                params![session.id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);

        if !exists {
            self.conn.execute(
                "INSERT INTO sessions (
                    id, project_id, title, status, harness, model, effort, account,
                    permission_mode, worktree_path, branch, shared_deps_json,
                    additions, deletions, time_ago, is_discord, discord_channel,
                    unread_done, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                params![
                    session.id,
                    mapped_project_id,
                    session.title,
                    if session.status == "RUNNING" { "IDLE" } else { &session.status },
                    session.harness,
                    session.model,
                    session.effort,
                    session.account,
                    session.permission_mode,
                    ".",
                    session.branch,
                    shared_deps_json,
                    session.additions,
                    session.deletions,
                    session.time_ago,
                    is_discord_int,
                    session.discord_channel,
                    unread_done_int,
                    now,
                    now
                ],
            )?;
        }

        let mut inserted_msgs = 0usize;
        for msg in &session.messages {
            let msg_exists: bool = self
                .conn
                .query_row(
                    "SELECT 1 FROM messages WHERE id = ?1",
                    params![msg.id],
                    |_| Ok(true),
                )
                .optional()?
                .unwrap_or(false);
            if !msg_exists {
                self.append_message(&session.id, msg)?;
                inserted_msgs += 1;
            }
        }
        Ok(inserted_msgs)
    }

    pub fn delete_project(&self, id: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    pub fn list_sessions(&self, project_id: Option<&str>) -> Result<Vec<Session>> {
        let sql_all = "SELECT id, project_id, title, status, harness, model, effort, account,
                              permission_mode, worktree_path, branch, shared_deps_json,
                              additions, deletions, time_ago, is_discord, discord_channel,
                              unread_done
                       FROM sessions
                       WHERE office_id IS NULL
                       ORDER BY sort_order ASC, updated_at DESC";
        let sql_filtered = "SELECT id, project_id, title, status, harness, model, effort, account,
                                   permission_mode, worktree_path, branch, shared_deps_json,
                                   additions, deletions, time_ago, is_discord, discord_channel,
                                   unread_done
                            FROM sessions
                            WHERE project_id = ?1 AND office_id IS NULL
                            ORDER BY sort_order ASC, updated_at DESC";

        let mut sessions = Vec::new();
        if let Some(pid) = project_id {
            let mut stmt = self.conn.prepare(sql_filtered)?;
            let rows = stmt.query_map(params![pid], Self::map_session_row)?;
            for row in rows {
                sessions.push(row?);
            }
        } else {
            let mut stmt = self.conn.prepare(sql_all)?;
            let rows = stmt.query_map([], Self::map_session_row)?;
            for row in rows {
                sessions.push(row?);
            }
        }

        for s in &mut sessions {
            s.messages = self.list_messages(&s.id)?;
            s.pending_approval = self.get_pending_approval(&s.id)?;
        }

        Ok(sessions)
    }

    fn map_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Thread> {
        let shared_deps_json: String = row.get(11)?;
        let shared_deps: Vec<String> = serde_json::from_str(&shared_deps_json).unwrap_or_default();
        let is_discord_int: i64 = row.get(15)?;
        let unread_done_int: i64 = row.get(17)?;

        Ok(Thread {
            id: row.get(0)?,
            project_id: row.get(1)?,
            title: row.get(2)?,
            status: row.get(3)?,
            harness: row.get(4)?,
            model: row.get(5)?,
            effort: row.get(6)?,
            account: row.get(7)?,
            permission_mode: row.get(8)?,
            worktree_path: row.get(9)?,
            branch: row.get(10)?,
            shared_deps,
            additions: row.get(12)?,
            deletions: row.get(13)?,
            time_ago: row.get(14)?,
            is_discord: if is_discord_int != 0 { Some(true) } else { None },
            discord_channel: row.get(16)?,
            unread_done: if unread_done_int != 0 { Some(true) } else { None },
            messages: Vec::new(),
            pending_approval: None,
        })
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let mut session = self
            .conn
            .query_row(
                "SELECT id, project_id, title, status, harness, model, effort, account,
                        permission_mode, worktree_path, branch, shared_deps_json,
                        additions, deletions, time_ago, is_discord, discord_channel,
                        unread_done
                 FROM sessions
                 WHERE id = ?1",
                params![id],
                Self::map_session_row,
            )
            .optional()?;

        if let Some(ref mut s) = session {
            s.messages = self.list_messages(&s.id)?;
            s.pending_approval = self.get_pending_approval(&s.id)?;
        }
        Ok(session)
    }

    pub fn create_session(&self, input: CreateSessionInput) -> Result<Session> {
        self.insert_session(input, None)
    }

    pub fn create_session_for_office(
        &self,
        input: CreateSessionInput,
        office_id: &str,
    ) -> Result<Session> {
        self.insert_session(input, Some(office_id))
    }

    fn insert_session(&self, input: CreateSessionInput, office_id: Option<&str>) -> Result<Session> {
        let id = format!("thr-{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();
        let harness = input.harness.unwrap_or_else(|| "sakana".to_string());
        let model = input.model.unwrap_or_else(|| "claude-sonnet-4".to_string());
        let effort = input.effort.unwrap_or_else(|| "medium".to_string());
        let account = input.account.unwrap_or_else(|| "default".to_string());
        let permission_mode = input
            .permission_mode
            .unwrap_or_else(|| "supervised".to_string());
        let worktree_path = input.worktree_path.unwrap_or_else(|| ".".to_string());
        let branch = input.branch.unwrap_or_else(|| "main".to_string());
        let shared_deps = input.shared_deps.unwrap_or_default();
        let shared_deps_json = serde_json::to_string(&shared_deps)?;
        let is_discord = input.is_discord.unwrap_or(false);

        self.conn.execute(
            "INSERT INTO sessions (
                id, project_id, title, status, harness, model, effort, account,
                permission_mode, worktree_path, branch, shared_deps_json,
                additions, deletions, time_ago, is_discord, discord_channel,
                unread_done, created_at, updated_at, office_id
             ) VALUES (
                ?1, ?2, ?3, 'IDLE', ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11,
                0, 0, 'just now', ?12, ?13,
                0, ?14, ?14, ?15
             )",
            params![
                id,
                input.project_id,
                input.title,
                harness,
                model,
                effort,
                account,
                permission_mode,
                worktree_path,
                branch,
                shared_deps_json,
                if is_discord { 1 } else { 0 },
                input.discord_channel,
                now,
                office_id
            ],
        )?;

        Ok(Thread {
            id,
            project_id: input.project_id,
            title: input.title,
            status: "IDLE".to_string(),
            harness,
            model,
            effort,
            account,
            permission_mode,
            worktree_path,
            branch,
            shared_deps,
            additions: 0,
            deletions: 0,
            time_ago: "just now".to_string(),
            is_discord: if is_discord { Some(true) } else { None },
            discord_channel: input.discord_channel,
            unread_done: None,
            messages: Vec::new(),
            pending_approval: None,
        })
    }

    pub fn update_session(&self, input: UpdateSessionInput) -> Result<Option<Session>> {
        let Some(existing) = self.get_session(&input.id)? else {
            return Ok(None);
        };

        let project_id = input.project_id.unwrap_or(existing.project_id);
        let title = input.title.unwrap_or(existing.title);
        let status = input.status.unwrap_or(existing.status);
        let harness = input.harness.unwrap_or(existing.harness);
        let model = input.model.unwrap_or(existing.model);
        let effort = input.effort.unwrap_or(existing.effort);
        let account = input.account.unwrap_or(existing.account);
        let permission_mode = input.permission_mode.unwrap_or(existing.permission_mode);
        let worktree_path = input.worktree_path.unwrap_or(existing.worktree_path);
        let branch = input.branch.unwrap_or(existing.branch);
        let shared_deps = input.shared_deps.unwrap_or(existing.shared_deps);
        let shared_deps_json = serde_json::to_string(&shared_deps)?;
        let additions = input.additions.unwrap_or(existing.additions);
        let deletions = input.deletions.unwrap_or(existing.deletions);
        let unread_done = input
            .unread_done
            .unwrap_or_else(|| existing.unread_done.unwrap_or(false));
        let now = Utc::now().to_rfc3339();

        self.conn.execute(
            "UPDATE sessions SET
                title = ?2,
                status = ?3,
                harness = ?4,
                model = ?5,
                effort = ?6,
                account = ?7,
                permission_mode = ?8,
                worktree_path = ?9,
                branch = ?10,
                shared_deps_json = ?11,
                additions = ?12,
                deletions = ?13,
                unread_done = ?14,
                updated_at = ?15,
                project_id = ?16
             WHERE id = ?1",
            params![
                input.id,
                title,
                status,
                harness,
                model,
                effort,
                account,
                permission_mode,
                worktree_path,
                branch,
                shared_deps_json,
                additions,
                deletions,
                if unread_done { 1 } else { 0 },
                now,
                project_id
            ],
        )?;

        self.get_session(&input.id)
    }

    pub fn delete_session(&self, id: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    pub fn fork_session(
        &self,
        source_id: &str,
        title: &str,
        branch: &str,
        up_to_index: usize,
    ) -> Result<Session> {
        let source = self
            .get_session(source_id)?
            .ok_or_else(|| anyhow::anyhow!("Source session '{}' not found", source_id))?;

        let new_id = format!("thr-{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();
        let new_title = if title.trim().is_empty() {
            format!("{} (fork)", source.title)
        } else {
            title.trim().to_string()
        };
        let new_branch = if branch.trim().is_empty() {
            source.branch.clone()
        } else {
            branch.trim().to_string()
        };
        let shared_deps_json = serde_json::to_string(&source.shared_deps)?;
        let is_discord = source.is_discord.unwrap_or(false);

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO sessions (
                id, project_id, title, status, harness, model, effort, account,
                permission_mode, worktree_path, branch, shared_deps_json,
                additions, deletions, time_ago, is_discord, discord_channel,
                unread_done, created_at, updated_at
             ) VALUES (
                ?1, ?2, ?3, 'IDLE', ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11,
                0, 0, 'just now', ?12, ?13,
                0, ?14, ?14
             )",
            params![
                new_id,
                source.project_id,
                new_title,
                source.harness,
                source.model,
                source.effort,
                source.account,
                source.permission_mode,
                source.worktree_path,
                new_branch,
                shared_deps_json,
                if is_discord { 1 } else { 0 },
                source.discord_channel,
                now
            ],
        )?;

        for (idx, msg) in source
            .messages
            .iter()
            .take(up_to_index.saturating_add(1))
            .enumerate()
        {
            let cloned_id = format!("msg-{}", Uuid::new_v4());
            let tools_json = msg
                .tools
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;
            let subagent_json = msg
                .subagent
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;
            let widget_json = msg
                .widget
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;
            let checkpoint_json = msg
                .checkpoint
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            tx.execute(
                "INSERT INTO messages (
                    id, session_id, role, author, timestamp, text, thinking, thinking_duration,
                    tools_json, subagent_json, widget_json, checkpoint_json, sort_order
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    cloned_id,
                    new_id,
                    msg.role,
                    msg.author,
                    msg.timestamp,
                    msg.text,
                    msg.thinking,
                    msg.thinking_duration,
                    tools_json,
                    subagent_json,
                    widget_json,
                    checkpoint_json,
                    idx as i64
                ],
            )?;
        }

        tx.commit()?;

        self.get_session(&new_id)?
            .ok_or_else(|| anyhow::anyhow!("Failed to read back forked session"))
    }

    pub fn update_session_status(
        &self,
        id: &str,
        status: &str,
        unread_done: Option<bool>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        if let Some(unread) = unread_done {
            self.conn.execute(
                "UPDATE sessions SET status = ?2, unread_done = ?3, updated_at = ?4 WHERE id = ?1",
                params![id, status, if unread { 1 } else { 0 }, now],
            )?;
        } else {
            self.conn.execute(
                "UPDATE sessions SET status = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, status, now],
            )?;
        }
        Ok(())
    }

    pub fn list_messages(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, role, author, timestamp, text, thinking, thinking_duration,
                    tools_json, subagent_json, widget_json, checkpoint_json
             FROM messages
             WHERE session_id = ?1
             ORDER BY sort_order ASC, rowid ASC",
        )?;

        let rows = stmt.query_map(params![session_id], |row| {
            let tools_json: Option<String> = row.get(7)?;
            let subagent_json: Option<String> = row.get(8)?;
            let widget_json: Option<String> = row.get(9)?;
            let checkpoint_json: Option<String> = row.get(10)?;

            Ok(ChatMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                author: row.get(2)?,
                timestamp: row.get(3)?,
                text: row.get(4)?,
                thinking: row.get(5)?,
                thinking_duration: row.get(6)?,
                tools: tools_json.and_then(|s| serde_json::from_str(&s).ok()),
                subagent: subagent_json.and_then(|s| serde_json::from_str(&s).ok()),
                widget: widget_json.and_then(|s| serde_json::from_str(&s).ok()),
                checkpoint: checkpoint_json.and_then(|s| serde_json::from_str(&s).ok()),
            })
        })?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    }

    pub fn list_recent_messages(&self, session_id: &str, limit: usize) -> Result<Vec<ChatMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, role, author, timestamp, text, thinking, thinking_duration,
                    tools_json, subagent_json, widget_json, checkpoint_json
             FROM messages
             WHERE session_id = ?1
             ORDER BY sort_order DESC, rowid DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![session_id, limit as i64], |row| {
            let tools_json: Option<String> = row.get(7)?;
            let subagent_json: Option<String> = row.get(8)?;
            let widget_json: Option<String> = row.get(9)?;
            let checkpoint_json: Option<String> = row.get(10)?;

            Ok(ChatMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                author: row.get(2)?,
                timestamp: row.get(3)?,
                text: row.get(4)?,
                thinking: row.get(5)?,
                thinking_duration: row.get(6)?,
                tools: tools_json.and_then(|s| serde_json::from_str(&s).ok()),
                subagent: subagent_json.and_then(|s| serde_json::from_str(&s).ok()),
                widget: widget_json.and_then(|s| serde_json::from_str(&s).ok()),
                checkpoint: checkpoint_json.and_then(|s| serde_json::from_str(&s).ok()),
            })
        })?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        messages.reverse();
        Ok(messages)
    }

    pub fn record_usage(
        &self,
        session_id: Option<&str>,
        harness: &str,
        model: &str,
        input_tokens: i64,
        output_tokens: i64,
        cost_usd: f64,
    ) -> Result<()> {
        let id = format!("use-{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO usage (id, session_id, harness, model, input_tokens, output_tokens, cost_usd, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, session_id, harness, model, input_tokens, output_tokens, cost_usd, now],
        )?;
        Ok(())
    }

    pub fn summarize_harness_usage_windows(
        &self,
        harness: &str,
    ) -> Result<(i64, Option<String>, i64, Option<String>)> {
        let now = Utc::now();
        let cutoff_5h = (now - chrono::Duration::hours(5)).to_rfc3339();
        let cutoff_7d = (now - chrono::Duration::days(7)).to_rfc3339();

        let (tokens_5h, oldest_5h): (i64, Option<String>) = self.conn.query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0), MIN(created_at)
             FROM usage WHERE harness = ?1 AND created_at >= ?2",
            params![harness, cutoff_5h],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let (tokens_7d, oldest_7d): (i64, Option<String>) = self.conn.query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0), MIN(created_at)
             FROM usage WHERE harness = ?1 AND created_at >= ?2",
            params![harness, cutoff_7d],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let five_hour_reset = oldest_5h.and_then(|ts| {
            chrono::DateTime::parse_from_rfc3339(&ts)
                .ok()
                .map(|dt| {
                    let reset_at = dt.with_timezone(&Utc) + chrono::Duration::hours(5);
                    let rem_mins = (reset_at - now).num_minutes().max(0);
                    let h = rem_mins / 60;
                    let m = rem_mins % 60;
                    if h > 0 {
                        format!("{h}h {m}m")
                    } else {
                        format!("{m}m")
                    }
                })
        });

        let weekly_reset = oldest_7d.and_then(|ts| {
            chrono::DateTime::parse_from_rfc3339(&ts)
                .ok()
                .map(|dt| {
                    let reset_at = dt.with_timezone(&Utc) + chrono::Duration::days(7);
                    let rem_hours = (reset_at - now).num_hours().max(0);
                    let d = rem_hours / 24;
                    let h = rem_hours % 24;
                    if d > 0 {
                        format!("{d}d {h}h")
                    } else {
                        format!("{h}h")
                    }
                })
        });

        Ok((tokens_5h, five_hour_reset, tokens_7d, weekly_reset))
    }

    pub fn append_message(&self, session_id: &str, msg: &ChatMessage) -> Result<()> {
        let next_order: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM messages WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;

        let tools_json = msg
            .tools
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let subagent_json = msg
            .subagent
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let widget_json = msg
            .widget
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let checkpoint_json = msg
            .checkpoint
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        self.conn.execute(
            "INSERT INTO messages (
                id, session_id, role, author, timestamp, text, thinking, thinking_duration,
                tools_json, subagent_json, widget_json, checkpoint_json, sort_order
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                msg.id,
                session_id,
                msg.role,
                msg.author,
                msg.timestamp,
                msg.text,
                msg.thinking,
                msg.thinking_duration,
                tools_json,
                subagent_json,
                widget_json,
                checkpoint_json,
                next_order
            ],
        )?;

        self.conn.execute(
            "UPDATE sessions SET updated_at = ?2 WHERE id = ?1",
            params![session_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    fn get_pending_approval(&self, session_id: &str) -> Result<Option<PendingApproval>> {
        let approval = self
            .conn
            .query_row(
                "SELECT id, command, cwd, reason, risk, requested_by
                 FROM approvals
                 WHERE session_id = ?1 AND resolved = 0
                 ORDER BY created_at DESC
                 LIMIT 1",
                params![session_id],
                |row| {
                    Ok(PendingApproval {
                        id: row.get(0)?,
                        command: row.get(1)?,
                        cwd: row.get(2)?,
                        reason: row.get(3)?,
                        risk: row.get(4)?,
                        requested_by: row.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(approval)
    }

    pub fn insert_approval(&self, session_id: &str, approval: &PendingApproval) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO approvals (
                id, session_id, command, cwd, reason, risk, requested_by, resolved, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
            params![
                approval.id,
                session_id,
                approval.command,
                approval.cwd,
                approval.reason,
                approval.risk,
                approval.requested_by,
                now
            ],
        )?;
        Ok(())
    }

    pub fn resolve_approval(
        &self,
        approval_id: &str,
        approved: bool,
    ) -> Result<Option<(String, String)>> {
        let existing: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT session_id, command FROM approvals WHERE id = ?1",
                params![approval_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let Some((session_id, command)) = existing else {
            return Ok(None);
        };

        let resolved_val = if approved { 1 } else { 2 };
        self.conn.execute(
            "UPDATE approvals SET resolved = ?2 WHERE id = ?1",
            params![approval_id, resolved_val],
        )?;

        let remaining: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM approvals WHERE session_id = ?1 AND resolved = 0",
            params![session_id],
            |row| row.get(0),
        )?;

        if remaining == 0 {
            self.update_session_status(&session_id, "IDLE", None)?;
        }

        Ok(Some((session_id, command)))
    }

    pub fn list_approvals(&self, session_id: Option<&str>) -> Result<Vec<PendingApproval>> {
        let mut items = Vec::new();
        if let Some(sid) = session_id {
            let mut stmt = self.conn.prepare(
                "SELECT id, command, cwd, reason, risk, requested_by
                 FROM approvals
                 WHERE session_id = ?1 AND resolved = 0
                 ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map(params![sid], |row| {
                Ok(PendingApproval {
                    id: row.get(0)?,
                    command: row.get(1)?,
                    cwd: row.get(2)?,
                    reason: row.get(3)?,
                    risk: row.get(4)?,
                    requested_by: row.get(5)?,
                })
            })?;
            for row in rows {
                items.push(row?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, command, cwd, reason, risk, requested_by
                 FROM approvals
                 WHERE resolved = 0
                 ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(PendingApproval {
                    id: row.get(0)?,
                    command: row.get(1)?,
                    cwd: row.get(2)?,
                    reason: row.get(3)?,
                    risk: row.get(4)?,
                    requested_by: row.get(5)?,
                })
            })?;
            for row in rows {
                items.push(row?);
            }
        }
        Ok(items)
    }

    pub fn insert_trace_span(
        &self,
        session_id: &str,
        parent_id: Option<&str>,
        span: &TraceSpan,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO trace_spans (
                id, session_id, parent_id, actor, harness, action, duration_ms, tokens, status, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                span.id,
                session_id,
                parent_id,
                span.actor,
                span.harness,
                span.action,
                span.duration_ms,
                span.tokens,
                span.status,
                now
            ],
        )?;
        if let Some(children) = &span.children {
            for child in children {
                self.insert_trace_span(session_id, Some(&span.id), child)?;
            }
        }
        Ok(())
    }

    pub fn list_accounts(&self) -> Result<Vec<QuotaAccount>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, harness, label, email, five_hour_pct, five_hour_reset, weekly_pct, weekly_reset, is_default
             FROM accounts",
        )?;
        let rows = stmt.query_map([], |row| {
            let is_default_int: i64 = row.get(8)?;
            Ok(QuotaAccount {
                id: row.get(0)?,
                harness: row.get(1)?,
                label: row.get(2)?,
                email: row.get(3)?,
                five_hour_pct: row.get(4)?,
                five_hour_reset: row.get(5)?,
                weekly_pct: row.get(6)?,
                weekly_reset: row.get(7)?,
                is_default: if is_default_int != 0 { Some(true) } else { None },
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn list_mcp_servers(&self) -> Result<Vec<McpServer>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, transport, endpoint, enabled, status, latency_ms FROM mcp_servers",
        )?;
        let rows = stmt.query_map([], |row| {
            let enabled_int: i64 = row.get(4)?;
            Ok(McpServer {
                id: row.get(0)?,
                name: row.get(1)?,
                transport: row.get(2)?,
                endpoint: row.get(3)?,
                enabled: enabled_int != 0,
                status: row.get(5)?,
                latency_ms: row.get(6)?,
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn list_automations(&self) -> Result<Vec<AutomationItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, schedule, prompt, enabled, last_run_status FROM automations",
        )?;
        let rows = stmt.query_map([], |row| {
            let enabled_int: i64 = row.get(5)?;
            Ok(AutomationItem {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                schedule: row.get(3)?,
                prompt: row.get(4)?,
                enabled: enabled_int != 0,
                last_run_status: row.get(6)?,
                history: Vec::new(),
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            let mut item = row?;
            let mut run_stmt = self.conn.prepare(
                "SELECT ts, status, duration_ms, summary
                 FROM automation_runs
                 WHERE automation_id = ?1
                 ORDER BY rowid DESC
                 LIMIT 20",
            )?;
            let runs = run_stmt.query_map(params![item.id], |r| {
                Ok(AutomationHistoryItem {
                    ts: r.get(0)?,
                    status: r.get(1)?,
                    duration_ms: r.get(2)?,
                    summary: r.get(3)?,
                })
            })?;
            for run in runs {
                item.history.push(run?);
            }
            items.push(item);
        }
        Ok(items)
    }

    pub fn list_discord_rules(&self) -> Result<Vec<DiscordUserRule>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, user_id, label, role FROM discord_rbac")?;
        let rows = stmt.query_map([], |row| {
            Ok(DiscordUserRule {
                id: row.get(0)?,
                user_id: row.get(1)?,
                label: row.get(2)?,
                role: row.get(3)?,
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn get_bootstrap_state(&self) -> Result<BootstrapState> {
        Ok(BootstrapState {
            theme: self.get_theme()?,
            projects: self.list_projects()?,
            sessions: self.list_sessions(None)?,
            accounts: self.list_accounts()?,
            mcp_servers: self.list_mcp_servers()?,
            skills: Vec::new(),
            automations: self.list_automations()?,
            discord_rules: self.list_discord_rules()?,
            env_secrets: Vec::new(),
        })
    }

    pub fn insert_checkpoint(&self, session_id: &str, cp: &TurnCheckpoint) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT OR REPLACE INTO checkpoints (
                id, session_id, turn_number, commit_hash, label,
                files_changed, additions, deletions, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                cp.id,
                session_id,
                cp.turn_number,
                cp.commit_hash,
                cp.label,
                cp.files_changed,
                cp.additions,
                cp.deletions,
                now
            ],
        )?;
        Ok(())
    }

    pub fn get_checkpoint(
        &self,
        session_id: &str,
        checkpoint_id: &str,
    ) -> Result<Option<TurnCheckpoint>> {
        let cp = self
            .conn
            .query_row(
                "SELECT id, turn_number, commit_hash, label, files_changed, additions, deletions
                 FROM checkpoints
                 WHERE session_id = ?1 AND id = ?2",
                params![session_id, checkpoint_id],
                |row| {
                    Ok(TurnCheckpoint {
                        id: row.get(0)?,
                        turn_number: row.get(1)?,
                        commit_hash: row.get(2)?,
                        label: row.get(3)?,
                        files_changed: row.get(4)?,
                        additions: row.get(5)?,
                        deletions: row.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(cp)
    }

    pub fn list_checkpoints(&self, session_id: &str) -> Result<Vec<TurnCheckpoint>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, turn_number, commit_hash, label, files_changed, additions, deletions
             FROM checkpoints
             WHERE session_id = ?1
             ORDER BY turn_number ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(TurnCheckpoint {
                id: row.get(0)?,
                turn_number: row.get(1)?,
                commit_hash: row.get(2)?,
                label: row.get(3)?,
                files_changed: row.get(4)?,
                additions: row.get(5)?,
                deletions: row.get(6)?,
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn truncate_after_checkpoint(
        &self,
        session_id: &str,
        checkpoint_id: &str,
        turn_number: i64,
    ) -> Result<()> {
        self.conn.execute(
            "DELETE FROM checkpoints WHERE session_id = ?1 AND turn_number > ?2",
            params![session_id, turn_number],
        )?;

        let mut stmt = self.conn.prepare(
            "SELECT sort_order, checkpoint_json
             FROM messages
             WHERE session_id = ?1
             ORDER BY sort_order ASC, rowid ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })?;

        let mut cutoff_sort_order: Option<i64> = None;
        for row in rows {
            let (sort_order, cp_json) = row?;
            if let Some(json_str) = cp_json {
                if let Ok(cp) = serde_json::from_str::<TurnCheckpoint>(&json_str) {
                    if cp.id == checkpoint_id || cp.turn_number == turn_number {
                        cutoff_sort_order = Some(sort_order);
                        break;
                    }
                }
            }
        }

        if let Some(cutoff) = cutoff_sort_order {
            self.conn.execute(
                "DELETE FROM messages WHERE session_id = ?1 AND sort_order > ?2",
                params![session_id, cutoff],
            )?;
        }

        self.conn.execute(
            "UPDATE sessions SET updated_at = ?2 WHERE id = ?1",
            params![session_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_trace_spans(&self, session_id: &str) -> Result<Vec<TraceSpan>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent_id, actor, harness, action, duration_ms, tokens, status
             FROM trace_spans
             WHERE session_id = ?1
             ORDER BY rowid ASC",
        )?;

        struct FlatSpan {
            parent_id: Option<String>,
            span: TraceSpan,
        }

        let rows = stmt.query_map(params![session_id], |row| {
            Ok(FlatSpan {
                parent_id: row.get(1)?,
                span: TraceSpan {
                    id: row.get(0)?,
                    actor: row.get(2)?,
                    harness: row.get(3)?,
                    action: row.get(4)?,
                    duration_ms: row.get(5)?,
                    tokens: row.get(6)?,
                    status: row.get(7)?,
                    children: None,
                },
            })
        })?;

        let mut flat = Vec::new();
        for row in rows {
            flat.push(row?);
        }

        let mut children_map: std::collections::HashMap<String, Vec<TraceSpan>> =
            std::collections::HashMap::new();
        let mut roots: Vec<TraceSpan> = Vec::new();
        let all_ids: std::collections::HashSet<String> =
            flat.iter().map(|f| f.span.id.clone()).collect();

        for item in &flat {
            if let Some(ref pid) = item.parent_id {
                if all_ids.contains(pid) {
                    children_map
                        .entry(pid.clone())
                        .or_default()
                        .push(item.span.clone());
                    continue;
                }
            }
            roots.push(item.span.clone());
        }

        fn attach_children(
            node: &mut TraceSpan,
            map: &mut std::collections::HashMap<String, Vec<TraceSpan>>,
        ) {
            if let Some(mut kids) = map.remove(&node.id) {
                for kid in &mut kids {
                    attach_children(kid, map);
                }
                node.children = Some(kids);
            }
        }

        for root in &mut roots {
            attach_children(root, &mut children_map);
        }

        Ok(roots)
    }

    pub fn get_mcp_server(&self, id: &str) -> Result<Option<McpServer>> {
        let server = self
            .conn
            .query_row(
                "SELECT id, name, transport, endpoint, enabled, status, latency_ms
                 FROM mcp_servers
                 WHERE id = ?1",
                params![id],
                |row| {
                    let enabled_int: i64 = row.get(4)?;
                    Ok(McpServer {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        transport: row.get(2)?,
                        endpoint: row.get(3)?,
                        enabled: enabled_int != 0,
                        status: row.get(5)?,
                        latency_ms: row.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(server)
    }

    pub fn add_mcp_server(
        &self,
        name: &str,
        transport: &str,
        endpoint: &str,
    ) -> Result<McpServer> {
        let id = format!("mcp-{}", Uuid::new_v4());
        self.conn.execute(
            "INSERT INTO mcp_servers (id, name, transport, endpoint, enabled, status, latency_ms)
             VALUES (?1, ?2, ?3, ?4, 1, 'idle', NULL)",
            params![id, name, transport, endpoint],
        )?;
        Ok(McpServer {
            id,
            name: name.to_string(),
            transport: transport.to_string(),
            endpoint: endpoint.to_string(),
            enabled: true,
            status: "idle".to_string(),
            latency_ms: None,
        })
    }

    pub fn upsert_mcp_server_by_name(
        &self,
        name: &str,
        transport: &str,
        endpoint: &str,
    ) -> Result<McpServer> {
        let existing_id: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM mcp_servers WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing_id {
            self.conn.execute(
                "UPDATE mcp_servers SET transport = ?2, endpoint = ?3 WHERE id = ?1",
                params![id, transport, endpoint],
            )?;
            return Ok(self
                .get_mcp_server(&id)?
                .expect("mcp server must exist after update"));
        }
        self.add_mcp_server(name, transport, endpoint)
    }

    pub fn toggle_mcp_server(&self, id: &str, enabled: bool) -> Result<Option<McpServer>> {
        let status = if enabled { "connected" } else { "idle" };
        let changed = self.conn.execute(
            "UPDATE mcp_servers SET enabled = ?2, status = ?3 WHERE id = ?1",
            params![id, if enabled { 1 } else { 0 }, status],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_mcp_server(id)
    }

    pub fn upsert_account(&self, account: &QuotaAccount) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO accounts (
                id, harness, label, email, five_hour_pct, five_hour_reset,
                weekly_pct, weekly_reset, is_default
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                account.id,
                account.harness,
                account.label,
                account.email,
                account.five_hour_pct,
                account.five_hour_reset,
                account.weekly_pct,
                account.weekly_reset,
                if account.is_default.unwrap_or(false) { 1 } else { 0 },
            ],
        )?;
        Ok(())
    }

    pub fn delete_account(&self, id: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    pub fn list_harness_accounts(&self, harness: Option<&str>) -> Result<Vec<HarnessAccountRow>> {
        let mut items = Vec::new();
        if let Some(h) = harness {
            let mut stmt = self.conn.prepare(
                "SELECT id, harness, label, email, profile_dir, auth_type, is_default, status
                 FROM harness_accounts WHERE harness = ?1
                 ORDER BY is_default DESC, created_at ASC",
            )?;
            let rows = stmt.query_map(params![h], |row| {
                let def_int: i64 = row.get(6)?;
                Ok(HarnessAccountRow {
                    id: row.get(0)?,
                    harness: row.get(1)?,
                    label: row.get(2)?,
                    email: row.get(3)?,
                    profile_dir: row.get(4)?,
                    auth_type: row.get(5)?,
                    is_default: def_int != 0,
                    status: row.get(7)?,
                })
            })?;
            for r in rows {
                items.push(r?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, harness, label, email, profile_dir, auth_type, is_default, status
                 FROM harness_accounts
                 ORDER BY harness ASC, is_default DESC, created_at ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                let def_int: i64 = row.get(6)?;
                Ok(HarnessAccountRow {
                    id: row.get(0)?,
                    harness: row.get(1)?,
                    label: row.get(2)?,
                    email: row.get(3)?,
                    profile_dir: row.get(4)?,
                    auth_type: row.get(5)?,
                    is_default: def_int != 0,
                    status: row.get(7)?,
                })
            })?;
            for r in rows {
                items.push(r?);
            }
        }
        Ok(items)
    }

    pub fn get_harness_account(&self, id: &str) -> Result<Option<HarnessAccountRow>> {
        self.conn
            .query_row(
                "SELECT id, harness, label, email, profile_dir, auth_type, is_default, status
                 FROM harness_accounts WHERE id = ?1",
                params![id],
                |row| {
                    let def_int: i64 = row.get(6)?;
                    Ok(HarnessAccountRow {
                        id: row.get(0)?,
                        harness: row.get(1)?,
                        label: row.get(2)?,
                        email: row.get(3)?,
                        profile_dir: row.get(4)?,
                        auth_type: row.get(5)?,
                        is_default: def_int != 0,
                        status: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_default_harness_account(&self, harness: &str) -> Result<Option<HarnessAccountRow>> {
        self.conn
            .query_row(
                "SELECT id, harness, label, email, profile_dir, auth_type, is_default, status
                 FROM harness_accounts WHERE harness = ?1
                 ORDER BY is_default DESC, created_at ASC LIMIT 1",
                params![harness],
                |row| {
                    let def_int: i64 = row.get(6)?;
                    Ok(HarnessAccountRow {
                        id: row.get(0)?,
                        harness: row.get(1)?,
                        label: row.get(2)?,
                        email: row.get(3)?,
                        profile_dir: row.get(4)?,
                        auth_type: row.get(5)?,
                        is_default: def_int != 0,
                        status: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn create_harness_account(&self, row: &HarnessAccountRow) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO harness_accounts
             (id, harness, label, email, profile_dir, auth_type, is_default, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                row.id,
                row.harness,
                row.label,
                row.email,
                row.profile_dir,
                row.auth_type,
                if row.is_default { 1 } else { 0 },
                row.status,
                now,
            ],
        )?;
        Ok(())
    }

    pub fn update_harness_account(
        &self,
        id: &str,
        email: Option<&str>,
        status: Option<&str>,
        auth_type: Option<&str>,
    ) -> Result<()> {
        if let Some(e) = email {
            self.conn.execute(
                "UPDATE harness_accounts SET email = ?2 WHERE id = ?1",
                params![id, e],
            )?;
        }
        if let Some(s) = status {
            self.conn.execute(
                "UPDATE harness_accounts SET status = ?2 WHERE id = ?1",
                params![id, s],
            )?;
        }
        if let Some(a) = auth_type {
            self.conn.execute(
                "UPDATE harness_accounts SET auth_type = ?2 WHERE id = ?1",
                params![id, a],
            )?;
        }
        Ok(())
    }

    pub fn rename_harness_account(&self, id: &str, label: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE harness_accounts SET label = ?2 WHERE id = ?1",
            params![id, label],
        )?;
        Ok(changed > 0)
    }

    pub fn set_default_harness_account(&self, harness: &str, id: &str) -> Result<bool> {
        self.conn.execute(
            "UPDATE harness_accounts SET is_default = 0 WHERE harness = ?1",
            params![harness],
        )?;
        let changed = self.conn.execute(
            "UPDATE harness_accounts SET is_default = 1 WHERE id = ?1 AND harness = ?2",
            params![id, harness],
        )?;
        Ok(changed > 0)
    }

    pub fn delete_harness_account(&self, id: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM harness_accounts WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    pub fn update_mcp_server_ping(
        &self,
        id: &str,
        status: &str,
        latency_ms: Option<i64>,
    ) -> Result<Option<McpServer>> {
        let changed = self.conn.execute(
            "UPDATE mcp_servers SET status = ?2, latency_ms = ?3 WHERE id = ?1",
            params![id, status, latency_ms],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_mcp_server(id)
    }

    pub fn delete_mcp_server(&self, id: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM mcp_servers WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    pub fn get_automation(&self, id: &str) -> Result<Option<AutomationItem>> {
        let mut item = self
            .conn
            .query_row(
                "SELECT id, name, kind, schedule, prompt, enabled, last_run_status
                 FROM automations
                 WHERE id = ?1",
                params![id],
                |row| {
                    let enabled_int: i64 = row.get(5)?;
                    Ok(AutomationItem {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        kind: row.get(2)?,
                        schedule: row.get(3)?,
                        prompt: row.get(4)?,
                        enabled: enabled_int != 0,
                        last_run_status: row.get(6)?,
                        history: Vec::new(),
                    })
                },
            )
            .optional()?;

        if let Some(ref mut auto) = item {
            let mut run_stmt = self.conn.prepare(
                "SELECT ts, status, duration_ms, summary
                 FROM automation_runs
                 WHERE automation_id = ?1
                 ORDER BY rowid DESC
                 LIMIT 20",
            )?;
            let runs = run_stmt.query_map(params![auto.id], |r| {
                Ok(AutomationHistoryItem {
                    ts: r.get(0)?,
                    status: r.get(1)?,
                    duration_ms: r.get(2)?,
                    summary: r.get(3)?,
                })
            })?;
            for run in runs {
                auto.history.push(run?);
            }
        }
        Ok(item)
    }

    pub fn create_automation(
        &self,
        name: &str,
        kind: &str,
        schedule: &str,
        prompt: &str,
    ) -> Result<AutomationItem> {
        let id = format!("auto-{}", Uuid::new_v4());
        self.conn.execute(
            "INSERT INTO automations (id, name, kind, schedule, prompt, enabled, last_run_status)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, 'OK')",
            params![id, name, kind, schedule, prompt],
        )?;
        Ok(AutomationItem {
            id,
            name: name.to_string(),
            kind: kind.to_string(),
            schedule: schedule.to_string(),
            prompt: prompt.to_string(),
            enabled: true,
            last_run_status: "OK".to_string(),
            history: Vec::new(),
        })
    }

    pub fn toggle_automation(&self, id: &str, enabled: bool) -> Result<Option<AutomationItem>> {
        let changed = self.conn.execute(
            "UPDATE automations SET enabled = ?2 WHERE id = ?1",
            params![id, if enabled { 1 } else { 0 }],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_automation(id)
    }

    pub fn delete_automation(&self, id: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM automations WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    pub fn record_automation_run(
        &self,
        automation_id: &str,
        status: &str,
        duration_ms: i64,
        summary: &str,
    ) -> Result<Option<AutomationItem>> {
        let run_id = format!("run-{}", Uuid::new_v4());
        let ts = Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
        self.conn.execute(
            "INSERT INTO automation_runs (id, automation_id, ts, status, duration_ms, summary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![run_id, automation_id, ts, status, duration_ms, summary],
        )?;
        self.conn.execute(
            "UPDATE automations SET last_run_status = ?2 WHERE id = ?1",
            params![automation_id, status],
        )?;
        self.get_automation(automation_id)
    }

    pub fn upsert_discord_rule(
        &self,
        user_id: &str,
        label: &str,
        role: &str,
    ) -> Result<DiscordUserRule> {
        let existing_id: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM discord_rbac WHERE user_id = ?1",
                params![user_id],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(id) = existing_id {
            self.conn.execute(
                "UPDATE discord_rbac SET label = ?2, role = ?3 WHERE id = ?1",
                params![id, label, role],
            )?;
            return Ok(DiscordUserRule {
                id,
                user_id: user_id.to_string(),
                label: label.to_string(),
                role: role.to_string(),
            });
        }

        let id = format!("disc-{}", Uuid::new_v4());
        self.conn.execute(
            "INSERT INTO discord_rbac (id, user_id, label, role) VALUES (?1, ?2, ?3, ?4)",
            params![id, user_id, label, role],
        )?;
        Ok(DiscordUserRule {
            id,
            user_id: user_id.to_string(),
            label: label.to_string(),
            role: role.to_string(),
        })
    }

    pub fn delete_discord_rule(&self, id: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM discord_rbac WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    pub fn get_discord_role(&self, user_id: &str) -> Result<Option<String>> {
        let role = self
            .conn
            .query_row(
                "SELECT role FROM discord_rbac WHERE user_id = ?1",
                params![user_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(role)
    }

    // =========================================================================
    // Office CRUD & helpers
    // =========================================================================

    pub fn list_messages_for_session(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        self.list_messages(session_id)
    }

    fn map_office_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Office> {
        Ok(Office {
            id: row.get(0)?,
            name: row.get(1)?,
            project_id: row.get(2)?,
            cwd: row.get(3)?,
            master_staff_id: row.get(4)?,
            max_rounds: row.get(5)?,
            status: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
            staff: Vec::new(),
            events: Vec::new(),
        })
    }

    fn map_office_staff_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OfficeStaff> {
        let is_master_int: i64 = row.get(5)?;
        let is_dynamic_int: i64 = row.get(6)?;
        let isolated_worktree_int: i64 = row.get(13)?;
        Ok(OfficeStaff {
            id: row.get(0)?,
            office_id: row.get(1)?,
            session_id: row.get(2)?,
            name: row.get(3)?,
            role: row.get(4)?,
            is_master: is_master_int != 0,
            is_dynamic: is_dynamic_int != 0,
            harness: row.get(7)?,
            model: row.get(8)?,
            effort: row.get(9)?,
            account: row.get(10)?,
            permission_mode: row.get(11)?,
            system_prompt: row.get(12)?,
            isolated_worktree: isolated_worktree_int != 0,
            worktree_path: row.get(14)?,
            branch: row.get(15)?,
            status: row.get(16)?,
            current_task: row.get(17)?,
            avatar_color: row.get(18)?,
            desk_order: row.get(19)?,
            created_at: row.get(20)?,
        })
    }

    fn map_office_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OfficeEvent> {
        Ok(OfficeEvent {
            id: row.get(0)?,
            office_id: row.get(1)?,
            kind: row.get(2)?,
            from_staff_id: row.get(3)?,
            from_name: row.get(4)?,
            to_staff_id: row.get(5)?,
            to_name: row.get(6)?,
            summary: row.get(7)?,
            detail: row.get(8)?,
            timestamp: row.get(9)?,
        })
    }

    fn list_office_staff_for_office(&self, office_id: &str) -> Result<Vec<OfficeStaff>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, office_id, session_id, name, role, is_master, is_dynamic,
                    harness, model, effort, account, permission_mode, system_prompt,
                    isolated_worktree, worktree_path, branch, status, current_task,
                    avatar_color, desk_order, created_at
             FROM office_staff
             WHERE office_id = ?1
             ORDER BY desk_order ASC, created_at ASC",
        )?;
        let rows = stmt.query_map(params![office_id], Self::map_office_staff_row)?;
        let mut staff = Vec::new();
        for row in rows {
            staff.push(row?);
        }
        Ok(staff)
    }

    pub fn list_offices(&self) -> Result<Vec<Office>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, project_id, cwd, master_staff_id, max_rounds, status, created_at, updated_at
             FROM offices
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], Self::map_office_row)?;
        let mut offices = Vec::new();
        for row in rows {
            offices.push(row?);
        }
        for office in &mut offices {
            office.staff = self.list_office_staff_for_office(&office.id)?;
            office.events = self.list_office_events(&office.id)?;
        }
        Ok(offices)
    }

    pub fn get_office(&self, id: &str) -> Result<Option<Office>> {
        let mut office = self
            .conn
            .query_row(
                "SELECT id, name, project_id, cwd, master_staff_id, max_rounds, status, created_at, updated_at
                 FROM offices
                 WHERE id = ?1",
                params![id],
                Self::map_office_row,
            )
            .optional()?;
        if let Some(ref mut o) = office {
            o.staff = self.list_office_staff_for_office(&o.id)?;
            o.events = self.list_office_events(&o.id)?;
        }
        Ok(office)
    }

    pub fn create_office_row(&self, office: &Office) -> Result<()> {
        self.conn.execute(
            "INSERT INTO offices (
                id, name, project_id, cwd, master_staff_id, max_rounds, status, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                office.id,
                office.name,
                office.project_id,
                office.cwd,
                office.master_staff_id,
                office.max_rounds,
                office.status,
                office.created_at,
                office.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_office(&self, id: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM offices WHERE id = ?1", params![id])?;
        if changed > 0 {
            self.conn
                .execute("DELETE FROM sessions WHERE office_id = ?1", params![id])?;
        }
        Ok(changed > 0)
    }

    pub fn update_office_status(&self, id: &str, status: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE offices SET status = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, status, now],
        )?;
        Ok(())
    }

    pub fn update_office_max_rounds(&self, id: &str, max_rounds: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE offices SET max_rounds = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, max_rounds, now],
        )?;
        Ok(())
    }

    pub fn insert_office_staff(&self, staff: &OfficeStaff) -> Result<()> {
        self.conn.execute(
            "INSERT INTO office_staff (
                id, office_id, session_id, name, role, is_master, is_dynamic,
                harness, model, effort, account, permission_mode, system_prompt,
                isolated_worktree, worktree_path, branch, status, current_task,
                avatar_color, desk_order, created_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18,
                ?19, ?20, ?21
             )",
            params![
                staff.id,
                staff.office_id,
                staff.session_id,
                staff.name,
                staff.role,
                if staff.is_master { 1 } else { 0 },
                if staff.is_dynamic { 1 } else { 0 },
                staff.harness,
                staff.model,
                staff.effort,
                staff.account,
                staff.permission_mode,
                staff.system_prompt,
                if staff.isolated_worktree { 1 } else { 0 },
                staff.worktree_path,
                staff.branch,
                staff.status,
                staff.current_task,
                staff.avatar_color,
                staff.desk_order,
                staff.created_at,
            ],
        )?;
        self.conn.execute(
            "UPDATE offices SET updated_at = ?2 WHERE id = ?1",
            params![staff.office_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn get_office_staff(&self, id: &str) -> Result<Option<OfficeStaff>> {
        self.conn
            .query_row(
                "SELECT id, office_id, session_id, name, role, is_master, is_dynamic,
                        harness, model, effort, account, permission_mode, system_prompt,
                        isolated_worktree, worktree_path, branch, status, current_task,
                        avatar_color, desk_order, created_at
                 FROM office_staff
                 WHERE id = ?1",
                params![id],
                Self::map_office_staff_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn update_office_staff(&self, input: UpdateStaffInput) -> Result<Option<OfficeStaff>> {
        let Some(existing) = self.get_office_staff(&input.id)? else {
            return Ok(None);
        };
        let name = input.name.unwrap_or(existing.name);
        let role = input.role.unwrap_or(existing.role);
        let harness = input.harness.unwrap_or(existing.harness);
        let model = input.model.unwrap_or(existing.model);
        let effort = input.effort.unwrap_or(existing.effort);
        let account = input.account.unwrap_or(existing.account);
        let permission_mode = input.permission_mode.unwrap_or(existing.permission_mode);
        let system_prompt = input.system_prompt.unwrap_or(existing.system_prompt);
        let isolated_worktree = input.isolated_worktree.unwrap_or(existing.isolated_worktree);
        let worktree_path = input.worktree_path.unwrap_or(existing.worktree_path);
        let branch = input.branch.unwrap_or(existing.branch);

        self.conn.execute(
            "UPDATE office_staff SET
                name = ?2,
                role = ?3,
                harness = ?4,
                model = ?5,
                effort = ?6,
                account = ?7,
                permission_mode = ?8,
                system_prompt = ?9,
                isolated_worktree = ?10,
                worktree_path = ?11,
                branch = ?12
             WHERE id = ?1",
            params![
                input.id,
                name,
                role,
                harness,
                model,
                effort,
                account,
                permission_mode,
                system_prompt,
                if isolated_worktree { 1 } else { 0 },
                worktree_path,
                branch,
            ],
        )?;
        self.conn.execute(
            "UPDATE offices SET updated_at = ?2 WHERE id = ?1",
            params![existing.office_id, Utc::now().to_rfc3339()],
        )?;
        self.get_office_staff(&input.id)
    }

    pub fn delete_office_staff(&self, id: &str) -> Result<bool> {
        let session_id: Option<String> = self
            .conn
            .query_row(
                "SELECT session_id FROM office_staff WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let changed = self
            .conn
            .execute("DELETE FROM office_staff WHERE id = ?1", params![id])?;
        if changed > 0 {
            if let Some(sid) = session_id {
                self.conn
                    .execute("DELETE FROM sessions WHERE id = ?1", params![sid])?;
            }
        }
        Ok(changed > 0)
    }

    pub fn update_staff_runtime_state(
        &self,
        id: &str,
        status: &str,
        current_task: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE office_staff SET status = ?2, current_task = ?3 WHERE id = ?1",
            params![id, status, current_task],
        )?;
        Ok(())
    }

    pub fn append_office_event(&self, event: &OfficeEvent) -> Result<()> {
        self.conn.execute(
            "INSERT INTO office_events (
                id, office_id, kind, from_staff_id, from_name, to_staff_id, to_name,
                summary, detail, timestamp
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                event.id,
                event.office_id,
                event.kind,
                event.from_staff_id,
                event.from_name,
                event.to_staff_id,
                event.to_name,
                event.summary,
                event.detail,
                event.timestamp,
            ],
        )?;
        self.conn.execute(
            "UPDATE offices SET updated_at = ?2 WHERE id = ?1",
            params![event.office_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_office_events(&self, office_id: &str) -> Result<Vec<OfficeEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, office_id, kind, from_staff_id, from_name, to_staff_id, to_name,
                    summary, detail, timestamp
             FROM office_events
             WHERE office_id = ?1
             ORDER BY rowid ASC",
        )?;
        let rows = stmt.query_map(params![office_id], Self::map_office_event_row)?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_temp_db() -> (tempfile_dir::TempDir, Database) {
        let dir = tempfile_dir::TempDir::new("sakana-db-test");
        let db = Database::open_in_dir(&dir.path).expect("open_in_dir");
        (dir, db)
    }

    mod tempfile_dir {
        use std::path::PathBuf;
        use uuid::Uuid;

        pub struct TempDir {
            pub path: PathBuf,
        }

        impl TempDir {
            pub fn new(prefix: &str) -> Self {
                let path = std::env::temp_dir().join(format!("{}-{}", prefix, Uuid::new_v4()));
                std::fs::create_dir_all(&path).expect("create temp dir");
                Self { path }
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }

    fn make_msg(id: &str, role: &str, text: &str) -> ChatMessage {
        ChatMessage {
            id: id.to_string(),
            role: role.to_string(),
            author: None,
            timestamp: "12:00".to_string(),
            text: text.to_string(),
            thinking: None,
            thinking_duration: None,
            tools: None,
            subagent: None,
            widget: None,
            checkpoint: None,
        }
    }

    #[test]
    fn schema_version_is_tracked_via_user_version() {
        let (_dir, db) = open_temp_db();
        assert_eq!(db.schema_version().expect("schema_version"), 1);
    }

    #[test]
    fn fork_session_copies_messages_up_to_index() {
        let (_dir, db) = open_temp_db();
        let proj = db
            .create_project(CreateProjectInput {
                name: "Test".to_string(),
                cwd: ".".to_string(),
                repo_url: None,
                discord_channel: None,
                runtime: None,
                wsl_distro: None,
            })
            .expect("create_project");

        let sess = db
            .create_session(CreateSessionInput {
                project_id: proj.id.clone(),
                title: "Original".to_string(),
                harness: Some("claude-code".to_string()),
                model: Some("claude-sonnet-4-6".to_string()),
                effort: Some("high".to_string()),
                account: None,
                permission_mode: Some("auto-edit".to_string()),
                worktree_path: None,
                branch: Some("main".to_string()),
                shared_deps: None,
                is_discord: None,
                discord_channel: None,
            })
            .expect("create_session");

        db.append_message(&sess.id, &make_msg("m0", "user", "first"))
            .unwrap();
        db.append_message(&sess.id, &make_msg("m1", "assistant", "second"))
            .unwrap();
        db.append_message(&sess.id, &make_msg("m2", "user", "third"))
            .unwrap();

        let forked = db
            .fork_session(&sess.id, "Forked Title", "feat/fork", 1)
            .expect("fork_session");

        assert_ne!(forked.id, sess.id);
        assert_eq!(forked.title, "Forked Title");
        assert_eq!(forked.branch, "feat/fork");
        assert_eq!(forked.harness, "claude-code");
        assert_eq!(forked.messages.len(), 2);
        assert_eq!(forked.messages[0].text, "first");
        assert_eq!(forked.messages[1].text, "second");
        assert_ne!(forked.messages[0].id, "m0");
        assert_ne!(forked.messages[1].id, "m1");
    }

    #[test]
    fn list_recent_messages_returns_last_n_in_chronological_order() {
        let (_dir, db) = open_temp_db();
        let proj = db
            .create_project(CreateProjectInput {
                name: "Test".to_string(),
                cwd: ".".to_string(),
                repo_url: None,
                discord_channel: None,
                runtime: None,
                wsl_distro: None,
            })
            .unwrap();
        let sess = db
            .create_session(CreateSessionInput {
                project_id: proj.id,
                title: "Recent".to_string(),
                harness: None,
                model: None,
                effort: None,
                account: None,
                permission_mode: None,
                worktree_path: None,
                branch: None,
                shared_deps: None,
                is_discord: None,
                discord_channel: None,
            })
            .unwrap();

        for i in 0..5 {
            db.append_message(&sess.id, &make_msg(&format!("m{i}"), "user", &format!("msg {i}")))
                .unwrap();
        }

        let recent = db.list_recent_messages(&sess.id, 3).unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].text, "msg 2");
        assert_eq!(recent[1].text, "msg 3");
        assert_eq!(recent[2].text, "msg 4");
    }

    #[test]
    fn record_usage_inserts_row() {
        let (_dir, db) = open_temp_db();
        db.record_usage(Some("thr-1"), "agy-cli", "gemini-4-neon", 120, 45, 0.002)
            .expect("record_usage");
        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM usage WHERE session_id = 'thr-1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn office_crud_and_session_isolation() {
        let (_dir, db) = open_temp_db();
        let proj = db
            .create_project(CreateProjectInput {
                name: "OfficeProj".to_string(),
                cwd: ".".to_string(),
                repo_url: None,
                discord_channel: None,
                runtime: None,
                wsl_distro: None,
            })
            .unwrap();

        let regular_sess = db
            .create_session(CreateSessionInput {
                project_id: proj.id.clone(),
                title: "Regular".to_string(),
                harness: None,
                model: None,
                effort: None,
                account: None,
                permission_mode: None,
                worktree_path: None,
                branch: None,
                shared_deps: None,
                is_discord: None,
                discord_channel: None,
            })
            .unwrap();

        let now = Utc::now().to_rfc3339();
        let office_id = "off-1".to_string();
        let office = Office {
            id: office_id.clone(),
            name: "Main Office".to_string(),
            project_id: proj.id.clone(),
            cwd: ".".to_string(),
            master_staff_id: "stf-master".to_string(),
            max_rounds: 6,
            status: "idle".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            staff: Vec::new(),
            events: Vec::new(),
        };
        db.create_office_row(&office).unwrap();

        let office_sess = db
            .create_session_for_office(
                CreateSessionInput {
                    project_id: proj.id.clone(),
                    title: "Master Session".to_string(),
                    harness: Some("claude-code".to_string()),
                    model: Some("claude-sonnet-4".to_string()),
                    effort: None,
                    account: None,
                    permission_mode: None,
                    worktree_path: None,
                    branch: None,
                    shared_deps: None,
                    is_discord: None,
                    discord_channel: None,
                },
                &office_id,
            )
            .unwrap();

        // Regular list_sessions must exclude office sessions
        let all_sessions = db.list_sessions(None).unwrap();
        assert_eq!(all_sessions.len(), 1);
        assert_eq!(all_sessions[0].id, regular_sess.id);
        let proj_sessions = db.list_sessions(Some(&proj.id)).unwrap();
        assert_eq!(proj_sessions.len(), 1);
        assert_eq!(proj_sessions[0].id, regular_sess.id);

        // Office session still accessible by id and messages
        db.append_message(&office_sess.id, &make_msg("m-off-1", "user", "hello office"))
            .unwrap();
        let msgs = db.list_messages_for_session(&office_sess.id).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].text, "hello office");

        let staff = OfficeStaff {
            id: "stf-master".to_string(),
            office_id: office_id.clone(),
            session_id: office_sess.id.clone(),
            name: "Director".to_string(),
            role: "Coordinator".to_string(),
            is_master: true,
            is_dynamic: false,
            harness: "claude-code".to_string(),
            model: "claude-sonnet-4".to_string(),
            effort: "medium".to_string(),
            account: "default".to_string(),
            permission_mode: "supervised".to_string(),
            system_prompt: "Lead the team".to_string(),
            isolated_worktree: false,
            worktree_path: ".".to_string(),
            branch: "main".to_string(),
            status: "idle".to_string(),
            current_task: None,
            avatar_color: "#6366f1".to_string(),
            desk_order: 0,
            created_at: now.clone(),
        };
        db.insert_office_staff(&staff).unwrap();

        db.update_office_status(&office_id, "running").unwrap();
        db.update_office_max_rounds(&office_id, 10).unwrap();
        db.update_staff_runtime_state("stf-master", "working", Some("Planning"))
            .unwrap();

        let updated_staff = db
            .update_office_staff(UpdateStaffInput {
                id: "stf-master".to_string(),
                name: Some("Chief".to_string()),
                role: None,
                harness: None,
                model: None,
                effort: Some("high".to_string()),
                account: None,
                permission_mode: None,
                system_prompt: None,
                isolated_worktree: Some(true),
                worktree_path: None,
                branch: None,
            })
            .unwrap()
            .unwrap();
        assert_eq!(updated_staff.name, "Chief");
        assert_eq!(updated_staff.effort, "high");
        assert!(updated_staff.isolated_worktree);
        assert_eq!(updated_staff.status, "working");
        assert_eq!(updated_staff.current_task.as_deref(), Some("Planning"));

        let evt = OfficeEvent {
            id: "evt-1".to_string(),
            office_id: office_id.clone(),
            kind: "assign".to_string(),
            from_staff_id: Some("stf-master".to_string()),
            from_name: Some("Chief".to_string()),
            to_staff_id: None,
            to_name: None,
            summary: "Started".to_string(),
            detail: Some("Details".to_string()),
            timestamp: now,
        };
        db.append_office_event(&evt).unwrap();

        let fetched = db.get_office(&office_id).unwrap().unwrap();
        assert_eq!(fetched.status, "running");
        assert_eq!(fetched.max_rounds, 10);
        assert_eq!(fetched.staff.len(), 1);
        assert_eq!(fetched.events.len(), 1);
        assert_eq!(db.list_offices().unwrap().len(), 1);
        assert_eq!(db.list_office_events(&office_id).unwrap().len(), 1);

        assert!(db.delete_office_staff("stf-master").unwrap());
        assert!(db.get_office_staff("stf-master").unwrap().is_none());
        assert!(db.delete_office(&office_id).unwrap());
        assert!(db.get_office(&office_id).unwrap().is_none());
    }
}
