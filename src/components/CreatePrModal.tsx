import { useEffect, useRef, useState } from 'react';
import { GitPullRequest, Sparkle, X } from '@phosphor-icons/react';
import type { Thread } from '../types';

interface CreatePrModalProps {
  open: boolean;
  onClose: () => void;
  thread?: Thread;
  onSubmit: (opts: {
    title: string;
    body: string;
    baseBranch: string;
  }) => Promise<void> | void;
}

export function CreatePrModal({ open, onClose, thread, onSubmit }: CreatePrModalProps) {
  const [title, setTitle] = useState(`feat: ${(thread?.title || 'worktree changes').toLowerCase()}`);
  const [baseBranch, setBaseBranch] = useState('main');
  const [body, setBody] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const titleInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!open || !thread) return;
    setTitle(`feat: ${(thread.title || 'worktree changes').toLowerCase()}`);
    setBaseBranch('main');
    setBody(
      `## Summary\n- Isolated per-thread Git worktree (\`${thread.branch || 'main'}\`)\n- Changes: +${thread.additions} -${thread.deletions}`,
    );
    setError(null);
    setSubmitting(false);
    titleInputRef.current?.focus();
  }, [open, thread?.id]);

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

  if (!open || !thread) return null;

  const handleSubmit = async () => {
    if (!title.trim() || submitting) return;
    setSubmitting(true);
    setError(null);
    try {
      await onSubmit({
        title: title.trim(),
        body,
        baseBranch: baseBranch.trim() || 'main',
      });
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      className="modal-backdrop-enter fixed inset-0 z-50 bg-black/45 backdrop-blur-[2px] flex items-center justify-center p-4"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Create Pull Request"
        className="sakana-popover modal-panel-enter w-full max-w-lg rounded-[14px] p-5 space-y-4"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between border-b pb-3" style={{ borderColor: 'var(--border-line)' }}>
          <div className="flex items-center gap-2">
            <GitPullRequest size={18} style={{ color: 'var(--accent-blue)' }} />
            <div>
              <h2 className="text-[15px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
                Create Pull Request
              </h2>
              <p className="font-mono-tech text-[11px] m-0" style={{ color: 'var(--text-muted)' }}>
                {thread.branch} → {baseBranch || 'main'} (+{thread.additions} -{thread.deletions})
              </p>
            </div>
          </div>
          <button type="button" onClick={onClose} aria-label="Close" className="p-1 cursor-pointer" style={{ color: 'var(--text-muted)' }}>
            <X size={15} />
          </button>
        </div>

        <div className="space-y-3">
          <div className="grid grid-cols-3 gap-2.5">
            <div className="col-span-2">
              <label className="block text-[11.5px] font-medium mb-1" style={{ color: 'var(--text-muted)' }}>
                PR Title
              </label>
              <input
                ref={titleInputRef}
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                className="w-full rounded-[7px] border px-3 py-1.5 text-[13px] focus:outline-none"
                style={{
                  backgroundColor: 'var(--bg-app)',
                  borderColor: 'var(--border-line)',
                  color: 'var(--text-main)',
                }}
              />
            </div>
            <div>
              <label className="block text-[11.5px] font-medium mb-1" style={{ color: 'var(--text-muted)' }}>
                Base Branch
              </label>
              <input
                value={baseBranch}
                onChange={(e) => setBaseBranch(e.target.value)}
                className="w-full rounded-[7px] border px-2.5 py-1.5 font-mono-tech text-[12px] focus:outline-none"
                style={{
                  backgroundColor: 'var(--bg-app)',
                  borderColor: 'var(--border-line)',
                  color: 'var(--text-main)',
                }}
              />
            </div>
          </div>
          <div>
            <div className="flex items-center justify-between mb-1">
              <label className="text-[11.5px] font-medium" style={{ color: 'var(--text-muted)' }}>
                Description
              </label>
              <span className="inline-flex items-center gap-1 font-mono-tech text-[10.5px]" style={{ color: 'var(--accent-blue)' }}>
                <Sparkle size={11} />
                AI-generated from turn diffs
              </span>
            </div>
            <textarea
              rows={5}
              value={body}
              onChange={(e) => setBody(e.target.value)}
              className="w-full rounded-[7px] border p-3 font-mono-tech text-[11.5px] focus:outline-none"
              style={{
                backgroundColor: 'var(--bg-app)',
                borderColor: 'var(--border-line)',
                color: 'var(--text-main)',
              }}
            />
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
        </div>

        <div className="flex justify-end gap-2 pt-1">
          <button
            type="button"
            onClick={onClose}
            disabled={submitting}
            className="btn-ghost px-3.5 py-1.5 text-[12px] cursor-pointer disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            disabled={submitting || !title.trim()}
            onClick={() => void handleSubmit()}
            className="btn-solid px-4 py-1.5 text-[12px] font-medium cursor-pointer disabled:opacity-50"
          >
            {submitting ? 'Creating PR...' : 'Push & Open PR'}
          </button>
        </div>
      </div>
    </div>
  );
}
