import { useEffect, useRef, useState } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { ArrowClockwise, Sparkle } from '@phosphor-icons/react';
import { isTauriRuntime, tauriApi } from '../lib/tauriApi';
import type { ThemeId } from '../types';

interface XtermTerminalProps {
  sessionKey: string;
  cwd: string;
  wslDistro?: string;
  theme: ThemeId;
  onToast?: (msg: string) => void;
}

function detectOsAndShell(): { os: string; shell: string } {
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent.toLowerCase() : '';
  if (ua.includes('win')) return { os: 'windows', shell: 'powershell' };
  if (ua.includes('mac')) return { os: 'macos', shell: 'zsh' };
  return { os: 'linux', shell: 'bash' };
}

function fallbackTranslateNl(prompt: string): string {
  const lower = prompt.toLowerCase();
  if (lower.includes('worktree')) return 'git worktree list';
  if (lower.includes('test') || lower.includes('pytest')) return 'pytest tests/ -q';
  if (lower.includes('status') || lower.includes('git')) return 'git status -sb';
  if (lower.includes('branch')) return 'git branch -a';
  if (lower.includes('build')) return 'npm run build';
  return `echo "${prompt.replace(/"/g, '\\"')}"`;
}

const TERMINAL_THEMES: Record<
  ThemeId,
  {
    background: string;
    foreground: string;
    cursor: string;
    selectionBackground: string;
  }
> = {
  paper: {
    background: '#FAFAFA',
    foreground: '#171717',
    cursor: '#1D6FA5',
    selectionBackground: 'rgba(29, 111, 165, 0.22)',
  },
  noir: {
    background: '#0E0E10',
    foreground: '#EDEDEF',
    cursor: '#60A5FA',
    selectionBackground: 'rgba(96, 165, 250, 0.30)',
  },
  slate: {
    background: '#10141D',
    foreground: '#E6EDF3',
    cursor: '#7C9BFF',
    selectionBackground: 'rgba(124, 155, 255, 0.28)',
  },
  sand: {
    background: '#F4F1EB',
    foreground: '#1C1917',
    cursor: '#B45309',
    selectionBackground: 'rgba(180, 83, 9, 0.22)',
  },
};

