import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ArrowClockwise,
  ArrowDown,
  ArrowSquareOut,
  ArrowsOutSimple,
  ArrowUp,
  Eye,
  CaretDown,
  CaretRight,
  Check,
  Crosshair,
  Desktop,
  DeviceMobile,
  DeviceTablet,
  FileCode,
  FolderSimple,
  GitBranch,
  GitDiff,
  GitPullRequest,
  Globe,
  MagnifyingGlass,
  PencilSimple,
  Plus,
  Sparkle,
  TerminalWindow,
  TreeStructure,
  X,
} from '@phosphor-icons/react';
import type {
  AttachedContextChip,
  DiffFile,
  InspectorTab,
  ThemeId,
  Thread,
  TraceSpan,
  WorkspaceFile,
} from '../types';
import { isTauriRuntime, tauriApi } from '../lib/tauriApi';
import { HarnessIcon } from './HarnessBadge';
import { CodeMirrorEditor } from './CodeMirrorEditor';
import { XtermTerminal } from './XtermTerminal';
import { SimpleMarkdown } from './ChatPane';
import { NativeBrowserPane, type NativeBrowserCommand } from './NativeBrowserPane';

interface RightInspectorProps {
  open: boolean;
  width?: number;
  expanded: boolean;
  onToggleExpand: () => void;
  onClose: () => void;
  activeTab: InspectorTab;
  onSelectTab: (tab: InspectorTab) => void;
  thread: Thread;
  projectCwd?: string;
  wslDistro?: string;
  theme: ThemeId;
  diffs: DiffFile[];
  onRefreshDiffs: (turnId?: string) => Promise<void> | void;
  onToggleStageHunk: (
    filePath: string,
    hunkId: string,
    hunkHeader: string,
    nextStaged: boolean,
  ) => Promise<void> | void;
  onDiscardHunk: (
    filePath: string,
    hunkId: string,
    hunkHeader: string,
  ) => Promise<void> | void;
  onBranchChanged?: (branch: string) => void;
  onOpenCreatePr: () => void;
  turnScopeFilter: string;
  onChangeTurnScope: (turnId: string) => void;
  files: WorkspaceFile[];
  onSaveFile: (path: string, content: string) => Promise<void> | void;
  traces: TraceSpan[];
  onAttachChip: (chip: AttachedContextChip) => void;
  onToast: (msg: string) => void;
  threads?: Thread[];
  onSelectThread?: (id: string) => void;
  requestedFile?: { path: string; seq: number } | null;
  browserOverlayBlocked?: boolean;
  browserCommand?: NativeBrowserCommand | null;
}

interface FileTreeNode {
  name: string;
  path: string;
  isDir: boolean;
  children: FileTreeNode[];
}

const CODE_TOKEN_RE =
  /(\/\/.*$|#.*$|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`|\b\d+(?:\.\d+)?\b|\b(?:import|export|from|const|let|var|function|return|if|else|for|while|match|switch|case|break|continue|async|await|try|catch|finally|throw|new|class|interface|type|enum|pub|fn|struct|impl|use|mod|crate|def|elif|pass|None|True|False|null|undefined|true|false)\b)/g;

function highlightCodeLine(line: string): React.ReactNode {
  if (!line) return ' ';
  const parts: React.ReactNode[] = [];
  let lastIndex = 0;
  CODE_TOKEN_RE.lastIndex = 0;
  let match: RegExpExecArray | null;
  let idx = 0;
  while ((match = CODE_TOKEN_RE.exec(line)) !== null) {
    if (match.index > lastIndex) {
      parts.push(line.slice(lastIndex, match.index));
    }
    const token = match[0];
    let color = 'var(--text-main)';
    let fontStyle: 'italic' | undefined;
    if (token.startsWith('//') || token.startsWith('#')) {
      color = 'var(--text-muted)';
      fontStyle = 'italic';
    } else if (token.startsWith('"') || token.startsWith("'") || token.startsWith('`')) {
      color = 'var(--accent-green)';
    } else if (/^\d/.test(token)) {
      color = 'var(--accent-amber)';
    } else {
      color = 'var(--accent-blue)';
    }
    parts.push(
      <span key={idx++} style={{ color, fontStyle }}>
        {token}
      </span>,
    );
    lastIndex = CODE_TOKEN_RE.lastIndex;
  }
  if (lastIndex < line.length) {
    parts.push(line.slice(lastIndex));
  }
  return parts.length > 0 ? parts : line;
}

function buildFileTree(paths: string[]): FileTreeNode[] {
  const root: FileTreeNode[] = [];
  const dirMap = new Map<string, FileTreeNode>();

  const ensureDir = (dirPath: string, name: string, parentChildren: FileTreeNode[]): FileTreeNode => {
    const existing = dirMap.get(dirPath);
    if (existing) return existing;
    const node: FileTreeNode = { name, path: dirPath, isDir: true, children: [] };
    dirMap.set(dirPath, node);
    parentChildren.push(node);
    return node;
  };

  for (const rawPath of paths) {
    const normalized = rawPath.replace(/\\/g, '/');
    const parts = normalized.split('/').filter(Boolean);
    if (parts.length === 0) continue;

    let currentChildren = root;
    let accum = '';
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      accum = accum ? `${accum}/${part}` : part;
      const isLast = i === parts.length - 1;
      if (isLast) {
        currentChildren.push({
          name: part,
          path: rawPath,
          isDir: false,
          children: [],
        });
      } else {
        const dirNode = ensureDir(accum, part, currentChildren);
        currentChildren = dirNode.children;
      }
    }
  }

  const sortNodes = (nodes: FileTreeNode[]) => {
    nodes.sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    for (const n of nodes) {
      if (n.isDir) sortNodes(n.children);
    }
  };
  sortNodes(root);
  return root;
}

function TraceSpanNode({ span, depth = 0 }: { span: TraceSpan; depth?: number }) {
  return (
    <div
      className={depth > 0 ? 'pl-3 border-l space-y-2 pt-1' : 'sakana-card rounded-[10px] p-3.5 space-y-2.5'}
      style={{ borderColor: 'var(--border-line)' }}
    >
      <div className="flex items-center justify-between gap-2">
        <span
          className="inline-flex items-center gap-1.5 min-w-0 truncate"
          style={{
            color: depth === 0 ? 'var(--text-main)' : 'var(--text-body)',
            fontSize: depth === 0 ? '12.5px' : '11.5px',
            fontWeight: depth === 0 ? 500 : 400,
          }}
        >
          <HarnessIcon harness={span.harness} size={depth === 0 ? 13 : 11} />
          <span className={depth > 0 ? 'font-mono-tech' : ''}>{span.actor}</span>
          {depth > 0 && <span className="truncate opacity-75">{span.action}</span>}
        </span>
        <span className="font-mono-tech text-[10.5px] shrink-0" style={{ color: 'var(--text-muted)' }}>
          {span.durationMs >= 1000
            ? `${(span.durationMs / 1000).toFixed(2)}s`
            : `${span.durationMs}ms`}
          {span.tokens ? ` · ${span.tokens}` : ''}
        </span>
      </div>
      {depth === 0 && (
        <div className="text-[12px]" style={{ color: 'var(--text-body)' }}>
          {span.action}
        </div>
      )}
      {span.children && span.children.length > 0 && (
        <div className="space-y-1.5">
          {span.children.map((ch) => (
            <TraceSpanNode key={ch.id} span={ch} depth={depth + 1} />
          ))}
        </div>
      )}
    </div>
  );
}

