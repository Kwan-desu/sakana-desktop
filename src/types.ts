export type ThemeId = 'paper' | 'noir' | 'slate' | 'sand';

export type HarnessId = 'agy-cli' | 'claude-code' | 'kiro-cli' | 'sakana';

export type ThreadStatus = 'RUNNING' | 'AWAITING_APPROVAL' | 'DONE' | 'IDLE';

export type ReasoningEffort = 'low' | 'medium' | 'high' | 'xhigh' | '';

export type PermissionMode = 'supervised' | 'auto-edit' | 'yolo';

export type PrimarySection = 'chat' | 'office' | 'dashboard';

export type InspectorTab = 'changes' | 'files' | 'preview' | 'terminal' | 'traces' | 'team';

export interface BrowserState {
  open: boolean;
  visible: boolean;
  url: string;
}

export interface BrowserNavigatedEvent {
  url: string;
  loading: boolean;
}

export interface UiControlEvent {
  kind: string;
  section?: PrimarySection;
  dashboardSubtab?: DashboardSubtab;
  inspectorTab?: InspectorTab;
  inspectorOpen?: boolean;
  sidebarOpen?: boolean;
  splitView?: boolean;
  sessionId?: string;
  officeId?: string;
  staffId?: string;
  url?: string;
  action?: string;
  script?: string;
  message?: string;
}

export type DashboardSubtab =
  | 'overview'
  | 'harnesses'
  | 'mcp'
  | 'automations'
  | 'discord'
  | 'remote';

export interface Project {
  id: string;
  name: string;
  cwd: string;
  repoUrl?: string;
  discordChannel?: string;
  runtime?: 'native' | 'wsl';
  wslDistro?: string;
}

export interface RemoteHost {
  id: string;
  name: string;
  url: string;
  token: string;
  os: string;
  hostname: string;
  status: string;
  lastSeenAt: string;
}

export interface HostInfo {
  hostId: string;
  hostname: string;
  os: string;
  arch: string;
  version: string;
  port: number;
}

export interface DirEntryItem {
  name: string;
  path: string;
  isDir: boolean;
  isGitRepo: boolean;
}

export interface DirListing {
  path: string;
  parent?: string | null;
  entries: DirEntryItem[];
  os: string;
  wslDistro?: string | null;
}

export interface WslDistro {
  name: string;
  state: string;
  version: number;
  isDefault: boolean;
}

export interface ToolCall {
  id: string;
  name: string;
  target: string;
  status: 'ok' | 'running' | 'error';
  durationMs: number;
}

export interface TurnCheckpoint {
  id: string;
  turnNumber: number;
  commitHash: string;
  label: string;
  filesChanged: number;
  additions: number;
  deletions: number;
}

export interface InlineWidgetData {
  title: string;
  subtitle: string;
  sourceCode: string;
  metrics: { label: string; value: string; delta: string; ok: boolean }[];
  bars: { label: string; pct: number; ms: number }[];
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  author?: string;
  timestamp: string;
  text: string;
  thinking?: string;
  thinkingDuration?: string;
  tools?: ToolCall[];
  subagent?: {
    harness: HarnessId;
    command: string;
    summary: string;
    durationMs: number;
  };
  widget?: InlineWidgetData;
  checkpoint?: TurnCheckpoint;
  failoverNotice?: string;
}

export interface PendingApproval {
  id: string;
  command: string;
  cwd: string;
  reason: string;
  risk: 'medium' | 'high';
  requestedBy: HarnessId;
}

export interface Thread {
  id: string;
  projectId: string;
  title: string;
  status: ThreadStatus;
  harness: HarnessId;
  model: string;
  effort: ReasoningEffort;
  account: string;
  permissionMode: PermissionMode;
  worktreePath: string;
  branch: string;
  sharedDeps: string[];
  additions: number;
  deletions: number;
  timeAgo: string;
  isDiscord?: boolean;
  discordChannel?: string;
  unreadDone?: boolean;
  messages: ChatMessage[];
  pendingApproval?: PendingApproval;
}

export interface DiffLine {
  type: 'ctx' | 'add' | 'del';
  oldLine?: number;
  newLine?: number;
  content: string;
}

export interface DiffHunk {
  id: string;
  header: string;
  staged: boolean;
  discarded?: boolean;
  lines: DiffLine[];
}

export interface DiffFile {
  path: string;
  status: 'modified' | 'added' | 'deleted';
  turnId?: string;
  additions: number;
  deletions: number;
  hunks: DiffHunk[];
}

export interface WorkspaceFile {
  path: string;
  language: string;
  lines: string[];
}

export interface TraceSpan {
  id: string;
  actor: string;
  harness: HarnessId;
  action: string;
  durationMs: number;
  tokens: string;
  status: 'ok' | 'running';
  children?: TraceSpan[];
}

