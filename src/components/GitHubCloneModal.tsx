import { useEffect, useMemo, useState } from 'react';
import {
  ArrowSquareOut,
  Check,
  FolderOpen,
  GitBranch,
  GithubLogo,
  Key,
  Lock,
  MagnifyingGlass,
  SignOut,
  SpinnerGap,
  Star,
  X,
} from '@phosphor-icons/react';
import type { GitHubAuthStatus, GitHubRepo } from '../types';
import { tauriApi } from '../lib/tauriApi';

interface GitHubCloneModalProps {
  open: boolean;
  onClose: () => void;
  onClonedProject: (opts: { name: string; cwd: string }) => Promise<void> | void;
  onToast?: (msg: string) => void;
}

export function GitHubCloneModal({
  open,
  onClose,
  onClonedProject,
  onToast,
}: GitHubCloneModalProps) {
  const [status, setStatus] = useState<GitHubAuthStatus | null>(null);
  const [loadingStatus, setLoadingStatus] = useState(false);
  const [tokenInput, setTokenInput] = useState('');
  const [savingToken, setSavingToken] = useState(false);
  const [authError, setAuthError] = useState('');

  const [repos, setRepos] = useState<GitHubRepo[]>([]);
  const [loadingRepos, setLoadingRepos] = useState(false);
  const [search, setSearch] = useState('');
  const [targetDir, setTargetDir] = useState('');
  const [cloningId, setCloningId] = useState<number | null>(null);
  const [cloneError, setCloneError] = useState('');

  const refreshStatusAndRepos = async () => {
    setLoadingStatus(true);
    setAuthError('');
    try {
      const st = await tauriApi.githubAuthStatus();
      setStatus(st);
      if (st?.defaultCloneDir && !targetDir) {
        setTargetDir(st.defaultCloneDir);
      }
      if (st?.authenticated) {
        setLoadingRepos(true);
        try {
          const list = await tauriApi.githubListRepos();
          setRepos(list);
        } catch (err) {
          setAuthError(String(err));
        } finally {
          setLoadingRepos(false);
        }
      } else {
        setRepos([]);
      }
    } catch (err) {
      setAuthError(String(err));
    } finally {
      setLoadingStatus(false);
    }
  };

  useEffect(() => {
    if (!open) return;
    void refreshStatusAndRepos();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  const filteredRepos = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return repos;
    return repos.filter(
      (r) =>
        r.fullName.toLowerCase().includes(q) ||
        r.description.toLowerCase().includes(q) ||
        r.language.toLowerCase().includes(q),
    );
  }, [repos, search]);

  if (!open) return null;

  const handleSaveToken = async () => {
    if (!tokenInput.trim()) return;
    setSavingToken(true);
    setAuthError('');
    try {
      const st = await tauriApi.githubSaveToken(tokenInput.trim());
      setStatus(st);
      setTokenInput('');
      if (st?.defaultCloneDir && !targetDir) {
        setTargetDir(st.defaultCloneDir);
      }
      if (st?.authenticated) {
        onToast?.(`Connected to GitHub as @${st.login}`);
        setLoadingRepos(true);
        const list = await tauriApi.githubListRepos();
        setRepos(list);
        setLoadingRepos(false);
      }
    } catch (err) {
      setAuthError(String(err));
    } finally {
      setSavingToken(false);
    }
  };

  const handleDisconnect = async () => {
    setSavingToken(true);
    setAuthError('');
    try {
      const st = await tauriApi.githubSaveToken('');
      setStatus(st);
      if (!st?.authenticated) {
        setRepos([]);
        onToast?.('Disconnected GitHub token');
      } else {
        onToast?.(`Still connected via gh CLI (@${st.login})`);
      }
    } catch (err) {
      setAuthError(String(err));
    } finally {
      setSavingToken(false);
    }
  };

  const handlePickTargetDir = async () => {
    try {
      const picked = await tauriApi.pickProjectFolder();
      if (picked) setTargetDir(picked);
    } catch {
      // ignore
    }
  };

  const handleCloneRepo = async (repo: GitHubRepo) => {
    setCloningId(repo.id);
    setCloneError('');
    try {
      const clonedPath = await tauriApi.githubCloneRepo(
        repo.cloneUrl,
        repo.name,
        targetDir.trim() || undefined,
      );
      if (!clonedPath) {
        throw new Error('Clone returned no path');
      }
      await onClonedProject({ name: repo.name, cwd: clonedPath });
      onToast?.(`Opened ${repo.fullName} in workspace`);
      onClose();
    } catch (err) {
      setCloneError(String(err));
    } finally {
      setCloningId(null);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ backgroundColor: 'rgba(0, 0, 0, 0.45)', backdropFilter: 'blur(3px)' }}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="sakana-popover pop-enter w-full max-w-[600px] max-h-[82vh] rounded-[14px] border flex flex-col overflow-hidden"
        style={{ backgroundColor: 'var(--bg-card)', borderColor: 'var(--border-strong)' }}
      >
        {/* Header */}
        <div
          className="px-4 py-3 border-b flex items-center justify-between gap-2 shrink-0"
          style={{ borderColor: 'var(--border-line)' }}
        >
          <div className="flex items-center gap-2 min-w-0">
            <GithubLogo size={17} weight="fill" style={{ color: 'var(--text-main)' }} />
            <span className="text-[13.5px] font-semibold truncate" style={{ color: 'var(--text-main)' }}>
              Clone from GitHub
            </span>
            {status?.authenticated && (
              <span
                className="font-mono-tech text-[10px] px-2 py-0.5 rounded-full inline-flex items-center gap-1"
                style={{ backgroundColor: 'var(--accent-green-bg)', color: 'var(--accent-green)' }}
              >
                <Check size={10} weight="bold" />
                @{status.login}
              </span>
            )}
          </div>
          <button
            type="button"
            onClick={onClose}
            className="p-1 rounded-[6px] hover:bg-[var(--bg-hover)] cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
            aria-label="Close GitHub modal"
          >
            <X size={14} />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 min-h-0 overflow-y-auto p-4 space-y-3">
          {loadingStatus && !status ? (
            <div className="py-10 flex items-center justify-center gap-2 text-[12px]" style={{ color: 'var(--text-muted)' }}>
              <SpinnerGap size={15} className="animate-spin" />
              <span>Checking GitHub authentication…</span>
            </div>
          ) : !status?.authenticated ? (
            <div className="space-y-3">
              <div
                className="rounded-[10px] border p-3.5 space-y-2.5"
                style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)' }}
              >
                <div className="text-[12.5px] font-medium" style={{ color: 'var(--text-main)' }}>
                  Connect your GitHub account
                </div>
                <p className="text-[11.5px] m-0 leading-relaxed" style={{ color: 'var(--text-muted)' }}>
                  Sign in with a GitHub Personal Access Token (<code>repo</code> + <code>read:user</code>) or authenticate with <code>gh auth login</code>. Sakana will automatically detect your repositories so you can clone &amp; work on them in one click.
                </p>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() =>
                      void tauriApi.openExternalUrl(
                        'https://github.com/settings/tokens/new?scopes=repo,read:user&description=Sakana%20Desktop',
                      )
                    }
                    className="btn-ghost px-2.5 py-1.5 text-[11.5px] font-medium inline-flex items-center gap-1.5 cursor-pointer"
                  >
                    <ArrowSquareOut size={12} />
                    <span>Create Token on GitHub</span>
                  </button>
                  {status?.ghInstalled && (
                    <button
                      type="button"
                      onClick={() => void refreshStatusAndRepos()}
                      className="btn-ghost px-2.5 py-1.5 text-[11.5px] font-medium inline-flex items-center gap-1.5 cursor-pointer"
                    >
                      <GithubLogo size={12} />
                      <span>Re-check gh CLI</span>
                    </button>
                  )}
                </div>
                <div className="flex items-center gap-1.5 pt-1">
                  <div className="relative flex-1">
                    <Key
                      size={12}
                      className="absolute left-2.5 top-1/2 -translate-y-1/2"
                      style={{ color: 'var(--text-muted)' }}
                    />
                    <input
                      type="password"
                      value={tokenInput}
                      onChange={(e) => setTokenInput(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') void handleSaveToken();
                      }}
                      placeholder="ghp_xxxxxxxxxxxxxxxxxxxx"
                      className="w-full rounded-[7px] border pl-8 pr-2.5 py-1.5 font-mono-tech text-[11.5px] focus:outline-none"
                      style={{
                        backgroundColor: 'var(--bg-card)',
                        borderColor: 'var(--border-line)',
                        color: 'var(--text-main)',
                      }}
                    />
                  </div>
                  <button
                    type="button"
                    disabled={savingToken || !tokenInput.trim()}
                    onClick={() => void handleSaveToken()}
                    className="btn-solid px-3 py-1.5 text-[11.5px] font-medium cursor-pointer disabled:opacity-50"
                  >
                    {savingToken ? 'Connecting…' : 'Connect'}
                  </button>
                </div>
                {authError && (
                  <div className="text-[11px] font-mono-tech" style={{ color: 'var(--accent-red)' }}>
                    {authError}
                  </div>
                )}
              </div>
            </div>
          ) : (
            <>
              {/* Clone Destination Bar */}
              <div className="flex items-center gap-1.5">
                <div className="flex-1 min-w-0">
                  <label className="block text-[10px] font-mono-tech mb-1" style={{ color: 'var(--text-muted)' }}>
                    Clone destination folder
                  </label>
                  <div className="flex items-center gap-1.5">
                    <input
                      value={targetDir}
                      onChange={(e) => setTargetDir(e.target.value)}
                      placeholder="C:\Users\...\Projects"
                      className="flex-1 min-w-0 rounded-[7px] border px-2.5 py-1.5 font-mono-tech text-[11px] focus:outline-none"
                      style={{
                        backgroundColor: 'var(--bg-app)',
                        borderColor: 'var(--border-line)',
                        color: 'var(--text-main)',
                      }}
                    />
                    <button
                      type="button"
                      onClick={() => void handlePickTargetDir()}
                      className="btn-ghost px-2.5 py-1.5 text-[11px] inline-flex items-center gap-1 cursor-pointer shrink-0"
                      title="Browse folder"
                    >
                      <FolderOpen size={12} />
                      <span>Browse</span>
                    </button>
                  </div>
                </div>
                {status.source === 'token' && (
                  <button
                    type="button"
                    onClick={() => void handleDisconnect()}
                    className="btn-ghost px-2 py-1.5 text-[11px] inline-flex items-center gap-1 cursor-pointer self-end"
                    title="Disconnect GitHub token"
                    style={{ color: 'var(--text-muted)' }}
                  >
                    <SignOut size={12} />
                  </button>
                )}
              </div>

              {/* Search repos */}
              <div className="relative">
                <MagnifyingGlass
                  size={13}
                  className="absolute left-2.5 top-1/2 -translate-y-1/2"
                  style={{ color: 'var(--text-muted)' }}
                />
                <input
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder={`Search ${repos.length} repositories…`}
                  className="w-full rounded-[7px] border pl-8 pr-2.5 py-1.5 text-[12px] focus:outline-none"
                  style={{
                    backgroundColor: 'var(--bg-app)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-main)',
                  }}
                  autoFocus
                />
              </div>

              {cloneError && (
                <div
                  className="rounded-[8px] border px-3 py-2 text-[11px] font-mono-tech"
                  style={{
                    backgroundColor: 'var(--accent-red-bg)',
                    borderColor: 'var(--accent-red)',
                    color: 'var(--accent-red)',
                  }}
                >
                  {cloneError}
                </div>
              )}

              {/* Repo list */}
              {loadingRepos ? (
                <div className="py-8 flex items-center justify-center gap-2 text-[12px]" style={{ color: 'var(--text-muted)' }}>
                  <SpinnerGap size={14} className="animate-spin" />
                  <span>Loading repositories…</span>
                </div>
              ) : filteredRepos.length === 0 ? (
                <div className="py-8 text-center text-[12px]" style={{ color: 'var(--text-muted)' }}>
                  No repositories found.
                </div>
              ) : (
                <div className="space-y-1.5">
                  {filteredRepos.map((repo) => {
                    const isCloning = cloningId === repo.id;
                    return (
                      <div
                        key={repo.id}
                        className="rounded-[9px] border px-3 py-2.5 flex items-center justify-between gap-3 hover:bg-[var(--bg-hover)] transition-colors"
                        style={{ borderColor: 'var(--border-line)' }}
                      >
                        <div className="min-w-0 flex-1 space-y-0.5">
                          <div className="flex items-center gap-1.5 min-w-0">
                            <span className="text-[12.5px] font-medium truncate" style={{ color: 'var(--text-main)' }}>
                              {repo.fullName}
                            </span>
                            {repo.private && (
                              <span
                                className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-[4px] font-mono-tech text-[9px] shrink-0"
                                style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }}
                              >
                                <Lock size={9} />
                                Private
                              </span>
                            )}
                          </div>
                          {repo.description && (
                            <p className="text-[11px] m-0 truncate" style={{ color: 'var(--text-muted)' }}>
                              {repo.description}
                            </p>
                          )}
                          <div className="flex items-center gap-3 font-mono-tech text-[10px]" style={{ color: 'var(--text-muted)' }}>
                            {repo.language && <span>{repo.language}</span>}
                            <span className="inline-flex items-center gap-0.5">
                              <GitBranch size={10} />
                              {repo.defaultBranch}
                            </span>
                            {repo.stargazersCount > 0 && (
                              <span className="inline-flex items-center gap-0.5">
                                <Star size={10} />
                                {repo.stargazersCount}
                              </span>
                            )}
                          </div>
                        </div>
                        <button
                          type="button"
                          disabled={cloningId !== null}
                          onClick={() => void handleCloneRepo(repo)}
                          className="btn-solid px-2.5 py-1.5 text-[11px] font-medium inline-flex items-center gap-1.5 cursor-pointer shrink-0 disabled:opacity-50"
                        >
                          {isCloning ? (
                            <>
                              <SpinnerGap size={12} className="animate-spin" />
                              <span>Cloning…</span>
                            </>
                          ) : (
                            <span>Clone &amp; Open</span>
                          )}
                        </button>
                      </div>
                    );
                  })}
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
