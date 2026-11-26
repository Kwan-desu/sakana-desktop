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
