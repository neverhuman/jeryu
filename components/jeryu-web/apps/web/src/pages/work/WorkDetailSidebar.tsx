import type { Dispatch, FormEvent, SetStateAction } from 'react';

import type { WorkItem } from '../../api/types';
import { displayPrincipal } from '../workModel';

export interface WorkDetailSidebarProps {
  item: WorkItem;
  pullOwner: string;
  pullRepo: string;
  pullNumber: string;
  setPullOwner: Dispatch<SetStateAction<string>>;
  setPullRepo: Dispatch<SetStateAction<string>>;
  setPullNumber: Dispatch<SetStateAction<string>>;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  pending: boolean;
  errorMessage: string | null;
}

export function WorkDetailSidebar({
  item,
  pullOwner,
  pullRepo,
  pullNumber,
  setPullOwner,
  setPullRepo,
  setPullNumber,
  onSubmit,
  pending,
  errorMessage,
}: WorkDetailSidebarProps): JSX.Element {
  return (
    <aside className="work-detail__panel work-detail__side">
      <section>
        <h2>Assignees</h2>
        <div className="work-card__chips">
          {item.assignees.length === 0 ? (
            <span className="work-detail__muted">Unassigned</span>
          ) : (
            item.assignees.map((assignee) => (
              <span className="work-chip work-chip--person" key={assignee.id}>
                {displayPrincipal(assignee)}
              </span>
            ))
          )}
        </div>
      </section>
      <section>
        <h2>Pull requests</h2>
        {item.pull_requests.length === 0 ? (
          <p className="work-detail__muted">No linked pull requests.</p>
        ) : (
          <ul className="work-detail__links">
            {item.pull_requests.map((pull) => (
              <li key={`${pull.owner}/${pull.repo}/${String(pull.number)}`}>
                {pull.url ? (
                  <a href={pull.url}>{pull.owner}/{pull.repo}#{String(pull.number)}</a>
                ) : (
                  <span>{pull.owner}/{pull.repo}#{String(pull.number)}</span>
                )}
              </li>
            ))}
          </ul>
        )}
        <form className="work-detail__link-form" onSubmit={onSubmit}>
          <input
            value={pullOwner}
            onChange={(event) => setPullOwner(event.target.value)}
            placeholder="owner"
            aria-label="Pull request owner"
          />
          <input
            value={pullRepo}
            onChange={(event) => setPullRepo(event.target.value)}
            placeholder="repo"
            aria-label="Pull request repo"
          />
          <input
            value={pullNumber}
            onChange={(event) => setPullNumber(event.target.value)}
            placeholder="number"
            inputMode="numeric"
            aria-label="Pull request number"
          />
          <button type="submit" disabled={pending}>
            Link
          </button>
        </form>
        {errorMessage ? (
          <p className="work-page__error">{errorMessage}</p>
        ) : null}
      </section>
    </aside>
  );
}
