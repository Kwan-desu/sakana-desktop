import { useEffect, useRef, useState } from 'react';
import {
  ArrowClockwise,
  ArrowLeft,
  CaretDown,
  CaretRight,
  Check,
  Copy,
  DownloadSimple,
  ArrowSquareOut,
  Eye,
  EyeSlash,
  GithubLogo,
  Key,
  Palette,
  PencilSimple,
  SignOut,
  Play,
  Plugs,
  Plus,
  QrCode,
  ShieldCheck,
  SidebarSimple,
  Stop,
  Trash,
  X,
} from '@phosphor-icons/react';
import type {
  AppCustomization,
  AutomationItem,
  DashboardSubtab,
  DiscordSettings,
  DiscordStatus,
  DiscordUserRule,
  EnvSecret,
  GitHubAuthStatus,
  HarnessAuthAccount,
  HarnessId,
  HarnessLoginResponse,
  HarnessModelOption,
  McpServer,
  Project,
  QuotaAccount,
  RemoteHost,
  RemoteStatus,
  SkillItem,
  SyncConfig,
  SyncStatus,
  SystemTelemetry,
  ThemeId,
  Thread,
  UpdateCheckResult,
} from '../types';
import { DiscordIcon, HARNESS_META, HarnessIcon, THEME_META } from './HarnessBadge';
import { isTauriRuntime, tauriApi } from '../lib/tauriApi';

interface DashboardViewProps {
  subtab: DashboardSubtab;
  theme: ThemeId;
  onSetTheme: (t: ThemeId) => void;
  leftSidebarOpen: boolean;
  onToggleLeftSidebar: () => void;
  onReturnToChat: () => void;
  threads: Thread[];
  onSelectThread: (id: string) => void;
  telemetry: SystemTelemetry | null;
  onRefreshTelemetry: () => void;
  onToggleAutostart: (enabled: boolean) => Promise<void> | void;
  quotas: QuotaAccount[];
  onRefreshQuotas: () => void;
  mcpServers: McpServer[];
  onRefreshMcp: () => void;
  onAddMcp: (name: string, transport: 'stdio' | 'sse', endpoint: string) => Promise<void> | void;
  onTestMcp: (id: string) => Promise<void> | void;
  onToggleMcp: (id: string, enabled: boolean) => Promise<void> | void;
  onDeleteMcp: (id: string) => Promise<void> | void;
  onImportAgyMcp: () => Promise<void> | void;
  skills: SkillItem[];
  onRefreshSkills: () => void;
  automations: AutomationItem[];
  onRefreshAutomations: () => void;
  onCreateAutomation: (
    name: string,
    kind: 'cron' | 'webhook',
    schedule: string,
    prompt: string,
  ) => Promise<void> | void;
  onToggleAutomation: (id: string, enabled: boolean) => Promise<void> | void;
  onDeleteAutomation: (id: string) => Promise<void> | void;
  onRunAutomation: (id: string) => Promise<void> | void;
  discordStatus: DiscordStatus | null;
  onStartDiscordBot: (token?: string) => Promise<void> | void;
  onStopDiscordBot: () => Promise<void> | void;
  discordUsers: DiscordUserRule[];
  onUpsertDiscordUser: (
    userId: string,
    label: string,
    role: DiscordUserRule['role'],
  ) => Promise<void> | void;
  onDeleteDiscordUser: (id: string) => Promise<void> | void;
  remoteStatus: RemoteStatus | null;
  onStartRemoteServer: (port?: number, lanEnabled?: boolean) => Promise<void> | void;
  onStopRemoteServer: () => Promise<void> | void;
  remoteHosts: RemoteHost[];
  activeHostId: string;
  onSwitchHost: (hostId: string) => void;
  onRefreshRemoteHosts: () => Promise<void> | void;
  secrets: EnvSecret[];
  onSetSecret: (key: string, value: string) => Promise<void> | void;
  onDeleteSecret: (key: string) => Promise<void> | void;
  onToast: (msg: string) => void;
  disabledHarnesses: HarnessId[];
  onToggleHarness: (hid: HarnessId) => void;
  harnessModels?: Record<string, HarnessModelOption[]>;
  onRefreshModels: () => Promise<void> | void;
  refreshingModels?: boolean;
  onOpenGitHubClone?: () => void;
  projects?: Project[];
  onSetProjectDiscordChannel?: (projectId: string, channel?: string) => Promise<void> | void;
  onSetThreadDiscordChannel?: (threadId: string, channel?: string | null) => Promise<void> | void;
  customization?: AppCustomization | null;
  onUpdateCustomization?: (next: AppCustomization) => Promise<void> | void;
  onResetCustomization?: () => Promise<void> | void;
  syncConfig?: SyncConfig | null;
  syncStatus?: SyncStatus | null;
  onUpdateSyncConfig?: (next: SyncConfig) => Promise<void> | void;
  onSyncNow?: () => Promise<void> | void;
}

function formatUptime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return '—';
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `${days}d ${String(hours).padStart(2, '0')}h`;
  if (hours > 0) return `${hours}h ${String(mins).padStart(2, '0')}m`;
  return `${mins}m`;
}

