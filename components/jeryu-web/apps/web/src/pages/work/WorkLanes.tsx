import { Link } from 'react-router-dom';

import type { WorkItem } from '../../api/types';
import {
  WORK_KIND_LABELS,
  WORK_PRIORITY_LABELS,
  displayPrincipal,
  workRepoName,
  type WorkLane,
} from '../workModel';

export interface WorkLanesProps {
  lanes: WorkLane[];
}

export function WorkLanes({ lanes }: WorkLanesProps): JSX.Element {
  return (
    <section className="work-lanes" aria-label="Work board">
      {lanes.map((lane) => (
        <div className="work-lane" key={lane.id}>
          <header className="work-lane__header">
            <h2>{lane.title}</h2>
            <span className="work-lane__count">{lane.items.length}</span>
          </header>
          <div className="work-lane__items">
            {lane.items.map((item) => (
              <WorkCard item={item} key={item.id} />
            ))}
          </div>
        </div>
      ))}
    </section>
  );
}

function WorkCard({ item }: { item: WorkItem }): JSX.Element {
  return (
    <article className={`work-card work-card--${item.priority}`}>
      <div className="work-card__top">
        <span className="work-card__key">{item.key}</span>
        <span className="work-card__priority">{WORK_PRIORITY_LABELS[item.priority]}</span>
      </div>
      <h3 className="work-card__title">
        <Link to={`/work/${encodeURIComponent(item.key)}`}>{item.title}</Link>
      </h3>
      <div className="work-card__meta">
        <span>{WORK_KIND_LABELS[item.kind]}</span>
        <span>{workRepoName(item)}</span>
      </div>
      <div className="work-card__chips" aria-label={`${item.key} labels`}>
        {item.labels.map((label) => (
          <span className="work-chip" key={label}>
            {label}
          </span>
        ))}
        {item.assignees.map((assignee) => (
          <span className="work-chip work-chip--person" key={assignee.id}>
            {displayPrincipal(assignee)}
          </span>
        ))}
      </div>
      <div className="work-card__links">
        {item.issue ? (
          item.issue.url ? (
            <a href={item.issue.url}>#{String(item.issue.number)}</a>
          ) : (
            <span>#{String(item.issue.number)}</span>
          )
        ) : null}
        {item.pull_requests.map((pull) => (
          pull.url ? (
            <a
              href={pull.url}
              key={`${pull.owner}/${pull.repo}/${String(pull.number)}`}
            >
              PR {String(pull.number)}
            </a>
          ) : (
            <span key={`${pull.owner}/${pull.repo}/${String(pull.number)}`}>
              PR {String(pull.number)}
            </span>
          )
        ))}
      </div>
    </article>
  );
}
