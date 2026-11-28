import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ArrowClockwise,
  ArrowLeft,
  ArrowRight,
  ArrowSquareOut,
  Desktop,
  DeviceMobile,
  DeviceTablet,
  Globe,
  X,
} from '@phosphor-icons/react';
import { isTauriRuntime, tauriApi } from '../lib/tauriApi';

export interface NativeBrowserCommand {
  seq: number;
  kind: 'navigate' | 'back' | 'forward' | 'reload' | 'hide' | 'show' | 'close' | 'eval';
  url?: string;
  script?: string;
}

interface NativeBrowserPaneProps {
  visible: boolean;
  overlayBlocked?: boolean;
  initialUrl?: string;
  command?: NativeBrowserCommand | null;
  onToast: (msg: string) => void;
}

function normalizeDisplayUrl(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return 'http://localhost:5173';
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(trimmed)) return trimmed;
  if (
    trimmed.startsWith('localhost') ||
    trimmed.startsWith('127.0.0.1') ||
    trimmed.startsWith('[::1]') ||
    /^\d{1,3}(\.\d{1,3}){3}(:\d+)?/.test(trimmed)
  ) {
    return `http://${trimmed}`;
  }
  return `https://${trimmed}`;
}

export function NativeBrowserPane({
  visible,
  overlayBlocked = false,
  initialUrl = 'http://localhost:5173',
  command,
  onToast,
}: NativeBrowserPaneProps) {
  const [urlInput, setUrlInput] = useState(initialUrl);
  const [currentUrl, setCurrentUrl] = useState(initialUrl);
  const [viewport, setViewport] = useState<'desktop' | 'tablet' | 'mobile'>('desktop');
  const [isOpen, setIsOpen] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [iframeKey, setIframeKey] = useState(0);

  const slotRef = useRef<HTMLDivElement | null>(null);
  const lastCmdSeqRef = useRef<number>(0);
  const isNative = isTauriRuntime();
  const shouldShowNative = isNative && visible && !overlayBlocked;

  const getSlotRect = useCallback(() => {
    const el = slotRef.current;
    if (!el) return null;
    const rect = el.getBoundingClientRect();
    if (rect.width < 20 || rect.height < 20) return null;
    return {
      x: Math.round(rect.left),
      y: Math.round(rect.top),
      width: Math.round(rect.width),
      height: Math.round(rect.height),
    };
  }, []);

  const openOrNavigate = useCallback(
    async (targetUrl: string) => {
      const normalized = normalizeDisplayUrl(targetUrl);
      setUrlInput(normalized);
      setCurrentUrl(normalized);
      setErrorMsg(null);

      if (!isNative) {
        setIframeKey((k) => k + 1);
        setIsOpen(true);
        return;
      }

      const rect = getSlotRect() ?? { x: 400, y: 120, width: 640, height: 480 };
      try {
        const state = await tauriApi.browserOpenOrNavigate(
          normalized,
          rect.x,
          rect.y,
          rect.width,
          rect.height,
        );
        if (state) {
          setIsOpen(state.open);
          setCurrentUrl(state.url);
          setUrlInput(state.url);
        } else {
          setIsOpen(true);
        }
        if (!shouldShowNative) {
          await tauriApi.browserSetVisible(false);
        }
      } catch (err) {
        setErrorMsg(String(err));
        onToast(`Browser error: ${String(err)}`);
      }
    },
    [getSlotRect, isNative, onToast, shouldShowNative],
  );

  // Sync visibility & bounds whenever visible, overlayBlocked, or viewport changes
  useEffect(() => {
    if (!isNative) return;
    if (!shouldShowNative) {
      void tauriApi.browserSetVisible(false);
      return;
    }
    const syncBounds = () => {
      const rect = getSlotRect();
      if (!rect) return;
      if (!isOpen) {
        void openOrNavigate(currentUrl);
      } else {
        void tauriApi.browserSetBounds(rect.x, rect.y, rect.width, rect.height);
        void tauriApi.browserSetVisible(true);
      }
    };

    const raf = requestAnimationFrame(syncBounds);
    const el = slotRef.current;
    let ro: ResizeObserver | null = null;
    if (el && typeof ResizeObserver !== 'undefined') {
      ro = new ResizeObserver(() => {
        const rect = getSlotRect();
        if (rect && shouldShowNative) {
          void tauriApi.browserSetBounds(rect.x, rect.y, rect.width, rect.height);
        }
      });
      ro.observe(el);
    }
    window.addEventListener('resize', syncBounds);
    return () => {
      cancelAnimationFrame(raf);
      ro?.disconnect();
      window.removeEventListener('resize', syncBounds);
    };
  }, [shouldShowNative, isNative, isOpen, viewport, currentUrl, getSlotRect, openOrNavigate]);

  // Hide native webview on unmount so it never orphans on screen
  useEffect(() => {
    return () => {
      if (isTauriRuntime()) {
        void tauriApi.browserSetVisible(false);
      }
    };
  }, []);

  // Listen for navigation events from the native child webview
  useEffect(() => {
    if (!isNative) return;
    let unlisten: (() => void) | null = null;
    void tauriApi
      .onBrowserNavigated((evt) => {
        if (evt?.url) {
          setCurrentUrl(evt.url);
          setUrlInput(evt.url);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => {
      unlisten?.();
    };
  }, [isNative]);

  // Handle imperative commands from AI self-control
  useEffect(() => {
    if (!command || command.seq === lastCmdSeqRef.current) return;
    lastCmdSeqRef.current = command.seq;
    if (command.kind === 'navigate' && command.url) {
      void openOrNavigate(command.url);
    } else if (command.kind === 'back' || command.kind === 'forward' || command.kind === 'reload') {
      if (isNative) {
        void tauriApi.browserNavAction(command.kind);
      } else if (command.kind === 'reload') {
        setIframeKey((k) => k + 1);
      }
    } else if (command.kind === 'hide') {
      if (isNative) void tauriApi.browserSetVisible(false);
    } else if (command.kind === 'show') {
      if (isNative) void tauriApi.browserSetVisible(true);
    } else if (command.kind === 'close') {
      if (isNative) void tauriApi.browserClose();
      setIsOpen(false);
    } else if (command.kind === 'eval' && command.script) {
      if (isNative) void tauriApi.browserEvalJs(command.script);
    }
  }, [command, isNative, openOrNavigate]);

  return (
    <div className="flex flex-col h-full">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void openOrNavigate(urlInput);
        }}
        className="p-2 border-b flex items-center gap-1.5 shrink-0"
        style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-card)' }}
      >
        <button
          type="button"
          onClick={() => isNative && void tauriApi.browserNavAction('back')}
          className="btn-ghost p-1.5 cursor-pointer"
          title="Back"
          aria-label="Browser back"
        >
          <ArrowLeft size={13} />
        </button>
        <button
          type="button"
          onClick={() => isNative && void tauriApi.browserNavAction('forward')}
          className="btn-ghost p-1.5 cursor-pointer"
          title="Forward"
          aria-label="Browser forward"
        >
          <ArrowRight size={13} />
        </button>
        <button
          type="button"
          onClick={() => {
            if (isNative) {
              void tauriApi.browserNavAction('reload');
            } else {
              setIframeKey((k) => k + 1);
            }
          }}
          className="btn-ghost p-1.5 cursor-pointer"
          title="Reload"
          aria-label="Reload browser"
        >
          <ArrowClockwise size={13} />
        </button>

        <input
          value={urlInput}
          onChange={(e) => setUrlInput(e.target.value)}
          placeholder="https://... or localhost:5173"
          className="flex-1 min-w-0 rounded-[6px] border px-2.5 py-1 font-mono-tech text-[11px]"
          style={{
            backgroundColor: 'var(--bg-app)',
            borderColor: 'var(--border-line)',
            color: 'var(--text-main)',
          }}
        />

        <div
          className="hidden sm:flex items-center p-0.5 rounded-[6px] border shrink-0"
          style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
        >
          <button
            type="button"
            onClick={() => setViewport('desktop')}
            className="p-1 rounded-[4px] cursor-pointer"
            style={viewport === 'desktop' ? { backgroundColor: 'var(--bg-card)' } : undefined}
            title="Desktop"
            aria-label="Desktop viewport"
          >
            <Desktop size={12} />
          </button>
          <button
            type="button"
            onClick={() => setViewport('tablet')}
            className="p-1 rounded-[4px] cursor-pointer"
            style={viewport === 'tablet' ? { backgroundColor: 'var(--bg-card)' } : undefined}
            title="Tablet"
            aria-label="Tablet viewport"
          >
            <DeviceTablet size={12} />
          </button>
          <button
            type="button"
            onClick={() => setViewport('mobile')}
            className="p-1 rounded-[4px] cursor-pointer"
            style={viewport === 'mobile' ? { backgroundColor: 'var(--bg-card)' } : undefined}
            title="Mobile"
            aria-label="Mobile viewport"
          >
            <DeviceMobile size={12} />
          </button>
        </div>

        <button
          type="button"
          onClick={() => void tauriApi.openExternalUrl(currentUrl)}
          className="btn-ghost p-1.5 cursor-pointer shrink-0"
          title="Open in system browser"
          aria-label="Open in system browser"
        >
          <ArrowSquareOut size={13} />
        </button>

        {isNative && isOpen && (
          <button
            type="button"
            onClick={() => {
              void tauriApi.browserClose();
              setIsOpen(false);
            }}
            className="btn-ghost p-1.5 cursor-pointer shrink-0"
            title="Close browser webview"
            aria-label="Close browser webview"
          >
            <X size={13} />
          </button>
        )}
      </form>

      {errorMsg && (
        <div
          className="px-3 py-1.5 border-b text-[11px] font-mono-tech"
          style={{
            borderColor: 'var(--border-line)',
            backgroundColor: 'var(--accent-red-bg)',
            color: 'var(--accent-red)',
          }}
        >
          {errorMsg}
        </div>
      )}

      <div
        className="flex-1 min-h-0 p-2 flex justify-center items-stretch"
        style={{ backgroundColor: 'var(--bg-app)' }}
      >
        <div
          ref={slotRef}
          className="rounded-[8px] border overflow-hidden w-full h-full flex items-center justify-center"
          style={{
            borderColor: 'var(--border-line)',
            backgroundColor: 'var(--bg-canvas)',
            maxWidth:
              viewport === 'mobile' ? '390px' : viewport === 'tablet' ? '640px' : '100%',
          }}
        >
          {isNative ? (
            <div className="flex flex-col items-center gap-2 text-center px-4 select-none">
              <Globe size={20} style={{ color: 'var(--text-muted)' }} />
              <div className="text-[12px] font-medium" style={{ color: 'var(--text-main)' }}>
                {overlayBlocked
                  ? 'Native browser paused while modal is open'
                  : isOpen
                    ? 'Native WebView active'
                    : 'Starting embedded browser...'}
              </div>
              <div className="text-[11px] font-mono-tech truncate max-w-[280px]" style={{ color: 'var(--text-muted)' }}>
                {currentUrl}
              </div>
            </div>
          ) : (
            <iframe
              key={iframeKey}
              src={currentUrl}
              title="Sakana Browser Preview"
              className="w-full h-full border-0 bg-white"
            />
          )}
        </div>
      </div>
    </div>
  );
}
