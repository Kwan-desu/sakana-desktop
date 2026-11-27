import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import {
  ChatCircleText,
  Crown,
  GitBranch,
  PencilSimple,
  Plus,
  Sparkle,
  Trash,
  WarningCircle,
} from '@phosphor-icons/react';
import type { Office, OfficeDelegationEdge, OfficeStaff, OfficeStaffStatus } from '../types';
import { HARNESS_META, HarnessIcon } from './HarnessBadge';

interface OfficeCanvasProps {
  office: Office;
  delegations: OfficeDelegationEdge[];
  selectedStaffId: string;
  onSelectStaff: (staffId: string) => void;
  onMessageStaff: (staffId: string) => void;
  onEditStaff: (staff: OfficeStaff) => void;
  onFireStaff: (staff: OfficeStaff) => void;
  onHireStaff: () => void;
}

interface EdgePath {
  key: string;
  d: string;
  labelX: number;
  labelY: number;
  task: string;
}

function getInitials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return 'S';
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[1][0]).toUpperCase();
}

function StatusBadge({ status }: { status: OfficeStaffStatus }) {
  const config: Record<
    OfficeStaffStatus,
    { label: string; color: string; bg: string; pulse?: boolean }
  > = {
    IDLE: {
      label: 'Idle',
      color: 'var(--text-muted)',
      bg: 'var(--bg-elevated)',
    },
    WORKING: {
      label: 'Working',
      color: 'var(--accent-blue)',
      bg: 'var(--accent-blue-bg)',
      pulse: true,
    },
    DONE: {
      label: 'Done',
      color: 'var(--accent-green)',
      bg: 'var(--accent-green-bg)',
    },
    ERROR: {
      label: 'Error',
      color: 'var(--accent-red)',
      bg: 'var(--accent-red-bg)',
    },
  };
  const cfg = config[status] || config.IDLE;

  return (
    <span
      className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[10.5px] font-medium shrink-0"
      style={{ backgroundColor: cfg.bg, color: cfg.color }}
    >
      <span
        className={`w-1.5 h-1.5 rounded-full ${cfg.pulse ? 'pulse-dot' : ''}`}
        style={{ backgroundColor: cfg.color }}
      />
      {cfg.label}
    </span>
  );
}

