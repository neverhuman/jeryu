// fleet/BottleneckBanner.tsx — the derived bottleneck/health alert banner for
// the /fleet dashboard. Shows the active bottleneck lines (most severe first)
// or a healthy/awaiting status row. Extracted verbatim from FleetPage.tsx.

import { Activity, AlertTriangle, CheckCircle2 } from 'lucide-react';

import type { FleetHealth } from '../fleetModel';

export function BottleneckBanner({
  health,
  bottlenecks,
}: {
  health: FleetHealth;
  bottlenecks: string[];
}): JSX.Element {
  const stuckCount = bottlenecks.filter((b) => b.includes('STUCK')).length;
  if (bottlenecks.length === 0) {
    const ok = health === 'healthy';
    return (
      <div
        className={`fleet__banner fleet__banner--${ok ? 'healthy' : health}`}
        role="status"
        data-testid="fleet-banner"
      >
        <span className="fleet__banner-title">
          {ok ? (
            <CheckCircle2 size={16} aria-hidden="true" />
          ) : (
            <Activity size={16} aria-hidden="true" />
          )}
          {ok
            ? 'All pools healthy — no bottlenecks.'
            : 'Awaiting fleet telemetry.'}
        </span>
      </div>
    );
  }
  return (
    <div
      className={`fleet__banner fleet__banner--${health}`}
      role="alert"
      data-testid="fleet-banner"
    >
      <span className="fleet__banner-title">
        <AlertTriangle size={16} aria-hidden="true" />
        {bottlenecks.length} active bottleneck(s)
        {stuckCount > 0 ? ` · ${stuckCount} with stuck runners` : ''}
      </span>
      <ul className="fleet__banner-list">
        {bottlenecks.map((line) => (
          <li key={line}>{line}</li>
        ))}
      </ul>
    </div>
  );
}
