// RiskBadge.tsx — risk-tier badge (icon + label) for action surfaces.
// The richer action UX treatment is layered on by W-FE-13.

import {
  AlertOctagon,
  AlertTriangle,
  Info,
  ShieldCheck,
  type LucideIcon,
} from 'lucide-react';

import './action.css';

export type RiskTier = 'low' | 'medium' | 'high' | 'critical';

const ICONS: Record<RiskTier, LucideIcon> = {
  low: ShieldCheck,
  medium: Info,
  high: AlertTriangle,
  critical: AlertOctagon,
};

const LABELS: Record<RiskTier, string> = {
  low: 'Low',
  medium: 'Medium',
  high: 'High',
  critical: 'Critical',
};

export interface RiskBadgeProps {
  tier: RiskTier;
  className?: string;
}

export function RiskBadge({ tier, className }: RiskBadgeProps): JSX.Element {
  const Icon = ICONS[tier];
  return (
    <span
      className={`risk-badge risk-badge--${tier} ${className ?? ''}`.trim()}
      aria-label={`Risk: ${LABELS[tier]}`}
    >
      <Icon aria-hidden="true" size={12} />
      {LABELS[tier]}
    </span>
  );
}
