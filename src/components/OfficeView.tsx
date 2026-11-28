import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  BookmarkSimple,
  Buildings,
  CaretDown,
  FolderOpen,
  GitBranch,
  Plus,
  SidebarSimple,
  Sparkle,
  Stop,
  Trash,
  UserPlus,
  X,
} from '@phosphor-icons/react';
import { isTauriRuntime, tauriApi } from '../lib/tauriApi';
import type {
  HarnessId,
  HarnessModelOption,
  Office,
  OfficeBlueprint,
  OfficeDelegationEdge,
  OfficeStaff,
  PermissionMode,
  Project,
  ReasoningEffort,
  WslDistro,
} from '../types';
import { FolderPickerModal } from './FolderPickerModal';
import { HARNESS_META, HarnessIcon } from './HarnessBadge';
import { OfficeCanvas } from './OfficeCanvas';
import { OfficeChatPanel } from './OfficeChatPanel';

export interface OfficeViewProps {
  offices: Office[];
  selectedOffice: Office | null;
  projects: Project[];
  harnessModels: Record<string, HarnessModelOption[]>;
  delegations: OfficeDelegationEdge[];
  onSelectOffice: (id: string) => void;
  onRefreshOffices: () => Promise<void> | void;
  onToast: (msg: string) => void;
  createModalOpen?: boolean;
  onSetCreateModalOpen?: (open: boolean) => void;
  sidebarOpen?: boolean;
  onToggleSidebar?: () => void;
}

const AVATAR_COLORS = [
  '#2563EB',
  '#7C3AED',
  '#059669',
  '#D97706',
  '#DC2626',
  '#0891B2',
  '#DB2777',
  '#4F46E5',
];

const EFFORT_OPTIONS: { value: ReasoningEffort; label: string }[] = [
  { value: 'low', label: 'Low' },
  { value: 'medium', label: 'Medium' },
  { value: 'high', label: 'High' },
  { value: 'xhigh', label: 'XHigh' },
];


const VALID_EFFORTS: ReasoningEffort[] = ['low', 'medium', 'high', 'xhigh'];

function getModelsForHarness(
  harness: HarnessId,
  harnessModels: Record<string, HarnessModelOption[]>,
): string[] {
  const fromBackend = harnessModels[harness]?.map((m) => m.id) ?? [];
  return fromBackend.length > 0 ? fromBackend : HARNESS_META[harness]?.models ?? [];
}

function getEffortsForModel(
  harness: HarnessId,
  modelId: string,
  harnessModels: Record<string, HarnessModelOption[]>,
): ReasoningEffort[] {
  const list = harnessModels[harness];
  if (!list || list.length === 0) {
    return VALID_EFFORTS;
  }
  const entry = list.find((m) => m.id === modelId);
  if (!entry) return VALID_EFFORTS;
  return entry.efforts.filter((e): e is ReasoningEffort =>
    VALID_EFFORTS.includes(e as ReasoningEffort),
  );
}

function cleanWindowsPath(p: string): string {
  return p.startsWith('\\\\?\\') ? p.slice(4) : p;
}