export interface AttachmentItem {
  path: string;
  name: string;
  sizeBytes: number;
  kind: 'image' | 'pdf' | 'file';
  previewDataUrl?: string;
}

export interface AttachedContextChip {
  id: string;
  kind?: 'lines' | 'diff-comment' | 'design-element' | 'context' | 'image' | 'pdf' | 'file';
  label: string;
  payload: string;
  path?: string;
  sizeBytes?: number;
  previewDataUrl?: string;
}

export interface QueuedPrompt {
  id: string;
  prompt: string;
}

export interface QuotaAccount {
  id: string;
  harness: HarnessId;
  label: string;
  email: string;
  fiveHourPct: number;
  fiveHourReset: string;
  weeklyPct: number;
  weeklyReset: string;
  isDefault?: boolean;
}

export interface McpServer {
  id: string;
  name: string;
  transport: 'stdio' | 'sse';
  endpoint: string;
  enabled: boolean;
  status: 'connected' | 'idle' | 'error';
  latencyMs?: number;
}

export interface SkillItem {
  slug: string;
  scope: 'global' | 'project';
  description: string;
}

export interface AutomationItem {
  id: string;
  name: string;
  kind: 'cron' | 'webhook';
  schedule: string;
  prompt: string;
  enabled: boolean;
  lastRunStatus: 'OK' | 'ERR';
  history: { ts: string; status: 'OK' | 'ERR'; durationMs: number; summary: string }[];
}

export interface DiscordUserRule {
  id: string;
  userId: string;
  label: string;
  role: 'owner' | 'friend' | 'guest' | 'blocked';
}

export interface EnvSecret {
  key: string;
  value: string;
  hint: string;
}

export interface DiscordStatus {
  running: boolean;
  botTag?: string;
  hasToken: boolean;
  connectedGuilds: number;
  lastError?: string;
}

export interface DiscordSettings {
  defaultHarness: string;
  defaultModel: string;
  defaultEffort: string;
  reactWithEmojis: boolean;
  autoStart: boolean;
}

export interface RemoteStatus {
  running: boolean;
  port: number;
  bindAddr: string;
  token: string;
  magicLink: string;
  qrSvg: string;
  lastError?: string;
  lanIp?: string;
  tailscaleIp?: string;
  tailscaleDns?: string;
  tailscaleMagicLink?: string;
}

export interface SystemTelemetry {
  osName: string;
  osVersion: string;
  hostName: string;
  cpuBrand: string;
  cpuCores: number;
  cpuUsagePct: number;
  memoryUsedBytes: number;
  memoryTotalBytes: number;
  diskUsedBytes: number;
  diskTotalBytes: number;
  uptimeSecs: number;
  autostartEnabled: boolean;
}

export type GuardMode = 'yolo' | 'guarded' | 'strict' | 'readonly';

export interface GuardConfig {
  mode: GuardMode;
  catastrophicGuardrail: boolean;
  allowlistPrefixes: string[];
}

export interface TurnTokenEvent {
  sessionId: string;
  messageId: string;
  token: string;
}

export interface TurnThinkingEvent {
  sessionId: string;
  messageId: string;
  thinking: string;
  durationMs?: number;
}

export interface TurnToolEvent {
  sessionId: string;
  messageId: string;
  tool: ToolCall;
}

export interface TurnSubagentEvent {
  sessionId: string;
  messageId: string;
  subagent: NonNullable<ChatMessage['subagent']>;
}

export interface TurnDoneEvent {
  sessionId: string;
  messageId: string;
  message?: ChatMessage;
  status?: ThreadStatus;
}

export interface TurnErrorEvent {
  sessionId: string;
  messageId: string;
  error: string;
}

export interface GuardApprovalEvent {
  sessionId: string;
  approval: PendingApproval;
}

export interface PtyDataEvent {
  sessionId: string;
  data: string;
}

export interface CustomThemePreset {
  id: string;
  name: string;
  baseTheme: ThemeId;
  cssVars: Record<string, string>;
  customCss: string;
}

export interface AppCustomization {
  enabled: boolean;
  theme: ThemeId;
  customThemeName?: string | null;
  cssVars: Record<string, string>;
  customCss: string;
  uiScale?: number | null;
  savedPresets: CustomThemePreset[];
}

export interface SyncConfig {
  enabled: boolean;
  peerHostId?: string | null;
  syncSessions: boolean;
  syncFiles: boolean;
  intervalSecs: number;
  conflictPolicy: 'newest-wins' | 'local-wins' | 'remote-wins';
}

export interface SyncStatus {
  running: boolean;
  syncing: boolean;
  peerName?: string | null;
  lastSyncAt?: string | null;
  lastError?: string | null;
  sessionsSynced: number;
  filesPulled: number;
  filesPushed: number;
}

