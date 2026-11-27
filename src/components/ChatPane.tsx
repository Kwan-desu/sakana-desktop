import { Fragment, memo, useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import {
  ArrowCounterClockwise,
  ArrowDown,
  ArrowUp,
  CaretDown,
  CaretRight,
  Check,
  Code,
  Columns,
  Copy,
  Eye,
  File,
  FilePdf,
  FileText,
  GitBranch,
  GitFork,
  Paperclip,
  PencilSimple,
  Plus,
  Robot,
  ShieldCheck,
  ShieldWarning,
  SidebarSimple,
  Stop,
  Trash,
  User,
  X,
} from '@phosphor-icons/react';
import { isTauriRuntime, tauriApi } from '../lib/tauriApi';
import type {
  AttachedContextChip,
  ChatMessage,
  HarnessAuthAccount,
  HarnessId,
  InlineWidgetData,
  PermissionMode,
  Project,
  QueuedPrompt,
  ReasoningEffort,
  SkillItem,
  ThemeId,
  Thread,
} from '../types';
import { DiscordIcon, HARNESS_META, HarnessIcon } from './HarnessBadge';

interface ChatPaneProps {
  thread: Thread;
  secondaryThread?: Thread;
  secondaryThreadId?: string;
  onSelectSecondaryThread?: (id: string) => void;
  onQuickNewThread?: () => void;
  project?: Project;
  allThreads: Thread[];
  splitView: boolean;
  onToggleSplitView: () => void;
  leftSidebarOpen: boolean;
  onToggleLeftSidebar: () => void;
  rightInspectorOpen: boolean;
  onToggleRightInspector: () => void;
  onSelectThread: (id: string) => void;
  onSendPrompt: (threadId: string, prompt: string) => void;
  onStopStreaming: (threadId: string) => void;
  onUpdateThreadConfig: (
    threadId: string,
    patch: Partial<Pick<Thread, 'harness' | 'model' | 'effort' | 'permissionMode' | 'account'>>,
  ) => void;
  onResolveApproval: (
    threadId: string,
    decision: 'approve_once' | 'always_allow' | 'enable_yolo' | 'deny',
  ) => void;
  onRevertToCheckpoint: (threadId: string, checkpointId: string) => void;
  onViewCheckpointDiff: (checkpointId: string) => void;
  onForkAtMessage: (threadId: string, msgIndex: number) => void;
  attachedChips: AttachedContextChip[];
  onRemoveChip: (id: string) => void;
  onAddChips?: (chips: AttachedContextChip[]) => void;
  onPickAttachments?: () => Promise<void> | void;
  queuedPrompts: QueuedPrompt[];
  onSendQueuedNow: (id: string) => void;
  onRemoveQueued: (id: string) => void;
  onOpenCommandPalette?: () => void;
  harnessModels?: Record<string, { id: string; label: string; family: string; efforts: string[] }[]>;
  harnessAccounts?: HarnessAuthAccount[];
  onOpenHarnessAccounts?: () => void;
  disabledHarnesses?: HarnessId[];
  onToast: (msg: string) => void;
  onOpenFile?: (path: string) => void;
  splitBridgeEnabled?: boolean;
  onToggleSplitBridge?: () => void;
  skills?: SkillItem[];
  theme?: ThemeId;
}

function CopyButton({
  text,
  className,
  onCopied,
}: {
  text: string;
  className?: string;
  onCopied?: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    };
  }, []);

  const handleCopy = () => {
    navigator.clipboard?.writeText(text);
    setCopied(true);
    onCopied?.();
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => setCopied(false), 1500);
  };

  return (
    <button
      type="button"
      onClick={handleCopy}
      className={className}
      style={copied ? { color: 'var(--accent-green)' } : undefined}
    >
      {copied ? <Check size={11} weight="bold" /> : <Copy size={11} />}
      <span>{copied ? 'Copied' : 'Copy'}</span>
    </button>
  );
}

const SYN_KEYWORDS = new Set([
  'const', 'let', 'var', 'function', 'return', 'if', 'else', 'for', 'while', 'do',
  'switch', 'case', 'break', 'continue', 'default', 'try', 'catch', 'finally', 'throw',
  'async', 'await', 'import', 'from', 'export', 'type', 'interface', 'enum', 'class',
  'extends', 'implements', 'new', 'this', 'super', 'typeof', 'keyof', 'in', 'of',
  'true', 'false', 'null', 'undefined', 'void', 'fn', 'pub', 'struct', 'impl', 'trait',
  'match', 'use', 'mod', 'crate', 'mut', 'ref', 'self', 'Self', 'where', 'loop', 'move',
  'def', 'elif', 'except', 'raise', 'with', 'as', 'pass', 'yield', 'lambda', 'None',
  'True', 'False', 'and', 'or', 'not', 'is', 'package', 'func', 'go', 'defer', 'chan',
  'select', 'SELECT', 'FROM', 'WHERE', 'INSERT', 'UPDATE', 'DELETE', 'JOIN', 'ON',
]);

function highlightCode(code: string, lang: string): ReactNode[] {
  const lowerLang = lang.toLowerCase();
  const isRust = lowerLang === 'rs' || lowerLang === 'rust';
  const hashComment = ['py', 'python', 'sh', 'bash', 'zsh', 'yaml', 'yml', 'toml', 'rb', 'ruby'].includes(lowerLang);
  const dashComment = lowerLang === 'sql';
  const commentPart = hashComment
    ? '#[^\\n]*'
    : dashComment
      ? '--[^\\n]*'
      : '\\/\\/[^\\n]*|\\/\\*[\\s\\S]*?\\*\\/';
  const singleQuotePart = isRust
    ? `'(?:\\\\(?:x[0-9a-fA-F]{2}|u\\{[0-9a-fA-F_]+\\}|.)|[^'\\\\\\n])'`
    : `'(?:\\\\.|[^'\\\\])*'`;
  const lifetimePart = isRust ? `|('[a-zA-Z_][a-zA-Z0-9_]*\\b(?!'))` : '';
  const tokenRe = new RegExp(
    `(${commentPart})|("(?:\\\\.|[^"\\\\])*"|${singleQuotePart}|\`(?:\\\\.|[^\`\\\\])*\`)|(\\b0x[0-9a-fA-F]+\\b|\\b\\d+(?:\\.\\d+)?\\b)|(\\b[a-zA-Z_][a-zA-Z0-9_]*\\b)(?=\\s*\\()|(\\b[a-zA-Z_][a-zA-Z0-9_]*\\b)${lifetimePart}`,
    'g',
  );

  const out: ReactNode[] = [];
  let lastIndex = 0;
  let key = 0;
  let match: RegExpExecArray | null;

  while ((match = tokenRe.exec(code)) !== null) {
    if (match.index > lastIndex) {
      out.push(code.slice(lastIndex, match.index));
    }
    const [full, cmt, str, num, fn, ident, lifetime] = match;
    if (cmt) {
      out.push(<span key={key++} className="syn-cmt">{cmt}</span>);
    } else if (str) {
      out.push(<span key={key++} className="syn-str">{str}</span>);
    } else if (num) {
      out.push(<span key={key++} className="syn-num">{num}</span>);
    } else if (fn) {
      out.push(
        <span key={key++} className={SYN_KEYWORDS.has(fn) ? 'syn-kw' : 'syn-fn'}>
          {fn}
        </span>,
      );
    } else if (ident) {
      out.push(
        SYN_KEYWORDS.has(ident) ? (
          <span key={key++} className="syn-kw">{ident}</span>
        ) : (
          ident
        ),
      );
    } else if (lifetime) {
      out.push(<span key={key++} className="syn-kw">{lifetime}</span>);
    } else {
      out.push(full);
    }
    lastIndex = tokenRe.lastIndex;
  }

  if (lastIndex < code.length) {
    out.push(code.slice(lastIndex));
  }
  return out;
}

