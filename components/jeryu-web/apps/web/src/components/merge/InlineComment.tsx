// InlineComment.tsx — composer for a single inline comment (W-FE-11).
//
// Two modes:
//   * `mode="compose"` — empty textarea + Submit / Cancel buttons.
//   * `mode="display"` — read-only rendering of an existing `ReviewComment`.
//
// We intentionally avoid embedding a full Markdown editor: the textarea is
// good enough for Phase 3, and the server-side renderer handles preview in
// `ThreadList`. Suggestion blocks (``` ```suggestion ``` ```) are rendered
// when the comment carries a `suggestion` payload.

import { useState } from 'react';

import type { ReviewComment } from '../../api/types';
import { ActionButton } from '../action/ActionButton';

import './merge.css';

export interface InlineCommentDisplayProps {
  mode: 'display';
  comment: ReviewComment;
  className?: string;
}

export interface InlineCommentComposeProps {
  mode: 'compose';
  hintText?: string;
  /** Pre-fill body (for editing flows; default empty). */
  initialBody?: string;
  /** Submit handler; receives the trimmed body. */
  onSubmit: (body: string) => Promise<void> | void;
  /** Cancel handler. */
  onCancel?: () => void;
  /** When true, the submit button stays disabled with a spinner. */
  isSubmitting?: boolean;
  className?: string;
}

export type InlineCommentProps =
  | InlineCommentDisplayProps
  | InlineCommentComposeProps;

export function InlineComment(props: InlineCommentProps): JSX.Element {
  if (props.mode === 'display') {
    const { comment, className } = props;
    return (
      <article
        className={`inline-comment inline-comment--display ${className ?? ''}`.trim()}
        aria-label={`Comment by ${comment.author}`}
      >
        <header className="inline-comment__header">
          <span className="inline-comment__author">{comment.author}</span>
          <time
            className="inline-comment__timestamp"
            dateTime={comment.created_at}
          >
            {new Date(comment.created_at).toLocaleString()}
          </time>
          {comment.edited_at ? (
            <span className="inline-comment__edited">edited</span>
          ) : null}
        </header>
        <div className="inline-comment__body">{comment.body_markdown}</div>
        {comment.suggestion ? (
          <pre className="inline-comment__suggestion" aria-label="Suggested change">
            <code>{comment.suggestion.suggested_text}</code>
          </pre>
        ) : null}
      </article>
    );
  }

  return <InlineCommentComposer {...props} />;
}

function InlineCommentComposer({
  hintText,
  initialBody,
  onSubmit,
  onCancel,
  isSubmitting = false,
  className,
}: InlineCommentComposeProps): JSX.Element {
  const [body, setBody] = useState(initialBody ?? '');

  const trimmed = body.trim();
  const canSubmit = trimmed.length > 0 && !isSubmitting;

  const handleSubmit = async (): Promise<void> => {
    if (!canSubmit) return;
    await onSubmit(trimmed);
    setBody('');
  };

  return (
    <form
      className={`inline-comment inline-comment--compose ${className ?? ''}`.trim()}
      onSubmit={(event) => {
        event.preventDefault();
        void handleSubmit();
      }}
    >
      <label className="sr-only" htmlFor="inline-comment-body">
        Comment body
      </label>
      <textarea
        id="inline-comment-body"
        className="inline-comment__textarea"
        aria-label={hintText ?? 'Comment body'}
        rows={3}
        value={body}
        onChange={(event) => setBody(event.target.value)}
        disabled={isSubmitting}
      />
      <div className="inline-comment__actions">
        {onCancel ? (
          <ActionButton
            variant="ghost"
            onClick={onCancel}
            disabled={isSubmitting}
          >
            Cancel
          </ActionButton>
        ) : null}
        <ActionButton
          variant="primary"
          type="submit"
          disabled={!canSubmit}
        >
          {isSubmitting ? 'Posting…' : 'Submit'}
        </ActionButton>
      </div>
    </form>
  );
}
