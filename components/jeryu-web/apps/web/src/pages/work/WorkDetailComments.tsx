import type { Dispatch, FormEvent, SetStateAction } from 'react';

import type { WorkComment } from '../../api/types';
import { displayPrincipal, formatDate } from '../workModel';

export interface WorkDetailCommentsProps {
  comments: WorkComment[];
  comment: string;
  setComment: Dispatch<SetStateAction<string>>;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  pending: boolean;
  errorMessage: string | null;
}

export function WorkDetailComments({
  comments,
  comment,
  setComment,
  onSubmit,
  pending,
  errorMessage,
}: WorkDetailCommentsProps): JSX.Element {
  return (
    <section className="work-detail__panel work-detail__comments">
      <h2>Comments</h2>
      <form className="work-detail__comment-form" onSubmit={onSubmit}>
        <textarea
          value={comment}
          onChange={(event) => setComment(event.target.value)}
          rows={3}
          aria-label="Comment body"
        />
        <button type="submit" disabled={pending || !comment.trim()}>
          Comment
        </button>
      </form>
      {errorMessage ? (
        <p className="work-page__error">{errorMessage}</p>
      ) : null}
      <div className="work-detail__comment-list">
        {comments.length === 0 ? (
          <p className="work-detail__muted">No comments yet.</p>
        ) : (
          comments.map((entry) => (
            <article className="work-comment" key={entry.id}>
              <header>
                <strong>{displayPrincipal(entry.author)}</strong>
                <time dateTime={entry.created_at}>{formatDate(entry.created_at)}</time>
              </header>
              <p>{entry.body}</p>
            </article>
          ))
        )}
      </div>
    </section>
  );
}