function handleExternalLinkClick(e: React.MouseEvent, url: string) {
  e.preventDefault();
  if (!/^(https?:|mailto:)/i.test(url)) return;
  if (isTauriRuntime()) {
    void tauriApi.openExternalUrl(url).catch(() => {});
  } else {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}

const FILE_EXT_RE =
  /\.(md|markdown|txt|ts|tsx|js|jsx|mjs|cjs|json|rs|py|go|java|kt|c|cc|cpp|h|hpp|cs|rb|php|css|scss|less|html|htm|xml|svg|yaml|yml|toml|ini|conf|sql|sh|bash|zsh|bat|cmd|ps1|env|csv|log|pdf|png|jpg|jpeg|gif|webp|docx|xlsx|pptx|lock|gitignore|dockerfile)(?::\d+(?::\d+)?)?$/i;

function isLikelyFilePath(raw: string): boolean {
  const s = raw.trim();
  if (!s || s.length > 220 || /\s/.test(s) || /^https?:\/\//i.test(s) || s.startsWith('--')) {
    return false;
  }
  if (FILE_EXT_RE.test(s)) return true;
  if (/^(README|LICENSE|Makefile|Dockerfile|Cargo\.toml|package\.json)(?::\d+)?$/i.test(s)) {
    return true;
  }
  return false;
}

function renderInline(line: string, onOpenFile?: (path: string) => void): ReactNode[] {
  const inlineRe = /(`[^`]+`|\[[^\]]+\]\([^)]+\)|\*\*[^*\n]+\*\*|~~[^~\n]+~~|\*[^*\n]+\*|(?<!\w)_[^_\n]+_(?!\w))/g;
  const out: ReactNode[] = [];
  let lastIndex = 0;
  let key = 0;
  let match: RegExpExecArray | null;

  while ((match = inlineRe.exec(line)) !== null) {
    if (match.index > lastIndex) {
      out.push(line.slice(lastIndex, match.index));
    }
    const tok = match[0];
    if (tok.startsWith('`') && tok.endsWith('`')) {
      const inner = tok.slice(1, -1);
      if (onOpenFile && isLikelyFilePath(inner)) {
        out.push(
          <button
            key={key++}
            type="button"
            onClick={() => onOpenFile(inner)}
            className="font-mono-tech text-[12px] px-1.5 py-0.5 rounded-[5px] inline cursor-pointer hover:underline transition-opacity hover:opacity-90"
            style={{ backgroundColor: 'var(--accent-blue-bg)', color: 'var(--accent-blue)' }}
            title={`Open ${inner} in Sakana sidebar`}
          >
            {inner}
          </button>,
        );
      } else {
        out.push(
          <code
            key={key++}
            className="font-mono-tech text-[12px] px-1.5 py-0.5 rounded-[5px]"
            style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-main)' }}
          >
            {inner}
          </code>,
        );
      }
    } else if (tok.startsWith('[')) {
      const linkMatch = tok.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
      if (linkMatch) {
        const [, label, url] = linkMatch;
        const isLocalFile = !/^https?:\/\//i.test(url) && !url.startsWith('mailto:') && !url.startsWith('#');
        if (onOpenFile && isLocalFile) {
          out.push(
            <button
              key={key++}
              type="button"
              onClick={() => onOpenFile(url)}
              className="underline decoration-1 underline-offset-2 hover:opacity-80 cursor-pointer inline"
              style={{ color: 'var(--accent-blue)' }}
              title={`Open ${url} in Sakana sidebar`}
            >
              {label}
            </button>,
          );
        } else {
          out.push(
            <a
              key={key++}
              href={url}
              onClick={(e) => handleExternalLinkClick(e, url)}
              className="underline decoration-1 underline-offset-2 hover:opacity-80 cursor-pointer"
              style={{ color: 'var(--accent-blue)' }}
              title={url}
            >
              {label}
            </a>,
          );
        }
      } else {
        out.push(tok);
      }
    } else if (tok.startsWith('**') && tok.endsWith('**')) {
      out.push(
        <strong key={key++} className="font-semibold" style={{ color: 'var(--text-main)' }}>
          {tok.slice(2, -2)}
        </strong>,
      );
    } else if (tok.startsWith('~~') && tok.endsWith('~~')) {
      out.push(
        <del key={key++} className="opacity-75">
          {tok.slice(2, -2)}
        </del>,
      );
    } else if ((tok.startsWith('*') && tok.endsWith('*')) || (tok.startsWith('_') && tok.endsWith('_'))) {
      out.push(
        <em key={key++} className="italic">
          {tok.slice(1, -1)}
        </em>,
      );
    } else {
      out.push(tok);
    }
    lastIndex = inlineRe.lastIndex;
  }

  if (lastIndex < line.length) {
    out.push(line.slice(lastIndex));
  }
  return out;
}

function parseTableRow(row: string): string[] {
  const trimmed = row.trim().replace(/^\|/, '').replace(/\|$/, '');
  return trimmed.split('|').map((c) => c.trim());
}

function isTableSeparator(row: string): boolean {
  const cells = parseTableRow(row);
  return cells.length > 0 && cells.every((c) => /^:?-{3,}:?$/.test(c));
}

function parseTableAlignments(row: string): ('left' | 'center' | 'right')[] {
  return parseTableRow(row).map((c) => {
    if (c.startsWith(':') && c.endsWith(':')) return 'center';
    if (c.endsWith(':')) return 'right';
    return 'left';
  });
}

interface ParsedListLine {
  indent: number;
  ordered: boolean;
  taskChecked: boolean | null;
  content: string;
}

function matchListLine(line: string): ParsedListLine | null {
  const m = line.match(/^(\s*)(?:([-*])\s+(?:\[([ xX])\]\s+)?|(\d+)\.\s+)(.*)$/);
  if (!m) return null;
  return {
    indent: m[1].replace(/\t/g, '  ').length,
    ordered: Boolean(m[4]),
    taskChecked: m[3] !== undefined ? m[3].toLowerCase() === 'x' : null,
    content: m[5],
  };
}

function isHorizontalRule(line: string): boolean {
  return /^\s*(?:-{3,}|\*{3,}|_{3,})\s*$/.test(line);
}

function parseListBlock(
  lines: string[],
  startIndex: number,
  keyPrefix: string,
  onOpenFile?: (path: string) => void,
): { node: ReactNode; nextIndex: number } {
  const first = matchListLine(lines[startIndex]);
  if (!first) return { node: null, nextIndex: startIndex + 1 };

  const baseIndent = first.indent;
  const isOrdered = first.ordered;
  const items: Array<{ parsed: ParsedListLine; children: ReactNode[] }> = [];
  let i = startIndex;

  while (i < lines.length) {
    const curr = matchListLine(lines[i]);
    if (!curr || curr.indent < baseIndent) break;

    if (curr.indent === baseIndent) {
      if (curr.ordered !== isOrdered) break;
      const item: { parsed: ParsedListLine; children: ReactNode[] } = {
        parsed: curr,
        children: [],
      };
      items.push(item);
      i++;

      while (i < lines.length) {
        const next = matchListLine(lines[i]);
        if (!next || next.indent <= baseIndent) break;
        const sub = parseListBlock(lines, i, `${keyPrefix}-sub-${i}`, onOpenFile);
        item.children.push(sub.node);
        i = sub.nextIndex;
      }
    } else {
      const sub = parseListBlock(lines, i, `${keyPrefix}-sub-${i}`, onOpenFile);
      if (items.length > 0) {
        items[items.length - 1].children.push(sub.node);
      }
      i = sub.nextIndex;
    }
  }

  const ListTag = isOrdered ? 'ol' : 'ul';
  const listClass = isOrdered ? 'list-decimal pl-5 space-y-1 m-0' : 'list-disc pl-5 space-y-1 m-0';

  const node = (
    <ListTag key={`${keyPrefix}-list-${startIndex}`} className={listClass}>
      {items.map((item, idx) => {
        const isTask = item.parsed.taskChecked !== null;
        return (
          <li key={idx} className={isTask ? 'list-none -ml-4' : undefined}>
            {isTask ? (
              <div className="flex items-start gap-2">
                <input
                  type="checkbox"
                  checked={item.parsed.taskChecked === true}
                  readOnly
                  tabIndex={-1}
                  aria-label={`Task: ${item.parsed.content}`}
                  className="mt-[3px] shrink-0 pointer-events-none"
                  style={{ accentColor: 'var(--accent-blue)' }}
                />
                <span className={item.parsed.taskChecked ? 'line-through opacity-75' : undefined}>
                  {renderInline(item.parsed.content, onOpenFile)}
                </span>
              </div>
            ) : (
              renderInline(item.parsed.content, onOpenFile)
            )}
            {item.children}
          </li>
        );
      })}
    </ListTag>
  );

  return { node, nextIndex: i };
}

function renderMarkdownBlocks(
  blockText: string,
  keyPrefix: string,
  onOpenFile?: (path: string) => void,
): ReactNode[] {
  const lines = blockText.split('\n');
  const nodes: ReactNode[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i++;
      continue;
    }

    // Headings (# to ######)
    const headingMatch = line.match(/^(#{1,6})\s+(.+)$/);
    if (headingMatch) {
      const level = headingMatch[1].length;
      const content = renderInline(headingMatch[2], onOpenFile);
      const cls =
        level === 1
          ? 'text-[17px] font-semibold mt-2 mb-1'
          : level === 2
            ? 'text-[15.5px] font-semibold mt-2 mb-1'
            : level === 3
              ? 'text-[14px] font-semibold mt-1.5 mb-0.5'
              : level === 4
                ? 'text-[13.5px] font-semibold mt-1 mb-0.5'
                : level === 5
                  ? 'text-[13px] font-semibold mt-1 mb-0.5'
                  : 'text-[12.5px] font-semibold mt-1 mb-0.5';
      nodes.push(
        <div key={`${keyPrefix}-h-${i}`} role="heading" aria-level={level} className={cls} style={{ color: 'var(--text-main)' }}>
          {content}
        </div>,
      );
      i++;
      continue;
    }

    // GFM Table
    if (line.includes('|') && i + 1 < lines.length && isTableSeparator(lines[i + 1])) {
      const headers = parseTableRow(line);
      const aligns = parseTableAlignments(lines[i + 1]);
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && lines[i].includes('|') && lines[i].trim() !== '') {
        rows.push(parseTableRow(lines[i]));
        i++;
      }
      nodes.push(
        <div
          key={`${keyPrefix}-tbl-${i}`}
          className="overflow-x-auto rounded-[8px] border"
          style={{ borderColor: 'var(--border-line)' }}
        >
          <table className="w-full border-collapse text-[12.5px]">
            <thead>
              <tr style={{ backgroundColor: 'var(--bg-elevated)' }}>
                {headers.map((h, hIdx) => (
                  <th
                    key={hIdx}
                    className="px-3 py-1.5 font-semibold border-b"
                    style={{
                      borderColor: 'var(--border-line)',
                      color: 'var(--text-main)',
                      textAlign: aligns[hIdx] || 'left',
                    }}
                  >
                    {renderInline(h, onOpenFile)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((r, rIdx) => (
                <tr key={rIdx} className="border-t" style={{ borderColor: 'var(--border-line)' }}>
                  {headers.map((_, cIdx) => (
                    <td
                      key={cIdx}
                      className="px-3 py-1.5"
                      style={{
                        color: 'var(--text-body)',
                        textAlign: aligns[cIdx] || 'left',
                      }}
                    >
                      {renderInline(r[cIdx] ?? '', onOpenFile)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>,
      );
      continue;
    }

    // Horizontal Rule (---, ***, ___)
    if (isHorizontalRule(line)) {
      nodes.push(
        <hr
          key={`${keyPrefix}-hr-${i}`}
          className="my-2 border-0 border-t"
          style={{ borderColor: 'var(--border-line)' }}
        />,
      );
      i++;
      continue;
    }

    // Blockquote
    if (/^\s*>\s?/.test(line)) {
      const quoteLines: string[] = [];
      while (i < lines.length && /^\s*>\s?/.test(lines[i])) {
        quoteLines.push(lines[i].replace(/^\s*>\s?/, ''));
        i++;
      }
      nodes.push(
        <blockquote
          key={`${keyPrefix}-bq-${i}`}
          className="m-0 pl-3 border-l-2 space-y-1 italic"
          style={{ borderColor: 'var(--border-strong)', color: 'var(--text-muted)' }}
        >
          {quoteLines.map((ql, qIdx) => (
            <p key={qIdx} className="m-0">
              {renderInline(ql, onOpenFile)}
            </p>
          ))}
        </blockquote>,
      );
      continue;
    }

    // Lists (unordered, ordered, task, and nested)
    if (matchListLine(line)) {
      const list = parseListBlock(lines, i, `${keyPrefix}-list-${i}`, onOpenFile);
      nodes.push(list.node);
      i = list.nextIndex;
      continue;
    }

    // Paragraph (collect consecutive plain lines, preserving soft line breaks)
    const paraLines: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== '' &&
      !/^(#{1,6})\s+/.test(lines[i]) &&
      !isHorizontalRule(lines[i]) &&
      !/^\s*>\s?/.test(lines[i]) &&
      !matchListLine(lines[i]) &&
      !(lines[i].includes('|') && i + 1 < lines.length && isTableSeparator(lines[i + 1]))
    ) {
      paraLines.push(lines[i]);
      i++;
    }
    if (paraLines.length > 0) {
      nodes.push(
        <p key={`${keyPrefix}-p-${i}`} className="m-0">
          {paraLines.map((pl, pIdx) => (
            <Fragment key={pIdx}>
              {pIdx > 0 && <br />}
              {renderInline(pl, onOpenFile)}
            </Fragment>
          ))}
        </p>,
      );
    }
  }

  return nodes;
}

export const SimpleMarkdown = memo(function SimpleMarkdown({
  text,
  stripSakanaUi = false,
  onOpenFile,
}: {
  text: string;
  stripSakanaUi?: boolean;
  onOpenFile?: (path: string) => void;
}) {
  const sourceText = stripSakanaUi
    ? text.replace(/```sakana-ui\b[\s\S]*?(?:```|$)/g, '').replace(/\n{3,}/g, '\n\n').trim()
    : text;

  if (!sourceText) return null;

  const parts = sourceText.split(/(```[\s\S]*?(?:```|$))/g);
  return (
    <div className="selectable-text space-y-2.5 text-[13.5px] leading-relaxed" style={{ color: 'var(--text-body)' }}>
      {parts.map((part, idx) => {
        if (!part) return null;
        if (part.startsWith('```')) {
          const isClosed = part.length >= 6 && part.endsWith('```');
          const inner = isClosed ? part.slice(3, -3) : part.slice(3);
          const lines = inner.split('\n');
          const lang = lines[0]?.trim() || 'code';
          const code = lines.slice(1).join('\n');
          if (lang === 'sakana-action') {
            return (
              <div
                key={idx}
                className="rounded-[8px] border px-3 py-1.5 inline-flex items-center gap-2 font-mono-tech text-[11px]"
                style={{
                  borderColor: 'var(--accent-blue)',
                  backgroundColor: 'var(--accent-blue-bg)',
                  color: 'var(--accent-blue)',
                }}
              >
                <span>✦ Sakana Action Applied</span>
              </div>
            );
          }
          if (lang === 'sakana-peer') {
            let peerText = code.trim();
            try {
              const parsed = JSON.parse(code.trim());
              if (parsed && typeof parsed.message === 'string') {
                peerText = parsed.message;
              }
            } catch {
              // fall back to raw text
            }
            return (
              <div
                key={idx}
                className="rounded-[9px] border px-3 py-2 space-y-1"
                style={{
                  borderColor: 'var(--accent-blue)',
                  backgroundColor: 'var(--accent-blue-bg)',
                }}
              >
                <div
                  className="font-mono-tech text-[10.5px] font-medium flex items-center gap-1.5"
                  style={{ color: 'var(--accent-blue)' }}
                >
                  <span>⇄ Sent to Split-Mode Peer Session</span>
                </div>
                <div className="text-[12.5px] leading-relaxed whitespace-pre-wrap" style={{ color: 'var(--text-main)' }}>
                  {peerText}
                </div>
              </div>
            );
          }
          return (
            <div
              key={idx}
              className="rounded-[8px] border overflow-hidden"
              style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-elevated)' }}
            >
              <div
                className="px-3 py-1.5 border-b flex items-center justify-between font-mono-tech text-[10.5px]"
                style={{ borderColor: 'var(--border-line)', color: 'var(--text-muted)' }}
              >
                <span>{lang}</span>
                <CopyButton
                  text={code}
                  className="inline-flex items-center gap-1 hover:opacity-80 cursor-pointer"
                />
              </div>
              <pre className="p-3 overflow-x-auto font-mono-tech text-[12px] m-0" style={{ color: 'var(--text-main)' }}>
                <code>{highlightCode(code, lang)}</code>
              </pre>
            </div>
          );
        }

        return renderMarkdownBlocks(part, `b-${idx}`, onOpenFile);
      })}
    </div>
  );
});

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function readThemeVar(cs: CSSStyleDeclaration | null, name: string, fallback: string): string {
  const val = cs?.getPropertyValue(name).trim();
  return val || fallback;
}

