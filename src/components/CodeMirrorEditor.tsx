import { useEffect, useRef } from 'react';
import { EditorState, type Extension } from '@codemirror/state';
import {
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from '@codemirror/view';
import { javascript } from '@codemirror/lang-javascript';
import { rust } from '@codemirror/lang-rust';
import { json } from '@codemirror/lang-json';
import { css } from '@codemirror/lang-css';
import { html } from '@codemirror/lang-html';
import { markdown } from '@codemirror/lang-markdown';
import { oneDark } from '@codemirror/theme-one-dark';
import type { ThemeId } from '../types';

interface CodeMirrorEditorProps {
  value: string;
  filePath: string;
  theme: ThemeId;
  onChange: (next: string) => void;
  onSave?: () => void;
}

function getLanguageExtension(filePath: string): Extension[] {
  const lower = filePath.toLowerCase();
  if (lower.endsWith('.ts') || lower.endsWith('.tsx')) {
    return [javascript({ typescript: true, jsx: lower.endsWith('.tsx') })];
  }
  if (lower.endsWith('.js') || lower.endsWith('.jsx') || lower.endsWith('.mjs')) {
    return [javascript({ jsx: lower.endsWith('.jsx') })];
  }
  if (lower.endsWith('.rs')) return [rust()];
  if (lower.endsWith('.json')) return [json()];
  if (lower.endsWith('.css')) return [css()];
  if (lower.endsWith('.html') || lower.endsWith('.htm')) return [html()];
  if (lower.endsWith('.md') || lower.endsWith('.mdx')) return [markdown()];
  return [];
}

export function CodeMirrorEditor({
  value,
  filePath,
  theme,
  onChange,
  onSave,
}: CodeMirrorEditorProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const onSaveRef = useRef(onSave);
  onChangeRef.current = onChange;
  onSaveRef.current = onSave;

  const isDark = theme === 'noir' || theme === 'slate';

  useEffect(() => {
    const parent = containerRef.current;
    if (!parent) return;

    const baseTheme = EditorView.theme(
      {
        '&': {
          height: '100%',
          fontSize: '12px',
          fontFamily: "'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
          backgroundColor: 'var(--bg-card)',
          color: 'var(--text-main)',
        },
        '.cm-scroller': {
          overflow: 'auto',
          lineHeight: '1.6',
        },
        '.cm-gutters': {
          backgroundColor: 'var(--bg-elevated)',
          borderRight: '1px solid var(--border-line)',
          color: 'var(--text-muted)',
        },
      },
      { dark: isDark },
    );

    const extensions: Extension[] = [
      lineNumbers(),
      highlightActiveLine(),
      highlightActiveLineGutter(),
      ...getLanguageExtension(filePath),
      keymap.of([
        {
          key: 'Mod-s',
          run: () => {
            onSaveRef.current?.();
            return true;
          },
        },
      ]),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onChangeRef.current(update.state.doc.toString());
        }
      }),
      ...(isDark ? [oneDark] : []),
      baseTheme,
    ];

    const state = EditorState.create({
      doc: value,
      extensions,
    });

    const view = new EditorView({ state, parent });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [filePath, isDark]);

  // Keep external value in sync if it changes without recreating editor
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== value) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: value },
      });
    }
  }, [value]);

  return (
    <div
      ref={containerRef}
      className="w-full h-full min-h-[360px] rounded-[8px] border overflow-hidden"
      style={{ borderColor: 'var(--border-strong)' }}
    />
  );
}
