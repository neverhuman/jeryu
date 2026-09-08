// PriorityRow.tsx - single ranked priority/insight row.

import type { PriorityInsight } from '../../api/types';

import { SeverityIcon, StatePill } from './StateIndicators';

export function PriorityRow({
  priority,
}: {
  priority: PriorityInsight;
}): JSX.Element {
  return (
    <article
      className={`intelligence__priority is-${priority.severity}`}
      data-testid={`priority-${priority.id}`}
    >
      <div className="intelligence__priority-score">{priority.score}</div>
      <div className="intelligence__priority-main">
        <div className="intelligence__priority-title">
          <SeverityIcon severity={priority.severity} />
          <span>{priority.title}</span>
        </div>
        <div className="intelligence__priority-meta">
          <span>{priority.owner}</span>
          <span>{priority.proofLane}</span>
          <span>{priority.recommendedAction}</span>
        </div>
      </div>
      <StatePill state={priority.state} />
    </article>
  );
}