function CreateOfficeModal({
  open,
  harnessModels,
  onClose,
  onCreated,
  onToast,
}: {
  open: boolean;
  projects: Project[];
  harnessModels: Record<string, HarnessModelOption[]>;
  onClose: () => void;
  onCreated: (office: Office) => void;
  onToast: (msg: string) => void;
}) {
  const [name, setName] = useState('');
  const [customCwd, setCustomCwd] = useState('');
  const [masterName, setMasterName] = useState('Master');
  const [masterHarness, setMasterHarness] = useState<HarnessId>('agy-cli');
  const [masterModel, setMasterModel] = useState(
    () => getModelsForHarness('agy-cli', harnessModels)[0] ?? 'gemini-4-hydrogen',
  );
  const [masterEffort, setMasterEffort] = useState<ReasoningEffort>(() => {
    const initialModel = getModelsForHarness('agy-cli', harnessModels)[0] ?? 'gemini-4-hydrogen';
    const valid = getEffortsForModel('agy-cli', initialModel, harnessModels);
    if (valid.length === 0) return '';
    return valid.includes('medium') ? 'medium' : valid[0];
  });
  const [maxRounds, setMaxRounds] = useState(0);
  const [wslDistros, setWslDistros] = useState<WslDistro[]>([]);
  const [wslDistro, setWslDistro] = useState('');
  const [folderPickerOpen, setFolderPickerOpen] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (!open) return;
    setName('');
    setCustomCwd('');
    setMasterName('Master');
    setWslDistro('');
    if (isTauriRuntime()) {
      tauriApi
        .listWslDistros()
        .then((list) => setWslDistros(list ?? []))
        .catch(() => setWslDistros([]));
    }
  }, [open]);

  useEffect(() => {
    const models = getModelsForHarness(masterHarness, harnessModels);
  const validMasterEfforts = getEffortsForModel(masterHarness, masterModel, harnessModels);
    if (models.length > 0 && !models.includes(masterModel)) {
      setMasterModel(models[0]);
    }
  }, [masterHarness, harnessModels, masterModel]);

  useEffect(() => {
    const valid = getEffortsForModel(masterHarness, masterModel, harnessModels);
    if (valid.length === 0) {
      if (masterEffort !== '') setMasterEffort('');
    } else if (!valid.includes(masterEffort)) {
      setMasterEffort(valid.includes('medium') ? 'medium' : valid[0]);
    }
  }, [masterHarness, masterModel, harnessModels, masterEffort]);

  useEffect(() => {
    if (!open || folderPickerOpen) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open, folderPickerOpen, onClose]);

  if (!open) return null;

  const models = getModelsForHarness(masterHarness, harnessModels);
  const validMasterEfforts = getEffortsForModel(masterHarness, masterModel, harnessModels);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      onToast('Office name is required');
      return;
    }
    if (!customCwd.trim()) {
      onToast('Please choose a workspace folder');
      return;
    }

    setSubmitting(true);
    try {
      const created = await tauriApi.createOffice({
        name: name.trim(),
        cwd: cleanWindowsPath(customCwd.trim()),
        masterName: masterName.trim() || 'Master',
        masterHarness,
        masterModel,
        masterEffort,
        maxRounds,
        runtime: wslDistro ? 'wsl' : 'native',
        wslDistro: wslDistro || undefined,
      });
      if (created) {
        onToast(`Created office "${created.name}"`);
        onCreated(created);
        onClose();
      }
    } catch (err) {
      onToast(`Failed to create office: ${String(err)}`);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <>
      <div
        className="fixed inset-0 z-50 flex items-center justify-center p-4"
        style={{ backgroundColor: 'rgba(0, 0, 0, 0.45)' }}
        onClick={onClose}
      >
        <div
          role="dialog"
          aria-modal="true"
          aria-label="Create Office"
          className="sakana-card rounded-[12px] w-full max-w-lg overflow-hidden pop-enter"
          onClick={(e) => e.stopPropagation()}
        >
          <div
            className="px-4 py-3 border-b flex items-center justify-between"
            style={{ borderColor: 'var(--border-line)' }}
          >
            <div className="flex items-center gap-2">
              <Buildings size={16} style={{ color: 'var(--accent-blue)' }} />
              <span className="text-[13.5px] font-semibold" style={{ color: 'var(--text-main)' }}>
                Create New Office
              </span>
            </div>
            <button
              type="button"
              onClick={onClose}
              aria-label="Close"
              className="btn-ghost p-1 cursor-pointer"
            >
              <X size={14} />
            </button>
          </div>

          <form onSubmit={(e) => void handleSubmit(e)} className="p-4 space-y-3.5">
            <div>
              <label
                className="block text-[11.5px] font-medium mb-1"
                style={{ color: 'var(--text-body)' }}
              >
                Office Name
              </label>
              <input
                type="text"
                autoFocus
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. Sakana Core Team"
                className="w-full rounded-[7px] border px-3 py-1.5 text-[12.5px]"
                style={{
                  backgroundColor: 'var(--bg-app)',
                  borderColor: 'var(--border-line)',
                  color: 'var(--text-main)',
                }}
              />
            </div>

            {wslDistros.length > 0 && (
              <div>
                <label
                  className="block text-[11.5px] font-medium mb-1"
                  style={{ color: 'var(--text-body)' }}
                >
                  Runtime / WSL Distro
                </label>
                <select
                  value={wslDistro}
                  onChange={(e) => setWslDistro(e.target.value)}
                  className="w-full rounded-[7px] border px-3 py-1.5 text-[12.5px]"
                  style={{
                    backgroundColor: 'var(--bg-app)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-main)',
                  }}
                >
                  <option value="">Native</option>
                  {wslDistros.map((d) => (
                    <option key={d.name} value={d.name}>
                      WSL: {d.name}{d.isDefault ? ' (default)' : ''}
                    </option>
                  ))}
                </select>
              </div>
            )}

            <div>
              <label
                className="block text-[11.5px] font-medium mb-1"
                style={{ color: 'var(--text-body)' }}
              >
                Workspace Directory
              </label>

              <div className="flex items-center gap-2">
                <input
                  type="text"
                  value={customCwd}
                  onChange={(e) => setCustomCwd(e.target.value)}
                  placeholder="C:\Projects\my-app or /home/user/app"
                  className="flex-1 min-w-0 rounded-[7px] border px-3 py-1.5 font-mono-tech text-[11.5px]"
                  style={{
                    backgroundColor: 'var(--bg-app)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-main)',
                  }}
                />
                <button
                  type="button"
                  onClick={async () => {
                    try {
                      const picked = await tauriApi.pickProjectFolder();
                      if (picked) setCustomCwd(cleanWindowsPath(picked));
                    } catch (err) {
                      onToast(`Explorer failed: ${String(err)}`);
                    }
                  }}
                  className="btn-ghost px-2.5 py-1.5 text-[12px] inline-flex items-center gap-1.5 cursor-pointer shrink-0"
                  title="Open default OS File Explorer"
                >
                  <FolderOpen size={14} />
                  <span>Explorer</span>
                </button>
                <button
                  type="button"
                  onClick={() => setFolderPickerOpen(true)}
                  className="btn-ghost px-2.5 py-1.5 text-[12px] inline-flex items-center gap-1.5 cursor-pointer shrink-0"
                  title="Browse inside Sakana folder picker"
                >
                  <FolderOpen size={14} />
                  <span>Browse</span>
                </button>
              </div>
            </div>

            <div className="grid grid-cols-2 gap-3">
              <div>
                <label
                  className="block text-[11.5px] font-medium mb-1"
                  style={{ color: 'var(--text-body)' }}
                >
                  Master Name
                </label>
                <input
                  type="text"
                  value={masterName}
                  onChange={(e) => setMasterName(e.target.value)}
                  placeholder="Master"
                  className="w-full rounded-[7px] border px-3 py-1.5 text-[12.5px]"
                  style={{
                    backgroundColor: 'var(--bg-app)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-main)',
                  }}
                />
              </div>

              <div>
                <label
                  className="block text-[11.5px] font-medium mb-1"
                  style={{ color: 'var(--text-body)' }}
                >
                  Max Rounds
                </label>
                <select
                  value={maxRounds}
                  onChange={(e) => setMaxRounds(Number(e.target.value))}
                  className="w-full rounded-[7px] border px-3 py-1.5 text-[12.5px]"
                  style={{
                    backgroundColor: 'var(--bg-app)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-main)',
                  }}
                >
                  <option value={0}>Unlimited (recommended)</option>
                  {[3, 5, 10, 20].map((r) => (
                    <option key={r} value={r}>
                      {r} rounds
                    </option>
                  ))}
                </select>
              </div>
            </div>

            <div className="grid grid-cols-3 gap-2.5">
              <div>
                <label
                  className="block text-[11.5px] font-medium mb-1"
                  style={{ color: 'var(--text-body)' }}
                >
                  Master Harness
                </label>
                <select
                  value={masterHarness}
                  onChange={(e) => setMasterHarness(e.target.value as HarnessId)}
                  className="w-full rounded-[7px] border px-2.5 py-1.5 text-[12px]"
                  style={{
                    backgroundColor: 'var(--bg-app)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-main)',
                  }}
                >
                  {(Object.keys(HARNESS_META) as HarnessId[]).map((h) => (
                    <option key={h} value={h}>
                      {HARNESS_META[h].name}
                    </option>
                  ))}
                </select>
              </div>

              <div>
                <label
                  className="block text-[11.5px] font-medium mb-1"
                  style={{ color: 'var(--text-body)' }}
                >
                  Master Model
                </label>
                <select
                  value={masterModel}
                  onChange={(e) => setMasterModel(e.target.value)}
                  className="w-full rounded-[7px] border px-2.5 py-1.5 font-mono-tech text-[11.5px]"
                  style={{
                    backgroundColor: 'var(--bg-app)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-main)',
                  }}
                >
                  {models.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              </div>

              {validMasterEfforts.length > 0 && (
                <div>
                  <label
                    className="block text-[11.5px] font-medium mb-1"
                    style={{ color: 'var(--text-body)' }}
                  >
                    Effort
                  </label>
                  <select
                    value={masterEffort}
                    onChange={(e) => setMasterEffort(e.target.value as ReasoningEffort)}
                    className="w-full rounded-[7px] border px-2.5 py-1.5 text-[12px]"
                    style={{
                      backgroundColor: 'var(--bg-app)',
                      borderColor: 'var(--border-line)',
                      color: 'var(--text-main)',
                    }}
                  >
                    {EFFORT_OPTIONS.filter((opt) => validMasterEfforts.includes(opt.value)).map((opt) => (
                      <option key={opt.value} value={opt.value}>
                        {opt.label}
                      </option>
                    ))}
                  </select>
                </div>
              )}
            </div>

            <div
              className="pt-3 border-t flex items-center justify-end gap-2"
              style={{ borderColor: 'var(--border-line)' }}
            >
              <button
                type="button"
                onClick={onClose}
                className="btn-ghost px-3 py-1.5 text-[12px] cursor-pointer"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={submitting}
                className="btn-solid px-3.5 py-1.5 text-[12px] font-medium cursor-pointer disabled:opacity-50"
              >
                {submitting ? 'Creating...' : 'Create Office'}
              </button>
            </div>
          </form>
        </div>
      </div>

      <FolderPickerModal
        open={folderPickerOpen}
        title="Select Office Workspace"
        initialPath={customCwd || undefined}
        wslDistro={wslDistro || undefined}
        onSelect={(path) => {
          setCustomCwd(cleanWindowsPath(path));
          setFolderPickerOpen(false);
        }}
        onClose={() => setFolderPickerOpen(false)}
      />
    </>
  );
}