export function XtermTerminal({ sessionKey, cwd, wslDistro, theme, onToast }: XtermTerminalProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const previewLineRef = useRef('');
  const themeRef = useRef(theme);
  themeRef.current = theme;

  const [nlInput, setNlInput] = useState('');
  const [translating, setTranslating] = useState(false);
  const [restartNonce, setRestartNonce] = useState(0);

  const termTheme = TERMINAL_THEMES[theme] ?? TERMINAL_THEMES.noir;

  useEffect(() => {
    if (termRef.current) {
      termRef.current.options.theme = TERMINAL_THEMES[theme] ?? TERMINAL_THEMES.noir;
    }
  }, [theme]);

  useEffect(() => {
    const container = hostRef.current;
    if (!container) return;

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 12,
      fontFamily: "'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
      theme: TERMINAL_THEMES[themeRef.current] ?? TERMINAL_THEMES.noir,
    });
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(container);
    fitAddon.fit();

    termRef.current = term;
    fitRef.current = fitAddon;

    let disposed = false;
    let unlistenData: (() => void) | null = null;
    let unlistenExit: (() => void) | null = null;

    const setupPty = async () => {
      if (!isTauriRuntime()) {
        term.writeln(`\x1b[34m[sakana]\x1b[0m Browser preview shell (${cwd})`);
        term.write('$ ');
        previewLineRef.current = '';
        term.onData((data) => {
          if (data === '\r') {
            term.writeln('');
            const cmd = previewLineRef.current.trim();
            if (cmd) {
              term.writeln(`\x1b[90m(mock)\x1b[0m executed: ${cmd}`);
            }
            previewLineRef.current = '';
            term.write('$ ');
          } else if (data === '\u007F') {
            if (previewLineRef.current.length > 0) {
              previewLineRef.current = previewLineRef.current.slice(0, -1);
              term.write('\b \b');
            }
          } else {
            previewLineRef.current += data;
            term.write(data);
          }
        });
        return;
      }

      const uData = await tauriApi.onPtyData((ev) => {
        if (!disposed && ev.sessionId === sessionKey) {
          term.write(ev.data);
        }
      });
      if (disposed) {
        uData();
        return;
      }
      unlistenData = uData;

      const uExit = await tauriApi.onPtyExit((ev) => {
        if (!disposed && ev.sessionId === sessionKey) {
          term.writeln(`\r\n\x1b[90m[process exited with code ${ev.code}]\x1b[0m`);
        }
      });
      if (disposed) {
        uExit();
        return;
      }
      unlistenExit = uExit;

      const cols = Math.max(20, term.cols || 80);
      const rows = Math.max(6, term.rows || 24);
      try {
        await tauriApi.ptySpawn(sessionKey, cwd, cols, rows, wslDistro);
        if (disposed) {
          void tauriApi.ptyKill(sessionKey).catch(() => {});
          return;
        }
      } catch (err) {
        if (!disposed) {
          term.writeln(`\x1b[31mFailed to spawn PTY:\x1b[0m ${String(err)}`);
        }
      }
      if (disposed) return;

      term.onData((data) => {
        if (disposed) return;
        void tauriApi.ptyWrite(sessionKey, data).catch((err) => {
          console.error('pty_write failed:', err);
        });
      });

      term.onResize(({ cols: nextCols, rows: nextRows }) => {
        if (disposed) return;
        void tauriApi.ptyResize(sessionKey, nextCols, nextRows).catch(() => {});
      });
    };

    void setupPty();

    const ro = new ResizeObserver(() => {
      try {
        fitAddon.fit();
      } catch {
        // ignore resize errors during unmount
      }
    });
    ro.observe(container);

    return () => {
      disposed = true;
      ro.disconnect();
      unlistenData?.();
      unlistenExit?.();
      if (isTauriRuntime()) {
        void tauriApi.ptyKill(sessionKey).catch(() => {});
      }
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, [sessionKey, cwd, wslDistro, restartNonce]);

  const handleTranslate = async (e: React.FormEvent) => {
    e.preventDefault();
    const raw = nlInput.trim();
    if (!raw) return;
    const prompt = raw.startsWith('?') ? raw.slice(1).trim() : raw;
    if (!prompt) return;

    setTranslating(true);
    try {
      const { os, shell } = detectOsAndShell();
      let cmd: string | null = null;
      if (isTauriRuntime()) {
        cmd = await tauriApi.translateNlCommand(prompt, os, shell);
      }
      const translated = (cmd && cmd.trim()) || fallbackTranslateNl(prompt);
      setNlInput('');
      if (isTauriRuntime()) {
        // Pre-fill without \r so the user reviews and presses Enter to execute
        await tauriApi.ptyWrite(sessionKey, translated);
      } else {
        previewLineRef.current += translated;
        termRef.current?.write(translated);
      }
      termRef.current?.focus();
      onToast?.(`Pre-filled: ${translated} (press Enter to run)`);
    } catch (err) {
      onToast?.(`NL translation failed: ${String(err)}`);
    } finally {
      setTranslating(false);
    }
  };

  return (
    <div className="flex flex-col h-full min-h-0" style={{ backgroundColor: termTheme.background }}>
      <div className="flex-1 min-h-0 p-2 relative">
        <div ref={hostRef} className="w-full h-full" />
        <button
          type="button"
          onClick={() => setRestartNonce((n) => n + 1)}
          title="Restart PTY"
          aria-label="Restart PTY"
          className="btn-ghost absolute top-2 right-2 p-1.5 rounded-[6px] cursor-pointer"
          style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }}
        >
          <ArrowClockwise size={12} />
        </button>
      </div>

      <form
        onSubmit={handleTranslate}
        className="px-2.5 py-2 border-t flex items-center gap-2 shrink-0"
        style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-elevated)' }}
      >
        <Sparkle size={13} className="shrink-0" style={{ color: 'var(--accent-blue)' }} />
        <span className="font-mono-tech text-[11.5px] shrink-0" style={{ color: 'var(--accent-blue)' }}>?</span>
        <input
          value={nlInput}
          onChange={(e) => setNlInput(e.target.value)}
          disabled={translating}
          placeholder='Natural language command (e.g. "find large files")...'
          className="flex-1 bg-transparent font-mono-tech text-[11.5px] focus:outline-none"
          style={{ color: 'var(--text-main)' }}
        />
        <button
          type="submit"
          disabled={translating || !nlInput.trim()}
          className="btn-solid px-2.5 py-1 rounded-[5px] text-[10.5px] font-mono-tech disabled:opacity-40 cursor-pointer"
        >
          {translating ? 'Translating...' : 'Fill'}
        </button>
      </form>
    </div>
  );
}
