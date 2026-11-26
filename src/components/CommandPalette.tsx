import { useEffect, useRef, useState } from 'react';
import {
  Clock,
  Columns,
  Cpu,
  Crosshair,
  FileCode,
  FolderSimple,
  GitBranch,
  GitDiff,
  GitPullRequest,
  Globe,
  ListChecks,
  MagnifyingGlass,
  Palette,
  Plugs,
  SquaresFour,
  Terminal,
  X,
} from '@phosphor-icons/react';
import type {
  DashboardSubtab,
  InspectorTab,
  Project,
  ThemeId,
  Thread,
} from '../types';
import { DiscordIcon, THEME_META } from './HarnessBadge';

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  threads: Thread[];
  projects: Project[];
  onSelectThread: (id: string) => void;
  onSelectProject: (projectId: string) => void;
  onSelectInspectorTab: (tab: InspectorTab) => void;
  onSelectDashSubtab: (tab: DashboardSubtab) => void;
  onSetTheme: (t: ThemeId) => void;
  onToggleSplit: () => void;
  onOpenNewWorktree: () => void;
  onOpenCreatePr: () => void;
}

export function CommandPalette({
  open,
  onClose,
  threads,
  projects,
  onSelectThread,
  onSelectProject,
  onSelectInspectorTab,
  onSelectDashSubtab,
  onSetTheme,
  onToggleSplit,
  onOpenNewWorktree,
  onOpenCreatePr,
}: CommandPaletteProps) {
  const [query, setQuery] = useState('');
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!open) return;
    inputRef.current?.focus();
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  const q = query.trim().toLowerCase();

  const inspectorItems: { id: InspectorTab; label: string; icon: React.ReactNode }[] = [
    { id: 'changes', label: 'Inspector: Changes & Git Diff', icon: <GitDiff size={14} /> },
    { id: 'files', label: 'Inspector: Workspace Files', icon: <FileCode size={14} /> },
    { id: 'preview', label: 'Inspector: Live Preview & Design Mode', icon: <Crosshair size={14} /> },
    { id: 'terminal', label: 'Inspector: Interactive Terminal', icon: <Terminal size={14} /> },
    { id: 'traces', label: 'Inspector: Execution Traces', icon: <ListChecks size={14} /> },
  ];

  const dashboardItems: { id: DashboardSubtab; label: string; icon: React.ReactNode }[] = [
    { id: 'overview', label: 'Dashboard: Overview & Telemetry', icon: <SquaresFour size={14} /> },
    { id: 'harnesses', label: 'Dashboard: Harnesses & Quotas', icon: <Cpu size={14} /> },
    { id: 'mcp', label: 'Dashboard: MCP Hub & Skills', icon: <Plugs size={14} /> },
    { id: 'automations', label: 'Dashboard: Cron & Webhook Automations', icon: <Clock size={14} /> },
    { id: 'discord', label: 'Dashboard: Discord Bot & RBAC', icon: <DiscordIcon size={14} /> },
    { id: 'remote', label: 'Dashboard: Remote Access & .env Secrets', icon: <Globe size={14} /> },
  ];

  const actions = [
    {
      id: 'new-wt',
      label: 'New Isolated Worktree Thread',
      hint: 'Git Worktree',
      icon: <GitBranch size={14} />,
      run: () => {
        onClose();
        onOpenNewWorktree();
      },
    },
    {
      id: 'create-pr',
      label: 'Create Pull Request from Active Worktree',
      hint: 'Git / PR',
      icon: <GitPullRequest size={14} />,
      run: () => {
        onClose();
        onOpenCreatePr();
      },
    },
    {
      id: 'split',
      label: 'Toggle 2-Thread Split View',
      hint: 'Layout',
      icon: <Columns size={14} />,
      run: () => {
        onClose();
        onToggleSplit();
      },
    },
    ...inspectorItems.map((item) => ({
      id: `insp-${item.id}`,
      label: item.label,
      hint: 'Inspector',
      icon: item.icon,
      run: () => {
        onSelectInspectorTab(item.id);
        onClose();
      },
    })),
    ...dashboardItems.map((item) => ({
      id: `dash-${item.id}`,
      label: item.label,
      hint: 'Dashboard',
      icon: item.icon,
      run: () => {
        onSelectDashSubtab(item.id);
        onClose();
      },
    })),
    ...THEME_META.map((tm) => ({
      id: `theme-${tm.id}`,
      label: `Switch Theme: ${tm.label}`,
      hint: tm.kind,
      icon: <Palette size={14} />,
      run: () => {
        onSetTheme(tm.id);
        onClose();
      },
    })),
    ...projects.map((p) => ({
      id: `proj-${p.id}`,
      label: `Project: ${p.name}`,
      hint: p.cwd,
      icon: <FolderSimple size={14} />,
      run: () => {
        onSelectProject(p.id);
        onClose();
      },
    })),
    ...threads.map((t) => ({
      id: `thr-${t.id}`,
      label: `Jump to: ${t.title}`,
      hint: `${t.branch} · ${t.harness}`,
      icon: <GitBranch size={14} />,
      run: () => {
        onSelectThread(t.id);
        onClose();
      },
    })),
  ].filter((a) => !q || `${a.label} ${a.hint}`.toLowerCase().includes(q));

  return (
    <div
      className="fixed inset-0 z-50 bg-black/45 backdrop-blur-[1px] flex items-start justify-center pt-[12vh] p-4"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        className="sakana-popover pop-enter w-full max-w-lg rounded-[14px] overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className="px-3.5 py-2.5 border-b flex items-center gap-2.5"
          style={{ borderColor: 'var(--border-line)' }}
        >
          <MagnifyingGlass size={15} style={{ color: 'var(--text-muted)' }} />
          <input
            ref={inputRef}
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Type a command, project, inspector, dashboard, theme, or thread..."
            className="flex-1 bg-transparent text-[13.5px] focus:outline-none"
            style={{ color: 'var(--text-main)' }}
          />
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="p-1 rounded cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
          >
            <X size={14} />
          </button>
        </div>

        <div className="max-h-[380px] overflow-y-auto p-1.5 space-y-0.5">
          {actions.length === 0 ? (
            <div className="py-6 text-center text-[12px]" style={{ color: 'var(--text-muted)' }}>
              No matching commands, projects, or threads.
            </div>
          ) : (
            actions.map((a) => (
              <button
                key={a.id}
                type="button"
                onClick={a.run}
                className="w-full px-3 py-2 rounded-[8px] flex items-center justify-between gap-3 text-[12.5px] hover:bg-[var(--bg-hover)] cursor-pointer text-left"
                style={{ color: 'var(--text-main)' }}
              >
                <span className="inline-flex items-center gap-2.5 min-w-0">
                  <span style={{ color: 'var(--text-muted)' }}>{a.icon}</span>
                  <span className="truncate">{a.label}</span>
                </span>
                <span className="font-mono-tech text-[10.5px] shrink-0 max-w-[180px] truncate" style={{ color: 'var(--text-muted)' }}>
                  {a.hint}
                </span>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