export function DashboardView({
  subtab,
  theme,
  onSetTheme,
  leftSidebarOpen,
  onToggleLeftSidebar,
  onReturnToChat,
  threads,
  onSelectThread,
  telemetry,
  onRefreshTelemetry,
  onToggleAutostart,
  quotas,
  onRefreshQuotas,
  mcpServers,
  onRefreshMcp,
  onAddMcp,
  onTestMcp,
  onToggleMcp,
  onDeleteMcp,
  onImportAgyMcp,
  skills,
  onRefreshSkills,
  automations,
  onRefreshAutomations,
  onCreateAutomation,
  onToggleAutomation,
  onDeleteAutomation,
  onRunAutomation,
  discordStatus,
  onStartDiscordBot,
  onStopDiscordBot,
  discordUsers,
  onUpsertDiscordUser,
  onDeleteDiscordUser,
  remoteStatus,
  onStartRemoteServer,
  onStopRemoteServer,
  remoteHosts,
  activeHostId,
  onSwitchHost,
  onRefreshRemoteHosts,
  secrets,
  onSetSecret,
  onDeleteSecret,
  onToast,
  disabledHarnesses,
  onToggleHarness,
  harnessModels = {},
  onRefreshModels,
  refreshingModels = false,
  onOpenGitHubClone,
  projects = [],
  onSetProjectDiscordChannel,
  onSetThreadDiscordChannel,
  customization,
  onUpdateCustomization,
  onResetCustomization,
  syncConfig,
  syncStatus,
  onUpdateSyncConfig,
  onSyncNow,
}: DashboardViewProps) {
  const [ghStatus, setGhStatus] = useState<GitHubAuthStatus | null>(null);
  const [ghTokenInput, setGhTokenInput] = useState('');
  const [ghBusy, setGhBusy] = useState(false);
  const [ghError, setGhError] = useState('');

  const refreshGitHubStatus = async () => {
    if (!isTauriRuntime()) return;
    try {
      const st = await tauriApi.githubAuthStatus();
      setGhStatus(st);
    } catch {
      // ignore
    }
  };

  const [discordSettings, setDiscordSettings] = useState<DiscordSettings>({
    defaultHarness: 'auto',
    defaultModel: '',
    defaultEffort: 'medium',
    reactWithEmojis: true,
    autoStart: true,
  });
  const [savingDcSettings, setSavingDcSettings] = useState(false);

  useEffect(() => {
    if (subtab === 'remote') {
      void refreshGitHubStatus();
    }
    if (subtab === 'discord' && isTauriRuntime()) {
      void tauriApi
        .getDiscordSettings()
        .then((cfg) => {
          if (cfg) setDiscordSettings(cfg);
        })
        .catch(() => {});
    }
  }, [subtab]);
  const [openAutoHistory, setOpenAutoHistory] = useState<Record<string, boolean>>({});
  const [revealedSecrets, setRevealedSecrets] = useState<Record<string, string>>({});
  const [projChannelDrafts, setProjChannelDrafts] = useState<Record<string, string>>({});
  const [hostName, setHostName] = useState('');
  const [hostUrl, setHostUrl] = useState('');
  const [hostToken, setHostToken] = useState('');
  const [pairingInput, setPairingInput] = useState('');
  const [testingHostId, setTestingHostId] = useState<string | null>(null);


  // Desktop App Updater state
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [updateResult, setUpdateResult] = useState<UpdateCheckResult | null>(null);

  // MCP Add form
  const [addingMcp, setAddingMcp] = useState(false);
  const [mcpName, setMcpName] = useState('');
  const [mcpTransport, setMcpTransport] = useState<'stdio' | 'sse'>('stdio');
  const [mcpEndpoint, setMcpEndpoint] = useState('');

  // Automation Add form
  const [addingAuto, setAddingAuto] = useState(false);
  const [autoName, setAutoName] = useState('');
  const [autoKind, setAutoKind] = useState<'cron' | 'webhook'>('cron');
  const [autoSchedule, setAutoSchedule] = useState('0 * * * *');
  const [autoPrompt, setAutoPrompt] = useState('');

  // Discord Bot & RBAC form
  const [dcToken, setDcToken] = useState('');
  const [dcClientId, setDcClientId] = useState('');
  const [newDcId, setNewDcId] = useState('');
  const [newDcLabel, setNewDcLabel] = useState('');
  const [newDcRole, setNewDcRole] = useState<DiscordUserRule['role']>('friend');

  // Remote & Secrets form
  const [remotePort, setRemotePort] = useState('4317');
  const [lanEnabled, setLanEnabled] = useState(false);
  const [secretKey, setSecretKey] = useState('');
  const [secretValue, setSecretValue] = useState('');

  // Harness Auth (AGY, Kiro, Claude)
  const [selectedHarness, setSelectedHarness] = useState<'agy-cli' | 'kiro-cli' | 'claude-code'>('agy-cli');
  const [authAccounts, setAuthAccounts] = useState<HarnessAuthAccount[]>([]);
  const [renamingAccId, setRenamingAccId] = useState<string | null>(null);
  const [renameAccDraft, setRenameAccDraft] = useState('');
  const [savingAccRename, setSavingAccRename] = useState(false);

  const commitRenameAccount = async (accountId: string) => {
    const trimmed = renameAccDraft.trim();
    if (!trimmed) {
      setRenamingAccId(null);
      return;
    }
    setSavingAccRename(true);
    try {
      const next = await tauriApi.renameHarnessAccount(accountId, trimmed);
      setAuthAccounts(next);
      setRenamingAccId(null);
      onToast(`Renamed account to "${trimmed}"`);
    } catch (err) {
      onToast(`Rename failed: ${String(err)}`);
    } finally {
      setSavingAccRename(false);
    }
  };
  const [authLoading, setAuthLoading] = useState(false);
  const [addingAccount, setAddingAccount] = useState(false);
  const [creatingAccount, setCreatingAccount] = useState(false);
  const [newAccLabel, setNewAccLabel] = useState('');
  const [newAccEmail, setNewAccEmail] = useState('');
  const [newAccAuthType, setNewAccAuthType] = useState('');
  const [loginBusyId, setLoginBusyId] = useState<string | null>(null);
  const [loginSessions, setLoginSessions] = useState<Record<string, HarnessLoginResponse>>({});
  const [loginCodes, setLoginCodes] = useState<Record<string, string>>({});
  const [authTypeOverrides, setAuthTypeOverrides] = useState<Record<string, string>>({});
  const [agyLoginMode, setAgyLoginMode] = useState<'headless' | 'loopback'>('headless');
  const [expandedLogs, setExpandedLogs] = useState<Record<string, boolean>>({});
  const [changeAccountOpen, setChangeAccountOpen] = useState(false);
  const accountMenuRef = useRef<HTMLDivElement | null>(null);

  const toggleProviderEnabled = (hid: 'agy-cli' | 'claude-code' | 'kiro-cli') => {
    onToggleHarness(hid);
  };

  useEffect(() => {
    if (!changeAccountOpen && !addingAccount) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setChangeAccountOpen(false);
        setAddingAccount(false);
      }
    };
    const onMouseDown = (e: MouseEvent) => {
      if (
        changeAccountOpen &&
        accountMenuRef.current &&
        !accountMenuRef.current.contains(e.target as Node)
      ) {
        setChangeAccountOpen(false);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    document.addEventListener('mousedown', onMouseDown);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('mousedown', onMouseDown);
    };
  }, [changeAccountOpen, addingAccount]);

  const refreshAuthAccounts = async () => {
    if (!isTauriRuntime()) return;
    setAuthLoading(true);
    try {
      const list = await tauriApi.listHarnessAccounts();
      setAuthAccounts(list);
    } catch (err) {
      onToast(`Failed to load harness accounts: ${String(err)}`);
    } finally {
      setAuthLoading(false);
    }
  };

  useEffect(() => {
    if (subtab === 'harnesses') {
      void refreshAuthAccounts();
    }
  }, [subtab]);

  const handleStartHarnessLogin = async (acc: HarnessAuthAccount) => {
    if (!isTauriRuntime()) {
      onToast('CLI authentication requires the Sakana desktop runtime.');
      return;
    }
    setLoginBusyId(acc.id);
    try {
      const chosenAuthType = authTypeOverrides[acc.id] || acc.authType;
      const res = await tauriApi.startHarnessLogin(
        acc.id,
        chosenAuthType,
        acc.harness === 'agy-cli' ? agyLoginMode : undefined,
      );
      if (res) {
        setLoginSessions((prev) => ({ ...prev, [acc.id]: res }));
        setExpandedLogs((prev) => {
          const copy = { ...prev };
          delete copy[acc.id];
          return copy;
        });
        if (res.authUrl) {
          void tauriApi.openExternalUrl(res.authUrl).catch(() => {});
        }
        onToast(res.authenticated ? `${acc.label} authenticated` : res.message.slice(0, 140));
      }
      await refreshAuthAccounts();
    } catch (err) {
      onToast(`Login failed: ${String(err)}`);
    } finally {
      setLoginBusyId(null);
    }
  };

  const handleSubmitLoginCode = async (accountId: string) => {
    const code = (loginCodes[accountId] || '').trim();
    if (!code) return;
    setLoginBusyId(accountId);
    try {
      const res = await tauriApi.submitHarnessLoginCode(accountId, code);
      if (res) {
        setLoginSessions((prev) => ({ ...prev, [accountId]: res }));
        setExpandedLogs((prev) => {
          const copy = { ...prev };
          delete copy[accountId];
          return copy;
        });
        if (res.authenticated) {
          setLoginCodes((prev) => ({ ...prev, [accountId]: '' }));
        }
        onToast(res.message.slice(0, 140));
      }
      await refreshAuthAccounts();
    } catch (err) {
      onToast(`Submit code failed: ${String(err)}`);
    } finally {
      setLoginBusyId(null);
    }
  };

  const handleCheckHarnessAuth = async (accountId: string) => {
    setLoginBusyId(accountId);
    try {
      const updated = await tauriApi.checkHarnessAuth(accountId);
      if (updated) {
        setAuthAccounts((prev) => prev.map((a) => (a.id === updated.id ? updated : a)));
        onRefreshQuotas?.();
        void onRefreshModels();
        onToast(
          updated.authenticated
            ? `${updated.label} is signed in${updated.email ? ` (${updated.email})` : ''}`
            : `${updated.label} is not signed in yet`,
        );
      }
    } catch (err) {
      onToast(`Status check failed: ${String(err)}`);
    } finally {
      setLoginBusyId(null);
    }
  };

  const handleLogoutHarness = async (acc: HarnessAuthAccount) => {
    setLoginBusyId(acc.id);
    try {
      const next = await tauriApi.logoutHarnessAccount(acc.id);
      setAuthAccounts(next);
      onRefreshQuotas?.();
      void onRefreshModels();
      setLoginSessions((prev) => {
        const copy = { ...prev };
        delete copy[acc.id];
        return copy;
      });
      onToast(`Signed out ${acc.label}`);
    } catch (err) {
      onToast(`Sign out failed: ${String(err)}`);
    } finally {
      setLoginBusyId(null);
    }
  };


  const handleCheckForUpdates = async () => {
    setCheckingUpdate(true);
    try {
      const res = await tauriApi.checkForUpdates();
      setUpdateResult(res);
      if (res.error) {
        onToast(`Update check: ${res.error}`);
      } else if (res.available) {
        onToast(`Update available: v${res.latestVersion ?? 'new'}`);
      } else {
        onToast(`Sakana v${res.currentVersion} is up to date`);
      }
    } catch (err) {
      const msg = String(err);
      setUpdateResult({ available: false, currentVersion: '0.1.0', error: msg });
      onToast(`Update check failed: ${msg}`);
    } finally {
      setCheckingUpdate(false);
    }
  };

  const handleInstallUpdate = async () => {
    setInstallingUpdate(true);
    try {
      await tauriApi.installUpdate();
      onToast('Installing update and restarting...');
    } catch (err) {
      onToast(`Install update failed: ${String(err)}`);
      setInstallingUpdate(false);
    }
  };

  const handleToggleRevealSecret = async (sec: EnvSecret) => {
    if (revealedSecrets[sec.key] !== undefined) {
      setRevealedSecrets((prev) => {
        const next = { ...prev };
        delete next[sec.key];
        return next;
      });
      return;
    }
    if (isTauriRuntime()) {
      try {
        const raw = await tauriApi.revealEnvSecret(sec.key);
        setRevealedSecrets((prev) => ({ ...prev, [sec.key]: raw || sec.value }));
      } catch (err) {
        onToast(`Failed to reveal ${sec.key}: ${String(err)}`);
      }
    } else {
      setRevealedSecrets((prev) => ({ ...prev, [sec.key]: sec.value }));
    }
  };

  const discordThreads = threads.filter((t) => t.isDiscord);
  const boundProjectsCount = projects.filter((p) => Boolean(p.discordChannel)).length;
  const autostartEnabled = telemetry?.autostartEnabled ?? false;

  const safeNum = (v: number | undefined | null): number =>
    typeof v === 'number' && Number.isFinite(v) ? v : 0;

  const cpuPct = telemetry ? Math.round(safeNum(telemetry.cpuUsagePct)) : 0;
  const memUsedGb = telemetry ? safeNum(telemetry.memoryUsedBytes) / 1024 ** 3 : 0;
  const memTotalGb = telemetry ? safeNum(telemetry.memoryTotalBytes) / 1024 ** 3 : 0;
  const memPct = memTotalGb > 0 ? Math.round((memUsedGb / memTotalGb) * 100) : 0;
  const memUsed = telemetry ? memUsedGb.toFixed(1) : '—';
  const memTotal = telemetry ? memTotalGb.toFixed(0) : '—';
  const diskUsedGb = telemetry ? safeNum(telemetry.diskUsedBytes) / 1024 ** 3 : 0;
  const diskTotalGb = telemetry ? safeNum(telemetry.diskTotalBytes) / 1024 ** 3 : 0;
  const diskPct = diskTotalGb > 0 ? Math.round((diskUsedGb / diskTotalGb) * 100) : 0;
  const diskUsed = telemetry ? diskUsedGb.toFixed(0) : '—';
  const diskTotal = telemetry ? diskTotalGb.toFixed(0) : '—';

  return (
    <div
      className="flex-1 min-w-0 h-full overflow-y-auto px-6 sm:px-10 py-7 space-y-6"
      style={{ backgroundColor: 'var(--bg-canvas)' }}
    >
      {/* Top Bar */}
      <div className="max-w-5xl mx-auto flex flex-wrap items-center justify-between gap-4 border-b pb-4" style={{ borderColor: 'var(--border-line)' }}>
        <div className="flex items-center gap-3">
          {!leftSidebarOpen && (
            <button
              type="button"
              onClick={onToggleLeftSidebar}
              className="btn-ghost p-1.5 cursor-pointer"
              title="Show Sidebar (Ctrl+B)"
              aria-label="Show Sidebar"
            >
              <SidebarSimple size={14} />
            </button>
          )}
          <div>
            <div className="text-[11.5px] font-medium" style={{ color: 'var(--text-muted)' }}>
              Sakana Operations
            </div>
            <h1 className="text-[24px] font-semibold tracking-tight m-0" style={{ color: 'var(--text-main)' }}>
              {subtab === 'overview' && 'Overview & Live Telemetry'}
              {subtab === 'harnesses' && 'Harnesses & Multi-Account Quotas'}
              {subtab === 'mcp' && 'MCP Hub & Skills'}
              {subtab === 'automations' && 'Cron & Webhook Automations'}
              {subtab === 'discord' && 'Discord Bot & RBAC Sandbox'}
              {subtab === 'remote' && 'Preferences & System'}
            </h1>
          </div>
        </div>

        <button
          type="button"
          onClick={onReturnToChat}
          className="btn-solid px-3.5 py-2 text-[12px] font-medium inline-flex items-center gap-1.5 cursor-pointer"
        >
          <ArrowLeft size={13} />
          <span>Return to Workspace</span>
        </button>
      </div>

      <div className="max-w-5xl mx-auto space-y-6">
        {/* 1. OVERVIEW */}
        {subtab === 'overview' && (
          <>
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
              {[
                { label: 'Active Worktrees', val: String(threads.length), sub: 'Isolated checkouts' },
                { label: 'Discord Chats', val: String(discordThreads.length), sub: `${boundProjectsCount} bound projects` },
                { label: 'MCP Servers', val: String(mcpServers.filter((m) => m.enabled).length), sub: `${mcpServers.length} configured` },
                {
                  label: 'Host Uptime',
                  val: telemetry ? formatUptime(safeNum(telemetry.uptimeSecs)) : '—',
                  sub: telemetry?.hostName || telemetry?.osName || 'Waiting for telemetry',
                },
              ].map((kpi) => (
                <div key={kpi.label} className="sakana-card rounded-[12px] p-4 space-y-1.5">
                  <div className="text-[12px]" style={{ color: 'var(--text-muted)' }}>
                    {kpi.label}
                  </div>
                  <div className="text-[28px] font-semibold tabular-nums leading-none" style={{ color: 'var(--text-main)' }}>
                    {kpi.val}
                  </div>
                  <div className="font-mono-tech text-[11px] truncate" style={{ color: 'var(--text-muted)' }}>
                    {kpi.sub}
                  </div>
                </div>
              ))}
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-12 gap-5">
              <div className="lg:col-span-7 sakana-card rounded-[12px] p-5 space-y-3.5">
                <div className="flex flex-wrap items-center justify-between gap-2 border-b pb-3" style={{ borderColor: 'var(--border-line)' }}>
                  <div>
                    <h2 className="text-[14.5px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                      Active &amp; Discord Threads
                    </h2>
                    <p className="text-[12px] m-0" style={{ color: 'var(--text-muted)' }}>
                      All agents run autonomously in YOLO mode
                    </p>
                  </div>
                  <span
                    className="font-mono-tech text-[10.5px] px-2 py-0.5 rounded-[5px]"
                    style={{ backgroundColor: 'var(--accent-green-bg)', color: 'var(--accent-green)' }}
                  >
                    YOLO Always On
                  </span>
                </div>

                {threads.length === 0 ? (
                  <div className="py-6 text-center text-[12.5px]" style={{ color: 'var(--text-muted)' }}>
                    No active threads yet.
                  </div>
                ) : (
                  <div className="space-y-2">
                    {threads.slice(0, 6).map((t) => (
                      <div
                        key={t.id}
                        className="rounded-[9px] border px-3.5 py-2.5 flex items-center justify-between gap-2"
                        style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
                      >
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-1.5 text-[12.5px] font-medium truncate" style={{ color: 'var(--text-main)' }}>
                            {t.isDiscord && <DiscordIcon size={12} style={{ color: 'var(--accent-discord)' }} />}
                            <span className="truncate">{t.title}</span>
                          </div>
                          <div className="font-mono-tech text-[10.5px] truncate" style={{ color: 'var(--text-muted)' }}>
                            {t.harness} · {t.discordChannel || t.branch || 'main'} · {t.status}
                          </div>
                        </div>
                        <button
                          type="button"
                          onClick={() => onSelectThread(t.id)}
                          className="btn-ghost px-2.5 py-1 text-[11px] cursor-pointer shrink-0"
                        >
                          Open
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <div className="lg:col-span-5 sakana-card rounded-[12px] p-5 space-y-4">
                <div className="flex items-center justify-between border-b pb-3" style={{ borderColor: 'var(--border-line)' }}>
                  <div>
                    <h2 className="text-[14.5px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                      Host Telemetry
                    </h2>
                    <p className="font-mono-tech text-[11px] m-0" style={{ color: 'var(--text-muted)' }}>
                      {telemetry ? `${telemetry.osName || 'Host'} · ${safeNum(telemetry.cpuCores)} vCPU` : 'System Monitor'}
                    </p>
                  </div>
                  <div className="flex items-center gap-1.5">
                    <button
                      type="button"
                      onClick={onRefreshTelemetry}
                      className="btn-ghost p-1 cursor-pointer"
                      title="Refresh telemetry"
                      aria-label="Refresh telemetry"
                    >
                      <ArrowClockwise size={13} />
                    </button>
                    <span
                      className="font-mono-tech text-[10.5px] px-2 py-0.5 rounded-[5px]"
                      style={
                        telemetry
                          ? { backgroundColor: 'var(--accent-green-bg)', color: 'var(--accent-green)' }
                          : { backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }
                      }
                    >
                      {telemetry ? 'Online' : 'Offline'}
                    </span>
                  </div>
                </div>

                {[
                  {
                    label: 'CPU',
                    val: telemetry ? `${cpuPct}%` : '—',
                    pct: cpuPct,
                    color: 'var(--accent-blue)',
                  },
                  {
                    label: 'Memory',
                    val: telemetry ? `${memUsed} / ${memTotal} GB` : '—',
                    pct: memPct,
                    color: 'var(--accent-green)',
                  },
                  {
                    label: 'Disk',
                    val: telemetry ? `${diskUsed} / ${diskTotal} GB` : '—',
                    pct: diskPct,
                    color: 'var(--accent-yellow)',
                  },
                ].map((bar) => (
                  <div key={bar.label} className="space-y-1">
                    <div className="flex justify-between text-[12px]">
                      <span style={{ color: 'var(--text-body)' }}>{bar.label}</span>
                      <span className="font-mono-tech" style={{ color: 'var(--text-main)' }}>
                        {bar.val}
                      </span>
                    </div>
                    <div className="h-1.5 rounded-full overflow-hidden" style={{ backgroundColor: 'var(--bg-elevated)' }}>
                      <div
                        className="h-full rounded-full transition-all"
                        style={{ width: `${Math.min(100, Math.max(0, bar.pct))}%`, backgroundColor: bar.color }}
                      />
                    </div>
                  </div>
                ))}

                <div className="pt-2 border-t space-y-2" style={{ borderColor: 'var(--border-line)' }}>
                  <div className="flex items-center justify-between gap-2">
                    <div>
                      <div className="text-[12.5px] font-medium" style={{ color: 'var(--text-main)' }}>
                        Desktop App Updates
                      </div>
                      <div className="font-mono-tech text-[10.5px]" style={{ color: 'var(--text-muted)' }}>
                        {updateResult
                          ? updateResult.error
                            ? `v${updateResult.currentVersion} · ${updateResult.error}`
                            : updateResult.available
                              ? `v${updateResult.currentVersion} → v${updateResult.latestVersion ?? 'latest'}`
                              : `v${updateResult.currentVersion} · Up to date`
                          : 'Check for signed desktop releases'}
                      </div>
                    </div>
                    <div className="flex items-center gap-1.5 shrink-0">
                      {updateResult?.available && (
                        <button
                          type="button"
                          disabled={installingUpdate}
                          onClick={() => void handleInstallUpdate()}
                          className="btn-solid px-2.5 py-1 text-[11px] cursor-pointer"
                        >
                          {installingUpdate ? 'Installing...' : 'Install & Restart'}
                        </button>
                      )}
                      <button
                        type="button"
                        disabled={checkingUpdate}
                        onClick={() => void handleCheckForUpdates()}
                        className="btn-ghost px-2.5 py-1 text-[11px] cursor-pointer"
                      >
                        {checkingUpdate ? 'Checking...' : 'Check Updates'}
                      </button>
                    </div>
                  </div>
                  {updateResult?.available && updateResult.body && (
                    <div
                      className="font-mono-tech text-[10.5px] rounded-[6px] border px-2.5 py-1.5"
                      style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)', color: 'var(--text-body)' }}
                    >
                      {updateResult.body}
                    </div>
                  )}
                </div>
              </div>
            </div>
          </>
        )}

        {/* 2. HARNESSES & QUOTAS (Providers Master-Detail Layout) */}
        {subtab === 'harnesses' && (
          <div className="space-y-6">
            <div className="sakana-card rounded-[12px] flex flex-col lg:flex-row">
              {/* Left Providers List */}
              <div
                className="w-full lg:w-64 shrink-0 border-b lg:border-b-0 lg:border-r p-3 space-y-2 rounded-t-[12px] lg:rounded-l-[12px] lg:rounded-tr-none"
                style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-sidebar)' }}
              >
                <div className="flex items-center justify-between px-1.5 py-1">
                  <span className="text-[12.5px] font-semibold" style={{ color: 'var(--text-main)' }}>
                    Providers
                  </span>
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      onClick={() => void refreshAuthAccounts()}
                      disabled={authLoading}
                      className="w-6 h-6 rounded-[6px] flex items-center justify-center hover:bg-[var(--bg-hover)] cursor-pointer"
                      style={{ color: 'var(--text-muted)' }}
                      title="Refresh provider status"
                      aria-label="Refresh provider status"
                    >
                      <ArrowClockwise size={12} />
                    </button>
                    <button
                      type="button"
                      onClick={() => setAddingAccount((v) => !v)}
                      className="w-6 h-6 rounded-[6px] flex items-center justify-center hover:bg-[var(--bg-hover)] cursor-pointer"
                      style={{ color: 'var(--text-muted)' }}
                      title={`Add ${HARNESS_META[selectedHarness].name} Account`}
                      aria-label={`Add ${HARNESS_META[selectedHarness].name} Account`}
                    >
                      <Plus size={13} />
                    </button>
                  </div>
                </div>

                <div className="space-y-1">
                  {(['agy-cli', 'claude-code', 'kiro-cli'] as const).map((hid) => {
                    const meta = HARNESS_META[hid];
                    const list = authAccounts.filter((a) => a.harness === hid);
                    const defaultAcc = list.find((a) => a.isDefault) || list[0];
                    const signedIn = list.some((a) => a.authenticated);
                    const installed = defaultAcc?.cliInstalled ?? false;
                    const active = selectedHarness === hid;
                    const enabled = !disabledHarnesses.includes(hid);
                    return (
                      <div
                        key={hid}
                        role="button"
                        tabIndex={0}
                        onClick={() => {
                          setSelectedHarness(hid);
                          setNewAccAuthType('');
                          setChangeAccountOpen(false);
                        }}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter' || e.key === ' ') {
                            e.preventDefault();
                            setSelectedHarness(hid);
                            setNewAccAuthType('');
                            setChangeAccountOpen(false);
                          }
                        }}
                        className="w-full rounded-[8px] px-2.5 py-2 text-left transition-colors cursor-pointer flex items-center justify-between gap-2 border"
                        style={{
                          borderColor: active ? 'var(--border-strong)' : 'transparent',
                          backgroundColor: active ? 'var(--bg-elevated)' : 'transparent',
                          opacity: enabled ? 1 : 0.65,
                        }}
                      >
                        <div className="min-w-0 space-y-0.5">
                          <div className="inline-flex items-center gap-2 text-[12.5px] font-medium" style={{ color: 'var(--text-main)' }}>
                            <HarnessIcon harness={hid} size={14} />
                            <span className="truncate">{meta.name}</span>
                          </div>
                          <div className="flex items-center gap-1.5 text-[11px] truncate" style={{ color: 'var(--text-muted)' }}>
                            <span
                              className="w-1.5 h-1.5 rounded-full shrink-0"
                              style={{
                                backgroundColor: signedIn
                                  ? 'var(--accent-green)'
                                  : installed
                                    ? 'var(--accent-yellow)'
                                    : 'var(--accent-red)',
                              }}
                            />
                            <span className="truncate">
                              {signedIn
                                ? `Authenticated · ${defaultAcc?.label || 'Signed in'}`
                                : installed
                                  ? 'Not signed in'
                                  : 'Not found · CLI missing'}
                            </span>
                          </div>
                        </div>
                        <button
                          type="button"
                          role="switch"
                          aria-checked={enabled}
                          aria-label={`Toggle ${meta.name}`}
                          onClick={(e) => {
                            e.stopPropagation();
                            toggleProviderEnabled(hid);
                          }}
                          className="w-7 h-4 rounded-full p-0.5 flex items-center shrink-0 transition-colors cursor-pointer"
                          style={{
                            backgroundColor: enabled ? 'var(--accent-blue)' : 'var(--bg-card)',
                            border: '1px solid var(--border-line)',
                            justifyContent: enabled ? 'flex-end' : 'flex-start',
                          }}
                          title={enabled ? `Disable ${meta.name}` : `Enable ${meta.name}`}
                        >
                          <span className="w-2.5 h-2.5 rounded-full bg-white" />
                        </button>
                      </div>
                    );
                  })}
                </div>
              </div>

              {/* Right Provider Detail Panel */}
              <div
                className="flex-1 min-w-0 p-5 space-y-5 rounded-b-[12px] lg:rounded-r-[12px] lg:rounded-bl-none"
                style={{ backgroundColor: 'var(--bg-card)' }}
              >
                {(() => {
                  const hid = selectedHarness;
                  const meta = HARNESS_META[hid];
                  const accounts = authAccounts.filter((a) => a.harness === hid);
                  const activeAcc = accounts.find((a) => a.isDefault) || accounts[0];
                  const installed = activeAcc?.cliInstalled ?? false;
                  const cliBin = activeAcc?.cliBin ?? '';
                  const busy = activeAcc ? loginBusyId === activeAcc.id : false;
                  const session = activeAcc ? loginSessions[activeAcc.id] : undefined;
                  const selectedAuthType = activeAcc
                    ? authTypeOverrides[activeAcc.id] || activeAcc.authType
                    : '';
                  const isLoginError = Boolean(
                    session && session.started && !session.authenticated && !session.authUrl && session.message,
                  );
                  const logOpen = activeAcc
                    ? (expandedLogs[activeAcc.id] ?? isLoginError)
                    : false;

                  return (
                    <>
                      {/* Provider Header */}
                      <div className="flex items-center justify-between gap-2 pb-3 border-b" style={{ borderColor: 'var(--border-line)' }}>
                        <div className="inline-flex items-center gap-2.5">
                          <HarnessIcon harness={hid} size={18} />
                          <span className="text-[15px] font-semibold" style={{ color: 'var(--text-main)' }}>
                            {meta.name}
                          </span>
                        </div>
                        <div className="flex items-center gap-1.5">
                          <button
                            type="button"
                            onClick={() => void onRefreshModels()}
                            disabled={refreshingModels}
                            className="btn-ghost px-2.5 py-1 text-[11px] inline-flex items-center gap-1.5 cursor-pointer"
                            title="Refresh live model catalog"
                          >
                            <ArrowClockwise size={11} />
                            <span>{refreshingModels ? 'Refreshing...' : 'Refresh Models'}</span>
                          </button>
                          <button
                            type="button"
                            onClick={() => void refreshAuthAccounts()}
                            disabled={authLoading}
                            className="btn-ghost px-2.5 py-1 text-[11px] inline-flex items-center gap-1.5 cursor-pointer"
                          >
                            <ArrowClockwise size={11} />
                            <span>{authLoading ? 'Checking...' : 'Check Status'}</span>
                          </button>
                        </div>
                      </div>

                      {/* Status */}
                      <div className="space-y-0.5">
                        <div className="text-[12.5px] font-medium" style={{ color: 'var(--text-main)' }}>
                          Status
                        </div>
                        <div className="text-[11.5px]" style={{ color: 'var(--text-muted)' }}>
                          {activeAcc?.authenticated
                            ? `Authenticated · ${activeAcc.email || activeAcc.label}`
                            : installed
                              ? 'CLI installed · Not signed in'
                              : 'CLI binary not detected on PATH'}
                        </div>
                      </div>

                      {/* Setup */}
                      <div className="space-y-2.5 pt-2 border-t" style={{ borderColor: 'var(--border-line)' }}>
                        <div className="text-[12.5px] font-semibold" style={{ color: 'var(--text-main)' }}>
                          Setup
                        </div>
                        <div className="flex items-center justify-between gap-4 text-[12px]">
                          <div>
                            <div style={{ color: 'var(--text-main)' }}>Environment</div>
                            <div className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
                              Device that runs this provider
                            </div>
                          </div>
                          <span className="font-mono-tech text-[11.5px]" style={{ color: 'var(--text-body)' }}>
                            {telemetry?.hostName || 'Local Machine'}
                          </span>
                        </div>
                        <div className="flex items-center justify-between gap-4 text-[12px]">
                          <div className="min-w-0">
                            <div style={{ color: 'var(--text-main)' }}>Runtime</div>
                            <div className="font-mono-tech text-[11px] truncate max-w-md" style={{ color: 'var(--text-muted)' }}>
                              {cliBin || `Install ${meta.name} CLI to enable`}
                            </div>
                          </div>
                          <span
                            className="font-mono-tech text-[11px] px-2 py-0.5 rounded-[5px] shrink-0"
                            style={{
                              backgroundColor: installed ? 'var(--accent-green-bg)' : 'var(--bg-elevated)',
                              color: installed ? 'var(--accent-green)' : 'var(--text-muted)',
                            }}
                          >
                            {installed ? 'Installed' : 'Not found'}
                          </span>
                        </div>
                      </div>

                      {/* Account */}
                      <div className="space-y-3 pt-2 border-t" style={{ borderColor: 'var(--border-line)' }}>
                        <div className="flex flex-wrap items-center justify-between gap-3">
                          <div className="min-w-0">
                            <div className="text-[12.5px] font-medium" style={{ color: 'var(--text-main)' }}>
                              Account
                            </div>
                            {activeAcc && renamingAccId === activeAcc.id && !changeAccountOpen ? (
                              <div className="flex items-center gap-1 mt-1">
                                <input
                                  type="text"
                                  autoFocus
                                  value={renameAccDraft}
                                  disabled={savingAccRename}
                                  onChange={(e) => setRenameAccDraft(e.target.value)}
                                  onKeyDown={(e) => {
                                    if (e.key === 'Enter') {
                                      e.preventDefault();
                                      void commitRenameAccount(activeAcc.id);
                                    } else if (e.key === 'Escape') {
                                      e.preventDefault();
                                      setRenamingAccId(null);
                                    }
                                  }}
                                  placeholder="Account name..."
                                  className="px-2 py-0.5 rounded-[5px] border text-[11.5px] w-44 focus:outline-none"
                                  style={{
                                    backgroundColor: 'var(--bg-card)',
                                    borderColor: 'var(--accent-blue)',
                                    color: 'var(--text-main)',
                                  }}
                                />
                                <button
                                  type="button"
                                  disabled={savingAccRename}
                                  onClick={() => void commitRenameAccount(activeAcc.id)}
                                  className="p-1 rounded hover:bg-[var(--bg-hover)] cursor-pointer"
                                  style={{ color: 'var(--accent-green)' }}
                                  title="Save name"
                                >
                                  <Check size={12} weight="bold" />
                                </button>
                                <button
                                  type="button"
                                  disabled={savingAccRename}
                                  onClick={() => setRenamingAccId(null)}
                                  className="p-1 rounded hover:bg-[var(--bg-hover)] cursor-pointer"
                                  style={{ color: 'var(--text-muted)' }}
                                  title="Cancel"
                                >
                                  <X size={12} />
                                </button>
                              </div>
                            ) : (
                              <div className="flex items-center gap-1.5 text-[11.5px]" style={{ color: 'var(--text-muted)' }}>
                                <span>
                                  {activeAcc
                                    ? activeAcc.authenticated
                                      ? `Signed in · ${activeAcc.label}${activeAcc.email ? ` (${activeAcc.email})` : ''}`
                                      : `Profile "${activeAcc.label}" · Signed out`
                                    : isTauriRuntime()
                                      ? 'No account profile configured'
                                      : 'Run desktop app to manage local CLI auth'}
                                </span>
                                {activeAcc && (
                                  <button
                                    type="button"
                                    onClick={() => {
                                      setRenamingAccId(activeAcc.id);
                                      setRenameAccDraft(activeAcc.label);
                                    }}
                                    className="p-0.5 rounded hover:bg-[var(--bg-hover)] cursor-pointer"
                                    style={{ color: 'var(--text-muted)' }}
                                    title="Rename account"
                                    aria-label={`Rename account ${activeAcc.label}`}
                                  >
                                    <PencilSimple size={11} />
                                  </button>
                                )}
                              </div>
                            )}
                          </div>

                          <div ref={accountMenuRef} className="flex items-center gap-2 relative">
                            <button
                              type="button"
                              onClick={() => setChangeAccountOpen((v) => !v)}
                              className="btn-ghost px-3 py-1.5 text-[11.5px] font-medium cursor-pointer"
                            >
                              Change account
                            </button>
                            {activeAcc && (
                              activeAcc.authenticated ? (
                                <>
                                  <button
                                    type="button"
                                    disabled={busy}
                                    onClick={() => void handleStartHarnessLogin(activeAcc)}
                                    className="btn-ghost px-2.5 py-1.5 text-[11.5px] font-medium cursor-pointer"
                                  >
                                    {busy ? 'Working...' : 'Re-login'}
                                  </button>
                                  <button
                                    type="button"
                                    disabled={busy}
                                    onClick={() => void handleLogoutHarness(activeAcc)}
                                    className="px-2.5 py-1.5 text-[11.5px] font-medium cursor-pointer hover:opacity-80"
                                    style={{ color: 'var(--text-body)' }}
                                  >
                                    Sign out
                                  </button>
                                </>
                              ) : (
                                <button
                                  type="button"
                                  disabled={busy}
                                  onClick={() => void handleStartHarnessLogin(activeAcc)}
                                  className="btn-solid px-3 py-1.5 text-[11.5px] font-medium cursor-pointer"
                                >
                                  {busy ? 'Working...' : 'Sign in'}
                                </button>
                              )
                            )}

                            {changeAccountOpen && (
                              <div className="sakana-popover pop-enter absolute right-0 top-full mt-1.5 w-72 rounded-[10px] p-1.5 z-40 space-y-1">
                                <div className="px-2 py-1 text-[10.5px] font-medium" style={{ color: 'var(--text-muted)' }}>
                                  {meta.name} Accounts ({accounts.length})
                                </div>
                                {accounts.map((acc) => {
                                  const isPrimary =
                                    acc.id === 'default' || acc.id === 'kiro_default' || acc.id === 'claude_default';
                                  return (
                                    <div
                                      key={acc.id}
                                      className="px-2 py-1.5 rounded-[7px] flex items-center justify-between gap-2 hover:bg-[var(--bg-hover)]"
                                    >
                                      {renamingAccId === acc.id ? (
                                        <div className="flex items-center gap-1 flex-1 min-w-0">
                                          <input
                                            type="text"
                                            autoFocus
                                            value={renameAccDraft}
                                            disabled={savingAccRename}
                                            onChange={(e) => setRenameAccDraft(e.target.value)}
                                            onKeyDown={(e) => {
                                              if (e.key === 'Enter') {
                                                e.preventDefault();
                                                void commitRenameAccount(acc.id);
                                              } else if (e.key === 'Escape') {
                                                e.preventDefault();
                                                setRenamingAccId(null);
                                              }
                                            }}
                                            placeholder="Account name..."
                                            className="flex-1 min-w-0 px-2 py-1 rounded-[5px] border text-[11.5px] focus:outline-none"
                                            style={{
                                              backgroundColor: 'var(--bg-card)',
                                              borderColor: 'var(--accent-blue)',
                                              color: 'var(--text-main)',
                                            }}
                                          />
                                          <button
                                            type="button"
                                            disabled={savingAccRename}
                                            onClick={() => void commitRenameAccount(acc.id)}
                                            className="p-1 rounded hover:bg-[var(--bg-elevated)] cursor-pointer"
                                            style={{ color: 'var(--accent-green)' }}
                                            title="Save name"
                                          >
                                            <Check size={12} weight="bold" />
                                          </button>
                                          <button
                                            type="button"
                                            disabled={savingAccRename}
                                            onClick={() => setRenamingAccId(null)}
                                            className="p-1 rounded hover:bg-[var(--bg-elevated)] cursor-pointer"
                                            style={{ color: 'var(--text-muted)' }}
                                            title="Cancel"
                                          >
                                            <X size={12} />
                                          </button>
                                        </div>
                                      ) : (
                                        <>
                                          <button
                                            type="button"
                                            onClick={async () => {
                                              if (!acc.isDefault) {
                                                const next = await tauriApi.setDefaultHarnessAccount(acc.id);
                                                setAuthAccounts(next);
                                                onToast(`Switched ${meta.name} to ${acc.label}`);
                                                void onRefreshModels();
                                              }
                                              setChangeAccountOpen(false);
                                            }}
                                            className="min-w-0 flex-1 text-left cursor-pointer"
                                          >
                                            <div className="flex items-center gap-1.5 text-[12px] font-medium" style={{ color: 'var(--text-main)' }}>
                                              <span
                                                className="w-1.5 h-1.5 rounded-full shrink-0"
                                                style={{
                                                  backgroundColor: acc.authenticated ? 'var(--accent-green)' : 'var(--text-muted)',
                                                }}
                                              />
                                              <span className="truncate">{acc.label}</span>
                                              {acc.isDefault && <Check size={11} weight="bold" style={{ color: 'var(--accent-blue)' }} />}
                                            </div>
                                            {acc.email && (
                                              <div className="font-mono-tech text-[10px] truncate pl-3" style={{ color: 'var(--text-muted)' }}>
                                                {acc.email}
                                              </div>
                                            )}
                                          </button>
                                          <div className="flex items-center gap-1 shrink-0">
                                            <button
                                              type="button"
                                              onClick={(e) => {
                                                e.stopPropagation();
                                                setRenamingAccId(acc.id);
                                                setRenameAccDraft(acc.label);
                                              }}
                                              className="p-1 rounded hover:bg-[var(--bg-elevated)] cursor-pointer"
                                              style={{ color: 'var(--text-muted)' }}
                                              title="Rename account"
                                              aria-label={`Rename account ${acc.label}`}
                                            >
                                              <PencilSimple size={11} />
                                            </button>
                                            {!acc.authenticated && (
                                              <button
                                                type="button"
                                                onClick={() => {
                                                  setChangeAccountOpen(false);
                                                  void handleStartHarnessLogin(acc);
                                                }}
                                                className="btn-ghost px-1.5 py-0.5 text-[10.5px] cursor-pointer"
                                              >
                                                Sign in
                                              </button>
                                            )}
                                            {!isPrimary && (
                                              <button
                                                type="button"
                                                onClick={async () => {
                                                  const next = await tauriApi.deleteHarnessAccount(acc.id);
                                                  setAuthAccounts(next);
                                                  onToast(`Removed ${acc.label}`);
                                                }}
                                                className="p-1 rounded hover:bg-[var(--bg-elevated)] cursor-pointer"
                                                style={{ color: 'var(--accent-red)' }}
                                                title="Delete account"
                                                aria-label={`Delete account ${acc.label}`}
                                              >
                                                <Trash size={11} />
                                              </button>
                                            )}
                                          </div>
                                        </>
                                      )}
                                    </div>
                                  );
                                })}
                                <div className="border-t pt-1" style={{ borderColor: 'var(--border-line)' }}>
                                  <button
                                    type="button"
                                    onClick={() => {
                                      setChangeAccountOpen(false);
                                      setAddingAccount(true);
                                    }}
                                    className="w-full px-2 py-1.5 rounded-[6px] text-[11.5px] flex items-center gap-1.5 cursor-pointer hover:bg-[var(--bg-hover)]"
                                    style={{ color: 'var(--text-body)' }}
                                  >
                                    <Plus size={12} />
                                    <span>Add isolated account…</span>
                                  </button>
                                </div>
                              </div>
                            )}
                          </div>
                        </div>

                        {addingAccount && (
                          <form
                            onSubmit={async (e) => {
                              e.preventDefault();
                              if (creatingAccount) return;
                              setCreatingAccount(true);
                              try {
                                const next = await tauriApi.createHarnessAccount(
                                  selectedHarness,
                                  newAccLabel.trim(),
                                  newAccEmail.trim() || undefined,
                                  newAccAuthType || undefined,
                                );
                                setAuthAccounts(next);
                                setNewAccLabel('');
                                setNewAccEmail('');
                                setNewAccAuthType('');
                                setAddingAccount(false);
                                onToast(`Added ${meta.name} account`);
                              } catch (err) {
                                onToast(`Failed to create account: ${String(err)}`);
                              } finally {
                                setCreatingAccount(false);
                              }
                            }}
                            className="rounded-[8px] border p-3 grid grid-cols-1 sm:grid-cols-3 gap-2 items-end"
                            style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
                          >
                            <div className="space-y-1">
                              <label className="text-[10.5px] font-medium" style={{ color: 'var(--text-muted)' }}>
                                Account Label
                              </label>
                              <input
                                type="text"
                                value={newAccLabel}
                                onChange={(e) => setNewAccLabel(e.target.value)}
                                placeholder="e.g. Work"
                                className="w-full rounded-[6px] border px-2 py-1 text-[11.5px]"
                                style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-surface)', color: 'var(--text-main)' }}
                              />
                            </div>
                            <div className="space-y-1">
                              <label className="text-[10.5px] font-medium" style={{ color: 'var(--text-muted)' }}>
                                Email (optional)
                              </label>
                              <input
                                type="text"
                                value={newAccEmail}
                                onChange={(e) => setNewAccEmail(e.target.value)}
                                placeholder="you@domain.com"
                                className="w-full rounded-[6px] border px-2 py-1 text-[11.5px]"
                                style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-surface)', color: 'var(--text-main)' }}
                              />
                            </div>
                            <div className="flex items-center gap-1.5">
                              <button
                                type="submit"
                                disabled={creatingAccount}
                                className="btn-solid px-3 py-1 text-[11.5px] font-medium cursor-pointer"
                              >
                                {creatingAccount ? 'Creating...' : 'Create'}
                              </button>
                              <button
                                type="button"
                                onClick={() => setAddingAccount(false)}
                                className="btn-ghost px-2.5 py-1 text-[11.5px] cursor-pointer"
                              >
                                Cancel
                              </button>
                            </div>
                          </form>
                        )}

                        {activeAcc && session && (
                          <div
                            className="rounded-[8px] border p-3 space-y-2 text-[11.5px]"
                            style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
                          >
                            {session.deviceCode && (
                              <div className="flex items-center justify-between gap-2">
                                <span style={{ color: 'var(--text-muted)' }}>Device Code:</span>
                                <button
                                  type="button"
                                  onClick={() => {
                                    void navigator.clipboard.writeText(session.deviceCode);
                                    onToast(`Copied device code ${session.deviceCode}`);
                                  }}
                                  className="font-mono-tech font-semibold px-2 py-0.5 rounded-[4px] inline-flex items-center gap-1 cursor-pointer"
                                  style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-main)' }}
                                >
                                  <span>{session.deviceCode}</span>
                                  <Copy size={11} />
                                </button>
                              </div>
                            )}
                            {session.authUrl && (
                              <div className="flex items-center gap-1.5">
                                <button
                                  type="button"
                                  onClick={() => void tauriApi.openExternalUrl(session.authUrl)}
                                  className="btn-solid px-2.5 py-1 text-[11px] cursor-pointer"
                                >
                                  Open Login URL
                                </button>
                                <button
                                  type="button"
                                  onClick={() => {
                                    void navigator.clipboard.writeText(session.authUrl);
                                    onToast('Copied login URL');
                                  }}
                                  className="btn-ghost px-2 py-1 text-[11px] inline-flex items-center gap-1 cursor-pointer"
                                >
                                  <Copy size={11} />
                                  <span>Copy URL</span>
                                </button>
                              </div>
                            )}
                            {session.message && (
                              <div className="space-y-1">
                                <button
                                  type="button"
                                  onClick={() =>
                                    setExpandedLogs((prev) => ({ ...prev, [activeAcc.id]: !logOpen }))
                                  }
                                  className="inline-flex items-center gap-1 text-[10.5px] font-mono-tech cursor-pointer"
                                  style={{ color: isLoginError ? 'var(--accent-red, #ef4444)' : 'var(--text-muted)' }}
                                >
                                  {logOpen ? <CaretDown size={11} /> : <CaretRight size={11} />}
                                  <span>CLI Output</span>
                                </button>
                                {logOpen && (
                                  <pre
                                    className="font-mono-tech text-[10px] whitespace-pre-wrap break-all max-h-28 overflow-y-auto p-2 rounded-[5px]"
                                    style={{
                                      color: isLoginError ? 'var(--accent-red, #ef4444)' : 'var(--text-muted)',
                                      backgroundColor: 'var(--bg-surface)',
                                    }}
                                  >
                                    {session.message}
                                  </pre>
                                )}
                              </div>
                            )}
                            {(session.needsCode || (hid === 'agy-cli' && !activeAcc.authenticated)) && (
                              <div className="flex items-center gap-1.5 pt-1">
                                <input
                                  type="text"
                                  value={loginCodes[activeAcc.id] || ''}
                                  onChange={(e) =>
                                    setLoginCodes((prev) => ({ ...prev, [activeAcc.id]: e.target.value }))
                                  }
                                  placeholder="Paste auth code or callback URL"
                                  className="flex-1 min-w-0 rounded-[6px] border px-2 py-1 text-[11px] font-mono-tech"
                                  style={{
                                    borderColor: 'var(--border-line)',
                                    backgroundColor: 'var(--bg-surface)',
                                    color: 'var(--text-main)',
                                  }}
                                />
                                <button
                                  type="button"
                                  disabled={busy}
                                  onClick={() => void handleSubmitLoginCode(activeAcc.id)}
                                  className="btn-solid px-2.5 py-1 text-[11px] shrink-0 cursor-pointer"
                                >
                                  Submit
                                </button>
                              </div>
                            )}
                          </div>
                        )}
                      </div>

                      {/* Sign-in method */}
                      <div className="flex flex-wrap items-center justify-between gap-3 pt-2 border-t" style={{ borderColor: 'var(--border-line)' }}>
                        <div>
                          <div className="text-[12.5px] font-medium" style={{ color: 'var(--text-main)' }}>
                            Sign-in method
                          </div>
                          <div className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
                            {hid === 'agy-cli' ? 'OAuth or API key mode' : 'Authentication flow'}
                          </div>
                        </div>
                        <div className="flex items-center gap-2">
                          {hid === 'agy-cli' && (
                            <div className="inline-flex rounded-[6px] border overflow-hidden" style={{ borderColor: 'var(--border-line)' }}>
                              {(['headless', 'loopback'] as const).map((m) => (
                                <button
                                  key={m}
                                  type="button"
                                  onClick={() => setAgyLoginMode(m)}
                                  className="px-2 py-1 text-[11px] font-mono-tech cursor-pointer"
                                  style={{
                                    backgroundColor: agyLoginMode === m ? 'var(--bg-elevated)' : 'transparent',
                                    color: agyLoginMode === m ? 'var(--text-main)' : 'var(--text-muted)',
                                  }}
                                >
                                  {m}
                                </button>
                              ))}
                            </div>
                          )}
                          {hid === 'kiro-cli' ? (
                            <span
                              className="rounded-[6px] border px-2.5 py-1 text-[11.5px] font-mono-tech"
                              style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)', color: 'var(--text-body)' }}
                            >
                              Device Flow (AWS)
                            </span>
                          ) : (
                            activeAcc && (
                              <select
                                value={selectedAuthType}
                                onChange={(e) =>
                                  setAuthTypeOverrides((prev) => ({ ...prev, [activeAcc.id]: e.target.value }))
                                }
                                className="rounded-[6px] border px-2.5 py-1 text-[11.5px]"
                                style={{
                                  borderColor: 'var(--border-line)',
                                  backgroundColor: 'var(--bg-app)',
                                  color: 'var(--text-main)',
                                }}
                              >
                                {hid === 'agy-cli' ? (
                                  <>
                                    <option value="oauth-personal">Google Account (OAuth)</option>
                                    <option value="oauth-business">Gemini Enterprise (OAuth)</option>
                                    <option value="gemini-api-key">Gemini API Key (.env)</option>
                                    <option value="agent-platform">Vertex AI / Agent Platform</option>
                                  </>
                                ) : (
                                  <>
                                    <option value="claude-oauth">Claude OAuth</option>
                                    <option value="anthropic-api-key">Anthropic API Key (.env)</option>
                                  </>
                                )}
                              </select>
                            )
                          )}
                        </div>
                      </div>
                    </>
                  );
                })()}
              </div>
            </div>

            {(() => {
              const mergedQuotas: QuotaAccount[] = [...quotas];
              for (const acc of authAccounts) {
                if (!acc.authenticated) continue;
                const exists = mergedQuotas.some(
                  (q) =>
                    q.harness === acc.harness &&
                    (q.id === acc.id ||
                      q.label.toLowerCase() === acc.label.toLowerCase() ||
                      (acc.email && q.email && q.email.toLowerCase() === acc.email.toLowerCase())),
                );
                if (!exists) {
                  mergedQuotas.push({
                    id: acc.id,
                    harness: acc.harness,
                    label: acc.label,
                    email: acc.email || 'Authenticated',
                    fiveHourPct: 0,
                    fiveHourReset: '—',
                    weeklyPct: 0,
                    weeklyReset: '—',
                    isDefault: acc.isDefault,
                  });
                }
              }
              return (
                <>
                  <div className="flex items-center justify-between">
                    <div className="text-[12.5px]" style={{ color: 'var(--text-muted)' }}>
                      {mergedQuotas.length} connected quota account{mergedQuotas.length === 1 ? '' : 's'}
                    </div>
                    <button
                      type="button"
                      onClick={() => {
                        onRefreshQuotas?.();
                        void refreshAuthAccounts();
                      }}
                      className="btn-ghost px-2.5 py-1 text-[11.5px] inline-flex items-center gap-1.5 cursor-pointer"
                    >
                      <ArrowClockwise size={12} />
                      <span>Refresh Quotas</span>
                    </button>
                  </div>

                  {mergedQuotas.length === 0 ? (
                    <div
                      className="sakana-card rounded-[12px] p-6 text-center text-[12.5px]"
                      style={{ color: 'var(--text-muted)' }}
                    >
                      No quota accounts connected yet.
                    </div>
                  ) : (
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    {mergedQuotas.map((q) => (
                <div key={q.id} className="sakana-card rounded-[12px] p-5 space-y-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <HarnessIcon harness={q.harness} size={15} />
                      <div>
                        <div className="text-[13.5px] font-semibold" style={{ color: 'var(--text-main)' }}>
                          {HARNESS_META[q.harness]?.name ?? q.harness} · {q.label}
                        </div>
                        <div className="font-mono-tech text-[11px]" style={{ color: 'var(--text-muted)' }}>
                          {q.email}
                        </div>
                      </div>
                    </div>
                    {q.isDefault && (
                      <span
                        className="font-mono-tech text-[10px] px-2 py-0.5 rounded-[5px]"
                        style={{ backgroundColor: 'var(--accent-green-bg)', color: 'var(--accent-green)' }}
                      >
                        Default
                      </span>
                    )}
                  </div>

                  <div className="space-y-2 pt-1">
                    <div className="space-y-1">
                      <div className="flex justify-between font-mono-tech text-[11px]">
                        <span style={{ color: 'var(--text-body)' }}>5-Hour Window</span>
                        <span style={{ color: 'var(--text-main)' }}>
                          {q.fiveHourPct}% used · resets in {q.fiveHourReset}
                        </span>
                      </div>
                      <div className="h-1.5 rounded-full overflow-hidden" style={{ backgroundColor: 'var(--bg-elevated)' }}>
                        <div
                          className="h-full rounded-full"
                          style={{ width: `${Math.min(100, Math.max(0, q.fiveHourPct))}%`, backgroundColor: 'var(--accent-blue)' }}
                        />
                      </div>
                    </div>

                    <div className="space-y-1">
                      <div className="flex justify-between font-mono-tech text-[11px]">
                        <span style={{ color: 'var(--text-body)' }}>Weekly Window</span>
                        <span style={{ color: 'var(--text-main)' }}>
                          {q.weeklyPct}% used · resets in {q.weeklyReset}
                        </span>
                      </div>
                      <div className="h-1.5 rounded-full overflow-hidden" style={{ backgroundColor: 'var(--bg-elevated)' }}>
                        <div
                          className="h-full rounded-full"
                          style={{ width: `${Math.min(100, Math.max(0, q.weeklyPct))}%`, backgroundColor: 'var(--effort-high)' }}
                        />
                      </div>
                    </div>
                  </div>
                </div>
              ))}
            </div>
            )}
                </>
              );
            })()}

            {/* Harness & Model Fleet Overview */}
            <div className="sakana-card rounded-[12px] p-5 space-y-3">
              <div className="flex items-center justify-between border-b pb-2.5" style={{ borderColor: 'var(--border-line)' }}>
                <span className="text-[14px] font-semibold" style={{ color: 'var(--text-main)' }}>
                  Configured Harnesses &amp; Models
                </span>
                <span className="font-mono-tech text-[11px]" style={{ color: 'var(--text-muted)' }}>
                  4 runtimes
                </span>
              </div>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                {(Object.keys(HARNESS_META) as HarnessId[]).map((hid) => {
                  const meta = HARNESS_META[hid];
                  const activeCount = threads.filter((t) => t.harness === hid).length;
                  const liveModels = harnessModels[hid];
                  const modelBadges =
                    liveModels && liveModels.length > 0
                      ? liveModels.map((m) => ({ id: m.id, label: m.label || m.id }))
                      : meta.models.map((m) => ({ id: m, label: m }));
                  return (
                    <div
                      key={hid}
                      className="rounded-[9px] border p-3.5 space-y-2"
                      style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
                    >
                      <div className="flex items-center justify-between">
                        <span className="inline-flex items-center gap-2 text-[13px] font-semibold" style={{ color: 'var(--text-main)' }}>
                          <HarnessIcon harness={hid} size={14} />
                          <span>{meta.name}</span>
                        </span>
                        <span className="font-mono-tech text-[10.5px]" style={{ color: 'var(--text-muted)' }}>
                          {activeCount} thread{activeCount === 1 ? '' : 's'}
                        </span>
                      </div>
                      <div className="flex flex-wrap gap-1.5">
                        {modelBadges.map((m) => (
                          <span
                            key={m.id}
                            className="font-mono-tech text-[10.5px] px-2 py-0.5 rounded-[5px]"
                            style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-body)' }}
                            title={m.id !== m.label ? m.id : undefined}
                          >
                            {m.label}
                          </span>
                        ))}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>
        )}

        {/* 3. MCP & SKILLS */}
        {subtab === 'mcp' && (
          <div className="space-y-6">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-[12.5px]" style={{ color: 'var(--text-muted)' }}>
                {mcpServers.length} MCP server{mcpServers.length === 1 ? '' : 's'} registered
              </span>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={onRefreshMcp}
                  className="btn-ghost px-2.5 py-1.5 text-[11.5px] inline-flex items-center gap-1.5 cursor-pointer"
                >
                  <ArrowClockwise size={12} />
                  <span>Refresh</span>
                </button>
                <button
                  type="button"
                  onClick={() => void onImportAgyMcp()}
                  className="btn-ghost px-2.5 py-1.5 text-[11.5px] inline-flex items-center gap-1.5 cursor-pointer"
                >
                  <DownloadSimple size={12} />
                  <span>Import from AGY</span>
                </button>
                <button
                  type="button"
                  onClick={() => setAddingMcp((v) => !v)}
                  className="btn-solid px-3 py-1.5 text-[11.5px] font-medium inline-flex items-center gap-1.5 cursor-pointer"
                >
                  <Plus size={12} weight="bold" />
                  <span>Add MCP Server</span>
                </button>
              </div>
            </div>

            {addingMcp && (
              <form
                onSubmit={async (e) => {
                  e.preventDefault();
                  if (!mcpName.trim() || !mcpEndpoint.trim()) return;
                  await onAddMcp(mcpName.trim(), mcpTransport, mcpEndpoint.trim());
                  setMcpName('');
                  setMcpEndpoint('');
                  setAddingMcp(false);
                }}
                className="sakana-card rounded-[12px] p-4 space-y-3 pop-enter"
              >
                <div className="flex items-center justify-between">
                  <span className="text-[13px] font-semibold" style={{ color: 'var(--text-main)' }}>
                    Add MCP Server
                  </span>
                  <button
                    type="button"
                    onClick={() => setAddingMcp(false)}
                    aria-label="Close add MCP server form"
                    className="p-1 cursor-pointer"
                    style={{ color: 'var(--text-muted)' }}
                  >
                    <X size={13} />
                  </button>
                </div>
                <div className="grid grid-cols-1 sm:grid-cols-12 gap-2.5">
                  <input
                    value={mcpName}
                    onChange={(e) => setMcpName(e.target.value)}
                    placeholder="Server name (e.g. github)"
                    className="sm:col-span-3 rounded-[6px] border px-2.5 py-1.5 text-[12px]"
                    style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                  />
                  <select
                    value={mcpTransport}
                    onChange={(e) => setMcpTransport(e.target.value as 'stdio' | 'sse')}
                    className="sm:col-span-2 rounded-[6px] border px-2 py-1.5 text-[12px]"
                    style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                  >
                    <option value="stdio">stdio</option>
                    <option value="sse">sse</option>
                  </select>
                  <input
                    value={mcpEndpoint}
                    onChange={(e) => setMcpEndpoint(e.target.value)}
                    placeholder="Command or URL (e.g. npx -y @modelcontextprotocol/server-github)"
                    className="sm:col-span-5 rounded-[6px] border px-2.5 py-1.5 font-mono-tech text-[12px]"
                    style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                  />
                  <button type="submit" className="sm:col-span-2 btn-solid py-1.5 text-[12px] font-medium cursor-pointer">
                    Save
                  </button>
                </div>
              </form>
            )}

            {mcpServers.length === 0 ? (
              <div
                className="sakana-card rounded-[12px] p-6 text-center text-[12.5px]"
                style={{ color: 'var(--text-muted)' }}
              >
                No MCP servers registered yet. Add one or import from AGY.
              </div>
            ) : (
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              {mcpServers.map((srv) => (
                <div key={srv.id} className="sakana-card rounded-[12px] p-4 space-y-3 flex flex-col justify-between">
                  <div className="space-y-1.5">
                    <div className="flex items-center justify-between gap-2">
                      <span className="inline-flex items-center gap-1.5 text-[13.5px] font-semibold" style={{ color: 'var(--text-main)' }}>
                        <Plugs size={14} />
                        <span>{srv.name}</span>
                      </span>
                      <span
                        className="font-mono-tech text-[10px] px-1.5 py-0.5 rounded-[5px]"
                        style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }}
                      >
                        {srv.transport}
                      </span>
                    </div>
                    <div className="font-mono-tech text-[11px] break-all" style={{ color: 'var(--text-muted)' }}>
                      {srv.endpoint}
                    </div>
                  </div>

                  <div className="pt-2 border-t flex items-center justify-between gap-2" style={{ borderColor: 'var(--border-line)' }}>
                    <span
                      className="font-mono-tech text-[10.5px]"
                      style={{
                        color:
                          srv.status === 'connected'
                            ? 'var(--accent-green)'
                            : srv.status === 'error'
                              ? 'var(--accent-red)'
                              : 'var(--text-muted)',
                      }}
                    >
                      {srv.status === 'connected'
                        ? srv.latencyMs !== undefined
                          ? `✓ ${srv.latencyMs}ms`
                          : '✓ Connected'
                        : srv.status === 'error'
                          ? 'Error'
                          : 'Idle'}
                    </span>
                    <div className="flex items-center gap-1">
                      <button
                        type="button"
                        onClick={() => void onTestMcp(srv.id)}
                        className="btn-ghost px-2 py-1 text-[11px] cursor-pointer"
                        title="Ping MCP server"
                      >
                        Ping
                      </button>
                      <button
                        type="button"
                        onClick={() => void onToggleMcp(srv.id, !srv.enabled)}
                        className={srv.enabled ? 'btn-solid px-2 py-1 text-[11px] cursor-pointer' : 'btn-ghost px-2 py-1 text-[11px] cursor-pointer'}
                      >
                        {srv.enabled ? 'On' : 'Off'}
                      </button>
                      <button
                        type="button"
                        onClick={() => void onDeleteMcp(srv.id)}
                        className="btn-ghost p-1 cursor-pointer"
                        title="Delete MCP server"
                        aria-label={`Delete MCP server ${srv.name}`}
                        style={{ color: 'var(--text-muted)' }}
                      >
                        <Trash size={12} />
                      </button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
            )}

            <div className="sakana-card rounded-[12px] p-5 space-y-3">
              <div className="flex items-center justify-between border-b pb-2.5" style={{ borderColor: 'var(--border-line)' }}>
                <span className="text-[14px] font-semibold" style={{ color: 'var(--text-main)' }}>
                  Registered Skills (SKILL.md)
                </span>
                <div className="flex items-center gap-2">
                  <span className="font-mono-tech text-[11px]" style={{ color: 'var(--text-muted)' }}>
                    {skills.length} loaded
                  </span>
                  <button
                    type="button"
                    onClick={onRefreshSkills}
                    className="btn-ghost p-1 cursor-pointer"
                    title="Reload skills"
                    aria-label="Reload skills"
                  >
                    <ArrowClockwise size={12} />
                  </button>
                </div>
              </div>
              {skills.length === 0 ? (
                <div className="py-4 text-center text-[12px]" style={{ color: 'var(--text-muted)' }}>
                  No skills discovered in workspace or global ~/.sakana/skills.
                </div>
              ) : (
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
                {skills.map((sk) => (
                  <div
                    key={`${sk.scope}-${sk.slug}`}
                    className="rounded-[8px] border p-3 flex items-start justify-between gap-2"
                    style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
                  >
                    <div>
                      <div className="font-mono-tech text-[12px] font-semibold" style={{ color: 'var(--text-main)' }}>
                        /{sk.slug}
                      </div>
                      <div className="text-[12px]" style={{ color: 'var(--text-body)' }}>
                        {sk.description}
                      </div>
                    </div>
                    <span
                      className="font-mono-tech text-[10px] px-1.5 py-0.5 rounded-[4px] shrink-0"
                      style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }}
                    >
                      {sk.scope}
                    </span>
                  </div>
                ))}
              </div>
              )}
            </div>
          </div>
        )}

        {/* 4. AUTOMATIONS */}
        {subtab === 'automations' && (
          <div className="space-y-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-[12.5px]" style={{ color: 'var(--text-muted)' }}>
                {automations.length} automation{automations.length === 1 ? '' : 's'} configured
              </span>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={onRefreshAutomations}
                  className="btn-ghost px-2.5 py-1.5 text-[11.5px] inline-flex items-center gap-1.5 cursor-pointer"
                >
                  <ArrowClockwise size={12} />
                  <span>Refresh</span>
                </button>
                <button
                  type="button"
                  onClick={() => setAddingAuto((v) => !v)}
                  className="btn-solid px-3 py-1.5 text-[11.5px] font-medium inline-flex items-center gap-1.5 cursor-pointer"
                >
                  <Plus size={12} weight="bold" />
                  <span>New Automation</span>
                </button>
              </div>
            </div>

            {addingAuto && (
              <form
                onSubmit={async (e) => {
                  e.preventDefault();
                  if (!autoName.trim() || !autoSchedule.trim() || !autoPrompt.trim()) return;
                  await onCreateAutomation(
                    autoName.trim(),
                    autoKind,
                    autoSchedule.trim(),
                    autoPrompt.trim(),
                  );
                  setAutoName('');
                  setAutoPrompt('');
                  setAddingAuto(false);
                }}
                className="sakana-card rounded-[12px] p-4 space-y-3 pop-enter"
              >
                <div className="flex items-center justify-between">
                  <span className="text-[13px] font-semibold" style={{ color: 'var(--text-main)' }}>
                    Create Cron / Webhook Automation
                  </span>
                  <button
                    type="button"
                    onClick={() => setAddingAuto(false)}
                    aria-label="Close create automation form"
                    className="p-1 cursor-pointer"
                    style={{ color: 'var(--text-muted)' }}
                  >
                    <X size={13} />
                  </button>
                </div>
                <div className="grid grid-cols-1 sm:grid-cols-12 gap-2.5">
                  <input
                    value={autoName}
                    onChange={(e) => setAutoName(e.target.value)}
                    placeholder="Name (e.g. Nightly Test Sweep)"
                    className="sm:col-span-5 rounded-[6px] border px-2.5 py-1.5 text-[12px]"
                    style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                  />
                  <select
                    value={autoKind}
                    onChange={(e) => setAutoKind(e.target.value as 'cron' | 'webhook')}
                    className="sm:col-span-3 rounded-[6px] border px-2 py-1.5 text-[12px]"
                    style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                  >
                    <option value="cron">Cron Schedule</option>
                    <option value="webhook">Webhook</option>
                  </select>
                  <input
                    value={autoSchedule}
                    onChange={(e) => setAutoSchedule(e.target.value)}
                    placeholder={autoKind === 'cron' ? '0 * * * *' : '/hooks/ci'}
                    className="sm:col-span-4 rounded-[6px] border px-2.5 py-1.5 font-mono-tech text-[12px]"
                    style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                  />
                </div>
                <div className="flex gap-2">
                  <input
                    value={autoPrompt}
                    onChange={(e) => setAutoPrompt(e.target.value)}
                    placeholder="Prompt or task to run..."
                    className="flex-1 rounded-[6px] border px-2.5 py-1.5 text-[12px]"
                    style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                  />
                  <button type="submit" className="btn-solid px-4 py-1.5 text-[12px] font-medium cursor-pointer">
                    Create
                  </button>
                </div>
              </form>
            )}

            {automations.length === 0 ? (
              <div
                className="sakana-card rounded-[12px] p-6 text-center text-[12.5px]"
                style={{ color: 'var(--text-muted)' }}
              >
                No cron or webhook automations configured yet.
              </div>
            ) : (
            automations.map((auto) => {
              const histOpen = !!openAutoHistory[auto.id];
              return (
                <div key={auto.id} className="sakana-card rounded-[12px] p-5 space-y-3">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-[14px] font-semibold" style={{ color: 'var(--text-main)' }}>
                          {auto.name}
                        </span>
                        <span
                          className="font-mono-tech text-[10px] px-1.5 py-0.5 rounded-[4px]"
                          style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }}
                        >
                          {auto.kind} · {auto.schedule}
                        </span>
                      </div>
                      <p className="text-[12.5px] mt-1 m-0" style={{ color: 'var(--text-body)' }}>
                        {auto.prompt}
                      </p>
                    </div>
                    <div className="flex items-center gap-1.5 shrink-0">
                      <button
                        type="button"
                        onClick={() => void onRunAutomation(auto.id)}
                        className="btn-ghost px-2.5 py-1 text-[11.5px] inline-flex items-center gap-1 cursor-pointer"
                      >
                        <Play size={11} weight="fill" />
                        <span>Run Now</span>
                      </button>
                      <button
                        type="button"
                        onClick={() => void onToggleAutomation(auto.id, !auto.enabled)}
                        className={auto.enabled ? 'btn-solid px-2.5 py-1 text-[11.5px] cursor-pointer' : 'btn-ghost px-2.5 py-1 text-[11.5px] cursor-pointer'}
                      >
                        {auto.enabled ? 'Active' : 'Paused'}
                      </button>
                      <button
                        type="button"
                        onClick={() => void onDeleteAutomation(auto.id)}
                        className="btn-ghost p-1.5 cursor-pointer"
                        title="Delete automation"
                        aria-label={`Delete automation ${auto.name}`}
                        style={{ color: 'var(--text-muted)' }}
                      >
                        <Trash size={12} />
                      </button>
                    </div>
                  </div>

                  <div className="pt-2 border-t" style={{ borderColor: 'var(--border-line)' }}>
                    <button
                      type="button"
                      onClick={() => setOpenAutoHistory((p) => ({ ...p, [auto.id]: !p[auto.id] }))}
                      className="inline-flex items-center gap-1 text-[11.5px] font-mono-tech cursor-pointer"
                      style={{ color: 'var(--text-muted)' }}
                    >
                      {histOpen ? <CaretDown size={11} /> : <CaretRight size={11} />}
                      <span>Run History ({auto.history.length})</span>
                    </button>

                    {histOpen && (
                      <div
                        className="mt-2 rounded-[8px] border divide-y"
                        style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
                      >
                        {auto.history.length === 0 ? (
                          <div className="px-3 py-2 text-[11.5px]" style={{ color: 'var(--text-muted)' }}>
                            No runs recorded yet.
                          </div>
                        ) : (
                          auto.history.map((h, i) => (
                            <div key={i} className="px-3 py-2 flex items-center justify-between gap-2 text-[11.5px]">
                              <span style={{ color: 'var(--text-body)' }}>
                                <span
                                  className="font-mono-tech text-[10px] px-1.5 py-0.5 rounded mr-2"
                                  style={
                                    h.status === 'OK'
                                      ? { backgroundColor: 'var(--accent-green-bg)', color: 'var(--accent-green)' }
                                      : { backgroundColor: 'var(--accent-red-bg)', color: 'var(--accent-red)' }
                                  }
                                >
                                  {h.status}
                                </span>
                                {h.summary}
                              </span>
                              <span className="font-mono-tech text-[10.5px] shrink-0" style={{ color: 'var(--text-muted)' }}>
                                {h.ts} · {h.durationMs}ms
                              </span>
                            </div>
                          ))
                        )}
                      </div>
                    )}
                  </div>
                </div>
              );
            })
            )}
          </div>
        )}

        {/* 5. DISCORD & RBAC */}
        {subtab === 'discord' && (
          <div className="grid grid-cols-1 lg:grid-cols-12 gap-5">
            <div className="lg:col-span-8 sakana-card rounded-[12px] p-5 space-y-4">
              <div className="flex items-center justify-between border-b pb-3" style={{ borderColor: 'var(--border-line)' }}>
                <div className="flex items-center gap-2">
                  <DiscordIcon size={18} style={{ color: 'var(--accent-discord)' }} />
                  <div>
                    <h2 className="text-[14.5px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                      Users &amp; Role Sandbox
                    </h2>
                    <p className="text-[11.5px] m-0" style={{ color: 'var(--text-muted)' }}>
                      Owner &amp; Friends get full private DM + channel control · Guests can privately DM the bot in a read-only sandbox
                    </p>
                  </div>
                </div>
                <span
                  className="font-mono-tech text-[10px] px-2 py-0.5 rounded-[5px]"
                  style={{ backgroundColor: 'var(--accent-green-bg)', color: 'var(--accent-green)' }}
                >
                  Sandbox Enforced
                </span>
              </div>

              <form
                onSubmit={async (e) => {
                  e.preventDefault();
                  if (!newDcId.trim()) return;
                  await onUpsertDiscordUser(
                    newDcId.trim(),
                    newDcLabel.trim() || 'user',
                    newDcRole,
                  );
                  setNewDcId('');
                  setNewDcLabel('');
                }}
                className="grid grid-cols-1 sm:grid-cols-12 gap-2"
              >
                <input
                  value={newDcId}
                  onChange={(e) => setNewDcId(e.target.value)}
                  placeholder="Discord User ID"
                  className="sm:col-span-4 rounded-[6px] border px-2.5 py-1.5 font-mono-tech text-[12px]"
                  style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                />
                <input
                  value={newDcLabel}
                  onChange={(e) => setNewDcLabel(e.target.value)}
                  placeholder="Label"
                  className="sm:col-span-3 rounded-[6px] border px-2.5 py-1.5 text-[12px]"
                  style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                />
                <select
                  value={newDcRole}
                  onChange={(e) => setNewDcRole(e.target.value as DiscordUserRule['role'])}
                  className="sm:col-span-3 rounded-[6px] border px-2 py-1.5 text-[12px]"
                  style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                >
                  <option value="owner">Owner (Full)</option>
                  <option value="friend">Friend (Chat)</option>
                  <option value="guest">Guest (Sandbox)</option>
                  <option value="blocked">Blocked</option>
                </select>
                <button type="submit" className="sm:col-span-2 btn-solid py-1.5 text-[12px] font-medium cursor-pointer">
                  + Save
                </button>
              </form>

              <div className="space-y-1.5">
                {discordUsers.length === 0 ? (
                  <div className="py-4 text-center text-[12px]" style={{ color: 'var(--text-muted)' }}>
                    No Discord RBAC rules configured yet.
                  </div>
                ) : (
                  discordUsers.map((u) => (
                  <div
                    key={u.id}
                    className="rounded-[8px] border px-3 py-2 flex items-center justify-between gap-2 text-[12px]"
                    style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
                  >
                    <div className="flex items-center gap-2.5 min-w-0">
                      <span className="font-medium truncate" style={{ color: 'var(--text-main)' }}>
                        {u.label}
                      </span>
                      <span className="font-mono-tech text-[11px] truncate" style={{ color: 'var(--text-muted)' }}>
                        {u.userId}
                      </span>
                    </div>
                    <div className="flex items-center gap-1.5 shrink-0">
                      <select
                        value={u.role}
                        onChange={(e) =>
                          void onUpsertDiscordUser(
                            u.userId,
                            u.label,
                            e.target.value as DiscordUserRule['role'],
                          )
                        }
                        className="font-mono-tech text-[10.5px] px-2 py-0.5 rounded-[5px] border uppercase cursor-pointer"
                        style={{ backgroundColor: 'var(--bg-elevated)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                      >
                        <option value="owner">owner</option>
                        <option value="friend">friend</option>
                        <option value="guest">guest</option>
                        <option value="blocked">blocked</option>
                      </select>
                      <button
                        type="button"
                        onClick={() => void onDeleteDiscordUser(u.id)}
                        className="btn-ghost p-1 cursor-pointer"
                        title="Remove RBAC rule"
                        aria-label={`Remove RBAC rule for ${u.label}`}
                        style={{ color: 'var(--text-muted)' }}
                      >
                        <Trash size={12} />
                      </button>
                    </div>
                  </div>
                  ))
                )}
              </div>

              {onSetProjectDiscordChannel && projects.length > 0 && (
                <div className="sakana-card rounded-[12px] p-5 space-y-3 mt-4">
                  <div className="flex items-center justify-between gap-2">
                    <h3 className="text-[14px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                      Project Channel Bindings
                    </h3>
                    <span className="font-mono-tech text-[10.5px]" style={{ color: 'var(--text-muted)' }}>
                      Route #channel → project
                    </span>
                  </div>
                  <p className="text-[11.5px] m-0" style={{ color: 'var(--text-muted)' }}>
                    Bind a workspace project to a Discord channel name (e.g. <code>#sakana</code>). Messages in that channel automatically route to the matching project.
                  </p>
                  <div className="space-y-2">
                    {projects.map((p) => {
                      const draft = projChannelDrafts[p.id] ?? p.discordChannel ?? '';
                      const dirty = draft.trim() !== (p.discordChannel ?? '');
                      return (
                        <div
                          key={p.id}
                          className="flex items-center justify-between gap-2 p-2.5 rounded-[8px] border"
                          style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
                        >
                          <div className="min-w-0 flex-1">
                            <div className="text-[12.5px] font-medium truncate" style={{ color: 'var(--text-main)' }}>
                              {p.name}
                            </div>
                            <div className="font-mono-tech text-[10px] truncate" style={{ color: 'var(--text-muted)' }}>
                              {p.cwd}
                            </div>
                          </div>
                          <div className="flex items-center gap-1.5 shrink-0">
                            <input
                              value={draft}
                              onChange={(e) =>
                                setProjChannelDrafts((prev) => ({ ...prev, [p.id]: e.target.value }))
                              }
                              placeholder="#channel-name"
                              className="w-36 rounded-[6px] border px-2 py-1 font-mono-tech text-[11px]"
                              style={{
                                backgroundColor: 'var(--bg-card)',
                                borderColor: 'var(--border-line)',
                                color: 'var(--text-main)',
                              }}
                            />
                            <button
                              type="button"
                              disabled={!dirty}
                              onClick={() => void onSetProjectDiscordChannel(p.id, draft)}
                              className="btn-solid px-2.5 py-1 text-[11px] font-medium cursor-pointer disabled:opacity-40"
                            >
                              Save
                            </button>
                            {p.discordChannel && (
                              <button
                                type="button"
                                onClick={() => {
                                  setProjChannelDrafts((prev) => ({ ...prev, [p.id]: '' }));
                                  void onSetProjectDiscordChannel(p.id, '');
                                }}
                                className="btn-ghost p-1 cursor-pointer"
                                title="Unbind Discord channel"
                                aria-label={`Unbind Discord channel for ${p.name}`}
                                style={{ color: 'var(--text-muted)' }}
                              >
                                <Trash size={12} />
                              </button>
                            )}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}

              <div className="sakana-card rounded-[12px] p-5 space-y-3 mt-4">
                <div className="flex items-center justify-between gap-2">
                  <h3 className="text-[14px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                    Discord-Synced Threads &amp; Private DMs
                  </h3>
                  <span className="font-mono-tech text-[10.5px]" style={{ color: 'var(--text-muted)' }}>
                    {discordThreads.length} active
                  </span>
                </div>
                <p className="text-[11.5px] m-0" style={{ color: 'var(--text-muted)' }}>
                  Right-click any project or thread in the sidebar to turn it into a Discord chat, or DM the bot directly.
                </p>
                {discordThreads.length === 0 ? (
                  <div className="py-3 text-center text-[12px]" style={{ color: 'var(--text-muted)' }}>
                    No Discord-synced threads yet.
                  </div>
                ) : (
                  <div className="space-y-1.5">
                    {discordThreads.map((t) => (
                      <div
                        key={t.id}
                        className="flex items-center justify-between gap-2 p-2.5 rounded-[8px] border"
                        style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
                      >
                        <div className="min-w-0 flex-1">
                          <div className="text-[12.5px] font-medium truncate flex items-center gap-1.5" style={{ color: 'var(--text-main)' }}>
                            <DiscordIcon size={12} style={{ color: 'var(--accent-discord)' }} />
                            <span className="truncate">{t.title}</span>
                          </div>
                          <div className="font-mono-tech text-[10px] truncate" style={{ color: 'var(--text-muted)' }}>
                            {t.discordChannel || '#discord'} · {t.messages.length} messages
                          </div>
                        </div>
                        <div className="flex items-center gap-1.5 shrink-0">
                          <button
                            type="button"
                            onClick={() => onSelectThread(t.id)}
                            className="btn-ghost px-2.5 py-1 text-[11px] cursor-pointer"
                          >
                            Open
                          </button>
                          {onSetThreadDiscordChannel && (
                            <button
                              type="button"
                              onClick={() => void onSetThreadDiscordChannel(t.id, null)}
                              className="btn-ghost p-1 cursor-pointer"
                              title="Disconnect thread from Discord"
                              aria-label={`Disconnect ${t.title} from Discord`}
                              style={{ color: 'var(--text-muted)' }}
                            >
                              <Trash size={12} />
                            </button>
                          )}
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>

            <div className="lg:col-span-4 space-y-4">
              <div className="sakana-card rounded-[12px] p-5 space-y-3">
                <div className="flex items-center justify-between">
                  <h3 className="text-[14px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                    Discord Bot Runtime
                  </h3>
                  <span
                    className="font-mono-tech text-[10px] px-2 py-0.5 rounded-[5px]"
                    style={
                      discordStatus?.running
                        ? { backgroundColor: 'var(--accent-green-bg)', color: 'var(--accent-green)' }
                        : { backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }
                    }
                  >
                    {discordStatus?.running && (discordStatus?.connectedGuilds ?? 0) > 0
                      ? `Connected (${discordStatus.connectedGuilds})`
                      : discordStatus?.running
                        ? 'Running'
                        : 'Stopped'}
                  </span>
                </div>
                {discordStatus?.botTag && (
                  <div className="font-mono-tech text-[11px]" style={{ color: 'var(--text-muted)' }}>
                    Bot: {discordStatus.botTag}
                  </div>
                )}
                {discordStatus?.lastError && (
                  <div className="font-mono-tech text-[11px] break-words" style={{ color: 'var(--accent-red)' }}>
                    {discordStatus.lastError}
                  </div>
                )}
                <input
                  type="password"
                  value={dcToken}
                  onChange={(e) => setDcToken(e.target.value)}
                  placeholder={discordStatus?.hasToken ? 'Token saved in .env (optional override)' : 'DISCORD_BOT_TOKEN'}
                  className="w-full rounded-[6px] border px-2.5 py-1.5 font-mono-tech text-[11.5px]"
                  style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                />
                {discordStatus?.running ? (
                  <button
                    type="button"
                    onClick={() => void onStopDiscordBot()}
                    className="btn-ghost w-full py-2 text-[12px] font-medium inline-flex items-center justify-center gap-1.5 cursor-pointer"
                  >
                    <Stop size={13} weight="fill" />
                    <span>Stop Discord Bot</span>
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => void onStartDiscordBot(dcToken.trim() || undefined)}
                    className="btn-solid w-full py-2 text-[12px] font-medium inline-flex items-center justify-center gap-1.5 cursor-pointer"
                  >
                    <Play size={13} weight="fill" />
                    <span>Start Discord Bot</span>
                  </button>
                )}
              </div>

              <div className="sakana-card rounded-[12px] p-5 space-y-3 h-fit">
                <div className="flex items-center justify-between gap-2">
                  <h3 className="text-[14px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                    Bot Harness &amp; Reactions
                  </h3>
                  <span className="font-mono-tech text-[10px]" style={{ color: 'var(--text-muted)' }}>
                    Default Config
                  </span>
                </div>
                <p className="text-[11.5px] m-0" style={{ color: 'var(--text-muted)' }}>
                  Choose which connected CLI harness Discord DMs &amp; channels use so replies never fall back to missing API keys.
                </p>
                <div className="space-y-2">
                  <div>
                    <label className="block text-[11px] font-medium mb-1" style={{ color: 'var(--text-muted)' }}>
                      Default Harness
                    </label>
                    <select
                      value={discordSettings.defaultHarness}
                      onChange={(e) =>
                        setDiscordSettings((prev) => ({
                          ...prev,
                          defaultHarness: e.target.value,
                          defaultModel: '',
                        }))
                      }
                      className="w-full rounded-[6px] border px-2.5 py-1.5 font-mono-tech text-[11.5px]"
                      style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                    >
                      <option value="auto">Auto (Detect connected CLI)</option>
                      <option value="agy-cli">agy-cli (Antigravity)</option>
                      <option value="claude-code">claude-code (Claude Code)</option>
                      <option value="kiro-cli">kiro-cli (Kiro)</option>
                      <option value="sakana">sakana (OpenRouter/OpenAI API key)</option>
                    </select>
                  </div>
                  <div className="grid grid-cols-2 gap-2">
                    <div>
                      <label className="block text-[11px] font-medium mb-1" style={{ color: 'var(--text-muted)' }}>
                        Model (optional)
                      </label>
                      <input
                        value={discordSettings.defaultModel}
                        onChange={(e) =>
                          setDiscordSettings((prev) => ({ ...prev, defaultModel: e.target.value }))
                        }
                        list="discord-harness-models"
                        placeholder="harness default"
                        className="w-full rounded-[6px] border px-2 py-1.5 font-mono-tech text-[11px]"
                        style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                      />
                      <datalist id="discord-harness-models">
                        {(harnessModels[discordSettings.defaultHarness] ?? []).map((m) => (
                          <option key={m.id} value={m.id} />
                        ))}
                      </datalist>
                    </div>
                    <div>
                      <label className="block text-[11px] font-medium mb-1" style={{ color: 'var(--text-muted)' }}>
                        Effort
                      </label>
                      <select
                        value={discordSettings.defaultEffort}
                        onChange={(e) =>
                          setDiscordSettings((prev) => ({ ...prev, defaultEffort: e.target.value }))
                        }
                        className="w-full rounded-[6px] border px-2 py-1.5 font-mono-tech text-[11px]"
                        style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                      >
                        <option value="">Default</option>
                        <option value="low">low</option>
                        <option value="medium">medium</option>
                        <option value="high">high</option>
                        <option value="xhigh">xhigh</option>
                      </select>
                    </div>
                  </div>
                  <label className="flex items-center justify-between gap-2 pt-1 cursor-pointer text-[12px]" style={{ color: 'var(--text-body)' }}>
                    <span>Auto-start bot on app launch</span>
                    <input
                      type="checkbox"
                      checked={discordSettings.autoStart}
                      onChange={(e) =>
                        setDiscordSettings((prev) => ({ ...prev, autoStart: e.target.checked }))
                      }
                      className="cursor-pointer"
                    />
                  </label>
                  <label className="flex items-center justify-between gap-2 pt-1 cursor-pointer text-[12px]" style={{ color: 'var(--text-body)' }}>
                    <span>React with 👀 (read) → ✅ (done)</span>
                    <input
                      type="checkbox"
                      checked={discordSettings.reactWithEmojis}
                      onChange={(e) =>
                        setDiscordSettings((prev) => ({ ...prev, reactWithEmojis: e.target.checked }))
                      }
                      className="cursor-pointer"
                    />
                  </label>
                </div>
                <button
                  type="button"
                  disabled={savingDcSettings}
                  onClick={async () => {
                    setSavingDcSettings(true);
                    try {
                      if (isTauriRuntime()) {
                        const saved = await tauriApi.updateDiscordSettings(discordSettings);
                        if (saved) setDiscordSettings(saved);
                      }
                      onToast('Saved Discord bot harness & reaction settings');
                    } catch (err) {
                      onToast(`Failed to save Discord settings: ${String(err)}`);
                    } finally {
                      setSavingDcSettings(false);
                    }
                  }}
                  className="btn-solid w-full py-1.5 text-[12px] font-medium cursor-pointer disabled:opacity-50"
                >
                  {savingDcSettings ? 'Saving…' : 'Save Bot Settings'}
                </button>
              </div>

              <div className="sakana-card rounded-[12px] p-5 space-y-3 h-fit">
                <h3 className="text-[14px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                  Quick Invite Bot
                </h3>
                <p className="text-[12px] m-0" style={{ color: 'var(--text-muted)' }}>
                  Enter your Discord Application Client ID to generate an OAuth2 invite URL with slash commands &amp; thread permissions.
                </p>
                <input
                  value={dcClientId}
                  onChange={(e) => setDcClientId(e.target.value)}
                  placeholder="Discord Application Client ID"
                  className="w-full rounded-[6px] border px-2.5 py-1.5 font-mono-tech text-[11.5px]"
                  style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                />
                <button
                  type="button"
                  onClick={() => {
                    const trimmedId = dcClientId.trim();
                    if (!trimmedId) {
                      onToast('Enter your Discord Application Client ID first');
                      return;
                    }
                    const url = `https://discord.com/oauth2/authorize?client_id=${encodeURIComponent(trimmedId)}&scope=bot%20applications.commands`;
                    void navigator.clipboard?.writeText(url);
                    onToast('Copied Discord OAuth2 invite link');
                  }}
                  className="btn-solid w-full py-2 text-[12px] font-medium inline-flex items-center justify-center gap-1.5 cursor-pointer"
                >
                  <Copy size={13} />
                  <span>Copy OAuth2 Invite</span>
                </button>
              </div>

              <div className="sakana-card rounded-[12px] p-5 space-y-2.5 h-fit">
                <h3 className="text-[14px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                  Commands &amp; Private DMs
                </h3>
                <div className="space-y-1.5 text-[11.5px] font-mono-tech" style={{ color: 'var(--text-body)' }}>
                  <div><strong>Direct Message</strong> — Private chat (👀 read → ✅ done)</div>
                  <div><strong>!project &lt;name&gt;</strong> — Switch active DM project</div>
                  <div><strong>!auto list|run|toggle|delete</strong> — Manage automations</div>
                  <div><strong>!auto create name | cron | prompt</strong> — Schedule task to this chat</div>
                  <div><strong>/automation</strong> — Slash command for automations</div>
                  <div><strong>/projects</strong> — List projects &amp; channel links</div>
                  <div><strong>/bind &lt;project&gt;</strong> — Link current channel or DM</div>
                  <div><strong>/ask &lt;prompt&gt;</strong> — Send prompt to bound project/DM</div>
                  <div><strong>/status</strong> — Show workspace status</div>
                </div>
              </div>
            </div>
          </div>
        )}

        {/* 6. PREFERENCES, REMOTE & .ENV SECRETS */}
        {subtab === 'remote' && (
          <div className="space-y-5">
            {/* Appearance & Preferences */}
            <div className="sakana-card rounded-[12px] p-5 space-y-4">
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                  <Palette size={15} style={{ color: 'var(--text-muted)' }} />
                  <h2 className="text-[14.5px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                    Appearance & Preferences
                  </h2>
                </div>
                <span className="font-mono-tech text-[11px]" style={{ color: 'var(--text-muted)' }}>
                  Active: {THEME_META.find((t) => t.id === theme)?.label || theme}
                </span>
              </div>
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                {THEME_META.map((tm) => {
                  const active = theme === tm.id;
                  const swatch =
                    tm.id === 'paper'
                      ? { bg: '#faf9f7', card: '#ffffff', ink: '#171717', border: '#e5e2dc' }
                      : tm.id === 'noir'
                        ? { bg: '#050505', card: '#121212', ink: '#f4f4f5', border: '#262626' }
                        : tm.id === 'slate'
                          ? { bg: '#13161c', card: '#1b2028', ink: '#e6edf3', border: '#2d333d' }
                          : { bg: '#f4eee4', card: '#fbf7f0', ink: '#2c251d', border: '#dfd5c5' };
                  return (
                    <button
                      key={tm.id}
                      type="button"
                      onClick={() => onSetTheme(tm.id)}
                      className="rounded-[10px] border p-3 text-left cursor-pointer transition-all space-y-2.5"
                      style={{
                        backgroundColor: active ? 'var(--bg-elevated)' : 'var(--bg-app)',
                        borderColor: active ? 'var(--accent-blue)' : 'var(--border-line)',
                      }}
                    >
                      <div
                        className="h-12 rounded-[7px] border p-2 flex flex-col justify-between"
                        style={{ backgroundColor: swatch.bg, borderColor: swatch.border }}
                      >
                        <div className="flex items-center justify-between">
                          <span className="w-6 h-1.5 rounded-full" style={{ backgroundColor: swatch.ink, opacity: 0.7 }} />
                          <span className="w-2 h-2 rounded-full" style={{ backgroundColor: swatch.ink, opacity: 0.35 }} />
                        </div>
                        <div
                          className="h-4 rounded-[4px] border"
                          style={{ backgroundColor: swatch.card, borderColor: swatch.border }}
                        />
                      </div>
                      <div className="flex items-center justify-between gap-1">
                        <div>
                          <div className="text-[12.5px] font-medium" style={{ color: 'var(--text-main)' }}>
                            {tm.label}
                          </div>
                          <div className="font-mono-tech text-[10px]" style={{ color: 'var(--text-muted)' }}>
                            {tm.kind}
                          </div>
                        </div>
                        {active && (
                          <span
                            className="w-5 h-5 rounded-full flex items-center justify-center shrink-0"
                            style={{ backgroundColor: 'var(--accent-blue)', color: '#ffffff' }}
                          >
                            <Check size={11} weight="bold" />
                          </span>
                        )}
                      </div>
                    </button>
                  );
                })}
              </div>

              {customization && onUpdateCustomization && (
                <div
                  className="pt-3 mt-1 border-t space-y-3"
                  style={{ borderColor: 'var(--border-line)' }}
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div>
                      <div className="text-[12.5px] font-medium" style={{ color: 'var(--text-main)' }}>
                        AI Self-Modify &amp; Custom Theme
                      </div>
                      <div className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
                        Allow agents in chat to apply themes, CSS overrides, UI scale, and automations via <code>sakana-action</code>.
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <label className="inline-flex items-center gap-1.5 text-[11.5px] cursor-pointer select-none">
                        <input
                          type="checkbox"
                          checked={customization.enabled}
                          onChange={(e) =>
                            void onUpdateCustomization({
                              ...customization,
                              enabled: e.target.checked,
                            })
                          }
                        />
                        <span style={{ color: 'var(--text-main)' }}>
                          {customization.enabled ? 'Enabled' : 'Disabled'}
                        </span>
                      </label>
                      {onResetCustomization && (
                        <button
                          type="button"
                          onClick={() => void onResetCustomization()}
                          className="btn-ghost px-2.5 py-1 text-[11px] cursor-pointer"
                        >
                          Reset Customizations
                        </button>
                      )}
                    </div>
                  </div>

                  {(customization.customThemeName ||
                    Object.keys(customization.cssVars || {}).length > 0 ||
                    customization.customCss ||
                    customization.uiScale) && (
                    <div
                      className="rounded-[8px] border px-3 py-2 flex flex-wrap items-center gap-3 font-mono-tech text-[11px]"
                      style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)', color: 'var(--text-muted)' }}
                    >
                      {customization.customThemeName && (
                        <span style={{ color: 'var(--text-main)' }}>
                          Custom Theme: {customization.customThemeName}
                        </span>
                      )}
                      <span>
                        CSS Vars: {Object.keys(customization.cssVars || {}).length}
                      </span>
                      {customization.uiScale && (
                        <span>Scale: {Math.round(customization.uiScale * 100)}%</span>
                      )}
                      {customization.customCss && (
                        <span>Custom CSS: {customization.customCss.length} chars</span>
                      )}
                    </div>
                  )}

                  {customization.savedPresets && customization.savedPresets.length > 0 && (
                    <div className="space-y-1.5">
                      <div className="font-mono-tech text-[10.5px] uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>
                        Saved AI Theme Presets
                      </div>
                      <div className="flex flex-wrap gap-2">
                        {customization.savedPresets.map((preset) => (
                          <div
                            key={preset.id}
                            className="inline-flex items-center gap-1 rounded-[7px] border px-2 py-1 text-[11.5px]"
                            style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
                          >
                            <button
                              type="button"
                              onClick={() =>
                                void onUpdateCustomization({
                                  ...customization,
                                  theme: preset.baseTheme,
                                  customThemeName: preset.name,
                                  cssVars: { ...preset.cssVars },
                                  customCss: preset.customCss,
                                })
                              }
                              className="cursor-pointer hover:underline"
                              style={{ color: 'var(--text-main)' }}
                            >
                              {preset.name}
                            </button>
                            <button
                              type="button"
                              onClick={() =>
                                void onUpdateCustomization({
                                  ...customization,
                                  savedPresets: customization.savedPresets.filter((p) => p.id !== preset.id),
                                })
                              }
                              className="p-0.5 rounded hover:bg-[var(--bg-hover)] cursor-pointer"
                              style={{ color: 'var(--text-muted)' }}
                              title="Delete preset"
                            >
                              <X size={10} />
                            </button>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>

            {/* GitHub Integration */}
            <div className="sakana-card rounded-[12px] p-5 space-y-4">
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                  <GithubLogo size={16} weight="fill" style={{ color: 'var(--text-main)' }} />
                  <h2 className="text-[14.5px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                    GitHub Account &amp; Repository Clone
                  </h2>
                </div>
                <span
                  className="font-mono-tech text-[10px] px-2 py-0.5 rounded-[5px]"
                  style={
                    ghStatus?.authenticated
                      ? { backgroundColor: 'var(--accent-green-bg)', color: 'var(--accent-green)' }
                      : { backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }
                  }
                >
                  {ghStatus?.authenticated
                    ? `@${ghStatus.login} (${ghStatus.source === 'gh-cli' ? 'gh CLI' : 'Token'})`
                    : 'Not Connected'}
                </span>
              </div>
              {ghStatus?.authenticated ? (
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="flex items-center gap-3 min-w-0">
                    {ghStatus.avatarUrl && (
                      <img
                        src={ghStatus.avatarUrl}
                        alt={ghStatus.login}
                        className="w-9 h-9 rounded-full border shrink-0"
                        style={{ borderColor: 'var(--border-line)' }}
                      />
                    )}
                    <div className="min-w-0">
                      <div className="text-[13px] font-medium truncate" style={{ color: 'var(--text-main)' }}>
                        {ghStatus.name || ghStatus.login}{' '}
                        <span className="font-mono-tech text-[11px]" style={{ color: 'var(--text-muted)' }}>
                          @{ghStatus.login}
                        </span>
                      </div>
                      <div className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
                        Default clone folder: <span className="font-mono-tech">{ghStatus.defaultCloneDir}</span>
                      </div>
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    {onOpenGitHubClone && (
                      <button
                        type="button"
                        onClick={onOpenGitHubClone}
                        className="btn-solid px-3 py-1.5 text-[11.5px] font-medium inline-flex items-center gap-1.5 cursor-pointer"
                      >
                        <GithubLogo size={13} weight="fill" />
                        <span>Browse &amp; Clone Repos</span>
                      </button>
                    )}
                    {ghStatus.source === 'token' && (
                      <button
                        type="button"
                        disabled={ghBusy}
                        onClick={async () => {
                          setGhBusy(true);
                          setGhError('');
                          try {
                            const st = await tauriApi.githubSaveToken('');
                            setGhStatus(st);
                            onToast('Disconnected GitHub token');
                          } catch (err) {
                            setGhError(String(err));
                          } finally {
                            setGhBusy(false);
                          }
                        }}
                        className="btn-ghost px-2.5 py-1.5 text-[11.5px] inline-flex items-center gap-1.5 cursor-pointer"
                      >
                        <SignOut size={12} />
                        <span>Disconnect</span>
                      </button>
                    )}
                  </div>
                </div>
              ) : (
                <div className="space-y-3">
                  <p className="text-[12px] m-0 leading-relaxed" style={{ color: 'var(--text-muted)' }}>
                    Connect your GitHub account to automatically list your repositories and clone them directly into a Sakana workspace.
                  </p>
                  <div className="flex flex-wrap items-center gap-2">
                    <button
                      type="button"
                      onClick={() =>
                        void tauriApi.openExternalUrl(
                          'https://github.com/settings/tokens/new?scopes=repo,read:user&description=Sakana%20Desktop',
                        )
                      }
                      className="btn-ghost px-2.5 py-1.5 text-[11.5px] font-medium inline-flex items-center gap-1.5 cursor-pointer"
                    >
                      <ArrowSquareOut size={12} />
                      <span>Create Token on GitHub</span>
                    </button>
                    <button
                      type="button"
                      disabled={ghBusy}
                      onClick={async () => {
                        setGhBusy(true);
                        setGhError('');
                        await refreshGitHubStatus();
                        setGhBusy(false);
                      }}
                      className="btn-ghost px-2.5 py-1.5 text-[11.5px] font-medium inline-flex items-center gap-1.5 cursor-pointer"
                    >
                      <ArrowClockwise size={12} />
                      <span>Auto-detect gh CLI</span>
                    </button>
                  </div>
                  <div className="flex items-center gap-2">
                    <div className="relative flex-1">
                      <Key
                        size={12}
                        className="absolute left-2.5 top-1/2 -translate-y-1/2"
                        style={{ color: 'var(--text-muted)' }}
                      />
                      <input
                        type="password"
                        value={ghTokenInput}
                        onChange={(e) => setGhTokenInput(e.target.value)}
                        onKeyDown={async (e) => {
                          if (e.key === 'Enter' && ghTokenInput.trim()) {
                            setGhBusy(true);
                            setGhError('');
                            try {
                              const st = await tauriApi.githubSaveToken(ghTokenInput.trim());
                              setGhStatus(st);
                              setGhTokenInput('');
                              if (st?.authenticated) onToast(`Connected to GitHub as @${st.login}`);
                            } catch (err) {
                              setGhError(String(err));
                            } finally {
                              setGhBusy(false);
                            }
                          }
                        }}
                        placeholder="Paste GitHub Personal Access Token (ghp_...)"
                        className="w-full rounded-[7px] border pl-8 pr-3 py-1.5 font-mono-tech text-[11.5px] focus:outline-none"
                        style={{
                          backgroundColor: 'var(--bg-app)',
                          borderColor: 'var(--border-line)',
                          color: 'var(--text-main)',
                        }}
                      />
                    </div>
                    <button
                      type="button"
                      disabled={ghBusy || !ghTokenInput.trim()}
                      onClick={async () => {
                        setGhBusy(true);
                        setGhError('');
                        try {
                          const st = await tauriApi.githubSaveToken(ghTokenInput.trim());
                          setGhStatus(st);
                          setGhTokenInput('');
                          if (st?.authenticated) onToast(`Connected to GitHub as @${st.login}`);
                        } catch (err) {
                          setGhError(String(err));
                        } finally {
                          setGhBusy(false);
                        }
                      }}
                      className="btn-solid px-3.5 py-1.5 text-[11.5px] font-medium cursor-pointer disabled:opacity-50"
                    >
                      {ghBusy ? 'Connecting…' : 'Connect'}
                    </button>
                  </div>
                  {ghError && (
                    <div className="text-[11px] font-mono-tech" style={{ color: 'var(--accent-red)' }}>
                      {ghError}
                    </div>
                  )}
                </div>
              )}
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-12 gap-5">
            <div className="lg:col-span-5 space-y-4">
              <div className="sakana-card rounded-[12px] p-5 space-y-4">
                <div className="flex items-center justify-between">
                  <h2 className="text-[14.5px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                    Remote Axum Gateway
                  </h2>
                  <span
                    className="font-mono-tech text-[10px] px-2 py-0.5 rounded-[5px]"
                    style={
                      remoteStatus?.running
                        ? { backgroundColor: 'var(--accent-green-bg)', color: 'var(--accent-green)' }
                        : { backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }
                    }
                  >
                    {remoteStatus?.running
                      ? `http://${
                          remoteStatus.bindAddr?.includes(':')
                            ? remoteStatus.bindAddr
                            : `${remoteStatus.bindAddr || '0.0.0.0'}:${remoteStatus.port}`
                        }`
                      : 'Stopped'}
                  </span>
                </div>

                <div className="space-y-2">
                  <div className="flex items-center gap-2">
                    <input
                      value={remotePort}
                      onChange={(e) => setRemotePort(e.target.value)}
                      placeholder="4317"
                      aria-label="Remote server port"
                      className="w-24 h-8 rounded-[6px] border px-2.5 font-mono-tech text-[12px] shrink-0"
                      style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                    />
                    {remoteStatus?.running ? (
                      <button
                        type="button"
                        onClick={() => void onStopRemoteServer()}
                        className="btn-ghost flex-1 h-8 px-3 text-[12px] font-medium inline-flex items-center justify-center gap-1.5 whitespace-nowrap shrink-0 cursor-pointer"
                      >
                        <Stop size={13} weight="fill" />
                        <span>Stop Server</span>
                      </button>
                    ) : (
                      <button
                        type="button"
                        onClick={() => {
                          const parsed = Number.parseInt(remotePort, 10);
                          void onStartRemoteServer(Number.isFinite(parsed) ? parsed : undefined, lanEnabled);
                        }}
                        className="btn-solid flex-1 h-8 px-3 text-[12px] font-medium inline-flex items-center justify-center gap-1.5 whitespace-nowrap shrink-0 cursor-pointer"
                      >
                        <Play size={13} weight="fill" />
                        <span>Start Remote Server</span>
                      </button>
                    )}
                  </div>

                  <label
                    className="flex items-center justify-between gap-2 text-[11.5px] px-2.5 py-1.5 rounded-[6px] border cursor-pointer select-none"
                    style={{ borderColor: 'var(--border-line)', color: 'var(--text-body)', backgroundColor: 'var(--bg-app)' }}
                    title="Bind to 0.0.0.0 for cross-device LAN/QR pairing instead of 127.0.0.1"
                  >
                    <span className="inline-flex items-center gap-2">
                      <input
                        type="checkbox"
                        checked={lanEnabled}
                        disabled={Boolean(remoteStatus?.running)}
                        onChange={(e) => setLanEnabled(e.target.checked)}
                      />
                      <span className="font-mono-tech text-[11px]">LAN Access (0.0.0.0)</span>
                    </span>
                    <span className="text-[10.5px]" style={{ color: 'var(--text-muted)' }}>
                      Allow LAN / QR pairing
                    </span>
                  </label>
                </div>

                {(() => {
                  const rootStyles =
                    typeof document !== 'undefined' ? getComputedStyle(document.documentElement) : null;
                  const qrBg = rootStyles?.getPropertyValue('--bg-card').trim() || 'var(--bg-card)';
                  const qrFg = rootStyles?.getPropertyValue('--text-main').trim() || 'var(--text-main)';
                  return (
                    <div
                      className="p-4 rounded-[10px] border flex items-center justify-center"
                      style={{ borderColor: 'var(--border-line)', backgroundColor: qrBg, color: qrFg }}
                    >
                      {remoteStatus?.qrSvg ? (
                        <div
                          className="w-32 h-32 [&>svg]:w-full [&>svg]:h-full"
                          dangerouslySetInnerHTML={{ __html: remoteStatus.qrSvg }}
                        />
                      ) : (
                        <QrCode size={112} weight="regular" />
                      )}
                    </div>
                  );
                })()}

                <div
                  className="rounded-[7px] border px-3 py-2 font-mono-tech text-[11px] truncate"
                  style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)', color: 'var(--text-main)' }}
                >
                  {remoteStatus?.magicLink || 'Start remote server to generate Magic Link'}
                </div>

                <button
                  type="button"
                  onClick={() => {
                    const link = remoteStatus?.magicLink;
                    if (!link) {
                      onToast('Start the remote server first');
                      return;
                    }
                    void navigator.clipboard?.writeText(link);
                    onToast('Copied Tailscale Magic Link');
                  }}
                  className="btn-solid w-full py-2 text-[12px] font-medium inline-flex items-center justify-center gap-1.5 cursor-pointer"
                >
                  <Copy size={13} />
                  <span>Copy Magic Link</span>
                </button>
              </div>

              <div className="sakana-card rounded-[12px] p-5 space-y-3">
                <div className="flex items-center justify-between">
                  <h2 className="text-[14.5px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                    Remote Sakana Hosts
                  </h2>
                  <span className="font-mono-tech text-[10px]" style={{ color: 'var(--text-muted)' }}>
                    Client Mode
                  </span>
                </div>
                <p className="text-[11.5px] m-0 leading-relaxed" style={{ color: 'var(--text-muted)' }}>
                  Connect this desktop UI to a Linux/macOS machine running <code className="font-mono-tech">sakana-daemon</code> or another Sakana Desktop.
                </p>

                <div className="flex items-center gap-1.5">
                  <input
                    value={pairingInput}
                    onChange={(e) => setPairingInput(e.target.value)}
                    placeholder="Paste sakana://ip:4317?token=...&name=..."
                    className="flex-1 min-w-0 rounded-[6px] border px-2.5 py-1.5 font-mono-tech text-[11px]"
                    style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                  />
                  <button
                    type="button"
                    onClick={() => {
                      const raw = pairingInput.trim();
                      if (!raw) return;
                      try {
                        const normalized = raw.replace(/^sakana:\/\//i, 'http://');
                        const u = new URL(normalized);
                        const tok = u.searchParams.get('token') || '';
                        const nm = u.searchParams.get('name') || u.hostname;
                        setHostUrl(`${u.protocol}//${u.host}`);
                        if (tok) setHostToken(tok);
                        if (nm) setHostName(nm);
                        setPairingInput('');
                        onToast('Parsed Magic Pairing Link');
                      } catch {
                        onToast('Invalid pairing link format');
                      }
                    }}
                    className="btn-ghost px-2.5 py-1.5 text-[11px] shrink-0 cursor-pointer"
                  >
                    Parse
                  </button>
                </div>

                <form
                  onSubmit={async (e) => {
                    e.preventDefault();
                    if (!hostName.trim() || !hostUrl.trim() || !hostToken.trim()) return;
                    try {
                      await tauriApi.upsertRemoteHost({
                        name: hostName.trim(),
                        url: hostUrl.trim(),
                        token: hostToken.trim(),
                      });
                      setHostName('');
                      setHostUrl('');
                      setHostToken('');
                      await onRefreshRemoteHosts();
                      onToast('Saved remote host');
                    } catch (err) {
                      onToast(`Save remote host failed: ${String(err)}`);
                    }
                  }}
                  className="space-y-2"
                >
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                    <input
                      value={hostName}
                      onChange={(e) => setHostName(e.target.value)}
                      placeholder="Host name (e.g. Linux GPU Box)"
                      className="rounded-[6px] border px-2.5 py-1.5 text-[11.5px]"
                      style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                    />
                    <input
                      value={hostUrl}
                      onChange={(e) => setHostUrl(e.target.value)}
                      placeholder="http://100.x.y.z:4317"
                      className="rounded-[6px] border px-2.5 py-1.5 font-mono-tech text-[11.5px]"
                      style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                    />
                  </div>
                  <div className="flex items-center gap-2">
                    <input
                      type="password"
                      value={hostToken}
                      onChange={(e) => setHostToken(e.target.value)}
                      placeholder="Bearer token (SAKANA_TOKEN)"
                      className="flex-1 min-w-0 rounded-[6px] border px-2.5 py-1.5 font-mono-tech text-[11.5px]"
                      style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                    />
                    <button type="submit" className="btn-solid px-3 py-1.5 text-[11.5px] font-medium shrink-0 cursor-pointer">
                      Add Host
                    </button>
                  </div>
                </form>

                <div className="space-y-2">
                  {remoteHosts.length === 0 ? (
                    <div className="py-2 text-center text-[11.5px]" style={{ color: 'var(--text-muted)' }}>
                      No remote hosts configured yet.
                    </div>
                  ) : (
                    remoteHosts.map((host) => {
                      const isActive = activeHostId === host.id;
                      return (
                        <div
                          key={host.id}
                          className="rounded-[8px] border p-2.5 flex items-center justify-between gap-2"
                          style={{
                            borderColor: isActive ? 'var(--accent-green)' : 'var(--border-line)',
                            backgroundColor: 'var(--bg-app)',
                          }}
                        >
                          <div className="min-w-0">
                            <div className="flex items-center gap-1.5">
                              <span
                                className="h-1.5 w-1.5 rounded-full shrink-0"
                                style={{
                                  backgroundColor:
                                    host.status === 'online' ? 'var(--accent-green)' : 'var(--text-muted)',
                                }}
                              />
                              <span className="text-[12px] font-semibold truncate" style={{ color: 'var(--text-main)' }}>
                                {host.name}
                              </span>
                              {host.os && (
                                <span
                                  className="font-mono-tech text-[9.5px] px-1 rounded-[4px]"
                                  style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }}
                                >
                                  {host.os}
                                </span>
                              )}
                            </div>
                            <div className="font-mono-tech text-[10.5px] truncate" style={{ color: 'var(--text-muted)' }}>
                              {host.url}
                            </div>
                          </div>
                          <div className="flex items-center gap-1 shrink-0">
                            <button
                              type="button"
                              disabled={testingHostId === host.id}
                              onClick={async () => {
                                setTestingHostId(host.id);
                                try {
                                  const info = await tauriApi.testRemoteHost(host.id);
                                  await onRefreshRemoteHosts();
                                  onToast(`Online: ${info?.hostname || host.name} (${info?.os || 'remote'})`);
                                } catch (err) {
                                  await onRefreshRemoteHosts();
                                  onToast(`Host unreachable: ${String(err)}`);
                                } finally {
                                  setTestingHostId(null);
                                }
                              }}
                              className="btn-ghost px-2 py-1 text-[10.5px] cursor-pointer"
                            >
                              {testingHostId === host.id ? 'Testing…' : 'Test'}
                            </button>
                            <button
                              type="button"
                              onClick={() => onSwitchHost(isActive ? 'local' : host.id)}
                              className={isActive ? 'btn-ghost px-2 py-1 text-[10.5px] cursor-pointer' : 'btn-solid px-2 py-1 text-[10.5px] cursor-pointer'}
                            >
                              {isActive ? 'Disconnect' : 'Connect'}
                            </button>
                            <button
                              type="button"
                              onClick={async () => {
                                if (isActive) onSwitchHost('local');
                                await tauriApi.deleteRemoteHost(host.id);
                                await onRefreshRemoteHosts();
                                onToast(`Removed ${host.name}`);
                              }}
                              className="btn-ghost p-1 cursor-pointer"
                              title="Delete host"
                              aria-label={`Delete host ${host.name}`}
                            >
                              <Trash size={12} />
                            </button>
                          </div>
                        </div>
                      );
                    })
                  )}
                </div>

                <div
                  className="rounded-[7px] border px-2.5 py-2 font-mono-tech text-[10.5px] flex items-center justify-between gap-2"
                  style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)', color: 'var(--text-muted)' }}
                >
                  <span className="truncate">sakana-daemon --port 4317</span>
                  <button
                    type="button"
                    onClick={() => {
                      void navigator.clipboard?.writeText('sakana-daemon --port 4317');
                      onToast('Copied daemon command');
                    }}
                    className="btn-ghost p-1 shrink-0 cursor-pointer"
                    title="Copy daemon command"
                    aria-label="Copy daemon command"
                  >
                    <Copy size={11} />
                  </button>
                </div>
              </div>

              {onUpdateSyncConfig && (
                <div className="sakana-card rounded-[12px] p-5 space-y-3">
                  <div className="flex items-center justify-between gap-2">
                    <h2 className="text-[14.5px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                      Cross-Device Sync (Sessions &amp; Files)
                    </h2>
                    <span
                      className="font-mono-tech text-[10px] px-2 py-0.5 rounded-[5px]"
                      style={
                        syncStatus?.syncing
                          ? { backgroundColor: 'var(--accent-blue-bg)', color: 'var(--accent-blue)' }
                          : syncStatus?.running
                            ? { backgroundColor: 'var(--accent-green-bg)', color: 'var(--accent-green)' }
                            : { backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }
                      }
                    >
                      {syncStatus?.syncing ? 'Syncing…' : syncStatus?.running ? 'Auto-Sync ON' : 'Idle'}
                    </span>
                  </div>
                  <p className="text-[11.5px] m-0 leading-relaxed" style={{ color: 'var(--text-muted)' }}>
                    Automatically mirror chat history and workspace files with another PC running Sakana. Add the second PC in <strong>Remote Sakana Hosts</strong> above, then select it below.
                  </p>

                  {(() => {
                    const cfg: SyncConfig = syncConfig ?? {
                      enabled: false,
                      peerHostId: remoteHosts[0]?.id ?? null,
                      syncSessions: true,
                      syncFiles: true,
                      intervalSecs: 15,
                      conflictPolicy: 'newest-wins',
                    };
                    return (
                      <div className="space-y-2.5">
                        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                          <label className="space-y-1 text-[11px]" style={{ color: 'var(--text-muted)' }}>
                            <span>Peer PC Host</span>
                            <select
                              value={cfg.peerHostId ?? ''}
                              onChange={(e) =>
                                void onUpdateSyncConfig({
                                  ...cfg,
                                  peerHostId: e.target.value || null,
                                })
                              }
                              className="w-full rounded-[6px] border px-2 py-1.5 font-mono-tech text-[11px]"
                              style={{
                                backgroundColor: 'var(--bg-app)',
                                borderColor: 'var(--border-line)',
                                color: 'var(--text-main)',
                              }}
                            >
                              <option value="">— Select a Remote Host —</option>
                              {remoteHosts.map((h) => (
                                <option key={h.id} value={h.id}>
                                  {h.name} ({h.url})
                                </option>
                              ))}
                            </select>
                          </label>
                          <label className="space-y-1 text-[11px]" style={{ color: 'var(--text-muted)' }}>
                            <span>Sync Interval &amp; Conflict Policy</span>
                            <div className="flex items-center gap-1.5">
                              <select
                                value={cfg.intervalSecs}
                                onChange={(e) =>
                                  void onUpdateSyncConfig({
                                    ...cfg,
                                    intervalSecs: Number(e.target.value) || 15,
                                  })
                                }
                                className="w-20 rounded-[6px] border px-2 py-1.5 font-mono-tech text-[11px]"
                                style={{
                                  backgroundColor: 'var(--bg-app)',
                                  borderColor: 'var(--border-line)',
                                  color: 'var(--text-main)',
                                }}
                              >
                                <option value={10}>10s</option>
                                <option value={15}>15s</option>
                                <option value={30}>30s</option>
                                <option value={60}>60s</option>
                              </select>
                              <select
                                value={cfg.conflictPolicy}
                                onChange={(e) =>
                                  void onUpdateSyncConfig({
                                    ...cfg,
                                    conflictPolicy: e.target.value as SyncConfig['conflictPolicy'],
                                  })
                                }
                                className="flex-1 rounded-[6px] border px-2 py-1.5 font-mono-tech text-[11px]"
                                style={{
                                  backgroundColor: 'var(--bg-app)',
                                  borderColor: 'var(--border-line)',
                                  color: 'var(--text-main)',
                                }}
                              >
                                <option value="newest-wins">Newest wins</option>
                                <option value="local-wins">This PC wins</option>
                                <option value="remote-wins">Peer PC wins</option>
                              </select>
                            </div>
                          </label>
                        </div>

                        <div className="flex flex-wrap items-center gap-3 text-[11.5px]">
                          <label className="inline-flex items-center gap-1.5 cursor-pointer select-none">
                            <input
                              type="checkbox"
                              checked={cfg.enabled}
                              disabled={!cfg.peerHostId}
                              onChange={(e) =>
                                void onUpdateSyncConfig({ ...cfg, enabled: e.target.checked })
                              }
                            />
                            <span style={{ color: 'var(--text-main)' }}>Auto-Sync</span>
                          </label>
                          <label className="inline-flex items-center gap-1.5 cursor-pointer select-none">
                            <input
                              type="checkbox"
                              checked={cfg.syncSessions}
                              onChange={(e) =>
                                void onUpdateSyncConfig({ ...cfg, syncSessions: e.target.checked })
                              }
                            />
                            <span style={{ color: 'var(--text-body)' }}>Chat Sessions &amp; History</span>
                          </label>
                          <label className="inline-flex items-center gap-1.5 cursor-pointer select-none">
                            <input
                              type="checkbox"
                              checked={cfg.syncFiles}
                              onChange={(e) =>
                                void onUpdateSyncConfig({ ...cfg, syncFiles: e.target.checked })
                              }
                            />
                            <span style={{ color: 'var(--text-body)' }}>Workspace Files</span>
                          </label>
                        </div>

                        <div className="flex items-center justify-between gap-2 pt-1">
                          <div className="font-mono-tech text-[10.5px] truncate" style={{ color: 'var(--text-muted)' }}>
                            {syncStatus?.lastError ? (
                              <span style={{ color: 'var(--accent-red)' }}>{syncStatus.lastError}</span>
                            ) : syncStatus?.lastSyncAt ? (
                              <span>
                                Last sync: {new Date(syncStatus.lastSyncAt).toLocaleTimeString()} · {syncStatus.sessionsSynced} sessions · ↑{syncStatus.filesPushed} ↓{syncStatus.filesPulled} files
                              </span>
                            ) : (
                              <span>Not synced yet</span>
                            )}
                          </div>
                          {onSyncNow && (
                            <button
                              type="button"
                              disabled={!cfg.peerHostId || Boolean(syncStatus?.syncing)}
                              onClick={() => void onSyncNow()}
                              className="btn-solid px-3 py-1.5 text-[11.5px] font-medium inline-flex items-center gap-1.5 shrink-0 cursor-pointer disabled:opacity-40"
                            >
                              <ArrowClockwise size={12} />
                              <span>{syncStatus?.syncing ? 'Syncing…' : 'Sync Now'}</span>
                            </button>
                          )}
                        </div>
                      </div>
                    );
                  })()}
                </div>
              )}

              <div className="sakana-card rounded-[12px] p-5 flex items-center justify-between">
                <div>
                  <div className="text-[13px] font-semibold" style={{ color: 'var(--text-main)' }}>
                    OS Auto-Start
                  </div>
                  <div className="font-mono-tech text-[10.5px]" style={{ color: 'var(--text-muted)' }}>
                    Start Sakana automatically on boot
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => void onToggleAutostart(!autostartEnabled)}
                  className={autostartEnabled ? 'btn-solid px-3 py-1.5 text-[11.5px] cursor-pointer' : 'btn-ghost px-3 py-1.5 text-[11.5px] cursor-pointer'}
                >
                  {autostartEnabled ? 'Enabled' : 'Disabled'}
                </button>
              </div>
            </div>

            <div className="lg:col-span-7 sakana-card rounded-[12px] p-5 space-y-3.5 h-fit">
              <div className="flex items-center justify-between border-b pb-3" style={{ borderColor: 'var(--border-line)' }}>
                <div className="flex items-center gap-2">
                  <ShieldCheck size={16} style={{ color: 'var(--accent-green)' }} />
                  <h2 className="text-[14.5px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                    .env Secrets
                  </h2>
                </div>
                <span className="font-mono-tech text-[11px]" style={{ color: 'var(--text-muted)' }}>
                  gitignored
                </span>
              </div>

              <form
                onSubmit={async (e) => {
                  e.preventDefault();
                  if (!secretKey.trim() || !secretValue.trim()) return;
                  await onSetSecret(secretKey.trim(), secretValue.trim());
                  setSecretKey('');
                  setSecretValue('');
                }}
                className="grid grid-cols-1 sm:grid-cols-12 gap-2"
              >
                <input
                  value={secretKey}
                  onChange={(e) => setSecretKey(e.target.value)}
                  placeholder="KEY (e.g. OPENROUTER_API_KEY)"
                  className="sm:col-span-5 rounded-[6px] border px-2.5 py-1.5 font-mono-tech text-[11.5px]"
                  style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                />
                <input
                  type="password"
                  value={secretValue}
                  onChange={(e) => setSecretValue(e.target.value)}
                  placeholder="Secret value"
                  className="sm:col-span-5 rounded-[6px] border px-2.5 py-1.5 font-mono-tech text-[11.5px]"
                  style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                />
                <button type="submit" className="sm:col-span-2 btn-solid py-1.5 text-[11.5px] font-medium cursor-pointer">
                  Save
                </button>
              </form>

              <div className="space-y-2.5">
                {secrets.length === 0 ? (
                  <div className="py-4 text-center text-[12px]" style={{ color: 'var(--text-muted)' }}>
                    No .env secrets saved yet.
                  </div>
                ) : (
                secrets.map((sec) => {
                  const revealedValue = revealedSecrets[sec.key];
                  const visible = revealedValue !== undefined;
                  return (
                    <div
                      key={sec.key}
                      className="rounded-[8px] border p-3 flex items-center justify-between gap-3"
                      style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
                    >
                      <div className="min-w-0">
                        <div className="font-mono-tech text-[11.5px] font-semibold" style={{ color: 'var(--text-main)' }}>
                          {sec.key}
                        </div>
                        <div className="font-mono-tech text-[11px] truncate" style={{ color: 'var(--text-muted)' }}>
                          {visible ? revealedValue : sec.value || sec.hint || '••••••••••••••••••••••••••••'}
                        </div>
                      </div>
                      <div className="flex items-center gap-1 shrink-0">
                        <button
                          type="button"
                          onClick={() => void handleToggleRevealSecret(sec)}
                          className="btn-ghost p-1.5 cursor-pointer"
                          title={visible ? 'Hide secret' : 'Reveal secret'}
                          aria-label={visible ? `Hide secret ${sec.key}` : `Reveal secret ${sec.key}`}
                        >
                          {visible ? <EyeSlash size={13} /> : <Eye size={13} />}
                        </button>
                        <button
                          type="button"
                          onClick={() => void onDeleteSecret(sec.key)}
                          className="btn-ghost p-1.5 cursor-pointer"
                          title="Delete secret"
                          aria-label={`Delete secret ${sec.key}`}
                          style={{ color: 'var(--text-muted)' }}
                        >
                          <Trash size={13} />
                        </button>
                      </div>
                    </div>
                  );
                })
                )}
              </div>
            </div>
          </div>
          </div>
        )}
      </div>
    </div>
  );
}
