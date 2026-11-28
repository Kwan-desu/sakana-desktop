import { useEffect, useRef, useState } from 'react';
import { ArrowUp, FolderSimple, GitBranch, X } from '@phosphor-icons/react';
import type { DirListing } from '../types';
import { tauriApi } from '../lib/tauriApi';

interface FolderPickerModalProps {
  open: boolean;
  title: string;
  initialPath?: string;
  wslDistro?: string;
  onSelect: (path: string) => void;
  onClose: () => void;
}

export function FolderPickerModal({
  open,
  title,
  initialPath,
  wslDistro,
  onSelect,
  onClose,
}: FolderPickerModalProps) {
  const [listing, setListing] = useState<DirListing | null>(null);
  const [manualPath, setManualPath] = useState(initialPath || '');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pathInputRef = useRef<HTMLInputElement | null>(null);

  const loadDir = async (target?: string) => {
    setLoading(true);
    setError(null);
    try {
      const res = await tauriApi.listDirectories(target || undefined, wslDistro);
      if (res) {
        setListing(res);
        setManualPath(res.path);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (open) {
      void loadDir(initialPath || undefined);
      pathInputRef.current?.focus();
    }
  }, [open, wslDistro]);

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

  return (
    <div
      className="modal-backdrop-enter fixed inset-0 z-50 flex items-center justify-center p-4 backdrop-blur-[2px]"
      style={{ backgroundColor: 'rgba(0,0,0,0.45)' }}
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="sakana-card modal-panel-enter rounded-[12px] w-full max-w-lg max-h-[75vh] flex flex-col overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className="px-4 py-3 border-b flex items-center justify-between gap-2"
          style={{ borderColor: 'var(--border-line)' }}
        >
          <div>
            <div className="text-[13.5px] font-semibold" style={{ color: 'var(--text-main)' }}>
              {title}
            </div>
            <div className="font-mono-tech text-[10.5px]" style={{ color: 'var(--text-muted)' }}>
              {wslDistro ? `WSL · ${wslDistro}` : listing?.os || 'Remote filesystem'}
            </div>
          </div>
          <button type="button" onClick={onClose} aria-label="Close" className="btn-ghost p-1 cursor-pointer">
            <X size={14} />
          </button>
        </div>

        <form
          onSubmit={(e) => {
            e.preventDefault();
            void loadDir(manualPath.trim());
          }}
          className="px-4 py-2.5 border-b flex items-center gap-1.5"
          style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
        >
          <button
            type="button"
            disabled={!listing?.parent || loading}
            onClick={() => listing?.parent && void loadDir(listing.parent)}
            className="btn-ghost p-1.5 cursor-pointer disabled:opacity-40"
            title="Parent directory"
            aria-label="Parent directory"
          >
            <ArrowUp size={13} />
          </button>
          <input
            ref={pathInputRef}
            value={manualPath}
            onChange={(e) => setManualPath(e.target.value)}
            placeholder="/home/user/projects"
            className="flex-1 min-w-0 rounded-[6px] border px-2.5 py-1 font-mono-tech text-[11.5px]"
            style={{
              backgroundColor: 'var(--bg-surface)',
              borderColor: 'var(--border-line)',
              color: 'var(--text-main)',
            }}
          />
          <button type="submit" className="btn-ghost px-2.5 py-1 text-[11.5px] cursor-pointer">
            Go
          </button>
        </form>

        <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
          {loading && (
            <div className="py-8 text-center text-[12px]" style={{ color: 'var(--text-muted)' }}>
              Loading directories...
            </div>
          )}
          {error && (
            <div className="p-3 rounded-[8px] font-mono-tech text-[11px]" style={{ color: 'var(--accent-red)' }}>
              {error}
            </div>
          )}
          {!loading && !error && (listing?.entries.length ?? 0) === 0 && (
            <div className="py-8 text-center text-[12px]" style={{ color: 'var(--text-muted)' }}>
              No subdirectories in this folder.
            </div>
          )}
          {!loading &&
            listing?.entries.map((entry) => (
              <button
                key={entry.path}
                type="button"
                onClick={() => void loadDir(entry.path)}
                className="w-full px-3 py-2 rounded-[7px] text-left flex items-center justify-between gap-2 hover:bg-[var(--bg-elevated)] cursor-pointer"
              >
                <span className="inline-flex items-center gap-2 min-w-0 text-[12px]" style={{ color: 'var(--text-main)' }}>
                  <FolderSimple size={14} style={{ color: 'var(--text-muted)' }} />
                  <span className="truncate">{entry.name}</span>
                </span>
                {entry.isGitRepo && (
                  <span
                    className="font-mono-tech text-[9.5px] px-1.5 py-0.5 rounded-[4px] inline-flex items-center gap-1 shrink-0"
                    style={{ backgroundColor: 'var(--accent-green-bg)', color: 'var(--accent-green)' }}
                  >
                    <GitBranch size={10} />
                    <span>git</span>
                  </span>
                )}
              </button>
            ))}
        </div>

        <div
          className="px-4 py-3 border-t flex items-center justify-between gap-2"
          style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
        >
          <span className="font-mono-tech text-[11px] truncate" style={{ color: 'var(--text-muted)' }}>
            {listing?.path || manualPath}
          </span>
          <div className="flex items-center gap-2 shrink-0">
            <button type="button" onClick={onClose} className="btn-ghost px-3 py-1.5 text-[11.5px] cursor-pointer">
              Cancel
            </button>
            <button
              type="button"
              onClick={() => {
                const chosen = (listing?.path || manualPath).trim();
                if (chosen) onSelect(chosen);
              }}
              className="btn-solid px-3 py-1.5 text-[11.5px] font-medium cursor-pointer"
            >
              Select Folder
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
