// DiffViewer.tsx — virtualized unified-diff renderer (W-FE-11).
//
// Renders the active file's hunks one row per line via
// `@tanstack/react-virtual` so a 5 000-line diff stays interactive. Each row
// carries its line type (`+` / `-` / context / hunk-header) so CSS can
// colour it; clicking the line number opens an inline-comment composer
// anchored to that line.
//
// `mode` is a viewer preference (unified vs split). Phase 3 ships the
// unified renderer; the split variant lays out left/right gutters but
// reuses the same row data. Toggle is wired here so the cockpit page can
// keep the preference in the preferences store.

import { useVirtualizer } from '@tanstack/react-virtual';
import { MessageSquarePlus } from 'lucide-react';
import { useMemo, useRef, useState } from 'react';

import type {
  PullRequestDiffFile,
  PullRequestDiffHunk,
} from '../../api/types';
import { ActionButton } from '../action/ActionButton';

import { InlineComment } from './InlineComment';
import './merge.css';

export type DiffViewerMode = 'unified' | 'split';

interface DiffRow {
  /** Flattened key for React. */
  key: string;
  /** Hunk header (purely informational) or a normal diff line. */
  kind: 'hunk' | 'context' | 'add' | 'del';
  /** Base (left) line number. */
  baseLine: number | null;
  /** Head (right) line number. */
  headLine: number | null;
  /** Raw line text without the leading prefix. */
  text: string;
}

function flatten(hunks: PullRequestDiffHunk[]): DiffRow[] {
  const rows: DiffRow[] = [];
  for (let h = 0; h < hunks.length; h += 1) {
    const hunk = hunks[h]!;
    rows.push({
      key: `h${h}:${hunk.header}`,
      kind: 'hunk',
      baseLine: null,
      headLine: null,
      text: hunk.header,
    });
    let baseLine = hunk.old_start;
    let headLine = hunk.new_start;
    for (let i = 0; i < hunk.lines.length; i += 1) {
      const line = hunk.lines[i]!;
      const prefix = line[0] ?? ' ';
      const text = line.slice(1);
      if (prefix === '+') {
        rows.push({
          key: `${h}:+${headLine}:${i}`,
          kind: 'add',
          baseLine: null,
          headLine,
          text,
        });
        headLine += 1;
      } else if (prefix === '-') {
        rows.push({
          key: `${h}:-${baseLine}:${i}`,
          kind: 'del',
          baseLine,
          headLine: null,
          text,
        });
        baseLine += 1;
      } else {
        rows.push({
          key: `${h}: ${headLine}:${i}`,
          kind: 'context',
          baseLine,
          headLine,
          text,
        });
        baseLine += 1;
        headLine += 1;
      }
    }
  }
  return rows;
}

const ROW_HEIGHT_PX = 20;

export interface DiffViewerProps {
  file: PullRequestDiffFile;
  mode: DiffViewerMode;
  onModeChange: (mode: DiffViewerMode) => void;
  /** Submit a new inline comment at `path:line`. */
  onAddComment?: (path: string, line: number, body: string) => Promise<void> | void;
  className?: string;
}

