// EvidencePanel.tsx - evidence-freshness panel for a single evidence source.

import type { ReactNode } from 'react';

import type { EvidenceState } from '../../api/types';

import { StatePill } from './StateIndicators';

export function EvidencePanel({
  icon,
  title,
  state,
  body,
}: {
  icon: ReactNode;
  title: string;
  state: EvidenceState;
  body: string;
}): JSX.Element {
  return (
    <article className={`intelligence__evidence is-${state}`}>
      <div className="intelligence__evidence-top">
        <span className="intelligence__evidence-icon">{icon}</span>
        <h3>{title}</h3>
        <StatePill state={state} />
      </div>
      <p>{body}</p>
    </article>
  );
}
