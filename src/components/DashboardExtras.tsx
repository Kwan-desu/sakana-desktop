import { useEffect, useState } from 'react';
import {
  ArrowsClockwise,
  ArrowsLeftRight,
  ChartBar,
  Microphone,
  Plus,
  ShieldCheck,
  Trash,
} from '@phosphor-icons/react';
import { tauriApi } from '../lib/tauriApi';
import type {
  AuditLogEntry,
  FailoverConfig,
  FailoverTarget,
  HarnessId,
  TelemetrySummary,
  VoiceConfig,
} from '../types';

const HARNESS_OPTIONS: HarnessId[] = ['claude-code', 'agy-cli', 'kiro-cli', 'sakana'];

export function TelemetryAnalyticsCard() {
  const [summary, setSummary] = useState<TelemetrySummary | null>(null);
  const [days, setDays] = useState(14);
  const [loading, setLoading] = useState(false);

  const load = async (d: number) => {
    setLoading(true);
    try {
      const res = await tauriApi.getTelemetrySummary(d);
      setSummary(res);
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load(days);
  }, [days]);

  const maxDayTokens = Math.max(1, ...(summary?.byDay.map((p) => p.totalTokens) ?? [1]));

  return (
    <div className="sakana-card view-enter rounded-[12px] p-4 space-y-3">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <ChartBar size={15} style={{ color: 'var(--accent-blue)' }} />
          <span className="text-[13px] font-semibold" style={{ color: 'var(--text-main)' }}>
            Execution Telemetry Analytics
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          {[7, 14, 30].map((d) => (
            <button
              key={d}
              type="button"
              onClick={() => setDays(d)}
              className="px-2 py-0.5 rounded text-[11px] font-mono-tech cursor-pointer border"
              style={{
                borderColor: days === d ? 'var(--accent-blue)' : 'var(--border-line)',
                backgroundColor: days === d ? 'var(--bg-elevated)' : 'transparent',
                color: 'var(--text-main)',
              }}
            >
              {d}d
            </button>
          ))}
          <button
            type="button"
            onClick={() => void load(days)}
            className="p-1 rounded hover:bg-[var(--bg-hover)] cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
            title="Refresh telemetry"
          >
            <ArrowsClockwise size={13} className={loading ? 'animate-spin' : ''} />
          </button>
        </div>
      </div>

      {summary ? (
        <>
          <div className="grid grid-cols-3 gap-2.5">
            <div className="rounded-[8px] border p-2.5" style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}>
              <div className="text-[10.5px] font-mono-tech" style={{ color: 'var(--text-muted)' }}>Total Spans</div>
              <div className="text-[16px] font-semibold font-mono-tech" style={{ color: 'var(--text-main)' }}>
                {summary.totalSpans.toLocaleString()}
              </div>
            </div>
            <div className="rounded-[8px] border p-2.5" style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}>
              <div className="text-[10.5px] font-mono-tech" style={{ color: 'var(--text-muted)' }}>Total Tokens</div>
              <div className="text-[16px] font-semibold font-mono-tech" style={{ color: 'var(--text-main)' }}>
                {summary.totalTokens.toLocaleString()}
              </div>
            </div>
            <div className="rounded-[8px] border p-2.5" style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}>
              <div className="text-[10.5px] font-mono-tech" style={{ color: 'var(--text-muted)' }}>Avg Latency</div>
              <div className="text-[16px] font-semibold font-mono-tech" style={{ color: 'var(--text-main)' }}>
                {(summary.avgDurationMs / 1000).toFixed(2)}s
              </div>
            </div>
          </div>

          {summary.byHarness.length > 0 && (
            <div className="space-y-1">
              <div className="text-[11px] font-mono-tech" style={{ color: 'var(--text-muted)' }}>By Harness</div>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-1.5">
                {summary.byHarness.map((b) => (
                  <div
                    key={b.key}
                    className="px-2.5 py-1.5 rounded-[7px] border flex items-center justify-between text-[11.5px] font-mono-tech"
                    style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
                  >
                    <span style={{ color: 'var(--text-main)' }}>{b.label}</span>
                    <span style={{ color: 'var(--text-muted)' }}>
                      {b.spans} spans · {b.totalTokens.toLocaleString()} tok
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {summary.byDay.length > 0 && (
            <div className="space-y-1">
              <div className="text-[11px] font-mono-tech" style={{ color: 'var(--text-muted)' }}>Daily Token Activity</div>
              <div className="flex items-end gap-1 h-14 pt-2">
                {summary.byDay.map((pt) => {
                  const pct = Math.max(8, Math.round((pt.totalTokens / maxDayTokens) * 100));
                  return (
                    <div
                      key={pt.day}
                      className="flex-1 rounded-t"
                      style={{
                        height: `${pct}%`,
                        backgroundColor: 'var(--accent-blue)',
                        opacity: 0.8,
                      }}
                      title={`${pt.day}: ${pt.spans} spans, ${pt.totalTokens.toLocaleString()} tokens`}
                    />
                  );
                })}
              </div>
            </div>
          )}
        </>
      ) : (
        <div className="text-[12px] py-2" style={{ color: 'var(--text-muted)' }}>
          No telemetry spans recorded yet.
        </div>
      )}
    </div>
  );
}

export function FailoverConfigCard({ onToast }: { onToast: (msg: string) => void }) {
  const [cfg, setCfg] = useState<FailoverConfig>({
    enabled: false,
    quotaThresholdPct: 90,
    rotateAccountsSameHarness: true,
    fallbackChain: [],
  });
  const [saving, setSaving] = useState(false);
  const [newHarness, setNewHarness] = useState<HarnessId>('agy-cli');
  const [newModel, setNewModel] = useState('');

  useEffect(() => {
    tauriApi.getFailoverConfig().then(setCfg).catch(() => {});
  }, []);

  const save = async (next: FailoverConfig) => {
    setSaving(true);
    try {
      const saved = await tauriApi.updateFailoverConfig(next);
      setCfg(saved);
      onToast('Auto-failover policy saved');
    } catch (e) {
      onToast(`Failed to save failover config: ${String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  const addTarget = () => {
    const target: FailoverTarget = { harness: newHarness, model: newModel.trim() };
    const next = { ...cfg, fallbackChain: [...cfg.fallbackChain, target] };
    setCfg(next);
    setNewModel('');
    void save(next);
  };

  const removeTarget = (idx: number) => {
    const next = { ...cfg, fallbackChain: cfg.fallbackChain.filter((_, i) => i !== idx) };
    setCfg(next);
    void save(next);
  };

  return (
    <div className="sakana-card view-enter rounded-[12px] p-4 space-y-3">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <ArrowsLeftRight size={15} style={{ color: 'var(--accent-yellow)' }} />
          <span className="text-[13px] font-semibold" style={{ color: 'var(--text-main)' }}>
            Auto-Failover & Smart Quota Routing
          </span>
        </div>
        <label className="flex items-center gap-2 text-[12px] cursor-pointer" style={{ color: 'var(--text-main)' }}>
          <input
            type="checkbox"
            checked={cfg.enabled}
            onChange={(e) => {
              const next = { ...cfg, enabled: e.target.checked };
              setCfg(next);
              void save(next);
            }}
          />
          <span>Enabled</span>
        </label>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-[12px]">
        <div className="flex items-center justify-between gap-2 rounded-[8px] border px-3 py-2" style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}>
          <span style={{ color: 'var(--text-body)' }}>Quota Threshold (%)</span>
          <input
            type="number"
            min={50}
            max={100}
            value={cfg.quotaThresholdPct}
            onChange={(e) => setCfg({ ...cfg, quotaThresholdPct: Number(e.target.value) || 90 })}
            onBlur={() => void save(cfg)}
            className="w-16 px-2 py-0.5 rounded border text-right font-mono-tech bg-transparent"
            style={{ borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
          />
        </div>
        <label className="flex items-center justify-between gap-2 rounded-[8px] border px-3 py-2 cursor-pointer" style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}>
          <span style={{ color: 'var(--text-body)' }}>Rotate accounts on same harness first</span>
          <input
            type="checkbox"
            checked={cfg.rotateAccountsSameHarness}
            onChange={(e) => {
              const next = { ...cfg, rotateAccountsSameHarness: e.target.checked };
              setCfg(next);
              void save(next);
            }}
          />
        </label>
      </div>

      <div className="space-y-1.5">
        <div className="text-[11px] font-mono-tech" style={{ color: 'var(--text-muted)' }}>
          Fallback Chain ({cfg.fallbackChain.length} hops)
        </div>
        {cfg.fallbackChain.map((t, i) => (
          <div
            key={`${t.harness}-${i}`}
            className="flex items-center justify-between px-2.5 py-1.5 rounded-[7px] border text-[12px] font-mono-tech"
            style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
          >
            <span style={{ color: 'var(--text-main)' }}>
              #{i + 1} {t.harness} {t.model ? `(${t.model})` : '(default model)'}
            </span>
            <button
              type="button"
              onClick={() => removeTarget(i)}
              className="p-1 rounded hover:bg-[var(--bg-hover)] cursor-pointer"
              style={{ color: 'var(--accent-red)' }}
            >
              <Trash size={12} />
            </button>
          </div>
        ))}
        <div className="flex items-center gap-2 pt-1">
          <select
            value={newHarness}
            onChange={(e) => setNewHarness(e.target.value as HarnessId)}
            className="px-2 py-1 rounded-[7px] border text-[12px] bg-transparent"
            style={{ borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
          >
            {HARNESS_OPTIONS.map((h) => (
              <option key={h} value={h} style={{ backgroundColor: 'var(--bg-elevated)' }}>
                {h}
              </option>
            ))}
          </select>
          <input
            type="text"
            value={newModel}
            onChange={(e) => setNewModel(e.target.value)}
            placeholder="Optional model override"
            className="flex-1 px-2.5 py-1 rounded-[7px] border text-[12px] bg-transparent"
            style={{ borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
          />
          <button
            type="button"
            disabled={saving}
            onClick={addTarget}
            className="btn-ghost px-2.5 py-1 text-[11.5px] inline-flex items-center gap-1 cursor-pointer"
          >
            <Plus size={12} />
            <span>Add Hop</span>
          </button>
        </div>
      </div>
    </div>
  );
}

export function VoiceSettingsCard({ onToast }: { onToast: (msg: string) => void }) {
  const [cfg, setCfg] = useState<VoiceConfig>({
    engine: 'auto',
    language: 'en-US',
    whisperUrl: '',
    whisperModel: 'whisper-1',
    whisperCliPath: '',
    autoSend: false,
    silenceMs: 1500,
    ttsEnabled: false,
    ttsVoice: '',
    ttsRate: 1.05,
  });
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    tauriApi.getVoiceConfig().then(setCfg).catch(() => {});
  }, []);

  const handleSave = async () => {
    setSaving(true);
    try {
      const saved = await tauriApi.updateVoiceConfig(cfg);
      setCfg(saved);
      onToast('Voice settings saved');
    } catch (e) {
      onToast(`Failed to save voice settings: ${String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="sakana-card view-enter rounded-[12px] p-5 space-y-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Microphone size={15} style={{ color: 'var(--accent-blue)' }} />
          <span className="text-[13px] font-semibold" style={{ color: 'var(--text-main)' }}>
            Voice Mode (STT + TTS)
          </span>
        </div>
        <button
          type="button"
          disabled={saving}
          onClick={() => void handleSave()}
          className="btn-solid px-3 py-1.5 text-[11.5px] font-medium cursor-pointer disabled:opacity-50"
        >
          {saving ? 'Saving...' : 'Save Voice Config'}
        </button>
      </div>
      <div className="grid grid-cols-1 md:grid-cols-3 gap-2.5 text-[12px]">
        <div>
          <label className="block text-[10.5px] font-mono-tech mb-1" style={{ color: 'var(--text-muted)' }}>STT Engine</label>
          <select
            value={cfg.engine}
            onChange={(e) => setCfg({ ...cfg, engine: e.target.value })}
            className="w-full px-2 py-1.5 rounded-[7px] border bg-transparent"
            style={{ borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
          >
            <option value="auto" style={{ backgroundColor: 'var(--bg-elevated)' }}>Auto (Browser → Whisper)</option>
            <option value="browser" style={{ backgroundColor: 'var(--bg-elevated)' }}>Browser Web Speech</option>
            <option value="whisper" style={{ backgroundColor: 'var(--bg-elevated)' }}>Whisper (HTTP / CLI)</option>
          </select>
        </div>
        <div>
          <label className="block text-[10.5px] font-mono-tech mb-1" style={{ color: 'var(--text-muted)' }}>Language</label>
          <input
            type="text"
            value={cfg.language}
            onChange={(e) => setCfg({ ...cfg, language: e.target.value })}
            className="w-full px-2 py-1.5 rounded-[7px] border bg-transparent"
            style={{ borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
          />
        </div>
        <div>
          <label className="block text-[10.5px] font-mono-tech mb-1" style={{ color: 'var(--text-muted)' }}>Whisper Endpoint URL</label>
          <input
            type="text"
            value={cfg.whisperUrl}
            onChange={(e) => setCfg({ ...cfg, whisperUrl: e.target.value })}
            placeholder="http://127.0.0.1:8080/inference"
            className="w-full px-2 py-1.5 rounded-[7px] border bg-transparent"
            style={{ borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
          />
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-4 text-[12px]">
        <label className="flex items-center gap-2 cursor-pointer" style={{ color: 'var(--text-body)' }}>
          <input
            type="checkbox"
            checked={cfg.ttsEnabled}
            onChange={(e) => setCfg({ ...cfg, ttsEnabled: e.target.checked })}
          />
          <span>Auto-speak assistant replies (TTS)</span>
        </label>
        <label className="flex items-center gap-2 cursor-pointer" style={{ color: 'var(--text-body)' }}>
          <input
            type="checkbox"
            checked={cfg.autoSend}
            onChange={(e) => setCfg({ ...cfg, autoSend: e.target.checked })}
          />
          <span>Auto-send after dictation</span>
        </label>
      </div>
    </div>
  );
}

export function AuditLogCard() {
  const [logs, setLogs] = useState<AuditLogEntry[]>([]);
  const [source, setSource] = useState('');
  const [status, setStatus] = useState('');
  const [loading, setLoading] = useState(false);

  const load = async () => {
    setLoading(true);
    try {
      const rows = await tauriApi.listAuditLogs(source || undefined, status || undefined, 50);
      setLogs(rows);
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, [source, status]);

  return (
    <div className="sakana-card view-enter rounded-[12px] p-5 space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <ShieldCheck size={15} style={{ color: 'var(--accent-green)' }} />
          <span className="text-[13px] font-semibold" style={{ color: 'var(--text-main)' }}>
            Security & Audit Log
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          <select
            value={source}
            onChange={(e) => setSource(e.target.value)}
            className="px-2 py-1 rounded-[7px] border text-[11px] bg-transparent"
            style={{ borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
          >
            <option value="" style={{ backgroundColor: 'var(--bg-elevated)' }}>All Sources</option>
            <option value="desktop" style={{ backgroundColor: 'var(--bg-elevated)' }}>desktop</option>
            <option value="remote" style={{ backgroundColor: 'var(--bg-elevated)' }}>remote</option>
            <option value="discord" style={{ backgroundColor: 'var(--bg-elevated)' }}>discord</option>
            <option value="automation" style={{ backgroundColor: 'var(--bg-elevated)' }}>automation</option>
          </select>
          <select
            value={status}
            onChange={(e) => setStatus(e.target.value)}
            className="px-2 py-1 rounded-[7px] border text-[11px] bg-transparent"
            style={{ borderColor: 'var(--border-line)', color: 'var(--text-main)' }}
          >
            <option value="" style={{ backgroundColor: 'var(--bg-elevated)' }}>All Status</option>
            <option value="allowed" style={{ backgroundColor: 'var(--bg-elevated)' }}>allowed</option>
            <option value="approved" style={{ backgroundColor: 'var(--bg-elevated)' }}>approved</option>
            <option value="denied" style={{ backgroundColor: 'var(--bg-elevated)' }}>denied</option>
            <option value="blocked" style={{ backgroundColor: 'var(--bg-elevated)' }}>blocked</option>
          </select>
          <button
            type="button"
            onClick={() => void load()}
            className="p-1 rounded hover:bg-[var(--bg-hover)] cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
            title="Refresh audit log"
          >
            <ArrowsClockwise size={13} className={loading ? 'animate-spin' : ''} />
          </button>
        </div>
      </div>

      {logs.length === 0 ? (
        <div className="text-[12px] py-2" style={{ color: 'var(--text-muted)' }}>
          No audit log entries match the current filter.
        </div>
      ) : (
        <div className="max-h-60 overflow-y-auto space-y-1">
          {logs.map((entry) => (
            <div
              key={entry.id}
              className="px-2.5 py-1.5 rounded-[7px] border flex items-center justify-between gap-2 text-[11.5px]"
              style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 font-mono-tech text-[10.5px]">
                  <span className="px-1.5 py-0.5 rounded" style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-main)' }}>
                    {entry.source}
                  </span>
                  <span style={{ color: 'var(--text-main)' }}>{entry.action}</span>
                  <span
                    style={{
                      color:
                        entry.status === 'blocked' || entry.status === 'denied'
                          ? 'var(--accent-red)'
                          : 'var(--accent-green)',
                    }}
                  >
                    [{entry.status}]
                  </span>
                </div>
                <div className="truncate mt-0.5" style={{ color: 'var(--text-body)' }}>
                  {entry.summary}
                </div>
              </div>
              <span className="font-mono-tech text-[10px] shrink-0" style={{ color: 'var(--text-muted)' }}>
                {entry.timestamp.slice(0, 19).replace('T', ' ')}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
