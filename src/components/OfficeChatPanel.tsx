import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ArrowDown,
  ArrowRight,
  ArrowUp,
  Broadcast,
  CaretDown,
  ChatCircleText,
  Crown,
  GitBranch,
  PaperPlaneRight,
  Pulse,
  Robot,
  Stop,
  User,
  UserMinus,
  UserPlus,
  WarningCircle,
  Wrench,
} from '@phosphor-icons/react';
import { isTauriRuntime, tauriApi } from '../lib/tauriApi';
import type {
  ChatMessage,
  HarnessId,
  HarnessModelOption,
  Office,
  OfficeEvent,
  OfficeEventKind,
  OfficeStaff,
  ToolCall,
} from '../types';
import { HARNESS_META, HarnessIcon } from './HarnessBadge';

interface OfficeChatPanelProps {
  office: Office;
  selectedStaffId: string;
  onSelectStaff: (staffId: string) => void;
  harnessModels: Record<string, HarnessModelOption[]>;
  onRefreshOffices: () => Promise<void> | void;
  onToast: (msg: string) => void;
  activeTab: 'chat' | 'activity';
  onTabChange: (tab: 'chat' | 'activity') => void;
}

interface ParsedDelegationItem {
  action?: string;
  to?: string;
  staff?: string;
  role?: string;
  task?: string;
  summary?: string;
  raw?: string;
}

function parseSakanaOfficeBlock(blockContent: string): ParsedDelegationItem[] {
  const trimmed = blockContent.trim();
  if (!trimmed) return [];
  try {
    const parsed = JSON.parse(trimmed);
    if (Array.isArray(parsed)) {
      return parsed.map((item) =>
        typeof item === 'object' && item !== null
          ? (item as ParsedDelegationItem)
          : { task: String(item) },
      );
    }
    if (typeof parsed === 'object' && parsed !== null) {
      const obj = parsed as Record<string, unknown>;
      if (Array.isArray(obj.delegations)) {
        return obj.delegations as ParsedDelegationItem[];
      }
      if (Array.isArray(obj.tasks)) {
        return obj.tasks as ParsedDelegationItem[];
      }
      return [obj as ParsedDelegationItem];
    }
  } catch {
    // Fallback for non-JSON directive blocks
  }
  return [{ raw: trimmed }];
}