function buildWidgetSrcDoc(w: InlineWidgetData, theme: ThemeId): string {
  const isDark = theme === 'noir' || theme === 'slate';
  const cs = typeof document !== 'undefined' ? getComputedStyle(document.documentElement) : null;
  const bgApp = readThemeVar(cs, '--bg-app', isDark ? '#0E0E10' : '#FAFAFA');
  const bgCard = readThemeVar(cs, '--bg-card', isDark ? '#17171A' : '#FFFFFF');
  const bgElevated = readThemeVar(cs, '--bg-elevated', isDark ? '#202024' : '#F4F4F5');
  const borderLine = readThemeVar(cs, '--border-line', isDark ? 'rgba(237,237,239,0.1)' : 'rgba(23,23,23,0.09)');
  const textMain = readThemeVar(cs, '--text-main', isDark ? '#EDEDEF' : '#171717');
  const textMuted = readThemeVar(cs, '--text-muted', isDark ? '#787882' : '#737373');
  const accentBlue = readThemeVar(cs, '--accent-blue', isDark ? '#60A5FA' : '#1D6FA5');
  const accentGreen = readThemeVar(cs, '--accent-green', isDark ? '#4CC38A' : '#15803D');
  const accentYellow = readThemeVar(cs, '--accent-yellow', isDark ? '#EAB308' : '#A16207');

  const themeVarsCss = `
    :root {
      color-scheme: ${isDark ? 'dark' : 'light'};
      --bg-app: ${bgApp};
      --bg-card: ${bgCard};
      --bg-elevated: ${bgElevated};
      --border-line: ${borderLine};
      --text-main: ${textMain};
      --text-muted: ${textMuted};
      --accent-blue: ${accentBlue};
      --accent-green: ${accentGreen};
      --accent-yellow: ${accentYellow};
      --bg: var(--bg-app);
      --card: var(--bg-card);
      --elevated: var(--bg-elevated);
      --border: var(--border-line);
      --text: var(--text-main);
      --muted: var(--text-muted);
      --blue: var(--accent-blue);
      --green: var(--accent-green);
      --yellow: var(--accent-yellow);
    }
  `;

  const baseCss = `
    ${themeVarsCss}
    * { box-sizing: border-box; }
    body {
      margin: 0;
      padding: 14px;
      font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
      background: var(--bg-card);
      color: var(--text-main);
      font-size: 12px;
    }
    .metrics { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; margin-bottom: 12px; }
    .metric { background: var(--bg-app); border: 1px solid var(--border-line); border-radius: 8px; padding: 10px; }
    .metric-label { font-size: 11px; color: var(--text-muted); }
    .metric-val { font-size: 16px; font-weight: 600; margin-top: 2px; font-variant-numeric: tabular-nums; }
    .metric-delta { font-family: 'JetBrains Mono', ui-monospace, monospace; font-size: 10px; margin-top: 2px; }
    .bars { display: flex; flex-direction: column; gap: 8px; }
    .bar-head { display: flex; justify-content: space-between; font-size: 11px; margin-bottom: 4px; }
    .bar-ms { font-family: 'JetBrains Mono', ui-monospace, monospace; color: var(--text-muted); }
    .track { height: 6px; border-radius: 999px; background: var(--bg-elevated); overflow: hidden; }
    .fill { height: 100%; border-radius: 999px; }
  `;

  const trimmed = (w.sourceCode || '').trim();
  const isRawHtml = /<(?:!doctype|html|body|div|svg|canvas|section|main|script)\b/i.test(trimmed);
  if (isRawHtml) {
    if (/<html\b/i.test(trimmed)) {
      if (/<head\b[^>]*>/i.test(trimmed)) {
        return trimmed.replace(/<head\b[^>]*>/i, (m) => `${m}<style>${themeVarsCss}</style>`);
      }
      return trimmed.replace(/<html\b[^>]*>/i, (m) => `${m}<head><style>${themeVarsCss}</style></head>`);
    }
    return `<!doctype html><html><head><meta charset="utf-8" /><style>${baseCss}</style></head><body>${trimmed}</body></html>`;
  }

  const metricsHtml = (w.metrics || [])
    .map(
      (m) => `
      <div class="metric">
        <div class="metric-label">${escapeHtml(m.label)}</div>
        <div class="metric-val">${escapeHtml(m.value)}</div>
        <div class="metric-delta" style="color:${m.ok ? 'var(--accent-green)' : 'var(--accent-yellow)'}">${escapeHtml(m.delta)}</div>
      </div>`,
    )
    .join('');

  const barsHtml = (w.bars || [])
    .map(
      (b) => `
      <div>
        <div class="bar-head">
          <span>${escapeHtml(b.label)}</span>
          <span class="bar-ms">${Number(b.ms)}ms</span>
        </div>
        <div class="track">
          <div class="fill" style="width:${Math.max(0, Math.min(100, Number(b.pct)))}%;background:${b.pct === 100 ? 'var(--accent-yellow)' : 'var(--accent-blue)'}"></div>
        </div>
      </div>`,
    )
    .join('');

  return `<!doctype html><html><head><meta charset="utf-8" /><style>${baseCss}</style></head><body>
    <div class="metrics">${metricsHtml}</div>
    <div class="bars">${barsHtml}</div>
  </body></html>`;
}

function SakanaUiCard({ msg, theme = 'paper' }: { msg: ChatMessage; theme?: ThemeId }) {
  const [viewMode, setViewMode] = useState<'preview' | 'source'>('preview');
  const [iframeHeight, setIframeHeight] = useState(210);
  const w = msg.widget;
  if (!w) return null;

  return (
    <div
      className="rounded-[10px] border overflow-hidden mt-2"
      style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-card)' }}
    >
      <div
        className="px-3.5 py-2 border-b flex items-center justify-between gap-2"
        style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-elevated)' }}
      >
        <div className="min-w-0">
          <div className="text-[12px] font-medium truncate" style={{ color: 'var(--text-main)' }}>
            {w.title}
          </div>
          <div className="font-mono-tech text-[10px]" style={{ color: 'var(--text-muted)' }}>
            {w.subtitle}
          </div>
        </div>
        <div
          className="grid grid-cols-2 p-0.5 rounded-[6px] border text-[10.5px] shrink-0"
          style={{ backgroundColor: 'var(--bg-app)', borderColor: 'var(--border-line)' }}
        >
          <button
            type="button"
            onClick={() => setViewMode('preview')}
            className="px-2 py-0.5 rounded-[4px] inline-flex items-center gap-1 cursor-pointer"
            style={
              viewMode === 'preview'
                ? { backgroundColor: 'var(--btn-solid-bg)', color: 'var(--btn-solid-text)' }
                : { color: 'var(--text-muted)' }
            }
          >
            <Eye size={11} />
            <span>Preview</span>
          </button>
          <button
            type="button"
            onClick={() => setViewMode('source')}
            className="px-2 py-0.5 rounded-[4px] inline-flex items-center gap-1 cursor-pointer"
            style={
              viewMode === 'source'
                ? { backgroundColor: 'var(--btn-solid-bg)', color: 'var(--btn-solid-text)' }
                : { color: 'var(--text-muted)' }
            }
          >
            <Code size={11} />
            <span>Source</span>
          </button>
        </div>
      </div>

      {viewMode === 'preview' ? (
        <iframe
          sandbox="allow-scripts allow-same-origin"
          srcDoc={buildWidgetSrcDoc(w, theme)}
          title={w.title}
          onLoad={(e) => {
            try {
              const h = e.currentTarget.contentDocument?.body?.scrollHeight;
              if (typeof h === 'number' && h > 0) {
                setIframeHeight(Math.max(140, Math.min(420, h + 4)));
              }
            } catch {
              // Ignore cross-origin access errors
            }
          }}
          className="w-full border-0 block"
          style={{ backgroundColor: 'var(--bg-card)', height: `${iframeHeight}px` }}
        />
      ) : (
        <pre
          className="p-3.5 font-mono-tech text-[11.5px] overflow-x-auto m-0"
          style={{ color: 'var(--text-main)', backgroundColor: 'var(--bg-app)' }}
        >
          <code>{w.sourceCode}</code>
        </pre>
      )}
    </div>
  );
}

function formatEffortLabel(ef: string): string {
  switch (ef.toLowerCase()) {
    case 'low':
      return 'Low';
    case 'medium':
      return 'Medium';
    case 'high':
      return 'High';
    case 'xhigh':
      return 'XHigh';
    case 'max':
      return 'Max';
    default:
      return ef.charAt(0).toUpperCase() + ef.slice(1);
  }
}

const VALID_EFFORTS: ReasoningEffort[] = ['low', 'medium', 'high', 'xhigh'];

function resolveNextEffort(
  modelEfforts: string[] | undefined,
  currentEffort: ReasoningEffort,
): ReasoningEffort | undefined {
  if (modelEfforts === undefined) {
    return VALID_EFFORTS.includes(currentEffort) ? undefined : 'medium';
  }
  const valid = modelEfforts.filter((e): e is ReasoningEffort =>
    VALID_EFFORTS.includes(e as ReasoningEffort),
  );
  if (valid.length === 0) {
    return '';
  }
  if (!valid.includes(currentEffort)) {
    return valid.includes('medium') ? 'medium' : valid[0];
  }
  return undefined;
}

interface MessageRowProps {
  m: ChatMessage;
  idx: number;
  threadId: string;
  harness: HarnessId;
  isDiscord?: boolean;
  traceOpen: boolean;
  theme: ThemeId;
  onToggleTrace: (msgId: string) => void;
  onForkAtMessage: (threadId: string, msgIndex: number) => void;
  onViewCheckpointDiff: (checkpointId: string) => void;
  onRevertToCheckpoint: (threadId: string, checkpointId: string) => void;
  onToast: (msg: string) => void;
  onOpenFile?: (path: string) => void;
}

