// CodeViewer.tsx — Monaco-based viewer with markdown rendering toggle
// (W-FE-10).
//
// Monaco is heavy (~1 MB minified) so we lazy-load it via React.lazy + a
// dynamic import. The Suspense loading boundary renders the existing skeleton
// while the chunk streams. For .md files, the viewer offers a second tab that
// renders the server-provided HTML via MarkdownRenderer; the default tab is
// "Rendered" when HTML is available so the README experience matches a
// GitHub-style blob view.

import { lazy, Suspense, useMemo, useState } from 'react';

import { usePreferencesStore } from '../../stores/preferencesStore';
import { LoadingState } from '../state';

import { MarkdownRenderer } from './MarkdownRenderer';
import { isMarkdownPath } from '../../hooks/useBlob';

import './browser.css';

const MonacoEditor = lazy(() =>
  import('@monaco-editor/react').then((m) => ({ default: m.Editor }))
);

export interface CodeViewerProps {
  /** File path (used to detect language + markdown handling). */
  path: string;
  /** UTF-8 file content (Monaco). May be `null` if the blob is binary. */
  text: string | null;
  /** Server-rendered sanitized markdown HTML; `null` if not markdown. */
  renderedHtml?: string | null;
  /** Best-effort MIME type from the blob response. */
  mime?: string;
  /** When true, the viewer renders a "Binary file" notice instead. */
  isBinary?: boolean;
}

/** Map a file extension to a Monaco language id. */
function languageForPath(path: string): string {
  const lower = path.toLowerCase();
  const ext = lower.split('.').pop() ?? '';
  const map: Record<string, string> = {
    ts: 'typescript',
    tsx: 'typescript',
    js: 'javascript',
    jsx: 'javascript',
    mjs: 'javascript',
    cjs: 'javascript',
    rs: 'rust',
    py: 'python',
    go: 'go',
    java: 'java',
    kt: 'kotlin',
    c: 'c',
    cc: 'cpp',
    cpp: 'cpp',
    cxx: 'cpp',
    h: 'cpp',
    hpp: 'cpp',
    cs: 'csharp',
    php: 'php',
    swift: 'swift',
    sh: 'shell',
    bash: 'shell',
    sql: 'sql',
    json: 'json',
    toml: 'toml',
    yaml: 'yaml',
    yml: 'yaml',
    md: 'markdown',
    markdown: 'markdown',
    html: 'html',
    css: 'css',
    scss: 'scss',
    xml: 'xml',
    dockerfile: 'dockerfile',
  };
  return map[ext] ?? 'plaintext';
}

export function CodeViewer({
  path,
  text,
  renderedHtml,
  mime,
  isBinary,
}: CodeViewerProps): JSX.Element {
  const isMd = isMarkdownPath(path);
  const hasRenderedHtml = isMd && typeof renderedHtml === 'string';
  const codeFontSize = usePreferencesStore((s) => s.codeFontSize);
  const [tab, setTab] = useState<'rendered' | 'raw'>(
    hasRenderedHtml ? 'rendered' : 'raw'
  );

  const language = useMemo(() => languageForPath(path), [path]);

  if (isBinary) {
    return (
      <div className="code-viewer">
        <p>Binary file ({mime ?? 'application/octet-stream'}).</p>
      </div>
    );
  }

  return (
    <div className="code-viewer">
      {hasRenderedHtml ? (
        <div className="code-viewer__toolbar">
          <div
            className="code-viewer__tabs"
            role="tablist"
            aria-label="View"
          >
            <button
              type="button"
              role="tab"
              className="code-viewer__tab"
              aria-pressed={tab === 'rendered'}
              onClick={() => setTab('rendered')}
            >
              Rendered
            </button>
            <button
              type="button"
              role="tab"
              className="code-viewer__tab"
              aria-pressed={tab === 'raw'}
              onClick={() => setTab('raw')}
            >
              Raw
            </button>
          </div>
        </div>
      ) : null}
      {tab === 'rendered' && typeof renderedHtml === 'string' ? (
        <div className="code-viewer__rendered">
          <MarkdownRenderer html={renderedHtml} />
        </div>
      ) : (
        <Suspense
          fallback={
            <LoadingState
              title="Loading editor…"
              variant="message"
              description="Monaco is loading."
            />
          }
        >
          <div className="code-viewer__monaco">
            <MonacoEditor
              value={text ?? ''}
              language={language}
              theme="vs-dark"
              height="100%"
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: codeFontSize,
                scrollBeyondLastLine: false,
                wordWrap: 'on',
                renderLineHighlight: 'none',
              }}
            />
          </div>
        </Suspense>
      )}
    </div>
  );
}
