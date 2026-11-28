import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AppCustomization,
  AttachedContextChip,
  PeerMessageEvent,
  SyncConfig,
  SyncStatus,
  AttachmentItem,
  AutomationItem,
  BootstrapState,
  ChatMessage,
  CreateOfficeInput,
  DiffFile,
  DirListing,
  DiscordPromptEvent,
  DiscordSettings,
  DiscordStatus,
  DiscordUserRule,
  EnvSecret,
  GitHubAuthStatus,
  GitHubRepo,
  HarnessAuthAccount,
  HarnessId,
  HarnessLoginResponse,
  HarnessModelOption,
  HireStaffInput,
  HostInfo,
  McpServer,
  Office,
  OfficeDelegationEdge,
  OfficeEvent,
  OfficeStaff,
  PendingApproval,
  PermissionMode,
  Project,
  PtyDataEvent,
  PtyExitEvent,
  QuotaAccount,
  ReasoningEffort,
  RemoteHost,
  RemoteStatus,
  SkillItem,
  SystemTelemetry,
  ThemeId,
  Thread,
  ThreadStatus,
  TraceSpan,
  TurnCheckpoint,
  TurnDoneEvent,
  TurnErrorEvent,
  TurnSubagentEvent,
  TurnThinkingEvent,
  TurnTokenEvent,
  TurnToolEvent,
  UpdateCheckResult,
  CompactionConfig,
  CompactionResult,
  UpdateStaffInput,
  WorkspaceFile,
  WslDistro,
} from '../types';

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__);
}

const LOCAL_ONLY_COMMANDS = new Set([
  'list_remote_hosts',
  'upsert_remote_host',
  'delete_remote_host',
  'test_remote_host',
  'remote_rpc',
  'connect_remote_events',
  'disconnect_remote_events',
  'pick_project_folder',
  'pick_attachment_files',
  'open_external_url',
  'get_remote_status',
  'start_remote_server',
  'stop_remote_server',
  'set_autostart',
  'check_for_updates',
  'install_update',
]);

let activeHostId = 'local';

export function setActiveHost(hostId: string): void {
  activeHostId = hostId || 'local';
}

export function getActiveHost(): string {
  return activeHostId;
}

export async function tauriInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  if (activeHostId !== 'local' && !LOCAL_ONLY_COMMANDS.has(cmd)) {
    return invoke<T>('remote_rpc', {
      hostId: activeHostId,
      cmd,
      args: args ?? {},
    });
  }
  return invoke<T>(cmd, args);
}

export async function tauriListen<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) {
    return () => {};
  }
  return listen<T>(event, (e) => handler(e.payload));
}

export interface CreateProjectInput {
  name: string;
  cwd: string;
  repoUrl?: string;
  discordChannel?: string;
  runtime?: 'native' | 'wsl';
  wslDistro?: string;
}

export interface CreateSessionInput {
  projectId: string;
  title: string;
  branch: string;
  harness: HarnessId;
  model?: string;
  effort?: ReasoningEffort;
  account?: string;
  permissionMode?: PermissionMode;
  worktreePath?: string;
  sharedDeps?: string[];
}

export interface UpdateSessionInput {
  id: string;
  projectId?: string;
  title?: string;
  status?: ThreadStatus;
  harness?: HarnessId;
  model?: string;
  effort?: ReasoningEffort;
  account?: string;
  permissionMode?: PermissionMode;
  additions?: number;
  deletions?: number;
}

export interface UpsertQuotaAccountInput {
  id?: string;
  harness: HarnessId | string;
  label: string;
  email: string;
  fiveHourPct: number;
  fiveHourReset?: string;
  weeklyPct: number;
  weeklyReset?: string;
  isDefault?: boolean;
}

