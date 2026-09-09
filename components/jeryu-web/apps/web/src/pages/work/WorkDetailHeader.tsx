import { Link } from 'react-router-dom';

import type { WorkItem } from '../../api/types';
import { formatDate, workRepoName } from '../workModel';

export interface WorkDetailHeaderProps {
  item: WorkItem;
}

export function WorkDetailHeader({ item }: WorkDetailHeaderProps): JSX.Element {
  return (
    <header className="page__header work-detail__header">
      <div>
        <Link className="work-detail__back" to="/work">
          Work
        </Link>
        <h1 className="page__title">{item.key}</h1>
        <p className="page__subtitle">{item.title}</p>
      </div>
      <div className="work-detail__facts">
        <Fact label="Repo" value={workRepoName(item)} />
        <IssueFact issue={item.issue} />
        <Fact label="Updated" value={formatDate(item.updated_at)} />
      </div>
    </header>
  );
}

function Fact({ label, value }: { label: string; value: string }): JSX.Element {
  return (
    <div className="work-detail__fact">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function IssueFact({ issue }: { issue: WorkItem['issue'] }): JSX.Element {
  const label = issue ? `#${String(issue.number)}` : 'None';
  return (
    <div className="work-detail__fact">
      <span>Issue</span>
      <strong>{issue?.url ? <a href={issue.url}>{label}</a> : label}</strong>
    </div>
  );
}