export interface PeerMessageEvent {
  fromSessionId: string;
  toSessionId: string;
  fromHarness: string;
  fromTitle: string;
  message: string;
  expectReply: boolean;
}

export interface PtyExitEvent {
  sessionId: string;
  code: number;
}

export interface HarnessModelOption {
  id: string;
  label: string;
  family: string;
  efforts: string[];
}

export interface HarnessAuthAccount {
  id: string;
  harness: HarnessId;
  label: string;
  email: string;
  profileDir: string;
  authType: string;
  isDefault: boolean;
  authenticated: boolean;
  status: string;
  cliInstalled: boolean;
  cliBin: string;
  lastLoginOutput: string;
}

export interface HarnessLoginResponse {
  started: boolean;
  accountId: string;
  harness: HarnessId;
  authType: string;
  mode: string;
  authenticated: boolean;
  needsCode: boolean;
  authUrl: string;
  deviceCode: string;
  email: string;
  message: string;
}

export interface GitHubAuthStatus {
  authenticated: boolean;
  login: string;
  name: string;
  avatarUrl: string;
  source: 'token' | 'gh-cli' | 'none' | string;
  ghInstalled: boolean;
  defaultCloneDir: string;
}

export interface GitHubRepo {
  id: number;
  name: string;
  fullName: string;
  description: string;
  cloneUrl: string;
  htmlUrl: string;
  private: boolean;
  defaultBranch: string;
  language: string;
  updatedAt: string;
  stargazersCount: number;
}

export interface UpdateCheckResult {
  available: boolean;
  currentVersion: string;
  latestVersion?: string;
  body?: string;
  error?: string;
  releaseUrl?: string;
}

export interface CompactionConfig {
  autoCompact: boolean;
  messageThreshold: number;
  charThreshold: number;
  keepRecent: number;
}

export interface CompactionResult {
  sessionId: string;
  compactedCount: number;
  remainingCount: number;
  summary: string;
  message?: ChatMessage;
  autoTriggered: boolean;
}

export interface DiscordPromptEvent {
  sessionId: string;
  channel: string;
  author: string;
  prompt: string;
}

export interface GuardResolvedEvent {
  approvalId: string;
  sessionId: string;
  approved: boolean;
  command: string;
}

export interface BootstrapState {
  theme: ThemeId;
  projects: Project[];
  sessions: Thread[];
  accounts?: QuotaAccount[];
  quotas?: QuotaAccount[];
  mcpServers?: McpServer[];
  skills?: SkillItem[];
  automations?: AutomationItem[];
  discordRules?: DiscordUserRule[];
  discordUsers?: DiscordUserRule[];
  envSecrets?: EnvSecret[];
  secrets?: EnvSecret[];
}

export type OfficeStaffStatus = 'IDLE' | 'WORKING' | 'DONE' | 'ERROR';

export type OfficeEventKind =
  | 'delegation'
  | 'hire'
  | 'fire'
  | 'report'
  | 'message'
  | 'system'
  | 'error';

export interface OfficeStaff {
  id: string;
  officeId: string;
  sessionId: string;
  name: string;
  role: string;
  isMaster: boolean;
  isDynamic: boolean;
  harness: HarnessId;
  model: string;
  effort: ReasoningEffort;
  account: string;
  permissionMode: PermissionMode;
  systemPrompt: string;
  isolatedWorktree: boolean;
  worktreePath: string;
  branch: string;
  status: OfficeStaffStatus;
  currentTask?: string | null;
  avatarColor: string;
  deskOrder: number;
  createdAt: string;
}

export interface OfficeEvent {
  id: string;
  officeId: string;
  kind: OfficeEventKind;
  fromStaffId?: string | null;
  fromName?: string | null;
  toStaffId?: string | null;
  toName?: string | null;
  summary: string;
  detail?: string | null;
  timestamp: string;
}

export interface Office {
  id: string;
  name: string;
  projectId: string;
  cwd: string;
  masterStaffId: string;
  maxRounds: number;
  status: 'IDLE' | 'RUNNING';
  createdAt: string;
  updatedAt: string;
  staff: OfficeStaff[];
  events: OfficeEvent[];
}

export interface OfficeDelegationEdge {
  officeId: string;
  fromStaffId: string;
  toStaffId: string;
  task: string;
}

export interface CreateOfficeInput {
  name: string;
  projectId?: string;
  cwd?: string;
  runtime?: 'native' | 'wsl';
  wslDistro?: string;
  masterName?: string;
  masterHarness: HarnessId;
  masterModel: string;
  masterEffort?: ReasoningEffort;
  masterAccount?: string;
  masterPermissionMode?: PermissionMode;
  maxRounds?: number;
}

