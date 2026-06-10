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

import { useMemo } from 'react';
import { Link } from 'react-router-dom';
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  Cpu,
  Server,
  CircleAlert,
  CircleSlash,
  ExternalLink,
} from 'lucide-react';

import { useBootstrap } from '../hooks/useBootstrap';
import { useControlPlaneRunners } from '../hooks/useControlPlaneRunners';
import { useRealtime } from '../hooks/useRealtime';
import { useRealtimeStore } from '../stores/realtimeStore';
import type { WebEvent } from '../api/types';
import {
  applyFleetEvents,
  fleetStateFromBootstrap,
  isOutOfDate,
  type FleetComponent,
  type FleetHealth,
  type FleetPool,
  type FleetState,
} from './fleetModel';
import {
  runnerNetworkFromResponse,
  type RunnerNetworkNode,
} from './runnerNetworkModel';

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

function RunnerNetworkBoard({
  nodes,
}: {
  nodes: RunnerNetworkNode[];
}): JSX.Element {
  return (
    <div
      className="fleet__network-board"
      data-testid="fleet-node-board"
      aria-label="Runner node grid"
    >
      {nodes.map((node) => (
        <RunnerNodeBox key={node.runnerId} node={node} />
      ))}
    </div>
  );
}

function RunnerNodeBox({ node }: { node: RunnerNetworkNode }): JSX.Element {
  const nodeId = testIdSegment(node.runnerId);
  const usedSlots = Math.min(
    Math.max(node.inFlight, node.activeTaskCount),
    Math.max(node.capacity, 0)
  );
  const visibleSlots = Math.min(Math.max(node.capacity, 1), 24);
  return (
    <article
      className={`fleet__node-box is-${node.availability} is-${node.activityState}`}
      data-testid={`fleet-node-box-${nodeId}`}
      aria-label={`Runner node ${node.runnerId}: ${node.availability}, ${node.activityState}`}
    >
      <div className="fleet__node-box-top">
        <span className="fleet__node-box-name">{node.runnerId}</span>
        <span className="fleet__node-box-count">
          {usedSlots}/{node.capacity}
        </span>
      </div>
      <div className="fleet__slot-grid" aria-hidden="true">
        {Array.from({ length: visibleSlots }).map((_, index) => (
          <span
            // Slot positions are stable because capacity is stable per node.
            key={`${node.runnerId}-slot-${index}`}
            className={`fleet__slot${index < usedSlots ? ' is-used' : ''}`}
          />
        ))}
      </div>
      <div className="fleet__node-box-foot">
        <span>{node.source}</span>
        <span>{node.activityState}</span>
      </div>
    </article>
  );
}

function RunnerNodeCard({ node }: { node: RunnerNetworkNode }): JSX.Element {
  const nodeId = testIdSegment(node.runnerId);
  return (
    <article
      className="fleet__node-card"
      data-testid={`fleet-node-${nodeId}`}
      aria-label={`Runner node ${node.runnerId}`}
    >
      <div className="fleet__node-head">
        <div className="fleet__node-titleblock">
          <h3 className="fleet__node-title">{node.runnerId}</h3>
          <p className="fleet__node-source">{node.source}</p>
        </div>
        <div className="fleet__node-pills">
          <AvailabilityPill availability={node.availability} />
          <ActivityPill activity={node.activityState} />
        </div>
      </div>

      <dl className="fleet__node-stats">
        <div className="fleet__node-stat">
          <dt>Capacity</dt>
          <dd>{node.capacity}</dd>
        </div>
        <div className="fleet__node-stat">
          <dt>In flight</dt>
          <dd>{node.inFlight}</dd>
        </div>
        <div className="fleet__node-stat">
          <dt>Active tasks</dt>
          <dd>{node.activeTaskCount}</dd>
        </div>
        <div className="fleet__node-stat">
          <dt>Last updated</dt>
          <dd>{node.lastUpdated ?? 'unknown'}</dd>
        </div>
      </dl>

      <div className="fleet__node-tags">
        {node.labels.length > 0 ? (
          node.labels.map((label) => (
            <span className="page__pill" key={label}>
              {label}
            </span>
          ))
        ) : (
          <span className="page__pill page__pill--warning">no labels</span>
        )}
        {node.classes.length > 0 ? (
          node.classes.map((runnerClass) => (
            <span className="page__pill" key={runnerClass}>
              {runnerClass}
            </span>
          ))
        ) : (
          <span className="page__pill page__pill--warning">no classes</span>
        )}
      </div>

      {node.tasks.length > 0 ? (
        <div className="fleet__task-list">
          {node.tasks.map((task) => (
            <RunnerTaskCard key={task.taskId} task={task} />
          ))}
        </div>
      ) : (
        <p className="fleet__node-empty">
          No active tasks on this node right now.
        </p>
      )}
    </article>
  );
}