const MessageRow = memo(function MessageRow({
  m,
  idx,
  threadId,
  harness,
  isDiscord,
  traceOpen,
  theme,
  onToggleTrace,
  onForkAtMessage,
  onViewCheckpointDiff,
  onRevertToCheckpoint,
  onToast,
  onOpenFile,
}: MessageRowProps) {
  if (m.role === 'user') {
    return (
      <div className="group flex flex-col items-end gap-1 msg-enter" data-turn-id={m.id}>
        <div className="flex items-center gap-2 text-[10.5px] font-mono-tech" style={{ color: 'var(--text-muted)' }}>
          {isDiscord && m.author && m.author.toLowerCase() !== 'you' && (
            <span
              className="px-1.5 py-0.5 rounded-[5px] inline-flex items-center gap-1"
              style={{ backgroundColor: 'var(--accent-discord-bg)', color: 'var(--accent-discord)' }}
            >
              <DiscordIcon size={10} />
              {m.author}
            </span>
          )}
          <span>{m.timestamp}</span>
        </div>
        <div
          className="max-w-[80%] rounded-[14px] px-4 py-2.5 text-[13.5px]"
          style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-main)' }}
        >
          <SimpleMarkdown text={m.text} stripSakanaUi={Boolean(m.widget)} onOpenFile={onOpenFile} />
        </div>
        <div className="opacity-0 group-hover:opacity-100 transition-opacity flex items-center gap-1 text-[10.5px]" style={{ color: 'var(--text-muted)' }}>
          <CopyButton
            text={m.text}
            onCopied={() => onToast('Copied prompt')}
            className="px-1.5 py-0.5 rounded hover:bg-[var(--bg-hover)] cursor-pointer inline-flex items-center gap-1"
          />
          <button
            type="button"
            onClick={() => onForkAtMessage(threadId, idx)}
            className="px-1.5 py-0.5 rounded hover:bg-[var(--bg-hover)] cursor-pointer inline-flex items-center gap-1"
          >
            <GitFork size={11} />
            <span>Fork</span>
          </button>
        </div>
      </div>
    );
  }

  const tools = m.tools ?? [];
  const runningTools = tools.filter((t) => t.status === 'running');
  const errorCount = tools.filter((t) => t.status === 'error').length;

  return (
    <div className="group space-y-2 msg-enter" data-turn-id={m.id}>
      <div
        className="flex items-center gap-1.5 text-[11px]"
        style={{ color: 'var(--text-muted)' }}
        title={HARNESS_META[harness].name}
      >
        <HarnessIcon harness={harness} size={14} />
        <span className="font-mono-tech text-[10px]">{m.timestamp}</span>
      </div>

      {/* Collapsible Thinking & Tool Timeline (single source of truth for tools) */}
      {(m.thinking || tools.length > 0) && (
        <div className="pl-1">
          <button
            type="button"
            onClick={() => onToggleTrace(m.id)}
            className="inline-flex items-center gap-1.5 text-[11.5px] font-mono-tech cursor-pointer hover:opacity-80"
            style={{ color: 'var(--text-muted)' }}
          >
            {traceOpen ? <CaretDown size={11} /> : <CaretRight size={11} />}
            <span>
              Thought {m.thinkingDuration || '2.0s'} · {tools.length} tool call
              {tools.length === 1 ? '' : 's'}
            </span>
            {errorCount > 0 && (
              <span style={{ color: 'var(--accent-red)' }}>
                · {errorCount} error{errorCount === 1 ? '' : 's'}
              </span>
            )}
          </button>

          {traceOpen && (
            <div
              className="mt-1.5 ml-1.5 pl-3 border-l space-y-2"
              style={{ borderColor: 'var(--border-line)' }}
            >
              {m.thinking && (
                <p className="text-[12px] m-0 leading-relaxed" style={{ color: 'var(--text-muted)' }}>
                  {m.thinking}
                </p>
              )}
              {tools.map((tool) => (
                <div
                  key={tool.id}
                  className="flex items-center justify-between gap-2 px-2 py-1 rounded-[6px] font-mono-tech text-[11px]"
                  style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-body)' }}
                >
                  <span className="truncate inline-flex items-center gap-1.5">
                    {tool.status === 'running' ? (
                      <span
                        className="w-1.5 h-1.5 rounded-full pulse-dot inline-block shrink-0"
                        style={{ backgroundColor: 'var(--accent-blue)' }}
                      />
                    ) : (
                      <span
                        className="shrink-0"
                        style={{
                          color: tool.status === 'error' ? 'var(--accent-red)' : 'var(--accent-green)',
                        }}
                      >
                        {tool.status === 'error' ? '✕' : '✓'}
                      </span>
                    )}
                    <span style={{ color: 'var(--text-main)' }}>{tool.name}</span>
                    {tool.target && (
                      onOpenFile && isLikelyFilePath(tool.target) ? (
                        <button
                          type="button"
                          onClick={() => onOpenFile(tool.target)}
                          className="truncate hover:underline cursor-pointer"
                          style={{ color: 'var(--accent-blue)' }}
                          title={`Open ${tool.target} in Sakana sidebar`}
                        >
                          {tool.target}
                        </button>
                      ) : (
                        <span className="truncate" style={{ color: 'var(--text-muted)' }}>
                          {tool.target}
                        </span>
                      )
                    )}
                  </span>
                  {tool.durationMs > 0 && (
                    <span className="shrink-0" style={{ color: 'var(--text-muted)' }}>
                      {tool.durationMs}ms
                    </span>
                  )}
                </div>
              ))}
            </div>
          )}

          {/* Show live pills only for actively running tools while timeline is collapsed */}
          {!traceOpen && runningTools.length > 0 && (
            <div className="flex flex-wrap gap-1.5 mt-1.5">
              {runningTools.map((tool) => (
                <span
                  key={tool.id}
                  className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-[6px] border font-mono-tech text-[10.5px]"
                  style={{
                    borderColor: 'var(--border-line)',
                    backgroundColor: 'var(--bg-elevated)',
                    color: 'var(--text-body)',
                  }}
                >
                  <span
                    className="w-1.5 h-1.5 rounded-full pulse-dot"
                    style={{ backgroundColor: 'var(--accent-blue)' }}
                  />
                  <span style={{ color: 'var(--text-main)' }}>{tool.name}</span>
                  {tool.target && (
                    <span className="truncate max-w-[180px]" style={{ color: 'var(--text-muted)' }}>
                      {tool.target}
                    </span>
                  )}
                </span>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Subagent Card */}
      {m.subagent && (
        <div
          className="rounded-[8px] border px-3 py-2 flex items-start justify-between gap-3 text-[12px]"
          style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-elevated)' }}
        >
          <div className="space-y-0.5 min-w-0">
            <div className="inline-flex items-center gap-1.5 font-mono-tech text-[11px]" style={{ color: 'var(--text-main)' }}>
              <HarnessIcon harness={m.subagent.harness} size={12} />
              <span>{m.subagent.command}</span>
            </div>
            <p className="m-0 text-[11.5px]" style={{ color: 'var(--text-body)' }}>
              {m.subagent.summary}
            </p>
          </div>
          <span className="font-mono-tech text-[10.5px] shrink-0" style={{ color: 'var(--text-muted)' }}>
            {(m.subagent.durationMs / 1000).toFixed(1)}s
          </span>
        </div>
      )}

      <SimpleMarkdown text={m.text} stripSakanaUi={Boolean(m.widget)} onOpenFile={onOpenFile} />

      {/* Inline sakana-ui Widget */}
      {m.widget && <SakanaUiCard msg={m} theme={theme} />}

      {/* Per-Turn Checkpoint Bar */}
      {m.checkpoint && (
        <div
          className="rounded-[8px] border px-3 py-1.5 flex flex-wrap items-center justify-between gap-2 text-[11px]"
          style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
        >
          <div className="flex items-center gap-2 font-mono-tech min-w-0">
            <span
              className="px-1.5 py-0.5 rounded-[4px]"
              style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-main)' }}
            >
              Checkpoint #{m.checkpoint.turnNumber}
            </span>
            <span style={{ color: 'var(--text-muted)' }}>{m.checkpoint.commitHash}</span>
            <span className="truncate font-sans" style={{ color: 'var(--text-body)' }}>
              {m.checkpoint.filesChanged} files (
              <span style={{ color: 'var(--accent-green)' }}>+{m.checkpoint.additions}</span>{' '}
              <span style={{ color: 'var(--accent-red)' }}>-{m.checkpoint.deletions}</span>)
            </span>
          </div>
          <div className="flex items-center gap-1.5 shrink-0">
            <button
              type="button"
              onClick={() => onViewCheckpointDiff(m.checkpoint!.id)}
              className="btn-ghost px-2 py-0.5 text-[10.5px] font-mono-tech cursor-pointer"
            >
              View Diff
            </button>
            <button
              type="button"
              onClick={() => onRevertToCheckpoint(threadId, m.checkpoint!.id)}
              className="btn-ghost px-2 py-0.5 text-[10.5px] font-mono-tech inline-flex items-center gap-1 cursor-pointer"
            >
              <ArrowCounterClockwise size={11} />
              <span>Revert</span>
            </button>
          </div>
        </div>
      )}

      {/* Turn Hover Actions */}
      <div className="opacity-0 group-hover:opacity-100 transition-opacity flex items-center gap-1 text-[10.5px]" style={{ color: 'var(--text-muted)' }}>
        <CopyButton
          text={m.text}
          onCopied={() => onToast('Copied response')}
          className="px-1.5 py-0.5 rounded hover:bg-[var(--bg-hover)] cursor-pointer inline-flex items-center gap-1"
        />
        <button
          type="button"
          onClick={() => onForkAtMessage(threadId, idx)}
          className="px-1.5 py-0.5 rounded hover:bg-[var(--bg-hover)] cursor-pointer inline-flex items-center gap-1"
        >
          <GitFork size={11} />
          <span>Fork from here</span>
        </button>
      </div>
    </div>
  );
});

function ApprovalCard({
  approval,
  threadId,
  onResolveApproval,
}: {
  approval: NonNullable<Thread['pendingApproval']>;
  threadId: string;
  onResolveApproval: ChatPaneProps['onResolveApproval'];
}) {
  return (
    <div
      className="sakana-card rounded-[12px] p-4 space-y-3 pop-enter"
      style={{ borderColor: 'var(--accent-yellow)' }}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex items-center gap-2">
          <ShieldWarning size={16} weight="fill" style={{ color: 'var(--accent-yellow)' }} />
          <div>
            <div className="text-[13px] font-semibold" style={{ color: 'var(--text-main)' }}>
              Terminal Guard Approval Required
            </div>
            <div className="text-[11.5px]" style={{ color: 'var(--text-muted)' }}>
              {approval.reason}
            </div>
          </div>
        </div>
        <span
          className="font-mono-tech text-[10px] px-2 py-0.5 rounded-[5px] uppercase"
          style={{ backgroundColor: 'var(--accent-yellow-bg)', color: 'var(--accent-yellow)' }}
        >
          {approval.risk} risk
        </span>
      </div>

      <pre
        className="p-2.5 rounded-[8px] font-mono-tech text-[12px] overflow-x-auto m-0"
        style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-main)' }}
      >
        <code>$ {approval.command}</code>
      </pre>

      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={() => onResolveApproval(threadId, 'approve_once')}
          className="btn-solid px-3 py-1.5 text-[11.5px] font-medium cursor-pointer"
        >
          Approve Once
        </button>
        <button
          type="button"
          onClick={() => onResolveApproval(threadId, 'always_allow')}
          className="btn-ghost px-3 py-1.5 text-[11.5px] font-medium cursor-pointer"
        >
          Approve + Allowlist
        </button>
        <button
          type="button"
          onClick={() => onResolveApproval(threadId, 'enable_yolo')}
          className="px-3 py-1.5 rounded-[8px] text-[11.5px] font-medium cursor-pointer"
          style={{ backgroundColor: 'var(--accent-red-bg)', color: 'var(--accent-red)' }}
        >
          Approve &amp; YOLO
        </button>
        <button
          type="button"
          onClick={() => onResolveApproval(threadId, 'deny')}
          className="btn-ghost px-3 py-1.5 text-[11.5px] font-medium cursor-pointer ml-auto"
          style={{ color: 'var(--accent-red)' }}
        >
          Deny
        </button>
      </div>
    </div>
  );
}

export function ChatPane({
  thread,
  secondaryThread,
  secondaryThreadId,
  onSelectSecondaryThread,
  onQuickNewThread,
  project,
  allThreads,
  splitView,
  onToggleSplitView,
  leftSidebarOpen,
  onToggleLeftSidebar,
  rightInspectorOpen,
  onToggleRightInspector,
  onSelectThread,
  onSendPrompt,
  onStopStreaming,
  onUpdateThreadConfig,
  onResolveApproval,
  onRevertToCheckpoint,
  onViewCheckpointDiff,
  onForkAtMessage,
  attachedChips,
  onRemoveChip,
  onAddChips,
  onPickAttachments,
  queuedPrompts,
  onSendQueuedNow,
  onRemoveQueued,
  onOpenCommandPalette,
  harnessModels,
  harnessAccounts = [],
  onOpenHarnessAccounts,
  disabledHarnesses = [],
  onToast,
  onOpenFile,
  splitBridgeEnabled = true,
  onToggleSplitBridge,
  skills = [],
  theme = 'paper',
}: ChatPaneProps) {
  const [input, setInput] = useState('');
  const [secondaryInput, setSecondaryInput] = useState('');
  const [openMenu, setOpenMenu] = useState<'harness' | 'model' | 'effort' | 'perm' | 'account' | null>(null);
  const composerToolbarRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!openMenu) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        setOpenMenu(null);
      }
    };
    const onMouseDown = (e: MouseEvent) => {
      if (composerToolbarRef.current && !composerToolbarRef.current.contains(e.target as Node)) {
        setOpenMenu(null);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    document.addEventListener('mousedown', onMouseDown);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('mousedown', onMouseDown);
    };
  }, [openMenu]);
  const [expandedTraces, setExpandedTraces] = useState<Record<string, boolean>>({});
  const [skillIdx, setSkillIdx] = useState(0);
  const [slashDismissed, setSlashDismissed] = useState(false);
  const [showScrollBottom, setShowScrollBottom] = useState(false);
  const timelineCloseTimerRef = useRef<number | null>(null);

  const openTimeline = () => {
    if (timelineCloseTimerRef.current !== null) {
      window.clearTimeout(timelineCloseTimerRef.current);
      timelineCloseTimerRef.current = null;
    }
    setTimelineOpen(true);
  };

  const scheduleCloseTimeline = () => {
    if (timelineCloseTimerRef.current !== null) {
      window.clearTimeout(timelineCloseTimerRef.current);
    }
    timelineCloseTimerRef.current = window.setTimeout(() => {
      setTimelineOpen(false);
      setHoveredTurnId(null);
      timelineCloseTimerRef.current = null;
    }, 180);
  };
  const [timelineOpen, setTimelineOpen] = useState(false);
  const [hoveredTurnId, setHoveredTurnId] = useState<string | null>(null);
  const [activeTurnId, setActiveTurnId] = useState<string | null>(null);
  const [isDraggingFile, setIsDraggingFile] = useState(false);
  const feedRef = useRef<HTMLDivElement>(null);
  const isNearBottomRef = useRef(true);
  const prevThreadIdRef = useRef<string | undefined>(thread?.id);
  const secondaryFeedRef = useRef<HTMLDivElement>(null);
  const secondaryNearBottomRef = useRef(true);
  const prevSecondaryThreadIdRef = useRef<string | undefined>(secondaryThread?.id);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const secondaryTextareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const formatBytes = (bytes?: number) => {
    if (bytes === undefined || bytes === null) return '';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const readFileAsDataUrl = (file: File) =>
    new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(typeof reader.result === 'string' ? reader.result : '');
      reader.onerror = () => reject(reader.error ?? new Error('Failed to read file'));
      reader.readAsDataURL(file);
    });

  const classifyAttachmentKind = (file: File): 'image' | 'pdf' | 'file' => {
    const ext = file.name.split('.').pop()?.toLowerCase() ?? '';
    if (file.type.startsWith('image/') || ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg'].includes(ext)) {
      return 'image';
    }
    if (file.type === 'application/pdf' || ext === 'pdf') {
      return 'pdf';
    }
    return 'file';
  };

  const processAttachmentFiles = async (files: File[]) => {
    if (files.length === 0) return;
    if (!onAddChips) {
      onToast('Attachment handler is not wired');
      return;
    }
    const chips: AttachedContextChip[] = [];
    for (const file of files) {
      const fallbackName = file.name || `attachment-${Date.now()}`;
      const localKind = classifyAttachmentKind(file);
      try {
        const dataUrl = await readFileAsDataUrl(file);
        if (isTauriRuntime()) {
          const saved = await tauriApi.saveClipboardAttachment(fallbackName, dataUrl);
          if (saved) {
            chips.push({
              id: `att-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
              label: saved.name,
              payload: saved.path,
              path: saved.path,
              kind: saved.kind,
              sizeBytes: saved.sizeBytes,
              previewDataUrl: saved.previewDataUrl ?? (saved.kind === 'image' ? dataUrl : undefined),
            });
            continue;
          }
        }
        chips.push({
          id: `att-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
          label: fallbackName,
          payload: fallbackName,
          kind: localKind,
          sizeBytes: file.size,
          previewDataUrl: localKind === 'image' ? dataUrl : undefined,
        });
      } catch (err) {
        onToast(`Failed to attach ${fallbackName}: ${err instanceof Error ? err.message : String(err)}`);
      }
    }
    if (chips.length > 0) {
      onAddChips(chips);
    }
  };

  const handleFeedScroll = () => {
    const el = feedRef.current;
    if (!el) return;
    const near = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
    isNearBottomRef.current = near;
    setShowScrollBottom(!near);
  };

  useEffect(() => {
    const el = feedRef.current;
    if (!el || !thread?.id) return;
    if (prevThreadIdRef.current !== thread.id) {
      prevThreadIdRef.current = thread.id;
      isNearBottomRef.current = true;
      setShowScrollBottom(false);
      el.scrollTo({ top: el.scrollHeight, behavior: 'auto' });
      return;
    }
    if (isNearBottomRef.current) {
      el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
    }
  }, [thread?.id, thread?.messages, thread?.status]);

  const handleSecondaryFeedScroll = () => {
    const el = secondaryFeedRef.current;
    if (!el) return;
    secondaryNearBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
  };

  useEffect(() => {
    const el = secondaryFeedRef.current;
    if (!el || !secondaryThread?.id) return;
    if (prevSecondaryThreadIdRef.current !== secondaryThread.id) {
      prevSecondaryThreadIdRef.current = secondaryThread.id;
      secondaryNearBottomRef.current = true;
      el.scrollTo({ top: el.scrollHeight, behavior: 'auto' });
      return;
    }
    if (secondaryNearBottomRef.current) {
      el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
    }
  }, [secondaryThread?.id, secondaryThread?.messages, secondaryThread?.status]);

  const autoGrowComposer = (el: HTMLTextAreaElement | null) => {
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, 168)}px`;
  };

  const builtInCommands: SkillItem[] = [
    { slug: 'verify', scope: 'global', description: 'Run cross-harness /verify check' },
    { slug: 'delegate', scope: 'global', description: 'Delegate subtask via /delegate' },
  ];
  const allSkills: SkillItem[] = [
    ...skills,
    ...builtInCommands.filter((b) => !skills.some((s) => s.slug === b.slug)),
  ];
  const slashMatch = input.match(/^\/([a-zA-Z0-9_-]*)$/);
  const filteredSkills =
    slashMatch && !slashDismissed
      ? allSkills.filter((s) =>
          s.slug.toLowerCase().includes(slashMatch[1].toLowerCase()),
        )
      : [];
  const showSkillMenu = filteredSkills.length > 0;

  const activeFleet = allThreads.filter(
    (t) => t.status === 'RUNNING' || t.status === 'AWAITING_APPROVAL',
  );

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = input.trim();
    if (!trimmed || !thread || thread.id === 'empty') return;
    onSendPrompt(thread.id, trimmed);
    setInput('');
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  };

  const toggleTrace = useCallback((msgId: string) => {
    setExpandedTraces((prev) => ({ ...prev, [msgId]: !prev[msgId] }));
  }, []);

  const handleSecondarySubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = secondaryInput.trim();
    if (!trimmed || !secondaryThread) return;
    onSendPrompt(secondaryThread.id, trimmed);
    setSecondaryInput('');
    if (secondaryTextareaRef.current) {
      secondaryTextareaRef.current.style.height = 'auto';
    }
  };

  const permLabel: Record<PermissionMode, string> = {
    supervised: 'Supervised',
    'auto-edit': 'Auto-Edit',
    yolo: 'YOLO',
  };

  const availableModels: { id: string; label: string; family: string; efforts?: string[] }[] =
    harnessModels?.[thread.harness]?.length
      ? harnessModels[thread.harness].map((m) => ({
          id: m.id,
          label: m.label || m.id,
          family: m.family || '',
          efforts: m.efforts,
        }))
      : HARNESS_META[thread.harness].models.map((id) => ({ id, label: id, family: '' }));

  const resolveModelGroup = (mod: { id: string; family: string }) => {
    const f = mod.family.toLowerCase();
    const id = mod.id.toLowerCase();
    if (f === 'gemini' || id.startsWith('gemini')) return 'Gemini';
    if (f === 'claude' || id.startsWith('claude')) return 'Claude';
    if (f === 'openai' || id.startsWith('gpt') || id.startsWith('o1') || id.startsWith('o3')) return 'GPT';
    if (f === 'deepseek' || id.startsWith('deepseek')) return 'DeepSeek';
    if (f === 'qwen' || id.startsWith('qwen')) return 'Qwen';
    if (f === 'auto' || id === 'auto') return 'Auto';
    return 'Other';
  };

  const groupOrder = ['Gemini', 'Claude', 'GPT', 'DeepSeek', 'Qwen', 'Auto', 'Other'];
  const groupedModels = groupOrder
    .map((group) => ({
      group,
      items: availableModels.filter((m) => resolveModelGroup(m) === group),
    }))
    .filter((g) => g.items.length > 0);

  const currentModelEntry = availableModels.find((m) => m.id === thread.model);
  const availableEfforts: ReasoningEffort[] =
    currentModelEntry?.efforts !== undefined
      ? (currentModelEntry.efforts.filter((e): e is ReasoningEffort =>
          ['low', 'medium', 'high', 'xhigh'].includes(e),
        ) as ReasoningEffort[])
      : ['low', 'medium', 'high', 'xhigh'];

  if (!thread || thread.id === 'empty' || allThreads.length === 0) {
    return (
      <div className="flex-1 min-w-0 h-full flex flex-col overflow-hidden relative" style={{ backgroundColor: 'var(--bg-canvas)' }}>
        <header
          className="h-12 px-4 border-b flex items-center justify-between gap-3 shrink-0"
          style={{ borderColor: 'var(--border-line)' }}
        >
          <div className="flex items-center gap-2 min-w-0">
            {!leftSidebarOpen && (
              <button
                type="button"
                onClick={onToggleLeftSidebar}
                className="btn-ghost p-1.5 cursor-pointer"
                title="Show Sidebar (Ctrl+B)"
                aria-label="Show sidebar"
              >
                <SidebarSimple size={14} />
              </button>
            )}
            <span className="text-[13px] font-medium" style={{ color: 'var(--text-main)' }}>
              Sakana
            </span>
          </div>
          <button
            type="button"
            onClick={onToggleRightInspector}
            className="btn-ghost p-1.5 cursor-pointer"
            title="Toggle Right Inspector (Ctrl+I)"
            aria-label="Toggle right inspector"
          >
            <SidebarSimple size={14} className="rotate-180" />
          </button>
        </header>
        <div className="flex-1 flex items-center justify-center p-8">
          <div className="max-w-sm text-center space-y-2.5">
            <div className="text-[15px] font-semibold" style={{ color: 'var(--text-main)' }}>
              No thread selected
            </div>
            <p className="text-[13px] m-0 leading-relaxed" style={{ color: 'var(--text-muted)' }}>
              Create a project or start a new worktree thread from the sidebar to begin chatting.
            </p>
            {onOpenCommandPalette && (
              <button
                type="button"
                onClick={() => onOpenCommandPalette?.()}
                className="btn-ghost px-3 py-1.5 text-[12px] font-medium inline-flex items-center gap-1.5 cursor-pointer mt-1"
              >
                <span>Open Command Palette</span>
                <span className="sakana-kbd">Ctrl+K</span>
              </button>
            )}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 min-w-0 h-full flex flex-col overflow-hidden relative" style={{ backgroundColor: 'var(--bg-canvas)' }}>
      {/* Top Bar */}
      <header
        className="h-12 px-4 border-b flex items-center justify-between gap-3 shrink-0"
        style={{ borderColor: 'var(--border-line)' }}
      >
        <div className="flex items-center gap-2 min-w-0">
          {!leftSidebarOpen && (
            <button
              type="button"
              onClick={onToggleLeftSidebar}
              className="btn-ghost p-1.5 cursor-pointer"
              title="Show Sidebar (Ctrl+B)"
              aria-label="Show sidebar"
            >
              <SidebarSimple size={14} />
            </button>
          )}
          <span className="text-[12.5px] font-medium truncate" style={{ color: 'var(--text-muted)' }}>
            {project?.name || 'project'}
          </span>
          <span style={{ color: 'var(--border-strong)' }}>/</span>
          <span className="text-[13px] font-medium truncate" style={{ color: 'var(--text-main)' }}>
            {thread.title}
          </span>
          <span
            className="hidden sm:inline-flex items-center gap-1 font-mono-tech text-[10.5px] px-2 py-0.5 rounded-[6px] border shrink-0"
            style={{
              backgroundColor: 'var(--bg-elevated)',
              borderColor: 'var(--border-line)',
              color: 'var(--text-body)',
            }}
            title={`Worktree: ${thread.worktreePath}`}
          >
            <GitBranch size={11} />
            <span>{thread.branch}</span>
          </span>
        </div>

        <div className="flex items-center gap-1.5 shrink-0">
          <button
            type="button"
            onClick={onToggleSplitView}
            className="btn-ghost px-2 py-1 text-[11px] font-medium inline-flex items-center gap-1.5 cursor-pointer"
            style={
              splitView
                ? { backgroundColor: 'var(--bg-elevated)', color: 'var(--text-main)', borderColor: 'var(--border-strong)' }
                : undefined
            }
            title="Toggle 2-Thread Split View"
            aria-label="Toggle split view"
          >
            <Columns size={13} />
            <span className="hidden md:inline">Split</span>
          </button>
          <button
            type="button"
            onClick={onToggleRightInspector}
            className="btn-ghost p-1.5 cursor-pointer"
            style={
              rightInspectorOpen
                ? { backgroundColor: 'var(--bg-elevated)', color: 'var(--text-main)', borderColor: 'var(--border-strong)' }
                : undefined
            }
            title="Toggle Right Inspector (Ctrl+I)"
            aria-label="Toggle right inspector"
          >
            <SidebarSimple size={14} className="rotate-180" />
          </button>
        </div>
      </header>

      {/* Active Fleet Strip (Orca-inspired parallel visibility) */}
      {activeFleet.length > 0 && (
        <div
          className="px-4 py-1.5 border-b flex items-center gap-2 overflow-x-auto shrink-0 text-[11px]"
          style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
        >
          <span className="font-mono-tech text-[10px] shrink-0" style={{ color: 'var(--text-muted)' }}>
            ACTIVE FLEET ({activeFleet.length})
          </span>
          {activeFleet.map((ft) => (
            <button
              key={ft.id}
              type="button"
              onClick={() => onSelectThread(ft.id)}
              className="px-2 py-0.5 rounded-[6px] border inline-flex items-center gap-1.5 shrink-0 cursor-pointer"
              style={{
                borderColor: ft.id === thread.id ? 'var(--border-strong)' : 'var(--border-line)',
                backgroundColor: ft.id === thread.id ? 'var(--bg-card)' : 'transparent',
                color: 'var(--text-main)',
              }}
            >
              <span
                className="w-1.5 h-1.5 rounded-full pulse-dot"
                style={{
                  backgroundColor:
                    ft.status === 'AWAITING_APPROVAL' ? 'var(--accent-yellow)' : 'var(--accent-blue)',
                }}
              />
              <span className="truncate max-w-[160px]">{ft.title}</span>
              <span className="font-mono-tech text-[10px]" style={{ color: 'var(--text-muted)' }}>
                {ft.branch}
              </span>
            </button>
          ))}
        </div>
      )}

      {/* Feed Area (Single or Split View) */}
      <div className="flex-1 min-h-0 flex overflow-hidden relative">
        {/* Primary Feed + Minimap */}
        <div className="relative flex-1 min-w-0 flex overflow-hidden">
          <div ref={feedRef} onScroll={handleFeedScroll} className="flex-1 min-w-0 overflow-y-auto px-5 sm:px-8 py-6">
            <div className="max-w-3xl mx-auto space-y-6">
              {thread.messages.map((m, idx) => (
                <MessageRow
                  key={m.id}
                  m={m}
                  idx={idx}
                  threadId={thread.id}
                  harness={thread.harness}
                  isDiscord={thread.isDiscord}
                  traceOpen={!!expandedTraces[m.id]}
                  theme={theme}
                  onToggleTrace={toggleTrace}
                  onForkAtMessage={onForkAtMessage}
                  onViewCheckpointDiff={onViewCheckpointDiff}
                  onRevertToCheckpoint={onRevertToCheckpoint}
                  onToast={onToast}
                  onOpenFile={onOpenFile}
                />
              ))}

              {/* Inline Terminal Guard Approval */}
              {thread.pendingApproval && (
                <ApprovalCard
                  approval={thread.pendingApproval}
                  threadId={thread.id}
                  onResolveApproval={onResolveApproval}
                />
              )}

              {thread.status === 'RUNNING' && (
                <div className="flex items-center gap-2 text-[12px] font-mono-tech" style={{ color: 'var(--text-muted)' }}>
                  <span className="w-2 h-2 rounded-full pulse-dot" style={{ backgroundColor: 'var(--accent-blue)' }} />
                  <span>{HARNESS_META[thread.harness].name} is working in {thread.branch}...</span>
                </div>
              )}
            </div>
          </div>

          {/* Floating Turn Minimap Scrubber + Timeline Viewer Popover */}
          {thread.messages.length > 0 && (
            <div
              className="hidden lg:flex flex-col items-end gap-1.5 absolute right-3 top-1/2 -translate-y-1/2 z-20 py-2 pl-3"
              onMouseEnter={openTimeline}
              onMouseLeave={scheduleCloseTimeline}
            >
              {timelineOpen && (
                <div
                  className="absolute right-full top-1/2 -translate-y-1/2 pr-3 z-30"
                  onMouseEnter={openTimeline}
                  onMouseLeave={scheduleCloseTimeline}
                >
                  <div className="w-72 max-h-80 overflow-y-auto rounded-[10px] p-1.5 space-y-0.5 sakana-popover pop-enter">
                  <div
                    className="px-2 py-1 text-[10px] font-mono-tech uppercase flex items-center justify-between"
                    style={{ color: 'var(--text-muted)' }}
                  >
                    <span>Timeline</span>
                    <span>{thread.messages.length} turns</span>
                  </div>
                  {thread.messages.map((m) => {
                    const isUser = m.role === 'user';
                    const isHighlighted = hoveredTurnId === m.id || activeTurnId === m.id;
                    const snippet =
                      m.text.trim().replace(/\s+/g, ' ') ||
                      (m.tools?.length ? `${m.tools.length} tool call(s)` : '(empty)');
                    return (
                      <button
                        key={m.id}
                        type="button"
                        onMouseEnter={() => setHoveredTurnId(m.id)}
                        onClick={() => {
                          setActiveTurnId(m.id);
                          const el = feedRef.current?.querySelector(`[data-turn-id="${m.id}"]`);
                          el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
                        }}
                        className="w-full px-2 py-1.5 rounded-[7px] text-left flex items-start gap-2 cursor-pointer transition-colors"
                        style={{
                          backgroundColor: isHighlighted ? 'var(--bg-hover)' : 'transparent',
                          color: 'var(--text-main)',
                        }}
                      >
                        <span
                          className="mt-0.5 w-4 h-4 rounded-[4px] flex items-center justify-center shrink-0"
                          style={{
                            backgroundColor: isUser ? 'var(--accent-blue-bg)' : 'var(--bg-elevated)',
                            color: isUser ? 'var(--accent-blue)' : 'var(--text-muted)',
                          }}
                        >
                          {isUser ? <User size={10} weight="bold" /> : <Robot size={10} weight="bold" />}
                        </span>
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center justify-between gap-2 text-[10px] font-mono-tech">
                            <span className="font-medium truncate" style={{ color: isUser ? 'var(--accent-blue)' : 'var(--text-main)' }}>
                              {isUser ? 'You' : m.author || HARNESS_META[thread.harness].name}
                            </span>
                            <span className="shrink-0" style={{ color: 'var(--text-muted)' }}>
                              {m.timestamp}
                            </span>
                          </div>
                          <div className="text-[11px] truncate" style={{ color: 'var(--text-muted)' }}>
                            {snippet}
                          </div>
                        </div>
                      </button>
                    );
                  })}
                  </div>
                </div>
              )}

              {thread.messages.map((m) => {
                const isHighlighted = hoveredTurnId === m.id || activeTurnId === m.id;
                return (
                  <button
                    key={m.id}
                    type="button"
                    onMouseEnter={() => setHoveredTurnId(m.id)}
                    onClick={() => {
                      setActiveTurnId(m.id);
                      const el = feedRef.current?.querySelector(`[data-turn-id="${m.id}"]`);
                      el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
                    }}
                    title={`${m.role === 'user' ? 'You' : 'Agent'} · ${m.timestamp}`}
                    aria-label={`Jump to ${m.role === 'user' ? 'user' : 'agent'} message at ${m.timestamp}`}
                    className="h-[2.5px] rounded-full transition-all cursor-pointer hover:w-5"
                    style={{
                      width: isHighlighted ? '20px' : m.role === 'user' ? '14px' : '9px',
                      backgroundColor: isHighlighted
                        ? 'var(--accent-blue)'
                        : m.role === 'user'
                          ? 'var(--text-main)'
                          : 'var(--border-strong)',
                    }}
                  />
                );
              })}
            </div>
          )}

          {/* Floating Jump-to-Bottom Button */}
          {showScrollBottom && (
            <button
              type="button"
              onClick={() => {
                const el = feedRef.current;
                if (!el) return;
                isNearBottomRef.current = true;
                setShowScrollBottom(false);
                el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
              }}
              className="pop-enter absolute bottom-4 right-5 z-20 w-8 h-8 rounded-full border flex items-center justify-center cursor-pointer transition-colors hover:bg-[var(--bg-hover)]"
              style={{
                backgroundColor: 'var(--bg-card)',
                borderColor: 'var(--border-strong)',
                color: 'var(--text-main)',
                boxShadow: 'var(--popover-shadow)',
              }}
              title="Jump to bottom"
              aria-label="Jump to bottom"
            >
              <ArrowDown size={14} weight="bold" />
            </button>
          )}
        </div>

        {/* Secondary Split Pane */}
        {splitView && (
          <div
            className="flex flex-1 min-w-0 border-l flex-col overflow-hidden"
            style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-app)' }}
          >
            <div
              className="px-3 py-2 border-b flex items-center justify-between gap-2 text-[11.5px] shrink-0"
              style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-sidebar)' }}
            >
              {allThreads.length > 1 ? (
                <select
                  value={secondaryThread?.id || secondaryThreadId || ''}
                  onChange={(e) => onSelectSecondaryThread?.(e.target.value)}
                  aria-label="Select secondary thread"
                  className="min-w-0 flex-1 rounded-[6px] border px-2 py-1 text-[11.5px] font-medium truncate focus:outline-none"
                  style={{
                    backgroundColor: 'var(--bg-card)',
                    borderColor: 'var(--border-line)',
                    color: 'var(--text-main)',
                  }}
                >
                  {allThreads
                    .filter((t) => t.id !== thread.id)
                    .map((t) => (
                      <option key={t.id} value={t.id}>
                        {t.title} ({t.branch})
                      </option>
                    ))}
                </select>
              ) : (
                <span className="font-medium truncate" style={{ color: 'var(--text-muted)' }}>
                  Split View
                </span>
              )}
              <div className="flex items-center gap-1 shrink-0">
                {secondaryThread && onToggleSplitBridge && (
                  <button
                    type="button"
                    onClick={onToggleSplitBridge}
                    className="px-2 py-1 rounded-[6px] border font-mono-tech text-[10.5px] inline-flex items-center gap-1 cursor-pointer transition-colors"
                    style={
                      splitBridgeEnabled
                        ? {
                            borderColor: 'var(--accent-blue)',
                            backgroundColor: 'var(--accent-blue-bg)',
                            color: 'var(--accent-blue)',
                          }
                        : {
                            borderColor: 'var(--border-line)',
                            backgroundColor: 'var(--bg-card)',
                            color: 'var(--text-muted)',
                          }
                    }
                    title={
                      splitBridgeEnabled
                        ? `Native Split Bridge ON: ${HARNESS_META[thread.harness].name} ⇄ ${HARNESS_META[secondaryThread.harness].name} can talk to each other automatically`
                        : 'Enable native agent-to-agent communication between split sessions'
                    }
                  >
                    <span>⇄</span>
                    <span>{splitBridgeEnabled ? 'Bridge ON' : 'Bridge OFF'}</span>
                  </button>
                )}
                {onQuickNewThread && (
                  <button
                    type="button"
                    onClick={onQuickNewThread}
                    className="btn-ghost px-2 py-1 text-[11px] inline-flex items-center gap-1 cursor-pointer"
                    title="Create a new thread"
                  >
                    <Plus size={11} />
                    <span>New</span>
                  </button>
                )}
                {secondaryThread && (
                  <button
                    type="button"
                    onClick={() => onSelectThread(secondaryThread.id)}
                    className="btn-ghost px-2 py-1 text-[11px] cursor-pointer"
                  >
                    Focus
                  </button>
                )}
                <button
                  type="button"
                  onClick={onToggleSplitView}
                  className="btn-ghost p-1 cursor-pointer"
                  title="Close Split View"
                  aria-label="Close split view"
                >
                  <X size={12} />
                </button>
              </div>
            </div>
            {secondaryThread ? (
              <>
                <div
                  ref={secondaryFeedRef}
                  onScroll={handleSecondaryFeedScroll}
                  className="flex-1 overflow-y-auto px-5 py-5 space-y-5"
                >
                  {secondaryThread.messages.map((m, idx) => (
                    <MessageRow
                      key={m.id}
                      m={m}
                      idx={idx}
                      threadId={secondaryThread.id}
                      harness={secondaryThread.harness}
                      isDiscord={secondaryThread.isDiscord}
                      traceOpen={!!expandedTraces[m.id]}
                      theme={theme}
                      onToggleTrace={toggleTrace}
                      onForkAtMessage={onForkAtMessage}
                      onViewCheckpointDiff={onViewCheckpointDiff}
                      onRevertToCheckpoint={onRevertToCheckpoint}
                      onToast={onToast}
                      onOpenFile={onOpenFile}
                    />
                  ))}
                  {secondaryThread.pendingApproval && (
                    <ApprovalCard
                      approval={secondaryThread.pendingApproval}
                      threadId={secondaryThread.id}
                      onResolveApproval={onResolveApproval}
                    />
                  )}
                  {secondaryThread.status === 'RUNNING' && (
                    <div className="flex items-center gap-2 text-[12px] font-mono-tech" style={{ color: 'var(--text-muted)' }}>
                      <span className="w-2 h-2 rounded-full pulse-dot" style={{ backgroundColor: 'var(--accent-blue)' }} />
                      <span>{HARNESS_META[secondaryThread.harness].name} is working in {secondaryThread.branch}...</span>
                    </div>
                  )}
                </div>
                <form
                  onSubmit={handleSecondarySubmit}
                  className="px-3 py-2 border-t flex items-end gap-2 shrink-0"
                  style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-card)' }}
                >
                  <textarea
                    ref={secondaryTextareaRef}
                    rows={1}
                    value={secondaryInput}
                    onChange={(e) => {
                      setSecondaryInput(e.target.value);
                      autoGrowComposer(e.target);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && !e.shiftKey) {
                        e.preventDefault();
                        handleSecondarySubmit(e);
                      }
                    }}
                    placeholder={
                      secondaryThread.status === 'RUNNING'
                        ? `Queue prompt for ${secondaryThread.title}...`
                        : `Message ${HARNESS_META[secondaryThread.harness].name} in ${secondaryThread.branch || 'main'}...`
                    }
                    aria-label="Secondary thread message"
                    className="flex-1 min-w-0 bg-transparent text-[12.5px] resize-none focus:outline-none leading-relaxed max-h-[120px] py-0.5"
                    style={{ color: 'var(--text-main)' }}
                  />
                  {secondaryThread.status === 'RUNNING' ? (
                    <button
                      type="button"
                      onClick={() => onStopStreaming(secondaryThread.id)}
                      className="w-6 h-6 rounded-[6px] flex items-center justify-center cursor-pointer shrink-0"
                      style={{ backgroundColor: 'var(--accent-red)', color: '#FFFFFF' }}
                      title="Stop active turn"
                      aria-label="Stop active turn"
                    >
                      <Stop size={11} weight="fill" />
                    </button>
                  ) : (
                    <button
                      type="submit"
                      disabled={!secondaryInput.trim()}
                      className="btn-solid w-6 h-6 rounded-[6px] flex items-center justify-center cursor-pointer disabled:opacity-40 shrink-0"
                      title="Send"
                      aria-label="Send message to secondary thread"
                    >
                      <ArrowUp size={12} weight="bold" />
                    </button>
                  )}
                </form>
              </>
            ) : (
              <div className="flex-1 flex flex-col items-center justify-center p-6 text-center space-y-3">
                <div className="text-[13px] font-medium" style={{ color: 'var(--text-main)' }}>
                  No second thread yet
                </div>
                <p className="text-[11.5px] m-0 max-w-xs" style={{ color: 'var(--text-muted)' }}>
                  Create another thread to compare or run two sessions side-by-side.
                </p>
                {onQuickNewThread && (
                  <button
                    type="button"
                    onClick={onQuickNewThread}
                    className="btn-solid px-3 py-1.5 text-[11.5px] font-medium inline-flex items-center gap-1.5 cursor-pointer"
                  >
                    <Plus size={12} weight="bold" />
                    <span>Create second thread</span>
                  </button>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Floating Bottom Composer */}
      <div className="px-4 sm:px-6 pb-3.5 pt-2 shrink-0">
        <div className="max-w-4xl mx-auto space-y-2">
          {/* Prompt Queue Strip */}
          {queuedPrompts.length > 0 && (
            <div
              className="rounded-[10px] border px-3 py-2 space-y-1.5 pop-enter"
              style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-elevated)' }}
            >
              <div className="flex items-center justify-between text-[11px] font-medium" style={{ color: 'var(--text-main)' }}>
                <span>Queued Prompts ({queuedPrompts.length}) — auto-runs next</span>
              </div>
              {queuedPrompts.map((q, idx) => (
                <div
                  key={q.id}
                  className="flex items-center justify-between gap-2 px-2.5 py-1 rounded-[7px] border text-[11.5px]"
                  style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-card)' }}
                >
                  <span className="truncate" style={{ color: 'var(--text-body)' }}>
                    <span className="font-mono-tech mr-1.5" style={{ color: 'var(--text-muted)' }}>
                      #{idx + 1}
                    </span>
                    {q.prompt}
                  </span>
                  <div className="flex items-center gap-1 shrink-0">
                    <button
                      type="button"
                      onClick={() => onSendQueuedNow(q.id)}
                      className="px-1.5 py-0.5 rounded text-[10.5px] font-medium cursor-pointer"
                      style={{ color: 'var(--accent-blue)' }}
                    >
                      Send now
                    </button>
                    <button
                      type="button"
                      onClick={() => onRemoveQueued(q.id)}
                      className="p-0.5 rounded cursor-pointer"
                      style={{ color: 'var(--text-muted)' }}
                      title="Remove queued prompt"
                      aria-label="Remove queued prompt"
                    >
                      <Trash size={12} />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}

          {/* Composer Card (Original Web Sakana Bottom Chat Bar) */}
          <form
            onSubmit={handleSubmit}
            onDragOver={(e) => {
              if (e.dataTransfer?.types?.includes('Files')) {
                e.preventDefault();
                e.dataTransfer.dropEffect = 'copy';
                if (!isDraggingFile) setIsDraggingFile(true);
              }
            }}
            onDragLeave={(e) => {
              if (!e.currentTarget.contains(e.relatedTarget as Node)) {
                setIsDraggingFile(false);
              }
            }}
            onDrop={(e) => {
              e.preventDefault();
              setIsDraggingFile(false);
              const files = Array.from(e.dataTransfer?.files ?? []);
              if (files.length > 0) {
                void processAttachmentFiles(files);
              }
            }}
            className="rounded-[14px] border overflow-visible relative transition-colors"
            style={{
              backgroundColor: 'var(--bg-card)',
              borderColor: isDraggingFile ? 'var(--accent-blue)' : 'var(--border-line)',
              boxShadow: isDraggingFile
                ? '0 0 0 2px var(--accent-blue-bg), var(--surface-shadow)'
                : 'var(--surface-shadow)',
            }}
          >
            {/* / Skill Autocomplete Popover */}
            {showSkillMenu && (
              <div className="sakana-popover pop-enter absolute bottom-full left-0 mb-2 w-80 max-w-full rounded-[10px] p-1.5 z-50 space-y-0.5">
                <div className="px-2 py-1 text-[10px] font-mono-tech uppercase" style={{ color: 'var(--text-muted)' }}>
                  Skills &amp; Commands
                </div>
                {filteredSkills.slice(0, 8).map((sk, idx) => (
                  <button
                    key={sk.slug}
                    type="button"
                    onClick={() => {
                      setInput(`/${sk.slug} `);
                      setSlashDismissed(false);
                    }}
                    className="w-full px-2.5 py-1.5 rounded-[7px] text-left flex items-center justify-between gap-2 cursor-pointer"
                    style={{
                      backgroundColor: idx === skillIdx ? 'var(--bg-hover)' : 'transparent',
                      color: 'var(--text-main)',
                    }}
                  >
                    <div className="min-w-0">
                      <div className="font-mono-tech text-[11.5px] font-medium truncate">
                        /{sk.slug}
                      </div>
                      <div className="text-[11px] truncate" style={{ color: 'var(--text-muted)' }}>
                        {sk.description}
                      </div>
                    </div>
                    <span
                      className="font-mono-tech text-[9.5px] px-1.5 py-0.5 rounded-[4px] shrink-0"
                      style={{ backgroundColor: 'var(--bg-elevated)', color: 'var(--text-muted)' }}
                    >
                      {sk.scope}
                    </span>
                  </button>
                ))}
              </div>
            )}

            {/* Attached Context & File Chips */}
            {attachedChips.length > 0 && (
              <div className="px-3.5 pt-2.5 flex flex-wrap gap-1.5">
                {attachedChips.map((chip) => {
                  const isImage = chip.kind === 'image' && Boolean(chip.previewDataUrl);
                  const isPdf = chip.kind === 'pdf';
                  const isFile = chip.kind === 'file' || chip.kind === 'pdf' || chip.kind === 'image';
                  const sizeLabel = formatBytes(chip.sizeBytes);

                  if (isImage) {
                    return (
                      <span
                        key={chip.id}
                        className="inline-flex items-center gap-2 pl-1 pr-2 py-1 rounded-[8px] border font-mono-tech text-[10.5px] max-w-[220px]"
                        style={{
                          backgroundColor: 'var(--bg-elevated)',
                          borderColor: 'var(--border-line)',
                          color: 'var(--text-main)',
                        }}
                        title={chip.path || chip.label}
                      >
                        <img
                          src={chip.previewDataUrl}
                          alt={chip.label}
                          className="w-8 h-8 rounded-[5px] object-cover shrink-0 border"
                          style={{ borderColor: 'var(--border-line)' }}
                        />
                        <div className="min-w-0 flex-1">
                          <div className="truncate font-medium">{chip.label}</div>
                          {sizeLabel && (
                            <div className="text-[9.5px]" style={{ color: 'var(--text-muted)' }}>
                              {sizeLabel}
                            </div>
                          )}
                        </div>
                        <button
                          type="button"
                          onClick={() => onRemoveChip(chip.id)}
                          className="hover:opacity-70 cursor-pointer shrink-0"
                          title="Remove attachment"
                          aria-label={`Remove ${chip.label}`}
                        >
                          <X size={11} />
                        </button>
                      </span>
                    );
                  }

                  return (
                    <span
                      key={chip.id}
                      className="inline-flex items-center gap-1.5 px-2 py-1 rounded-[7px] border font-mono-tech text-[10.5px] max-w-[240px]"
                      style={
                        isFile
                          ? {
                              backgroundColor: 'var(--bg-elevated)',
                              borderColor: 'var(--border-line)',
                              color: 'var(--text-main)',
                            }
                          : {
                              backgroundColor: 'var(--accent-blue-bg)',
                              borderColor: 'var(--border-line)',
                              color: 'var(--accent-blue)',
                            }
                      }
                      title={chip.path || chip.label}
                    >
                      {isPdf ? (
                        <FilePdf size={13} weight="fill" style={{ color: 'var(--accent-red)' }} className="shrink-0" />
                      ) : chip.kind === 'file' ? (
                        <FileText size={13} style={{ color: 'var(--accent-blue)' }} className="shrink-0" />
                      ) : chip.kind === 'image' ? (
                        <File size={13} style={{ color: 'var(--accent-blue)' }} className="shrink-0" />
                      ) : (
                        <PencilSimple size={11} className="shrink-0" />
                      )}
                      <span className="truncate">{chip.label}</span>
                      {sizeLabel && (
                        <span className="text-[9.5px] shrink-0" style={{ color: 'var(--text-muted)' }}>
                          {sizeLabel}
                        </span>
                      )}
                      <button
                        type="button"
                        onClick={() => onRemoveChip(chip.id)}
                        className="hover:opacity-70 cursor-pointer shrink-0"
                        title="Remove attachment"
                        aria-label={`Remove ${chip.label}`}
                      >
                        <X size={11} />
                      </button>
                    </span>
                  );
                })}
              </div>
            )}

            <div className="px-3.5 pt-3 pb-2.5 space-y-2.5">
              <textarea
                ref={textareaRef}
                rows={1}
                value={input}
                onChange={(e) => {
                  setInput(e.target.value);
                  setSkillIdx(0);
                  setSlashDismissed(false);
                  autoGrowComposer(e.target);
                }}
                onPaste={(e) => {
                  const items = Array.from(e.clipboardData?.items ?? []);
                  const itemFiles = items
                    .filter((item) => item.kind === 'file')
                    .map((item) => item.getAsFile())
                    .filter((f): f is File => Boolean(f));
                  const files =
                    itemFiles.length > 0 ? itemFiles : Array.from(e.clipboardData?.files ?? []);
                  if (files.length > 0) {
                    e.preventDefault();
                    void processAttachmentFiles(files);
                  }
                }}
                onKeyDown={(e) => {
                  if (showSkillMenu) {
                    const count = Math.min(filteredSkills.length, 8);
                    if (e.key === 'ArrowDown') {
                      e.preventDefault();
                      setSkillIdx((i) => (i + 1) % count);
                      return;
                    }
                    if (e.key === 'ArrowUp') {
                      e.preventDefault();
                      setSkillIdx((i) => (i - 1 + count) % count);
                      return;
                    }
                    if (e.key === 'Tab' || (e.key === 'Enter' && !e.shiftKey)) {
                      e.preventDefault();
                      const chosen = filteredSkills[skillIdx];
                      if (chosen) {
                        setInput(`/${chosen.slug} `);
                        setSlashDismissed(false);
                      }
                      return;
                    }
                    if (e.key === 'Escape') {
                      e.preventDefault();
                      setSlashDismissed(true);
                      return;
                    }
                  }
                  if (e.key === 'Escape' && openMenu) {
                    e.preventDefault();
                    setOpenMenu(null);
                    return;
                  }
                  if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault();
                    handleSubmit(e);
                  }
                }}
                placeholder={
                  thread.status === 'RUNNING'
                    ? 'Type to queue next prompt while agent runs...'
                    : `Message ${HARNESS_META[thread.harness].name} in ${thread.branch || 'main'} (Enter to send)`
                }
                className="w-full bg-transparent text-[13.5px] resize-none focus:outline-none leading-relaxed max-h-[168px]"
                style={{ color: 'var(--text-main)' }}
              />

              {/* Middle Toolbar Row */}
              <div ref={composerToolbarRef} className="flex flex-nowrap items-center justify-between gap-1.5 min-w-0">
                <div className="flex items-center gap-0.5 min-w-0 flex-1">
                  {/* Harness Selector Pill (Icon-only) */}
                  <div className="relative shrink-0">
                    <button
                      type="button"
                      onClick={() => setOpenMenu((m) => (m === 'harness' ? null : 'harness'))}
                      className="h-7 px-1.5 rounded-[7px] text-[11px] font-medium inline-flex items-center gap-1 cursor-pointer hover:bg-[var(--bg-hover)]"
                      style={{ color: 'var(--text-main)' }}
                      title={HARNESS_META[thread.harness].name}
                      aria-label={`Select harness (${HARNESS_META[thread.harness].name})`}
                    >
                      <HarnessIcon harness={thread.harness} size={13} />
                      <CaretDown size={9} style={{ color: 'var(--text-muted)' }} />
                    </button>
                    {openMenu === 'harness' && (
                      <div className="sakana-popover pop-enter absolute bottom-full left-0 mb-1.5 w-48 rounded-[10px] p-1 z-50 space-y-0.5">
                        {(['agy-cli', 'claude-code', 'kiro-cli', 'sakana'] as HarnessId[]).map((hid) => {
                          const isDisabled = disabledHarnesses.includes(hid);
                          const nextModels = harnessModels?.[hid];
                          const firstModel = nextModels?.[0];
                          const defaultModel = firstModel?.id || HARNESS_META[hid].models[0];
                          const nextEffort = resolveNextEffort(firstModel?.efforts, thread.effort);
                          return (
                            <button
                              key={hid}
                              type="button"
                              disabled={isDisabled}
                              title={isDisabled ? `${HARNESS_META[hid].name} is unavailable` : HARNESS_META[hid].name}
                              onClick={() => {
                                if (isDisabled) return;
                                const nextHarnessAccs = harnessAccounts.filter((a) => a.harness === hid);
                                const nextDefaultAcc =
                                  nextHarnessAccs.find((a) => a.isDefault) || nextHarnessAccs[0];
                                onUpdateThreadConfig(thread.id, {
                                  harness: hid,
                                  model: defaultModel,
                                  ...(nextEffort !== undefined ? { effort: nextEffort } : {}),
                                  ...(nextDefaultAcc ? { account: nextDefaultAcc.id } : {}),
                                });
                                setOpenMenu(null);
                              }}
                              className={`w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center justify-between ${
                                isDisabled ? 'opacity-45 cursor-not-allowed' : 'cursor-pointer hover:bg-[var(--bg-hover)]'
                              }`}
                              style={{ color: 'var(--text-main)' }}
                            >
                              <span className="inline-flex items-center gap-2 min-w-0">
                                <HarnessIcon harness={hid} size={13} />
                                <span className="truncate">{HARNESS_META[hid].name}</span>
                              </span>
                              {thread.harness === hid && <Check size={12} weight="bold" />}
                            </button>
                          );
                        })}
                      </div>
                    )}
                  </div>

                  {/* Model Selector */}
                  <div className="relative min-w-0 flex-shrink">
                    <button
                      type="button"
                      onClick={() => setOpenMenu((m) => (m === 'model' ? null : 'model'))}
                      className="h-7 px-1.5 rounded-[7px] text-[11px] font-medium inline-flex items-center gap-1 cursor-pointer hover:bg-[var(--bg-hover)] max-w-full min-w-0"
                      style={{ color: 'var(--text-body)' }}
                      title={currentModelEntry?.label || thread.model}
                    >
                      <span className="truncate">{currentModelEntry?.label || thread.model}</span>
                      <CaretDown size={9} className="shrink-0" style={{ color: 'var(--text-muted)' }} />
                    </button>
                    {openMenu === 'model' && (
                      <div className="sakana-popover pop-enter absolute bottom-full left-0 mb-1.5 w-60 max-h-72 overflow-y-auto rounded-[10px] p-1 z-50 space-y-0.5">
                        <div className="px-2.5 py-1 text-[10px] font-mono-tech" style={{ color: 'var(--text-muted)' }}>
                          {HARNESS_META[thread.harness].name} Models
                        </div>
                        {groupedModels.map((sec, sIdx) => (
                          <div key={sec.group} className="space-y-0.5">
                            {groupedModels.length > 1 && (
                              <div
                                className={`px-2.5 pt-1.5 pb-0.5 text-[10px] font-medium ${sIdx > 0 ? 'mt-1 border-t' : ''}`}
                                style={{
                                  color: 'var(--text-muted)',
                                  borderColor: 'var(--border-line)',
                                }}
                              >
                                {sec.group}
                              </div>
                            )}
                            {sec.items.map((mod) => (
                              <button
                                key={mod.id}
                                type="button"
                                onClick={() => {
                                  const nextEffort = resolveNextEffort(mod.efforts, thread.effort);
                                  onUpdateThreadConfig(thread.id, {
                                    model: mod.id,
                                    ...(nextEffort !== undefined ? { effort: nextEffort } : {}),
                                  });
                                  setOpenMenu(null);
                                }}
                                className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center justify-between cursor-pointer hover:bg-[var(--bg-hover)]"
                                style={{ color: 'var(--text-main)' }}
                              >
                                <span className="truncate">{mod.label}</span>
                                {thread.model === mod.id && <Check size={12} weight="bold" />}
                              </button>
                            ))}
                          </div>
                        ))}
                      </div>
                    )}
                  </div>

                  {/* Effort Selector (Title Case, no icon; hidden if model has no effort levels) */}
                  {availableEfforts.length > 0 && (() => {
                    const normEffort = (thread.effort || '').toLowerCase();
                    const isTriggerHigh = normEffort === 'high';
                    const isTriggerXHigh = normEffort === 'xhigh' || normEffort === 'max';
                    const triggerTextClass = isTriggerXHigh
                      ? 'effort-xhigh-text font-semibold'
                      : isTriggerHigh
                        ? 'effort-high-text font-semibold'
                        : '';
                    const triggerColor =
                      normEffort === 'low'
                        ? 'var(--text-muted)'
                        : normEffort === 'medium'
                          ? 'var(--effort-med)'
                          : undefined;
                    return (
                      <div className="relative shrink-0">
                        <button
                          type="button"
                          onClick={() => setOpenMenu((m) => (m === 'effort' ? null : 'effort'))}
                          className={`h-7 px-2 rounded-[7px] text-[11px] font-medium inline-flex items-center gap-1 cursor-pointer whitespace-nowrap ${
                            isTriggerXHigh ? 'effort-xhigh-pill' : 'hover:bg-[var(--bg-hover)]'
                          }`}
                          style={triggerColor ? { color: triggerColor } : undefined}
                        >
                          <span className={triggerTextClass}>{formatEffortLabel(thread.effort)}</span>
                        </button>
                        {openMenu === 'effort' && (
                          <div className="sakana-popover pop-enter absolute bottom-full left-0 mb-1.5 w-32 rounded-[10px] p-1 z-50 space-y-0.5">
                            {availableEfforts.map((ef) => {
                              const norm = ef.toLowerCase();
                              const isHigh = norm === 'high';
                              const isXHigh = norm === 'xhigh' || norm === 'max';
                              const optTextClass = isXHigh
                                ? 'effort-xhigh-text font-semibold'
                                : isHigh
                                  ? 'effort-high-text font-semibold'
                                  : '';
                              const optColor =
                                norm === 'low'
                                  ? 'var(--text-muted)'
                                  : norm === 'medium'
                                    ? 'var(--effort-med)'
                                    : undefined;
                              return (
                                <button
                                  key={ef}
                                  type="button"
                                  onClick={() => {
                                    onUpdateThreadConfig(thread.id, { effort: ef });
                                    setOpenMenu(null);
                                  }}
                                  className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] font-medium flex items-center justify-between cursor-pointer hover:bg-[var(--bg-hover)]"
                                  style={optColor ? { color: optColor } : undefined}
                                >
                                  <span className={optTextClass}>{formatEffortLabel(ef)}</span>
                                  {thread.effort === ef && (
                                    <Check
                                      size={12}
                                      weight="bold"
                                      style={
                                        isXHigh
                                          ? { color: 'var(--effort-xhigh-b)' }
                                          : isHigh
                                            ? { color: 'var(--effort-high)' }
                                            : undefined
                                      }
                                    />
                                  )}
                                </button>
                              );
                            })}
                          </div>
                        )}
                      </div>
                    );
                  })()}

                  {/* Fast Account Switcher Pill */}
                  {thread.harness !== 'sakana' && (() => {
                    const matchingAccounts = harnessAccounts.filter((a) => a.harness === thread.harness);
                    const activeAcc =
                      matchingAccounts.find((a) => a.id === thread.account) ||
                      matchingAccounts.find((a) => a.isDefault) ||
                      matchingAccounts[0];
                    const shortLabel = activeAcc
                      ? activeAcc.label?.trim() ||
                        (activeAcc.email ? activeAcc.email.split('@')[0] : 'Account')
                      : 'Account';
                    return (
                      <div className="relative shrink-0">
                        <button
                          type="button"
                          onClick={() => setOpenMenu((m) => (m === 'account' ? null : 'account'))}
                          className="h-7 px-1.5 rounded-[7px] text-[11px] font-medium inline-flex items-center gap-1 cursor-pointer hover:bg-[var(--bg-hover)] max-w-[130px]"
                          style={{ color: 'var(--text-body)' }}
                          title={
                            activeAcc
                              ? `Account: ${activeAcc.label}${activeAcc.email ? ` (${activeAcc.email})` : ''} · ${activeAcc.authenticated ? 'Signed in' : 'Not signed in'}`
                              : 'Switch account'
                          }
                          aria-label="Switch account"
                        >
                          <span
                            className="w-1.5 h-1.5 rounded-full shrink-0"
                            style={{
                              backgroundColor: activeAcc?.authenticated
                                ? 'var(--accent-green)'
                                : 'var(--accent-yellow)',
                            }}
                          />
                          <User size={11} className="shrink-0" style={{ color: 'var(--text-muted)' }} />
                          <span className="truncate">{shortLabel}</span>
                          <CaretDown size={9} className="shrink-0" style={{ color: 'var(--text-muted)' }} />
                        </button>
                        {openMenu === 'account' && (
                          <div className="sakana-popover pop-enter absolute bottom-full left-0 mb-1.5 w-60 rounded-[10px] p-1 z-50 space-y-0.5">
                            <div className="px-2.5 py-1 text-[10px] font-mono-tech flex items-center justify-between" style={{ color: 'var(--text-muted)' }}>
                              <span>{HARNESS_META[thread.harness].name} Accounts</span>
                              <span>{matchingAccounts.length}</span>
                            </div>
                            {matchingAccounts.length === 0 ? (
                              <div className="px-2.5 py-2 text-[11px]" style={{ color: 'var(--text-muted)' }}>
                                No accounts configured.
                              </div>
                            ) : (
                              matchingAccounts.map((acc) => {
                                const isSelected = activeAcc?.id === acc.id;
                                return (
                                  <button
                                    key={acc.id}
                                    type="button"
                                    onClick={() => {
                                      onUpdateThreadConfig(thread.id, { account: acc.id });
                                      setOpenMenu(null);
                                      onToast(
                                        `Switched to ${acc.email || acc.label}`,
                                      );
                                    }}
                                    className="w-full px-2.5 py-1.5 rounded-[6px] text-left flex items-center justify-between gap-2 cursor-pointer hover:bg-[var(--bg-hover)]"
                                    style={{ color: 'var(--text-main)' }}
                                  >
                                    <div className="min-w-0 flex-1">
                                      <div className="flex items-center gap-1.5 text-[11.5px] font-medium truncate">
                                        <span
                                          className="w-1.5 h-1.5 rounded-full shrink-0"
                                          style={{
                                            backgroundColor: acc.authenticated
                                              ? 'var(--accent-green)'
                                              : 'var(--accent-yellow)',
                                          }}
                                        />
                                        <span className="truncate">{acc.label}</span>
                                      </div>
                                      {acc.email && (
                                        <div className="text-[10px] font-mono-tech truncate pl-3" style={{ color: 'var(--text-muted)' }}>
                                          {acc.email}
                                        </div>
                                      )}
                                    </div>
                                    {isSelected && <Check size={12} weight="bold" className="shrink-0" />}
                                  </button>
                                );
                              })
                            )}
                            {onOpenHarnessAccounts && (
                              <>
                                <div className="my-0.5 border-t" style={{ borderColor: 'var(--border-line)' }} />
                                <button
                                  type="button"
                                  onClick={() => {
                                    setOpenMenu(null);
                                    onOpenHarnessAccounts();
                                  }}
                                  className="w-full px-2.5 py-1.5 rounded-[6px] text-[11px] font-medium flex items-center gap-1.5 cursor-pointer hover:bg-[var(--bg-hover)]"
                                  style={{ color: 'var(--accent-blue)' }}
                                >
                                  <Plus size={11} weight="bold" />
                                  <span>Manage / Sign in accounts…</span>
                                </button>
                              </>
                            )}
                          </div>
                        )}
                      </div>
                    );
                  })()}

                  {/* Permission / Guard Pill (Icon-only) */}
                  <div className="relative shrink-0">
                    <button
                      type="button"
                      onClick={() => setOpenMenu((m) => (m === 'perm' ? null : 'perm'))}
                      className="w-7 h-7 rounded-[7px] inline-flex items-center justify-center cursor-pointer hover:bg-[var(--bg-hover)]"
                      style={{
                        color:
                          thread.permissionMode === 'yolo'
                            ? 'var(--accent-red)'
                            : thread.permissionMode === 'auto-edit'
                              ? 'var(--accent-blue)'
                              : 'var(--text-muted)',
                      }}
                      title={`Permission: ${permLabel[thread.permissionMode]}`}
                      aria-label={`Permission mode: ${permLabel[thread.permissionMode]}`}
                    >
                      <ShieldCheck size={14} weight={thread.permissionMode === 'supervised' ? 'regular' : 'fill'} />
                    </button>
                    {openMenu === 'perm' && (
                      <div className="sakana-popover pop-enter absolute bottom-full left-0 mb-1.5 w-44 rounded-[10px] p-1 z-50 space-y-0.5">
                        {(['supervised', 'auto-edit', 'yolo'] as PermissionMode[]).map((pm) => (
                          <button
                            key={pm}
                            type="button"
                            onClick={() => {
                              onUpdateThreadConfig(thread.id, { permissionMode: pm });
                              setOpenMenu(null);
                            }}
                            className="w-full px-2.5 py-1.5 rounded-[6px] text-[12px] flex items-center justify-between cursor-pointer hover:bg-[var(--bg-hover)]"
                            style={{ color: 'var(--text-main)' }}
                          >
                            <span>{permLabel[pm]}</span>
                            {thread.permissionMode === pm && <Check size={12} weight="bold" />}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                </div>

                <div className="flex items-center gap-1 shrink-0">
                  <input
                    ref={fileInputRef}
                    type="file"
                    multiple
                    className="hidden"
                    onChange={(e) => {
                      const files = Array.from(e.target.files ?? []);
                      if (files.length > 0) {
                        void processAttachmentFiles(files);
                      }
                      e.target.value = '';
                    }}
                  />
                  <button
                    type="button"
                    onClick={() => {
                      if (onPickAttachments && isTauriRuntime()) {
                        void onPickAttachments();
                      } else {
                        fileInputRef.current?.click();
                      }
                    }}
                    className="w-7 h-7 rounded-[7px] flex items-center justify-center hover:bg-[var(--bg-hover)] cursor-pointer"
                    style={{ color: 'var(--text-muted)' }}
                    title="Attach file, image, or PDF"
                    aria-label="Attach file, image, or PDF"
                  >
                    <Paperclip size={14} />
                  </button>
                  {thread.status === 'RUNNING' ? (
                    <button
                      type="button"
                      onClick={() => onStopStreaming(thread.id)}
                      className="w-7 h-7 rounded-[8px] flex items-center justify-center cursor-pointer"
                      style={{ backgroundColor: 'var(--accent-red)', color: '#FFFFFF' }}
                      title="Stop active turn"
                      aria-label="Stop active turn"
                    >
                      <Stop size={12} weight="fill" />
                    </button>
                  ) : (
                    <button
                      type="submit"
                      className="btn-solid w-7 h-7 rounded-[8px] flex items-center justify-center cursor-pointer"
                      title="Send"
                      aria-label="Send message"
                    >
                      <ArrowUp size={14} weight="bold" />
                    </button>
                  )}
                </div>
              </div>
            </div>

          </form>
        </div>
      </div>
    </div>
  );
}