export function RightInspector({
  open,
  width,
  expanded,
  onToggleExpand,
  onClose,
  activeTab,
  onSelectTab,
  thread,
  projectCwd,
  wslDistro,
  theme,
  diffs,
  onRefreshDiffs,
  onToggleStageHunk,
  onDiscardHunk,
  onBranchChanged,
  onOpenCreatePr,
  turnScopeFilter,
  onChangeTurnScope,
  files,
  onSaveFile,
  traces,
  onAttachChip,
  onToast,
  threads = [],
  onSelectThread,
  requestedFile,
  browserOverlayBlocked = false,
  browserCommand = null,
}: RightInspectorProps) {
  const effectiveCwd =
    thread.worktreePath && thread.worktreePath !== '.'
      ? thread.worktreePath
      : projectCwd || '.';

  // Dynamic open tabs + launcher state
  const [openTabs, setOpenTabs] = useState<InspectorTab[]>(() => {
    const valid: InspectorTab[] = ['changes', 'files', 'preview', 'terminal', 'traces'];
    if (typeof window === 'undefined') return ['changes'];
    try {
      const raw = window.localStorage.getItem('sakana_inspector_tabs');
      if (raw === null) return ['changes'];
      const parsed = JSON.parse(raw);
      if (!Array.isArray(parsed)) return ['changes'];
      const filtered = parsed.filter((t): t is InspectorTab => valid.includes(t));
      return Array.from(new Set(filtered));
    } catch {
      return ['changes'];
    }
  });
  const [launcherOpen, setLauncherOpen] = useState<boolean>(() => openTabs.length === 0);
  const [launcherQuery, setLauncherQuery] = useState('');
  const prevActiveTabRef = useRef<InspectorTab>(activeTab);

  useEffect(() => {
    try {
      window.localStorage.setItem('sakana_inspector_tabs', JSON.stringify(openTabs));
    } catch {
      // ignore storage errors
    }
  }, [openTabs]);

  useEffect(() => {
    if (activeTab !== prevActiveTabRef.current) {
      prevActiveTabRef.current = activeTab;
      setOpenTabs((prev) => (prev.includes(activeTab) ? prev : [...prev, activeTab]));
      setLauncherOpen(false);
    }
  }, [activeTab]);

  const showLauncher = launcherOpen || openTabs.length === 0;

  const openAndFocusTab = useCallback(
    (tab: InspectorTab) => {
      setOpenTabs((prev) => (prev.includes(tab) ? prev : [...prev, tab]));
      setLauncherOpen(false);
      prevActiveTabRef.current = tab;
      onSelectTab(tab);
    },
    [onSelectTab],
  );

  const handleCloseTab = useCallback(
    (tab: InspectorTab) => {
      setOpenTabs((prev) => {
        const idx = prev.indexOf(tab);
        if (idx === -1) return prev;
        const next = prev.filter((t) => t !== tab);
        if (next.length === 0) {
          setLauncherOpen(true);
        } else if (activeTab === tab && !launcherOpen) {
          const fallback = next[Math.min(idx, next.length - 1)];
          prevActiveTabRef.current = fallback;
          onSelectTab(fallback);
        }
        return next;
      });
    },
    [activeTab, launcherOpen, onSelectTab],
  );

  // Changes state
  const [diffMode, setDiffMode] = useState<'unified' | 'split'>('unified');
  const [commentTarget, setCommentTarget] = useState<{ file: string; line: number } | null>(null);
  const [commentText, setCommentText] = useState('');
  const [commitMsg, setCommitMsg] = useState('');
  const [branches, setBranches] = useState<string[]>([]);
  const [gitBusy, setGitBusy] = useState(false);

  // Files state
  const [workspacePaths, setWorkspacePaths] = useState<string[]>([]);
  const [fileFilter, setFileFilter] = useState('');
  const [expandedDirs, setExpandedDirs] = useState<Record<string, boolean>>({});
  const [selectedFilePath, setSelectedFilePath] = useState(files[0]?.path || '');
  const selectedFilePathRef = useRef(selectedFilePath);
  selectedFilePathRef.current = selectedFilePath;
  const [loadedFile, setLoadedFile] = useState<WorkspaceFile | null>(null);
  const [loadingFile, setLoadingFile] = useState(false);
  const [editingFile, setEditingFile] = useState(false);
  const [previewMarkdown, setPreviewMarkdown] = useState(true);
  const [editDraft, setEditDraft] = useState('');
  const [savingFile, setSavingFile] = useState(false);
  const [lineSelection, setLineSelection] = useState<{ start: number; end: number } | null>(null);

  // Preview + Design Mode state
  const [previewInput, setPreviewInput] = useState('http://localhost:5173');
  const [previewUrl, setPreviewUrl] = useState('http://localhost:5173');
  const [previewKey, setPreviewKey] = useState(0);
  const [viewport, setViewport] = useState<'desktop' | 'tablet' | 'mobile'>('desktop');
  const [designMode, setDesignMode] = useState(false);
  const [hoveredEl, setHoveredEl] = useState<{ tag: string; cls: string; size: string } | null>(null);
  const iframeRef = useRef<HTMLIFrameElement | null>(null);

  // Terminal state
  const [termTab, setTermTab] = useState<'wt' | 'host'>('wt');

  // Traces state
  const [liveTraces, setLiveTraces] = useState<TraceSpan[]>(traces);
  const [loadingTraces, setLoadingTraces] = useState(false);

  // Reset stale file selection when switching workspaces/projects
  const workspaceKey = `${effectiveCwd}|${wslDistro || ''}`;
  const prevWorkspaceKeyRef = useRef(workspaceKey);
  useEffect(() => {
    if (prevWorkspaceKeyRef.current !== workspaceKey) {
      prevWorkspaceKeyRef.current = workspaceKey;
      selectedFilePathRef.current = '';
      setSelectedFilePath('');
      setLoadedFile(null);
      setEditingFile(false);
      setLineSelection(null);
    }
  }, [workspaceKey]);

  const branchesReqRef = useRef(0);
  const filesReqRef = useRef(0);
  const tracesReqRef = useRef(0);

  // Sync branches when Changes tab is active
  const loadBranches = useCallback(async () => {
    const reqId = ++branchesReqRef.current;
    if (!isTauriRuntime()) {
      setBranches(thread.branch ? [thread.branch] : []);
      return;
    }
    try {
      const list = await tauriApi.listBranches(effectiveCwd, wslDistro);
      if (reqId !== branchesReqRef.current) return;
      if (list && list.length > 0) {
        setBranches(list);
      } else {
        setBranches(thread.branch ? [thread.branch] : []);
      }
    } catch {
      if (reqId === branchesReqRef.current) {
        setBranches(thread.branch ? [thread.branch] : []);
      }
    }
  }, [effectiveCwd, wslDistro, thread.branch]);

  useEffect(() => {
    if (open && activeTab === 'changes') {
      void loadBranches();
    }
  }, [open, activeTab, loadBranches]);

  // Sync workspace file list when Files tab is active
  const loadWorkspaceFiles = useCallback(async () => {
    const reqId = ++filesReqRef.current;
    if (!isTauriRuntime()) {
      const mockPaths = files.map((f) => f.path);
      setWorkspacePaths(mockPaths);
      if (!selectedFilePathRef.current && mockPaths[0]) {
        setSelectedFilePath(mockPaths[0]);
      }
      return;
    }
    try {
      const list = await tauriApi.listWorkspaceFiles(effectiveCwd, wslDistro);
      if (reqId !== filesReqRef.current) return;
      if (list) {
        setWorkspacePaths(list);
        if (!selectedFilePathRef.current && list[0]) {
          selectedFilePathRef.current = list[0];
          setSelectedFilePath(list[0]);
        }
      }
    } catch (err) {
      if (reqId === filesReqRef.current) {
        onToast(`Failed to list files: ${String(err)}`);
      }
    }
  }, [effectiveCwd, wslDistro, files, onToast]);

  useEffect(() => {
    if (open && (activeTab === 'files' || showLauncher)) {
      void loadWorkspaceFiles();
    }
  }, [open, activeTab, showLauncher, loadWorkspaceFiles]);

  // Open file requested from chat links or tool calls
  useEffect(() => {
    if (!requestedFile?.path) return;
    const raw = requestedFile.path
      .trim()
      .replace(/^file:\/\//i, '')
      .replace(/^\.[\/]/, '')
      .replace(/\\/g, '/');
    const withoutLine = raw.replace(/:\d+(?::\d+)?$/, '');
    const normalizedCwd = effectiveCwd.replace(/\\/g, '/').replace(/\/+$/, '');
    const rel =
      normalizedCwd && withoutLine.startsWith(normalizedCwd + '/')
        ? withoutLine.slice(normalizedCwd.length + 1)
        : withoutLine;
    selectedFilePathRef.current = rel;
    setSelectedFilePath(rel);
    setEditingFile(false);
    setLineSelection(null);
    if (/\.(md|markdown)$/i.test(rel)) {
      setPreviewMarkdown(true);
    }
    const parts = rel.split('/');
    if (parts.length > 1) {
      setExpandedDirs((prev) => {
        const next = { ...prev };
        let acc = '';
        for (let i = 0; i < parts.length - 1; i++) {
          acc = acc ? `${acc}/${parts[i]}` : parts[i];
          next[acc] = true;
        }
        return next;
      });
    }
    void loadWorkspaceFiles();
  }, [requestedFile, effectiveCwd, loadWorkspaceFiles]);

  // Load file contents when selectedFilePath changes
  useEffect(() => {
    if (!open || activeTab !== 'files' || !selectedFilePath) return;
    if (!isTauriRuntime()) {
      const found = files.find((f) => f.path === selectedFilePath) || files[0] || null;
      setLoadedFile(found);
      return;
    }
    let cancelled = false;
    setLoadingFile(true);
    setLoadedFile((prev) => (prev && prev.path === selectedFilePath ? prev : null));
    void tauriApi
      .readWorkspaceFile(effectiveCwd, selectedFilePath, wslDistro)
      .then((wf) => {
        if (!cancelled && wf) setLoadedFile(wf);
      })
      .catch((err) => {
        if (!cancelled) {
          setLoadedFile(null);
          onToast(`Failed to read ${selectedFilePath}: ${String(err)}`);
        }
      })
      .finally(() => {
        if (!cancelled) setLoadingFile(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, activeTab, selectedFilePath, effectiveCwd, wslDistro, files, onToast]);

  // Load traces when Traces tab is active
  const loadTraces = useCallback(async () => {
    const reqId = ++tracesReqRef.current;
    if (!isTauriRuntime()) {
      setLiveTraces(traces);
      return;
    }
    setLoadingTraces(true);
    try {
      const list = await tauriApi.listTraceSpans(thread.id);
      if (reqId === tracesReqRef.current) {
        setLiveTraces(list ?? []);
      }
    } catch {
      if (reqId === tracesReqRef.current) {
        setLiveTraces([]);
      }
    } finally {
      if (reqId === tracesReqRef.current) {
        setLoadingTraces(false);
      }
    }
  }, [thread.id, traces]);

  useEffect(() => {
    if (open && activeTab === 'traces') {
      void loadTraces();
    }
  }, [open, activeTab, loadTraces]);

  // Same-origin iframe Design Mode listener
  useEffect(() => {
    if (!designMode || activeTab !== 'preview') return;
    const iframe = iframeRef.current;
    if (!iframe) return;

    let doc: Document | null = null;
    try {
      doc = iframe.contentDocument;
    } catch {
      doc = null;
    }
    if (!doc) return;

    const onOver = (e: MouseEvent) => {
      const el = e.target as HTMLElement | null;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const cls =
        typeof el.className === 'string' && el.className.trim()
          ? el.className.trim().split(/\s+/)[0]
          : 'element';
      setHoveredEl({
        tag: el.tagName.toLowerCase(),
        cls,
        size: `${Math.round(rect.width)}×${Math.round(rect.height)}px`,
      });
    };

    const onClick = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const el = e.target as HTMLElement | null;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const cls =
        typeof el.className === 'string' && el.className.trim()
          ? el.className.trim().split(/\s+/)[0]
          : 'element';
      const tag = el.tagName.toLowerCase();
      const size = `${Math.round(rect.width)}×${Math.round(rect.height)}px`;
      onAttachChip({
        id: `dm-${Date.now()}`,
        kind: 'design-element',
        label: `<${tag}.${cls}> (${size})`,
        payload: `${previewUrl}: <${tag} class="${el.className || ''}"> ${el.textContent?.trim().slice(0, 80) || ''}`,
      });
      setDesignMode(false);
      setHoveredEl(null);
    };

    doc.addEventListener('mouseover', onOver, true);
    doc.addEventListener('click', onClick, true);
    return () => {
      doc?.removeEventListener('mouseover', onOver, true);
      doc?.removeEventListener('click', onClick, true);
    };
  }, [designMode, activeTab, previewUrl, previewKey, onAttachChip]);

  const filteredPaths = useMemo(() => {
    const q = fileFilter.trim().toLowerCase();
    const source = workspacePaths.length > 0 ? workspacePaths : files.map((f) => f.path);
    if (!q) return source;
    return source.filter((p) => p.toLowerCase().includes(q));
  }, [workspacePaths, files, fileFilter]);

  const fileTree = useMemo(() => buildFileTree(filteredPaths), [filteredPaths]);

  const launcherFilteredFiles = useMemo(() => {
    const q = launcherQuery.trim().toLowerCase();
    if (!q) return [];
    const source = workspacePaths.length > 0 ? workspacePaths : files.map((f) => f.path);
    return source.filter((p) => p.toLowerCase().includes(q)).slice(0, 10);
  }, [workspacePaths, files, launcherQuery]);

  const activeThreads = useMemo(
    () => threads.filter((t) => t.status === 'RUNNING' || t.status === 'AWAITING_APPROVAL'),
    [threads],
  );

  if (!open) return null;

  const tabs: { id: InspectorTab; label: string; icon: React.ReactNode }[] = [
    { id: 'changes', label: 'Changes', icon: <GitDiff size={13} /> },
    { id: 'files', label: 'Files', icon: <FileCode size={13} /> },
    { id: 'preview', label: 'Preview', icon: <Globe size={13} /> },
    { id: 'terminal', label: 'Terminal', icon: <TerminalWindow size={13} /> },
    { id: 'traces', label: 'Traces', icon: <TreeStructure size={13} /> },
  ];

  const checkpoints = thread.messages.filter((m) => m.checkpoint).map((m) => m.checkpoint!);
  const visibleDiffs =
    turnScopeFilter === 'all'
      ? diffs
      : diffs.some((d) => d.turnId === turnScopeFilter)
        ? diffs.filter((d) => d.turnId === turnScopeFilter)
        : diffs;

  const activeFile = isTauriRuntime()
    ? loadedFile
    : loadedFile || files.find((f) => f.path === selectedFilePath) || files[0];

  const handleLineClick = (lineNo: number, shiftKey: boolean) => {
    if (shiftKey && lineSelection) {
      setLineSelection({
        start: Math.min(lineSelection.start, lineNo),
        end: Math.max(lineSelection.start, lineNo),
      });
    } else {
      setLineSelection({ start: lineNo, end: lineNo });
    }
  };

  const handleCheckoutBranch = async (nextBranch: string) => {
    if (!nextBranch || nextBranch === thread.branch) return;
    setGitBusy(true);
    try {
      await tauriApi.checkoutBranch(effectiveCwd, nextBranch, wslDistro);
      onBranchChanged?.(nextBranch);
      await onRefreshDiffs(turnScopeFilter);
      onToast(`Checked out ${nextBranch}`);
    } catch (err) {
      onToast(`Checkout failed: ${String(err)}`);
    } finally {
      setGitBusy(false);
    }
  };

  const handlePull = async () => {
    setGitBusy(true);
    try {
      const res = await tauriApi.gitPull(effectiveCwd, wslDistro);
      await onRefreshDiffs(turnScopeFilter);
      onToast(res ? `Pull: ${res.split('\n')[0]}` : `Pulled ${thread.branch}`);
    } catch (err) {
      onToast(`Pull failed: ${String(err)}`);
    } finally {
      setGitBusy(false);
    }
  };

  const handlePush = async () => {
    setGitBusy(true);
    try {
      const res = await tauriApi.gitPush(effectiveCwd, wslDistro);
      onToast(res ? `Push: ${res.split('\n')[0]}` : `Pushed ${thread.branch}`);
    } catch (err) {
      onToast(`Push failed: ${String(err)}`);
    } finally {
      setGitBusy(false);
    }
  };

  const handleDraftCommit = async () => {
    setGitBusy(true);
    try {
      const msg = await tauriApi.draftCommitMessage(effectiveCwd, wslDistro);
      if (msg) {
        setCommitMsg(msg);
        onToast('Drafted AI commit message');
      } else {
        onToast('No uncommitted changes to draft');
      }
    } catch (err) {
      onToast(`Draft failed: ${String(err)}`);
    } finally {
      setGitBusy(false);
    }
  };

  const handleCommit = async () => {
    const msg = commitMsg.trim();
    if (!msg) return;
    setGitBusy(true);
    try {
      await tauriApi.gitCommit(effectiveCwd, msg, wslDistro);
      setCommitMsg('');
      await onRefreshDiffs(turnScopeFilter);
      onToast(`Committed to ${thread.branch}`);
    } catch (err) {
      onToast(`Commit failed: ${String(err)}`);
    } finally {
      setGitBusy(false);
    }
  };

  const handleSaveActiveFile = async () => {
    if (!activeFile || savingFile) return;
    setSavingFile(true);
    try {
      await onSaveFile(activeFile.path, editDraft);
      setLoadedFile({
        ...activeFile,
        lines: editDraft.split('\n'),
      });
      setEditingFile(false);
    } catch (err) {
      onToast(`Save failed: ${String(err)}`);
    } finally {
      setSavingFile(false);
    }
  };

  const compactTabs = !expanded && (width ?? 440) < 430;
  const resolvedWidth = expanded
    ? typeof window !== 'undefined'
      ? Math.min(680, Math.max(480, Math.round(window.innerWidth * 0.48)))
      : 640
    : Math.max(360, width ?? 440);

  return (
    <aside
      style={{
        width: resolvedWidth,
        backgroundColor: 'var(--bg-sidebar)',
        borderColor: 'var(--border-line)',
      }}
      className="drawer-right-enter h-full min-w-[360px] shrink-0 border-l flex flex-col overflow-hidden"
    >
      {/* Inspector Tab Bar */}
      <div
        className="h-11 px-2 border-b flex items-center justify-between gap-1 shrink-0"
        style={{ borderColor: 'var(--border-line)' }}
      >
        <div className="flex items-center h-full overflow-x-auto no-scrollbar gap-1 min-w-0">
          {openTabs.map((tabId) => {
            const t = tabs.find((item) => item.id === tabId);
            if (!t) return null;
            const active = !showLauncher && activeTab === t.id;
            return (
              <div
                key={t.id}
                role="tab"
                aria-selected={active}
                tabIndex={0}
                onClick={() => {
                  setLauncherOpen(false);
                  prevActiveTabRef.current = t.id;
                  onSelectTab(t.id);
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    setLauncherOpen(false);
                    prevActiveTabRef.current = t.id;
                    onSelectTab(t.id);
                  }
                }}
                title={t.label}
                className="group pl-2 pr-1 h-7 rounded-[6px] inline-flex items-center gap-1.5 text-[11.5px] font-medium whitespace-nowrap shrink-0 cursor-pointer transition-colors border"
                style={{
                  color: active ? 'var(--text-main)' : 'var(--text-muted)',
                  backgroundColor: active ? 'var(--bg-card)' : 'transparent',
                  borderColor: active ? 'var(--border-strong)' : 'transparent',
                }}
              >
                {t.icon}
                {(!compactTabs || active) && <span>{t.label}</span>}
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    handleCloseTab(t.id);
                  }}
                  className="w-4 h-4 rounded-[4px] inline-flex items-center justify-center opacity-0 group-hover:opacity-100 hover:bg-[var(--bg-hover)] transition-opacity cursor-pointer"
                  style={{ color: 'var(--text-muted)' }}
                  title={`Close ${t.label}`}
                  aria-label={`Close ${t.label}`}
                >
                  <X size={10} />
                </button>
              </div>
            );
          })}
          <button
            type="button"
            onClick={() => setLauncherOpen(true)}
            className="w-7 h-7 rounded-[6px] inline-flex items-center justify-center shrink-0 hover:bg-[var(--bg-hover)] cursor-pointer transition-colors"
            style={{
              color: showLauncher ? 'var(--text-main)' : 'var(--text-muted)',
              backgroundColor: showLauncher ? 'var(--bg-card)' : 'transparent',
            }}
            title="New Tab / Launcher"
            aria-label="New Tab / Launcher"
          >
            <Plus size={13} />
          </button>
        </div>
        <div className="flex items-center gap-0.5 shrink-0">
          <button
            type="button"
            onClick={onToggleExpand}
            className="w-7 h-7 rounded-[6px] flex items-center justify-center hover:bg-[var(--bg-hover)] cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
            title="Expand / Restore Width"
            aria-label="Expand or restore inspector width"
          >
            <ArrowsOutSimple size={13} />
          </button>
          <button
            type="button"
            onClick={onClose}
            className="w-7 h-7 rounded-[6px] flex items-center justify-center hover:bg-[var(--bg-hover)] cursor-pointer"
            style={{ color: 'var(--text-muted)' }}
            title="Close Inspector (Ctrl+I)"
            aria-label="Close Inspector"
          >
            <X size={13} />
          </button>
        </div>
      </div>

      {/* Tab Body */}
      <div className="flex-1 min-h-0 overflow-hidden" style={{ backgroundColor: 'var(--bg-app)' }}>
        {/* 0. LAUNCHER / NEW TAB */}
        {showLauncher && (
          <div className="h-full overflow-y-auto p-3.5 space-y-5">
            <form
              onSubmit={(e) => {
                e.preventDefault();
                const q = launcherQuery.trim();
                if (!q) return;
                const isUrl =
                  /^https?:\/\//i.test(q) ||
                  /^localhost(:\d+)?(\/.*)?$/i.test(q) ||
                  /^127\.0\.0\.1(:\d+)?(\/.*)?$/i.test(q);
                if (isUrl) {
                  const normalized = /^https?:\/\//i.test(q) ? q : `http://${q}`;
                  setPreviewInput(normalized);
                  setPreviewUrl(normalized);
                  setPreviewKey((k) => k + 1);
                  setLauncherQuery('');
                  openAndFocusTab('preview');
                  return;
                }
                if (launcherFilteredFiles.length > 0) {
                  setSelectedFilePath(launcherFilteredFiles[0]);
                  setLauncherQuery('');
                  openAndFocusTab('files');
                }
              }}
              className="relative"
            >
              <MagnifyingGlass
                size={13}
                className="absolute left-2.5 top-1/2 -translate-y-1/2 pointer-events-none"
                style={{ color: 'var(--text-muted)' }}
              />
              <input
                value={launcherQuery}
                onChange={(e) => setLauncherQuery(e.target.value)}
                placeholder="Search files or enter a URL…"
                autoFocus
                className="w-full rounded-[8px] border pl-8 pr-3 py-1.5 text-[12px] focus:outline-none"
                style={{
                  backgroundColor: 'var(--bg-card)',
                  borderColor: 'var(--border-line)',
                  color: 'var(--text-main)',
                }}
              />
            </form>

            {launcherQuery.trim() && launcherFilteredFiles.length > 0 && (
              <div className="space-y-1">
                <div
                  className="px-1 text-[10.5px] font-medium uppercase tracking-wider"
                  style={{ color: 'var(--text-muted)' }}
                >
                  Matching Files
                </div>
                <div className="space-y-0.5">
                  {launcherFilteredFiles.map((path) => (
                    <button
                      key={path}
                      type="button"
                      onClick={() => {
                        setSelectedFilePath(path);
                        setLauncherQuery('');
                        openAndFocusTab('files');
                      }}
                      className="w-full px-2.5 py-1.5 rounded-[7px] flex items-center gap-2 text-left hover:bg-[var(--bg-hover)] cursor-pointer transition-colors"
                      style={{ color: 'var(--text-body)' }}
                    >
                      <FileCode size={13} className="shrink-0" style={{ color: 'var(--text-muted)' }} />
                      <span className="font-mono-tech text-[11.5px] truncate">{path}</span>
                    </button>
                  ))}
                </div>
              </div>
            )}

            <div className="space-y-1">
              <div
                className="px-1 text-[10.5px] font-medium uppercase tracking-wider"
                style={{ color: 'var(--text-muted)' }}
              >
                Quick Open
              </div>
              <div className="space-y-0.5">
                {[
                  { id: 'changes' as InspectorTab, label: 'Changes', hint: 'Review', icon: <GitDiff size={14} /> },
                  { id: 'files' as InspectorTab, label: 'Open File', hint: 'Workspace', icon: <FileCode size={14} /> },
                  { id: 'terminal' as InspectorTab, label: 'Terminal', hint: 'Shell', icon: <TerminalWindow size={14} /> },
                  { id: 'preview' as InspectorTab, label: 'Browser / Preview', hint: 'Live URL', icon: <Globe size={14} /> },
                  { id: 'traces' as InspectorTab, label: 'Traces', hint: 'Spans', icon: <TreeStructure size={14} /> },
                ].map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => openAndFocusTab(item.id)}
                    className="w-full px-2.5 py-2 rounded-[7px] flex items-center justify-between gap-2 text-left hover:bg-[var(--bg-hover)] cursor-pointer transition-colors"
                    style={{ color: 'var(--text-main)' }}
                  >
                    <span className="inline-flex items-center gap-2.5 min-w-0 text-[12px] font-medium">
                      <span style={{ color: 'var(--text-muted)' }}>{item.icon}</span>
                      <span className="truncate">{item.label}</span>
                    </span>
                    <span className="text-[11px] shrink-0" style={{ color: 'var(--text-muted)' }}>
                      {item.hint}
                    </span>
                  </button>
                ))}
              </div>
            </div>

            <div className="space-y-1.5">
              <div className="px-1 flex items-center justify-between">
                <span
                  className="text-[10.5px] font-medium uppercase tracking-wider"
                  style={{ color: 'var(--text-muted)' }}
                >
                  Changed Files
                </span>
                <span
                  className="font-mono-tech text-[10px] px-1.5 py-0.5 rounded-full"
                  style={{ backgroundColor: 'var(--bg-card)', color: 'var(--text-muted)' }}
                >
                  {diffs.length}
                </span>
              </div>
              {diffs.length === 0 ? (
                <div className="px-2.5 py-1.5 text-[11.5px]" style={{ color: 'var(--text-muted)' }}>
                  Working tree clean
                </div>
              ) : (
                <div className="space-y-0.5">
                  {diffs.slice(0, 8).map((df) => (
                    <button
                      key={df.path}
                      type="button"
                      onClick={() => openAndFocusTab('changes')}
                      className="w-full px-2.5 py-1.5 rounded-[7px] flex items-center justify-between gap-2 text-left hover:bg-[var(--bg-hover)] cursor-pointer transition-colors"
                    >
                      <span className="inline-flex items-center gap-2 min-w-0">
                        <GitDiff size={12} className="shrink-0" style={{ color: 'var(--accent-blue)' }} />
                        <span className="font-mono-tech text-[11.5px] truncate" style={{ color: 'var(--text-body)' }}>
                          {df.path}
                        </span>
                      </span>
                      <span className="font-mono-tech text-[10px] shrink-0 flex items-center gap-1">
                        {df.additions > 0 && <span style={{ color: 'var(--accent-green)' }}>+{df.additions}</span>}
                        {df.deletions > 0 && <span style={{ color: 'var(--accent-red)' }}>-{df.deletions}</span>}
                      </span>
                    </button>
                  ))}
                </div>
              )}
            </div>

            <div className="space-y-1.5">
              <div className="px-1 flex items-center justify-between">
                <span
                  className="text-[10.5px] font-medium uppercase tracking-wider"
                  style={{ color: 'var(--text-muted)' }}
                >
                  Active Threads
                </span>
                <span
                  className="font-mono-tech text-[10px] px-1.5 py-0.5 rounded-full"
                  style={{ backgroundColor: 'var(--bg-card)', color: 'var(--text-muted)' }}
                >
                  {activeThreads.length}
                </span>
              </div>
              {activeThreads.length === 0 ? (
                <div className="px-2.5 py-1.5 text-[11.5px]" style={{ color: 'var(--text-muted)' }}>
                  No running threads
                </div>
              ) : (
                <div className="space-y-0.5">
                  {activeThreads.map((t) => (
                    <button
                      key={t.id}
                      type="button"
                      onClick={() => onSelectThread?.(t.id)}
                      className="w-full px-2.5 py-1.5 rounded-[7px] flex items-center justify-between gap-2 text-left hover:bg-[var(--bg-hover)] cursor-pointer transition-colors"
                    >
                      <span className="inline-flex items-center gap-2 min-w-0">
                        <HarnessIcon harness={t.harness} size={12} />
                        <span className="text-[11.5px] truncate" style={{ color: 'var(--text-body)' }}>
                          {t.title}
                        </span>
                      </span>
                      <span
                        className="font-mono-tech text-[10px] shrink-0"
                        style={{
                          color: t.status === 'AWAITING_APPROVAL' ? 'var(--accent-yellow)' : 'var(--accent-green)',
                        }}
                      >
                        {t.status === 'AWAITING_APPROVAL' ? 'approval' : 'running'}
                      </span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          </div>
        )}

        {/* 1. CHANGES (GIT & DIFF) */}
        {!showLauncher && activeTab === 'changes' && (
          <div className="flex flex-col h-full">
            <div
              className="p-2.5 border-b space-y-2 shrink-0"
              style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-card)' }}
            >
              <div className="flex items-center justify-between gap-2 min-w-0">
                <div className="flex items-center gap-1 min-w-0 font-mono-tech text-[11px]">
                  <GitBranch size={12} className="shrink-0" style={{ color: 'var(--accent-blue)' }} />
                  <select
                    value={thread.branch}
                    disabled={gitBusy}
                    onChange={(e) => void handleCheckoutBranch(e.target.value)}
                    className="h-7 rounded-[6px] border px-1.5 font-mono-tech text-[11px] max-w-[130px] truncate focus:outline-none"
                    style={{
                      backgroundColor: 'var(--bg-app)',
                      borderColor: 'var(--border-line)',
                      color: 'var(--text-main)',
                    }}
                  >
                    {(branches.includes(thread.branch) ? branches : [thread.branch, ...branches]).map((b) => (
                      <option key={b} value={b}>
                        {b}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    disabled={gitBusy}
                    onClick={() => void handlePull()}
                    className="btn-ghost w-7 h-7 flex items-center justify-center cursor-pointer disabled:opacity-50 shrink-0"
                    title="Git Pull"
                    aria-label="Git Pull"
                  >
                    <ArrowDown size={11} />
                  </button>
                  <button
                    type="button"
                    disabled={gitBusy}
                    onClick={() => void handlePush()}
                    className="btn-ghost w-7 h-7 flex items-center justify-center cursor-pointer disabled:opacity-50 shrink-0"
                    title="Git Push"
                    aria-label="Git Push"
                  >
                    <ArrowUp size={11} />
                  </button>
                  <button
                    type="button"
                    disabled={gitBusy}
                    onClick={() => void onRefreshDiffs(turnScopeFilter)}
                    className="btn-ghost w-7 h-7 flex items-center justify-center cursor-pointer disabled:opacity-50 shrink-0"
                    title="Refresh Diffs"
                    aria-label="Refresh Diffs"
                  >
                    <ArrowClockwise size={11} />
                  </button>
                </div>

                <div className="flex items-center gap-1 shrink-0">
                  <div
                    className="grid grid-cols-2 h-7 p-0.5 rounded-[6px] border text-[10.5px] font-mono-tech shrink-0"
                    style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
                  >
                    <button
                      type="button"
                      onClick={() => setDiffMode('unified')}
                      className="px-2 rounded-[4px] whitespace-nowrap cursor-pointer"
                      style={
                        diffMode === 'unified'
                          ? { backgroundColor: 'var(--btn-solid-bg)', color: 'var(--btn-solid-text)' }
                          : { color: 'var(--text-muted)' }
                      }
                    >
                      Unified
                    </button>
                    <button
                      type="button"
                      onClick={() => setDiffMode('split')}
                      className="px-2 rounded-[4px] whitespace-nowrap cursor-pointer"
                      style={
                        diffMode === 'split'
                          ? { backgroundColor: 'var(--btn-solid-bg)', color: 'var(--btn-solid-text)' }
                          : { color: 'var(--text-muted)' }
                      }
                    >
                      Split
                    </button>
                  </div>
                </div>
              </div>

              <div className="flex items-center justify-between gap-2 min-w-0">
                <select
                  value={turnScopeFilter}
                  onChange={(e) => {
                    const val = e.target.value;
                    onChangeTurnScope(val);
                    void onRefreshDiffs(val);
                  }}
                  className="h-7 rounded-[6px] border px-2 text-[11px] font-mono-tech focus:outline-none min-w-0 flex-1 max-w-[230px]"
                  style={{
                    backgroundColor: 'var(--bg-app)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-main)',
                  }}
                >
                  <option value="all">All Uncommitted ({diffs.length} files)</option>
                  {checkpoints.map((cp) => (
                    <option key={cp.id} value={cp.id}>
                      Turn #{cp.turnNumber} · {cp.commitHash}
                    </option>
                  ))}
                </select>

                <button
                  type="button"
                  onClick={onOpenCreatePr}
                  className="btn-solid h-7 px-2.5 text-[11px] font-medium inline-flex items-center gap-1.5 whitespace-nowrap shrink-0 cursor-pointer"
                >
                  <GitPullRequest size={12} />
                  <span>Create PR</span>
                </button>
              </div>
            </div>

            <div className="flex-1 overflow-y-auto p-3 space-y-3">
              {visibleDiffs.length === 0 ? (
                <div
                  className="sakana-card rounded-[10px] p-6 text-center text-[12px]"
                  style={{ color: 'var(--text-muted)' }}
                >
                  Working tree clean — no uncommitted changes.
                </div>
              ) : (
                visibleDiffs.map((df) => (
                  <div
                    key={df.path}
                    className="rounded-[10px] border overflow-hidden"
                    style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-card)' }}
                  >
                    <div
                      className="px-3 py-2 border-b flex items-center justify-between gap-2 text-[11.5px] font-mono-tech"
                      style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-elevated)' }}
                    >
                      <span className="truncate font-medium" style={{ color: 'var(--text-main)' }}>
                        {df.path}
                      </span>
                      <span className="shrink-0 flex items-center gap-1.5 text-[10.5px]">
                        <span style={{ color: 'var(--accent-green)' }}>+{df.additions}</span>
                        <span style={{ color: 'var(--accent-red)' }}>-{df.deletions}</span>
                      </span>
                    </div>

                    {df.hunks
                      .filter((h) => !h.discarded)
                      .map((hunk) => (
                        <div key={hunk.id} className="border-t first:border-t-0" style={{ borderColor: 'var(--border-line)' }}>
                          <div
                            className="px-3 py-1 flex items-center justify-between gap-2 font-mono-tech text-[10.5px]"
                            style={{ backgroundColor: 'var(--bg-app)', color: 'var(--text-muted)' }}
                          >
                            <span className="truncate">{hunk.header}</span>
                            <div className="flex items-center gap-1 shrink-0">
                              <button
                                type="button"
                                onClick={() =>
                                  void onToggleStageHunk(df.path, hunk.id, hunk.header, !hunk.staged)
                                }
                                className="px-1.5 py-0.5 rounded border cursor-pointer"
                                style={{
                                  borderColor: 'var(--border-line)',
                                  color: hunk.staged ? 'var(--accent-green)' : 'var(--text-body)',
                                }}
                              >
                                {hunk.staged ? 'Staged' : 'Stage'}
                              </button>
                              <button
                                type="button"
                                onClick={() => void onDiscardHunk(df.path, hunk.id, hunk.header)}
                                className="px-1.5 py-0.5 rounded border cursor-pointer"
                                style={{ borderColor: 'var(--border-line)', color: 'var(--accent-red)' }}
                              >
                                Discard
                              </button>
                            </div>
                          </div>

                          <div className="font-mono-tech text-[11px] leading-[1.6] overflow-x-auto">
                            {hunk.lines.map((line, lIdx) => {
                              const bg =
                                line.type === 'add'
                                  ? 'var(--diff-add-bg)'
                                  : line.type === 'del'
                                    ? 'var(--diff-del-bg)'
                                    : 'transparent';
                              const gutter =
                                line.type === 'add'
                                  ? 'var(--diff-add-gutter)'
                                  : line.type === 'del'
                                    ? 'var(--diff-del-gutter)'
                                    : 'transparent';
                              const lineNum = line.newLine ?? line.oldLine ?? lIdx + 1;

                              if (diffMode === 'split') {
                                return (
                                  <div
                                    key={lIdx}
                                    className="grid grid-cols-2 border-b last:border-b-0"
                                    style={{ borderColor: 'var(--border-line)' }}
                                  >
                                    <div
                                      className="px-2 py-0.5 truncate border-r"
                                      style={{
                                        borderColor: 'var(--border-line)',
                                        backgroundColor: line.type === 'del' ? 'var(--diff-del-bg)' : 'transparent',
                                        color: line.type === 'add' ? 'var(--text-muted)' : 'var(--text-main)',
                                      }}
                                    >
                                      <span className="inline-block w-7 opacity-50 select-none">
                                        {line.type !== 'add' ? line.oldLine ?? '' : ''}
                                      </span>
                                      {line.type !== 'add' ? line.content : ''}
                                    </div>
                                    <div
                                      className="px-2 py-0.5 truncate"
                                      style={{
                                        backgroundColor: line.type === 'add' ? 'var(--diff-add-bg)' : 'transparent',
                                        color: line.type === 'del' ? 'var(--text-muted)' : 'var(--text-main)',
                                      }}
                                    >
                                      <span className="inline-block w-7 opacity-50 select-none">
                                        {line.type !== 'del' ? line.newLine ?? '' : ''}
                                      </span>
                                      {line.type !== 'del' ? line.content : ''}
                                    </div>
                                  </div>
                                );
                              }

                              return (
                                <div
                                  key={lIdx}
                                  className="group flex items-center px-2 py-0.5 hover:brightness-95"
                                  style={{ backgroundColor: bg, borderLeft: `2px solid ${gutter}` }}
                                >
                                  <button
                                    type="button"
                                    onClick={() => setCommentTarget({ file: df.path, line: lineNum })}
                                    className="opacity-0 group-hover:opacity-100 mr-1 p-0.5 rounded cursor-pointer"
                                    style={{ backgroundColor: 'var(--accent-blue)', color: '#fff' }}
                                    title="Add inline diff comment to chat"
                                    aria-label="Add inline diff comment to chat"
                                  >
                                    <Plus size={9} weight="bold" />
                                  </button>
                                  <span className="w-7 text-right pr-2 opacity-50 select-none shrink-0">
                                    {lineNum}
                                  </span>
                                  <span className="w-4 select-none shrink-0">
                                    {line.type === 'add' ? '+' : line.type === 'del' ? '-' : ' '}
                                  </span>
                                  <span className="whitespace-pre" style={{ color: 'var(--text-main)' }}>
                                    {line.content}
                                  </span>
                                </div>
                              );
                            })}
                          </div>
                        </div>
                      ))}
                  </div>
                ))
              )}

              {commentTarget && (
                <div
                  className="sakana-card rounded-[10px] p-3 space-y-2 pop-enter"
                  style={{ borderColor: 'var(--accent-blue)' }}
                >
                  <div className="flex items-center justify-between text-[11px] font-mono-tech">
                    <span style={{ color: 'var(--accent-blue)' }}>
                      Comment on {commentTarget.file}:L{commentTarget.line}
                    </span>
                    <button
                      type="button"
                      onClick={() => setCommentTarget(null)}
                      aria-label="Close diff comment"
                      className="cursor-pointer"
                      style={{ color: 'var(--text-muted)' }}
                    >
                      <X size={12} />
                    </button>
                  </div>
                  <input
                    value={commentText}
                    onChange={(e) => setCommentText(e.target.value)}
                    placeholder="Ask agent to refine this line..."
                    className="w-full rounded-[6px] border px-2.5 py-1.5 text-[12px] focus:outline-none"
                    style={{
                      backgroundColor: 'var(--bg-app)',
                      borderColor: 'var(--border-line)',
                      color: 'var(--text-main)',
                    }}
                  />
                  <div className="flex justify-end">
                    <button
                      type="button"
                      onClick={() => {
                        if (!commentText.trim()) return;
                        onAttachChip({
                          id: `dc-${Date.now()}`,
                          kind: 'diff-comment',
                          label: `Diff ${commentTarget.file}:L${commentTarget.line}`,
                          payload: commentText.trim(),
                        });
                        setCommentText('');
                        setCommentTarget(null);
                      }}
                      className="btn-solid px-3 py-1 text-[11px] font-medium cursor-pointer"
                    >
                      Attach to Chat
                    </button>
                  </div>
                </div>
              )}
            </div>

            <div
              className="p-2.5 border-t space-y-2 shrink-0"
              style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-card)' }}
            >
              <div className="flex items-center gap-1.5">
                <input
                  value={commitMsg}
                  onChange={(e) => setCommitMsg(e.target.value)}
                  placeholder="Commit message..."
                  className="flex-1 rounded-[6px] border px-2.5 py-1.5 text-[11.5px] font-mono-tech focus:outline-none"
                  style={{
                    backgroundColor: 'var(--bg-app)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-main)',
                  }}
                />
                <button
                  type="button"
                  disabled={gitBusy}
                  onClick={() => void handleDraftCommit()}
                  className="btn-ghost px-2 py-1.5 text-[11px] inline-flex items-center gap-1 cursor-pointer disabled:opacity-50"
                  title="Draft commit message"
                >
                  <Sparkle size={12} />
                  <span>Draft</span>
                </button>
                <button
                  type="button"
                  disabled={gitBusy || !commitMsg.trim()}
                  onClick={() => void handleCommit()}
                  className="btn-solid px-3 py-1.5 text-[11px] font-medium cursor-pointer disabled:opacity-50"
                >
                  Commit
                </button>
              </div>
            </div>
          </div>
        )}

        {/* 2. FILES (TREE + CODEMIRROR 6 EDITOR + ASK SAKANA) */}
        {!showLauncher && activeTab === 'files' && (
          <div className="flex flex-col h-full">
            <div
              className="px-3 py-2 border-b flex items-center justify-between gap-2 shrink-0"
              style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-card)' }}
            >
              <div className="flex items-center gap-1.5 min-w-0 flex-1">
                <MagnifyingGlass size={12} style={{ color: 'var(--text-muted)' }} className="shrink-0" />
                <input
                  value={fileFilter}
                  onChange={(e) => setFileFilter(e.target.value)}
                  placeholder="Filter workspace files..."
                  className="w-full bg-transparent font-mono-tech text-[11px] focus:outline-none"
                  style={{ color: 'var(--text-main)' }}
                />
              </div>

              <div className="flex items-center gap-1.5 shrink-0">
                {lineSelection && !editingFile && activeFile && (
                  <button
                    type="button"
                    onClick={() => {
                      const label =
                        lineSelection.start === lineSelection.end
                          ? `@${activeFile.path}:L${lineSelection.start}`
                          : `@${activeFile.path}:L${lineSelection.start}-L${lineSelection.end}`;
                      onAttachChip({
                        id: `ln-${Date.now()}`,
                        kind: 'lines',
                        label,
                        payload: activeFile.lines
                          .slice(lineSelection.start - 1, lineSelection.end)
                          .join('\n'),
                      });
                      setLineSelection(null);
                    }}
                    className="btn-solid px-2 py-1 text-[10.5px] font-mono-tech cursor-pointer"
                  >
                    Ask Sakana (L{lineSelection.start}
                    {lineSelection.end !== lineSelection.start ? `-${lineSelection.end}` : ''})
                  </button>
                )}

                {activeFile && /\.(md|markdown)$/i.test(activeFile.path) && !editingFile && (
                  <button
                    type="button"
                    onClick={() => setPreviewMarkdown((v) => !v)}
                    className={previewMarkdown ? 'btn-solid px-2 py-1 text-[11px] inline-flex items-center gap-1 cursor-pointer' : 'btn-ghost px-2 py-1 text-[11px] inline-flex items-center gap-1 cursor-pointer'}
                    title={previewMarkdown ? 'Switch to raw source view' : 'Preview rendered Markdown'}
                  >
                    <Eye size={12} />
                    <span>{previewMarkdown ? 'Preview' : 'Source'}</span>
                  </button>
                )}

                {activeFile && !editingFile && (
                  <button
                    type="button"
                    onClick={async () => {
                      try {
                        await tauriApi.openFileExternal(effectiveCwd, activeFile.path, wslDistro);
                        onToast(`Opened ${activeFile.path} externally`);
                      } catch (err) {
                        onToast(`Open external failed: ${String(err)}`);
                      }
                    }}
                    className="btn-ghost px-2 py-1 text-[11px] inline-flex items-center gap-1 cursor-pointer"
                    title="Open in external default application"
                  >
                    <ArrowSquareOut size={12} />
                    <span>Open</span>
                  </button>
                )}

                {activeFile && (editingFile ? (
                  <>
                    <button
                      type="button"
                      disabled={savingFile}
                      onClick={() => void handleSaveActiveFile()}
                      className="btn-solid px-2.5 py-1 text-[11px] font-medium inline-flex items-center gap-1 cursor-pointer disabled:opacity-50"
                    >
                      <Check size={12} />
                      <span>{savingFile ? 'Saving...' : 'Save'}</span>
                    </button>
                    <button
                      type="button"
                      onClick={() => setEditingFile(false)}
                      className="btn-ghost px-2 py-1 text-[11px] cursor-pointer"
                    >
                      Cancel
                    </button>
                  </>
                ) : (
                  <button
                    type="button"
                    onClick={() => {
                      setEditDraft(activeFile.lines.join('\n'));
                      setEditingFile(true);
                    }}
                    className="btn-ghost px-2 py-1 text-[11px] inline-flex items-center gap-1 cursor-pointer"
                  >
                    <PencilSimple size={12} />
                    <span>Edit</span>
                  </button>
                ))}
              </div>
            </div>

            {/* Collapsible Directory Tree */}
            <div
              className="max-h-[180px] overflow-y-auto border-b px-1.5 py-1.5 space-y-0.5 shrink-0"
              style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-elevated)' }}
            >
              {filteredPaths.length === 0 ? (
                <div className="px-2 py-2 text-[11px] font-mono-tech" style={{ color: 'var(--text-muted)' }}>
                  {fileFilter ? 'No matching files' : 'No workspace files'}
                </div>
              ) : (
                (() => {
                  const forceExpand = fileFilter.trim().length > 0;
                  const renderNode = (node: FileTreeNode, depth: number): React.ReactNode => {
                    if (node.isDir) {
                      const isExpanded = forceExpand || Boolean(expandedDirs[node.path]);
                      return (
                        <div key={`dir:${node.path}`}>
                          <button
                            type="button"
                            onClick={() =>
                              setExpandedDirs((prev) => ({
                                ...prev,
                                [node.path]: !isExpanded,
                              }))
                            }
                            className="w-full text-left py-1 pr-2 rounded-[5px] font-mono-tech text-[11px] flex items-center gap-1 truncate cursor-pointer hover:bg-[var(--bg-hover)]"
                            style={{ paddingLeft: `${depth * 12 + 6}px`, color: 'var(--text-body)' }}
                          >
                            {isExpanded ? (
                              <CaretDown size={10} className="shrink-0 opacity-70" />
                            ) : (
                              <CaretRight size={10} className="shrink-0 opacity-70" />
                            )}
                            <FolderSimple size={12} className="shrink-0 opacity-75" />
                            <span className="truncate">{node.name}</span>
                          </button>
                          {isExpanded && node.children.map((ch) => renderNode(ch, depth + 1))}
                        </div>
                      );
                    }
                    const active = node.path === activeFile?.path;
                    return (
                      <button
                        key={`file:${node.path}`}
                        type="button"
                        onClick={() => {
                          setSelectedFilePath(node.path);
                          setEditingFile(false);
                          setLineSelection(null);
                        }}
                        className="w-full text-left py-1 pr-2 rounded-[5px] font-mono-tech text-[11px] flex items-center gap-1.5 truncate cursor-pointer"
                        style={
                          active
                            ? {
                                paddingLeft: `${depth * 12 + 18}px`,
                                backgroundColor: 'var(--bg-card)',
                                color: 'var(--text-main)',
                                fontWeight: 500,
                              }
                            : {
                                paddingLeft: `${depth * 12 + 18}px`,
                                color: 'var(--text-muted)',
                              }
                        }
                      >
                        <FileCode size={12} className="shrink-0 opacity-75" />
                        <span className="truncate">{node.name}</span>
                      </button>
                    );
                  };
                  return fileTree.map((node) => renderNode(node, 0));
                })()
              )}
            </div>

            {(activeFile || selectedFilePath) && (() => {
              const shownPath = activeFile?.path || selectedFilePath;
              const fileName = shownPath.split('/').pop() || shownPath;
              const isInWorkspace = workspacePaths.includes(shownPath);
              return (
                <div
                  className="px-3 py-1.5 border-b flex items-center justify-between gap-2 shrink-0"
                  style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-card)' }}
                  title={shownPath}
                >
                  <div className="flex items-center gap-1.5 min-w-0">
                    <FileCode size={12} className="shrink-0" style={{ color: 'var(--accent-blue)' }} />
                    <span
                      className="font-mono-tech text-[11.5px] font-semibold shrink-0"
                      style={{ color: 'var(--text-main)' }}
                    >
                      {fileName}
                    </span>
                    {shownPath !== fileName && (
                      <span
                        className="font-mono-tech text-[10.5px] truncate"
                        style={{ color: 'var(--text-muted)' }}
                      >
                        {shownPath}
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-1.5 shrink-0">
                    {!isInWorkspace && workspacePaths.length > 0 && (
                      <span
                        className="font-mono-tech text-[9.5px] px-1.5 py-0.5 rounded-[4px]"
                        style={{
                          backgroundColor: 'var(--accent-blue-bg)',
                          color: 'var(--accent-blue)',
                        }}
                        title="Opened from outside the current workspace tree"
                      >
                        External
                      </span>
                    )}
                    {activeFile?.language && (
                      <span
                        className="font-mono-tech text-[9.5px] uppercase"
                        style={{ color: 'var(--text-muted)' }}
                      >
                        {activeFile.language}
                      </span>
                    )}
                  </div>
                </div>
              );
            })()}

            {!activeFile ? (
              <div className="p-6 text-center text-[12px]" style={{ color: 'var(--text-muted)' }}>
                {loadingFile ? `Loading ${selectedFilePath || 'file'}...` : 'Select a file to inspect'}
              </div>
            ) : editingFile ? (
              <div className="flex-1 min-h-0 p-3">
                <CodeMirrorEditor
                  value={editDraft}
                  filePath={activeFile.path}
                  theme={theme}
                  onChange={setEditDraft}
                  onSave={() => void handleSaveActiveFile()}
                />
              </div>
            ) : /\.(md|markdown)$/i.test(activeFile.path) && previewMarkdown ? (
              <div className="flex-1 overflow-y-auto p-4">
                {loadingFile ? (
                  <div className="text-[12px]" style={{ color: 'var(--text-muted)' }}>
                    Loading {selectedFilePath}...
                  </div>
                ) : (
                  <SimpleMarkdown text={activeFile.lines.join('\n')} />
                )}
              </div>
            ) : (
              <div className="flex-1 overflow-y-auto p-3 font-mono-tech text-[11.5px] leading-[1.65]">
                {loadingFile ? (
                  <div className="text-[12px]" style={{ color: 'var(--text-muted)' }}>
                    Loading {selectedFilePath}...
                  </div>
                ) : (
                  activeFile.lines.map((line, idx) => {
                    const ln = idx + 1;
                    const selected =
                      lineSelection && ln >= lineSelection.start && ln <= lineSelection.end;
                    return (
                      <div
                        key={ln}
                        className="flex items-start rounded-[4px] px-1.5"
                        style={{
                          backgroundColor: selected ? 'var(--accent-blue-bg)' : 'transparent',
                        }}
                      >
                        <button
                          type="button"
                          onClick={(e) => handleLineClick(ln, e.shiftKey)}
                          className="w-8 text-right pr-3 select-none shrink-0 cursor-pointer hover:underline"
                          style={{ color: selected ? 'var(--accent-blue)' : 'var(--text-muted)' }}
                          title="Click or Shift+Click to select lines for Ask Sakana"
                        >
                          {ln}
                        </button>
                        <span className="whitespace-pre" style={{ color: 'var(--text-main)' }}>
                          {highlightCodeLine(line)}
                        </span>
                      </div>
                    );
                  })
                )}
              </div>
            )}
          </div>
        )}

        {/* 3. REAL NATIVE EMBEDDED BROWSER */}
        {!showLauncher && activeTab === 'preview' && (
          <NativeBrowserPane
            visible={open && !showLauncher && activeTab === 'preview'}
            overlayBlocked={browserOverlayBlocked}
            initialUrl={previewUrl}
            command={browserCommand}
            onToast={onToast}
          />
        )}

        {/* 4. TERMINAL (REAL XTERM.JS + PTY + NATURAL LANGUAGE ASSIST) */}
        {!showLauncher && activeTab === 'terminal' && (
          <div className="flex flex-col h-full">
            <div
              className="px-3 py-2 border-b flex items-center justify-between gap-2 shrink-0"
              style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-card)' }}
            >
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={() => setTermTab('wt')}
                  className="px-2.5 py-1 rounded-[6px] font-mono-tech text-[11px] cursor-pointer"
                  style={
                    termTab === 'wt'
                      ? { backgroundColor: 'var(--bg-elevated)', color: 'var(--text-main)' }
                      : { color: 'var(--text-muted)' }
                  }
                >
                  shell · {thread.branch}
                </button>
                <button
                  type="button"
                  onClick={() => setTermTab('host')}
                  className="px-2.5 py-1 rounded-[6px] font-mono-tech text-[11px] cursor-pointer"
                  style={
                    termTab === 'host'
                      ? { backgroundColor: 'var(--bg-elevated)', color: 'var(--text-main)' }
                      : { color: 'var(--text-muted)' }
                  }
                >
                  host · project
                </button>
              </div>
            </div>

            <div className="flex-1 min-h-0">
              <XtermTerminal
                sessionKey={`${thread.id}:${termTab}`}
                cwd={termTab === 'wt' ? effectiveCwd : projectCwd || effectiveCwd}
                wslDistro={wslDistro}
                theme={theme}
                onToast={onToast}
              />
            </div>
          </div>
        )}

        {/* 5. TRACES */}
        {!showLauncher && activeTab === 'traces' && (
          <div className="h-full overflow-y-auto p-3.5 space-y-3">
            <div className="flex items-center justify-between">
              <div className="text-[11.5px] font-medium" style={{ color: 'var(--text-muted)' }}>
                Master-to-Subagent Execution Tree
              </div>
              <button
                type="button"
                disabled={loadingTraces}
                onClick={() => void loadTraces()}
                className="btn-ghost p-1 cursor-pointer disabled:opacity-50"
                title="Refresh Traces"
                aria-label="Refresh Traces"
              >
                <ArrowClockwise size={12} />
              </button>
            </div>
            {liveTraces.length === 0 ? (
              <div
                className="sakana-card rounded-[10px] p-6 text-center text-[12px]"
                style={{ color: 'var(--text-muted)' }}
              >
                No trace spans recorded for this thread yet.
              </div>
            ) : (
              liveTraces.map((sp) => <TraceSpanNode key={sp.id} span={sp} />)
            )}
          </div>
        )}
      </div>
    </aside>
  );
}