export const tauriApi = {
  getBootstrapState: () => tauriInvoke<BootstrapState>('get_bootstrap_state'),

  listProjects: () => tauriInvoke<Project[]>('list_projects'),

  createProject: (input: CreateProjectInput) =>
    tauriInvoke<Project>('create_project', { input }),

  pickProjectFolder: (): Promise<string | null> =>
    tauriInvoke<string>('pick_project_folder'),

  pickAttachmentFiles: async (): Promise<AttachmentItem[]> =>
    (await tauriInvoke<AttachmentItem[]>('pick_attachment_files')) ?? [],

  saveClipboardAttachment: (name: string, base64Data: string): Promise<AttachmentItem | null> =>
    tauriInvoke<AttachmentItem>('save_clipboard_attachment', { name, base64Data }),

  renameProject: (id: string, name: string) =>
    tauriInvoke<boolean>('rename_project', { id, name }),

  setProjectDiscordChannel: (id: string, discordChannel?: string | null) =>
    tauriInvoke<boolean>('set_project_discord_channel', {
      id,
      discordChannel: discordChannel ?? null,
    }),

  setSessionDiscordChannel: (id: string, discordChannel?: string | null) =>
    tauriInvoke<boolean>('set_session_discord_channel', {
      id,
      discordChannel: discordChannel ?? null,
    }),

  openFileExternal: (cwd: string, relPath: string, wslDistro?: string) =>
    tauriInvoke<void>('open_file_external', {
      cwd,
      relPath,
      ...(wslDistro ? { wslDistro } : {}),
    }),

  reorderProjects: (orderedIds: string[]) =>
    tauriInvoke<void>('reorder_projects', { orderedIds }),

  reorderSessions: (orderedIds: string[]) =>
    tauriInvoke<void>('reorder_sessions', { orderedIds }),

  deleteProject: (id: string) =>
    tauriInvoke<void>('delete_project', { id }),

  listSessions: (projectId?: string) =>
    tauriInvoke<Thread[]>('list_sessions', projectId ? { projectId } : {}),

  createSession: (input: CreateSessionInput) =>
    tauriInvoke<Thread>('create_session', { input }),

  updateSession: (input: UpdateSessionInput) =>
    tauriInvoke<Thread>('update_session', { input }),

  deleteSession: (id: string) =>
    tauriInvoke<void>('delete_session', { id }),

  forkSession: (opts: {
    sourceId: string;
    title: string;
    branch: string;
    upToIndex: number;
  }) => tauriInvoke<Thread>('fork_session', opts),

  setTheme: (theme: ThemeId) =>
    tauriInvoke<void>('set_theme', { theme }),

  // Phase 2: Live turns and skills
  sendTurn: (
    sessionId: string,
    prompt: string,
    attachments: AttachedContextChip[] = [],
    peerSessionId?: string | null,
  ) =>
    tauriInvoke<string>('send_turn', {
      sessionId,
      prompt,
      attachments,
      ...(peerSessionId ? { peerSessionId } : {}),
    }),

  cancelTurn: (sessionId: string) =>
    tauriInvoke<void>('cancel_turn', { sessionId }),

  listSkills: (projectCwd: string) =>
    tauriInvoke<SkillItem[]>('list_skills', { projectCwd }),


  // Phase 2: Typed Tauri event listeners
  onTurnToken: (handler: (payload: TurnTokenEvent) => void) =>
    tauriListen<TurnTokenEvent>('turn:token', handler),

  onTurnThinking: (handler: (payload: TurnThinkingEvent) => void) =>
    tauriListen<TurnThinkingEvent>('turn:thinking', handler),

  onTurnTool: (handler: (payload: TurnToolEvent) => void) =>
    tauriListen<TurnToolEvent>('turn:tool', handler),

  onTurnSubagent: (handler: (payload: TurnSubagentEvent) => void) =>
    tauriListen<TurnSubagentEvent>('turn:subagent', handler),

  onTurnDone: (handler: (payload: TurnDoneEvent) => void) =>
    tauriListen<TurnDoneEvent>('turn:done', handler),

  onTurnError: (handler: (payload: TurnErrorEvent) => void) =>
    tauriListen<TurnErrorEvent>('turn:error', handler),

  onAutomationRun: (handler: (payload: AutomationItem) => void) =>
    tauriListen<AutomationItem>('automation-run', handler),

  onDiscordPrompt: (handler: (payload: DiscordPromptEvent) => void) =>
    tauriListen<DiscordPromptEvent>('discord-prompt', handler),

  // Phase 3: Git worktrees, branches, checkpoints, diffs, commits, PRs
  createWorktree: (
    sessionId: string,
    projectCwd: string,
    branch: string,
    sharedDeps: string[] = [],
  ) =>
    tauriInvoke<string>('create_worktree', {
      sessionId,
      projectCwd,
      branch,
      sharedDeps,
    }),

  removeWorktree: (sessionId: string, projectCwd: string) =>
    tauriInvoke<void>('remove_worktree', { sessionId, projectCwd }),

  listBranches: (cwd: string) =>
    tauriInvoke<string[]>('list_branches', { cwd }),

  checkoutBranch: (cwd: string, branch: string) =>
    tauriInvoke<void>('checkout_branch', { cwd, branch }),

  gitPull: (cwd: string) => tauriInvoke<string>('git_pull', { cwd }),

  gitPush: (cwd: string) => tauriInvoke<string>('git_push', { cwd }),

  createCheckpoint: (
    sessionId: string,
    turnNumber: number,
    label: string,
    cwd: string,
  ) =>
    tauriInvoke<TurnCheckpoint>('create_checkpoint', {
      sessionId,
      turnNumber,
      label,
      cwd,
    }),

  getCheckpointDiff: (sessionId: string, checkpointId: string) =>
    tauriInvoke<DiffFile[]>('get_checkpoint_diff', {
      sessionId,
      checkpointId,
    }),

  revertToCheckpoint: (sessionId: string, checkpointId: string) =>
    tauriInvoke<void>('revert_to_checkpoint', {
      sessionId,
      checkpointId,
    }),

  getGitDiffs: (cwd: string, turnId?: string, wslDistro?: string) =>
    tauriInvoke<DiffFile[]>(
      'get_git_diffs',
      {
        cwd,
        ...(turnId && turnId !== 'all' ? { turnId } : {}),
        ...(wslDistro ? { wslDistro } : {}),
      },
    ),

  stageHunk: (
    cwd: string,
    filePath: string,
    hunkHeader: string,
    stage: boolean,
    wslDistro?: string,
  ) =>
    tauriInvoke<void>('stage_hunk', {
      cwd,
      filePath,
      hunkHeader,
      stage,
      ...(wslDistro ? { wslDistro } : {}),
    }),

  discardHunk: (cwd: string, filePath: string, hunkHeader: string, wslDistro?: string) =>
    tauriInvoke<void>('discard_hunk', {
      cwd,
      filePath,
      hunkHeader,
      ...(wslDistro ? { wslDistro } : {}),
    }),

  draftCommitMessage: (cwd: string) =>
    tauriInvoke<string>('draft_commit_message', { cwd }),

  gitCommit: (cwd: string, message: string) =>
    tauriInvoke<string>('git_commit', { cwd, message }),

  createPr: (
    cwd: string,
    title: string,
    body: string,
    baseBranch: string = 'main',
    wslDistro?: string,
  ) =>
    tauriInvoke<string>('create_pr', {
      cwd,
      title,
      body,
      baseBranch,
      ...(wslDistro ? { wslDistro } : {}),
    }),

  githubAuthStatus: () => tauriInvoke<GitHubAuthStatus>('github_auth_status'),

  githubSaveToken: (token: string) =>
    tauriInvoke<GitHubAuthStatus>('github_save_token', { token }),

  githubListRepos: async (query?: string): Promise<GitHubRepo[]> =>
    (await tauriInvoke<GitHubRepo[]>('github_list_repos', query ? { query } : {})) ?? [],

  githubCloneRepo: (cloneUrl: string, repoName: string, targetParentDir?: string) =>
    tauriInvoke<string>('github_clone_repo', {
      cloneUrl,
      repoName,
      ...(targetParentDir ? { targetParentDir } : {}),
    }),

  // Phase 3: Workspace files & trace spans
  listWorkspaceFiles: (cwd: string, wslDistro?: string) =>
    tauriInvoke<string[]>('list_workspace_files', {
      cwd,
      ...(wslDistro ? { wslDistro } : {}),
    }),

  readWorkspaceFile: (cwd: string, relPath: string, wslDistro?: string) =>
    tauriInvoke<WorkspaceFile>('read_workspace_file', {
      cwd,
      relPath,
      ...(wslDistro ? { wslDistro } : {}),
    }),

  writeWorkspaceFile: (cwd: string, relPath: string, content: string, wslDistro?: string) =>
    tauriInvoke<void>('write_workspace_file', {
      cwd,
      relPath,
      content,
      ...(wslDistro ? { wslDistro } : {}),
    }),

  listTraceSpans: (sessionId: string) =>
    tauriInvoke<TraceSpan[]>('list_trace_spans', { sessionId }),

  // Phase 3: Interactive PTY & NL command translation
  ptySpawn: (sessionId: string, cwd: string, cols: number, rows: number, wslDistro?: string) =>
    tauriInvoke<void>('pty_spawn', {
      sessionId,
      cwd,
      cols,
      rows,
      ...(wslDistro ? { wslDistro } : {}),
    }),

  ptyWrite: (sessionId: string, data: string) =>
    tauriInvoke<void>('pty_write', { sessionId, data }),

  ptyResize: (sessionId: string, cols: number, rows: number) =>
    tauriInvoke<void>('pty_resize', { sessionId, cols, rows }),

  ptyKill: (sessionId: string) =>
    tauriInvoke<void>('pty_kill', { sessionId }),

  translateNlCommand: (prompt: string, os: string, shell: string) =>
    tauriInvoke<string>('translate_nl_command', { prompt, os, shell }),

  onPtyData: (handler: (payload: PtyDataEvent) => void) =>
    tauriListen<PtyDataEvent>('pty:data', handler),

  onPtyExit: (handler: (payload: PtyExitEvent) => void) =>
    tauriListen<PtyExitEvent>('pty:exit', handler),

  // Phase 4: MCP Hub
  listMcpServers: () => tauriInvoke<McpServer[]>('list_mcp_servers'),

  addMcpServer: (name: string, transport: 'stdio' | 'sse', endpoint: string) =>
    tauriInvoke<McpServer>('add_mcp_server', { name, transport, endpoint }),

  toggleMcpServer: (id: string, enabled: boolean) =>
    tauriInvoke<McpServer>('toggle_mcp_server', { id, enabled }),

  deleteMcpServer: (id: string) =>
    tauriInvoke<void>('delete_mcp_server', { id }),

  pingMcpServer: (id: string) =>
    tauriInvoke<McpServer>('ping_mcp_server', { id }),

  importAgyMcpServers: () =>
    tauriInvoke<McpServer[]>('import_agy_mcp_servers'),

  // Phase 4: Automations
  listAutomations: () => tauriInvoke<AutomationItem[]>('list_automations'),

  createAutomation: (
    name: string,
    kind: 'cron' | 'webhook',
    schedule: string,
    prompt: string,
  ) =>
    tauriInvoke<AutomationItem>('create_automation', {
      name,
      kind,
      schedule,
      prompt,
    }),

  toggleAutomation: (id: string, enabled: boolean) =>
    tauriInvoke<AutomationItem>('toggle_automation', { id, enabled }),

  deleteAutomation: (id: string) =>
    tauriInvoke<void>('delete_automation', { id }),

  runAutomationNow: (id: string) =>
    tauriInvoke<AutomationItem>('run_automation_now', { id }),

  // Phase 4: Discord & RBAC
  getDiscordStatus: () => tauriInvoke<DiscordStatus>('get_discord_status'),

  startDiscordBot: (token?: string) =>
    tauriInvoke<DiscordStatus>('start_discord_bot', token ? { token } : {}),

  stopDiscordBot: () => tauriInvoke<DiscordStatus>('stop_discord_bot'),

  upsertDiscordRule: (
    userId: string,
    label: string,
    role: DiscordUserRule['role'],
  ) =>
    tauriInvoke<DiscordUserRule>('upsert_discord_rule', {
      userId,
      label,
      role,
    }),

  deleteDiscordRule: (id: string) =>
    tauriInvoke<void>('delete_discord_rule', { id }),

  getDiscordSettings: () =>
    tauriInvoke<DiscordSettings>('get_discord_settings'),

  updateDiscordSettings: (settings: DiscordSettings) =>
    tauriInvoke<DiscordSettings>('update_discord_settings', { settings }),

  // Phase 4: Remote Access
  getRemoteStatus: () => tauriInvoke<RemoteStatus>('get_remote_status'),

  startRemoteServer: (port?: number, lanEnabled?: boolean) =>
    tauriInvoke<RemoteStatus>('start_remote_server', {
      ...(port !== undefined ? { port } : {}),
      ...(lanEnabled !== undefined ? { lanEnabled } : {}),
    }),

  stopRemoteServer: () => tauriInvoke<RemoteStatus>('stop_remote_server'),

  // Phase 4: System Telemetry, Quotas, .env Secrets & Autostart
  getSystemTelemetry: () =>
    tauriInvoke<SystemTelemetry>('get_system_telemetry'),

  listQuotaAccounts: () => tauriInvoke<QuotaAccount[]>('list_quota_accounts'),

  upsertQuotaAccount: (input: UpsertQuotaAccountInput) =>
    tauriInvoke<QuotaAccount>('upsert_quota_account', { ...input }),

  deleteQuotaAccount: (id: string) =>
    tauriInvoke<void>('delete_quota_account', { id }),

  listEnvSecrets: () => tauriInvoke<EnvSecret[]>('list_env_secrets'),

  revealEnvSecret: async (key: string): Promise<string> =>
    (await tauriInvoke<string>('reveal_env_secret', { key })) ?? '',

  setEnvSecret: (key: string, value: string) =>
    tauriInvoke<EnvSecret[]>('set_env_secret', { key, value }),

  deleteEnvSecret: (key: string) =>
    tauriInvoke<void>('delete_env_secret', { key }),

  setAutostart: (enabled: boolean) =>
    tauriInvoke<boolean>('set_autostart', { enabled }),

  checkForUpdates: async (): Promise<UpdateCheckResult> =>
    (await tauriInvoke<UpdateCheckResult>('check_for_updates')) ?? {
      available: false,
      currentVersion: '0.1.0',
      error: 'Updater requires Tauri desktop runtime',
    },

  installUpdate: async (): Promise<void> => {
    await tauriInvoke<void>('install_update');
  },

  onUpdateProgress: (
    handler: (payload: { downloaded: number; total?: number | null }) => void,
  ) => tauriListen<{ downloaded: number; total?: number | null }>('update:progress', handler),

  compactSession: async (sessionId: string): Promise<CompactionResult | null> =>
    (await tauriInvoke<CompactionResult>('compact_session', { sessionId })) ?? null,

  getCompactionConfig: async (): Promise<CompactionConfig> =>
    (await tauriInvoke<CompactionConfig>('get_compaction_config')) ?? {
      autoCompact: true,
      messageThreshold: 36,
      charThreshold: 32000,
      keepRecent: 8,
    },

  updateCompactionConfig: async (config: CompactionConfig): Promise<CompactionConfig> =>
    (await tauriInvoke<CompactionConfig>('update_compaction_config', { config })) ?? config,

  onSessionCompacted: (handler: (payload: CompactionResult) => void) =>
    tauriListen<CompactionResult>('session:compacted', handler),

  listHarnessModels: async (refresh?: boolean): Promise<Record<string, HarnessModelOption[]>> =>
    (await tauriInvoke<Record<string, HarnessModelOption[]>>(
      'list_harness_models',
      refresh !== undefined ? { refresh } : {},
    )) ?? {},

  // CLI Harness Auth (AGY, Kiro, Claude)
  listHarnessAccounts: async (harness?: string): Promise<HarnessAuthAccount[]> =>
    (await tauriInvoke<HarnessAuthAccount[]>(
      'list_harness_accounts',
      harness ? { harness } : {},
    )) ?? [],

  createHarnessAccount: async (
    harness: string,
    label: string,
    email?: string,
    authType?: string,
  ): Promise<HarnessAuthAccount[]> =>
    (await tauriInvoke<HarnessAuthAccount[]>('create_harness_account', {
      harness,
      label,
      email: email ?? null,
      authType: authType ?? null,
    })) ?? [],

  renameHarnessAccount: async (
    accountId: string,
    label: string,
  ): Promise<HarnessAuthAccount[]> =>
    (await tauriInvoke<HarnessAuthAccount[]>('rename_harness_account', {
      accountId,
      label,
    })) ?? [],

  setDefaultHarnessAccount: async (accountId: string): Promise<HarnessAuthAccount[]> =>
    (await tauriInvoke<HarnessAuthAccount[]>('set_default_harness_account', {
      accountId,
    })) ?? [],

  deleteHarnessAccount: async (accountId: string): Promise<HarnessAuthAccount[]> =>
    (await tauriInvoke<HarnessAuthAccount[]>('delete_harness_account', {
      accountId,
    })) ?? [],

  startHarnessLogin: async (
    accountId: string,
    authType?: string,
    mode?: string,
  ): Promise<HarnessLoginResponse | null> =>
    tauriInvoke<HarnessLoginResponse>('start_harness_login', {
      accountId,
      authType: authType ?? null,
      mode: mode ?? null,
    }),

  submitHarnessLoginCode: async (
    accountId: string,
    code: string,
  ): Promise<HarnessLoginResponse | null> =>
    tauriInvoke<HarnessLoginResponse>('submit_harness_login_code', {
      accountId,
      code,
    }),

  checkHarnessAuth: async (accountId: string): Promise<HarnessAuthAccount | null> =>
    tauriInvoke<HarnessAuthAccount>('check_harness_auth', { accountId }),

  logoutHarnessAccount: async (accountId: string): Promise<HarnessAuthAccount[]> =>
    (await tauriInvoke<HarnessAuthAccount[]>('logout_harness_account', {
      accountId,
    })) ?? [],

  openExternalUrl: async (url: string): Promise<void> => {
    await tauriInvoke<void>('open_external_url', { url });
  },

  // Remote Sakana Instances (Client Mode) & WSL/Remote Directory Browsing
  listRemoteHosts: async (): Promise<RemoteHost[]> =>
    (await tauriInvoke<RemoteHost[]>('list_remote_hosts')) ?? [],

  upsertRemoteHost: async (input: {
    id?: string;
    name: string;
    url: string;
    token: string;
  }): Promise<RemoteHost[]> =>
    (await tauriInvoke<RemoteHost[]>('upsert_remote_host', { input })) ?? [],

  deleteRemoteHost: async (id: string): Promise<RemoteHost[]> =>
    (await tauriInvoke<RemoteHost[]>('delete_remote_host', { id })) ?? [],

  testRemoteHost: async (id: string): Promise<HostInfo | null> =>
    tauriInvoke<HostInfo>('test_remote_host', { id }),

  connectRemoteEvents: async (hostId: string): Promise<void> => {
    await tauriInvoke<void>('connect_remote_events', { hostId });
  },

  disconnectRemoteEvents: async (): Promise<void> => {
    await tauriInvoke<void>('disconnect_remote_events');
  },

  listWslDistros: async (): Promise<WslDistro[]> =>
    (await tauriInvoke<WslDistro[]>('list_wsl_distros')) ?? [],

  listDirectories: async (path?: string, wslDistro?: string): Promise<DirListing | null> =>
    tauriInvoke<DirListing>('list_directories', {
      path: path ?? null,
      wslDistro: wslDistro ?? null,
    }),

  // Office Multi-Agent Orchestration
  listOffices: async (): Promise<Office[]> =>
    (await tauriInvoke<Office[]>('list_offices')) ?? [],

  getOffice: async (officeId: string): Promise<Office | undefined> =>
    (await tauriInvoke<Office>('get_office', { id: officeId })) ?? undefined,

  createOffice: async (input: CreateOfficeInput): Promise<Office | undefined> =>
    (await tauriInvoke<Office>('create_office', { input })) ?? undefined,

  deleteOffice: async (officeId: string): Promise<boolean | undefined> =>
    (await tauriInvoke<boolean>('delete_office', { id: officeId })) ?? undefined,

  updateOfficeSettings: async (
    officeId: string,
    maxRounds: number,
  ): Promise<Office | undefined> =>
    (await tauriInvoke<Office>('update_office_settings', { officeId, maxRounds })) ??
    undefined,

  hireOfficeStaff: async (input: HireStaffInput): Promise<OfficeStaff | undefined> =>
    (await tauriInvoke<OfficeStaff>('hire_office_staff', { input })) ?? undefined,

  updateOfficeStaff: async (input: UpdateStaffInput): Promise<OfficeStaff | undefined> =>
    (await tauriInvoke<OfficeStaff>('update_office_staff', { input })) ?? undefined,

  fireOfficeStaff: async (staffId: string): Promise<boolean | undefined> =>
    (await tauriInvoke<boolean>('fire_office_staff', { id: staffId })) ?? undefined,

  getOfficeStaffMessages: async (staffId: string): Promise<ChatMessage[]> =>
    (await tauriInvoke<ChatMessage[]>('get_office_staff_messages', { staffId })) ?? [],

  sendOfficeMessage: async (
    officeId: string,
    targetStaffId: string,
    prompt: string,
    attachments: AttachedContextChip[] = [],
  ): Promise<string | undefined> =>
    (await tauriInvoke<string>('send_office_message', {
      officeId,
      targetStaffId,
      prompt,
      attachments,
    })) ?? undefined,

  reportStaffToMaster: async (
    officeId: string,
    staffId: string,
    note?: string,
  ): Promise<void> => {
    await tauriInvoke<void>('report_staff_to_master', {
      officeId,
      staffId,
      note: note ?? null,
    });
  },

  stopOfficeRun: async (officeId: string): Promise<void> => {
    await tauriInvoke<void>('stop_office_run', { officeId });
  },

  onOfficeUpdated: (handler: (payload: Office) => void) =>
    tauriListen<Office>('office:updated', handler),

  onOfficeEvent: (handler: (payload: OfficeEvent) => void) =>
    tauriListen<OfficeEvent>('office:event', handler),

  onOfficeDelegation: (
    handler: (payload: { officeId: string; edges: OfficeDelegationEdge[] }) => void,
  ) =>
    tauriListen<{ officeId: string; edges: OfficeDelegationEdge[] }>(
      'office:delegation',
      handler,
    ),

  // Sakana Self-Modify (AI App Control & Custom Themes)
  getAppCustomization: async (): Promise<AppCustomization | null> =>
    tauriInvoke<AppCustomization>('get_app_customization'),

  updateAppCustomization: async (
    customization: AppCustomization,
  ): Promise<AppCustomization | null> =>
    tauriInvoke<AppCustomization>('update_app_customization', { customization }),

  resetAppCustomization: async (): Promise<AppCustomization | null> =>
    tauriInvoke<AppCustomization>('reset_app_customization'),

  onCustomizationUpdated: (handler: (payload: AppCustomization) => void) =>
    tauriListen<AppCustomization>('sakana:customization-updated', handler),

  onAutomationsUpdated: (handler: (payload: AutomationItem[]) => void) =>
    tauriListen<AutomationItem[]>('sakana:automations-updated', handler),

  onActionsApplied: (handler: (payload: string[]) => void) =>
    tauriListen<string[]>('sakana:actions-applied', handler),

  onNotify: (handler: (payload: { title: string; body: string }) => void) =>
    tauriListen<{ title: string; body: string }>('sakana:notify', handler),

  // Split-Mode Native Peer Bridge
  onPeerMessage: (handler: (payload: PeerMessageEvent) => void) =>
    tauriListen<PeerMessageEvent>('sakana:peer-message', handler),

  // Cross-Device Sync (Chat Sessions & Workspace Files)
  getSyncConfig: async (): Promise<SyncConfig | null> =>
    tauriInvoke<SyncConfig>('get_sync_config'),

  updateSyncConfig: async (config: SyncConfig): Promise<SyncConfig | null> =>
    tauriInvoke<SyncConfig>('update_sync_config', { config }),

  getSyncStatus: async (): Promise<SyncStatus | null> =>
    tauriInvoke<SyncStatus>('get_sync_status'),

  syncNow: async (): Promise<SyncStatus | null> =>
    tauriInvoke<SyncStatus>('sync_now'),

  onSyncStatus: (handler: (payload: SyncStatus) => void) =>
    tauriListen<SyncStatus>('sakana:sync-status', handler),

  onSyncUpdated: (
    handler: (payload: { newMessages: number; filesPulled: number; filesPushed: number }) => void,
  ) =>
    tauriListen<{ newMessages: number; filesPulled: number; filesPushed: number }>(
      'sakana:sync-updated',
      handler,
    ),
};