export interface HireStaffInput {
  officeId: string;
  name: string;
  role: string;
  harness: HarnessId;
  model: string;
  effort?: ReasoningEffort;
  account?: string;
  permissionMode?: PermissionMode;
  systemPrompt?: string;
  isolatedWorktree?: boolean;
  avatarColor?: string;
  isDynamic?: boolean;
}

export interface UpdateStaffInput {
  id: string;
  name?: string;
  role?: string;
  harness?: HarnessId;
  model?: string;
  effort?: ReasoningEffort;
  account?: string;
  permissionMode?: PermissionMode;
  systemPrompt?: string;
  isolatedWorktree?: boolean;
  worktreePath?: string;
  branch?: string;
}

// ── Feature 1: Voice Mode ──────────────────────────────────────────
export interface VoiceConfig {
  engine: 'auto' | 'browser' | 'whisper' | string;
  language: string;
  whisperUrl: string;
  whisperModel: string;
  whisperCliPath: string;
  autoSend: boolean;
  silenceMs: number;
  ttsEnabled: boolean;
  ttsVoice: string;
  ttsRate: number;
}

export interface TranscribeResult {
  text: string;
  engine: string;
  durationMs: number;
}

// ── Feature 2: Auto-Failover & Smart Quota Routing ─────────────────
export interface FailoverTarget {
  harness: HarnessId | string;
  model: string;
  account?: string | null;
}

export interface FailoverConfig {
  enabled: boolean;
  quotaThresholdPct: number;
  rotateAccountsSameHarness: boolean;
  fallbackChain: FailoverTarget[];
}

export interface TurnFailoverEvent {
  sessionId: string;
  messageId: string;
  fromHarness: string;
  fromAccount: string;
  toHarness: string;
  toAccount: string;
  toModel: string;
  reason: string;
  hop: number;
  notice: string;
}

// ── Feature 3: Checkpoint Rewind ───────────────────────────────────
export interface RewindResult {
  session: Thread;
  stashed: boolean;
  stashRef?: string | null;
  removedMessages: number;
  resetGit: boolean;
}

// ── Feature 4: Multi-Harness Parallel Race Mode ────────────────────
export interface RaceContestantInput {
  harness: HarnessId | string;
  model?: string;
  account?: string | null;
}

export interface RaceContestant {
  sessionId: string;
  harness: string;
  model: string;
  branch: string;
  worktreePath: string;
  status: 'running' | 'done' | 'error' | 'merged' | string;
  output: string;
  durationMs: number;
  filesChanged: number;
  additions: number;
  deletions: number;
  verifyOk?: boolean | null;
  verifyOutput?: string | null;
  error?: string | null;
}

export interface RaceRun {
  id: string;
  projectId: string;
  baseBranch: string;
  baseCwd: string;
  prompt: string;
  status: 'running' | 'completed' | 'merged' | string;
  winnerSessionId?: string | null;
  verifyCmd?: string | null;
  createdAt: string;
  contestants: RaceContestant[];
}

// ── Feature 5: Global Full-Text Search (FTS5) ──────────────────────
export interface SearchHit {
  kind: 'message' | 'office' | string;
  entityId: string;
  sessionId: string;
  projectId: string;
  projectName: string;
  sessionTitle: string;
  harness: string;
  snippet: string;
  timestamp: string;
  isDiscord: boolean;
}

// ── Feature 6: Office Blueprints & Telemetry ───────────────────────
export interface OfficeBlueprintStaff {
  name: string;
  role: string;
  isMaster: boolean;
  harness: HarnessId | string;
  model: string;
  effort?: string;
  permissionMode?: string;
  systemPrompt?: string;
  isolatedWorktree?: boolean;
  avatarColor?: string;
}

export interface OfficeBlueprintConfig {
  maxRounds: number;
  staff: OfficeBlueprintStaff[];
}

export interface OfficeBlueprint {
  id: string;
  name: string;
  description: string;
  isBuiltin: boolean;
  config: OfficeBlueprintConfig;
  createdAt: string;
}

export interface TelemetryBucket {
  key: string;
  label: string;
  spans: number;
  totalTokens: number;
  totalDurationMs: number;
  avgDurationMs: number;
}

export interface TelemetryDayPoint {
  day: string;
  spans: number;
  totalTokens: number;
  totalDurationMs: number;
}

export interface TelemetrySummary {
  totalSpans: number;
  totalTokens: number;
  avgDurationMs: number;
  totalDurationMs: number;
  byHarness: TelemetryBucket[];
  byProject: TelemetryBucket[];
  byDay: TelemetryDayPoint[];
}

// ── Feature 7: Security & Audit Log ────────────────────────────────
export interface AuditLogEntry {
  id: string;
  timestamp: string;
  source: 'desktop' | 'remote' | 'discord' | 'automation' | string;
  action: string;
  status: 'allowed' | 'blocked' | 'approved' | 'denied' | string;
  summary: string;
  payloadJson?: string | null;
}
