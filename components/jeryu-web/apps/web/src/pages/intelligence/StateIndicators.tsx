// StateIndicators.tsx - shared state/severity pill and icon primitives.

import { AlertTriangle, CheckCircle2, Circle } from 'lucide-react';

import type { EvidenceState, InsightSeverity } from '../../api/types';

export function StatePill({
  state,
  label,
}: {
  state: EvidenceState;
  label?: string;
}): JSX.Element {
  return (
    <span className={`intelligence__state is-${state}`}>
      {label ? `${label}: ` : ''}
      {state}
    </span>
  );
}

export function SeverityPill({
  severity,
}: {
  severity: InsightSeverity;
}): JSX.Element {
  return (
    <span className={`intelligence__severity is-${severity}`}>{severity}</span>
  );
}

export function SeverityIcon({
  severity,
}: {
  severity: InsightSeverity;
}): JSX.Element {
  if (severity === 'critical' || severity === 'high') {
    return <AlertTriangle size={16} aria-hidden="true" />;
  }
  if (severity === 'medium') {
    return <Circle size={15} aria-hidden="true" />;
  }
  return <CheckCircle2 size={15} aria-hidden="true" />;
}
