import { useCallback, useEffect, useRef, useState } from 'react';
import type {
  AppCustomization,
  AttachedContextChip,
  AutomationItem,
  ChatMessage,
  DashboardSubtab,
  DiffFile,
  DiscordStatus,
  DiscordUserRule,
  EnvSecret,
  HarnessAuthAccount,
  HarnessId,
  HarnessModelOption,
  InspectorTab,
  McpServer,
  Office,
  OfficeDelegationEdge,
  PendingApproval,
  PrimarySection,
  Project,
  QueuedPrompt,
  QuotaAccount,
  ReasoningEffort,
  RemoteHost,
  RemoteStatus,
  SkillItem,
  SyncConfig,
  SyncStatus,
  SystemTelemetry,
  ThemeId,
  Thread,
  ToolCall,
  TraceSpan,
  WorkspaceFile,
} from './types';
import { Sidebar } from './components/Sidebar';
import { ChatPane } from './components/ChatPane';
import { RightInspector } from './components/RightInspector';
import { DashboardView } from './components/DashboardView';
import { OfficeView } from './components/OfficeView';
import { ErrorBoundary } from './components/ErrorBoundary';
import { CommandPalette } from './components/CommandPalette';
import { CreatePrModal } from './components/CreatePrModal';
import { RaceModal } from './components/RaceModal';
import { GitHubCloneModal } from './components/GitHubCloneModal';
import { NewWorktreeModal } from './components/NewWorktreeModal';
import { HARNESS_META } from './components/HarnessBadge';
import { getActiveHost, isTauriRuntime, setActiveHost, tauriApi } from './lib/tauriApi';

function formatNowTime(): string {
  const d = new Date();
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

function upsertAssistantMessage(
  threads: Thread[],
  threadId: string,
  messageId: string,
  updater: (msg: ChatMessage) => ChatMessage,
  threadPatch?: Partial<Thread>,
): Thread[] {
  return threads.map((t) => {
    if (t.id !== threadId) return t;
    const exists = t.messages.some((m) => m.id === messageId);
    const messages = exists
      ? t.messages.map((m) => (m.id === messageId ? updater(m) : m))
      : [
          ...t.messages,
          updater({
            id: messageId,
            role: 'assistant',
            timestamp: formatNowTime(),
            text: '',
          }),
        ];
    return {
      ...t,
      ...threadPatch,
      messages,
    };
  });
}

function readPersistedWidth(
  key: string,
  fallback: number,
  min: number,
  max: number,
): number {
  if (typeof window === 'undefined') return fallback;
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return fallback;
    const parsed = Number.parseInt(raw, 10);
    if (!Number.isFinite(parsed)) return fallback;
    return Math.min(max, Math.max(min, parsed));
  } catch {
    return fallback;
  }
}