function HireStaffModal({
  open,
  officeId,
  editingStaff,
  harnessModels,
  onClose,
  onSaved,
  onToast,
}: {
  open: boolean;
  officeId: string;
  editingStaff: OfficeStaff | null;
  harnessModels: Record<string, HarnessModelOption[]>;
  onClose: () => void;
  onSaved: () => Promise<void> | void;
  onToast: (msg: string) => void;
}) {
  const [name, setName] = useState('');
  const [role, setRole] = useState('');
  const [harness, setHarness] = useState<HarnessId>('agy-cli');
  const [model, setModel] = useState('');
  const [effort, setEffort] = useState<ReasoningEffort>('medium');
  const [systemPrompt, setSystemPrompt] = useState('');
  const [isolatedWorktree, setIsolatedWorktree] = useState(false);
  const [avatarColor, setAvatarColor] = useState(AVATAR_COLORS[0]);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (!open) return;
    if (editingStaff) {
      setName(editingStaff.name);
      setRole(editingStaff.role);
      setHarness(editingStaff.harness);
      setModel(editingStaff.model);
      setEffort(editingStaff.effort);
      setSystemPrompt(editingStaff.systemPrompt || '');
      setIsolatedWorktree(editingStaff.isolatedWorktree);
      setAvatarColor(editingStaff.avatarColor || AVATAR_COLORS[0]);
    } else {
      const defaultHarness: HarnessId = 'agy-cli';
      const defaultModels = getModelsForHarness(defaultHarness, harnessModels);
      const initialModel = defaultModels[0] ?? 'gemini-4-hydrogen';
      const valid = getEffortsForModel(defaultHarness, initialModel, harnessModels);
      setName('');
      setRole('');
      setHarness(defaultHarness);
      setModel(initialModel);
      setEffort(valid.length === 0 ? '' : valid.includes('medium') ? 'medium' : valid[0]);
      setSystemPrompt('');
      setIsolatedWorktree(false);
      setAvatarColor(AVATAR_COLORS[Math.floor(Math.random() * AVATAR_COLORS.length)]);
    }
  }, [open, editingStaff, harnessModels]);

  useEffect(() => {
    const models = getModelsForHarness(harness, harnessModels);
  const validEfforts = getEffortsForModel(harness, model, harnessModels);
    if (models.length > 0 && !models.includes(model)) {
      setModel(models[0]);
    }
  }, [harness, harnessModels, model]);

  useEffect(() => {
    const valid = getEffortsForModel(harness, model, harnessModels);
    if (valid.length === 0) {
      if (effort !== '') setEffort('');
    } else if (!valid.includes(effort)) {
      setEffort(valid.includes('medium') ? 'medium' : valid[0]);
    }
  }, [harness, model, harnessModels, effort]);

  useEffect(() => {
    if (!open) return;
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

  const models = getModelsForHarness(harness, harnessModels);
  const validEfforts = getEffortsForModel(harness, model, harnessModels);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !role.trim()) {
      onToast('Name and role are required');
      return;
    }

    setSubmitting(true);
    try {
      if (editingStaff) {
        await tauriApi.updateOfficeStaff({
          id: editingStaff.id,
          name: name.trim(),
          role: role.trim(),
          harness,
          model,
          effort,
          permissionMode: 'yolo',
          systemPrompt: systemPrompt.trim(),
          isolatedWorktree: editingStaff.isMaster ? false : isolatedWorktree,
        });
        onToast(`Updated ${name.trim()}`);
      } else {
        await tauriApi.hireOfficeStaff({
          officeId,
          name: name.trim(),
          role: role.trim(),
          harness,
          model,
          effort,
          permissionMode: 'yolo',
          systemPrompt: systemPrompt.trim() || undefined,
          isolatedWorktree,
          avatarColor,
        });
        onToast(`Hired ${name.trim()}`);
      }
      await onSaved();
      onClose();
    } catch (err) {
      onToast(`Failed to save staff: ${String(err)}`);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ backgroundColor: 'rgba(0, 0, 0, 0.45)' }}
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={editingStaff ? 'Edit Staff' : 'Hire Staff'}
        className="sakana-card rounded-[12px] w-full max-w-lg max-h-[85vh] flex flex-col overflow-hidden pop-enter"
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className="px-4 py-3 border-b flex items-center justify-between shrink-0"
          style={{ borderColor: 'var(--border-line)' }}
        >
          <div className="flex items-center gap-2">
            <UserPlus size={16} style={{ color: 'var(--accent-blue)' }} />
            <span className="text-[13.5px] font-semibold" style={{ color: 'var(--text-main)' }}>
              {editingStaff ? `Edit ${editingStaff.name}` : 'Hire Staff Member'}
            </span>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="btn-ghost p-1 cursor-pointer"
          >
            <X size={14} />
          </button>
        </div>

        <form
          onSubmit={(e) => void handleSubmit(e)}
          className="p-4 space-y-3.5 overflow-y-auto"
        >
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label
                className="block text-[11.5px] font-medium mb-1"
                style={{ color: 'var(--text-body)' }}
              >
                Staff Name
              </label>
              <input
                type="text"
                autoFocus
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. Alice"
                className="w-full rounded-[7px] border px-3 py-1.5 text-[12.5px]"
                style={{
                  backgroundColor: 'var(--bg-app)',
                  borderColor: 'var(--border-line)',
                  color: 'var(--text-main)',
                }}
              />
            </div>

            <div>
              <label
                className="block text-[11.5px] font-medium mb-1"
                style={{ color: 'var(--text-body)' }}
              >
                Role
              </label>
              <input
                type="text"
                value={role}
                onChange={(e) => setRole(e.target.value)}
                placeholder="e.g. Frontend Engineer"
                className="w-full rounded-[7px] border px-3 py-1.5 text-[12.5px]"
                style={{
                  backgroundColor: 'var(--bg-app)',
                  borderColor: 'var(--border-line)',
                  color: 'var(--text-main)',
                }}
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label
                className="block text-[11.5px] font-medium mb-1"
                style={{ color: 'var(--text-body)' }}
              >
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
                {(Object.keys(HARNESS_META) as HarnessId[]).map((h) => (
                  <option key={h} value={h}>
                    {HARNESS_META[h].name}
                  </option>
                ))}
              </select>
            </div>

            <div>
              <label
                className="block text-[11.5px] font-medium mb-1"
                style={{ color: 'var(--text-body)' }}
              >
                Model
              </label>
              <select
                value={model}
                onChange={(e) => setModel(e.target.value)}
                className="w-full rounded-[7px] border px-2.5 py-1.5 font-mono-tech text-[11.5px]"
                style={{
                  backgroundColor: 'var(--bg-app)',
                  borderColor: 'var(--border-line)',
                  color: 'var(--text-main)',
                }}
              >
                {models.map((m) => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            {validEfforts.length > 0 && (
              <div>
                <label
                  className="block text-[11.5px] font-medium mb-1"
                  style={{ color: 'var(--text-body)' }}
                >
                  Reasoning Effort
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
                  {EFFORT_OPTIONS.filter((opt) => validEfforts.includes(opt.value)).map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {opt.label}
                    </option>
                  ))}
                </select>
              </div>
            )}

          </div>

          {!editingStaff && (
            <div>
              <label
                className="block text-[11.5px] font-medium mb-1.5"
                style={{ color: 'var(--text-body)' }}
              >
                Avatar Color
              </label>
              <div className="flex items-center gap-2">
                {AVATAR_COLORS.map((color) => (
                  <button
                    key={color}
                    type="button"
                    aria-label={`Select color ${color}`}
                    onClick={() => setAvatarColor(color)}
                    className="w-6 h-6 rounded-full cursor-pointer transition-transform"
                    style={{
                      backgroundColor: color,
                      transform: avatarColor === color ? 'scale(1.15)' : 'scale(1)',
                      boxShadow:
                        avatarColor === color ? '0 0 0 2px var(--bg-card), 0 0 0 4px var(--text-main)' : 'none',
                    }}
                  />
                ))}
              </div>
            </div>
          )}

          {!editingStaff?.isMaster && (
            <label
              className="flex items-center justify-between gap-3 p-2.5 rounded-[8px] border cursor-pointer"
              style={{
                backgroundColor: 'var(--bg-app)',
                borderColor: 'var(--border-line)',
              }}
            >
              <div className="flex items-center gap-2">
                <GitBranch size={15} style={{ color: 'var(--accent-green)' }} />
                <div>
                  <div className="text-[12px] font-medium" style={{ color: 'var(--text-main)' }}>
                    Isolated Git Worktree
                  </div>
                  <div className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
                    Give this staff member an isolated branch and working copy
                  </div>
                </div>
              </div>
              <input
                type="checkbox"
                checked={isolatedWorktree}
                onChange={(e) => setIsolatedWorktree(e.target.checked)}
                className="w-4 h-4 accent-[var(--accent-blue)] cursor-pointer"
              />
            </label>
          )}

          <div>
            <label
              className="block text-[11.5px] font-medium mb-1"
              style={{ color: 'var(--text-body)' }}
            >
              System Prompt (Optional)
            </label>
            <textarea
              rows={3}
              value={systemPrompt}
              onChange={(e) => setSystemPrompt(e.target.value)}
              placeholder="Role-specific instructions, coding standards, or owned files..."
              className="w-full rounded-[7px] border px-3 py-2 text-[12px] resize-y"
              style={{
                backgroundColor: 'var(--bg-app)',
                borderColor: 'var(--border-line)',
                color: 'var(--text-main)',
              }}
            />
          </div>

          <div
            className="pt-3 border-t flex items-center justify-end gap-2"
            style={{ borderColor: 'var(--border-line)' }}
          >
            <button
              type="button"
              onClick={onClose}
              className="btn-ghost px-3 py-1.5 text-[12px] cursor-pointer"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="btn-solid px-3.5 py-1.5 text-[12px] font-medium cursor-pointer disabled:opacity-50"
            >
              {submitting ? 'Saving...' : editingStaff ? 'Save Changes' : 'Hire Staff'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

export function OfficeView({
  offices,
  selectedOffice,
  projects,
  harnessModels,
  delegations,
  onSelectOffice,
  onRefreshOffices,
  onToast,
  createModalOpen,
  onSetCreateModalOpen,
  sidebarOpen = true,
  onToggleSidebar,
}: OfficeViewProps) {
  const [internalCreateOpen, setInternalCreateOpen] = useState(false);
  const isCreateOpen = createModalOpen ?? internalCreateOpen;
  const setCreateOpen = onSetCreateModalOpen ?? setInternalCreateOpen;

  const [hireModalOpen, setHireModalOpen] = useState(false);
  const [editingStaff, setEditingStaff] = useState<OfficeStaff | null>(null);

  const [selectedStaffId, setSelectedStaffId] = useState<string>('');
  const [chatTab, setChatTab] = useState<'chat' | 'activity'>('chat');

  // Resizable right chat panel width
  const [panelWidth, setPanelWidth] = useState(400);
  const [isDragging, setIsDragging] = useState(false);
  const containerRef = useRef<HTMLDivElement | null>(null);

  // Keep selectedStaffId valid when office changes
  useEffect(() => {
    if (!selectedOffice) {
      setSelectedStaffId('');
      return;
    }
    const exists = selectedOffice.staff.some((s) => s.id === selectedStaffId);
    if (!exists) {
      const master =
        selectedOffice.staff.find((s) => s.id === selectedOffice.masterStaffId || s.isMaster) ??
        selectedOffice.staff[0];
      setSelectedStaffId(master?.id ?? '');
    }
  }, [selectedOffice, selectedStaffId]);

  // Handle pane resizer drag
  useEffect(() => {
    if (!isDragging) return;
    document.body.classList.add('is-resizing');

    const onMouseMove = (e: MouseEvent) => {
      const rect = containerRef.current?.getBoundingClientRect();
      if (!rect) return;
      const next = Math.min(620, Math.max(300, rect.right - e.clientX));
      setPanelWidth(next);
    };

    const onMouseUp = () => {
      setIsDragging(false);
      document.body.classList.remove('is-resizing');
    };

    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
    return () => {
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
      document.body.classList.remove('is-resizing');
    };
  }, [isDragging]);

  const handleMaxRoundsChange = async (nextRounds: number) => {
    if (!selectedOffice) return;
    try {
      await tauriApi.updateOfficeSettings(selectedOffice.id, nextRounds);
      onToast(`Max rounds set to ${nextRounds}`);
      void onRefreshOffices();
    } catch (err) {
      onToast(`Failed to update settings: ${String(err)}`);
    }
  };

  const handleStopRun = async () => {
    if (!selectedOffice) return;
    try {
      await tauriApi.stopOfficeRun(selectedOffice.id);
      onToast('Stopped office run');
      void onRefreshOffices();
    } catch (err) {
      onToast(`Failed to stop run: ${String(err)}`);
    }
  };

  const handleFireStaff = useCallback(
    async (staff: OfficeStaff) => {
      try {
        await tauriApi.fireOfficeStaff(staff.id);
        onToast(`Removed ${staff.name}`);
        await onRefreshOffices();
      } catch (err) {
        onToast(`Failed to fire staff: ${String(err)}`);
      }
    },
    [onRefreshOffices, onToast],
  );

  const [blueprints, setBlueprints] = useState<OfficeBlueprint[]>([]);
  const [blueprintMenuOpen, setBlueprintMenuOpen] = useState(false);

  useEffect(() => {
    tauriApi.listOfficeBlueprints().then(setBlueprints).catch(() => {});
  }, []);

  const handleSaveBlueprint = async () => {
    if (!selectedOffice) return;
    const bpName = `${selectedOffice.name} Blueprint`;
    try {
      const saved = await tauriApi.saveOfficeBlueprint(selectedOffice.id, bpName);
      if (saved) {
        setBlueprints((prev) => [saved, ...prev.filter((b) => b.id !== saved.id)]);
        onToast(`Saved blueprint "${saved.name}"`);
      }
    } catch (err) {
      onToast(`Failed to save blueprint: ${String(err)}`);
    }
  };

  const handleCreateFromBlueprint = async (bp: OfficeBlueprint) => {
    const projectId = selectedOffice?.projectId || projects[0]?.id;
    if (!projectId) {
      onToast('Create a project first before using a blueprint');
      return;
    }
    setBlueprintMenuOpen(false);
    try {
      const created = await tauriApi.createOfficeFromBlueprint(bp.id, projectId);
      if (created) {
        onToast(`Created office from "${bp.name}"`);
        await onRefreshOffices();
        onSelectOffice(created.id);
      }
    } catch (err) {
      onToast(`Failed to create from blueprint: ${String(err)}`);
    }
  };

  const handleDeleteOffice = async () => {
    if (!selectedOffice) return;
    try {
      await tauriApi.deleteOffice(selectedOffice.id);
      onToast(`Deleted office "${selectedOffice.name}"`);
      await onRefreshOffices();
    } catch (err) {
      onToast(`Failed to delete office: ${String(err)}`);
    }
  };

  if (!selectedOffice) {
    return (
      <div
        className="flex-1 h-full flex flex-col relative"
        style={{ backgroundColor: 'var(--bg-canvas)' }}
      >
        {!sidebarOpen && onToggleSidebar && (
          <div className="absolute top-3 left-3 z-10">
            <button
              type="button"
              onClick={onToggleSidebar}
              className="btn-ghost p-1.5 cursor-pointer"
              title="Show Sidebar (Ctrl+B)"
              aria-label="Show Sidebar"
            >
              <SidebarSimple size={15} />
            </button>
          </div>
        )}
        <div className="flex-1 flex items-center justify-center p-8">
        <div
          className="sakana-card rounded-[14px] p-8 max-w-md w-full text-center space-y-4"
        >
          <div
            className="w-12 h-12 rounded-[12px] mx-auto flex items-center justify-center"
            style={{
              backgroundColor: 'var(--accent-blue-bg)',
              color: 'var(--accent-blue)',
            }}
          >
            <Buildings size={24} weight="duotone" />
          </div>
          <div className="space-y-1.5">
            <h2 className="text-[16px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
              {offices.length === 0 ? 'Create Your First Office' : 'Select an Office'}
            </h2>
            <p className="text-[12.5px] m-0" style={{ color: 'var(--text-muted)' }}>
              Assemble a Master Orchestrator and specialized Staff agents in a spatial workspace
              with live delegation and direct desk chat.
            </p>
          </div>
          <button
            type="button"
            onClick={() => setCreateOpen(true)}
            className="btn-solid px-4 py-2 text-[12.5px] font-medium inline-flex items-center gap-2 cursor-pointer"
          >
            <Plus size={14} weight="bold" />
            <span>Create Your First Office</span>
          </button>
        </div>
        </div>

        <CreateOfficeModal
          open={isCreateOpen}
          projects={projects}
          harnessModels={harnessModels}
          onClose={() => setCreateOpen(false)}
          onCreated={(office) => {
            onSelectOffice(office.id);
            void onRefreshOffices();
          }}
          onToast={onToast}
        />
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="flex-1 h-full min-w-0 flex flex-col overflow-hidden"
      style={{ backgroundColor: 'var(--bg-app)' }}
    >
      {/* Top Header Bar */}
      <header
        className="px-4 py-2.5 border-b flex items-center justify-between gap-3 flex-wrap shrink-0"
        style={{
          backgroundColor: 'var(--bg-card)',
          borderColor: 'var(--border-line)',
        }}
      >
        <div className="flex items-center gap-2.5 min-w-0 flex-wrap">
          {!sidebarOpen && onToggleSidebar && (
            <button
              type="button"
              onClick={onToggleSidebar}
              className="btn-ghost p-1.5 cursor-pointer shrink-0"
              title="Show Sidebar (Ctrl+B)"
              aria-label="Show Sidebar"
            >
              <SidebarSimple size={15} />
            </button>
          )}
          <div className="flex items-center gap-2 min-w-0">
            <Buildings size={16} style={{ color: 'var(--accent-blue)' }} />
            <h1
              className="text-[14px] font-semibold m-0 truncate"
              style={{ color: 'var(--text-main)' }}
            >
              {selectedOffice.name}
            </h1>
          </div>

          <span
            className="px-2 py-0.5 rounded-[6px] font-mono-tech text-[11px] truncate max-w-[280px] border"
            style={{
              backgroundColor: 'var(--bg-app)',
              borderColor: 'var(--border-line)',
              color: 'var(--text-muted)',
            }}
            title={selectedOffice.cwd}
          >
            {selectedOffice.cwd}
          </span>

          {selectedOffice.status === 'RUNNING' && (
            <span
              className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[11px] font-medium"
              style={{
                backgroundColor: 'var(--accent-blue-bg)',
                color: 'var(--accent-blue)',
              }}
            >
              <span
                className="w-1.5 h-1.5 rounded-full pulse-dot"
                style={{ backgroundColor: 'var(--accent-blue)' }}
              />
              Running
            </span>
          )}
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          <div className="relative inline-flex items-center">
            <select
              aria-label="Max rounds"
              value={selectedOffice.maxRounds}
              onChange={(e) => void handleMaxRoundsChange(Number(e.target.value))}
              className="appearance-none rounded-[7px] border pl-2.5 pr-6 py-1 text-[11.5px] font-medium cursor-pointer"
              style={{
                backgroundColor: 'var(--bg-app)',
                borderColor: 'var(--border-line)',
                color: 'var(--text-body)',
              }}
            >
              <option value={0}>Rounds: Unlimited</option>
              {selectedOffice.maxRounds > 0 &&
                ![3, 5, 10, 20].includes(selectedOffice.maxRounds) && (
                  <option value={selectedOffice.maxRounds}>
                    Max Rounds: {selectedOffice.maxRounds}
                  </option>
                )}
              {[3, 5, 10, 20].map((n) => (
                <option key={n} value={n}>
                  Max Rounds: {n}
                </option>
              ))}
            </select>
            <CaretDown
              size={11}
              className="pointer-events-none absolute right-2"
              style={{ color: 'var(--text-muted)' }}
            />
          </div>

          <button
            type="button"
            onClick={() => {
              setEditingStaff(null);
              setHireModalOpen(true);
            }}
            className="btn-solid px-3 py-1 text-[11.5px] font-medium inline-flex items-center gap-1.5 cursor-pointer"
          >
            <UserPlus size={13} weight="bold" />
            <span>Hire Staff</span>
          </button>

          {selectedOffice.status === 'RUNNING' && (
            <button
              type="button"
              onClick={() => void handleStopRun()}
              className="px-3 py-1 rounded-[8px] text-[11.5px] font-medium inline-flex items-center gap-1.5 cursor-pointer border"
              style={{
                backgroundColor: 'var(--accent-red-bg)',
                borderColor: 'var(--accent-red)',
                color: 'var(--accent-red)',
              }}
            >
              <Stop size={12} weight="fill" />
              <span>Stop Run</span>
            </button>
          )}

          <div className="relative">
            <button
              type="button"
              onClick={() => setBlueprintMenuOpen((v) => !v)}
              className="btn-ghost px-2.5 py-1 text-[11.5px] font-medium inline-flex items-center gap-1 cursor-pointer"
              title="Create office from a saved or built-in blueprint"
            >
              <Sparkle size={13} />
              <span>Blueprints</span>
            </button>
            {blueprintMenuOpen && (
              <div
                className="sakana-popover pop-enter absolute right-0 top-full mt-1.5 w-64 rounded-[10px] p-1.5 z-50 space-y-1"
              >
                <div className="px-2 py-1 text-[10px] font-mono-tech uppercase" style={{ color: 'var(--text-muted)' }}>
                  Office Blueprints ({blueprints.length})
                </div>
                {blueprints.length === 0 ? (
                  <div className="px-2 py-2 text-[11px]" style={{ color: 'var(--text-muted)' }}>
                    No blueprints yet.
                  </div>
                ) : (
                  blueprints.map((bp) => (
                    <button
                      key={bp.id}
                      type="button"
                      onClick={() => void handleCreateFromBlueprint(bp)}
                      className="w-full px-2 py-1.5 rounded-[6px] text-left hover:bg-[var(--bg-hover)] cursor-pointer"
                    >
                      <div className="text-[12px] font-medium truncate" style={{ color: 'var(--text-main)' }}>
                        {bp.name}
                      </div>
                      <div className="text-[10.5px] truncate" style={{ color: 'var(--text-muted)' }}>
                        {bp.description || `${bp.config.staff.length} staff`}
                      </div>
                    </button>
                  ))
                )}
              </div>
            )}
          </div>

          <button
            type="button"
            onClick={() => void handleSaveBlueprint()}
            aria-label="Save as Blueprint"
            title="Save current office as reusable blueprint"
            className="btn-ghost p-1.5 cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
          >
            <BookmarkSimple size={14} />
          </button>

          <button
            type="button"
            onClick={() => void handleDeleteOffice()}
            aria-label="Delete Office"
            title="Delete Office"
            className="btn-ghost p-1.5 cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
          >
            <Trash size={14} />
          </button>
        </div>
      </header>

      {/* Split Layout: Spatial Canvas + Docked Right Chat Panel */}
      <div className="flex-1 min-h-0 flex overflow-hidden">
        <OfficeCanvas
          office={selectedOffice}
          delegations={delegations}
          selectedStaffId={selectedStaffId}
          onSelectStaff={(id) => {
            setSelectedStaffId(id);
          }}
          onMessageStaff={(id) => {
            setSelectedStaffId(id);
            setChatTab('chat');
          }}
          onEditStaff={(staff) => {
            setEditingStaff(staff);
            setHireModalOpen(true);
          }}
          onFireStaff={(staff) => void handleFireStaff(staff)}
          onHireStaff={() => {
            setEditingStaff(null);
            setHireModalOpen(true);
          }}
        />

        <div
          role="separator"
          tabIndex={0}
          aria-orientation="vertical"
          aria-label="Resize office chat panel"
          aria-valuemin={300}
          aria-valuemax={620}
          aria-valuenow={panelWidth}
          onMouseDown={() => setIsDragging(true)}
          onKeyDown={(e) => {
            if (e.key === 'ArrowLeft') {
              e.preventDefault();
              setPanelWidth((w) => Math.min(620, w + 24));
            } else if (e.key === 'ArrowRight') {
              e.preventDefault();
              setPanelWidth((w) => Math.max(300, w - 24));
            }
          }}
          className={`pane-resizer ${isDragging ? 'is-dragging' : ''}`}
        />

        <div style={{ width: `${panelWidth}px` }} className="shrink-0 h-full min-w-0">
          <OfficeChatPanel
            office={selectedOffice}
            selectedStaffId={selectedStaffId}
            onSelectStaff={setSelectedStaffId}
            harnessModels={harnessModels}
            onRefreshOffices={onRefreshOffices}
            onToast={onToast}
            activeTab={chatTab}
            onTabChange={setChatTab}
          />
        </div>
      </div>

      <CreateOfficeModal
        open={isCreateOpen}
        projects={projects}
        harnessModels={harnessModels}
        onClose={() => setCreateOpen(false)}
        onCreated={(office) => {
          onSelectOffice(office.id);
          void onRefreshOffices();
        }}
        onToast={onToast}
      />

      <HireStaffModal
        open={hireModalOpen}
        officeId={selectedOffice.id}
        editingStaff={editingStaff}
        harnessModels={harnessModels}
        onClose={() => {
          setHireModalOpen(false);
          setEditingStaff(null);
        }}
        onSaved={onRefreshOffices}
        onToast={onToast}
      />
    </div>
  );
}