/** Build the drill-down URL for a task that has both a repo and an agent run id. */
function taskTerminalPath(task: RunnerNetworkNode['tasks'][number]): string | null {
  if (!task.repo || !task.agentRunId) return null;
  const provider = 'jeryu';
  const fullName = encodeURIComponent(task.repo);
  return `/repos/${encodeURIComponent(provider)}/${fullName}/agents/${encodeURIComponent(task.agentRunId)}`;
}

function RunnerTaskCard({ task }: { task: RunnerNetworkNode['tasks'][number] }): JSX.Element {
  const taskId = testIdSegment(task.taskId);
  const drillPath = taskTerminalPath(task);
  const cardContent = (
    <>
      <div className="fleet__task-head">
        <h4 className="fleet__task-title">{task.label}</h4>
        <span className="page__pill page__pill--warning">{task.state}</span>
      </div>
      <p className="fleet__task-meta">
        <span>{task.jobId}</span>
        {task.workcellId ? <span>workcell {task.workcellId}</span> : null}
        {task.agentRunId ? <span>agent {task.agentRunId}</span> : null}
        {task.repo ? <span>{task.repo}</span> : <span>repo unavailable</span>}
      </p>
      <p className="fleet__task-program">{task.program}</p>
      <p className="fleet__task-tty">
        {task.lastTtyLine ?? 'TTY preview unavailable.'}
      </p>
      {drillPath ? (
        <span className="fleet__task-open">
          <ExternalLink size={12} aria-hidden="true" /> Open terminal
        </span>
      ) : null}
    </>
  );
  if (drillPath) {
    return (
      <Link
        to={drillPath}
        className="fleet__task-card fleet__task-card--interactive"
        data-testid={`fleet-task-${taskId}`}
        aria-label={`Open terminal for ${task.label}`}
      >
        {cardContent}
      </Link>
    );
  }
  return (
    <article
      className="fleet__task-card"
      data-testid={`fleet-task-${taskId}`}
      aria-label={`Runner task ${task.label}`}
    >
      {cardContent}
    </article>
  );
}

function AvailabilityPill({
  availability,
}: {
  availability: RunnerNetworkNode['availability'];
}): JSX.Element {
  const variant =
    availability === 'online'
      ? 'success'
      : availability === 'draining'
        ? 'warning'
        : availability === 'offline'
          ? 'danger'
          : '';
  const Icon =
    availability === 'offline' ? CircleSlash : availability === 'draining' ? CircleAlert : Server;
  return (
    <span className={`page__pill${variant ? ` page__pill--${variant}` : ''}`}>
      <Icon size={10} aria-hidden="true" /> {availability}
    </span>
  );
}

function ActivityPill({
  activity,
}: {
  activity: RunnerNetworkNode['activityState'];
}): JSX.Element {
  const variant =
    activity === 'active'
      ? 'success'
      : activity === 'idle'
        ? ''
        : 'warning';
  const Icon = activity === 'active' ? Activity : Cpu;
  return (
    <span className={`page__pill${variant ? ` page__pill--${variant}` : ''}`}>
      <Icon size={10} aria-hidden="true" /> {activity}
    </span>
  );
}

function testIdSegment(value: string): string {
  return value.replace(/[^a-zA-Z0-9_-]/g, '_');
}
