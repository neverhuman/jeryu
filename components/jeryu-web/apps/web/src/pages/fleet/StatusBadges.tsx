// fleet/StatusBadges.tsx — the header status pills for the /fleet dashboard:
// the overall pool-fabric health badge and the realtime connection pill.
// Extracted verbatim from FleetPage.tsx.

import { Activity, Cpu, Server } from 'lucide-react';

import type { FleetHealth } from '../fleetModel';

export function HealthBadge({ health }: { health: FleetHealth }): JSX.Element {
  const variant =
    health === 'healthy'
      ? 'success'
      : health === 'critical'
        ? 'danger'
        : health === 'unknown'
          ? ''
          : 'warning';
  const Icon = health === 'healthy' ? Server : Cpu;
  return (
    <span
      className={`page__pill${variant ? ` page__pill--${variant}` : ''}`}
      data-testid="fleet-health-badge"
    >
      <Icon size={10} aria-hidden="true" /> {health}
    </span>
  );
}

export function RealtimePill({ status }: { status: string }): JSX.Element {
  const variant: 'success' | 'warning' | 'danger' =
    status === 'open'
      ? 'success'
      : status === 'connecting' || status === 'reconnecting'
        ? 'warning'
        : 'danger';
  return (
    <span className={`page__pill page__pill--${variant}`}>
      <Activity size={10} aria-hidden="true" /> {status}
    </span>
  );
}
