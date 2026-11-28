use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord_channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wsl_distro: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub target: String,
    pub status: String,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnCheckpoint {
    pub id: String,
    pub turn_number: i64,
    pub commit_hash: String,
    pub label: String,
    pub files_changed: i64,
    pub additions: i64,
    pub deletions: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetMetric {
    pub label: String,
    pub value: String,
    pub delta: String,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetBar {
    pub label: String,
    pub pct: f64,
    pub ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineWidgetData {
    pub title: String,
    pub subtitle: String,
    pub source_code: String,
    pub metrics: Vec<WidgetMetric>,
    pub bars: Vec<WidgetBar>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentInfo {
    pub harness: String,
    pub command: String,
    pub summary: String,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub timestamp: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_duration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent: Option<SubagentInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widget: Option<InlineWidgetData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<TurnCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failover_notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingApproval {
    pub id: String,
    pub command: String,
    pub cwd: String,
    pub reason: String,
    pub risk: String,
    pub requested_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub status: String,
    pub harness: String,
    pub model: String,
    pub effort: String,
    pub account: String,
    pub permission_mode: String,
    pub worktree_path: String,
    pub branch: String,
    pub shared_deps: Vec<String>,
    pub additions: i64,
    pub deletions: i64,
    pub time_ago: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_discord: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord_channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unread_done: Option<bool>,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_approval: Option<PendingApproval>,
}

pub type Session = Thread;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceSpan {
    pub id: String,
    pub actor: String,
    pub harness: String,
    pub action: String,
    pub duration_ms: i64,
    pub tokens: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TraceSpan>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaAccount {
    pub id: String,
    pub harness: String,
    pub label: String,
    pub email: String,
    pub five_hour_pct: f64,
    pub five_hour_reset: String,
    pub weekly_pct: f64,
    pub weekly_reset: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessAccountRow {
    pub id: String,
    pub harness: String,
    pub label: String,
    pub email: String,
    pub profile_dir: String,
    pub auth_type: String,
    pub is_default: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessAuthAccount {
    pub id: String,
    pub harness: String,
    pub label: String,
    pub email: String,
    pub profile_dir: String,
    pub auth_type: String,
    pub is_default: bool,
    pub authenticated: bool,
    pub status: String,
    pub cli_installed: bool,
    pub cli_bin: String,
    pub last_login_output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessLoginResponse {
    pub started: bool,
    pub account_id: String,
    pub harness: String,
    pub auth_type: String,
    pub mode: String,
    pub authenticated: bool,
    pub needs_code: bool,
    pub auth_url: String,
    pub device_code: String,
    pub email: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub endpoint: String,
    pub enabled: bool,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillItem {
    pub slug: String,
    pub scope: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationHistoryItem {
    pub ts: String,
    pub status: String,
    pub duration_ms: i64,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationItem {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub schedule: String,
    pub prompt: String,
    pub enabled: bool,
    pub last_run_status: String,
    pub history: Vec<AutomationHistoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordUserRule {
    pub id: String,
    pub user_id: String,
    pub label: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvSecret {
    pub key: String,
    pub value: String,
    pub hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapState {
    pub theme: String,
    pub projects: Vec<Project>,
    pub sessions: Vec<Thread>,
    pub accounts: Vec<QuotaAccount>,
    pub mcp_servers: Vec<McpServer>,
    pub skills: Vec<SkillItem>,
    pub automations: Vec<AutomationItem>,
    pub discord_rules: Vec<DiscordUserRule>,
    pub env_secrets: Vec<EnvSecret>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
    pub cwd: String,
    pub repo_url: Option<String>,
    pub discord_channel: Option<String>,
    pub runtime: Option<String>,
    pub wsl_distro: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionInput {
    pub project_id: String,
    pub title: String,
    pub harness: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub account: Option<String>,
    pub permission_mode: Option<String>,
    pub worktree_path: Option<String>,
    pub branch: Option<String>,
    pub shared_deps: Option<Vec<String>>,
    pub is_discord: Option<bool>,
    pub discord_channel: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionInput {
    pub id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub status: Option<String>,
    pub harness: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub account: Option<String>,
    pub permission_mode: Option<String>,
    pub worktree_path: Option<String>,
    pub branch: Option<String>,
    pub shared_deps: Option<Vec<String>>,
    pub additions: Option<i64>,
    pub deletions: Option<i64>,
    pub unread_done: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardConfig {
    pub mode: String,
    pub catastrophic_guardrail: bool,
    pub allowlist_prefixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardApprovalEvent {
    pub session_id: String,
    pub approval: PendingApproval,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    #[serde(rename = "type")]
    pub line_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_line: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_line: Option<i64>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub id: String,
    pub header: String,
    pub staged: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discarded: Option<bool>,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFile {
    pub path: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub additions: i64,
    pub deletions: i64,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFile {
    pub path: String,
    pub language: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordStatus {
    pub running: bool,
    pub bot_tag: Option<String>,
    pub has_token: bool,
    pub connected_guilds: usize,
    pub last_error: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordSettings {
    pub default_harness: String,
    pub default_model: String,
    pub default_effort: String,
    #[serde(default = "default_true")]
    pub react_with_emojis: bool,
    #[serde(default = "default_true")]
    pub auto_start: bool,
}

impl Default for DiscordSettings {
    fn default() -> Self {
        Self {
            default_harness: "auto".to_string(),
            default_model: String::new(),
            default_effort: "medium".to_string(),
            react_with_emojis: true,
            auto_start: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    pub running: bool,
    pub port: u16,
    pub bind_addr: String,
    pub token: String,
    pub magic_link: String,
    pub qr_svg: String,
    pub last_error: Option<String>,
    pub lan_ip: Option<String>,
    pub tailscale_ip: Option<String>,
    pub tailscale_dns: Option<String>,
    pub tailscale_magic_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemTelemetry {
    pub os_name: String,
    pub os_version: String,
    pub host_name: String,
    pub cpu_brand: String,
    pub cpu_cores: usize,
    pub cpu_usage_pct: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    pub uptime_secs: u64,
    pub autostart_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessModelOption {
    pub id: String,
    pub label: String,
    pub family: String,
    pub efforts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteHost {
    pub id: String,
    pub name: String,
    pub url: String,
    pub token: String,
    pub os: String,
    pub hostname: String,
    pub status: String,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertRemoteHostInput {
    pub id: Option<String>,
    pub name: String,
    pub url: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub host_id: String,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_git_repo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<DirEntryItem>,
    pub os: String,
    pub wsl_distro: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WslDistro {
    pub name: String,
    pub state: String,
    pub version: u32,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentItem {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Office {
    pub id: String,
    pub name: String,
    pub project_id: String,
    pub cwd: String,
    pub master_staff_id: String,
    pub max_rounds: i64,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub staff: Vec<OfficeStaff>,
    pub events: Vec<OfficeEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeStaff {
    pub id: String,
    pub office_id: String,
    pub session_id: String,
    pub name: String,
    pub role: String,
    pub is_master: bool,
    pub is_dynamic: bool,
    pub harness: String,
    pub model: String,
    pub effort: String,
    pub account: String,
    pub permission_mode: String,
    pub system_prompt: String,
    pub isolated_worktree: bool,
    pub worktree_path: String,
    pub branch: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    pub avatar_color: String,
    pub desk_order: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeEvent {
    pub id: String,
    pub office_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_staff_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_staff_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_name: Option<String>,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOfficeInput {
    pub name: String,
    pub project_id: Option<String>,
    pub cwd: Option<String>,
    pub runtime: Option<String>,
    pub wsl_distro: Option<String>,
    pub master_name: Option<String>,
    pub master_harness: String,
    pub master_model: String,
    pub master_effort: Option<String>,
    pub master_account: Option<String>,
    pub master_permission_mode: Option<String>,
    pub max_rounds: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HireStaffInput {
    pub office_id: String,
    pub name: String,
    pub role: String,
    pub harness: String,
    pub model: String,
    pub effort: Option<String>,
    pub account: Option<String>,
    pub permission_mode: Option<String>,
    pub system_prompt: Option<String>,
    pub isolated_worktree: Option<bool>,
    pub avatar_color: Option<String>,
    pub is_dynamic: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStaffInput {
    pub id: String,
    pub name: Option<String>,
    pub role: Option<String>,
    pub harness: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub account: Option<String>,
    pub permission_mode: Option<String>,
    pub system_prompt: Option<String>,
    pub isolated_worktree: Option<bool>,
    pub worktree_path: Option<String>,
    pub branch: Option<String>,
}

// ── Feature 1: Voice Mode ──────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceConfig {
    #[serde(default = "default_voice_engine")]
    pub engine: String, // "auto" | "browser" | "whisper"
    #[serde(default = "default_voice_lang")]
    pub language: String,
    #[serde(default)]
    pub whisper_url: String,
    #[serde(default = "default_whisper_model")]
    pub whisper_model: String,
    #[serde(default)]
    pub whisper_cli_path: String,
    #[serde(default)]
    pub auto_send: bool,
    #[serde(default = "default_silence_ms")]
    pub silence_ms: u64,
    #[serde(default)]
    pub tts_enabled: bool,
    #[serde(default)]
    pub tts_voice: String,
    #[serde(default = "default_tts_rate")]
    pub tts_rate: f64,
}

fn default_voice_engine() -> String {
    "auto".to_string()
}
fn default_voice_lang() -> String {
    "en-US".to_string()
}
fn default_whisper_model() -> String {
    "whisper-1".to_string()
}
fn default_silence_ms() -> u64 {
    1500
}
fn default_tts_rate() -> f64 {
    1.05
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            engine: default_voice_engine(),
            language: default_voice_lang(),
            whisper_url: String::new(),
            whisper_model: default_whisper_model(),
            whisper_cli_path: String::new(),
            auto_send: false,
            silence_ms: default_silence_ms(),
            tts_enabled: false,
            tts_voice: String::new(),
            tts_rate: default_tts_rate(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscribeResult {
    pub text: String,
    pub engine: String,
    pub duration_ms: i64,
}

// ── Feature 2: Auto-Failover & Smart Quota Routing ─────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailoverTarget {
    pub harness: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailoverConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_quota_threshold")]
    pub quota_threshold_pct: i64,
    #[serde(default = "default_true")]
    pub rotate_accounts_same_harness: bool,
    #[serde(default)]
    pub fallback_chain: Vec<FailoverTarget>,
}

fn default_quota_threshold() -> i64 {
    90
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            quota_threshold_pct: 90,
            rotate_accounts_same_harness: true,
            fallback_chain: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnFailoverEvent {
    pub session_id: String,
    pub message_id: String,
    pub from_harness: String,
    pub from_account: String,
    pub to_harness: String,
    pub to_account: String,
    pub to_model: String,
    pub reason: String,
    pub hop: u8,
    pub notice: String,
}

// ── Feature 3: Checkpoint Rewind ───────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewindResult {
    pub session: Session,
    pub stashed: bool,
    pub stash_ref: Option<String>,
    pub removed_messages: usize,
    pub reset_git: bool,
}

// ── Feature 4: Multi-Harness Parallel Race Mode ────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RaceContestantInput {
    pub harness: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub account: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RaceContestant {
    pub session_id: String,
    pub harness: String,
    pub model: String,
    pub branch: String,
    pub worktree_path: String,
    #[serde(default)]
    pub worktree_slug: String,
    pub status: String, // "running" | "done" | "error" | "merged"
    pub output: String,
    pub duration_ms: i64,
    pub files_changed: i64,
    pub additions: i64,
    pub deletions: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_ok: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RaceRun {
    pub id: String,
    pub project_id: String,
    pub base_branch: String,
    pub base_cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wsl_distro: Option<String>,
    pub prompt: String,
    pub status: String, // "running" | "completed" | "merged"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winner_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_cmd: Option<String>,
    pub created_at: String,
    pub contestants: Vec<RaceContestant>,
}

// ── Feature 5: Global Full-Text Search (FTS5) ──────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub kind: String, // "message" | "office"
    pub entity_id: String,
    pub session_id: String,
    pub project_id: String,
    pub project_name: String,
    pub session_title: String,
    pub harness: String,
    pub snippet: String,
    pub timestamp: String,
    pub is_discord: bool,
}

// ── Feature 6: Office Blueprints & Telemetry ───────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeBlueprintStaff {
    pub name: String,
    pub role: String,
    pub is_master: bool,
    pub harness: String,
    pub model: String,
    #[serde(default)]
    pub effort: String,
    #[serde(default)]
    pub permission_mode: String,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub isolated_worktree: bool,
    #[serde(default)]
    pub avatar_color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeBlueprintConfig {
    #[serde(default)]
    pub max_rounds: i64,
    pub staff: Vec<OfficeBlueprintStaff>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeBlueprint {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_builtin: bool,
    pub config: OfficeBlueprintConfig,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryBucket {
    pub key: String,
    pub label: String,
    pub spans: i64,
    pub total_tokens: i64,
    pub total_duration_ms: i64,
    pub avg_duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryDayPoint {
    pub day: String,
    pub spans: i64,
    pub total_tokens: i64,
    pub total_duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetrySummary {
    pub total_spans: i64,
    pub total_tokens: i64,
    pub avg_duration_ms: i64,
    pub total_duration_ms: i64,
    pub by_harness: Vec<TelemetryBucket>,
    pub by_project: Vec<TelemetryBucket>,
    pub by_day: Vec<TelemetryDayPoint>,
}

// ── Feature 7: Security & Audit Log ────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogEntry {
    pub id: String,
    pub timestamp: String,
    pub source: String, // "desktop" | "remote" | "discord" | "automation"
    pub action: String,
    pub status: String, // "allowed" | "blocked" | "approved" | "denied"
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_json: Option<String>,
}
