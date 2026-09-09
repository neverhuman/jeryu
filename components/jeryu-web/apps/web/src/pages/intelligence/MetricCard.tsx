// MetricCard.tsx - control-plane summary metric card.

import type { ReactNode } from 'react';

import type { EvidenceState } from '../../api/types';

export function MetricCard({
  icon,
  label,
  value,
  detail,
  state,
}: {
  icon: ReactNode;
  label: string;
  value: number;
  detail: string;
  state: EvidenceState;
}): JSX.Element {
  return (
    <article className={`intelligence__metric is-${state}`}>
      <div className="intelligence__metric-icon">{icon}</div>
      <div>
        <div className="intelligence__metric-label">{label}</div>
        <div className="intelligence__metric-value">{value}</div>
        <div className="intelligence__metric-detail">{detail}</div>
      </div>
    </article>
  );
}