export function DiffViewer({
  file,
  mode,
  onModeChange,
  onAddComment,
  className,
}: DiffViewerProps): JSX.Element {
  const rows = useMemo(() => flatten(file.hunks), [file.hunks]);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [composerLine, setComposerLine] = useState<number | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => containerRef.current,
    estimateSize: () => ROW_HEIGHT_PX,
    overscan: 12,
  });

  const handleSubmitComment = async (line: number, body: string): Promise<void> => {
    if (!onAddComment) return;
    try {
      setSubmitting(true);
      await onAddComment(file.path, line, body);
      setComposerLine(null);
    } finally {
      setSubmitting(false);
    }
  };

  if (file.is_binary) {
    return (
      <div
        className={`diff-viewer diff-viewer--binary ${className ?? ''}`.trim()}
        role="region"
        aria-label={`Diff for ${file.path}`}
      >
        <header className="diff-viewer__toolbar">
          <h4 className="diff-viewer__title">{file.path}</h4>
        </header>
        <p className="diff-viewer__binary">Binary file — diff suppressed.</p>
      </div>
    );
  }

  return (
    <div
      className={`diff-viewer diff-viewer--${mode} ${className ?? ''}`.trim()}
      role="region"
      aria-label={`Diff for ${file.path}`}
    >
      <header className="diff-viewer__toolbar">
        <h4 className="diff-viewer__title">{file.path}</h4>
        <div
          className="diff-viewer__mode-toggle"
          role="group"
          aria-label="Diff view"
        >
          <button
            type="button"
            className={`diff-viewer__mode-button ${mode === 'unified' ? 'diff-viewer__mode-button--active' : ''}`.trim()}
            aria-pressed={mode === 'unified'}
            onClick={() => onModeChange('unified')}
          >
            Unified
          </button>
          <button
            type="button"
            className={`diff-viewer__mode-button ${mode === 'split' ? 'diff-viewer__mode-button--active' : ''}`.trim()}
            aria-pressed={mode === 'split'}
            onClick={() => onModeChange('split')}
          >
            Split
          </button>
        </div>
      </header>
      <div
        ref={containerRef}
        className="diff-viewer__scroll"
        // Bounded height so the virtualizer has a scrolling parent.
        style={{ maxHeight: '70vh', overflow: 'auto' }}
        tabIndex={0}
        data-testid="diff-viewer-scroll"
      >
        <div
          className="diff-viewer__inner"
          style={{
            height: `${virtualizer.getTotalSize()}px`,
            position: 'relative',
          }}
        >
          {virtualizer.getVirtualItems().map((virtualRow) => {
            const row = rows[virtualRow.index];
            if (!row) return null;
            const line = row.headLine ?? row.baseLine;
            return (
              <div
                key={row.key}
                className={`diff-viewer__row diff-viewer__row--${row.kind}`}
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  height: `${virtualRow.size}px`,
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                {row.kind === 'hunk' ? (
                  <span className="diff-viewer__hunk-header">{row.text}</span>
                ) : (
                  <>
                    <span className="diff-viewer__gutter diff-viewer__gutter--base">
                      {row.baseLine ?? ''}
                    </span>
                    <span className="diff-viewer__gutter diff-viewer__gutter--head">
                      {row.headLine ?? ''}
                    </span>
                    <span className="diff-viewer__prefix">
                      {row.kind === 'add' ? '+' : row.kind === 'del' ? '−' : ' '}
                    </span>
                    <span className="diff-viewer__text">{row.text}</span>
                    {onAddComment && line !== null ? (
                      <button
                        type="button"
                        className="diff-viewer__add-comment"
                        aria-label={`Comment on line ${line}`}
                        onClick={() => setComposerLine(line)}
                      >
                        <MessageSquarePlus aria-hidden="true" size={10} />
                      </button>
                    ) : null}
                  </>
                )}
              </div>
            );
          })}
        </div>
      </div>
      {composerLine !== null ? (
        <div className="diff-viewer__composer" data-line={composerLine}>
          <p className="diff-viewer__composer-anchor">
            Commenting on line {composerLine}
          </p>
          <InlineComment
            mode="compose"
            placeholder="Leave an inline comment…"
            isSubmitting={submitting}
            onSubmit={(body) => handleSubmitComment(composerLine, body)}
            onCancel={() => setComposerLine(null)}
          />
        </div>
      ) : null}
      {!composerLine && onAddComment ? (
        <ActionButton
          variant="ghost"
          icon={<MessageSquarePlus aria-hidden="true" size={12} />}
          onClick={() => setComposerLine(rows.find((r) => r.headLine)?.headLine ?? 1)}
          className="diff-viewer__add-button"
        >
          Add comment
        </ActionButton>
      ) : null}
    </div>
  );
}
