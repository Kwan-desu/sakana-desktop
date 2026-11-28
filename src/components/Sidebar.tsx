import { useEffect, useRef, useState } from 'react';
import {
  BellRinging,
  Buildings,
  CaretDown,
  CaretRight,
  ChatCircle,
  Check,
  Clock,
  Cpu,
  Desktop,
  FolderOpen,
  FolderPlus,
  FolderSimple,
  GearSix,
  GitBranch,
  GithubLogo,
  Globe,
  Lightning,
  ListChecks,
  MagnifyingGlass,
  Moon,
  PencilSimple,
  Play,
  Plugs,
  Plus,
  SidebarSimple,
  SquaresFour,
  Trash,
  X,
} from '@phosphor-icons/react';
import type {
  AutomationItem,
  DashboardSubtab,
  HarnessId,
  Office,
  PrimarySection,
  Project,
  QuotaAccount,
  RemoteHost,
  Thread,
  WslDistro,
} from '../types';
import { DiscordIcon, HARNESS_META, HarnessIcon } from './HarnessBadge';
import { FolderPickerModal } from './FolderPickerModal';
import { tauriApi } from '../lib/tauriApi';

interface SidebarProps {
  open: boolean;
  width?: number;
  onToggleOpen: () => void;
  primarySection: PrimarySection;
  onSwitchSection: (sec: PrimarySection) => void;
  projects: Project[];
  threads: Thread[];
  awakeSessionIds?: string[];
  onSleepSession?: (id: string) => Promise<void> | void;
  onWakeSession?: (id: string) => Promise<void> | void;
  selectedThreadId: string;
  onSelectThread: (id: string) => void;
  onQuickNewThread: (projectId?: string) => Promise<void> | void;
  onNewWorktreeThread: () => void;
  onCreateProject: (opts: {
    name: string;
    cwd: string;
    runtime?: 'native' | 'wsl';
    wslDistro?: string;
  }) => Promise<void> | void;
  onDeleteProject: (id: string) => Promise<void> | void;
  onRenameProject?: (id: string, name: string) => Promise<void> | void;
  onReorderProjects?: (orderedIds: string[]) => Promise<void> | void;
  onSetProjectDiscordChannel?: (projectId: string, channel?: string) => Promise<void> | void;
  onDeleteThread: (id: string) => Promise<void> | void;
  onRenameThread?: (id: string, title: string) => Promise<void> | void;
  onMoveThread?: (id: string, projectId: string) => Promise<void> | void;
  onSetThreadDiscordChannel?: (threadId: string, channel?: string | null) => Promise<void> | void;
  onReorderThreads?: (
    orderedIds: string[],
    moved?: { id: string; projectId: string },
  ) => Promise<void> | void;
  onOpenGitHubClone?: () => void;
  dashSubtab: DashboardSubtab;
  onSelectDashSubtab: (tab: DashboardSubtab) => void;
  automations?: AutomationItem[];
  onCreateAutomation?: (
    name: string,
    kind: 'cron' | 'webhook',
    schedule: string,
    prompt: string,
  ) => Promise<void> | void;
  onToggleAutomation?: (id: string, enabled: boolean) => Promise<void> | void;
  onRunAutomation?: (id: string) => Promise<void> | void;
  quotas: QuotaAccount[];
  onRefreshQuotas?: () => void;
  remoteHosts: RemoteHost[];
  activeHostId: string;
  onSwitchHost: (hostId: string) => void;
  onManageHosts: () => void;
  offices?: Office[];
  selectedOfficeId?: string;
  onSelectOffice?: (id: string) => void;
  onNewOffice?: () => void;
  onDeleteOffice?: (id: string) => Promise<void> | void;
}

