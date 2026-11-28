import { useEffect, useState } from 'react';
import {
  CheckCircle,
  CircleNotch,
  FlagCheckered,
  GitMerge,
  Play,
  WarningCircle,
  X,
} from '@phosphor-icons/react';
import type {
  HarnessId,
  HarnessModelOption,
  Project,
  RaceContestantInput,
  RaceRun,
} from '../types';
import { tauriApi } from '../lib/tauriApi';
import { HARNESS_META, HarnessIcon } from './HarnessBadge';

interface RaceModalProps {
  open: boolean;
  onClose: () => void;
  projects: Project[];
  activeProjectId?: string;
  harnessModels?: Record<string, HarnessModelOption[]>;
  onToast: (msg: string) => void;
  onMerged?: () => void;
}

const RACE_HARNESSES: HarnessId[] = ['claude-code', 'agy-cli', 'kiro-cli', 'sakana'];

export function RaceModal({
  open,
  onClose,
  projects,
  activeProjectId,
  harnessModels,
  onToast,
  onMerged,
}: RaceModalProps) {
  const [projectId, setProjectId] = useState(activeProjectId || projects[0]?.id || '');
  const [prompt, setPrompt] = useState('');
  const [verifyCmd, setVerifyCmd] = useState('');
  const [selected, setSelected] = useState<Record<HarnessId, boolean>>({
    'claude-code': true,
    'agy-cli': true,
    'kiro-cli': false,
    sakana: false,
  });
  const [models, setModels] = useState<Record<HarnessId, string>>({
    'claude-code': '',
    'agy-cli': '',
    'kiro-cli': '',
    sakana: '',
  });
  const [races, setRaces] = useState<RaceRun[]>([]);
  const [activeRace, setActiveRace] = useState<RaceRun | null>(null);
  const [starting, setStarting] = useState(false);
  const [mergingId, setMergingId] = useState<string | null>(null);

  useEffect(() => {
    if (activeProjectId) setProjectId(activeProjectId);
  }, [activeProjectId]);

  useEffect(() => {
    if (!open) return;
    tauriApi.listRaces().then((list) => {
      setRaces(list);
      if (list.length > 0 && !activeRace) {
        setActiveRace(list[0]);
      }
    });
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    tauriApi.onRaceUpdate((run) => {
      if (cancelled) return;
      setRaces((prev) => {
        const idx = prev.findIndex((r) => r.id === run.id);
        if (idx >= 0) {
          const copy = [...prev];
          copy[idx] = run;
          return copy;
        }
        return [run, ...prev];
      });
      setActiveRace((cur) => (cur && cur.id === run.id ? run : cur ?? run));
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [open]);

  if (!open) return null;

  const handleStart = async () => {
    if (!projectId) {
      onToast('Select a project first');
      return;
    }
    if (!prompt.trim()) {
      onToast('Enter a prompt for the race');
      return;
    }
    const contestants: RaceContestantInput[] = RACE_HARNESSES.filter((h) => selected[h]).map(
      (h) => ({
        harness: h,
        model: models[h] || harnessModels?.[h]?.[0]?.id || HARNESS_META[h].models[0] || '',
      }),
    );
    if (contestants.length < 2) {
      onToast('Select at least 2 harnesses to race');
      return;
    }
    setStarting(true);
    try {
      const run = await tauriApi.startRace(
        projectId,
        prompt.trim(),
        contestants,
        verifyCmd.trim() || undefined,
      );
      if (run) {
        setActiveRace(run);
        setRaces((prev) => [run, ...prev.filter((r) => r.id !== run.id)]);
        onToast(`Race started with ${contestants.length} harnesses`);
      }
    } catch (e) {
      onToast(`Failed to start race: ${String(e)}`);
    } finally {
      setStarting(false);
    }
  };

  const handleMerge = async (raceId: string, sessionId: string) => {
    setMergingId(sessionId);
    try {
      const updated = await tauriApi.mergeRaceWinner(raceId, sessionId);
      if (updated) {
        setActiveRace(updated);
        setRaces((prev) => prev.map((r) => (r.id === updated.id ? updated : r)));
        onToast('Winner merged into base branch');
        onMerged?.();
      }
    } catch (e) {
      onToast(`Merge failed: ${String(e)}`);
    } finally {
      setMergingId(null);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 bg-black/50 backdrop-blur-[1px] flex items-center justify-center p-4"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Multi-Harness Parallel Race"
        className="sakana-popover pop-enter w-full max-w-4xl max-h-[86vh] rounded-[14px] flex flex-col overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div
          className="px-4 py-3 border-b flex items-center justify-between"
          style={{ borderColor: 'var(--border-line)' }}
        >
          <div className="flex items-center gap-2">
            <FlagCheckered size={16} style={{ color: 'var(--accent-blue)' }} />
            <span className="font-semibold text-[14px]" style={{ color: 'var(--text-main)' }}>
              Multi-Harness Parallel Race
            </span>
            <span className="text-[11px] font-mono-tech px-1.5 py-0.5 rounded" style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }}>
              Isolated Git Worktrees
            </span>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="p-1 rounded hover:bg-[var(--bg-hover)] cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
          >
            <X size={15} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-4 space-y-4">
          {/* Setup Form */}
          <div
            className="rounded-[10px] border p-3.5 space-y-3"
            style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
          >
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div>
                <label className="block text-[11px] font-mono-tech mb-1" style={{ color: 'var(--text-muted)' }}>
                  Project
                </label>
                <select
                  value={projectId}
                  onChange={(e) => setProjectId(e.target.value)}
                  className="w-full px-2.5 py-1.5 rounded-[7px] border text-[12.5px] bg-transparent"
                  style={{ borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                >
                  {projects.map((p) => (
                    <option key={p.id} value={p.id} style={{ backgroundColor: 'var(--bg-elevated)' }}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className="block text-[11px] font-mono-tech mb-1" style={{ color: 'var(--text-muted)' }}>
                  Optional Verify Command (runs after each contestant)
                </label>
                <input
                  type="text"
                  value={verifyCmd}
                  onChange={(e) => setVerifyCmd(e.target.value)}
                  placeholder="e.g. npm test or cargo test"
                  className="w-full px-2.5 py-1.5 rounded-[7px] border text-[12.5px] bg-transparent"
                  style={{ borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
                />
              </div>
            </div>

            <div>
              <label className="block text-[11px] font-mono-tech mb-1" style={{ color: 'var(--text-muted)' }}>
                Race Prompt
              </label>
              <textarea
                rows={2}
                value={prompt}
                onChange={(e) => setPrompt(e.target.value)}
                placeholder="Describe the task each harness should solve in parallel..."
                className="w-full px-2.5 py-1.5 rounded-[7px] border text-[12.5px] bg-transparent resize-none"
                style={{ borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
              />
            </div>

            <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
              {RACE_HARNESSES.map((h) => {
                const active = selected[h];
                const opts = harnessModels?.[h]?.map((m) => m.id) || HARNESS_META[h].models;
                return (
                  <div
                    key={h}
                    className="rounded-[8px] border p-2 space-y-1.5"
                    style={{
                      borderColor: active ? 'var(--accent-blue)' : 'var(--border-line)',
                      backgroundColor: active ? 'var(--bg-elevated)' : 'transparent',
                    }}
                  >
                    <label className="flex items-center gap-1.5 text-[12px] font-medium cursor-pointer" style={{ color: 'var(--text-main)' }}>
                      <input
                        type="checkbox"
                        checked={active}
                        onChange={(e) => setSelected((prev) => ({ ...prev, [h]: e.target.checked }))}
                      />
                      <HarnessIcon harness={h} size={13} />
                      <span className="truncate">{HARNESS_META[h].name}</span>
                    </label>
                    <select
                      value={models[h] || opts[0] || ''}
                      disabled={!active}
                      onChange={(e) => setModels((prev) => ({ ...prev, [h]: e.target.value }))}
                      className="w-full px-1.5 py-1 rounded border text-[10.5px] font-mono-tech bg-transparent disabled:opacity-45"
                      style={{ borderColor: 'var(--border-line)', color: 'var(--text-body)' }}
                    >
                      {opts.map((m) => (
                        <option key={m} value={m} style={{ backgroundColor: 'var(--bg-elevated)' }}>
                          {m}
                        </option>
                      ))}
                    </select>
                  </div>
                );
              })}
            </div>

            <div className="flex items-center justify-between">
              <div className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
                Each contestant runs in its own temporary git worktree & branch.
              </div>
              <button
                type="button"
                disabled={starting}
                onClick={handleStart}
                className="btn-primary px-3.5 py-1.5 text-[12px] inline-flex items-center gap-1.5 cursor-pointer disabled:opacity-50"
              >
                {starting ? <CircleNotch size={13} className="animate-spin" /> : <Play size={13} weight="fill" />}
                <span>{starting ? 'Launching...' : 'Start Race'}</span>
              </button>
            </div>
          </div>

          {/* Race Selector if multiple */}
          {races.length > 1 && (
            <div className="flex items-center gap-2 overflow-x-auto pb-1">
              {races.slice(0, 6).map((r) => (
                <button
                  key={r.id}
                  type="button"
                  onClick={() => setActiveRace(r)}
                  className="px-2.5 py-1 rounded-[7px] border text-[11px] font-mono-tech shrink-0 cursor-pointer"
                  style={{
                    borderColor: activeRace?.id === r.id ? 'var(--accent-blue)' : 'var(--border-line)',
                    backgroundColor: activeRace?.id === r.id ? 'var(--bg-elevated)' : 'transparent',
                    color: 'var(--text-main)',
                  }}
                >
                  {r.prompt.slice(0, 28)} ({r.status})
                </button>
              ))}
            </div>
          )}

          {/* Active Race Contestants */}
          {activeRace && (
            <div className="space-y-2.5">
              <div className="flex items-center justify-between text-[12px]">
                <div className="font-medium truncate" style={{ color: 'var(--text-main)' }}>
                  Race: {activeRace.prompt}
                </div>
                <span className="font-mono-tech text-[11px] uppercase px-2 py-0.5 rounded" style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }}>
                  {activeRace.status}
                </span>
              </div>

              <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                {activeRace.contestants.map((c) => {
                  const isWinner = activeRace.winnerSessionId === c.sessionId;
                  return (
                    <div
                      key={c.sessionId}
                      className="rounded-[10px] border p-3 flex flex-col gap-2"
                      style={{
                        borderColor: isWinner ? 'var(--accent-green)' : 'var(--border-line)',
                        backgroundColor: 'var(--bg-app)',
                      }}
                    >
                      <div className="flex items-center justify-between gap-2">
                        <div className="flex items-center gap-1.5 min-w-0">
                          <HarnessIcon harness={(c.harness as HarnessId) || 'claude-code'} size={14} />
                          <span className="font-semibold text-[12.5px] truncate" style={{ color: 'var(--text-main)' }}>
                            {c.harness}
                          </span>
                          <span className="text-[10.5px] font-mono-tech truncate" style={{ color: 'var(--text-muted)' }}>
                            {c.model}
                          </span>
                        </div>
                        <span className="inline-flex items-center gap-1 text-[10.5px] font-mono-tech uppercase">
                          {c.status === 'running' && <CircleNotch size={12} className="animate-spin" style={{ color: 'var(--accent-blue)' }} />}
                          {c.status === 'done' && <CheckCircle size={12} style={{ color: 'var(--accent-green)' }} />}
                          {c.status === 'error' && <WarningCircle size={12} style={{ color: 'var(--accent-red)' }} />}
                          {c.status === 'merged' && <GitMerge size={12} style={{ color: 'var(--accent-green)' }} />}
                          <span>{c.status}</span>
                        </span>
                      </div>

                      <div className="flex flex-wrap items-center gap-2 text-[11px] font-mono-tech" style={{ color: 'var(--text-muted)' }}>
                        <span>{(c.durationMs / 1000).toFixed(1)}s</span>
                        <span>·</span>
                        <span>{c.filesChanged} files</span>
                        <span style={{ color: 'var(--accent-green)' }}>+{c.additions}</span>
                        <span style={{ color: 'var(--accent-red)' }}>-{c.deletions}</span>
                        {c.verifyOk !== undefined && c.verifyOk !== null && (
                          <span
                            className="px-1.5 py-0.5 rounded"
                            style={{
                              backgroundColor: c.verifyOk ? 'rgba(34,197,94,0.15)' : 'rgba(239,68,68,0.15)',
                              color: c.verifyOk ? 'var(--accent-green)' : 'var(--accent-red)',
                            }}
                          >
                            {c.verifyOk ? 'Verify PASS' : 'Verify FAIL'}
                          </span>
                        )}
                      </div>

                      {c.error && (
                        <div className="text-[11px] font-mono-tech p-2 rounded" style={{ backgroundColor: 'rgba(239,68,68,0.1)', color: 'var(--accent-red)' }}>
                          {c.error}
                        </div>
                      )}

                      {c.output && (
                        <pre
                          className="text-[11px] font-mono-tech p-2 rounded max-h-36 overflow-y-auto whitespace-pre-wrap"
                          style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-body)' }}
                        >
                          {c.output.slice(0, 1200)}
                        </pre>
                      )}

                      {c.status === 'done' && activeRace.status !== 'merged' && (
                        <div className="pt-1 flex justify-end">
                          <button
                            type="button"
                            disabled={mergingId !== null}
                            onClick={() => handleMerge(activeRace.id, c.sessionId)}
                            className="btn-ghost px-2.5 py-1 text-[11px] font-mono-tech inline-flex items-center gap-1 cursor-pointer"
                            style={{ color: 'var(--accent-green)' }}
                          >
                            <GitMerge size={12} />
                            <span>{mergingId === c.sessionId ? 'Merging...' : 'Merge Winner'}</span>
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
