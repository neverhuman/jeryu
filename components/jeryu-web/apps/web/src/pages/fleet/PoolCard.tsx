// fleet/PoolCard.tsx — per-pool utilization card for the /fleet dashboard.
//
// Renders one runner pool: utilization bar, idle/active slots, running/queued/
// failed job counts, online/stuck runners, and the paused/saturated/trust-tier
// tags. Extracted verbatim from FleetPage.tsx.

import type { FleetPool } from '../fleetModel';

export function PoolCard({
  pool,
  capacityAvailable,
}: {
  pool: FleetPool;
  capacityAvailable: boolean;
}): JSX.Element {
  const capacityKnown = capacityAvailable && pool.capacityKnown;
  const utilPct = Math.round(pool.utilization * 100);
  const fillVariant = pool.saturated
    ? 'fleet__bar-fill--danger'
    : utilPct >= 80
      ? 'fleet__bar-fill--warning'
      : '';
  const cardClass = [
    'fleet__pool-card',
    capacityKnown && pool.stuckRunners > 0 ? 'is-stuck' : '',
    capacityKnown && pool.saturated ? 'is-saturated' : '',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <article
      className={cardClass}
      data-testid={`fleet-pool-${pool.pool}`}
      aria-label={`Runner pool ${pool.pool}`}
    >
      <div className="fleet__pool-head">
        <h3 className="fleet__pool-name">{pool.pool}</h3>
        <div className="fleet__pool-tags">
          {capacityKnown && pool.paused ? (
            <span className="page__pill page__pill--warning">paused</span>
          ) : null}
          {capacityKnown && pool.saturated ? (
            <span className="page__pill page__pill--danger">saturated</span>
          ) : null}
          <span className="page__pill">
            {capacityKnown ? pool.trustTier : 'capacity unknown'}
          </span>
        </div>
      </div>

      {capacityKnown ? (
        <div>
          <div
            className="fleet__bar"
            role="progressbar"
            aria-valuenow={utilPct}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-label={`${pool.pool} utilization`}
          >
            <div
              className={`fleet__bar-fill ${fillVariant}`}
              style={{ width: `${utilPct}%` }}
            />
          </div>
        </div>
      ) : null}

      <dl className="fleet__stat-grid">
        <div className="fleet__stat">
          <dt>Utilization</dt>
          <dd>{capacityKnown ? `${utilPct}%` : 'unknown'}</dd>
        </div>
        <div className="fleet__stat">
          <dt>Slots</dt>
          <dd>
            {capacityKnown ? `${pool.idleSlots} idle / ${pool.activeSlots}` : 'unknown'}
          </dd>
        </div>
        <div className="fleet__stat">
          <dt>Running</dt>
          <dd>{pool.runningJobs}</dd>
        </div>
        <div className="fleet__stat">
          <dt>Queued</dt>
          <dd>{pool.queuedJobs}</dd>
        </div>
        <div className="fleet__stat">
          <dt>Failed</dt>
          <dd>{pool.failedJobs}</dd>
        </div>
        <div className="fleet__stat">
          <dt>Runners</dt>
          <dd>
            {capacityKnown ? `${pool.onlineRunners} on · ${pool.stuckRunners} stuck` : 'unknown'}
          </dd>
        </div>
      </dl>

      {pool.tags.length > 0 ? (
        <div className="fleet__pool-tags">
          {pool.tags.map((tag) => (
            <span className="page__pill" key={tag}>
              {tag}
            </span>
          ))}
        </div>
      ) : null}
    </article>
  );
}