export function Sidebar({
  open,
  width = 272,
  onToggleOpen,
  primarySection,
  onSwitchSection,
  projects,
  threads,
  awakeSessionIds = [],
  onSleepSession,
  onWakeSession,
  selectedThreadId,
  onSelectThread,
  onQuickNewThread,
  onNewWorktreeThread,
  onCreateProject,
  onDeleteProject,
  onRenameProject,
  onReorderProjects,
  onSetProjectDiscordChannel,
  onDeleteThread,
  onRenameThread,
  onMoveThread,
  onSetThreadDiscordChannel,
  onReorderThreads,
  onOpenGitHubClone,
  dashSubtab,
  onSelectDashSubtab,
  automations = [],
  onCreateAutomation,
  onToggleAutomation,
  onRunAutomation,
  quotas,
  onRefreshQuotas,
  remoteHosts,
  activeHostId,
  onSwitchHost,
  onManageHosts,
  offices = [],
  selectedOfficeId = '',
  onSelectOffice,
  onNewOffice,
  onDeleteOffice,
}: SidebarProps) {
  const [search, setSearch] = useState('');
  const [harnessFilter, setHarnessFilter] = useState<HarnessId | 'all'>('all');
  const [harnessMenuOpen, setHarnessMenuOpen] = useState(false);
  const [hostMenuOpen, setHostMenuOpen] = useState(false);
  const [collapsedProjects, setCollapsedProjects] = useState<Record<string, boolean>>({});
  const [usageOpen, setUsageOpen] = useState(false);
  const [usageDetailed, setUsageDetailed] = useState(true);
  const [addingProject, setAddingProject] = useState(false);
  const [automationsExpanded, setAutomationsExpanded] = useState(true);
  const [addingQuickAuto, setAddingQuickAuto] = useState(false);
  const [quickAutoName, setQuickAutoName] = useState('');
  const [quickAutoSchedule, setQuickAutoSchedule] = useState('0 9 * * *');
  const [quickAutoPrompt, setQuickAutoPrompt] = useState('');
  const [runningAutoId, setRunningAutoId] = useState<string | null>(null);
  const [newProjName, setNewProjName] = useState('');
  const [newProjCwd, setNewProjCwd] = useState('');
  const [newProjRuntime, setNewProjRuntime] = useState<'native' | 'wsl'>('native');
  const [newProjWslDistro, setNewProjWslDistro] = useState('');
  const [wslDistros, setWslDistros] = useState<WslDistro[]>([]);
  const [folderModalOpen, setFolderModalOpen] = useState(false);
  const [threadCtxMenu, setThreadCtxMenu] = useState<{
    thread: Thread;
    x: number;
    y: number;
    showMoveList: boolean;
    showDiscordInput: boolean;
    discordDraft: string;
  } | null>(null);
  const [projectCtxMenu, setProjectCtxMenu] = useState<{
    project: Project;
    x: number;
    y: number;
    showDiscordInput: boolean;
    discordDraft: string;
  } | null>(null);
  const [renamingThreadId, setRenamingThreadId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState('');
  const [renamingProjectId, setRenamingProjectId] = useState<string | null>(null);
  const [renameProjectDraft, setRenameProjectDraft] = useState('');
  const ctxMenuRef = useRef<HTMLDivElement | null>(null);

  type DragItem =
    | {
        kind: 'project';
        id: string;
        label: string;
        startX: number;
        startY: number;
        x: number;
        y: number;
        active: boolean;
      }
    | {
        kind: 'thread';
        id: string;
        projectId: string;
        label: string;
        startX: number;
        startY: number;
        x: number;
        y: number;
        active: boolean;
      };
  type DropTarget =
    | { kind: 'project-reorder'; projectId: string; position: 'before' | 'after' }
    | { kind: 'thread-to-project'; projectId: string }
    | { kind: 'thread-reorder'; threadId: string; projectId: string; position: 'before' | 'after' };

  const [dragItem, setDragItem] = useState<DragItem | null>(null);
  const [dropTarget, setDropTarget] = useState<DropTarget | null>(null);
  const dragItemRef = useRef<DragItem | null>(null);
  const dropTargetRef = useRef<DropTarget | null>(null);
  const suppressClickRef = useRef(false);
  dragItemRef.current = dragItem;
  dropTargetRef.current = dropTarget;

  const slugifyChannel = (raw: string) => {
    const slug = raw
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9-_]+/g, '-')
      .replace(/^-+|-+$/g, '');
    return `#${slug || 'sakana'}`;
  };

  useEffect(() => {
    if (!threadCtxMenu && !projectCtxMenu) return;
    const onDown = (e: MouseEvent) => {
      if (ctxMenuRef.current && !ctxMenuRef.current.contains(e.target as Node)) {
        setThreadCtxMenu(null);
        setProjectCtxMenu(null);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setThreadCtxMenu(null);
        setProjectCtxMenu(null);
      }
    };
    const onScroll = () => {
      setThreadCtxMenu(null);
      setProjectCtxMenu(null);
    };
    document.addEventListener('mousedown', onDown);
    window.addEventListener('keydown', onKey);
    window.addEventListener('scroll', onScroll, true);
    return () => {
      document.removeEventListener('mousedown', onDown);
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('scroll', onScroll, true);
    };
  }, [threadCtxMenu, projectCtxMenu]);

  const commitRename = async (threadId: string) => {
    const next = renameDraft.trim();
    setRenamingThreadId(null);
    if (!next) return;
    await onRenameThread?.(threadId, next);
  };

  const commitRenameProject = async (projectId: string) => {
    const next = renameProjectDraft.trim();
    setRenamingProjectId(null);
    if (!next) return;
    await onRenameProject?.(projectId, next);
  };

  useEffect(() => {
    if (!dragItem) return;

    const onPointerMove = (e: PointerEvent) => {
      const cur = dragItemRef.current;
      if (!cur) return;
      const dist = Math.hypot(e.clientX - cur.startX, e.clientY - cur.startY);
      const isActive = cur.active || dist >= 6;
      if (isActive && !cur.active) {
        suppressClickRef.current = true;
      }
      const nextDrag: DragItem = { ...cur, x: e.clientX, y: e.clientY, active: isActive };
      dragItemRef.current = nextDrag;
      setDragItem(nextDrag);

      if (!isActive) return;

      const els = document.elementsFromPoint(e.clientX, e.clientY);
      let nextTarget: DropTarget | null = null;

      if (cur.kind === 'project') {
        const projEl = els.find((el) => el.hasAttribute('data-dnd-project-section')) as
          | HTMLElement
          | undefined;
        const pid = projEl?.getAttribute('data-dnd-project-id');
        if (projEl && pid && pid !== cur.id) {
          const rect = projEl.getBoundingClientRect();
          const position = e.clientY < rect.top + rect.height / 2 ? 'before' : 'after';
          nextTarget = { kind: 'project-reorder', projectId: pid, position };
        }
      } else {
        const threadEl = els.find((el) => el.hasAttribute('data-dnd-thread-id')) as
          | HTMLElement
          | undefined;
        const tid = threadEl?.getAttribute('data-dnd-thread-id');
        const tpid = threadEl?.getAttribute('data-dnd-thread-project-id');
        if (threadEl && tid && tpid && tid !== cur.id) {
          const rect = threadEl.getBoundingClientRect();
          const position = e.clientY < rect.top + rect.height / 2 ? 'before' : 'after';
          nextTarget = { kind: 'thread-reorder', threadId: tid, projectId: tpid, position };
        } else {
          const projEl = els.find((el) => el.hasAttribute('data-dnd-project-section')) as
            | HTMLElement
            | undefined;
          const pid = projEl?.getAttribute('data-dnd-project-id');
          if (pid && pid !== cur.projectId) {
            nextTarget = { kind: 'thread-to-project', projectId: pid };
          }
        }
      }

      dropTargetRef.current = nextTarget;
      setDropTarget(nextTarget);
    };

    const finishDrag = () => {
      const cur = dragItemRef.current;
      const target = dropTargetRef.current;
      dragItemRef.current = null;
      dropTargetRef.current = null;
      setDragItem(null);
      setDropTarget(null);

      if (cur?.active && target) {
        if (cur.kind === 'project' && target.kind === 'project-reorder') {
          const ids = projects.map((p) => p.id).filter((id) => id !== cur.id);
          const idx = ids.indexOf(target.projectId);
          if (idx !== -1) {
            const insertAt = target.position === 'before' ? idx : idx + 1;
            ids.splice(insertAt, 0, cur.id);
            void onReorderProjects?.(ids);
          }
        } else if (cur.kind === 'thread' && target.kind === 'thread-to-project') {
          if (target.projectId !== cur.projectId) {
            setCollapsedProjects((prev) => ({ ...prev, [target.projectId]: false }));
            void onMoveThread?.(cur.id, target.projectId);
          }
        } else if (cur.kind === 'thread' && target.kind === 'thread-reorder') {
          const ids = threads.map((t) => t.id).filter((id) => id !== cur.id);
          const idx = ids.indexOf(target.threadId);
          if (idx !== -1) {
            const insertAt = target.position === 'before' ? idx : idx + 1;
            ids.splice(insertAt, 0, cur.id);
            const moved =
              target.projectId !== cur.projectId
                ? { id: cur.id, projectId: target.projectId }
                : undefined;
            if (moved) {
              setCollapsedProjects((prev) => ({ ...prev, [target.projectId]: false }));
            }
            void onReorderThreads?.(ids, moved);
          }
        }
      }

      if (suppressClickRef.current) {
        window.setTimeout(() => {
          suppressClickRef.current = false;
        }, 0);
      }
    };

    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', finishDrag);
    window.addEventListener('pointercancel', finishDrag);
    return () => {
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', finishDrag);
      window.removeEventListener('pointercancel', finishDrag);
    };
  }, [Boolean(dragItem), projects, threads, onReorderProjects, onMoveThread, onReorderThreads]);
  const sidebarRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const hasOpenPopover =
      hostMenuOpen || harnessMenuOpen || usageOpen || addingProject;
    if (!hasOpenPopover) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        setHostMenuOpen(false);
        setHarnessMenuOpen(false);
        setUsageOpen(false);
        setAddingProject(false);
      }
    };
    const onMouseDown = (e: MouseEvent) => {
      if (sidebarRef.current && !sidebarRef.current.contains(e.target as Node)) {
        setHostMenuOpen(false);
        setHarnessMenuOpen(false);
        setUsageOpen(false);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    document.addEventListener('mousedown', onMouseDown);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('mousedown', onMouseDown);
    };
  }, [hostMenuOpen, harnessMenuOpen, usageOpen, addingProject]);

  const handleQuickAddProject = async () => {
    if (activeHostId !== 'local') {
      setFolderModalOpen(true);
      return;
    }
    const selected = await tauriApi.pickProjectFolder();
    if (selected) {
      const base = selected.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || 'Project';
      await onCreateProject({ name: base, cwd: selected, runtime: 'native' });
      return;
    }
    setAddingProject((v) => !v);
  };

  useEffect(() => {
    if (!addingProject || activeHostId !== 'local') {
      setWslDistros([]);
      return;
    }
    let cancelled = false;
    void tauriApi
      .listWslDistros()
      .then((list) => {
        if (cancelled) return;
        setWslDistros(list);
        if (list.length > 0 && !newProjWslDistro) {
          const def = list.find((d) => d.isDefault) || list[0];
          setNewProjWslDistro(def.name);
        }
      })
      .catch(() => {
        if (!cancelled) setWslDistros([]);
      });
    return () => {
      cancelled = true;
    };
  }, [addingProject, activeHostId]);

  if (!open) return null;

  const toggleProject = (pid: string) => {
    setCollapsedProjects((prev) => ({ ...prev, [pid]: !prev[pid] }));
  };

  const activeThread = threads.find((t) => t.id === selectedThreadId);
  const activeHarness: HarnessId = activeThread?.harness || quotas[0]?.harness || 'agy-cli';
  const agyQuota = quotas.find((q) => q.harness === 'agy-cli');
  const kiroQuota = quotas.find((q) => q.harness === 'kiro-cli');
  const claudeQuota = quotas.find((q) => q.harness === 'claude-code');
  const sakanaQuota = quotas.find((q) => q.harness === 'sakana');

  let footerWinLabel = '5h';
  let footerUsedPct = 0;
  let hasFooterQuota = false;
  if (activeHarness === 'kiro-cli' && kiroQuota) {
    footerWinLabel = '30d';
    footerUsedPct = kiroQuota.fiveHourPct;
    hasFooterQuota = true;
  } else if (activeHarness === 'claude-code' && claudeQuota) {
    hasFooterQuota = true;
    if (claudeQuota.fiveHourPct >= 100) {
      footerWinLabel = '7d';
      footerUsedPct = claudeQuota.weeklyPct;
    } else {
      footerWinLabel = '5h';
      footerUsedPct = claudeQuota.fiveHourPct;
    }
  } else if (activeHarness === 'sakana' && sakanaQuota) {
    footerWinLabel = '5h';
    footerUsedPct = sakanaQuota.fiveHourPct;
    hasFooterQuota = true;
  } else if (agyQuota) {
    hasFooterQuota = true;
    if (agyQuota.fiveHourPct >= 100) {
      footerWinLabel = '7d';
      footerUsedPct = agyQuota.weeklyPct;
    } else {
      footerWinLabel = '5h';
      footerUsedPct = agyQuota.fiveHourPct;
    }
  }
  const footerRemainingPct = hasFooterQuota
    ? Math.max(0, Math.min(100, 100 - footerUsedPct))
    : 0;
  const footerPctColor =
    footerUsedPct >= 90
      ? 'var(--accent-red)'
      : footerUsedPct >= 70
        ? 'var(--accent-yellow)'
        : 'var(--text-main)';

  const renderUsageBar = (pct: number) => {
    const p = Math.max(0, Math.min(100, Number(pct || 0)));
    const col =
      p <= 15 ? 'var(--accent-red)' : p <= 35 ? 'var(--accent-yellow)' : 'var(--text-muted)';
    return (
      <span
        className="inline-block w-11 h-1.5 rounded-full overflow-hidden align-middle border shrink-0"
        style={{ backgroundColor: 'var(--bg-elevated)', borderColor: 'var(--border-line)' }}
      >
        <span className="block h-full rounded-full" style={{ width: `${p}%`, backgroundColor: col }} />
      </span>
    );
  };

  const agy5hRemaining = agyQuota ? Math.max(0, Math.min(100, 100 - agyQuota.fiveHourPct)) : 0;
  const agyWkRemaining = agyQuota ? Math.max(0, Math.min(100, 100 - agyQuota.weeklyPct)) : 0;
  const kiroRemaining = kiroQuota ? Math.max(0, Math.min(100, 100 - kiroQuota.weeklyPct)) : 0;

  const renderStatusDot = (t: Thread) => {
    if (t.status === 'RUNNING') {
      return (
        <span
          className="h-2 w-2 rounded-full pulse-dot shrink-0"
          style={{ backgroundColor: 'var(--accent-blue)' }}
          title="Working"
        />
      );
    }
    if (t.status === 'DONE' && t.unreadDone) {
      return (
        <span
          className="inline-flex items-center justify-center shrink-0"
          style={{ color: 'var(--accent-green)' }}
          title="Task done"
        >
          <BellRinging size={12} weight="fill" />
        </span>
      );
    }
    return <span className="h-1.5 w-1.5 rounded-full shrink-0 opacity-30" style={{ backgroundColor: 'var(--text-muted)' }} />;
  };

  const dashItems: { id: DashboardSubtab; label: string; icon: React.ReactNode }[] = [
    { id: 'overview', label: 'Overview & Telemetry', icon: <SquaresFour size={14} /> },
    { id: 'harnesses', label: 'Harnesses & Quotas', icon: <Cpu size={14} /> },
    { id: 'mcp', label: 'MCP & Skills', icon: <Plugs size={14} /> },
    { id: 'automations', label: 'Automations', icon: <Clock size={14} /> },
    { id: 'discord', label: 'Discord & RBAC', icon: <DiscordIcon size={14} /> },
    { id: 'remote', label: 'Preferences & System', icon: <GearSix size={14} /> },
  ];

  return (
    <aside
      ref={sidebarRef}
      style={{
        width: width ?? 272,
        backgroundColor: 'var(--bg-sidebar)',
        borderColor: 'var(--border-line)',
      }}
      className="h-full shrink-0 border-r flex flex-col select-none relative z-30"
    >
      {/* Header */}
      <div
        className="px-3 pt-3 pb-3 border-b space-y-2.5 shrink-0"
        style={{ borderColor: 'var(--border-line)' }}
      >
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2 min-w-0">
            <div
              className="w-6 h-6 rounded-[6px] border flex items-center justify-center shrink-0 font-mono-tech text-[11px] font-semibold"
              style={{
                backgroundColor: 'var(--bg-card)',
                borderColor: 'var(--border-strong)',
                color: 'var(--text-main)',
              }}
            >
              S
            </div>
            <span className="text-[14px] font-semibold tracking-tight truncate" style={{ color: 'var(--text-main)' }}>
              Sakana
            </span>
            <span
              className="font-mono-tech text-[10px] px-1.5 py-0.5 rounded-[5px]"
              style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }}
            >
              v2
            </span>
          </div>
          <button
            type="button"
            onClick={onToggleOpen}
            className="btn-ghost p-1.5 cursor-pointer"
            title="Hide Sidebar (Ctrl+B)"
            aria-label="Hide Sidebar"
          >
            <SidebarSimple size={14} />
          </button>
        </div>

        {/* Active Host Switcher */}
        <div className="relative">
          <button
            type="button"
            onClick={() => setHostMenuOpen((v) => !v)}
            className="w-full rounded-[7px] border px-2.5 py-1.5 text-[11.5px] font-medium flex items-center justify-between gap-2 cursor-pointer transition-colors hover:bg-[var(--bg-hover)]"
            style={{
              backgroundColor: 'var(--bg-card)',
              borderColor: activeHostId === 'local' ? 'var(--border-line)' : 'var(--accent-green)',
              color: 'var(--text-main)',
            }}
            title="Switch active Sakana host (Local or Remote)"
          >
            <span className="inline-flex items-center gap-1.5 min-w-0">
              {activeHostId === 'local' ? (
                <Desktop size={13} style={{ color: 'var(--text-muted)' }} />
              ) : (
                <Globe size={13} style={{ color: 'var(--accent-green)' }} />
              )}
              <span className="truncate">
                {activeHostId === 'local'
                  ? 'Local Machine'
                  : remoteHosts.find((h) => h.id === activeHostId)?.name || 'Remote Host'}
              </span>
              {activeHostId !== 'local' && (
                <span
                  className="font-mono-tech text-[9.5px] px-1 rounded-[4px] shrink-0"
                  style={{ backgroundColor: 'var(--accent-green-bg)', color: 'var(--accent-green)' }}
                >
                  {remoteHosts.find((h) => h.id === activeHostId)?.os || 'remote'}
                </span>
              )}
            </span>
            <CaretDown size={11} style={{ color: 'var(--text-muted)' }} />
          </button>

          {hostMenuOpen && (
            <div className="sakana-popover pop-enter absolute top-full left-0 right-0 mt-1 rounded-[10px] p-1 z-50 space-y-0.5">
              <button
                type="button"
                onClick={() => {
                  onSwitchHost('local');
                  setHostMenuOpen(false);
                }}
                className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center justify-between cursor-pointer hover:bg-[var(--bg-hover)]"
                style={{ color: activeHostId === 'local' ? 'var(--text-main)' : 'var(--text-body)' }}
              >
                <span className="inline-flex items-center gap-2 truncate">
                  <Desktop size={13} style={{ color: 'var(--text-muted)' }} />
                  <span className="truncate">Local Machine</span>
                </span>
                {activeHostId === 'local' && <Check size={12} weight="bold" />}
              </button>

              {remoteHosts.map((host) => (
                <button
                  key={host.id}
                  type="button"
                  onClick={() => {
                    onSwitchHost(host.id);
                    setHostMenuOpen(false);
                  }}
                  className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center justify-between gap-2 cursor-pointer hover:bg-[var(--bg-hover)]"
                  style={{ color: activeHostId === host.id ? 'var(--text-main)' : 'var(--text-body)' }}
                >
                  <span className="inline-flex items-center gap-2 min-w-0">
                    <span
                      className="h-1.5 w-1.5 rounded-full shrink-0"
                      style={{
                        backgroundColor:
                          host.status === 'online' ? 'var(--accent-green)' : 'var(--text-muted)',
                      }}
                    />
                    <span className="truncate">{host.name}</span>
                    {host.os && (
                      <span className="font-mono-tech text-[9.5px]" style={{ color: 'var(--text-muted)' }}>
                        {host.os}
                      </span>
                    )}
                  </span>
                  {activeHostId === host.id && <Check size={12} weight="bold" />}
                </button>
              ))}

              <div className="border-t my-1" style={{ borderColor: 'var(--border-line)' }} />
              <button
                type="button"
                onClick={() => {
                  setHostMenuOpen(false);
                  onManageHosts();
                }}
                className="w-full px-2.5 py-1.5 rounded-[6px] text-[11.5px] flex items-center gap-2 cursor-pointer hover:bg-[var(--bg-hover)]"
                style={{ color: 'var(--text-muted)' }}
              >
                <Globe size={12} />
                <span>Manage Remote Hosts…</span>
              </button>
            </div>
          )}
        </div>

        {/* 2-Section Switcher */}
        <div
          className="grid grid-cols-2 gap-1 p-0.5 rounded-[8px] border"
          style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)' }}
        >
          <button
            type="button"
            onClick={() => onSwitchSection('chat')}
            className="h-7 px-2 rounded-[6px] text-[12px] font-medium flex items-center justify-center gap-1.5 cursor-pointer transition-colors"
            style={
              primarySection === 'chat'
                ? { backgroundColor: 'var(--bg-card)', color: 'var(--text-main)', boxShadow: 'var(--surface-shadow)' }
                : { color: 'var(--text-muted)' }
            }
          >
            <ChatCircle size={13} weight={primarySection === 'chat' ? 'fill' : 'regular'} />
            <span>Chat</span>
          </button>
          <button
            type="button"
            onClick={() => onSwitchSection('office')}
            className="h-7 px-2 rounded-[6px] text-[12px] font-medium flex items-center justify-center gap-1.5 cursor-pointer transition-colors"
            style={
              primarySection === 'office'
                ? { backgroundColor: 'var(--bg-card)', color: 'var(--text-main)', boxShadow: 'var(--surface-shadow)' }
                : { color: 'var(--text-muted)' }
            }
          >
            <Buildings size={13} weight={primarySection === 'office' ? 'fill' : 'regular'} />
            <span>Work</span>
            {offices.some((o) => o.status === 'RUNNING') && (
              <span
                className="h-1.5 w-1.5 rounded-full pulse-dot shrink-0"
                style={{ backgroundColor: 'var(--accent-blue)' }}
                title="Workspaces running"
              />
            )}
          </button>
        </div>
      </div>

      {/* Body */}
      {primarySection === 'chat' ? (
        <div className="flex-1 min-h-0 flex flex-col">
          {/* Search + New Worktree + Add Project + Harness Filter */}
          <div className="px-3 py-2.5 border-b space-y-2 shrink-0" style={{ borderColor: 'var(--border-line)' }}>
            <div className="flex items-center gap-1.5">
              <div className="relative flex-1">
                <MagnifyingGlass
                  size={13}
                  className="absolute left-2.5 top-1/2 -translate-y-1/2"
                  style={{ color: 'var(--text-muted)' }}
                />
                <input
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder="Search threads..."
                  className="w-full rounded-[6px] border pl-8 pr-2.5 py-1.5 text-[12px] focus:outline-none"
                  style={{
                    backgroundColor: 'var(--bg-card)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-main)',
                  }}
                />
              </div>
              <button
                type="button"
                onClick={() => void handleQuickAddProject()}
                className="btn-ghost w-7 h-7 flex items-center justify-center cursor-pointer shrink-0"
                title="Add Project Folder (1-click)"
                aria-label="Add Project Folder"
              >
                <FolderPlus size={13} />
              </button>
              {onOpenGitHubClone && (
                <button
                  type="button"
                  onClick={onOpenGitHubClone}
                  className="btn-ghost w-7 h-7 flex items-center justify-center cursor-pointer shrink-0"
                  title="Clone from GitHub"
                  aria-label="Clone from GitHub"
                >
                  <GithubLogo size={13} />
                </button>
              )}
              <button
                type="button"
                onClick={onNewWorktreeThread}
                className="btn-ghost w-7 h-7 flex items-center justify-center cursor-pointer shrink-0"
                title="New Worktree Thread…"
                aria-label="New Worktree Thread"
              >
                <GitBranch size={13} />
              </button>
              <button
                type="button"
                onClick={() => void onQuickNewThread()}
                className="btn-solid w-7 h-7 flex items-center justify-center cursor-pointer shrink-0"
                title="New Thread"
                aria-label="New Thread"
              >
                <Plus size={13} weight="bold" />
              </button>
            </div>

            {addingProject && (
              <form
                onSubmit={async (e) => {
                  e.preventDefault();
                  if (!newProjName.trim() || !newProjCwd.trim()) return;
                  const isWsl = activeHostId === 'local' && newProjRuntime === 'wsl' && !!newProjWslDistro;
                  await onCreateProject({
                    name: newProjName.trim(),
                    cwd: newProjCwd.trim(),
                    runtime: isWsl ? 'wsl' : 'native',
                    wslDistro: isWsl ? newProjWslDistro : undefined,
                  });
                  setNewProjName('');
                  setNewProjCwd('');
                  setNewProjRuntime('native');
                  setAddingProject(false);
                }}
                className="rounded-[8px] border p-2 space-y-1.5 pop-enter"
                style={{ backgroundColor: 'var(--bg-card)', borderColor: 'var(--border-strong)' }}
              >
                <div className="flex items-center justify-between">
                  <span className="text-[11px] font-medium" style={{ color: 'var(--text-main)' }}>
                    New Project
                  </span>
                  <button
                    type="button"
                    onClick={() => setAddingProject(false)}
                    aria-label="Close new project form"
                    className="p-0.5 cursor-pointer"
                    style={{ color: 'var(--text-muted)' }}
                  >
                    <X size={11} />
                  </button>
                </div>
                {activeHostId === 'local' && wslDistros.length > 0 && (
                  <div className="flex items-center gap-1.5">
                    <div
                      className="grid grid-cols-2 gap-0.5 p-0.5 rounded-[6px] border shrink-0"
                      style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)' }}
                    >
                      {(['native', 'wsl'] as const).map((rt) => (
                        <button
                          key={rt}
                          type="button"
                          onClick={() => setNewProjRuntime(rt)}
                          className="px-2 py-0.5 rounded-[4px] font-mono-tech text-[10px] cursor-pointer"
                          style={
                            newProjRuntime === rt
                              ? { backgroundColor: 'var(--bg-card)', color: 'var(--text-main)' }
                              : { color: 'var(--text-muted)' }
                          }
                        >
                          {rt === 'native' ? 'Native' : 'WSL2'}
                        </button>
                      ))}
                    </div>
                    {newProjRuntime === 'wsl' && (
                      <select
                        value={newProjWslDistro}
                        onChange={(e) => setNewProjWslDistro(e.target.value)}
                        className="flex-1 min-w-0 rounded-[6px] border px-1.5 py-0.5 font-mono-tech text-[10.5px]"
                        style={{
                          backgroundColor: 'var(--bg-app)',
                          borderColor: 'var(--border-line)',
                          color: 'var(--text-main)',
                        }}
                      >
                        {wslDistros.map((d) => (
                          <option key={d.name} value={d.name}>
                            {d.name} ({d.state})
                          </option>
                        ))}
                      </select>
                    )}
                  </div>
                )}
                <input
                  value={newProjName}
                  onChange={(e) => setNewProjName(e.target.value)}
                  placeholder="Project name"
                  className="w-full rounded-[6px] border px-2 py-1 text-[11.5px] focus:outline-none"
                  style={{
                    backgroundColor: 'var(--bg-app)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-main)',
                  }}
                />
                <div className="flex items-center gap-1.5">
                  <input
                    value={newProjCwd}
                    onChange={(e) => setNewProjCwd(e.target.value)}
                    placeholder={
                      newProjRuntime === 'wsl' || activeHostId !== 'local'
                        ? '/home/user/repo'
                        : 'Workspace path (e.g. D:\repo)'
                    }
                    className="flex-1 min-w-0 rounded-[6px] border px-2 py-1 font-mono-tech text-[11px] focus:outline-none"
                    style={{
                      backgroundColor: 'var(--bg-app)',
                      borderColor: 'var(--border-line)',
                      color: 'var(--text-main)',
                    }}
                  />
                  <button
                    type="button"
                    onClick={async () => {
                      if (activeHostId !== 'local' || newProjRuntime === 'wsl') {
                        setFolderModalOpen(true);
                        return;
                      }
                      const selected = await tauriApi.pickProjectFolder();
                      if (!selected) return;
                      setNewProjCwd(selected);
                      if (!newProjName.trim()) {
                        const base = selected.replace(/[\\/]+$/, '').split(/[\\/]/).pop();
                        if (base) setNewProjName(base);
                      }
                    }}
                    className="btn-ghost px-2 py-1 text-[11px] inline-flex items-center gap-1 shrink-0 cursor-pointer"
                    title="Browse folder"
                  >
                    <FolderOpen size={12} />
                    <span>Browse…</span>
                  </button>
                </div>
                <button type="submit" className="btn-solid w-full py-1 text-[11px] font-medium cursor-pointer">
                  Create Project
                </button>
              </form>
            )}

            <div className="relative">
              <button
                type="button"
                onClick={() => setHarnessMenuOpen((v) => !v)}
                className="w-full rounded-[6px] border px-2.5 py-1.5 text-[11.5px] font-medium flex items-center justify-between gap-2 cursor-pointer"
                style={{
                  backgroundColor: 'var(--bg-card)',
                  borderColor: 'var(--border-line)',
                  color: 'var(--text-body)',
                }}
              >
                <span className="inline-flex items-center gap-2 truncate">
                  <HarnessIcon harness={harnessFilter} size={13} />
                  <span className="truncate">
                    {harnessFilter === 'all' ? 'All Harnesses' : HARNESS_META[harnessFilter].name}
                  </span>
                </span>
                <CaretDown size={11} style={{ color: 'var(--text-muted)' }} />
              </button>

              {harnessMenuOpen && (
                <div
                  className="sakana-popover pop-enter absolute top-full left-0 right-0 mt-1 rounded-[10px] p-1 z-50 space-y-0.5"
                >
                  {(['all', 'agy-cli', 'claude-code', 'kiro-cli', 'sakana'] as const).map((hid) => (
                    <button
                      key={hid}
                      type="button"
                      onClick={() => {
                        setHarnessFilter(hid);
                        setHarnessMenuOpen(false);
                      }}
                      className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center justify-between cursor-pointer hover:bg-[var(--bg-hover)]"
                      style={{ color: harnessFilter === hid ? 'var(--text-main)' : 'var(--text-body)' }}
                    >
                      <span className="inline-flex items-center gap-2">
                        <HarnessIcon harness={hid} size={13} />
                        <span>{hid === 'all' ? 'All Harnesses' : HARNESS_META[hid].name}</span>
                      </span>
                      {harnessFilter === hid && <Check size={12} weight="bold" />}
                    </button>
                  ))}
                </div>
              )}
            </div>
          </div>

          {/* Project & Thread Tree */}
          <div className="flex-1 min-h-0 overflow-y-auto px-2 py-2 space-y-2.5">
            {projects.length === 0 ? (
              <div className="px-3 py-8 text-center space-y-2">
                <div className="text-[12px] font-medium" style={{ color: 'var(--text-main)' }}>
                  No projects yet
                </div>
                <p className="text-[11px] m-0 leading-relaxed" style={{ color: 'var(--text-muted)' }}>
                  Add a project folder to start a thread.
                </p>
                <button
                  type="button"
                  onClick={() => setAddingProject(true)}
                  className="btn-ghost px-2.5 py-1 text-[11px] font-medium inline-flex items-center gap-1.5 cursor-pointer"
                >
                  <FolderPlus size={12} />
                  <span>Add Project</span>
                </button>
              </div>
            ) : (
              projects.map((proj) => {
                const collapsed = !!collapsedProjects[proj.id];
                const q = search.trim().toLowerCase();
                const projThreads = threads.filter((t) => {
                  if (t.projectId !== proj.id) return false;
                  if (harnessFilter !== 'all' && t.harness !== harnessFilter) return false;
                  if (q && !`${t.title} ${t.branch} ${proj.name}`.toLowerCase().includes(q)) return false;
                  return true;
                });

                const isDraggingProj = dragItem?.active && dragItem.kind === 'project' && dragItem.id === proj.id;
                const isProjDropBefore =
                  dropTarget?.kind === 'project-reorder' &&
                  dropTarget.projectId === proj.id &&
                  dropTarget.position === 'before';
                const isProjDropAfter =
                  dropTarget?.kind === 'project-reorder' &&
                  dropTarget.projectId === proj.id &&
                  dropTarget.position === 'after';
                const isThreadDropOverProj =
                  dropTarget?.kind === 'thread-to-project' && dropTarget.projectId === proj.id;
                const isRenamingProj = renamingProjectId === proj.id;

                return (
                  <div
                    key={proj.id}
                    data-dnd-project-section="true"
                    data-dnd-project-id={proj.id}
                    className={`space-y-0.5 rounded-[7px] transition-opacity relative ${isDraggingProj ? 'opacity-40' : ''}`}
                    style={
                      isThreadDropOverProj
                        ? {
                            backgroundColor: 'var(--accent-blue-bg)',
                            outline: '1.5px dashed var(--accent-blue)',
                          }
                        : undefined
                    }
                  >
                    {isProjDropBefore && (
                      <div
                        className="h-0.5 rounded-full mx-1 pointer-events-none"
                        style={{ backgroundColor: 'var(--accent-blue)' }}
                      />
                    )}
                    <div
                      className="group flex items-center gap-1"
                      onContextMenu={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        setThreadCtxMenu(null);
                        const menuW = 228;
                        const menuH = 220;
                        const x = Math.min(e.clientX, window.innerWidth - menuW - 8);
                        const y = Math.min(e.clientY, window.innerHeight - menuH - 8);
                        setProjectCtxMenu({
                          project: proj,
                          x: Math.max(8, x),
                          y: Math.max(8, y),
                          showDiscordInput: false,
                          discordDraft: proj.discordChannel || slugifyChannel(proj.name),
                        });
                      }}
                      onPointerDown={(e) => {
                        if (e.button !== 0 || isRenamingProj) return;
                        const targetEl = e.target as HTMLElement;
                        if (targetEl.closest('input') || targetEl.closest('[data-no-drag]')) return;
                        setDragItem({
                          kind: 'project',
                          id: proj.id,
                          label: proj.name,
                          startX: e.clientX,
                          startY: e.clientY,
                          x: e.clientX,
                          y: e.clientY,
                          active: false,
                        });
                      }}
                    >
                      <button
                        type="button"
                        onClick={() => {
                          if (suppressClickRef.current || isRenamingProj) return;
                          toggleProject(proj.id);
                        }}
                        onDoubleClick={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          setRenamingProjectId(proj.id);
                          setRenameProjectDraft(proj.name);
                        }}
                        className="flex-1 min-w-0 px-2 py-1 rounded-[6px] flex items-center justify-between gap-1.5 hover:bg-[var(--bg-hover)] cursor-pointer select-none"
                        title="Click to toggle · Double-click to rename · Drag to reorder"
                      >
                        <span className="inline-flex items-center gap-1.5 min-w-0 flex-1">
                          {collapsed ? (
                            <CaretRight size={11} className="shrink-0" style={{ color: 'var(--text-muted)' }} />
                          ) : (
                            <CaretDown size={11} className="shrink-0" style={{ color: 'var(--text-muted)' }} />
                          )}
                          <FolderSimple size={13} className="shrink-0" style={{ color: 'var(--text-muted)' }} />
                          {isRenamingProj ? (
                            <input
                              value={renameProjectDraft}
                              onChange={(e) => setRenameProjectDraft(e.target.value)}
                              onClick={(e) => e.stopPropagation()}
                              onDoubleClick={(e) => e.stopPropagation()}
                              onPointerDown={(e) => e.stopPropagation()}
                              onKeyDown={(e) => {
                                e.stopPropagation();
                                if (e.key === 'Enter') {
                                  e.preventDefault();
                                  void commitRenameProject(proj.id);
                                } else if (e.key === 'Escape') {
                                  e.preventDefault();
                                  setRenamingProjectId(null);
                                }
                              }}
                              onBlur={() => void commitRenameProject(proj.id)}
                              autoFocus
                              className="w-full rounded-[4px] border px-1.5 py-0.5 text-[12px] focus:outline-none"
                              style={{
                                backgroundColor: 'var(--bg-app)',
                                borderColor: 'var(--accent-blue)',
                                color: 'var(--text-main)',
                              }}
                            />
                          ) : (
                            <span className="text-[12px] font-medium truncate" style={{ color: 'var(--text-main)' }}>
                              {proj.name}
                            </span>
                          )}
                          {(proj.runtime === 'wsl' || proj.wslDistro) && !isRenamingProj && (
                            <span
                              className="font-mono-tech text-[9px] px-1 rounded-[4px] shrink-0"
                              style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }}
                              title={`WSL2 · ${proj.wslDistro || 'default'}`}
                            >
                              WSL
                            </span>
                          )}
                          {proj.discordChannel && !isRenamingProj && (
                            <span style={{ color: 'var(--accent-discord)' }} title={`Discord ${proj.discordChannel}`}>
                              <DiscordIcon size={11} />
                            </span>
                          )}
                        </span>
                        <span className="font-mono-tech text-[10px] shrink-0" style={{ color: 'var(--text-muted)' }}>
                          {projThreads.length}
                        </span>
                      </button>
                      <button
                        type="button"
                        data-no-drag="true"
                        onClick={() => void onQuickNewThread(proj.id)}
                        className="opacity-0 group-hover:opacity-100 p-1 rounded-[5px] hover:bg-[var(--bg-hover)] cursor-pointer transition-opacity"
                        style={{ color: 'var(--text-muted)' }}
                        title={`New thread in ${proj.name}`}
                        aria-label={`New thread in ${proj.name}`}
                      >
                        <Plus size={11} />
                      </button>
                      {projects.length > 1 && (
                        <button
                          type="button"
                          data-no-drag="true"
                          onClick={() => onDeleteProject(proj.id)}
                          className="opacity-0 group-hover:opacity-100 p-1 rounded-[5px] hover:bg-[var(--bg-hover)] cursor-pointer transition-opacity"
                          style={{ color: 'var(--text-muted)' }}
                          title="Delete project"
                          aria-label={`Delete project ${proj.name}`}
                        >
                          <Trash size={11} />
                        </button>
                      )}
                    </div>

                    {!collapsed && (
                      <div className="space-y-0.5 pl-2">
                        {projThreads.length === 0 ? (
                          <div className="px-2.5 py-2 text-[11px]" style={{ color: 'var(--text-muted)' }}>
                            {q || harnessFilter !== 'all' ? 'No matching threads' : 'No threads yet'}
                          </div>
                        ) : (
                          projThreads.map((t) => {
                            const active = t.id === selectedThreadId;
                            const isRenaming = renamingThreadId === t.id;
                            const isDraggingThread =
                              dragItem?.active && dragItem.kind === 'thread' && dragItem.id === t.id;
                            const isThreadDropBefore =
                              dropTarget?.kind === 'thread-reorder' &&
                              dropTarget.threadId === t.id &&
                              dropTarget.position === 'before';
                            const isThreadDropAfter =
                              dropTarget?.kind === 'thread-reorder' &&
                              dropTarget.threadId === t.id &&
                              dropTarget.position === 'after';
                            return (
                              <div
                                key={t.id}
                                data-dnd-thread-id={t.id}
                                data-dnd-thread-project-id={t.projectId}
                                className={`group relative transition-opacity ${isDraggingThread ? 'opacity-40' : ''}`}
                                onPointerDown={(e) => {
                                  if (e.button !== 0 || isRenaming) return;
                                  const targetEl = e.target as HTMLElement;
                                  if (targetEl.closest('input') || targetEl.closest('[data-no-drag]')) return;
                                  setDragItem({
                                    kind: 'thread',
                                    id: t.id,
                                    projectId: t.projectId,
                                    label: t.title,
                                    startX: e.clientX,
                                    startY: e.clientY,
                                    x: e.clientX,
                                    y: e.clientY,
                                    active: false,
                                  });
                                }}
                                onContextMenu={(e) => {
                                  e.preventDefault();
                                  e.stopPropagation();
                                  const menuW = 200;
                                  const menuH = 220;
                                  const x = Math.min(e.clientX, window.innerWidth - menuW - 8);
                                  const y = Math.min(e.clientY, window.innerHeight - menuH - 8);
                                  setProjectCtxMenu(null);
                                  const parentProj = projects.find((p) => p.id === t.projectId);
                                  setThreadCtxMenu({
                                    thread: t,
                                    x: Math.max(8, x),
                                    y: Math.max(8, y),
                                    showMoveList: false,
                                    showDiscordInput: false,
                                    discordDraft:
                                      t.discordChannel ||
                                      parentProj?.discordChannel ||
                                      slugifyChannel(t.title || parentProj?.name || 'discord'),
                                  });
                                }}
                              >
                                {isThreadDropBefore && (
                                  <div
                                    className="h-0.5 rounded-full mx-1 mb-0.5 pointer-events-none"
                                    style={{ backgroundColor: 'var(--accent-blue)' }}
                                  />
                                )}
                                <button
                                  type="button"
                                  onClick={() => {
                                    if (suppressClickRef.current || isRenaming) return;
                                    onSelectThread(t.id);
                                  }}
                                  onDoubleClick={(e) => {
                                    e.preventDefault();
                                    e.stopPropagation();
                                    setRenamingThreadId(t.id);
                                    setRenameDraft(t.title);
                                  }}
                                  className={`w-full text-left px-2 py-1.5 rounded-[7px] border transition-colors cursor-pointer space-y-0.5 select-none ${
                                    active ? '' : 'hover:bg-[var(--bg-hover)]'
                                  }`}
                                  style={
                                    active
                                      ? {
                                          backgroundColor: 'var(--bg-card)',
                                          borderColor: 'var(--border-strong)',
                                          boxShadow: 'var(--surface-shadow)',
                                        }
                                      : {
                                          backgroundColor: 'transparent',
                                          borderColor: 'transparent',
                                        }
                                  }
                                >
                                  <div className="flex items-center justify-between gap-2">
                                    <span className="inline-flex items-center gap-1.5 min-w-0 flex-1">
                                      {renderStatusDot(t)}
                                      {isRenaming ? (
                                        <input
                                          value={renameDraft}
                                          onChange={(e) => setRenameDraft(e.target.value)}
                                          onClick={(e) => e.stopPropagation()}
                                          onKeyDown={(e) => {
                                            e.stopPropagation();
                                            if (e.key === 'Enter') {
                                              e.preventDefault();
                                              void commitRename(t.id);
                                            } else if (e.key === 'Escape') {
                                              e.preventDefault();
                                              setRenamingThreadId(null);
                                            }
                                          }}
                                          onBlur={() => void commitRename(t.id)}
                                          autoFocus
                                          className="w-full rounded-[4px] border px-1.5 py-0.5 text-[12px] focus:outline-none"
                                          style={{
                                            backgroundColor: 'var(--bg-app)',
                                            borderColor: 'var(--accent-blue)',
                                            color: 'var(--text-main)',
                                          }}
                                        />
                                      ) : (
                                        <span
                                          className={`text-[12px] truncate ${active ? 'font-semibold' : 'font-normal'}`}
                                          style={{ color: active ? 'var(--text-main)' : 'var(--text-body)' }}
                                        >
                                          {t.title}
                                        </span>
                                      )}
                                    </span>
                                    {t.status === 'DONE' && t.unreadDone ? (
                                      <span
                                        className="inline-flex items-center gap-1 font-mono-tech text-[9.5px] px-1.5 py-0.5 rounded-[5px] shrink-0 group-hover:opacity-0 transition-opacity"
                                        style={{
                                          backgroundColor: 'var(--accent-green-bg)',
                                          color: 'var(--accent-green)',
                                        }}
                                        title="Task done"
                                      >
                                        <BellRinging size={10} weight="fill" />
                                        <span>Done</span>
                                      </span>
                                    ) : (
                                      <span className="font-mono-tech text-[10px] shrink-0 group-hover:opacity-0 transition-opacity" style={{ color: 'var(--text-muted)' }}>
                                        {t.timeAgo}
                                      </span>
                                    )}
                                  </div>

                                  <div className="flex items-center justify-between gap-2 pl-3.5">
                                    <span
                                      className="inline-flex items-center gap-1.5 font-mono-tech text-[10px] truncate"
                                      style={{ color: active ? 'var(--text-body)' : 'var(--text-muted)' }}
                                    >
                                      <HarnessIcon harness={t.harness} size={11} />
                                      {awakeSessionIds.includes(t.id) && (
                                        <span
                                          className="inline-flex items-center gap-0.5 px-1 py-0.2 rounded-[4px] text-[9px] font-medium shrink-0"
                                          style={{
                                            backgroundColor: 'var(--accent-green-bg)',
                                            color: 'var(--accent-green)',
                                          }}
                                          title="Persistent agy-cli ACP worker is awake & warm"
                                        >
                                          <Lightning size={8} weight="fill" />
                                          <span>Awake</span>
                                        </span>
                                      )}
                                      {t.isDiscord ? (
                                        <DiscordIcon size={10} style={{ color: 'var(--accent-discord)' }} />
                                      ) : (
                                        <GitBranch size={10} />
                                      )}
                                      <span className="truncate">{t.branch}</span>
                                    </span>

                                    {(t.additions > 0 || t.deletions > 0) && (
                                      <span className="font-mono-tech text-[10px] shrink-0 flex items-center gap-1">
                                        <span style={{ color: 'var(--accent-green)' }}>+{t.additions}</span>
                                        <span style={{ color: 'var(--accent-red)' }}>-{t.deletions}</span>
                                      </span>
                                    )}
                                  </div>
                                </button>
                                <button
                                  type="button"
                                  data-no-drag="true"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    onDeleteThread(t.id);
                                  }}
                                  className="opacity-0 group-hover:opacity-100 absolute right-2 top-2 p-1 rounded-[5px] hover:bg-[var(--bg-hover)] cursor-pointer transition-opacity"
                                  style={{ color: 'var(--text-muted)' }}
                                  title="Delete thread"
                                  aria-label={`Delete thread ${t.title}`}
                                >
                                  <Trash size={11} />
                                </button>
                                {isThreadDropAfter && (
                                  <div
                                    className="h-0.5 rounded-full mx-1 mt-0.5 pointer-events-none"
                                    style={{ backgroundColor: 'var(--accent-blue)' }}
                                  />
                                )}
                              </div>
                            );
                          })
                        )}
                      </div>
                    )}
                    {isProjDropAfter && (
                      <div
                        className="h-0.5 rounded-full mx-1 pointer-events-none"
                        style={{ backgroundColor: 'var(--accent-blue)' }}
                      />
                    )}
                  </div>
                );
              })
            )}
          </div>
        </div>
      ) : primarySection === 'office' ? (
        <div className="flex-1 min-h-0 flex flex-col">
          <div
            className="px-3 py-2.5 border-b flex items-center justify-between gap-2 shrink-0"
            style={{ borderColor: 'var(--border-line)' }}
          >
            <span className="text-[11.5px] font-semibold" style={{ color: 'var(--text-main)' }}>
              Offices
            </span>
            <button
              type="button"
              onClick={() => onNewOffice?.()}
              className="btn-solid px-2 py-1 text-[11px] inline-flex items-center gap-1 cursor-pointer shrink-0"
              title="New Office"
            >
              <Plus size={11} weight="bold" />
              <span>New Office</span>
            </button>
          </div>
          <div className="flex-1 min-h-0 overflow-y-auto p-2 space-y-1">
            {offices.length === 0 ? (
              <div className="px-2.5 py-6 text-center space-y-2">
                <div className="text-[11.5px]" style={{ color: 'var(--text-muted)' }}>
                  No offices yet
                </div>
                <button
                  type="button"
                  onClick={() => onNewOffice?.()}
                  className="btn-ghost px-2.5 py-1 text-[11.5px] inline-flex items-center gap-1.5 cursor-pointer"
                >
                  <Plus size={12} />
                  <span>Create Office</span>
                </button>
              </div>
            ) : (
              offices.map((office) => {
                const active = office.id === selectedOfficeId;
                const running = office.status === 'RUNNING';
                return (
                  <div key={office.id} className="group relative">
                    <button
                      type="button"
                      onClick={() => onSelectOffice?.(office.id)}
                      className={`w-full text-left px-2.5 py-2 rounded-[7px] border transition-colors cursor-pointer space-y-1 ${
                        active ? '' : 'hover:bg-[var(--bg-hover)]'
                      }`}
                      style={
                        active
                          ? {
                              backgroundColor: 'var(--bg-card)',
                              borderColor: 'var(--border-strong)',
                              boxShadow: 'var(--surface-shadow)',
                            }
                          : {
                              backgroundColor: 'transparent',
                              borderColor: 'transparent',
                            }
                      }
                    >
                      <div className="flex items-center justify-between gap-2 pr-4">
                        <span className="inline-flex items-center gap-1.5 min-w-0">
                          {running ? (
                            <span
                              className="h-2 w-2 rounded-full pulse-dot shrink-0"
                              style={{ backgroundColor: 'var(--accent-blue)' }}
                              title="Running"
                            />
                          ) : (
                            <span
                              className="h-1.5 w-1.5 rounded-full shrink-0 opacity-35"
                              style={{ backgroundColor: 'var(--text-muted)' }}
                              title="Idle"
                            />
                          )}
                          <span
                            className={`text-[12px] truncate ${active ? 'font-semibold' : 'font-medium'}`}
                            style={{ color: active ? 'var(--text-main)' : 'var(--text-body)' }}
                          >
                            {office.name}
                          </span>
                        </span>
                        <span
                          className="font-mono-tech text-[10px] px-1.5 py-0.5 rounded-full shrink-0"
                          style={{
                            backgroundColor: 'var(--bg-elevated)',
                            color: 'var(--text-muted)',
                          }}
                          title={`${office.staff.length} staff`}
                        >
                          {office.staff.length}
                        </span>
                      </div>
                      <div
                        className="font-mono-tech text-[10px] truncate pl-3.5"
                        style={{ color: 'var(--text-muted)' }}
                        title={office.cwd}
                      >
                        {office.cwd || '.'}
                      </div>
                    </button>
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        void onDeleteOffice?.(office.id);
                      }}
                      className="opacity-0 group-hover:opacity-100 absolute right-2 top-2 p-1 rounded-[5px] hover:bg-[var(--bg-hover)] cursor-pointer transition-opacity"
                      style={{ color: 'var(--text-muted)' }}
                      title={`Delete office ${office.name}`}
                      aria-label={`Delete office ${office.name}`}
                    >
                      <Trash size={11} />
                    </button>
                  </div>
                );
              })
            )}
          </div>
        </div>
      ) : (
        <div className="flex-1 min-h-0 overflow-y-auto p-3 space-y-1">
          <div className="px-2 py-1 text-[11px] font-medium" style={{ color: 'var(--text-muted)' }}>
            Control Surface
          </div>
          {dashItems.map((item) => {
            const active = dashSubtab === item.id;
            return (
              <button
                key={item.id}
                type="button"
                onClick={() => onSelectDashSubtab(item.id)}
                className="w-full h-8 px-2.5 rounded-[7px] text-[12.5px] flex items-center justify-between cursor-pointer transition-colors"
                style={
                  active
                    ? {
                        backgroundColor: 'var(--bg-card)',
                        color: 'var(--text-main)',
                        fontWeight: 500,
                        border: '1px solid var(--border-line)',
                      }
                    : { color: 'var(--text-body)', border: '1px solid transparent' }
                }
              >
                <span className="inline-flex items-center gap-2.5">
                  {item.icon}
                  <span>{item.label}</span>
                </span>
              </button>
            );
          })}
        </div>
      )}

      {/* Footer: Usage Limit Popover Trigger + Preferences */}
      <div
        className="px-3 py-2 border-t flex items-center justify-between gap-2 relative shrink-0"
        style={{ borderColor: 'var(--border-line)' }}
      >
        <button
          type="button"
          onClick={() => {
            setUsageOpen((v) => {
              const next = !v;
              if (next) onRefreshQuotas?.();
              return next;
            });
          }}
          className="flex items-center gap-1.5 text-[11.5px] font-mono-tech cursor-pointer hover:opacity-80 min-w-0"
          style={{ color: 'var(--text-body)' }}
          title={
            hasFooterQuota
              ? `${footerWinLabel} limit remaining: ${footerRemainingPct}% (click to view all harness quotas)`
              : 'No usage data yet — click to view all harness quotas'
          }
          aria-label={
            hasFooterQuota
              ? `${footerWinLabel} limit remaining ${footerRemainingPct}%, click to view all harness quotas`
              : 'No usage data yet, click to view all harness quotas'
          }
        >
          <HarnessIcon harness={activeHarness} size={13} />
          <span style={{ color: 'var(--text-muted)' }}>{footerWinLabel}</span>
          <span className="font-semibold" style={{ color: footerPctColor }}>
            {footerRemainingPct}%
          </span>
        </button>

        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => {
              setUsageOpen(false);
              onSwitchSection('dashboard');
            }}
            className="px-2 py-1 rounded-[6px] text-[11px] font-medium inline-flex items-center gap-1.5 cursor-pointer transition-colors"
            style={
              primarySection === 'dashboard'
                ? { backgroundColor: 'var(--bg-card)', color: 'var(--text-main)', border: '1px solid var(--border-line)' }
                : { color: 'var(--text-body)', border: '1px solid transparent' }
            }
            title="Open Operations Dashboard"
          >
            <ListChecks size={13} weight={primarySection === 'dashboard' ? 'fill' : 'regular'} />
            <span>Dashboard</span>
          </button>
        </div>

        {/* Floating Usage Limit Popover (Original Web Sakana Design) */}
        {usageOpen && (
          <div className="sakana-popover pop-enter absolute left-2 bottom-11 w-[312px] rounded-[12px] overflow-hidden z-50">
            <div
              className="px-3.5 py-2.5 border-b flex items-center justify-between"
              style={{ borderColor: 'var(--border-line)' }}
            >
              <span className="text-[12.5px] font-semibold" style={{ color: 'var(--text-main)' }}>
                Usage Limits
              </span>
              <div
                className="grid grid-cols-2 gap-0.5 p-0.5 rounded-[6px] border text-[11px]"
                style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)' }}
              >
                <button
                  type="button"
                  onClick={() => setUsageDetailed(true)}
                  className="px-2 py-0.5 font-medium rounded-[4px] cursor-pointer"
                  style={
                    usageDetailed
                      ? { backgroundColor: 'var(--btn-solid-bg)', color: 'var(--btn-solid-text)' }
                      : { color: 'var(--text-muted)' }
                  }
                >
                  Detailed
                </button>
                <button
                  type="button"
                  onClick={() => setUsageDetailed(false)}
                  className="px-2 py-0.5 font-medium rounded-[4px] cursor-pointer"
                  style={
                    !usageDetailed
                      ? { backgroundColor: 'var(--btn-solid-bg)', color: 'var(--btn-solid-text)' }
                      : { color: 'var(--text-muted)' }
                  }
                >
                  Compact
                </button>
              </div>
            </div>

            <div className="divide-y" style={{ borderColor: 'var(--border-line)' }}>
              {/* Antigravity Section */}
              <div className="px-3.5 py-3 space-y-2">
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-2 min-w-0">
                    <span
                      className="w-5 h-5 rounded-full border flex items-center justify-center shrink-0"
                      style={{ backgroundColor: 'var(--bg-elevated)', borderColor: 'var(--border-line)' }}
                    >
                      <HarnessIcon harness="agy-cli" size={11} />
                    </span>
                    <span className="text-[12.5px] font-semibold truncate" style={{ color: 'var(--text-main)' }}>
                      Antigravity
                    </span>
                  </div>
                  <span className="font-mono-tech text-[10.5px] shrink-0" style={{ color: 'var(--text-muted)' }}>
                    {agyQuota ? `resets ${agyQuota.fiveHourReset}` : 'No usage yet'}
                  </span>
                </div>

                {usageDetailed ? (
                  <div className="space-y-1.5 pl-1">
                    <div className="flex items-center justify-between gap-2 text-[11px]">
                      <span className="truncate" style={{ color: 'var(--text-muted)' }}>
                        Five Hour Limit Remaining
                      </span>
                      <div className="flex items-center gap-1.5 shrink-0">
                        {renderUsageBar(agy5hRemaining)}
                        <span className="font-mono-tech w-8 text-right" style={{ color: 'var(--text-main)' }}>
                          {agy5hRemaining}%
                        </span>
                      </div>
                    </div>
                    <div className="flex items-center justify-between gap-2 text-[11px]">
                      <span className="truncate" style={{ color: 'var(--text-muted)' }}>
                        Weekly Limit Remaining{agyQuota?.weeklyReset ? ` · resets ${agyQuota.weeklyReset}` : ''}
                      </span>
                      <div className="flex items-center gap-1.5 shrink-0">
                        {renderUsageBar(agyWkRemaining)}
                        <span className="font-mono-tech w-8 text-right" style={{ color: 'var(--text-main)' }}>
                          {agyWkRemaining}%
                        </span>
                      </div>
                    </div>
                  </div>
                ) : (
                  <div className="space-y-1.5 pl-1">
                    <div className="flex items-center justify-between gap-2 text-[11.5px]">
                      <span className="font-mono-tech" style={{ color: 'var(--text-muted)' }}>
                        5h
                      </span>
                      <div className="flex items-center gap-2">
                        {renderUsageBar(agy5hRemaining)}
                        <span className="font-mono-tech w-8 text-right" style={{ color: 'var(--text-main)' }}>
                          {agy5hRemaining}%
                        </span>
                      </div>
                    </div>
                    <div className="flex items-center justify-between gap-2 text-[11.5px]">
                      <span className="font-mono-tech" style={{ color: 'var(--text-muted)' }}>
                        wk
                      </span>
                      <div className="flex items-center gap-2">
                        {renderUsageBar(agyWkRemaining)}
                        <span className="font-mono-tech w-8 text-right" style={{ color: 'var(--text-main)' }}>
                          {agyWkRemaining}%
                        </span>
                      </div>
                    </div>
                  </div>
                )}

                <div className="font-mono-tech text-[9.5px] pt-0.5" style={{ color: 'var(--text-muted)' }}>
                  {agyQuota ? `${agyQuota.label}${agyQuota.email ? ` · ${agyQuota.email}` : ''}` : 'No usage yet'}
                </div>
              </div>

              {/* Kiro CLI Section (only when linked/present in quotas) */}
              {kiroQuota && (
                <div className="px-3.5 py-3 space-y-1.5">
                  <div className="flex items-center justify-between gap-2">
                    <div className="flex items-center gap-2 min-w-0">
                      <span
                        className="w-5 h-5 rounded-full border flex items-center justify-center shrink-0"
                        style={{ backgroundColor: 'var(--bg-elevated)', borderColor: 'var(--border-line)' }}
                      >
                        <HarnessIcon harness="kiro-cli" size={11} />
                      </span>
                      <span className="text-[12.5px] font-semibold truncate" style={{ color: 'var(--text-main)' }}>
                        Kiro CLI
                      </span>
                    </div>
                    <span className="font-mono-tech text-[10.5px] shrink-0" style={{ color: 'var(--text-muted)' }}>
                      resets {kiroQuota.weeklyReset}
                    </span>
                  </div>
                  <div className="flex items-center justify-between gap-2 text-[11px] pl-1">
                    <span className="truncate" style={{ color: 'var(--text-muted)' }}>
                      {kiroQuota.label} · 30d Remaining
                    </span>
                    <div className="flex items-center gap-1.5 shrink-0">
                      {renderUsageBar(kiroRemaining)}
                      <span className="font-mono-tech w-8 text-right" style={{ color: 'var(--text-main)' }}>
                        {kiroRemaining}%
                      </span>
                    </div>
                  </div>
                </div>
              )}

              {/* Claude Section (only when linked/present in quotas) */}
              {claudeQuota && (
                <div
                  className="px-3.5 py-3 space-y-1.5"
                  style={{ backgroundColor: 'var(--bg-elevated)' }}
                >
                  <div className="flex items-center justify-between gap-2">
                    <div className="flex items-center gap-2 min-w-0">
                      <span
                        className="w-5 h-5 rounded-full border flex items-center justify-center shrink-0"
                        style={{ backgroundColor: 'var(--bg-card)', borderColor: 'var(--border-line)' }}
                      >
                        <HarnessIcon harness="claude-code" size={11} />
                      </span>
                      <span className="text-[12.5px] font-semibold truncate" style={{ color: 'var(--text-main)' }}>
                        Claude
                      </span>
                    </div>
                    <span className="font-mono-tech text-[10.5px] shrink-0" style={{ color: 'var(--text-muted)' }}>
                      {claudeQuota.fiveHourPct}% used
                    </span>
                  </div>
                  <div className="flex items-center justify-between gap-2 text-[11px] pl-1">
                    <span className="truncate" style={{ color: 'var(--text-muted)' }}>
                      {claudeQuota.label} · 5h Remaining
                    </span>
                    <div className="flex items-center gap-1.5 shrink-0">
                      {renderUsageBar(Math.max(0, Math.min(100, 100 - claudeQuota.fiveHourPct)))}
                      <span className="font-mono-tech w-8 text-right" style={{ color: 'var(--text-main)' }}>
                        {Math.max(0, Math.min(100, 100 - claudeQuota.fiveHourPct))}%
                      </span>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      {dragItem?.active && (
        <div
          className="fixed z-[100] pointer-events-none px-2.5 py-1 rounded-[7px] border text-[11.5px] font-medium flex items-center gap-1.5 shadow-lg max-w-[220px]"
          style={{
            left: dragItem.x + 12,
            top: dragItem.y + 10,
            backgroundColor: 'var(--bg-card)',
            borderColor: 'var(--accent-blue)',
            color: 'var(--text-main)',
          }}
        >
          {dragItem.kind === 'project' ? (
            <FolderSimple size={12} className="shrink-0" style={{ color: 'var(--accent-blue)' }} />
          ) : (
            <ChatCircle size={12} className="shrink-0" style={{ color: 'var(--accent-blue)' }} />
          )}
          <span className="truncate">{dragItem.label}</span>
        </div>
      )}

      {threadCtxMenu && (
        <div
          ref={ctxMenuRef}
          className="sakana-popover pop-enter fixed z-50 w-52 rounded-[10px] p-1 space-y-0.5"
          style={{ left: threadCtxMenu.x, top: threadCtxMenu.y }}
          onContextMenu={(e) => {
            e.preventDefault();
            e.stopPropagation();
          }}
        >
          <button
            type="button"
            onClick={() => {
              setRenamingThreadId(threadCtxMenu.thread.id);
              setRenameDraft(threadCtxMenu.thread.title);
              setThreadCtxMenu(null);
            }}
            className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center gap-2 cursor-pointer hover:bg-[var(--bg-hover)]"
            style={{ color: 'var(--text-main)' }}
          >
            <PencilSimple size={13} style={{ color: 'var(--text-muted)' }} />
            <span>Rename thread</span>
          </button>
          {projects.length > 1 && onMoveThread && (
            <div>
              <button
                type="button"
                onClick={() =>
                  setThreadCtxMenu((prev) =>
                    prev ? { ...prev, showMoveList: !prev.showMoveList } : null,
                  )
                }
                className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center justify-between gap-2 cursor-pointer hover:bg-[var(--bg-hover)]"
                style={{ color: 'var(--text-main)' }}
              >
                <span className="inline-flex items-center gap-2">
                  <FolderSimple size={13} style={{ color: 'var(--text-muted)' }} />
                  <span>Move to project</span>
                </span>
                {threadCtxMenu.showMoveList ? (
                  <CaretDown size={11} style={{ color: 'var(--text-muted)' }} />
                ) : (
                  <CaretRight size={11} style={{ color: 'var(--text-muted)' }} />
                )}
              </button>
              {threadCtxMenu.showMoveList && (
                <div
                  className="mt-0.5 ml-3 pl-2 border-l space-y-0.5 max-h-40 overflow-y-auto"
                  style={{ borderColor: 'var(--border-line)' }}
                >
                  {projects
                    .filter((p) => p.id !== threadCtxMenu.thread.projectId)
                    .map((p) => (
                      <button
                        key={p.id}
                        type="button"
                        onClick={() => {
                          void onMoveThread(threadCtxMenu.thread.id, p.id);
                          setThreadCtxMenu(null);
                        }}
                        className="w-full px-2 py-1 rounded-[5px] text-[11.5px] flex items-center gap-1.5 cursor-pointer hover:bg-[var(--bg-hover)] truncate"
                        style={{ color: 'var(--text-body)' }}
                      >
                        <FolderOpen size={11} style={{ color: 'var(--text-muted)' }} className="shrink-0" />
                        <span className="truncate">{p.name}</span>
                      </button>
                    ))}
                </div>
              )}
            </div>
          )}
          {onSetThreadDiscordChannel && (
            <div>
              <button
                type="button"
                onClick={() =>
                  setThreadCtxMenu((prev) =>
                    prev ? { ...prev, showDiscordInput: !prev.showDiscordInput } : null,
                  )
                }
                className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center justify-between gap-2 cursor-pointer hover:bg-[var(--bg-hover)]"
                style={{ color: 'var(--text-main)' }}
              >
                <span className="inline-flex items-center gap-2 truncate">
                  <DiscordIcon size={13} style={{ color: 'var(--accent-discord)' }} />
                  <span className="truncate">
                    {threadCtxMenu.thread.isDiscord
                      ? `Discord (${threadCtxMenu.thread.discordChannel || 'linked'})`
                      : 'Make Discord Chat…'}
                  </span>
                </span>
              </button>
              {threadCtxMenu.showDiscordInput && (
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    const ch = threadCtxMenu.discordDraft.trim();
                    if (!ch) return;
                    void onSetThreadDiscordChannel(threadCtxMenu.thread.id, ch);
                    setThreadCtxMenu(null);
                  }}
                  className="px-2 py-1.5 space-y-1.5"
                >
                  <input
                    autoFocus
                    value={threadCtxMenu.discordDraft}
                    onChange={(e) =>
                      setThreadCtxMenu((prev) =>
                        prev ? { ...prev, discordDraft: e.target.value } : null,
                      )
                    }
                    placeholder="#channel-name"
                    className="w-full rounded-[5px] border px-2 py-1 font-mono-tech text-[11px] focus:outline-none"
                    style={{
                      backgroundColor: 'var(--bg-app)',
                      borderColor: 'var(--border-line)',
                      color: 'var(--text-main)',
                    }}
                  />
                  <div className="flex items-center gap-1">
                    <button
                      type="submit"
                      className="btn-solid flex-1 py-1 text-[11px] font-medium cursor-pointer"
                    >
                      Link
                    </button>
                    {threadCtxMenu.thread.isDiscord && (
                      <button
                        type="button"
                        onClick={() => {
                          void onSetThreadDiscordChannel(threadCtxMenu.thread.id, null);
                          setThreadCtxMenu(null);
                        }}
                        className="btn-ghost px-2 py-1 text-[11px] cursor-pointer"
                        style={{ color: 'var(--accent-red)' }}
                      >
                        Unlink
                      </button>
                    )}
                  </div>
                </form>
              )}
            </div>
          )}
          <div className="my-0.5 border-t" style={{ borderColor: 'var(--border-line)' }} />
          {awakeSessionIds.includes(threadCtxMenu.thread.id) && onSleepSession ? (
            <button
              type="button"
              onClick={() => {
                const id = threadCtxMenu.thread.id;
                setThreadCtxMenu(null);
                void onSleepSession(id);
              }}
              className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center gap-2 cursor-pointer hover:bg-[var(--bg-hover)]"
              style={{ color: 'var(--text-main)' }}
            >
              <Moon size={13} style={{ color: 'var(--accent-amber)' }} />
              <span>Put thread to sleep</span>
            </button>
          ) : threadCtxMenu.thread.harness === 'agy-cli' && onWakeSession ? (
            <button
              type="button"
              onClick={() => {
                const id = threadCtxMenu.thread.id;
                setThreadCtxMenu(null);
                void onWakeSession(id);
              }}
              className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center gap-2 cursor-pointer hover:bg-[var(--bg-hover)]"
              style={{ color: 'var(--text-main)' }}
            >
              <Lightning size={13} style={{ color: 'var(--accent-green)' }} />
              <span>Wake persistent worker</span>
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => {
              const id = threadCtxMenu.thread.id;
              setThreadCtxMenu(null);
              void onDeleteThread(id);
            }}
            className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center gap-2 cursor-pointer hover:bg-[var(--bg-hover)]"
            style={{ color: 'var(--accent-red)' }}
          >
            <Trash size={13} />
            <span>Delete thread</span>
          </button>
        </div>
      )}

      {projectCtxMenu && (
        <div
          ref={ctxMenuRef}
          className="sakana-popover pop-enter fixed z-50 w-56 rounded-[10px] p-1 space-y-0.5"
          style={{ left: projectCtxMenu.x, top: projectCtxMenu.y }}
          onContextMenu={(e) => {
            e.preventDefault();
            e.stopPropagation();
          }}
        >
          <button
            type="button"
            onClick={() => {
              const id = projectCtxMenu.project.id;
              setProjectCtxMenu(null);
              void onQuickNewThread(id);
            }}
            className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center gap-2 cursor-pointer hover:bg-[var(--bg-hover)]"
            style={{ color: 'var(--text-main)' }}
          >
            <Plus size={13} style={{ color: 'var(--text-muted)' }} />
            <span>New thread</span>
          </button>
          <button
            type="button"
            onClick={() => {
              setRenamingProjectId(projectCtxMenu.project.id);
              setRenameProjectDraft(projectCtxMenu.project.name);
              setProjectCtxMenu(null);
            }}
            className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center gap-2 cursor-pointer hover:bg-[var(--bg-hover)]"
            style={{ color: 'var(--text-main)' }}
          >
            <PencilSimple size={13} style={{ color: 'var(--text-muted)' }} />
            <span>Rename project</span>
          </button>
          {onSetProjectDiscordChannel && (
            <div>
              <button
                type="button"
                onClick={() =>
                  setProjectCtxMenu((prev) =>
                    prev ? { ...prev, showDiscordInput: !prev.showDiscordInput } : null,
                  )
                }
                className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center justify-between gap-2 cursor-pointer hover:bg-[var(--bg-hover)]"
                style={{ color: 'var(--text-main)' }}
              >
                <span className="inline-flex items-center gap-2 truncate">
                  <DiscordIcon size={13} style={{ color: 'var(--accent-discord)' }} />
                  <span className="truncate">
                    {projectCtxMenu.project.discordChannel
                      ? `Discord (${projectCtxMenu.project.discordChannel})`
                      : 'Make Discord Chat…'}
                  </span>
                </span>
              </button>
              {projectCtxMenu.showDiscordInput && (
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    const ch = projectCtxMenu.discordDraft.trim();
                    if (!ch) return;
                    void onSetProjectDiscordChannel(projectCtxMenu.project.id, ch);
                    setProjectCtxMenu(null);
                  }}
                  className="px-2 py-1.5 space-y-1.5"
                >
                  <input
                    autoFocus
                    value={projectCtxMenu.discordDraft}
                    onChange={(e) =>
                      setProjectCtxMenu((prev) =>
                        prev ? { ...prev, discordDraft: e.target.value } : null,
                      )
                    }
                    placeholder="#channel-name"
                    className="w-full rounded-[5px] border px-2 py-1 font-mono-tech text-[11px] focus:outline-none"
                    style={{
                      backgroundColor: 'var(--bg-app)',
                      borderColor: 'var(--border-line)',
                      color: 'var(--text-main)',
                    }}
                  />
                  <div className="flex items-center gap-1">
                    <button
                      type="submit"
                      className="btn-solid flex-1 py-1 text-[11px] font-medium cursor-pointer"
                    >
                      Bind
                    </button>
                    {projectCtxMenu.project.discordChannel && (
                      <button
                        type="button"
                        onClick={() => {
                          void onSetProjectDiscordChannel(projectCtxMenu.project.id, '');
                          setProjectCtxMenu(null);
                        }}
                        className="btn-ghost px-2 py-1 text-[11px] cursor-pointer"
                        style={{ color: 'var(--accent-red)' }}
                      >
                        Unbind
                      </button>
                    )}
                  </div>
                </form>
              )}
            </div>
          )}
          {projects.length > 1 && (
            <>
              <div className="my-0.5 border-t" style={{ borderColor: 'var(--border-line)' }} />
              <button
                type="button"
                onClick={() => {
                  const id = projectCtxMenu.project.id;
                  setProjectCtxMenu(null);
                  void onDeleteProject(id);
                }}
                className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center gap-2 cursor-pointer hover:bg-[var(--bg-hover)]"
                style={{ color: 'var(--accent-red)' }}
              >
                <Trash size={13} />
                <span>Delete project</span>
              </button>
            </>
          )}
        </div>
      )}

      <FolderPickerModal
        open={folderModalOpen}
        title={
          activeHostId !== 'local'
            ? `Browse ${remoteHosts.find((h) => h.id === activeHostId)?.name || 'Remote Host'}`
            : `Browse WSL2 (${newProjWslDistro || 'Linux'})`
        }
        initialPath={newProjCwd || undefined}
        wslDistro={activeHostId === 'local' && newProjRuntime === 'wsl' ? newProjWslDistro : undefined}
        onSelect={(selected) => {
          setNewProjCwd(selected);
          if (!newProjName.trim()) {
            const base = selected.replace(/[\\/]+$/, '').split(/[\\/]/).pop();
            if (base) setNewProjName(base);
          }
          setFolderModalOpen(false);
        }}
        onClose={() => setFolderModalOpen(false)}
      />
    </aside>
  );
}
