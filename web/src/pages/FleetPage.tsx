// FleetPage.tsx — cross-repo runner-pool + system-health operator dashboard.
//
// The /fleet page is the operator's single pane for the whole runner fabric:
// utilization across ALL repos, per-pool job/runner counts, saturation /
// pause state, the scm/database/sandbox/cache/vault health strip, and the
// derived bottleneck/health alert banner. It is live: it subscribes to the
// existing realtime socket for `global.activity` + `system.health` (and reads
// the structured first-paint snapshot from the bootstrap so it is never
// empty), then folds the rolling event buffer through the pure projection in
// `fleetModel.ts`. A "stale" badge appears when no fresh frame has arrived
// within the TTL.

import { useMemo } from 'react';
import { Activity, AlertTriangle, CheckCircle2, Cpu, Server } from 'lucide-react';

import { useBootstrap } from '../hooks/useBootstrap';
import { useRealtime } from '../hooks/useRealtime';
import { useRealtimeStore } from '../stores/realtimeStore';
import type { WebEvent } from '../api/types';
import {
  applyFleetEvents,
  fleetStateFromBootstrap,
  isStale,
  type FleetComponent,
  type FleetHealth,
  type FleetPool,
  type FleetState,
} from './fleetModel';

import './page.css';
import './FleetPage.css';

/** Frames older than this are surfaced as "stale" to the operator. */
export const FLEET_STALE_TTL_MS = 30_000;

/** Scopes the page subscribes to and folds (also accepts `pool.*` frames). */
const FLEET_SCOPES = ['global.activity', 'system.health'];

/** Pure builder kept exported so the render test can drive it directly. */
export function buildFleetState(
  tui: unknown,
  events: readonly WebEvent[]
): FleetState {
  return applyFleetEvents(fleetStateFromBootstrap(tui), events);
}

export function FleetPage(): JSX.Element {
  const bootstrap = useBootstrap();
  const realtimeStatus = useRealtimeStore((s) => s.status);
  const events = useRealtimeStore((s) => s.events);

  useRealtime(FLEET_SCOPES);

  const tui = bootstrap.data?.tui;
  const state = useMemo(
    () => buildFleetState(tui, fleetRelevantEvents(events)),
    [tui, events]
  );

  const stale = isStale(state.lastUpdated, FLEET_STALE_TTL_MS);

  return (
    <div className="page" data-testid="fleet-page">
      <header className="page__header">
        <div className="fleet__header-bar">
          <h1 className="page__title">Fleet</h1>
          <HealthBadge health={state.health} />
          {stale ? (
            <span
              className="page__pill page__pill--warning"
              data-testid="fleet-stale-badge"
              title={
                state.lastUpdated
                  ? `Last update ${state.lastUpdated}`
                  : 'No data received yet'
              }
            >
              stale
            </span>
          ) : null}
          <RealtimePill status={realtimeStatus} />
        </div>
        <p className="page__subtitle">
          Server-wide runner-pool activity across every repository — utilization,
          queued and running work, online and stuck runners, and infrastructure
          health.
        </p>
        <div className="fleet__header-bar">
          <div className="fleet__util">
            <span className="fleet__util-value">
              {Math.round(state.utilization * 100)}%
            </span>
            <span className="fleet__util-label">fleet utilization</span>
          </div>
          <span className="page__pill">{state.totals.pools} pool(s)</span>
          <span className="page__pill">{state.totals.repos} repo(s)</span>
          <span className="page__pill">{state.totals.runningJobs} running</span>
          <span className="page__pill">{state.totals.queuedJobs} queued</span>
          <span
            className={`page__pill${
              state.totals.failedJobs > 0 ? ' page__pill--danger' : ''
            }`}
          >
            {state.totals.failedJobs} failed
          </span>
          <span
            className={`page__pill${
              state.stuckRunnerTotal > 0 ? ' page__pill--danger' : ''
            }`}
          >
            {state.totals.onlineRunners} runners · {state.stuckRunnerTotal} stuck
          </span>
        </div>
      </header>

      <BottleneckBanner health={state.health} bottlenecks={state.bottlenecks} />

      <section className="page__section" aria-labelledby="fleet-pools">
        <h2 className="page__section-title" id="fleet-pools">
          Runner pools
        </h2>
        {state.pools.length === 0 ? (
          <p className="page__roadmap-note">
            No runner pools are reporting yet. Pools appear here as soon as the
            scheduler registers them.
          </p>
        ) : (
          <div className="page__cards">
            {state.pools.map((pool) => (
              <PoolCard key={pool.pool} pool={pool} />
            ))}
          </div>
        )}
      </section>

      <section className="page__section" aria-labelledby="fleet-system">
        <h2 className="page__section-title" id="fleet-system">
          System health
        </h2>
        {state.components.length === 0 ? (
          <p className="page__roadmap-note">No system health reported yet.</p>
        ) : (
          <div className="fleet__health-strip" data-testid="fleet-health-strip">
            {state.components.map((component) => (
              <ComponentCell key={component.name} component={component} />
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

/** Keep only the scopes this page folds, so an unrelated event cannot leak in. */
function fleetRelevantEvents(events: readonly WebEvent[]): WebEvent[] {
  return events.filter(
    (e) =>
      e.scope === 'global.activity' ||
      e.scope === 'system.health' ||
      e.scope.startsWith('pool.')
  );
}

function PoolCard({ pool }: { pool: FleetPool }): JSX.Element {
  const utilPct = Math.round(pool.utilization * 100);
  const fillVariant = pool.saturated
    ? 'fleet__bar-fill--danger'
    : utilPct >= 80
      ? 'fleet__bar-fill--warning'
      : '';
  const cardClass = [
    'fleet__pool-card',
    pool.stuckRunners > 0 ? 'is-stuck' : '',
    pool.saturated ? 'is-saturated' : '',
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
          {pool.paused ? (
            <span className="page__pill page__pill--warning">paused</span>
          ) : null}
          {pool.saturated ? (
            <span className="page__pill page__pill--danger">saturated</span>
          ) : null}
          <span className="page__pill">{pool.trustTier}</span>
        </div>
      </div>

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

      <dl className="fleet__stat-grid">
        <div className="fleet__stat">
          <dt>Utilization</dt>
          <dd>{utilPct}%</dd>
        </div>
        <div className="fleet__stat">
          <dt>Slots</dt>
          <dd>
            {pool.idleSlots} idle / {pool.activeSlots}
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
            {pool.onlineRunners} on · {pool.stuckRunners} stuck
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

function ComponentCell({
  component,
}: {
  component: FleetComponent;
}): JSX.Element {
  return (
    <div
      className="fleet__health-cell"
      data-testid={`fleet-health-${component.name}`}
    >
      <span
        className={`fleet__dot fleet__dot--${component.status}`}
        aria-hidden="true"
      />
      <div className="fleet__health-meta">
        <span className="fleet__health-name">{component.name}</span>
        <span className="fleet__health-detail">
          {component.status}
          {component.latencyMs !== null ? ` · ${component.latencyMs}ms` : ''}
          {component.detail ? ` · ${component.detail}` : ''}
        </span>
      </div>
    </div>
  );
}

function BottleneckBanner({
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

function HealthBadge({ health }: { health: FleetHealth }): JSX.Element {
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

function RealtimePill({ status }: { status: string }): JSX.Element {
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