function OfficeMessageBody({
  text,
  staffList,
  onSelectStaff,
}: {
  text: string;
  staffList: OfficeStaff[];
  onSelectStaff: (staffId: string) => void;
}) {
  if (!text) return null;

  const segments = text.split(/(```sakana-office[\s\S]*?(?:```|$))/g);

  return (
    <div
      className="selectable-text space-y-2 text-[12.5px] leading-relaxed"
      style={{ color: 'var(--text-body)' }}
    >
      {segments.map((seg, idx) => {
        if (!seg) return null;
        if (seg.startsWith('```sakana-office')) {
          const isClosed = seg.length >= 17 && seg.endsWith('```');
          const inner = isClosed ? seg.slice(16, -3) : seg.slice(16);
          const items = parseSakanaOfficeBlock(inner);

          return (
            <div
              key={idx}
              className="rounded-[9px] border p-2.5 space-y-2"
              style={{
                backgroundColor: 'var(--bg-app)',
                borderColor: 'var(--accent-blue)',
              }}
            >
              <div className="flex items-center justify-between gap-2">
                <span
                  className="inline-flex items-center gap-1.5 text-[10.5px] font-semibold uppercase tracking-wider"
                  style={{ color: 'var(--accent-blue)' }}
                >
                  <Broadcast size={12} weight="bold" />
                  Office Delegation
                </span>
              </div>

              {items.map((item, itemIdx) => {
                if (item.raw) {
                  return (
                    <pre
                      key={itemIdx}
                      className="m-0 font-mono-tech text-[11px] whitespace-pre-wrap break-words"
                      style={{ color: 'var(--text-main)' }}
                    >
                      {item.raw}
                    </pre>
                  );
                }

                const targetLabel = item.to || item.staff || item.role || 'Staff';
                const matchedStaff = staffList.find(
                  (s) =>
                    s.name.toLowerCase() === targetLabel.toLowerCase() ||
                    s.id === targetLabel ||
                    s.role.toLowerCase() === targetLabel.toLowerCase(),
                );

                return (
                  <div
                    key={itemIdx}
                    className="rounded-[7px] border px-2.5 py-2 space-y-1"
                    style={{
                      backgroundColor: 'var(--bg-card)',
                      borderColor: 'var(--border-line)',
                    }}
                  >
                    <div className="flex items-center justify-between gap-2 flex-wrap">
                      <div className="flex items-center gap-1.5">
                        <ArrowRight size={11} style={{ color: 'var(--accent-blue)' }} />
                        {matchedStaff ? (
                          <button
                            type="button"
                            onClick={() => onSelectStaff(matchedStaff.id)}
                            className="text-[11.5px] font-semibold hover:underline cursor-pointer"
                            style={{ color: 'var(--accent-blue)' }}
                          >
                            {matchedStaff.name}
                          </button>
                        ) : (
                          <span
                            className="text-[11.5px] font-semibold"
                            style={{ color: 'var(--text-main)' }}
                          >
                            {targetLabel}
                          </span>
                        )}
                      </div>
                      {item.action && (
                        <span
                          className="px-1.5 py-0.5 rounded-[4px] font-mono-tech text-[10px]"
                          style={{
                            backgroundColor: 'var(--bg-elevated)',
                            color: 'var(--text-muted)',
                          }}
                        >
                          {item.action}
                        </span>
                      )}
                    </div>
                    {(item.task || item.summary) && (
                      <div className="text-[11.5px]" style={{ color: 'var(--text-body)' }}>
                        {item.task || item.summary}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          );
        }

        // Standard code blocks vs text paragraphs
        const codeParts = seg.split(/(```[\s\S]*?(?:```|$))/g);
        return (
          <div key={idx} className="space-y-2">
            {codeParts.map((part, cIdx) => {
              if (!part) return null;
              if (part.startsWith('```')) {
                const isClosed = part.length >= 6 && part.endsWith('```');
                const inner = isClosed ? part.slice(3, -3) : part.slice(3);
                const lines = inner.split('\n');
                const lang = lines[0]?.trim() || 'code';
                const code = lines.slice(1).join('\n');
                return (
                  <div
                    key={cIdx}
                    className="rounded-[7px] border overflow-hidden"
                    style={{
                      borderColor: 'var(--border-line)',
                      backgroundColor: 'var(--bg-elevated)',
                    }}
                  >
                    <div
                      className="px-2.5 py-1 border-b font-mono-tech text-[10px]"
                      style={{
                        borderColor: 'var(--border-line)',
                        color: 'var(--text-muted)',
                      }}
                    >
                      {lang}
                    </div>
                    <pre
                      className="p-2.5 overflow-x-auto font-mono-tech text-[11px] m-0"
                      style={{ color: 'var(--text-main)' }}
                    >
                      <code>{code}</code>
                    </pre>
                  </div>
                );
              }
              return (
                <div key={cIdx} className="whitespace-pre-wrap break-words">
                  {part.trim()}
                </div>
              );
            })}
          </div>
        );
      })}
    </div>
  );
}

function eventBadgeMeta(kind: OfficeEventKind) {
  switch (kind) {
    case 'delegation':
      return { color: 'var(--accent-blue)', bg: 'var(--accent-blue-bg)', Icon: Broadcast };
    case 'hire':
      return { color: 'var(--accent-green)', bg: 'var(--accent-green-bg)', Icon: UserPlus };
    case 'fire':
      return { color: 'var(--accent-red)', bg: 'var(--accent-red-bg)', Icon: UserMinus };
    case 'report':
      return { color: 'var(--accent-yellow)', bg: 'var(--accent-yellow-bg)', Icon: ArrowUp };
    case 'error':
      return { color: 'var(--accent-red)', bg: 'var(--accent-red-bg)', Icon: WarningCircle };
    case 'message':
      return { color: 'var(--text-main)', bg: 'var(--bg-elevated)', Icon: ChatCircleText };
    default:
      return { color: 'var(--text-muted)', bg: 'var(--bg-elevated)', Icon: Pulse };
  }
}

export function OfficeChatPanel({
  office,
  selectedStaffId,
  onSelectStaff,
  harnessModels,
  onRefreshOffices,
  onToast,
  activeTab,
  onTabChange,
}: OfficeChatPanelProps) {
  const [messagesByStaff, setMessagesByStaff] = useState<Record<string, ChatMessage[]>>({});
  const [loadingMessages, setLoadingMessages] = useState(false);
  const [prompt, setPrompt] = useState('');
  const [sending, setSending] = useState(false);
  const [reporting, setReporting] = useState(false);
  const [showJumpBottom, setShowJumpBottom] = useState(false);

  const feedRef = useRef<HTMLDivElement | null>(null);
  const nearBottomRef = useRef(true);

  const selectedStaff = useMemo(
    () =>
      office.staff.find((s) => s.id === selectedStaffId) ??
      office.staff.find((s) => s.isMaster) ??
      office.staff[0] ??
      null,
    [office.staff, selectedStaffId],
  );

  const activeMessages = useMemo(
    () => (selectedStaff ? messagesByStaff[selectedStaff.id] ?? [] : []),
    [messagesByStaff, selectedStaff],
  );

  // Load messages for selected staff
  const loadMessages = useCallback(async (staff: OfficeStaff) => {
    if (!isTauriRuntime()) return;
    setLoadingMessages(true);
    try {
      const msgs = await tauriApi.getOfficeStaffMessages(staff.id);
      if (Array.isArray(msgs)) {
        setMessagesByStaff((prev) => ({ ...prev, [staff.id]: msgs }));
      }
    } catch (err) {
      console.error('getOfficeStaffMessages failed:', err);
    } finally {
      setLoadingMessages(false);
    }
  }, []);

  useEffect(() => {
    if (selectedStaff) {
      void loadMessages(selectedStaff);
    }
  }, [selectedStaff?.id, loadMessages]);

  // Subscribe to turn streaming events filtered by office staff sessionIds
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    const sessionToStaffId = new Map<string, string>();
    for (const s of office.staff) {
      if (s.sessionId) sessionToStaffId.set(s.sessionId, s.id);
    }

    const upsertAssistant = (
      sessionId: string,
      messageId: string,
      updater: (m: ChatMessage) => ChatMessage,
    ) => {
      const staffId = sessionToStaffId.get(sessionId);
      if (!staffId) return;
      setMessagesByStaff((prev) => {
        const list = prev[staffId] ?? [];
        const idx = list.findIndex((m) => m.id === messageId);
        if (idx >= 0) {
          const next = list.slice();
          next[idx] = updater(next[idx]);
          return { ...prev, [staffId]: next };
        }
        const created = updater({
          id: messageId,
          role: 'assistant',
          timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
          text: '',
        });
        return { ...prev, [staffId]: [...list, created] };
      });
    };

    const setup = async () => {
      const u1 = await tauriApi.onTurnToken((p) => {
        if (disposed || !sessionToStaffId.has(p.sessionId)) return;
        upsertAssistant(p.sessionId, p.messageId, (m) => ({
          ...m,
          text: `${m.text}${p.token}`,
        }));
      });
      const u2 = await tauriApi.onTurnThinking((p) => {
        if (disposed || !sessionToStaffId.has(p.sessionId)) return;
        upsertAssistant(p.sessionId, p.messageId, (m) => ({
          ...m,
          thinking: p.thinking,
          thinkingDuration:
            p.durationMs !== undefined ? `${(p.durationMs / 1000).toFixed(1)}s` : m.thinkingDuration,
        }));
      });
      const u3 = await tauriApi.onTurnTool((p) => {
        if (disposed || !sessionToStaffId.has(p.sessionId)) return;
        upsertAssistant(p.sessionId, p.messageId, (m) => {
          const existing = m.tools ?? [];
          const tIdx = existing.findIndex((t) => t.id === p.tool.id);
          const nextTools: ToolCall[] =
            tIdx >= 0
              ? existing.map((t, i) => (i === tIdx ? p.tool : t))
              : [...existing, p.tool];
          return { ...m, tools: nextTools };
        });
      });
      const u4 = await tauriApi.onTurnDone((p) => {
        if (disposed || !sessionToStaffId.has(p.sessionId)) return;
        upsertAssistant(p.sessionId, p.messageId, (m) =>
          p.message
            ? {
                ...m,
                ...p.message,
                text: p.message.text || m.text,
                timestamp: p.message.timestamp || m.timestamp,
              }
            : m,
        );
        void onRefreshOffices();
      });
      const u5 = await tauriApi.onTurnError((p) => {
        if (disposed || !sessionToStaffId.has(p.sessionId)) return;
        upsertAssistant(p.sessionId, p.messageId, (m) => ({
          ...m,
          text: m.text ? `${m.text}\n\nError: ${p.error}` : `Error: ${p.error}`,
        }));
        void onRefreshOffices();
      });

      if (disposed) {
        u1();
        u2();
        u3();
        u4();
        u5();
      } else {
        unlisteners.push(u1, u2, u3, u4, u5);
      }
    };

    void setup();
    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
    };
  }, [office.staff, onRefreshOffices]);

  // Auto-scroll chat feed
  useEffect(() => {
    const el = feedRef.current;
    if (!el || activeTab !== 'chat') return;
    if (nearBottomRef.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [activeMessages, activeTab, selectedStaff?.id]);

  const handleFeedScroll = () => {
    const el = feedRef.current;
    if (!el) return;
    const dist = el.scrollHeight - el.scrollTop - el.clientHeight;
    nearBottomRef.current = dist < 80;
    setShowJumpBottom(dist >= 120);
  };

  const handleSend = async () => {
    if (!selectedStaff || !prompt.trim() || sending) return;
    const text = prompt.trim();
    setPrompt('');
    setSending(true);

    const optimisticUser: ChatMessage = {
      id: `u-${Date.now()}`,
      role: 'user',
      timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
      text,
    };
    setMessagesByStaff((prev) => ({
      ...prev,
      [selectedStaff.id]: [...(prev[selectedStaff.id] ?? []), optimisticUser],
    }));

    try {
      await tauriApi.sendOfficeMessage(office.id, selectedStaff.id, text);
      void onRefreshOffices();
    } catch (err) {
      onToast(`Failed to send message: ${String(err)}`);
    } finally {
      setSending(false);
    }
  };

  const handleReportToMaster = async () => {
    if (!selectedStaff || selectedStaff.isMaster || reporting) return;
    setReporting(true);
    try {
      await tauriApi.reportStaffToMaster(office.id, selectedStaff.id);
      onToast(`Reported ${selectedStaff.name}'s status to Master`);
      void onRefreshOffices();
    } catch (err) {
      onToast(`Report failed: ${String(err)}`);
    } finally {
      setReporting(false);
    }
  };

  const handleStopRun = async () => {
    try {
      await tauriApi.stopOfficeRun(office.id);
      onToast('Stopped office run');
      void onRefreshOffices();
    } catch (err) {
      onToast(`Failed to stop run: ${String(err)}`);
    }
  };

  const handleQuickHarnessChange = async (newHarness: HarnessId) => {
    if (!selectedStaff) return;
    const models =
      harnessModels[newHarness]?.map((m) => m.id) ?? HARNESS_META[newHarness]?.models ?? [];
    const nextModel = models[0] || selectedStaff.model;
    try {
      await tauriApi.updateOfficeStaff({
        id: selectedStaff.id,
        harness: newHarness,
        model: nextModel,
      });
      void onRefreshOffices();
    } catch (err) {
      onToast(`Failed to update harness: ${String(err)}`);
    }
  };

  const handleQuickModelChange = async (newModel: string) => {
    if (!selectedStaff) return;
    try {
      await tauriApi.updateOfficeStaff({
        id: selectedStaff.id,
        model: newModel,
      });
      void onRefreshOffices();
    } catch (err) {
      onToast(`Failed to update model: ${String(err)}`);
    }
  };

  const availableModels = useMemo(() => {
    if (!selectedStaff) return [];
    const fromBackend = harnessModels[selectedStaff.harness]?.map((m) => m.id) ?? [];
    const fallback = HARNESS_META[selectedStaff.harness]?.models ?? [];
    const combined = fromBackend.length > 0 ? fromBackend : fallback;
    return combined.includes(selectedStaff.model)
      ? combined
      : [selectedStaff.model, ...combined].filter(Boolean);
  }, [selectedStaff, harnessModels]);

  const sortedEvents = useMemo(
    () =>
      office.events
        .slice()
        .sort((a, b) => a.timestamp.localeCompare(b.timestamp)),
    [office.events],
  );

  return (
    <aside
      className="flex flex-col h-full min-w-0 border-l"
      style={{
        backgroundColor: 'var(--bg-card)',
        borderColor: 'var(--border-line)',
      }}
    >
      {/* Top Tabs + Target Switcher */}
      <div
        className="px-3 pt-2.5 pb-2 border-b space-y-2 shrink-0"
        style={{ borderColor: 'var(--border-line)' }}
      >
        <div className="flex items-center justify-between gap-2">
          <div
            className="inline-flex rounded-[8px] p-0.5 border"
            style={{
              backgroundColor: 'var(--bg-app)',
              borderColor: 'var(--border-line)',
            }}
          >
            <button
              type="button"
              onClick={() => onTabChange('chat')}
              className="px-2.5 py-1 rounded-[6px] text-[11.5px] font-medium inline-flex items-center gap-1.5 cursor-pointer transition-colors"
              style={{
                backgroundColor: activeTab === 'chat' ? 'var(--bg-card)' : 'transparent',
                color: activeTab === 'chat' ? 'var(--text-main)' : 'var(--text-muted)',
                boxShadow: activeTab === 'chat' ? 'var(--surface-shadow)' : 'none',
              }}
            >
              <ChatCircleText size={13} />
              <span>Direct Chat</span>
            </button>
            <button
              type="button"
              onClick={() => onTabChange('activity')}
              className="px-2.5 py-1 rounded-[6px] text-[11.5px] font-medium inline-flex items-center gap-1.5 cursor-pointer transition-colors"
              style={{
                backgroundColor: activeTab === 'activity' ? 'var(--bg-card)' : 'transparent',
                color: activeTab === 'activity' ? 'var(--text-main)' : 'var(--text-muted)',
                boxShadow: activeTab === 'activity' ? 'var(--surface-shadow)' : 'none',
              }}
            >
              <Pulse size={13} />
              <span>Activity Feed</span>
              {office.events.length > 0 && (
                <span
                  className="px-1.5 py-[1px] rounded-full font-mono-tech text-[10px]"
                  style={{
                    backgroundColor: 'var(--bg-elevated)',
                    color: 'var(--text-muted)',
                  }}
                >
                  {office.events.length}
                </span>
              )}
            </button>
          </div>

          {office.status === 'RUNNING' && (
            <button
              type="button"
              onClick={handleStopRun}
              className="px-2.5 py-1 rounded-[7px] text-[11px] font-medium inline-flex items-center gap-1 cursor-pointer border"
              style={{
                backgroundColor: 'var(--accent-red-bg)',
                borderColor: 'var(--accent-red)',
                color: 'var(--accent-red)',
              }}
            >
              <Stop size={11} weight="fill" />
              <span>Stop Run</span>
            </button>
          )}
        </div>

        {/* Target Switcher Pills */}
        {activeTab === 'chat' && (
          <div className="flex items-center gap-1.5 overflow-x-auto no-scrollbar pb-0.5">
            {office.staff
              .slice()
              .sort((a, b) => (a.isMaster === b.isMaster ? a.deskOrder - b.deskOrder : a.isMaster ? -1 : 1))
              .map((staff) => {
                const active = selectedStaff?.id === staff.id;
                return (
                  <button
                    key={staff.id}
                    type="button"
                    onClick={() => onSelectStaff(staff.id)}
                    className="px-2.5 py-1 rounded-full text-[11px] font-medium inline-flex items-center gap-1.5 shrink-0 border cursor-pointer transition-colors"
                    style={{
                      backgroundColor: active ? 'var(--accent-blue-bg)' : 'var(--bg-app)',
                      borderColor: active ? 'var(--accent-blue)' : 'var(--border-line)',
                      color: active ? 'var(--accent-blue)' : 'var(--text-body)',
                    }}
                  >
                    {staff.isMaster && <Crown size={11} weight="fill" />}
                    <span>{staff.name}</span>
                    {staff.status === 'WORKING' && (
                      <span
                        className="w-1.5 h-1.5 rounded-full pulse-dot"
                        style={{ backgroundColor: 'var(--accent-blue)' }}
                      />
                    )}
                  </button>
                );
              })}
          </div>
        )}
      </div>

      {/* Main Panel Body */}
      {activeTab === 'chat' ? (
        selectedStaff ? (
          <>
            {/* Selected Desk Header + Quick Config + Report to Master */}
            <div
              className="px-3.5 py-2 border-b flex items-center justify-between gap-2 flex-wrap shrink-0"
              style={{
                backgroundColor: 'var(--bg-app)',
                borderColor: 'var(--border-line)',
              }}
            >
              <div className="flex items-center gap-2 min-w-0">
                <div className="relative inline-flex items-center">
                  <select
                    aria-label="Select desk harness"
                    value={selectedStaff.harness}
                    onChange={(e) => void handleQuickHarnessChange(e.target.value as HarnessId)}
                    className="appearance-none rounded-[6px] border pl-2 pr-6 py-1 text-[11px] font-medium cursor-pointer"
                    style={{
                      backgroundColor: 'var(--bg-card)',
                      borderColor: 'var(--border-line)',
                      color: 'var(--text-main)',
                    }}
                  >
                    {(Object.keys(HARNESS_META) as HarnessId[]).map((h) => (
                      <option key={h} value={h}>
                        {HARNESS_META[h].short}
                      </option>
                    ))}
                  </select>
                  <CaretDown
                    size={10}
                    className="pointer-events-none absolute right-1.5"
                    style={{ color: 'var(--text-muted)' }}
                  />
                </div>

                <div className="relative inline-flex items-center min-w-0">
                  <select
                    aria-label="Select desk model"
                    value={selectedStaff.model}
                    onChange={(e) => void handleQuickModelChange(e.target.value)}
                    className="appearance-none rounded-[6px] border pl-2 pr-6 py-1 font-mono-tech text-[11px] max-w-[165px] truncate cursor-pointer"
                    style={{
                      backgroundColor: 'var(--bg-card)',
                      borderColor: 'var(--border-line)',
                      color: 'var(--text-body)',
                    }}
                  >
                    {availableModels.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                  <CaretDown
                    size={10}
                    className="pointer-events-none absolute right-1.5"
                    style={{ color: 'var(--text-muted)' }}
                  />
                </div>
              </div>

              {!selectedStaff.isMaster && (
                <button
                  type="button"
                  disabled={reporting}
                  onClick={() => void handleReportToMaster()}
                  className="btn-ghost px-2.5 py-1 text-[11px] font-medium inline-flex items-center gap-1 cursor-pointer disabled:opacity-50"
                >
                  <ArrowUp size={11} />
                  <span>{reporting ? 'Reporting...' : 'Report to Master'}</span>
                </button>
              )}
            </div>

            {/* Chat Feed */}
            <div className="relative flex-1 min-h-0">
              <div
                ref={feedRef}
                onScroll={handleFeedScroll}
                className="h-full overflow-y-auto px-3.5 py-3 space-y-3"
              >
                {loadingMessages && activeMessages.length === 0 ? (
                  <div
                    className="py-10 text-center text-[12px]"
                    style={{ color: 'var(--text-muted)' }}
                  >
                    Loading conversation...
                  </div>
                ) : activeMessages.length === 0 ? (
                  <div className="py-10 text-center space-y-2">
                    <div
                      className="w-9 h-9 rounded-full mx-auto flex items-center justify-center border"
                      style={{
                        backgroundColor: 'var(--bg-app)',
                        borderColor: 'var(--border-line)',
                        color: 'var(--text-muted)',
                      }}
                    >
                      <HarnessIcon harness={selectedStaff.harness} size={16} />
                    </div>
                    <div className="text-[12.5px] font-medium" style={{ color: 'var(--text-main)' }}>
                      Direct channel with {selectedStaff.name}
                    </div>
                    <div
                      className="text-[11.5px] max-w-xs mx-auto"
                      style={{ color: 'var(--text-muted)' }}
                    >
                      {selectedStaff.isMaster
                        ? 'Ask the Master Orchestrator to plan, delegate tasks, or hire dynamic staff.'
                        : `Send instructions directly to ${selectedStaff.name} (${selectedStaff.role}).`}
                    </div>
                  </div>
                ) : (
                  activeMessages.map((msg) => (
                    <div
                      key={msg.id}
                      className="msg-enter rounded-[10px] p-3 border"
                      style={{
                        backgroundColor:
                          msg.role === 'user' ? 'var(--bg-app)' : 'var(--bg-elevated)',
                        borderColor: 'var(--border-line)',
                      }}
                    >
                      <div className="flex items-center justify-between gap-2 mb-1.5">
                        <div className="flex items-center gap-1.5">
                          {msg.role === 'user' ? (
                            <User size={12} style={{ color: 'var(--text-muted)' }} />
                          ) : (
                            <Robot size={12} style={{ color: 'var(--accent-blue)' }} />
                          )}
                          <span
                            className="text-[11.5px] font-semibold"
                            style={{ color: 'var(--text-main)' }}
                          >
                            {msg.role === 'user' ? msg.author || 'You' : selectedStaff.name}
                          </span>
                        </div>
                        <span
                          className="font-mono-tech text-[10px]"
                          style={{ color: 'var(--text-muted)' }}
                        >
                          {msg.timestamp}
                        </span>
                      </div>

                      {msg.thinking && (
                        <details className="mb-2">
                          <summary
                            className="text-[11px] cursor-pointer select-none"
                            style={{ color: 'var(--text-muted)' }}
                          >
                            Thinking {msg.thinkingDuration ? `(${msg.thinkingDuration})` : ''}
                          </summary>
                          <div
                            className="mt-1 pl-2.5 border-l text-[11px] whitespace-pre-wrap"
                            style={{
                              borderColor: 'var(--border-strong)',
                              color: 'var(--text-muted)',
                            }}
                          >
                            {msg.thinking}
                          </div>
                        </details>
                      )}

                      {msg.tools && msg.tools.length > 0 && (
                        <div className="mb-2 flex flex-wrap gap-1.5">
                          {msg.tools.map((tool) => (
                            <span
                              key={tool.id}
                              className="inline-flex items-center gap-1 px-2 py-0.5 rounded-[5px] font-mono-tech text-[10px] border"
                              style={{
                                backgroundColor: 'var(--bg-elevated)',
                                borderColor: 'var(--border-line)',
                                color:
                                  tool.status === 'error'
                                    ? 'var(--accent-red)'
                                    : tool.status === 'running'
                                      ? 'var(--accent-blue)'
                                      : 'var(--text-body)',
                              }}
                              title={tool.target}
                            >
                              <Wrench size={10} />
                              <span>{tool.name}</span>
                            </span>
                          ))}
                        </div>
                      )}

                      <OfficeMessageBody
                        text={msg.text}
                        staffList={office.staff}
                        onSelectStaff={onSelectStaff}
                      />
                    </div>
                  ))
                )}
                {selectedStaff.status === 'WORKING' && (
                  <div
                    className="msg-enter rounded-[10px] px-3 py-2 border flex items-center gap-2 text-[11.5px] font-mono-tech"
                    style={{ backgroundColor: 'var(--bg-elevated)', borderColor: 'var(--border-line)', color: 'var(--text-muted)' }}
                  >
                    <span className="thinking-dots" aria-hidden="true">
                      <span /><span /><span />
                    </span>
                    <span className="thinking-shimmer" style={{ color: 'var(--text-body)' }}>
                      {selectedStaff.name} is thinking…
                    </span>
                  </div>
                )}
              </div>

              {showJumpBottom && (
                <button
                  type="button"
                  onClick={() => {
                    if (feedRef.current) {
                      feedRef.current.scrollTop = feedRef.current.scrollHeight;
                    }
                  }}
                  aria-label="Jump to bottom"
                  className="sakana-popover pop-enter absolute bottom-3 right-3 w-7 h-7 rounded-full flex items-center justify-center cursor-pointer"
                  style={{ color: 'var(--text-main)' }}
                >
                  <ArrowDown size={13} />
                </button>
              )}
            </div>

            {/* Composer */}
            <div
              className="p-3 border-t shrink-0 space-y-2"
              style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-card)' }}
            >
              <div className="flex items-center justify-between text-[10.5px]" style={{ color: 'var(--text-muted)' }}>
                <span>
                  Tip: Use <code className="font-mono-tech">@master</code> or ask Master to delegate to staff.
                </span>
                {selectedStaff.isolatedWorktree && (
                  <span className="inline-flex items-center gap-1 font-mono-tech">
                    <GitBranch size={10} />
                    {selectedStaff.branch}
                  </span>
                )}
              </div>

              <div
                className="rounded-[10px] border p-2 flex flex-col gap-2"
                style={{
                  backgroundColor: 'var(--bg-app)',
                  borderColor: 'var(--border-strong)',
                }}
              >
                <textarea
                  rows={2}
                  value={prompt}
                  onChange={(e) => setPrompt(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && !e.shiftKey) {
                      e.preventDefault();
                      void handleSend();
                    }
                  }}
                  placeholder={`Message ${selectedStaff.name}...`}
                  className="w-full resize-none bg-transparent border-0 outline-none text-[12.5px] leading-relaxed"
                  style={{ color: 'var(--text-main)' }}
                />
                <div className="flex items-center justify-between gap-2">
                  <span className="text-[10.5px]" style={{ color: 'var(--text-muted)' }}>
                    Enter to send · Shift+Enter for newline
                  </span>
                  <div className="flex items-center gap-1.5">
                    {office.status === 'RUNNING' && (
                      <button
                        type="button"
                        onClick={handleStopRun}
                        className="btn-ghost px-2.5 py-1 text-[11px] inline-flex items-center gap-1 cursor-pointer"
                        style={{ color: 'var(--accent-red)' }}
                      >
                        <Stop size={11} weight="fill" />
                        <span>Stop</span>
                      </button>
                    )}
                    <button
                      type="button"
                      disabled={!prompt.trim() || sending}
                      onClick={() => void handleSend()}
                      className="btn-solid px-3 py-1 text-[11.5px] font-medium inline-flex items-center gap-1.5 cursor-pointer disabled:opacity-40"
                    >
                      <PaperPlaneRight size={12} weight="fill" />
                      <span>{sending ? 'Sending...' : 'Send'}</span>
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </>
        ) : (
          <div
            className="flex-1 flex items-center justify-center p-6 text-[12px]"
            style={{ color: 'var(--text-muted)' }}
          >
            Select a desk on the canvas to start chatting.
          </div>
        )
      ) : (
        /* Activity Feed Tab */
        <div className="flex-1 overflow-y-auto p-3.5 space-y-2.5">
          {sortedEvents.length === 0 ? (
            <div
              className="py-12 text-center text-[12px]"
              style={{ color: 'var(--text-muted)' }}
            >
              No office activity recorded yet.
            </div>
          ) : (
            sortedEvents.map((ev: OfficeEvent) => {
              const { color, bg, Icon } = eventBadgeMeta(ev.kind);
              const fromStaff = ev.fromStaffId
                ? office.staff.find((s) => s.id === ev.fromStaffId)
                : undefined;
              const toStaff = ev.toStaffId
                ? office.staff.find((s) => s.id === ev.toStaffId)
                : undefined;

              return (
                <div
                  key={ev.id}
                  className="rounded-[9px] border p-2.5 space-y-1.5"
                  style={{
                    backgroundColor: 'var(--bg-app)',
                    borderColor: 'var(--border-line)',
                  }}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span
                      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-[5px] text-[10px] font-medium uppercase tracking-wide"
                      style={{ backgroundColor: bg, color }}
                    >
                      <Icon size={10} weight="bold" />
                      {ev.kind}
                    </span>
                    <span
                      className="font-mono-tech text-[10px]"
                      style={{ color: 'var(--text-muted)' }}
                    >
                      {ev.timestamp}
                    </span>
                  </div>

                  {(ev.fromName || ev.toName) && (
                    <div className="flex items-center gap-1.5 flex-wrap text-[11px]">
                      {ev.fromName && (
                        <button
                          type="button"
                          disabled={!fromStaff}
                          onClick={() => {
                            if (fromStaff) {
                              onSelectStaff(fromStaff.id);
                              onTabChange('chat');
                            }
                          }}
                          className="px-1.5 py-0.5 rounded-[5px] font-medium border cursor-pointer disabled:cursor-default"
                          style={{
                            backgroundColor: 'var(--bg-card)',
                            borderColor: 'var(--border-line)',
                            color: fromStaff ? 'var(--accent-blue)' : 'var(--text-main)',
                          }}
                        >
                          {ev.fromName}
                        </button>
                      )}
                      {ev.fromName && ev.toName && (
                        <ArrowRight size={10} style={{ color: 'var(--text-muted)' }} />
                      )}
                      {ev.toName && (
                        <button
                          type="button"
                          disabled={!toStaff}
                          onClick={() => {
                            if (toStaff) {
                              onSelectStaff(toStaff.id);
                              onTabChange('chat');
                            }
                          }}
                          className="px-1.5 py-0.5 rounded-[5px] font-medium border cursor-pointer disabled:cursor-default"
                          style={{
                            backgroundColor: 'var(--bg-card)',
                            borderColor: 'var(--border-line)',
                            color: toStaff ? 'var(--accent-blue)' : 'var(--text-main)',
                          }}
                        >
                          {ev.toName}
                        </button>
                      )}
                    </div>
                  )}

                  <div className="text-[12px]" style={{ color: 'var(--text-main)' }}>
                    {ev.summary}
                  </div>

                  {ev.detail && (
                    <div
                      className="text-[11px] whitespace-pre-wrap break-words"
                      style={{ color: 'var(--text-muted)' }}
                    >
                      {ev.detail}
                    </div>
                  )}
                </div>
              );
            })
          )}
        </div>
      )}
    </aside>
  );
}