export default function App() {
  const [theme, setTheme] = useState<ThemeId>('paper');
  const [primarySection, setPrimarySection] = useState<PrimarySection>('chat');
  const [dashSubtab, setDashSubtab] = useState<DashboardSubtab>('overview');

  const [leftOpen, setLeftOpen] = useState<boolean>(() => {
    if (typeof window === 'undefined') return true;
    try {
      const raw = window.localStorage.getItem('sakana_left_open');
      return raw === null ? true : raw === 'true';
    } catch {
      return true;
    }
  });
  const [rightOpen, setRightOpen] = useState<boolean>(() => {
    if (typeof window === 'undefined') return true;
    try {
      const raw = window.localStorage.getItem('sakana_right_open');
      return raw === null ? true : raw === 'true';
    } catch {
      return true;
    }
  });

  useEffect(() => {
    try {
      window.localStorage.setItem('sakana_left_open', String(leftOpen));
    } catch {
      // ignore storage errors
    }
  }, [leftOpen]);

  useEffect(() => {
    try {
      window.localStorage.setItem('sakana_right_open', String(rightOpen));
    } catch {
      // ignore storage errors
    }
  }, [rightOpen]);
  const [rightExpanded, setRightExpanded] = useState(false);
  const [leftWidth, setLeftWidth] = useState<number>(() =>
    readPersistedWidth('sakana_left_width', 272, 220, 340),
  );
  const [rightWidth, setRightWidth] = useState<number>(() =>
    readPersistedWidth('sakana_right_width', 440, 360, 680),
  );
  const [isResizing, setIsResizing] = useState<'left' | 'right' | null>(null);
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>('changes');
  const [splitView, setSplitView] = useState(false);
  const [splitBridgeEnabled, setSplitBridgeEnabled] = useState(true);
  const peerBridgeHopRef = useRef<number>(0);

  const [projects, setProjects] = useState<Project[]>([]);
  const [threads, setThreads] = useState<Thread[]>([]);
  const [selectedThreadId, setSelectedThreadId] = useState<string>('');
  const [secondaryThreadId, setSecondaryThreadId] = useState<string>('');
  const [harnessModels, setHarnessModels] = useState<Record<string, HarnessModelOption[]>>({});
  const [harnessAccounts, setHarnessAccounts] = useState<HarnessAuthAccount[]>([]);
  const [refreshingModels, setRefreshingModels] = useState(false);
  const [disabledHarnesses, setDisabledHarnesses] = useState<HarnessId[]>(() => {
    if (typeof window === 'undefined') return [];
    try {
      const raw = window.localStorage.getItem('sakana_disabled_harnesses');
      if (!raw) return [];
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) return parsed as HarnessId[];
      if (parsed && typeof parsed === 'object') {
        return (Object.keys(parsed) as HarnessId[]).filter((k) => Boolean(parsed[k]));
      }
      return [];
    } catch {
      return [];
    }
  });

  const [diffs, setDiffs] = useState<DiffFile[]>([]);
  const [turnScopeFilter, setTurnScopeFilter] = useState('all');
  const [files, setFiles] = useState<WorkspaceFile[]>([]);
  const [requestedFile, setRequestedFile] = useState<{ path: string; seq: number } | null>(null);
  const [customization, setCustomization] = useState<AppCustomization | null>(null);
  const appliedCustomVarsRef = useRef<string[]>([]);
  const [traces] = useState<TraceSpan[]>([]);

  const [attachedChips, setAttachedChips] = useState<AttachedContextChip[]>([]);
  const [queues, setQueues] = useState<Record<string, QueuedPrompt[]>>({});
  const [raceOpen, setRaceOpen] = useState(false);

  // Dashboard state
  const [quotas, setQuotas] = useState<QuotaAccount[]>([]);
  const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
  const [skills, setSkills] = useState<SkillItem[]>([]);
  const [automations, setAutomations] = useState<AutomationItem[]>([]);
  const [discordUsers, setDiscordUsers] = useState<DiscordUserRule[]>([]);
  const [secrets, setSecrets] = useState<EnvSecret[]>([]);
  const [telemetry, setTelemetry] = useState<SystemTelemetry | null>(null);
  const [discordStatus, setDiscordStatus] = useState<DiscordStatus | null>(null);
  const [remoteStatus, setRemoteStatus] = useState<RemoteStatus | null>(null);
  const [remoteHosts, setRemoteHosts] = useState<RemoteHost[]>([]);
  const [activeHostId, setActiveHostId] = useState<string>('local');
  const [syncConfig, setSyncConfig] = useState<SyncConfig | null>(null);
  const [syncStatus, setSyncStatus] = useState<SyncStatus | null>(null);

  // Office state
  const [offices, setOffices] = useState<Office[]>([]);
  const [selectedOfficeId, setSelectedOfficeId] = useState<string>('');
  const [officeDelegations, setOfficeDelegations] = useState<OfficeDelegationEdge[]>([]);
  const [createOfficeOpen, setCreateOfficeOpen] = useState(false);

  // Modals & Toast
  const [cmdOpen, setCmdOpen] = useState(false);
  const [prOpen, setPrOpen] = useState(false);
  const [newWtOpen, setNewWtOpen] = useState(false);
  const [ghCloneOpen, setGhCloneOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<{
    currentVersion: string;
    latestVersion: string;
    body?: string;
    releaseUrl?: string;
  } | null>(null);
  const [updateDismissed, setUpdateDismissed] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<number | null>(null);
  const [updatePrefsTick, setUpdatePrefsTick] = useState(0);

  useEffect(() => {
    const onPrefsChanged = () => setUpdatePrefsTick((n) => n + 1);
    window.addEventListener('sakana:update-prefs-changed', onPrefsChanged);
    return () => window.removeEventListener('sakana:update-prefs-changed', onPrefsChanged);
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    tauriApi
      .onUpdateProgress((payload) => {
        if (cancelled) return;
        if (payload.total && payload.total > 0) {
          setUpdateProgress(Math.min(100, Math.round((payload.downloaded / payload.total) * 100)));
        }
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});

    const autoEnabled = (() => {
      try {
        return window.localStorage.getItem('sakana_auto_update_enabled') !== 'false';
      } catch {
        return true;
      }
    })();
    const intervalHours = (() => {
      try {
        const raw = Number(window.localStorage.getItem('sakana_auto_update_interval_hours') || '6');
        return Number.isFinite(raw) && raw > 0 ? raw : 6;
      } catch {
        return 6;
      }
    })();

    const checkUpdates = async () => {
      if (!autoEnabled) return;
      try {
        const snoozeUntil = Number(window.localStorage.getItem('sakana_update_snooze_until') || '0');
        if (snoozeUntil > Date.now()) return;
      } catch {
        // ignore storage errors
      }
      try {
        const res = await tauriApi.checkForUpdates();
        if (!cancelled && res.available && res.latestVersion) {
          let skipped = '';
          try {
            skipped = window.localStorage.getItem('sakana_update_skipped_version') || '';
          } catch {
            // ignore
          }
          if (skipped && skipped === res.latestVersion) return;
          setUpdateDismissed(false);
          setAvailableUpdate({
            currentVersion: res.currentVersion,
            latestVersion: res.latestVersion,
            body: res.body ?? undefined,
            releaseUrl: res.releaseUrl ?? undefined,
          });
        }
      } catch {
        // Silent background check; manual check in Settings surfaces errors
      }
    };

    const startupTimer = autoEnabled ? window.setTimeout(checkUpdates, 5000) : undefined;
    const intervalTimer = autoEnabled
      ? window.setInterval(checkUpdates, intervalHours * 60 * 60 * 1000)
      : undefined;

    return () => {
      cancelled = true;
      if (startupTimer !== undefined) window.clearTimeout(startupTimer);
      if (intervalTimer !== undefined) window.clearInterval(intervalTimer);
      unlisten?.();
    };
  }, [updatePrefsTick]);

  const showToast = (msg: string) => {
    const firstLine =
      msg
        .split(/\r?\n/)
        .map((l) => l.trim())
        .find((l) => l.length > 0) ?? '';
    const clamped = firstLine.slice(0, 180);
    setToast(clamped);
    setTimeout(() => setToast((cur) => (cur === clamped ? null : cur)), 2600);
  };

  const bootstrapActiveHost = useCallback(async () => {
    try {
      const state = await tauriApi.getBootstrapState();
      if (!state) return;
      if (state.theme) setTheme(state.theme);
      setProjects(state.projects || []);
      const sessions = state.sessions || [];
      setThreads(sessions);
      setSelectedThreadId((prev) =>
        sessions.some((s) => s.id === prev) ? prev : sessions[0]?.id || '',
      );
      setQuotas(state.accounts ?? state.quotas ?? []);
      setMcpServers(state.mcpServers || []);
      setSkills(state.skills || []);
      setAutomations(state.automations || []);
      setDiscordUsers(state.discordRules ?? state.discordUsers ?? []);
      setSecrets(state.envSecrets ?? state.secrets ?? []);
    } catch (err) {
      console.error('Failed to bootstrap Tauri state:', err);
      showToast(`Bootstrap failed: ${String(err)}`);
    }
  }, []);

  const refreshHarnessAccounts = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const accs = await tauriApi.listHarnessAccounts();
      setHarnessAccounts(accs);
    } catch (err) {
      console.error('list_harness_accounts failed:', err);
    }
  }, []);

  useEffect(() => {
    if (primarySection === 'chat') {
      void refreshHarnessAccounts();
    }
  }, [primarySection, refreshHarnessAccounts]);

  const refreshRemoteHosts = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const hosts = await tauriApi.listRemoteHosts();
      setRemoteHosts(hosts);
    } catch (err) {
      console.error('list_remote_hosts failed:', err);
    }
  }, []);

  const refreshOffices = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const list = await tauriApi.listOffices();
      setOffices(list);
      setSelectedOfficeId((prev) =>
        list.some((o) => o.id === prev) ? prev : list[0]?.id || '',
      );
    } catch (err) {
      console.error('list_offices failed:', err);
      setOffices([]);
      if (getActiveHost() !== 'local') {
        showToast(`Failed to load remote offices: ${String(err)}`);
      }
    }
  }, []);

  const switchHostReqRef = useRef(0);
  const diffsReqRef = useRef(0);
  const queuesRef = useRef(queues);
  queuesRef.current = queues;

  const handleSwitchHost = useCallback(
    async (nextHostId: string) => {
      const reqId = ++switchHostReqRef.current;
      const target = nextHostId || 'local';
      setActiveHost(target);
      setActiveHostId(target);
      setDiffs([]);
      setFiles([]);
      setOffices([]);
      setSelectedOfficeId('');
      setOfficeDelegations([]);
      if (isTauriRuntime()) {
        try {
          if (target === 'local') {
            await tauriApi.disconnectRemoteEvents();
          } else {
            await tauriApi.connectRemoteEvents(target);
          }
        } catch (err) {
          console.error('remote event bridge error:', err);
        }
      }
      if (reqId !== switchHostReqRef.current) return;
      await bootstrapActiveHost();
      if (reqId !== switchHostReqRef.current) return;
      await refreshOffices();
      if (reqId !== switchHostReqRef.current) return;
      void refreshQuotasRef.current(true);
      try {
        const models = await tauriApi.listHarnessModels();
        if (reqId === switchHostReqRef.current && models && Object.keys(models).length > 0) {
          setHarnessModels(models);
        }
      } catch {
        // ignore
      }
      if (reqId !== switchHostReqRef.current) return;
      const label =
        target === 'local'
          ? 'Local Machine'
          : remoteHosts.find((h) => h.id === target)?.name || 'Remote Host';
      showToast(`Switched host: ${label}`);
    },
    [bootstrapActiveHost, refreshOffices, remoteHosts],
  );

  // Bootstrap from Tauri SQLite backend when running inside Tauri v2
  useEffect(() => {
    void bootstrapActiveHost();
    void refreshRemoteHosts();
    void refreshOffices();
    void refreshQuotasRef.current(true);
  }, [bootstrapActiveHost, refreshRemoteHosts, refreshOffices]);

  // Auto-poll usage limits periodically with cooldown on focus/visibility to keep CPU & network low
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let lastFocusRefresh = 0;
    const maybeRefreshOnFocus = () => {
      const now = Date.now();
      if (now - lastFocusRefresh < 60_000) return;
      lastFocusRefresh = now;
      void refreshQuotasRef.current(false);
    };
    const timer = window.setInterval(() => {
      if (document.visibilityState === 'visible') {
        void refreshQuotasRef.current(true);
      }
    }, 300_000);
    const onFocus = () => maybeRefreshOnFocus();
    const onVisibility = () => {
      if (document.visibilityState === 'visible') {
        maybeRefreshOnFocus();
      }
    };
    window.addEventListener('focus', onFocus);
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener('focus', onFocus);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, []);

  const refreshHarnessModels = useCallback(async (refresh = false) => {
    setRefreshingModels(true);
    try {
      const models = await tauriApi.listHarnessModels(refresh);
      if (models && Object.keys(models).length > 0) {
        setHarnessModels(models);
      }
      if (refresh) {
        showToast('Refreshed harness model catalog');
      }
    } catch (err) {
      console.error('list_harness_models failed:', err);
      if (refresh) {
        showToast(`Failed to refresh models: ${String(err)}`);
      }
    } finally {
      setRefreshingModels(false);
    }
  }, []);

  const handleToggleHarness = useCallback((hid: HarnessId) => {
    setDisabledHarnesses((prev) => {
      const isCurrentlyDisabled = prev.includes(hid);
      const next = isCurrentlyDisabled ? prev.filter((h) => h !== hid) : [...prev, hid];
      try {
        window.localStorage.setItem('sakana_disabled_harnesses', JSON.stringify(next));
      } catch {
        // ignore storage errors
      }
      showToast(`${isCurrentlyDisabled ? 'Enabled' : 'Disabled'} ${HARNESS_META[hid].name}`);
      return next;
    });
  }, []);

  useEffect(() => {
    void refreshHarnessModels(false);
  }, [refreshHarnessModels]);

  useEffect(() => {
    try {
      window.localStorage.setItem('sakana_left_width', String(leftWidth));
    } catch {
      // ignore storage errors
    }
  }, [leftWidth]);

  useEffect(() => {
    try {
      window.localStorage.setItem('sakana_right_width', String(rightWidth));
    } catch {
      // ignore storage errors
    }
  }, [rightWidth]);

  useEffect(() => {
    if (!isResizing) return;
    const onMove = (e: MouseEvent) => {
      if (isResizing === 'left') {
        setLeftWidth(Math.min(340, Math.max(220, e.clientX)));
      } else {
        setRightWidth(Math.min(680, Math.max(360, window.innerWidth - e.clientX)));
      }
    };
    const onUp = () => setIsResizing(null);
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
  }, [isResizing]);

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme);
  }, [theme]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setCmdOpen((v) => !v);
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'b') {
        e.preventDefault();
        setLeftOpen((v) => !v);
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'i') {
        e.preventDefault();
        setRightOpen((v) => !v);
      } else if (e.key === 'Escape') {
        setCmdOpen(false);
        setPrOpen(false);
        setNewWtOpen(false);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const applyCustomizationToDom = useCallback((cfg: AppCustomization | null) => {
    if (typeof document === 'undefined') return;
    const root = document.documentElement;
    // Clear previously injected CSS variables
    for (const key of appliedCustomVarsRef.current) {
      root.style.removeProperty(key);
    }
    appliedCustomVarsRef.current = [];

    if (cfg?.theme) {
      setTheme(cfg.theme);
    }
    if (cfg?.cssVars) {
      const nextKeys: string[] = [];
      for (const [k, v] of Object.entries(cfg.cssVars)) {
        if (k.startsWith('--') && typeof v === 'string' && v.trim()) {
          root.style.setProperty(k, v.trim());
          nextKeys.push(k);
        }
      }
      appliedCustomVarsRef.current = nextKeys;
    }

    let styleEl = document.getElementById('sakana-custom-css') as HTMLStyleElement | null;
    const css = cfg?.customCss?.trim() || '';
    if (css) {
      if (!styleEl) {
        styleEl = document.createElement('style');
        styleEl.id = 'sakana-custom-css';
        document.head.appendChild(styleEl);
      }
      styleEl.textContent = css;
    } else if (styleEl) {
      styleEl.remove();
    }

    if (cfg?.uiScale && Number.isFinite(cfg.uiScale) && cfg.uiScale >= 0.75 && cfg.uiScale <= 1.4) {
      root.style.fontSize = `${Math.round(cfg.uiScale * 100)}%`;
    } else {
      root.style.removeProperty('font-size');
    }
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    void tauriApi
      .getAppCustomization()
      .then((cfg) => {
        if (cfg) {
          setCustomization(cfg);
          applyCustomizationToDom(cfg);
        }
      })
      .catch(() => {});
    void tauriApi
      .getSyncConfig()
      .then((cfg) => {
        if (cfg) setSyncConfig(cfg);
      })
      .catch(() => {});
    void tauriApi
      .getSyncStatus()
      .then((st) => {
        if (st) setSyncStatus(st);
      })
      .catch(() => {});
  }, [applyCustomizationToDom]);

  const handleOpenFileInSidebar = useCallback((path: string) => {
    setPrimarySection('chat');
    setRightOpen(true);
    setInspectorTab('files');
    setRequestedFile((prev) => ({ path, seq: (prev?.seq ?? 0) + 1 }));
  }, []);

  const handleSetTheme = (nextTheme: ThemeId) => {
    setTheme(nextTheme);
    showToast(`Theme: ${nextTheme}`);
    void tauriApi.setTheme(nextTheme).catch((err) => {
      console.error('set_theme failed:', err);
    });
  };

  const activeThread: Thread =
    threads.find((t) => t.id === selectedThreadId) ||
    threads[0] || {
      id: 'empty',
      projectId: projects[0]?.id || '',
      title: 'No active thread',
      status: 'IDLE',
      harness: 'agy-cli',
      model: HARNESS_META['agy-cli'].models[0],
      effort: 'medium',
      account: 'default',
      permissionMode: 'yolo',
      worktreePath: '.',
      branch: 'main',
      sharedDeps: [],
      additions: 0,
      deletions: 0,
      timeAgo: 'now',
      messages: [],
    };
  const secondaryThread =
    threads.find((t) => t.id === secondaryThreadId && t.id !== activeThread.id) ||
    threads.find((t) => t.id !== activeThread.id);
  const activeProject = projects.find((p) => p.id === activeThread.projectId);
  const effectiveCwd =
    activeThread.worktreePath && activeThread.worktreePath !== '.'
      ? activeThread.worktreePath
      : activeProject?.cwd || '';

  const refreshDiffs = useCallback(
    async (turnId?: string) => {
      const reqId = ++diffsReqRef.current;
      if (!isTauriRuntime()) return;
      if (!effectiveCwd) {
        setDiffs([]);
        return;
      }
      try {
        const scope = turnId ?? turnScopeFilter;
        const nextDiffs =
          scope && scope !== 'all'
            ? await tauriApi.getCheckpointDiff(activeThread.id, scope)
            : await tauriApi.getGitDiffs(effectiveCwd, undefined, activeProject?.wslDistro);
        if (reqId === diffsReqRef.current && nextDiffs) {
          setDiffs(nextDiffs);
        }
      } catch (err) {
        if (reqId === diffsReqRef.current) {
          console.error('Failed to load git diffs:', err);
        }
      }
    },
    [activeThread.id, effectiveCwd, turnScopeFilter, activeProject?.wslDistro],
  );

  const refreshTelemetry = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const next = await tauriApi.getSystemTelemetry();
      if (next) setTelemetry(next);
    } catch (err) {
      console.error('get_system_telemetry failed:', err);
    }
  }, []);

  const lastQuotaRefreshRef = useRef<number>(0);
  const refreshQuotas = useCallback(async (force = false) => {
    if (!isTauriRuntime()) return;
    const now = Date.now();
    if (!force && now - lastQuotaRefreshRef.current < 8000) return;
    lastQuotaRefreshRef.current = now;
    try {
      const next = await tauriApi.listQuotaAccounts();
      if (next) setQuotas(next);
    } catch (err) {
      console.error('list_quota_accounts failed:', err);
    }
  }, []);

  const refreshQuotasRef = useRef(refreshQuotas);
  refreshQuotasRef.current = refreshQuotas;

  const refreshMcp = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const next = await tauriApi.listMcpServers();
      if (next) setMcpServers(next);
    } catch (err) {
      console.error('list_mcp_servers failed:', err);
    }
  }, []);

  const refreshSkills = useCallback(async () => {
    const cwd = activeProject?.cwd || '.';
    if (!isTauriRuntime()) return;
    try {
      const next = await tauriApi.listSkills(cwd);
      if (next) setSkills(next);
    } catch (err) {
      console.error('list_skills failed:', err);
    }
  }, [activeProject?.cwd]);

  const refreshAutomations = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const next = await tauriApi.listAutomations();
      if (next) setAutomations(next);
    } catch (err) {
      console.error('list_automations failed:', err);
    }
  }, []);

  const refreshDiscordStatus = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const next = await tauriApi.getDiscordStatus();
      if (next) {
        setDiscordStatus(next);
      }
    } catch (err) {
      console.error('get_discord_status failed:', err);
    }
  }, []);

  const refreshRemoteStatus = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const next = await tauriApi.getRemoteStatus();
      if (next) setRemoteStatus(next);
    } catch (err) {
      console.error('get_remote_status failed:', err);
    }
  }, []);

  const refreshSecrets = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const next = await tauriApi.listEnvSecrets();
      if (next) setSecrets(next);
    } catch (err) {
      console.error('list_env_secrets failed:', err);
    }
  }, []);

  useEffect(() => {
    if (primarySection !== 'dashboard' || !isTauriRuntime()) return;
    if (dashSubtab === 'overview') {
      void refreshTelemetry();
      const timer = setInterval(() => {
        if (document.visibilityState === 'visible') {
          void refreshTelemetry();
        }
      }, 15_000);
      return () => clearInterval(timer);
    }
    if (dashSubtab === 'harnesses') {
      void refreshQuotas();
    } else if (dashSubtab === 'mcp') {
      void refreshMcp();
      void refreshSkills();
    } else if (dashSubtab === 'automations') {
      void refreshAutomations();
    } else if (dashSubtab === 'discord') {
      void refreshDiscordStatus();
    } else if (dashSubtab === 'remote') {
      void refreshRemoteStatus();
      void refreshSecrets();
      void refreshTelemetry();
    }
  }, [
    primarySection,
    dashSubtab,
    refreshTelemetry,
    refreshQuotas,
    refreshMcp,
    refreshSkills,
    refreshAutomations,
    refreshDiscordStatus,
    refreshRemoteStatus,
    refreshSecrets,
  ]);

  useEffect(() => {
    if (rightOpen && inspectorTab === 'changes') {
      void refreshDiffs(turnScopeFilter);
    }
  }, [rightOpen, inspectorTab, activeThread.id, refreshDiffs, turnScopeFilter]);

  const selectedThreadIdRef = useRef(selectedThreadId);
  selectedThreadIdRef.current = selectedThreadId;
  const threadsRef = useRef(threads);
  threadsRef.current = threads;
  const projectsRef = useRef(projects);
  projectsRef.current = projects;
  const attachedChipsRef = useRef(attachedChips);
  attachedChipsRef.current = attachedChips;
  const dispatchTurnRef = useRef<(threadId: string, promptText: string) => void>(() => {});

  const splitStateRef = useRef<{ splitView: boolean; splitBridgeEnabled: boolean; primaryId: string; secondaryId?: string }>({
    splitView: false,
    splitBridgeEnabled: true,
    primaryId: '',
    secondaryId: undefined,
  });
  splitStateRef.current = {
    splitView,
    splitBridgeEnabled,
    primaryId: activeThread.id,
    secondaryId: secondaryThread?.id,
  };

  const dispatchTurn = useCallback((threadId: string, promptText: string) => {
    const chips = attachedChipsRef.current;
    const attachmentBlock =
      chips.length > 0
        ? '\n\n' +
          chips
            .map((c) => {
              if (c.kind === 'image' && c.path) return `[Attached image: ${c.path}]`;
              if (c.kind === 'pdf' && c.path) return `[Attached pdf: ${c.path}]`;
              if (c.kind === 'file' && c.path) return `[Attached file: ${c.path}]`;
              return `[Attached context: ${c.label}\n${c.payload}]`;
            })
            .join('\n')
        : '';
    const fullPrompt = promptText + attachmentBlock;
    setAttachedChips([]);

    const userMsgId = `u-${Date.now()}`;
    setThreads((prev) =>
      prev.map((t) =>
        t.id === threadId
          ? {
              ...t,
              status: 'RUNNING',
              timeAgo: 'now',
              messages: [
                ...t.messages,
                {
                  id: userMsgId,
                  role: 'user',
                  timestamp: formatNowTime(),
                  text: fullPrompt,
                },
              ],
            }
          : t,
      ),
    );

    if (isTauriRuntime()) {
      const sp = splitStateRef.current;
      const peerSessionId =
        sp.splitView && sp.splitBridgeEnabled && sp.secondaryId && sp.primaryId !== sp.secondaryId
          ? threadId === sp.primaryId
            ? sp.secondaryId
            : threadId === sp.secondaryId
              ? sp.primaryId
              : undefined
          : undefined;
      void tauriApi
        .sendTurn(threadId, fullPrompt, chips, peerSessionId)
        .then((messageId) => {
          if (!messageId) return;
          setThreads((prev) =>
            upsertAssistantMessage(prev, threadId, messageId, (m) => m, {
              status: 'RUNNING',
            }),
          );
        })
        .catch((err) => {
          console.error('send_turn failed:', err);
          setThreads((prev) =>
            prev.map((t) =>
              t.id === threadId ? { ...t, status: 'IDLE' } : t,
            ),
          );
          showToast(`Turn failed: ${String(err)}`);
        });
      return;
    }

    setTimeout(() => {
      setThreads((prev) =>
        prev.map((t) => {
          if (t.id !== threadId) return t;
          const nextTurnNum = t.messages.filter((m) => m.checkpoint).length + 1;
          return {
            ...t,
            status: 'DONE',
            unreadDone: true,
            additions: t.additions + 18,
            deletions: t.deletions + 4,
            messages: [
              ...t.messages,
              {
                id: `a-${Date.now()}`,
                role: 'assistant',
                timestamp: formatNowTime(),
                thinkingDuration: '1.6s',
                thinking: `Executed in isolated worktree ${t.worktreePath} (${t.branch}).`,
                tools: [
                  {
                    id: `tl-${Date.now()}`,
                    name: 'edit_file',
                    target: 'backend/agent/worktree.py',
                    status: 'ok',
                    durationMs: 74,
                  },
                ],
                text: `Applied changes for **"${promptText.slice(0, 60)}"** inside \`${t.branch}\` and verified tests pass.`,
                checkpoint: {
                  id: `cp-${Date.now()}`,
                  turnNumber: nextTurnNum,
                  commitHash: Math.random().toString(16).slice(2, 9),
                  label: promptText.slice(0, 36),
                  filesChanged: 1,
                  additions: 18,
                  deletions: 4,
                },
              },
            ],
          };
        }),
      );

      // Auto-drain prompt queue if any (outside updater to prevent StrictMode double-dispatch)
      const q = queuesRef.current[threadId] || [];
      if (q.length > 0) {
        const [next, ...rest] = q;
        setQueues((prevQ) => ({ ...prevQ, [threadId]: rest }));
        setTimeout(() => dispatchTurnRef.current(threadId, next.prompt), 250);
      }
    }, 1100);
  }, []);

  dispatchTurnRef.current = dispatchTurn;

  // Load skills for the active project
  useEffect(() => {
    const cwd = activeProject?.cwd;
    if (!cwd || !isTauriRuntime()) return;
    let cancelled = false;
    void tauriApi
      .listSkills(cwd)
      .then((loaded) => {
        if (!cancelled && loaded && loaded.length > 0) {
          setSkills(loaded);
        }
      })
      .catch((err) => {
        console.error('list_skills failed:', err);
      });
    return () => {
      cancelled = true;
    };
  }, [activeProject?.cwd]);

  const pendingTokensRef = useRef<Record<string, Record<string, string>>>({});
  const tokenFlushTimerRef = useRef<number | null>(null);

  // Subscribe to Phase 2 Tauri streaming & Terminal Guard events
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    const flushPendingTokens = () => {
      if (tokenFlushTimerRef.current !== null) {
        window.clearTimeout(tokenFlushTimerRef.current);
        tokenFlushTimerRef.current = null;
      }
      const batch = pendingTokensRef.current;
      const sessionIds = Object.keys(batch);
      if (sessionIds.length === 0) return;
      pendingTokensRef.current = {};
      setThreads((prev) => {
        let next = prev;
        for (const sessionId of sessionIds) {
          const msgMap = batch[sessionId];
          for (const [messageId, chunk] of Object.entries(msgMap)) {
            if (!chunk) continue;
            next = upsertAssistantMessage(
              next,
              sessionId,
              messageId,
              (m) => ({ ...m, text: `${m.text}${chunk}` }),
              { status: 'RUNNING', timeAgo: 'now' },
            );
          }
        }
        return next;
      });
    };

    const register = async () => {
      const fns = await Promise.all([
        tauriApi.onSessionCompacted((res) => {
          if (res.message) {
            setThreads((prev) =>
              prev.map((t) => {
                if (t.id !== res.sessionId) return t;
                if (t.messages.some((m) => m.id === res.message!.id)) return t;
                return { ...t, messages: [...t.messages, res.message!] };
              }),
            );
          }
          if (res.compactedCount > 0) {
            showToast(
              `${res.autoTriggered ? 'Auto-compacted' : 'Compacted'} ${res.compactedCount} older messages into retained memory`,
            );
          }
        }),
        tauriApi.onTurnToken(({ sessionId, messageId, token }) => {
          if (!pendingTokensRef.current[sessionId]) {
            pendingTokensRef.current[sessionId] = {};
          }
          pendingTokensRef.current[sessionId][messageId] =
            (pendingTokensRef.current[sessionId][messageId] || '') + token;
          if (tokenFlushTimerRef.current === null) {
            tokenFlushTimerRef.current = window.setTimeout(flushPendingTokens, 80);
          }
        }),
        tauriApi.onTurnThinking(({ sessionId, messageId, thinking, durationMs }) => {
          setThreads((prev) =>
            upsertAssistantMessage(
              prev,
              sessionId,
              messageId,
              (m) => ({
                ...m,
                thinking: m.thinking ? `${m.thinking}\n${thinking}` : thinking,
                thinkingDuration:
                  durationMs !== undefined
                    ? `${(durationMs / 1000).toFixed(1)}s`
                    : m.thinkingDuration,
              }),
              { status: 'RUNNING' },
            ),
          );
        }),
        tauriApi.onTurnTool(({ sessionId, messageId, tool }) => {
          setThreads((prev) =>
            upsertAssistantMessage(
              prev,
              sessionId,
              messageId,
              (m) => {
                const existing: ToolCall[] = m.tools || [];
                const idx = existing.findIndex((t) => t.id === tool.id);
                const nextTools =
                  idx >= 0
                    ? existing.map((t, i) => (i === idx ? tool : t))
                    : [...existing, tool];
                return { ...m, tools: nextTools };
              },
              { status: 'RUNNING' },
            ),
          );
        }),
        tauriApi.onTurnSubagent(({ sessionId, messageId, subagent }) => {
          setThreads((prev) =>
            upsertAssistantMessage(
              prev,
              sessionId,
              messageId,
              (m) => ({ ...m, subagent }),
              { status: 'RUNNING' },
            ),
          );
        }),
        tauriApi.onTurnDone(({ sessionId, messageId, message, status }) => {
          flushPendingTokens();
          const nextStatus = status || 'DONE';
          const currentThread = threadsRef.current.find((t) => t.id === sessionId);
          const currentProject = currentThread
            ? projectsRef.current.find((p) => p.id === currentThread.projectId)
            : undefined;
          const turnCwd =
            currentThread?.worktreePath && currentThread.worktreePath !== '.'
              ? currentThread.worktreePath
              : currentProject?.cwd || '';
          const nextTurnNumber =
            (currentThread?.messages.filter((m) => m.checkpoint).length ?? 0) + 1;
          const lastUserMsg = currentThread?.messages
            .slice()
            .reverse()
            .find((m) => m.role === 'user');
          const checkpointLabel = (
            lastUserMsg?.text?.trim() ||
            currentThread?.title ||
            `Turn ${nextTurnNumber}`
          ).slice(0, 48);

          setThreads((prev) =>
            upsertAssistantMessage(
              prev,
              sessionId,
              messageId,
              (m) =>
                message
                  ? {
                      ...m,
                      ...message,
                      text: message.text || m.text,
                      timestamp: message.timestamp || m.timestamp,
                    }
                  : m,
              {
                status: nextStatus,
                unreadDone: selectedThreadIdRef.current !== sessionId,
                timeAgo: 'now',
              },
            ),
          );

          if (turnCwd) {
            void tauriApi
              .createCheckpoint(sessionId, nextTurnNumber, checkpointLabel, turnCwd)
              .then((cp) => {
                if (!cp) return;
                setThreads((prev) =>
                  upsertAssistantMessage(prev, sessionId, messageId, (m) =>
                    m.checkpoint ? m : { ...m, checkpoint: cp },
                  ),
                );
              })
              .catch(() => {});
          }

          // Refresh usage limits automatically after a turn finishes
          window.setTimeout(() => void refreshQuotasRef.current(true), 1500);

          // Auto-dispatch next queued prompt for this thread (outside updater to prevent StrictMode double-dispatch)
          const q = queuesRef.current[sessionId] || [];
          if (q.length > 0) {
            const [next, ...rest] = q;
            setQueues((prevQ) => ({ ...prevQ, [sessionId]: rest }));
            setTimeout(() => dispatchTurnRef.current(sessionId, next.prompt), 250);
          }
        }),
        tauriApi.onTurnError(({ sessionId, messageId, error }) => {
          flushPendingTokens();
          setThreads((prev) =>
            upsertAssistantMessage(
              prev,
              sessionId,
              messageId,
              (m) => ({
                ...m,
                text: m.text ? `${m.text}\n\n**Error:** ${error}` : `**Error:** ${error}`,
              }),
              { status: 'IDLE' },
            ),
          );
          showToast(`Turn error: ${error}`);
        }),
        tauriApi.onTurnFailover((ev) => {
          setThreads((prev) =>
            upsertAssistantMessage(
              prev,
              ev.sessionId,
              ev.messageId,
              (m) => ({
                ...m,
                failoverNotice: ev.notice,
              }),
            ),
          );
          showToast(ev.notice);
        }),
        tauriApi.onAutomationRun((updated) => {
          setAutomations((prev) => {
            const exists = prev.some((a) => a.id === updated.id);
            return exists
              ? prev.map((a) => (a.id === updated.id ? updated : a))
              : [updated, ...prev];
          });
          void tauriApi
            .listAutomations()
            .then((next) => {
              if (next) setAutomations(next);
            })
            .catch(() => {});
        }),
        tauriApi.onDiscordPrompt((payload) => {
          void tauriApi
            .getBootstrapState()
            .then((state) => {
              if (!state) return;
              if (state.projects) setProjects(state.projects);
              if (state.sessions) {
                setThreads((prev) => {
                  const prevById = new Map(prev.map((t) => [t.id, t]));
                  return state.sessions.map((s) => {
                    const existing = prevById.get(s.id);
                    if (!existing) return s;
                    // Preserve any live-streamed assistant message (thinking/tools/tokens)
                    // that arrived before the bootstrap snapshot resolved.
                    const dbIds = new Set(s.messages.map((m) => m.id));
                    const inFlight = existing.messages.filter(
                      (m) => m.role === 'assistant' && !dbIds.has(m.id),
                    );
                    if (inFlight.length === 0) {
                      return {
                        ...s,
                        status: existing.status === 'RUNNING' ? 'RUNNING' : s.status,
                      };
                    }
                    return {
                      ...s,
                      status: 'RUNNING',
                      messages: [...s.messages, ...inFlight],
                    };
                  });
                });
              }
            })
            .catch(() => {});
          showToast(`Discord prompt from ${payload.author} in #${payload.channel}`);
        }),
        tauriApi.onOfficeUpdated((updated) => {
          setOffices((prev) => {
            const exists = prev.some((o) => o.id === updated.id);
            return exists
              ? prev.map((o) => (o.id === updated.id ? updated : o))
              : [updated, ...prev];
          });
          setSelectedOfficeId((prev) => prev || updated.id);
        }),
        tauriApi.onOfficeEvent((evt) => {
          setOffices((prev) =>
            prev.map((o) => {
              if (o.id !== evt.officeId) return o;
              const exists = o.events.some((e) => e.id === evt.id);
              return exists ? o : { ...o, events: [...o.events, evt] };
            }),
          );
        }),
        tauriApi.onOfficeDelegation(({ officeId, edges }) => {
          setOfficeDelegations((prev) => [
            ...prev.filter((e) => e.officeId !== officeId),
            ...edges,
          ]);
        }),
        tauriApi.onCustomizationUpdated((cfg) => {
          setCustomization(cfg);
          applyCustomizationToDom(cfg);
        }),
        tauriApi.onAutomationsUpdated((items) => {
          setAutomations(items);
        }),
        tauriApi.onActionsApplied((actions) => {
          if (actions && actions.length > 0) {
            showToast(`Sakana updated: ${actions.join(', ')}`);
          }
        }),
        tauriApi.onNotify(({ title, body }) => {
          showToast(body ? `${title}: ${body}` : title);
        }),
        tauriApi.onPeerMessage((payload) => {
          const sp = splitStateRef.current;
          if (!sp.splitView || !sp.splitBridgeEnabled) return;
          if (peerBridgeHopRef.current >= 6) {
            showToast('Split Bridge paused after 6 consecutive agent turns');
            return;
          }
          peerBridgeHopRef.current += 1;
          const relayPrompt = `[Split-Mode message from ${payload.fromTitle} (${payload.fromHarness})]\n\n${payload.message}${
            payload.expectReply
              ? '\n\n(The peer session is waiting for your response — emit a ```sakana-peer block if you want to reply back to them.)'
              : ''
          }`;
          showToast(`Split Bridge: ${payload.fromHarness} → peer session`);
          setTimeout(() => {
            dispatchTurnRef.current(payload.toSessionId, relayPrompt);
          }, 250);
        }),
        tauriApi.onSyncStatus((st) => {
          setSyncStatus(st);
        }),
        tauriApi.onSyncUpdated(({ newMessages, filesPulled }) => {
          if (newMessages > 0) {
            void tauriApi
              .getBootstrapState()
              .then((state) => {
                if (!state) return;
                if (state.projects) setProjects(state.projects);
                if (state.sessions) {
                  // Preserve any in-flight streaming sessions so sync updates do not wipe live tokens
                  setThreads((prev) => {
                    const runningById = new Map(
                      prev.filter((t) => t.status === 'RUNNING').map((t) => [t.id, t]),
                    );
                    if (runningById.size === 0) return state.sessions;
                    return state.sessions.map((s) => runningById.get(s.id) ?? s);
                  });
                }
              })
              .catch(() => {});
          }
          if (newMessages > 0 || filesPulled > 0) {
            showToast(
              `Cross-Device Sync: +${newMessages} msg${newMessages === 1 ? '' : 's'}, ↓${filesPulled} file${filesPulled === 1 ? '' : 's'}`,
            );
          }
        }),
      ]);

      if (disposed) {
        fns.forEach((fn) => fn());
      } else {
        unlisteners.push(...fns);
      }
    };

    void register();
    return () => {
      disposed = true;
      if (tokenFlushTimerRef.current !== null) {
        window.clearTimeout(tokenFlushTimerRef.current);
        tokenFlushTimerRef.current = null;
      }
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  const handlePickAttachments = async () => {
    if (!isTauriRuntime()) {
      showToast('File attachment picker requires the Sakana desktop runtime');
      return;
    }
    try {
      const items = await tauriApi.pickAttachmentFiles();
      if (!items || items.length === 0) return;
      const newChips: AttachedContextChip[] = items.map((item) => ({
        id: `att-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
        label: item.name,
        payload: item.path,
        path: item.path,
        kind: item.kind,
        sizeBytes: item.sizeBytes,
        previewDataUrl: item.previewDataUrl,
      }));
      setAttachedChips((prev) => [...prev, ...newChips]);
    } catch (err) {
      showToast(`Failed to pick attachments: ${String(err)}`);
    }
  };

  const handleCompactThread = useCallback(async (threadId: string) => {
    if (!isTauriRuntime()) {
      showToast('Compaction requires Sakana Desktop runtime');
      return;
    }
    try {
      const res = await tauriApi.compactSession(threadId);
      if (!res || res.compactedCount === 0) {
        showToast('Thread is already compact (not enough older messages yet)');
      }
    } catch (err) {
      showToast(`Compact failed: ${String(err)}`);
    }
  }, []);

  const handleSendPrompt = (threadId: string, promptText: string) => {
    if (promptText.trim().toLowerCase() === '/compact') {
      void handleCompactThread(threadId);
      return;
    }
    // Reset Split-Mode Peer Bridge hop counter on every human-initiated message
    peerBridgeHopRef.current = 0;
    const target = threads.find((t) => t.id === threadId);
    if (target?.status === 'RUNNING') {
      setQueues((prev) => ({
        ...prev,
        [threadId]: [...(prev[threadId] || []), { id: `q-${Date.now()}`, prompt: promptText }],
      }));
      showToast('Queued prompt — will run when current turn finishes');
      return;
    }
    dispatchTurn(threadId, promptText);
  };

  const handleCreateProject = async (opts: {
    name: string;
    cwd: string;
    runtime?: 'native' | 'wsl';
    wslDistro?: string;
  }) => {
    try {
      const created = await tauriApi.createProject(opts);
      const proj: Project = created ?? {
        id: `proj-${Date.now()}`,
        name: opts.name,
        cwd: opts.cwd,
        runtime: opts.runtime,
        wslDistro: opts.wslDistro,
      };
      setProjects((prev) => [...prev, proj]);
      showToast(`Added project ${proj.name}`);
    } catch (err) {
      showToast(`Failed to create project: ${String(err)}`);
    }
  };

  const handleDeleteProject = async (id: string) => {
    try {
      await tauriApi.deleteProject(id);
      setProjects((prev) => prev.filter((p) => p.id !== id));
      setThreads((prev) => prev.filter((t) => t.projectId !== id));
      showToast('Deleted project');
    } catch (err) {
      showToast(`Failed to delete project: ${String(err)}`);
    }
  };

  const handleCreateWorktree = async (opts: {
    projectId: string;
    title: string;
    branch: string;
    harness: HarnessId;
    model?: string;
    effort?: ReasoningEffort;
    sharedDeps: string[];
  }) => {
    const defaultModel =
      opts.model ||
      harnessModels[opts.harness]?.[0]?.id ||
      HARNESS_META[opts.harness]?.models[0] ||
      'gemini-4-hydrogen';
    const modelEntry = harnessModels[opts.harness]?.find((m) => m.id === defaultModel);
    const validEfforts = modelEntry
      ? modelEntry.efforts.filter((e): e is ReasoningEffort =>
          ['low', 'medium', 'high', 'xhigh'].includes(e),
        )
      : (['low', 'medium', 'high', 'xhigh'] as ReasoningEffort[]);
    const defaultEffort: ReasoningEffort =
      opts.effort !== undefined
        ? opts.effort
        : validEfforts.length === 0
          ? ''
          : validEfforts.includes('medium')
            ? 'medium'
            : validEfforts[0];
    const fallbackWtPath = `.sakana/worktrees/${opts.branch.replace(/[^a-z0-9-]/gi, '-')}`;
    const project = projects.find((p) => p.id === opts.projectId);

    const created = await tauriApi.createSession({
      projectId: opts.projectId,
      title: opts.title,
      branch: opts.branch,
      harness: opts.harness,
      model: defaultModel,
      effort: defaultEffort,
      account: 'default',
      permissionMode: 'yolo',
      worktreePath: fallbackWtPath,
      sharedDeps: opts.sharedDeps,
    });

    let actualWorktreePath = created?.worktreePath || fallbackWtPath;
    if (isTauriRuntime() && created && project) {
      try {
        const wtPath = await tauriApi.createWorktree(
          created.id,
          project.cwd,
          opts.branch,
          opts.sharedDeps,
        );
        if (wtPath) {
          actualWorktreePath = wtPath;
        }
      } catch (err) {
        showToast(`Worktree git checkout warning: ${String(err)}`);
      }
    }

    const newThread: Thread = created
      ? { ...created, worktreePath: actualWorktreePath }
      : {
          id: `thr-${Date.now()}`,
          projectId: opts.projectId,
          title: opts.title,
          status: 'IDLE',
          harness: opts.harness,
          model: defaultModel,
          effort: defaultEffort,
          account: 'default',
          permissionMode: 'yolo',
          worktreePath: actualWorktreePath,
          branch: opts.branch,
          sharedDeps: opts.sharedDeps,
          additions: 0,
          deletions: 0,
          timeAgo: 'now',
          messages: [],
        };
    setThreads((prev) => [newThread, ...prev]);
    setSelectedThreadId(newThread.id);
    setPrimarySection('chat');
    showToast(`Spawned worktree ${opts.branch}`);
  };

  const handleQuickNewThread = async (projectId?: string) => {
    const targetProject =
      (projectId ? projects.find((p) => p.id === projectId) : undefined) ||
      activeProject ||
      projects[0];
    if (!targetProject) {
      showToast('Add a project folder first');
      return;
    }
    const harness: HarnessId = activeThread.id !== 'empty' ? activeThread.harness : 'agy-cli';
    const defaultModel =
      (activeThread.id !== 'empty' && activeThread.model) ||
      harnessModels[harness]?.[0]?.id ||
      HARNESS_META[harness]?.models[0] ||
      'gemini-4-hydrogen';
    const modelEntry = harnessModels[harness]?.find((m) => m.id === defaultModel);
    const validEfforts = modelEntry
      ? modelEntry.efforts.filter((e): e is ReasoningEffort =>
          ['low', 'medium', 'high', 'xhigh'].includes(e),
        )
      : (['low', 'medium', 'high', 'xhigh'] as ReasoningEffort[]);
    const defaultEffort: ReasoningEffort =
      activeThread.id !== 'empty'
        ? activeThread.effort
        : validEfforts.length === 0
          ? ''
          : validEfforts.includes('medium')
            ? 'medium'
            : validEfforts[0];
    const branch = activeThread.id !== 'empty' && activeThread.branch ? activeThread.branch : 'main';
    const title = `New thread`;

    try {
      const created = await tauriApi.createSession({
        projectId: targetProject.id,
        title,
        branch,
        harness,
        model: defaultModel,
        effort: defaultEffort,
        account: 'default',
        permissionMode: 'yolo',
        worktreePath: targetProject.cwd,
        sharedDeps: [],
      });

      const newThread: Thread = created ?? {
        id: `thr-${Date.now()}`,
        projectId: targetProject.id,
        title,
        status: 'IDLE',
        harness,
        model: defaultModel,
        effort: defaultEffort,
        account: 'default',
        permissionMode: 'yolo',
        worktreePath: targetProject.cwd,
        branch,
        sharedDeps: [],
        additions: 0,
        deletions: 0,
        timeAgo: 'now',
        messages: [],
      };
      setThreads((prev) => [newThread, ...prev]);
      if (splitView && selectedThreadId) {
        setSecondaryThreadId(newThread.id);
      } else {
        setSelectedThreadId(newThread.id);
      }
      setPrimarySection('chat');
    } catch (err) {
      showToast(`Failed to create thread: ${String(err)}`);
    }
  };

  const handleDeleteThread = async (id: string) => {
    try {
      const target = threads.find((t) => t.id === id);
      const project = target ? projects.find((p) => p.id === target.projectId) : undefined;
      if (
        isTauriRuntime() &&
        target &&
        project?.cwd &&
        target.worktreePath &&
        target.worktreePath !== '.' &&
        target.worktreePath !== project.cwd
      ) {
        await tauriApi.removeWorktree(id, project.cwd).catch(() => {});
      }
      await tauriApi.deleteSession(id);
      setThreads((prev) => {
        const remaining = prev.filter((t) => t.id !== id);
        if (selectedThreadId === id) {
          setSelectedThreadId(remaining[0]?.id || '');
        }
        return remaining;
      });
      showToast('Deleted thread');
    } catch (err) {
      showToast(`Failed to delete thread: ${String(err)}`);
    }
  };

  const handleUpdateThreadConfig = (
    id: string,
    patch: Partial<Pick<Thread, 'harness' | 'model' | 'effort' | 'account'>>,
  ) => {
    setThreads((prev) => prev.map((t) => (t.id === id ? { ...t, ...patch } : t)));
    void tauriApi.updateSession({ id, ...patch }).catch((err) => {
      console.error('update_session failed:', err);
    });
  };

  const handleSetProjectDiscordChannel = async (projectId: string, channel?: string) => {
    const normalized = channel?.trim()
      ? channel.trim().startsWith('#')
        ? channel.trim()
        : `#${channel.trim()}`
      : undefined;
    if (isTauriRuntime()) {
      try {
        await tauriApi.setProjectDiscordChannel(projectId, normalized ?? null);
      } catch (err) {
        showToast(`Failed to update Discord channel: ${String(err)}`);
        return;
      }
    }
    setProjects((prev) =>
      prev.map((p) => (p.id === projectId ? { ...p, discordChannel: normalized } : p)),
    );
    showToast(
      normalized
        ? `Bound project to Discord ${normalized}`
        : 'Cleared project Discord channel',
    );
  };

  const handleSetThreadDiscordChannel = async (threadId: string, channel?: string | null) => {
    const normalized = channel?.trim()
      ? channel.trim().startsWith('#')
        ? channel.trim()
        : `#${channel.trim()}`
      : undefined;
    if (isTauriRuntime()) {
      try {
        await tauriApi.setSessionDiscordChannel(threadId, normalized ?? null);
      } catch (err) {
        showToast(`Failed to update thread Discord channel: ${String(err)}`);
        return;
      }
    }
    setThreads((prev) =>
      prev.map((t) =>
        t.id === threadId
          ? { ...t, isDiscord: Boolean(normalized), discordChannel: normalized }
          : t,
      ),
    );
    showToast(
      normalized
        ? `Linked thread to Discord ${normalized}`
        : 'Disconnected thread from Discord',
    );
  };

  const handleRenameThread = async (id: string, title: string) => {
    const trimmed = title.trim();
    if (!trimmed) return;
    setThreads((prev) => prev.map((t) => (t.id === id ? { ...t, title: trimmed } : t)));
    try {
      await tauriApi.updateSession({ id, title: trimmed });
      showToast('Renamed thread');
    } catch (err) {
      showToast(`Failed to rename thread: ${String(err)}`);
    }
  };

  const handleMoveThread = async (id: string, projectId: string) => {
    const targetProject = projects.find((p) => p.id === projectId);
    setThreads((prev) => prev.map((t) => (t.id === id ? { ...t, projectId } : t)));
    try {
      await tauriApi.updateSession({ id, projectId });
      showToast(targetProject ? `Moved thread to ${targetProject.name}` : 'Moved thread');
    } catch (err) {
      showToast(`Failed to move thread: ${String(err)}`);
    }
  };

  const handleRenameProject = async (id: string, name: string) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    setProjects((prev) => prev.map((p) => (p.id === id ? { ...p, name: trimmed } : p)));
    try {
      await tauriApi.renameProject(id, trimmed);
      showToast(`Renamed project to "${trimmed}"`);
    } catch (err) {
      showToast(`Failed to rename project: ${String(err)}`);
    }
  };

  const handleReorderProjects = async (orderedIds: string[]) => {
    setProjects((prev) => {
      const byId = new Map(prev.map((p) => [p.id, p]));
      const next: Project[] = [];
      for (const id of orderedIds) {
        const p = byId.get(id);
        if (p) {
          next.push(p);
          byId.delete(id);
        }
      }
      for (const p of byId.values()) next.push(p);
      return next;
    });
    try {
      await tauriApi.reorderProjects(orderedIds);
    } catch (err) {
      showToast(`Failed to reorder projects: ${String(err)}`);
    }
  };

  const handleReorderThreads = async (
    orderedIds: string[],
    moved?: { id: string; projectId: string },
  ) => {
    setThreads((prev) => {
      const updated = moved
        ? prev.map((t) => (t.id === moved.id ? { ...t, projectId: moved.projectId } : t))
        : prev;
      const byId = new Map(updated.map((t) => [t.id, t]));
      const next: Thread[] = [];
      for (const id of orderedIds) {
        const t = byId.get(id);
        if (t) {
          next.push(t);
          byId.delete(id);
        }
      }
      for (const t of byId.values()) next.push(t);
      return next;
    });
    try {
      if (moved) {
        await tauriApi.updateSession({ id: moved.id, projectId: moved.projectId });
      }
      await tauriApi.reorderSessions(orderedIds);
    } catch (err) {
      showToast(`Failed to reorder threads: ${String(err)}`);
    }
  };

  const handleCreateAutomation = async (
    name: string,
    kind: 'cron' | 'webhook',
    schedule: string,
    prompt: string,
  ) => {
    if (isTauriRuntime()) {
      try {
        const created = await tauriApi.createAutomation(name, kind, schedule, prompt);
        if (created) setAutomations((prev) => [created, ...prev]);
        showToast(`Created automation ${name}`);
      } catch (err) {
        showToast(`Create automation failed: ${String(err)}`);
      }
      return;
    }
    setAutomations((prev) => [
      {
        id: `auto-${Date.now()}`,
        name,
        kind,
        schedule,
        prompt,
        enabled: true,
        lastRunStatus: 'OK',
        history: [],
      },
      ...prev,
    ]);
    showToast(`Created automation ${name}`);
  };

  const handleToggleAutomation = async (id: string, enabled: boolean) => {
    if (isTauriRuntime()) {
      try {
        const updated = await tauriApi.toggleAutomation(id, enabled);
        setAutomations((prev) =>
          prev.map((a) => (a.id === id ? updated ?? { ...a, enabled } : a)),
        );
      } catch (err) {
        showToast(`Toggle automation failed: ${String(err)}`);
      }
      return;
    }
    setAutomations((prev) => prev.map((a) => (a.id === id ? { ...a, enabled } : a)));
  };

  const handleRunAutomation = async (id: string) => {
    if (isTauriRuntime()) {
      try {
        const updated = await tauriApi.runAutomationNow(id);
        if (updated) {
          setAutomations((prev) => prev.map((a) => (a.id === id ? updated : a)));
        }
        showToast('Automation executed');
      } catch (err) {
        showToast(`Automation run failed: ${String(err)}`);
      }
      return;
    }
    setAutomations((prev) =>
      prev.map((a) =>
        a.id === id
          ? {
              ...a,
              history: [
                {
                  ts: 'Just now',
                  status: 'OK',
                  durationMs: 1120,
                  summary: 'Manual trigger completed.',
                },
                ...a.history,
              ],
            }
          : a,
      ),
    );
    showToast('Automation executed');
  };

  return (
    <div className={`h-[100dvh] w-screen flex overflow-hidden ${isResizing ? 'is-resizing' : ''}`}>
      <ErrorBoundary paneName="Sidebar">
        <Sidebar
          open={leftOpen}
        width={leftWidth}
        onToggleOpen={() => setLeftOpen((v) => !v)}
        primarySection={primarySection}
        onSwitchSection={setPrimarySection}
        projects={projects}
        threads={threads}
        selectedThreadId={activeThread.id}
        onSelectThread={(id) => {
          setSelectedThreadId(id);
          setPrimarySection('chat');
          setThreads((prev) => prev.map((t) => (t.id === id ? { ...t, unreadDone: false } : t)));
        }}
        onQuickNewThread={handleQuickNewThread}
        onNewWorktreeThread={() => setNewWtOpen(true)}
        onCreateProject={handleCreateProject}
        onDeleteProject={handleDeleteProject}
        onRenameProject={handleRenameProject}
        onReorderProjects={handleReorderProjects}
        onSetProjectDiscordChannel={handleSetProjectDiscordChannel}
        onDeleteThread={handleDeleteThread}
        onRenameThread={handleRenameThread}
        onMoveThread={handleMoveThread}
        onSetThreadDiscordChannel={handleSetThreadDiscordChannel}
        onReorderThreads={handleReorderThreads}
        onOpenGitHubClone={() => setGhCloneOpen(true)}
        dashSubtab={dashSubtab}
        onSelectDashSubtab={(tab) => {
          setDashSubtab(tab);
          setPrimarySection('dashboard');
        }}
        automations={automations}
        onCreateAutomation={handleCreateAutomation}
        onToggleAutomation={handleToggleAutomation}
        onRunAutomation={handleRunAutomation}
        quotas={quotas}
        onRefreshQuotas={() => void refreshQuotas(true)}
        remoteHosts={remoteHosts}
        activeHostId={activeHostId}
        onSwitchHost={(hid) => void handleSwitchHost(hid)}
        onManageHosts={() => {
          setDashSubtab('remote');
          setPrimarySection('dashboard');
        }}
        offices={offices}
        selectedOfficeId={selectedOfficeId}
        onSelectOffice={(id) => {
          setSelectedOfficeId(id);
          setPrimarySection('office');
        }}
        onNewOffice={() => {
          setPrimarySection('office');
          setCreateOfficeOpen(true);
        }}
        onDeleteOffice={async (id) => {
          try {
            if (isTauriRuntime()) {
              await tauriApi.deleteOffice(id);
            }
            setOffices((prev) => {
              const remaining = prev.filter((o) => o.id !== id);
              if (selectedOfficeId === id) {
                setSelectedOfficeId(remaining[0]?.id || '');
              }
              return remaining;
            });
            setOfficeDelegations((prev) => prev.filter((d) => d.officeId !== id));
            showToast('Deleted office');
          } catch (err) {
            showToast(`Failed to delete office: ${String(err)}`);
          }
        }}
      />
      </ErrorBoundary>

      {leftOpen && (
        <div
          className={`pane-resizer ${isResizing === 'left' ? 'is-dragging' : ''}`}
          tabIndex={0}
          onMouseDown={(e) => {
            e.preventDefault();
            setIsResizing('left');
          }}
          onKeyDown={(e) => {
            if (e.key === 'ArrowLeft') {
              e.preventDefault();
              setLeftWidth((w) => Math.max(220, w - 16));
            } else if (e.key === 'ArrowRight') {
              e.preventDefault();
              setLeftWidth((w) => Math.min(340, w + 16));
            }
          }}
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize left sidebar"
          aria-valuenow={leftWidth}
          aria-valuemin={220}
          aria-valuemax={340}
        />
      )}

      {primarySection === 'chat' ? (
        <>
          <ErrorBoundary paneName="Chat">
            <ChatPane
              thread={activeThread}
            secondaryThread={secondaryThread}
            secondaryThreadId={secondaryThread?.id}
            onSelectSecondaryThread={setSecondaryThreadId}
            onQuickNewThread={() => void handleQuickNewThread()}
            project={activeProject}
            allThreads={threads}
            splitView={splitView}
            onToggleSplitView={() => setSplitView((v) => !v)}
            leftSidebarOpen={leftOpen}
            onToggleLeftSidebar={() => setLeftOpen((v) => !v)}
            rightInspectorOpen={rightOpen}
            onToggleRightInspector={() => setRightOpen((v) => !v)}
            onSelectThread={(id) => setSelectedThreadId(id)}
            onSendPrompt={handleSendPrompt}
            onCompactThread={(id) => void handleCompactThread(id)}
            onStopStreaming={(id) => {
              void tauriApi.cancelTurn(id).catch((err) => {
                console.error('cancel_turn failed:', err);
              });
              setThreads((prev) => prev.map((t) => (t.id === id ? { ...t, status: 'IDLE' } : t)));
              showToast('Stopped active turn');
            }}
            onUpdateThreadConfig={handleUpdateThreadConfig}
            skills={skills}
            theme={theme}
            onRewindCheckpoint={async (threadId, checkpointId) => {
              if (!isTauriRuntime()) return;
              try {
                const res = await tauriApi.rewindCheckpoint(threadId, checkpointId, true, true);
                const refreshed = await tauriApi.listSessions();
                if (refreshed?.length) {
                  setThreads(refreshed);
                }
                await refreshDiffs('all');
                if (res) {
                  const stashMsg = res.stashed ? ` (stashed ${res.stashRef || 'changes'})` : '';
                  showToast(`Rewound ${res.removedMessages} msgs${stashMsg}`);
                }
              } catch (err) {
                showToast(`Rewind failed: ${String(err)}`);
              }
            }}
            onForkFromCheckpoint={async (threadId, checkpointId) => {
              if (!isTauriRuntime()) return;
              try {
                const forked = await tauriApi.forkFromCheckpoint(threadId, checkpointId);
                if (forked) {
                  setThreads((prev) => [forked, ...prev]);
                  setSelectedThreadId(forked.id);
                  showToast(`Forked thread: ${forked.title}`);
                }
              } catch (err) {
                showToast(`Fork failed: ${String(err)}`);
              }
            }}
            onRevertToCheckpoint={async (threadId, checkpointId) => {
              if (isTauriRuntime()) {
                try {
                  await tauriApi.revertToCheckpoint(threadId, checkpointId);
                  const refreshed = await tauriApi.listSessions();
                  if (refreshed?.length) {
                    setThreads(refreshed);
                  }
                  await refreshDiffs('all');
                  showToast('Reverted workspace & chat to checkpoint');
                } catch (err) {
                  showToast(`Revert failed: ${String(err)}`);
                }
                return;
              }
              setThreads((prev) =>
                prev.map((t) => {
                  if (t.id !== threadId) return t;
                  const idx = t.messages.findIndex((m) => m.checkpoint?.id === checkpointId);
                  if (idx < 0) return t;
                  return { ...t, messages: t.messages.slice(0, idx + 1) };
                }),
              );
              showToast('Reverted workspace & chat to checkpoint');
            }}
            onViewCheckpointDiff={(checkpointId) => {
              setRightOpen(true);
              setInspectorTab('changes');
              setTurnScopeFilter(checkpointId);
              void refreshDiffs(checkpointId);
            }}
            onForkAtMessage={(threadId, msgIndex) => {
              const src = threads.find((t) => t.id === threadId);
              if (!src) return;
              const slicedMessages = src.messages.slice(0, msgIndex + 1);
              const forkTitle = `${src.title} (Fork)`;
              const forkBranch = `${src.branch}-fork`;
              if (isTauriRuntime()) {
                void tauriApi
                  .forkSession({
                    sourceId: src.id,
                    title: forkTitle,
                    branch: forkBranch,
                    upToIndex: msgIndex,
                  })
                  .then((created) => {
                    const forked: Thread = created
                      ? created
                      : {
                          ...src,
                          id: `fork-${Date.now()}`,
                          title: forkTitle,
                          branch: forkBranch,
                          status: 'IDLE',
                          messages: slicedMessages,
                        };
                    setThreads((prev) => [forked, ...prev]);
                    setSelectedThreadId(forked.id);
                    showToast('Forked thread into new session');
                  })
                  .catch((err) => {
                    showToast(`Failed to fork thread: ${String(err)}`);
                  });
                return;
              }
              const forkId = `fork-${Date.now()}`;
              const forked: Thread = {
                ...src,
                id: forkId,
                title: forkTitle,
                branch: forkBranch,
                status: 'IDLE',
                messages: slicedMessages,
              };
              setThreads((prev) => [forked, ...prev]);
              setSelectedThreadId(forkId);
              showToast('Forked thread into new worktree');
            }}
            attachedChips={attachedChips}
            onAddChips={(chips) => setAttachedChips((prev) => [...prev, ...chips])}
            onPickAttachments={handlePickAttachments}
            onRemoveChip={(id) => setAttachedChips((prev) => prev.filter((c) => c.id !== id))}
            queuedPrompts={queues[activeThread.id] || []}
            onSendQueuedNow={(qid) => {
              const item = (queues[activeThread.id] || []).find((q) => q.id === qid);
              if (!item) return;
              setQueues((prev) => ({
                ...prev,
                [activeThread.id]: (prev[activeThread.id] || []).filter((q) => q.id !== qid),
              }));
              dispatchTurn(activeThread.id, item.prompt);
            }}
            onRemoveQueued={(qid) =>
              setQueues((prev) => ({
                ...prev,
                [activeThread.id]: (prev[activeThread.id] || []).filter((q) => q.id !== qid),
              }))
            }
            onOpenCommandPalette={() => setCmdOpen(true)}
            harnessModels={harnessModels}
            harnessAccounts={harnessAccounts}
            onOpenHarnessAccounts={() => {
              setDashSubtab('harnesses');
              setPrimarySection('dashboard');
            }}
            disabledHarnesses={disabledHarnesses}
            onToast={showToast}
            onOpenFile={handleOpenFileInSidebar}
            splitBridgeEnabled={splitBridgeEnabled}
            onToggleSplitBridge={() => {
              setSplitBridgeEnabled((v) => {
                const next = !v;
                peerBridgeHopRef.current = 0;
                showToast(next ? 'Split-Mode Agent Bridge enabled' : 'Split-Mode Agent Bridge paused');
                return next;
              });
            }}
          />
          </ErrorBoundary>

          {rightOpen && !rightExpanded && (
            <div
              className={`pane-resizer ${isResizing === 'right' ? 'is-dragging' : ''}`}
              tabIndex={0}
              onMouseDown={(e) => {
                e.preventDefault();
                setIsResizing('right');
              }}
              onKeyDown={(e) => {
                if (e.key === 'ArrowLeft') {
                  e.preventDefault();
                  setRightWidth((w) => Math.min(680, w + 16));
                } else if (e.key === 'ArrowRight') {
                  e.preventDefault();
                  setRightWidth((w) => Math.max(360, w - 16));
                }
              }}
              role="separator"
              aria-orientation="vertical"
              aria-label="Resize right inspector"
              aria-valuenow={rightWidth}
              aria-valuemin={360}
              aria-valuemax={680}
            />
          )}

          <ErrorBoundary paneName="Inspector">
            <RightInspector
              open={rightOpen}
            width={rightWidth}
            expanded={rightExpanded}
            onToggleExpand={() => setRightExpanded((v) => !v)}
            onClose={() => setRightOpen(false)}
            activeTab={inspectorTab}
            onSelectTab={setInspectorTab}
            thread={activeThread}
            threads={threads}
            onSelectThread={(id) => {
              setSelectedThreadId(id);
              setPrimarySection('chat');
            }}
            projectCwd={activeProject?.cwd}
            wslDistro={activeProject?.wslDistro}
            theme={theme}
            diffs={diffs}
            onRefreshDiffs={refreshDiffs}
            onToggleStageHunk={async (filePath, hunkId, hunkHeader, nextStaged) => {
              if (isTauriRuntime()) {
                try {
                  await tauriApi.stageHunk(effectiveCwd, filePath, hunkHeader, nextStaged, activeProject?.wslDistro);
                  await refreshDiffs(turnScopeFilter);
                  return;
                } catch (err) {
                  showToast(`Stage hunk failed: ${String(err)}`);
                  return;
                }
              }
              setDiffs((prev) =>
                prev.map((df) =>
                  df.path === filePath
                    ? {
                        ...df,
                        hunks: df.hunks.map((h) =>
                          h.id === hunkId ? { ...h, staged: nextStaged } : h,
                        ),
                      }
                    : df,
                ),
              );
            }}
            onDiscardHunk={async (filePath, hunkId, hunkHeader) => {
              if (isTauriRuntime()) {
                try {
                  await tauriApi.discardHunk(effectiveCwd, filePath, hunkHeader, activeProject?.wslDistro);
                  await refreshDiffs(turnScopeFilter);
                  showToast(`Discarded hunk in ${filePath}`);
                  return;
                } catch (err) {
                  showToast(`Discard hunk failed: ${String(err)}`);
                  return;
                }
              }
              setDiffs((prev) =>
                prev.map((df) =>
                  df.path === filePath
                    ? {
                        ...df,
                        hunks: df.hunks.map((h) =>
                          h.id === hunkId ? { ...h, discarded: true } : h,
                        ),
                      }
                    : df,
                ),
              );
              showToast(`Discarded hunk in ${filePath}`);
            }}
            onBranchChanged={(nextBranch) => {
              setThreads((prev) =>
                prev.map((t) =>
                  t.id === activeThread.id ? { ...t, branch: nextBranch } : t,
                ),
              );
            }}
            onOpenCreatePr={() => setPrOpen(true)}
            turnScopeFilter={turnScopeFilter}
            onChangeTurnScope={setTurnScopeFilter}
            files={files}
            onSaveFile={async (path, content) => {
              if (isTauriRuntime()) {
                await tauriApi.writeWorkspaceFile(effectiveCwd, path, content, activeProject?.wslDistro);
              } else {
                setFiles((prev) =>
                  prev.map((f) => (f.path === path ? { ...f, lines: content.split('\n') } : f)),
                );
              }
              showToast(`Saved ${path}`);
            }}
            traces={traces}
            onAttachChip={(chip) => {
              setAttachedChips((prev) => [...prev, chip]);
              showToast(`Attached ${chip.label} to composer`);
            }}
            onToast={showToast}
            requestedFile={requestedFile}
          />
          </ErrorBoundary>
        </>
      ) : primarySection === 'office' ? (
        <ErrorBoundary paneName="Office">
          <OfficeView
            offices={offices}
            selectedOffice={
              offices.find((o) => o.id === selectedOfficeId) ?? offices[0] ?? null
            }
            projects={projects}
            harnessModels={harnessModels}
            delegations={officeDelegations.filter(
              (d) =>
                d.officeId ===
                (offices.find((o) => o.id === selectedOfficeId)?.id ??
                  offices[0]?.id ??
                  ''),
            )}
            onSelectOffice={(id: string) => setSelectedOfficeId(id)}
            onRefreshOffices={refreshOffices}
            onToast={showToast}
            createModalOpen={createOfficeOpen}
            onSetCreateModalOpen={setCreateOfficeOpen}
            sidebarOpen={leftOpen}
            onToggleSidebar={() => setLeftOpen((v) => !v)}
          />
        </ErrorBoundary>
      ) : (
        <ErrorBoundary paneName="Dashboard">
          <DashboardView
            subtab={dashSubtab}
            harnessModels={harnessModels}
          theme={theme}
          onSetTheme={handleSetTheme}
          leftSidebarOpen={leftOpen}
          onToggleLeftSidebar={() => setLeftOpen((v) => !v)}
          onReturnToChat={() => setPrimarySection('chat')}
          threads={threads}
          onSelectThread={(id) => {
            setSelectedThreadId(id);
            setPrimarySection('chat');
          }}
          telemetry={telemetry}
          onRefreshTelemetry={() => void refreshTelemetry()}
          onToggleAutostart={async (enabled) => {
            if (isTauriRuntime()) {
              try {
                const next = await tauriApi.setAutostart(enabled);
                setTelemetry((prev) =>
                  prev ? { ...prev, autostartEnabled: next ?? enabled } : prev,
                );
                showToast(`OS auto-start ${enabled ? 'enabled' : 'disabled'}`);
              } catch (err) {
                showToast(`Failed to update auto-start: ${String(err)}`);
              }
              return;
            }
            setTelemetry((prev) =>
              prev ? { ...prev, autostartEnabled: enabled } : prev,
            );
            showToast(`OS auto-start ${enabled ? 'enabled' : 'disabled'}`);
          }}
          quotas={quotas}
          onRefreshQuotas={() => void refreshQuotas(true)}
          mcpServers={mcpServers}
          onRefreshMcp={() => void refreshMcp()}
          onAddMcp={async (name, transport, endpoint) => {
            if (isTauriRuntime()) {
              try {
                const created = await tauriApi.addMcpServer(name, transport, endpoint);
                if (created) setMcpServers((prev) => [...prev, created]);
                showToast(`Added MCP server ${name}`);
              } catch (err) {
                showToast(`Add MCP failed: ${String(err)}`);
              }
              return;
            }
            setMcpServers((prev) => [
              ...prev,
              {
                id: `mcp-${Date.now()}`,
                name,
                transport,
                endpoint,
                enabled: true,
                status: 'idle',
              },
            ]);
            showToast(`Added MCP server ${name}`);
          }}
          onTestMcp={async (id) => {
            if (isTauriRuntime()) {
              try {
                const updated = await tauriApi.pingMcpServer(id);
                if (updated) {
                  setMcpServers((prev) => prev.map((m) => (m.id === id ? updated : m)));
                  showToast(
                    updated.status === 'connected'
                      ? `MCP OK (${updated.latencyMs ?? 0}ms)`
                      : `MCP ping: ${updated.status}`,
                  );
                }
              } catch (err) {
                showToast(`MCP ping failed: ${String(err)}`);
              }
              return;
            }
            setMcpServers((prev) =>
              prev.map((m) =>
                m.id === id ? { ...m, status: 'connected', latencyMs: 19 } : m,
              ),
            );
            showToast('MCP connection OK (19ms)');
          }}
          onToggleMcp={async (id, enabled) => {
            if (isTauriRuntime()) {
              try {
                const updated = await tauriApi.toggleMcpServer(id, enabled);
                setMcpServers((prev) =>
                  prev.map((m) => (m.id === id ? updated ?? { ...m, enabled } : m)),
                );
              } catch (err) {
                showToast(`Toggle MCP failed: ${String(err)}`);
              }
              return;
            }
            setMcpServers((prev) =>
              prev.map((m) => (m.id === id ? { ...m, enabled } : m)),
            );
          }}
          onDeleteMcp={async (id) => {
            if (isTauriRuntime()) {
              try {
                await tauriApi.deleteMcpServer(id);
                setMcpServers((prev) => prev.filter((m) => m.id !== id));
                showToast('Deleted MCP server');
              } catch (err) {
                showToast(`Delete MCP failed: ${String(err)}`);
              }
              return;
            }
            setMcpServers((prev) => prev.filter((m) => m.id !== id));
            showToast('Deleted MCP server');
          }}
          onImportAgyMcp={async () => {
            if (isTauriRuntime()) {
              try {
                const imported = await tauriApi.importAgyMcpServers();
                if (imported) setMcpServers(imported);
                showToast(`Synced ${imported?.length ?? 0} MCP servers from AGY`);
              } catch (err) {
                showToast(`AGY import failed: ${String(err)}`);
              }
              return;
            }
            showToast('AGY MCP import requires Tauri desktop runtime');
          }}
          skills={skills}
          onRefreshSkills={() => void refreshSkills()}
          automations={automations}
          onRefreshAutomations={() => void refreshAutomations()}
          onCreateAutomation={handleCreateAutomation}
          onToggleAutomation={handleToggleAutomation}
          onDeleteAutomation={async (id) => {
            if (isTauriRuntime()) {
              try {
                await tauriApi.deleteAutomation(id);
                setAutomations((prev) => prev.filter((a) => a.id !== id));
                showToast('Deleted automation');
              } catch (err) {
                showToast(`Delete automation failed: ${String(err)}`);
              }
              return;
            }
            setAutomations((prev) => prev.filter((a) => a.id !== id));
            showToast('Deleted automation');
          }}
          onRunAutomation={handleRunAutomation}
          discordStatus={discordStatus}
          onStartDiscordBot={async (token) => {
            if (isTauriRuntime()) {
              try {
                const next = await tauriApi.startDiscordBot(token);
                if (next) setDiscordStatus(next);
                showToast('Discord bot started');
              } catch (err) {
                showToast(`Discord start failed: ${String(err)}`);
              }
              return;
            }
            setDiscordStatus({
              running: true,
              botTag: 'Sakana#0001',
              hasToken: true,
              connectedGuilds: 1,
            });
            showToast('Discord bot started (preview)');
          }}
          onStopDiscordBot={async () => {
            if (isTauriRuntime()) {
              try {
                const next = await tauriApi.stopDiscordBot();
                if (next) setDiscordStatus(next);
                showToast('Discord bot stopped');
              } catch (err) {
                showToast(`Discord stop failed: ${String(err)}`);
              }
              return;
            }
            setDiscordStatus({
              running: false,
              hasToken: false,
              connectedGuilds: 0,
            });
            showToast('Discord bot stopped');
          }}
          discordUsers={discordUsers}
          onUpsertDiscordUser={async (userId, label, role) => {
            if (isTauriRuntime()) {
              try {
                const saved = await tauriApi.upsertDiscordRule(userId, label, role);
                if (saved) {
                  setDiscordUsers((prev) => {
                    const idx = prev.findIndex((u) => u.id === saved.id || u.userId === saved.userId);
                    if (idx >= 0) {
                      return prev.map((u, i) => (i === idx ? saved : u));
                    }
                    return [...prev, saved];
                  });
                }
                showToast(`Saved Discord rule for ${label}`);
              } catch (err) {
                showToast(`Save Discord rule failed: ${String(err)}`);
              }
              return;
            }
            setDiscordUsers((prev) => {
              const idx = prev.findIndex((u) => u.userId === userId);
              if (idx >= 0) {
                return prev.map((u, i) => (i === idx ? { ...u, label, role } : u));
              }
              return [...prev, { id: `du-${Date.now()}`, userId, label, role }];
            });
            showToast(`Saved Discord rule for ${label}`);
          }}
          onDeleteDiscordUser={async (id) => {
            if (isTauriRuntime()) {
              try {
                await tauriApi.deleteDiscordRule(id);
                setDiscordUsers((prev) => prev.filter((u) => u.id !== id));
                showToast('Removed Discord rule');
              } catch (err) {
                showToast(`Delete Discord rule failed: ${String(err)}`);
              }
              return;
            }
            setDiscordUsers((prev) => prev.filter((u) => u.id !== id));
            showToast('Removed Discord rule');
          }}
          remoteHosts={remoteHosts}
          activeHostId={activeHostId}
          onSwitchHost={(hid) => void handleSwitchHost(hid)}
          onRefreshRemoteHosts={refreshRemoteHosts}
          remoteStatus={remoteStatus}
          onStartRemoteServer={async (port, lanEnabled) => {
            if (isTauriRuntime()) {
              try {
                const next = await tauriApi.startRemoteServer(port, lanEnabled);
                if (next) setRemoteStatus(next);
                showToast(`Remote server listening on :${next?.port ?? port ?? 4317}`);
              } catch (err) {
                showToast(`Remote start failed: ${String(err)}`);
              }
              return;
            }
            const p = port ?? 4317;
            const bindAddr = lanEnabled ? '0.0.0.0' : '127.0.0.1';
            setRemoteStatus({
              running: true,
              port: p,
              bindAddr,
              magicLink: `http://${bindAddr}:${p}/?token=skn_preview_token`,
              token: 'skn_preview_token',
              qrSvg: '',
            });
            showToast(`Remote server started on :${p} (preview)`);
          }}
          onStopRemoteServer={async () => {
            if (isTauriRuntime()) {
              try {
                const next = await tauriApi.stopRemoteServer();
                if (next) setRemoteStatus(next);
                showToast('Remote server stopped');
              } catch (err) {
                showToast(`Remote stop failed: ${String(err)}`);
              }
              return;
            }
            setRemoteStatus(null);
            showToast('Remote server stopped');
          }}
          secrets={secrets}
          onSetSecret={async (key, value) => {
            if (isTauriRuntime()) {
              try {
                const next = await tauriApi.setEnvSecret(key, value);
                if (next) setSecrets(next);
                showToast(`Saved secret ${key}`);
              } catch (err) {
                showToast(`Save secret failed: ${String(err)}`);
              }
              return;
            }
            setSecrets((prev) => {
              const exists = prev.some((s) => s.key === key);
              if (exists) {
                return prev.map((s) => (s.key === key ? { ...s, value } : s));
              }
              return [...prev, { key, value, hint: '••••' }];
            });
            showToast(`Saved secret ${key}`);
          }}
          onDeleteSecret={async (key) => {
            if (isTauriRuntime()) {
              try {
                await tauriApi.deleteEnvSecret(key);
                setSecrets((prev) => prev.filter((s) => s.key !== key));
                showToast(`Deleted secret ${key}`);
              } catch (err) {
                showToast(`Delete secret failed: ${String(err)}`);
              }
              return;
            }
            setSecrets((prev) => prev.filter((s) => s.key !== key));
            showToast(`Deleted secret ${key}`);
          }}
          onToast={showToast}
          disabledHarnesses={disabledHarnesses}
          onToggleHarness={handleToggleHarness}
          onRefreshModels={() => refreshHarnessModels(true)}
          refreshingModels={refreshingModels}
          onOpenGitHubClone={() => setGhCloneOpen(true)}
          projects={projects}
          onSetProjectDiscordChannel={handleSetProjectDiscordChannel}
          onSetThreadDiscordChannel={handleSetThreadDiscordChannel}
          customization={customization}
          onUpdateCustomization={async (next) => {
            setCustomization(next);
            applyCustomizationToDom(next);
            if (isTauriRuntime()) {
              try {
                const saved = await tauriApi.updateAppCustomization(next);
                if (saved) {
                  setCustomization(saved);
                  applyCustomizationToDom(saved);
                }
              } catch (err) {
                showToast(`Failed to save customization: ${String(err)}`);
              }
            }
          }}
          onResetCustomization={async () => {
            if (isTauriRuntime()) {
              try {
                const reset = await tauriApi.resetAppCustomization();
                if (reset) {
                  setCustomization(reset);
                  applyCustomizationToDom(reset);
                }
                showToast('Reset Sakana appearance to defaults');
              } catch (err) {
                showToast(`Failed to reset customization: ${String(err)}`);
              }
              return;
            }
            setCustomization(null);
            applyCustomizationToDom(null);
            showToast('Reset Sakana appearance to defaults');
          }}
          syncConfig={syncConfig}
          syncStatus={syncStatus}
          onUpdateSyncConfig={async (next) => {
            setSyncConfig(next);
            if (isTauriRuntime()) {
              try {
                const saved = await tauriApi.updateSyncConfig(next);
                if (saved) setSyncConfig(saved);
                showToast(next.enabled ? 'Cross-Device Auto-Sync enabled' : 'Cross-Device Sync settings saved');
              } catch (err) {
                showToast(`Sync config failed: ${String(err)}`);
              }
            }
          }}
          onSyncNow={async () => {
            if (!isTauriRuntime()) {
              showToast('Cross-Device Sync requires desktop runtime');
              return;
            }
            try {
              const st = await tauriApi.syncNow();
              if (st) setSyncStatus(st);
              showToast('Cross-Device Sync complete');
            } catch (err) {
              showToast(`Sync failed: ${String(err)}`);
            }
          }}
        />
        </ErrorBoundary>
      )}

      {/* Modals */}
      <CommandPalette
        open={cmdOpen}
        onClose={() => setCmdOpen(false)}
        threads={threads}
        projects={projects}
        onSelectThread={(id) => {
          setSelectedThreadId(id);
          setPrimarySection('chat');
        }}
        onSelectProject={(projectId) => {
          const match = threads.find((t) => t.projectId === projectId);
          if (match) {
            setSelectedThreadId(match.id);
            setPrimarySection('chat');
          } else {
            setNewWtOpen(true);
          }
        }}
        onSelectInspectorTab={(tab) => {
          setPrimarySection('chat');
          setRightOpen(true);
          setInspectorTab(tab);
        }}
        onSelectDashSubtab={(tab) => {
          setDashSubtab(tab);
          setPrimarySection('dashboard');
        }}
        onSetTheme={handleSetTheme}
        onToggleSplit={() => setSplitView((v) => !v)}
        onOpenNewWorktree={() => setNewWtOpen(true)}
        onOpenCreatePr={() => setPrOpen(true)}
        onOpenRace={() => setRaceOpen(true)}
      />

      <RaceModal
        open={raceOpen}
        onClose={() => setRaceOpen(false)}
        projects={projects}
        activeProjectId={activeProject?.id}
        harnessModels={harnessModels}
        onToast={showToast}
        onMerged={() => {
          void refreshDiffs('all');
        }}
      />

      <CreatePrModal
        open={prOpen}
        onClose={() => setPrOpen(false)}
        thread={activeThread}
        onSubmit={async ({ title, body, baseBranch }) => {
          if (isTauriRuntime()) {
            const url = await tauriApi.createPr(effectiveCwd, title, body, baseBranch, activeProject?.wslDistro);
            showToast(url ? `PR created: ${url}` : `Opened PR: ${title}`);
          } else {
            showToast(`Opened PR: ${title}`);
          }
        }}
      />

      <NewWorktreeModal
        open={newWtOpen}
        onClose={() => setNewWtOpen(false)}
        projects={projects}
        harnessModels={harnessModels}
        onCreate={handleCreateWorktree}
      />

      <GitHubCloneModal
        open={ghCloneOpen}
        onClose={() => setGhCloneOpen(false)}
        onClonedProject={handleCreateProject}
        onToast={showToast}
      />

      {/* Auto-Update Banner */}
      {availableUpdate && !updateDismissed && (
        <div
          className="sakana-popover pop-enter fixed bottom-5 left-5 z-[80] w-[360px] rounded-[12px] p-3.5 border"
          style={{
            background: 'var(--bg-elevated)',
            borderColor: 'var(--border-subtle)',
            color: 'var(--text-main)',
          }}
        >
          <div className="flex items-start justify-between gap-2">
            <div>
              <div className="text-[12.5px] font-semibold">
                Update Available · v{availableUpdate.latestVersion}
              </div>
              <div className="text-[11px] mt-0.5" style={{ color: 'var(--text-dim)' }}>
                Current v{availableUpdate.currentVersion} → v{availableUpdate.latestVersion}
              </div>
            </div>
            <button
              type="button"
              onClick={() => {
                try {
                  window.localStorage.setItem(
                    'sakana_update_skipped_version',
                    availableUpdate.latestVersion,
                  );
                  window.dispatchEvent(new Event('sakana:update-prefs-changed'));
                } catch {
                  // ignore
                }
                setUpdateDismissed(true);
                showToast(`Skipped v${availableUpdate.latestVersion}`);
              }}
              className="text-[11px] px-1.5 py-0.5 rounded hover:opacity-80 cursor-pointer"
              style={{ color: 'var(--text-dim)' }}
              disabled={installingUpdate}
              title="Skip this version"
            >
              Skip
            </button>
          </div>
          {installingUpdate && (
            <div className="mt-2.5">
              <div
                className="h-1.5 w-full rounded-full overflow-hidden"
                style={{ background: 'var(--bg-hover)' }}
              >
                <div
                  className="h-full transition-all duration-150"
                  style={{
                    width: `${updateProgress ?? 15}%`,
                    background: 'var(--accent)',
                  }}
                />
              </div>
              <div className="text-[10.5px] mt-1" style={{ color: 'var(--text-dim)' }}>
                {updateProgress !== null
                  ? `Downloading update (${updateProgress}%)...`
                  : 'Downloading and verifying signature...'}
              </div>
            </div>
          )}
          <div className="mt-3 flex flex-wrap items-center justify-between gap-1.5">
            <div className="flex items-center gap-1">
              <button
                type="button"
                disabled={installingUpdate}
                onClick={() => {
                  try {
                    window.localStorage.setItem(
                      'sakana_update_snooze_until',
                      String(Date.now() + 60 * 60 * 1000),
                    );
                    window.dispatchEvent(new Event('sakana:update-prefs-changed'));
                  } catch {
                    // ignore
                  }
                  setUpdateDismissed(true);
                  showToast('Snoozed update reminder for 1 hour');
                }}
                className="btn-ghost px-2 py-1 text-[10.5px] cursor-pointer"
              >
                Snooze 1h
              </button>
              <button
                type="button"
                disabled={installingUpdate}
                onClick={() => {
                  try {
                    window.localStorage.setItem(
                      'sakana_update_snooze_until',
                      String(Date.now() + 24 * 60 * 60 * 1000),
                    );
                    window.dispatchEvent(new Event('sakana:update-prefs-changed'));
                  } catch {
                    // ignore
                  }
                  setUpdateDismissed(true);
                  showToast('Snoozed update reminder for 24 hours');
                }}
                className="btn-ghost px-2 py-1 text-[10.5px] cursor-pointer"
              >
                24h
              </button>
              {availableUpdate.releaseUrl && (
                <button
                  type="button"
                  disabled={installingUpdate}
                  onClick={() => {
                    void tauriApi.openExternalUrl(availableUpdate.releaseUrl!);
                  }}
                  className="btn-ghost px-2 py-1 text-[10.5px] cursor-pointer"
                  title="Download installer manually from GitHub"
                >
                  Manual
                </button>
              )}
            </div>
            <button
              type="button"
              disabled={installingUpdate}
              onClick={async () => {
                setInstallingUpdate(true);
                setUpdateProgress(0);
                try {
                  await tauriApi.installUpdate();
                } catch (err) {
                  setInstallingUpdate(false);
                  setUpdateProgress(null);
                  showToast(`Update failed: ${String(err)}`);
                }
              }}
              className="px-3 py-1.5 rounded-[8px] text-[11.5px] font-medium text-white disabled:opacity-60 cursor-pointer"
              style={{ background: 'var(--accent)' }}
            >
              {installingUpdate
                ? updateProgress !== null
                  ? `Installing ${updateProgress}%...`
                  : 'Installing...'
                : 'Install & Restart'}
            </button>
          </div>
        </div>
      )}

      {/* Toast */}
      {toast && (
        <div
          className="sakana-popover pop-enter fixed bottom-5 right-5 z-[80] max-w-[520px] break-words px-3.5 py-2 rounded-[10px] text-[12px] font-medium"
          style={{ color: 'var(--text-main)' }}
        >
          {toast}
        </div>
      )}
    </div>
  );
}
