// FleetPage.tsx — cross-repo runner-pool + system-health operator dashboard.
//
// The /fleet page is the operator's single pane for the whole runner fabric:
// utilization across ALL repos, per-pool job/runner counts, saturation /
// pause state, the scm/database/sandbox/cache/vault health strip, and the
// derived bottleneck/health alert banner. It is live: it subscribes to the
// existing realtime socket for `global.activity` + `system.health` (and reads
// the structured first-paint snapshot from the bootstrap so it is never
// empty), then folds the rolling event buffer through the pure projection in
// `fleetModel.ts`. A freshness badge appears when no recent frame has arrived
// within the TTL.
//
// This module is a thin composition shell: the per-pool cards, the health
// strip cells, the bottleneck banner, the header status badges, and the
// runner-network drilldown all live in `./fleet/*` and are re-exported below.

import { useMemo } from 'react';

import { useBootstrap } from '../hooks/useBootstrap';
import { useControlPlaneRunners } from '../hooks/useControlPlaneRunners';
import { useRealtime } from '../hooks/useRealtime';
import { useRealtimeStore } from '../stores/realtimeStore';
import type { WebEvent } from '../api/types';
import {
  applyFleetEvents,
  fleetStateFromBootstrap,
  isOutOfDate,
  type FleetState,
} from './fleetModel';
import { runnerNetworkFromResponse } from './runnerNetworkModel';
import {
  BottleneckBanner,
  ComponentCell,
  HealthBadge,
  PoolCard,
  RealtimePill,
  RunnerNetworkBoard,
  RunnerNodeCard,
} from './fleet';

import './page.css';
import './FleetPage.css';

/** Frames older than this are surfaced as out of date to the operator. */
export const FLEET_FRESHNESS_TTL_MS = 30_000;

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
  const runnersQuery = useControlPlaneRunners();
  const realtimeStatus = useRealtimeStore((s) => s.status);
  const events = useRealtimeStore((s) => s.events);

  useRealtime(FLEET_SCOPES);

  const tui = bootstrap.data?.tui;
  const state = useMemo(
    () => buildFleetState(tui, fleetRelevantEvents(events)),
    [tui, events]
  );
  const runnerNetwork = useMemo(
    () => runnerNetworkFromResponse(runnersQuery.data),
    [runnersQuery.data]
  );

  const outOfDate = isOutOfDate(state.lastUpdated, FLEET_FRESHNESS_TTL_MS);
  const runnerNetworkNote = runnersQuery.isError
    ? runnersQuery.error?.message ?? 'Runner network snapshot unavailable.'
    : runnersQuery.isLoading
      ? 'Loading runner network snapshot.'
      : null;

  return (
    <div className="page" data-testid="fleet-page">
      <header className="page__header">
        <div className="fleet__header-bar">
          <h1 className="page__title">Fleet</h1>
          <HealthBadge health={state.health} />
          {outOfDate ? (
            <span
              className="page__pill page__pill--warning"
              data-testid="fleet-freshness-badge"
              title={
                state.lastUpdated
                  ? `Last updated ${state.lastUpdated}`
                  : 'No data received yet'
              }
            >
              out of date
            </span>
          ) : null}
          <RealtimePill status={realtimeStatus} />
        </div>
        <p className="page__subtitle">
          Live runner-network drilldown across the local fabric — node
          availability, active tasks, TTY previews, and the existing pool and
          system-health summaries.
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
          <span className="page__pill">
            {runnerNetwork.totals.nodes} node(s)
          </span>
          <span className="page__pill">
            {runnerNetwork.totals.activeTasks} active task(s)
          </span>
          <span className="page__pill">
            {runnerNetwork.totals.onlineNodes} online
          </span>
          <span
            className={`page__pill${
              runnerNetwork.totals.offlineNodes > 0 ? ' page__pill--warning' : ''
            }`}
          >
            {runnerNetwork.totals.offlineNodes} offline
          </span>
        </div>
      </header>

      <BottleneckBanner health={state.health} bottlenecks={state.bottlenecks} />

      <section className="page__section" aria-labelledby="fleet-runners">
        <div className="fleet__section-head">
          <h2 className="page__section-title" id="fleet-runners">
            Runner network
          </h2>
          <span className="page__pill">
            {runnerNetwork.state}
          </span>
          <span className="page__pill">
            {runnerNetwork.totals.capacity} slots
          </span>
          <span className="page__pill">
            {runnerNetwork.totals.inFlight} in flight
          </span>
        </div>
        {runnerNetworkNote ? (
          <p className="page__roadmap-note">{runnerNetworkNote}</p>
        ) : runnerNetwork.nodes.length === 0 ? (
          <p className="page__roadmap-note">
            No runner nodes are reporting yet. The network appears here as soon
            as the backend exposes real node snapshots.
          </p>
        ) : (
          <div className="fleet__network-layout" data-testid="fleet-network">
            <RunnerNetworkBoard nodes={runnerNetwork.nodes} />
            <div className="fleet__network-grid">
              {runnerNetwork.nodes.map((node) => (
                <RunnerNodeCard key={node.runnerId} node={node} />
              ))}
            </div>
          </div>
        )}
      </section>

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
