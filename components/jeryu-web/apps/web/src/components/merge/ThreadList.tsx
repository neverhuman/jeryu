// ThreadList.tsx — unresolved review threads (W-FE-11).
//
// Renders a compact list of conversation threads, prioritising unresolved
// ones at the top. Each item links to the file + line that anchors the
// thread; the parent page wires this up to scroll the diff viewer.

import { MessageSquare, MessageSquareOff } from 'lucide-react';

import type { ReviewThread } from '../../api/types';

import './merge.css';

export interface ThreadListProps {
  threads: ReviewThread[];
  /** Called when the user clicks a thread, passing the anchor coordinate. */
  onJump?: (thread: ReviewThread) => void;
  className?: string;
}

export function ThreadList({
  threads,
  onJump,
  className,
}: ThreadListProps): JSX.Element {
  if (threads.length === 0) {
    return (
      <div className={`thread-list ${className ?? ''}`.trim()}>
        <p className="thread-list__empty">No conversation threads.</p>
      </div>
    );
  }

  // Sort: unresolved first, then by most-recent update.
  const sorted = [...threads].sort((a, b) => {
    if (a.resolved !== b.resolved) return a.resolved ? 1 : -1;
    return (b.updated_at ?? '').localeCompare(a.updated_at ?? '');
  });

  return (
    <section
      className={`thread-list ${className ?? ''}`.trim()}
      aria-label="Review threads"
    >
      <header className="thread-list__header">
        <h3 className="thread-list__title">Threads</h3>
        <span className="thread-list__count">
          {threads.filter((t) => !t.resolved).length} unresolved
        </span>
      </header>
      <ul className="thread-list__items">
        {sorted.map((thread) => {
          const first = thread.comments[0];
          const snippet = first?.body_markdown?.slice(0, 120) ?? '';
          const Icon = thread.resolved ? MessageSquareOff : MessageSquare;
          return (
            <li
              key={thread.id}
              className={`thread-list__item ${thread.resolved ? 'thread-list__item--resolved' : ''}`.trim()}
            >
              <button
                type="button"
                className="thread-list__button"
                onClick={() => onJump?.(thread)}
              >
                <Icon
                  aria-hidden="true"
                  size={12}
                  className="thread-list__icon"
                />
                <span className="thread-list__meta">
                  {thread.file_path ? (
                    <>
                      <code className="thread-list__path">
                        {thread.file_path}
                      </code>
                      {thread.line ? (
                        <span aria-hidden="true">:{thread.line}</span>
                      ) : null}
                    </>
                  ) : (
                    <span className="thread-list__general">General</span>
                  )}
                </span>
                <span className="thread-list__snippet">{snippet}</span>
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
