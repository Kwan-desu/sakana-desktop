import { useEffect, useRef, useState } from 'react';
import { GitBranch, X } from '@phosphor-icons/react';
import type { HarnessId, HarnessModelOption, Project, ReasoningEffort } from '../types';
import { HARNESS_META } from './HarnessBadge';

const VALID_EFFORTS: ReasoningEffort[] = ['low', 'medium', 'high', 'xhigh'];
const EFFORT_LABELS: Record<Exclude<ReasoningEffort, ''>, string> = {
  low: 'Low',
  medium: 'Medium',
  high: 'High',
  xhigh: 'XHigh',
};

interface NewWorktreeModalProps {
  open: boolean;
  onClose: () => void;
  projects: Project[];
  harnessModels?: Record<string, HarnessModelOption[]>;
  onCreate: (opts: {
    projectId: string;
    title: string;
    branch: string;
    harness: HarnessId;
    model?: string;
    effort?: ReasoningEffort;
    sharedDeps: string[];
  }) => Promise<void> | void;
}

export function NewWorktreeModal({
  open,
  onClose,
  projects,
  harnessModels,
  onCreate,
}: NewWorktreeModalProps) {
  const [projectId, setProjectId] = useState(projects[0]?.id || '');
  const [title, setTitle] = useState('');
  const [branch, setBranch] = useState('feat/new-agent-task');
  const [harness, setHarness] = useState<HarnessId>('agy-cli');
  const [model, setModel] = useState('');
  const [effort, setEffort] = useState<ReasoningEffort>('medium');
  const [shareNode, setShareNode] = useState(true);
  const [shareVenv, setShareVenv] = useState(true);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const titleInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!open) return;
    titleInputRef.current?.focus();
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open, onClose]);

  const hasBackendModels = Boolean(harnessModels?.[harness]?.length);
  const availableModels: HarnessModelOption[] =
    hasBackendModels && harnessModels?.[harness]
      ? harnessModels[harness]
      : (HARNESS_META[harness]?.models ?? []).map((id) => ({
          id,
          label: id,
          family: harness,
          efforts: [],
        }));

  const currentModelEntry = availableModels.find((m) => m.id === model);
  const validEfforts: ReasoningEffort[] = hasBackendModels
    ? (currentModelEntry?.efforts ?? []).filter((e): e is ReasoningEffort =>
        VALID_EFFORTS.includes(e as ReasoningEffort),
      )
    : VALID_EFFORTS;

  useEffect(() => {
    if (!availableModels.some((m) => m.id === model)) {
      setModel(availableModels[0]?.id || '');
    }
  }, [harness, availableModels, model]);

  useEffect(() => {
    if (validEfforts.length === 0) {
      if (effort !== '') setEffort('');
    } else if (!validEfforts.includes(effort)) {
      setEffort(validEfforts.includes('medium') ? 'medium' : validEfforts[0]);
    }
  }, [validEfforts, effort]);

  useEffect(() => {
    if (!projectId && projects[0]?.id) {
      setProjectId(projects[0].id);
    } else if (projectId && projects.length > 0 && !projects.some((p) => p.id === projectId)) {
      setProjectId(projects[0].id);
    }
  }, [projects, projectId]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/45 backdrop-blur-[1px] flex items-center justify-center p-4"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="New Worktree Thread"
        className="sakana-popover pop-enter w-full max-w-md rounded-[14px] p-5 space-y-4"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between border-b pb-3" style={{ borderColor: 'var(--border-line)' }}>
          <div className="flex items-center gap-2">
            <GitBranch size={17} style={{ color: 'var(--accent-blue)' }} />
            <div>
              <h2 className="text-[15px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                New Worktree Thread
              </h2>
              <p className="text-[11.5px] m-0" style={{ color: 'var(--text-muted)' }}>
                Isolated checkout so parallel agents never collide
              </p>
            </div>
          </div>
          <button type="button" onClick={onClose} aria-label="Close" className="p-1 cursor-pointer" style={{ color: 'var(--text-muted)' }}>
            <X size={15} />
          </button>
        </div>

        <form
          onSubmit={async (e) => {
            e.preventDefault();
            if (submitting) return;
            const targetProjectId = projectId || projects[0]?.id || '';
            if (!targetProjectId) return;
            const deps: string[] = [];
            if (shareNode) deps.push('node_modules');
            if (shareVenv) deps.push('.venv');
            setSubmitting(true);
            setError(null);
            try {
              await onCreate({
                projectId: targetProjectId,
                title: title.trim() || 'Untitled worktree task',
                branch: branch.trim() || 'feat/task',
                harness,
                model: model || undefined,
                effort,
                sharedDeps: deps,
              });
              setTitle('');
              onClose();
            } catch (err) {
              setError(String(err));
            } finally {
              setSubmitting(false);
            }
          }}
          className="space-y-3"
        >
          <div>
            <label className="block text-[11.5px] font-medium mb-1" style={{ color: 'var(--text-muted)' }}>
              Project
            </label>
            <select
              value={projectId}
              onChange={(e) => setProjectId(e.target.value)}
              className="w-full rounded-[7px] border px-3 py-1.5 text-[12.5px]"
              style={{
                backgroundColor: 'var(--bg-app)',
                borderColor: 'var(--border-line)',
                color: 'var(--text-main)',
              }}
            >
              {projects.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} ({p.cwd})
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-[11.5px] font-medium mb-1" style={{ color: 'var(--text-muted)' }}>
              Task Title
            </label>
            <input
              ref={titleInputRef}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="e.g. Refactor auth middleware"
              className="w-full rounded-[7px] border px-3 py-1.5 text-[12.5px] focus:outline-none"
              style={{
                backgroundColor: 'var(--bg-app)',
                borderColor: 'var(--border-line)',
                color: 'var(--text-main)',
              }}
            />
          </div>

          <div className="grid grid-cols-2 gap-2.5">
            <div>
              <label className="block text-[11.5px] font-medium mb-1" style={{ color: 'var(--text-muted)' }}>
                Worktree Branch
              </label>
              <input
                value={branch}
                onChange={(e) => setBranch(e.target.value)}
                className="w-full rounded-[7px] border px-2.5 py-1.5 font-mono-tech text-[11.5px] focus:outline-none"
                style={{
                  backgroundColor: 'var(--bg-app)',
                  borderColor: 'var(--border-line)',
                  color: 'var(--text-main)',
                }}
              />
            </div>
            <div>
              <label className="block text-[11.5px] font-medium mb-1" style={{ color: 'var(--text-muted)' }}>
                Harness
              </label>
              <select
                value={harness}
                onChange={(e) => setHarness(e.target.value as HarnessId)}
                className="w-full rounded-[7px] border px-2.5 py-1.5 text-[12px]"
                style={{
                  backgroundColor: 'var(--bg-app)',
                  borderColor: 'var(--border-line)',
                  color: 'var(--text-main)',
                }}
              >
                <option value="agy-cli">AGY CLI</option>
                <option value="claude-code">Claude Code</option>
                <option value="kiro-cli">Kiro CLI</option>
                <option value="sakana">Sakana Master</option>
              </select>
            </div>
          </div>

          <div className={validEfforts.length > 0 ? 'grid grid-cols-2 gap-2.5' : ''}>
            <div>
              <label className="block text-[11.5px] font-medium mb-1" style={{ color: 'var(--text-muted)' }}>
                Model
              </label>
              <select
                value={model}
                onChange={(e) => setModel(e.target.value)}
                className="w-full rounded-[7px] border px-2.5 py-1.5 font-mono-tech text-[12px]"
                style={{
                  backgroundColor: 'var(--bg-app)',
                  borderColor: 'var(--border-line)',
                  color: 'var(--text-main)',
                }}
              >
                {availableModels.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label}
                  </option>
                ))}
              </select>
            </div>
            {validEfforts.length > 0 && (
              <div>
                <label className="block text-[11.5px] font-medium mb-1" style={{ color: 'var(--text-muted)' }}>
                  Effort
                </label>
                <select
                  value={effort}
                  onChange={(e) => setEffort(e.target.value as ReasoningEffort)}
                  className="w-full rounded-[7px] border px-2.5 py-1.5 text-[12px]"
                  style={{
                    backgroundColor: 'var(--bg-app)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-main)',
                  }}
                >
                  {validEfforts.map((ef) => (
                    <option key={ef} value={ef}>
                      {ef ? EFFORT_LABELS[ef] : ef}
                    </option>
                  ))}
                </select>
              </div>
            )}
          </div>

          <div className="pt-1 space-y-1.5">
            <div className="text-[11px] font-medium" style={{ color: 'var(--text-muted)' }}>
              Symlink Shared Caches
            </div>
            <div className="flex items-center gap-4 text-[12px]" style={{ color: 'var(--text-body)' }}>
              <label className="inline-flex items-center gap-1.5 cursor-pointer">
                <input type="checkbox" checked={shareNode} onChange={(e) => setShareNode(e.target.checked)} />
                <span className="font-mono-tech">node_modules</span>
              </label>
              <label className="inline-flex items-center gap-1.5 cursor-pointer">
                <input type="checkbox" checked={shareVenv} onChange={(e) => setShareVenv(e.target.checked)} />
                <span className="font-mono-tech">.venv</span>
              </label>
            </div>
          </div>

          {error && (
            <div
              className="rounded-[7px] border px-3 py-2 text-[11.5px] font-mono-tech"
              style={{
                borderColor: 'var(--accent-red)',
                color: 'var(--accent-red)',
                backgroundColor: 'var(--diff-del-bg)',
              }}
            >
              {error}
            </div>
          )}

          <div className="flex justify-end gap-2 pt-2">
            <button
              type="button"
              onClick={onClose}
              disabled={submitting}
              className="btn-ghost px-3.5 py-1.5 text-[12px] cursor-pointer disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="btn-solid px-4 py-1.5 text-[12px] font-medium cursor-pointer disabled:opacity-50"
            >
              {submitting ? 'Spawning...' : 'Spawn Worktree'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
