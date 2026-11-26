import { useEffect, useRef, useState } from 'react';
import {
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
  Globe,
  ListChecks,
  MagnifyingGlass,
  Plugs,
  Plus,
  SidebarSimple,
  SquaresFour,
  Trash,
  X,
} from '@phosphor-icons/react';
import type {
  DashboardSubtab,
  HarnessId,
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
  onDeleteThread: (id: string) => Promise<void> | void;
  dashSubtab: DashboardSubtab;
  onSelectDashSubtab: (tab: DashboardSubtab) => void;
  quotas: QuotaAccount[];
  onRefreshQuotas?: () => void;
  pendingApprovalCount: number;
  remoteHosts: RemoteHost[];
  activeHostId: string;
  onSwitchHost: (hostId: string) => void;
  onManageHosts: () => void;
}

export function Sidebar({
  open,
  width = 272,
  onToggleOpen,
  primarySection,
  onSwitchSection,
  projects,
  threads,
  selectedThreadId,
  onSelectThread,
  onQuickNewThread,
  onNewWorktreeThread,
  onCreateProject,
  onDeleteProject,
  onDeleteThread,
  dashSubtab,
  onSelectDashSubtab,
  quotas,
  onRefreshQuotas,
  pendingApprovalCount,
  remoteHosts,
  activeHostId,
  onSwitchHost,
  onManageHosts,
}: SidebarProps) {
  const [search, setSearch] = useState('');
  const [harnessFilter, setHarnessFilter] = useState<HarnessId | 'all'>('all');
  const [harnessMenuOpen, setHarnessMenuOpen] = useState(false);
  const [hostMenuOpen, setHostMenuOpen] = useState(false);
  const [collapsedProjects, setCollapsedProjects] = useState<Record<string, boolean>>({});
  const [usageOpen, setUsageOpen] = useState(false);
  const [usageDetailed, setUsageDetailed] = useState(true);
  const [addingProject, setAddingProject] = useState(false);
  const [newProjName, setNewProjName] = useState('');
  const [newProjCwd, setNewProjCwd] = useState('');
  const [newProjRuntime, setNewProjRuntime] = useState<'native' | 'wsl'>('native');
  const [newProjWslDistro, setNewProjWslDistro] = useState('');
  const [wslDistros, setWslDistros] = useState<WslDistro[]>([]);
  const [folderModalOpen, setFolderModalOpen] = useState(false);
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
  let footerPctNum = 0;
  if (activeHarness === 'kiro-cli' && kiroQuota) {
    footerWinLabel = '30d';
    footerPctNum = kiroQuota.fiveHourPct;
  } else if (activeHarness === 'claude-code' && claudeQuota) {
    if (claudeQuota.fiveHourPct >= 100) {
      footerWinLabel = '7d';
      footerPctNum = claudeQuota.weeklyPct;
    } else {
      footerWinLabel = '5h';
      footerPctNum = claudeQuota.fiveHourPct;
    }
  } else if (activeHarness === 'sakana' && sakanaQuota) {
    footerWinLabel = '5h';
    footerPctNum = sakanaQuota.fiveHourPct;
  } else if (agyQuota) {
    if (agyQuota.fiveHourPct >= 100) {
      footerWinLabel = '7d';
      footerPctNum = agyQuota.weeklyPct;
    } else {
      footerWinLabel = '5h';
      footerPctNum = agyQuota.fiveHourPct;
    }
  }
  const footerPctColor =
    footerPctNum >= 90
      ? 'var(--accent-red)'
      : footerPctNum >= 70
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
    if (t.status === 'AWAITING_APPROVAL') {
      return (
        <span
          className="h-2 w-2 rounded-full shrink-0"
          style={{ backgroundColor: 'var(--accent-yellow)' }}
          title="Waiting for Terminal Guard approval"
        />
      );
    }
    if (t.status === 'DONE' && t.unreadDone) {
      return (
        <span
          className="h-2 w-2 rounded-full shrink-0"
          style={{ backgroundColor: 'var(--accent-green)' }}
          title="Completed"
        />
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
            onClick={() => onSwitchSection('dashboard')}
            className="h-7 px-2 rounded-[6px] text-[12px] font-medium flex items-center justify-center gap-1.5 cursor-pointer transition-colors"
            style={
              primarySection === 'dashboard'
                ? { backgroundColor: 'var(--bg-card)', color: 'var(--text-main)', boxShadow: 'var(--surface-shadow)' }
                : { color: 'var(--text-muted)' }
            }
          >
            <ListChecks size={13} weight={primarySection === 'dashboard' ? 'fill' : 'regular'} />
            <span>Dashboard</span>
            {pendingApprovalCount > 0 && (
              <span
                className="font-mono-tech text-[9.5px] px-1.5 rounded-full"
                style={{ backgroundColor: 'var(--accent-yellow-bg)', color: 'var(--accent-yellow)' }}
              >
                {pendingApprovalCount}
              </span>
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

                return (
                  <div key={proj.id} className="space-y-0.5">
                    <div className="group flex items-center gap-1">
                      <button
                        type="button"
                        onClick={() => toggleProject(proj.id)}
                        className="flex-1 min-w-0 px-2 py-1 rounded-[6px] flex items-center justify-between gap-1.5 hover:bg-[var(--bg-hover)] cursor-pointer"
                      >
                        <span className="inline-flex items-center gap-1.5 min-w-0">
                          {collapsed ? (
                            <CaretRight size={11} style={{ color: 'var(--text-muted)' }} />
                          ) : (
                            <CaretDown size={11} style={{ color: 'var(--text-muted)' }} />
                          )}
                          <FolderSimple size={13} style={{ color: 'var(--text-muted)' }} />
                          <span className="text-[12px] font-medium truncate" style={{ color: 'var(--text-main)' }}>
                            {proj.name}
                          </span>
                          {(proj.runtime === 'wsl' || proj.wslDistro) && (
                            <span
                              className="font-mono-tech text-[9px] px-1 rounded-[4px] shrink-0"
                              style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }}
                              title={`WSL2 · ${proj.wslDistro || 'default'}`}
                            >
                              WSL
                            </span>
                          )}
                          {proj.discordChannel && (
                            <span style={{ color: 'var(--accent-discord)' }} title={`Discord ${proj.discordChannel}`}>
                              <DiscordIcon size={11} />
                            </span>
                          )}
                        </span>
                        <span className="font-mono-tech text-[10px]" style={{ color: 'var(--text-muted)' }}>
                          {projThreads.length}
                        </span>
                      </button>
                      <button
                        type="button"
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
                            return (
                              <div
                                key={t.id}
                                className="group relative"
                              >
                                <button
                                  type="button"
                                  onClick={() => onSelectThread(t.id)}
                                  className={`w-full text-left px-2 py-1.5 rounded-[7px] border transition-colors cursor-pointer space-y-0.5 ${
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
                                    <span className="inline-flex items-center gap-1.5 min-w-0">
                                      {renderStatusDot(t)}
                                      <span
                                        className={`text-[12px] truncate ${active ? 'font-semibold' : 'font-normal'}`}
                                        style={{ color: active ? 'var(--text-main)' : 'var(--text-body)' }}
                                      >
                                        {t.title}
                                      </span>
                                    </span>
                                    <span className="font-mono-tech text-[10px] shrink-0 group-hover:opacity-0 transition-opacity" style={{ color: 'var(--text-muted)' }}>
                                      {t.timeAgo}
                                    </span>
                                  </div>

                                  <div className="flex items-center justify-between gap-2 pl-3.5">
                                    <span
                                      className="inline-flex items-center gap-1.5 font-mono-tech text-[10px] truncate"
                                      style={{ color: active ? 'var(--text-body)' : 'var(--text-muted)' }}
                                    >
                                      <HarnessIcon harness={t.harness} size={11} />
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
                              </div>
                            );
                          })
                        )}
                      </div>
                    )}
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
                {item.id === 'overview' && pendingApprovalCount > 0 && (
                  <span
                    className="font-mono-tech text-[10px] px-1.5 rounded-full"
                    style={{ backgroundColor: 'var(--accent-yellow-bg)', color: 'var(--accent-yellow)' }}
                  >
                    {pendingApprovalCount}
                  </span>
                )}
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
          title="Click to view all harness quotas"
        >
          <HarnessIcon harness={activeHarness} size={13} />
          <span style={{ color: 'var(--text-muted)' }}>{footerWinLabel}</span>
          <span className="font-semibold" style={{ color: footerPctColor }}>
            {footerPctNum}%
          </span>
        </button>

        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => {
              setUsageOpen(false);
              onSelectDashSubtab('remote');
            }}
            className="btn-ghost px-2 py-1 text-[11px] inline-flex items-center gap-1.5 cursor-pointer"
            title="Open Preferences & System"
          >
            <GearSix size={13} />
            <span>Preferences</span>
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

              {/* Gemini Bridge Section */}
              <div className="px-3.5 py-3 space-y-1.5">
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-2 min-w-0">
                    <span
                      className="w-5 h-5 rounded-full border flex items-center justify-center shrink-0"
                      style={{ backgroundColor: 'var(--bg-elevated)', borderColor: 'var(--border-line)' }}
                    >
                      <HarnessIcon harness="sakana" size={11} />
                    </span>
                    <span className="text-[12.5px] font-semibold truncate" style={{ color: 'var(--text-main)' }}>
                      Gemini Bridge
                    </span>
                  </div>
                  <span className="font-mono-tech text-[10.5px] shrink-0" style={{ color: 'var(--text-muted)' }}>
                    {sakanaQuota ? `${sakanaQuota.fiveHourPct} (5h) · ${sakanaQuota.weeklyPct} (7d)` : '0 (5h) · 0 (7d)'}
                  </span>
                </div>
                <div className="flex items-center justify-between gap-2 text-[11px] pl-1">
                  <span className="truncate" style={{ color: 'var(--text-muted)' }}>
                    {sakanaQuota ? sakanaQuota.label : 'No usage yet'}
                  </span>
                  <span className="font-mono-tech text-[10px] shrink-0" style={{ color: 'var(--text-muted)' }}>
                    No cap
                  </span>
                </div>
              </div>

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