export function OfficeCanvas({
  office,
  delegations,
  selectedStaffId,
  onSelectStaff,
  onMessageStaff,
  onEditStaff,
  onFireStaff,
  onHireStaff,
}: OfficeCanvasProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const deskRefs = useRef<Record<string, HTMLDivElement | null>>({});
  const [edgePaths, setEdgePaths] = useState<EdgePath[]>([]);

  const master = useMemo(
    () =>
      office.staff.find((s) => s.id === office.masterStaffId || s.isMaster) ??
      office.staff[0] ??
      null,
    [office.staff, office.masterStaffId],
  );

  const staffList = useMemo(
    () =>
      office.staff
        .filter((s) => !s.isMaster && s.id !== master?.id)
        .slice()
        .sort((a, b) => a.deskOrder - b.deskOrder),
    [office.staff, master?.id],
  );

  const activeEdges = useMemo(
    () => delegations.filter((d) => d.officeId === office.id),
    [delegations, office.id],
  );

  const activeTargets = useMemo(
    () => new Set(activeEdges.map((e) => e.toStaffId)),
    [activeEdges],
  );

  const setDeskRef = useCallback((id: string) => {
    return (el: HTMLDivElement | null) => {
      deskRefs.current[id] = el;
    };
  }, []);

  const recomputePaths = useCallback(() => {
    const container = containerRef.current;
    if (!container || activeEdges.length === 0) {
      setEdgePaths([]);
      return;
    }

    const cRect = container.getBoundingClientRect();
    const next: EdgePath[] = [];

    for (const edge of activeEdges) {
      const fromEl = deskRefs.current[edge.fromStaffId];
      const toEl = deskRefs.current[edge.toStaffId];
      if (!fromEl || !toEl) continue;

      const fRect = fromEl.getBoundingClientRect();
      const tRect = toEl.getBoundingClientRect();

      const x1 = fRect.left - cRect.left + fRect.width / 2;
      const y1 = fRect.bottom - cRect.top;
      const x2 = tRect.left - cRect.left + tRect.width / 2;
      const y2 = tRect.top - cRect.top;

      const dy = Math.max(40, Math.abs(y2 - y1) * 0.55);
      const d = `M ${x1.toFixed(1)} ${y1.toFixed(1)} C ${x1.toFixed(1)} ${(y1 + dy).toFixed(1)}, ${x2.toFixed(1)} ${(y2 - dy).toFixed(1)}, ${x2.toFixed(1)} ${y2.toFixed(1)}`;

      next.push({
        key: `${edge.fromStaffId}->${edge.toStaffId}`,
        d,
        labelX: (x1 + x2) / 2,
        labelY: (y1 + y2) / 2,
        task: edge.task,
      });
    }

    setEdgePaths(next);
  }, [activeEdges]);

  useLayoutEffect(() => {
    recomputePaths();
  }, [recomputePaths, office.staff]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const ro = new ResizeObserver(() => {
      recomputePaths();
    });
    ro.observe(container);
    for (const el of Object.values(deskRefs.current)) {
      if (el) ro.observe(el);
    }

    window.addEventListener('resize', recomputePaths);
    return () => {
      ro.disconnect();
      window.removeEventListener('resize', recomputePaths);
    };
  }, [recomputePaths, office.staff.length]);

  return (
    <div
      className="relative flex-1 min-w-0 h-full overflow-auto"
      style={{
        backgroundColor: 'var(--bg-canvas)',
        backgroundImage:
          'radial-gradient(circle, var(--border-line) 1px, transparent 1px)',
        backgroundSize: '20px 20px',
      }}
    >
      <style>{`
        @keyframes officeDashFlow {
          to {
            stroke-dashoffset: -24;
          }
        }
        .office-delegation-line {
          stroke-dasharray: 6 6;
          animation: officeDashFlow 0.9s linear infinite;
        }
      `}</style>

      <div
        ref={containerRef}
        className="relative min-h-full w-full max-w-5xl mx-auto px-6 py-7 flex flex-col items-center gap-12"
      >
        {/* SVG Delegation Overlay */}
        {edgePaths.length > 0 && (
          <svg
            className="pointer-events-none absolute inset-0 w-full h-full overflow-visible z-10"
            aria-hidden="true"
          >
            <defs>
              <marker
                id="office-arrow"
                viewBox="0 0 10 10"
                refX="8"
                refY="5"
                markerWidth="6"
                markerHeight="6"
                orient="auto-start-reverse"
              >
                <path d="M 0 1.5 L 9 5 L 0 8.5 z" fill="var(--accent-blue)" />
              </marker>
            </defs>
            {edgePaths.map((edge) => (
              <g key={edge.key}>
                <path
                  d={edge.d}
                  fill="none"
                  stroke="var(--accent-blue)"
                  strokeOpacity={0.22}
                  strokeWidth={5}
                  strokeLinecap="round"
                />
                <path
                  d={edge.d}
                  fill="none"
                  stroke="var(--accent-blue)"
                  strokeWidth={2}
                  strokeLinecap="round"
                  markerEnd="url(#office-arrow)"
                  className="office-delegation-line"
                />
              </g>
            ))}
          </svg>
        )}

        {/* Executive Desk (Master Orchestrator) */}
        {master && (
          <div className="relative z-20 w-full flex justify-center">
            <div
              ref={setDeskRef(master.id)}
              role="button"
              tabIndex={0}
              onClick={() => onSelectStaff(master.id)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  onSelectStaff(master.id);
                }
              }}
              className="group relative w-full max-w-md rounded-[12px] p-4 transition-all cursor-pointer"
              style={{
                backgroundColor: 'var(--bg-card)',
                border: `1.5px solid ${
                  selectedStaffId === master.id || master.status === 'WORKING'
                    ? 'var(--accent-blue)'
                    : 'var(--border-strong)'
                }`,
                boxShadow:
                  selectedStaffId === master.id
                    ? '0 0 0 3px var(--accent-blue-bg), var(--surface-shadow)'
                    : 'var(--surface-shadow)',
              }}
            >
              <div className="flex items-start justify-between gap-3">
                <div className="flex items-center gap-3 min-w-0">
                  <div
                    className="w-10 h-10 rounded-[10px] flex items-center justify-center text-[13px] font-semibold shrink-0"
                    style={{
                      backgroundColor: master.avatarColor || 'var(--accent-blue)',
                      color: '#FFFFFF',
                    }}
                  >
                    {getInitials(master.name)}
                  </div>
                  <div className="min-w-0">
                    <div className="flex items-center gap-1.5 flex-wrap">
                      <span
                        className="text-[14px] font-semibold truncate"
                        style={{ color: 'var(--text-main)' }}
                      >
                        {master.name}
                      </span>
                      <span
                        className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-[5px] text-[10px] font-medium"
                        style={{
                          backgroundColor: 'var(--accent-blue-bg)',
                          color: 'var(--accent-blue)',
                        }}
                      >
                        <Crown size={10} weight="fill" />
                        Orchestrator
                      </span>
                    </div>
                    <div
                      className="text-[11.5px] truncate mt-0.5"
                      style={{ color: 'var(--text-muted)' }}
                    >
                      {master.role || 'Master Orchestrator'}
                    </div>
                  </div>
                </div>
                <StatusBadge status={master.status} />
              </div>

              <div className="mt-3 flex items-center justify-between gap-2 flex-wrap">
                <div className="flex items-center gap-1.5 min-w-0">
                  <span
                    className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-[6px] text-[11px] font-medium border"
                    style={{
                      backgroundColor: 'var(--bg-elevated)',
                      borderColor: 'var(--border-line)',
                      color: 'var(--text-main)',
                    }}
                  >
                    <HarnessIcon harness={master.harness} size={12} />
                    <span>{HARNESS_META[master.harness]?.short ?? master.harness}</span>
                  </span>
                  <span
                    className="px-2 py-0.5 rounded-[6px] font-mono-tech text-[10.5px] truncate max-w-[180px]"
                    style={{
                      backgroundColor: 'var(--bg-elevated)',
                      color: 'var(--text-body)',
                    }}
                    title={master.model}
                  >
                    {master.model}
                  </span>
                </div>

                <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 focus-within:opacity-100 transition-opacity">
                  <button
                    type="button"
                    aria-label={`Message ${master.name}`}
                    title="Message Master"
                    onClick={(e) => {
                      e.stopPropagation();
                      onMessageStaff(master.id);
                    }}
                    className="btn-ghost p-1.5 cursor-pointer"
                  >
                    <ChatCircleText size={13} />
                  </button>
                  <button
                    type="button"
                    aria-label={`Edit ${master.name}`}
                    title="Edit Master"
                    onClick={(e) => {
                      e.stopPropagation();
                      onEditStaff(master);
                    }}
                    className="btn-ghost p-1.5 cursor-pointer"
                  >
                    <PencilSimple size={13} />
                  </button>
                </div>
              </div>

              {master.currentTask && (
                <div
                  className="mt-2.5 px-2.5 py-1.5 rounded-[7px] text-[11.5px] truncate border"
                  style={{
                    backgroundColor: 'var(--bg-app)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-body)',
                  }}
                  title={master.currentTask}
                >
                  {master.currentTask}
                </div>
              )}
            </div>
          </div>
        )}

        {/* Staff Workstation Zone */}
        <div className="relative z-20 w-full">
          <div className="flex items-center justify-between mb-3 px-0.5">
            <div
              className="text-[11px] font-semibold uppercase tracking-wider"
              style={{ color: 'var(--text-muted)' }}
            >
              Staff Workstations ({staffList.length})
            </div>
            {activeEdges.length > 0 && (
              <div
                className="text-[11px] font-medium flex items-center gap-1.5"
                style={{ color: 'var(--accent-blue)' }}
              >
                <span className="w-1.5 h-1.5 rounded-full pulse-dot" style={{ backgroundColor: 'var(--accent-blue)' }} />
                {activeEdges.length} active delegation{activeEdges.length > 1 ? 's' : ''}
              </div>
            )}
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
            {staffList.map((staff) => {
              const isSelected = selectedStaffId === staff.id;
              const isDelegated = activeTargets.has(staff.id);
              const isWorking = staff.status === 'WORKING';

              return (
                <div
                  key={staff.id}
                  ref={setDeskRef(staff.id)}
                  role="button"
                  tabIndex={0}
                  onClick={() => onSelectStaff(staff.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      onSelectStaff(staff.id);
                    }
                  }}
                  className="group relative rounded-[11px] p-3.5 transition-all cursor-pointer flex flex-col justify-between gap-3"
                  style={{
                    backgroundColor: 'var(--bg-card)',
                    border: `1px solid ${
                      isSelected
                        ? 'var(--accent-blue)'
                        : isDelegated || isWorking
                          ? 'var(--accent-blue)'
                          : 'var(--border-line)'
                    }`,
                    boxShadow: isSelected
                      ? '0 0 0 2px var(--accent-blue-bg), var(--surface-shadow)'
                      : 'var(--surface-shadow)',
                  }}
                >
                  <div>
                    <div className="flex items-start justify-between gap-2">
                      <div className="flex items-center gap-2.5 min-w-0">
                        <div
                          className="w-8 h-8 rounded-[8px] flex items-center justify-center text-[11.5px] font-semibold shrink-0"
                          style={{
                            backgroundColor: staff.avatarColor || 'var(--bg-elevated)',
                            color: staff.avatarColor ? '#FFFFFF' : 'var(--text-main)',
                          }}
                        >
                          {getInitials(staff.name)}
                        </div>
                        <div className="min-w-0">
                          <div className="flex items-center gap-1.5 flex-wrap">
                            <span
                              className="text-[13px] font-semibold truncate"
                              style={{ color: 'var(--text-main)' }}
                            >
                              {staff.name}
                            </span>
                            {staff.isDynamic && (
                              <span
                                className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-[5px] text-[9.5px] font-medium"
                                style={{
                                  backgroundColor: 'var(--accent-yellow-bg)',
                                  color: 'var(--accent-yellow)',
                                }}
                                title="Dynamically hired by Master"
                              >
                                <Sparkle size={9} weight="fill" />
                                Dynamic
                              </span>
                            )}
                          </div>
                          <div
                            className="text-[11px] truncate"
                            style={{ color: 'var(--text-muted)' }}
                            title={staff.role}
                          >
                            {staff.role}
                          </div>
                        </div>
                      </div>
                      <StatusBadge status={staff.status} />
                    </div>

                    {/* Harness, Model & Worktree */}
                    <div className="mt-2.5 flex items-center gap-1.5 flex-wrap">
                      <span
                        className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-[5px] text-[10.5px] font-medium border"
                        style={{
                          backgroundColor: 'var(--bg-elevated)',
                          borderColor: 'var(--border-line)',
                          color: 'var(--text-main)',
                        }}
                      >
                        <HarnessIcon harness={staff.harness} size={11} />
                        <span>{HARNESS_META[staff.harness]?.short ?? staff.harness}</span>
                      </span>
                      <span
                        className="px-1.5 py-0.5 rounded-[5px] font-mono-tech text-[10px] truncate max-w-[140px]"
                        style={{
                          backgroundColor: 'var(--bg-elevated)',
                          color: 'var(--text-body)',
                        }}
                        title={staff.model}
                      >
                        {staff.model}
                      </span>
                      {staff.isolatedWorktree && (
                        <span
                          className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-[5px] font-mono-tech text-[10px] truncate max-w-[150px]"
                          style={{
                            backgroundColor: 'var(--accent-green-bg)',
                            color: 'var(--accent-green)',
                          }}
                          title={staff.worktreePath || staff.branch}
                        >
                          <GitBranch size={10} />
                          Worktree: {staff.branch || 'isolated'}
                        </span>
                      )}
                    </div>

                    {/* Current Task Snippet */}
                    {staff.currentTask && (
                      <div
                        className="mt-2.5 px-2.5 py-1.5 rounded-[6px] text-[11px] line-clamp-2 border"
                        style={{
                          backgroundColor: 'var(--bg-app)',
                          borderColor: isWorking ? 'var(--accent-blue-bg)' : 'var(--border-line)',
                          color: 'var(--text-body)',
                        }}
                        title={staff.currentTask}
                      >
                        {staff.status === 'ERROR' && (
                          <WarningCircle
                            size={12}
                            className="inline mr-1 align-text-bottom"
                            style={{ color: 'var(--accent-red)' }}
                          />
                        )}
                        {staff.currentTask}
                      </div>
                    )}
                  </div>

                  {/* Hover Actions */}
                  <div
                    className="pt-2 border-t flex items-center justify-end gap-1 opacity-0 group-hover:opacity-100 focus-within:opacity-100 transition-opacity"
                    style={{ borderColor: 'var(--border-line)' }}
                  >
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        onMessageStaff(staff.id);
                      }}
                      className="btn-ghost px-2 py-1 text-[11px] inline-flex items-center gap-1 cursor-pointer"
                      aria-label={`Message ${staff.name}`}
                    >
                      <ChatCircleText size={12} />
                      <span>Message</span>
                    </button>
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        onEditStaff(staff);
                      }}
                      className="btn-ghost px-2 py-1 text-[11px] inline-flex items-center gap-1 cursor-pointer"
                      aria-label={`Edit ${staff.name}`}
                    >
                      <PencilSimple size={12} />
                      <span>Edit</span>
                    </button>
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        onFireStaff(staff);
                      }}
                      className="btn-ghost px-2 py-1 text-[11px] inline-flex items-center gap-1 cursor-pointer"
                      style={{ color: 'var(--accent-red)' }}
                      aria-label={`Fire ${staff.name}`}
                    >
                      <Trash size={12} />
                      <span>Fire</span>
                    </button>
                  </div>
                </div>
              );
            })}

            {/* + Hire Staff Ghost Desk Card */}
            <button
              type="button"
              onClick={onHireStaff}
              className="rounded-[11px] p-4 min-h-[144px] border border-dashed flex flex-col items-center justify-center gap-2 transition-colors cursor-pointer"
              style={{
                borderColor: 'var(--border-strong)',
                backgroundColor: 'transparent',
                color: 'var(--text-muted)',
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.backgroundColor = 'var(--bg-hover)';
                e.currentTarget.style.color = 'var(--text-main)';
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.backgroundColor = 'transparent';
                e.currentTarget.style.color = 'var(--text-muted)';
              }}
            >
              <div
                className="w-9 h-9 rounded-full flex items-center justify-center border"
                style={{
                  borderColor: 'var(--border-strong)',
                  backgroundColor: 'var(--bg-card)',
                }}
              >
                <Plus size={16} />
              </div>
              <span className="text-[12.5px] font-medium">Hire Staff</span>
              <span className="text-[11px] opacity-80">Add a specialized agent desk</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
