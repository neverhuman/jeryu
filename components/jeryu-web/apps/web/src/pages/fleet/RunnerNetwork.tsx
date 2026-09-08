// fleet/RunnerNetwork.tsx — the runner-network drilldown view for the /fleet
// dashboard: the node board (slot grid), per-node cards, and the per-task TTY
// preview cards with terminal drill-down links. Extracted verbatim from
// FleetPage.tsx.

import { Link } from 'react-router-dom';
import {
  Activity,
  Cpu,
  Server,
  CircleAlert,
  CircleSlash,
  ExternalLink,
} from 'lucide-react';

import type { RunnerNetworkNode } from '../runnerNetworkModel';

export function RunnerNetworkBoard({
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

export function RunnerNodeCard({ node }: { node: RunnerNetworkNode }): JSX.Element {
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
function taskTerminalPath(task: RunnerNetworkNode['tasks'][number]): string | undefined {
  if (!task.repo || !task.agentRunId) return;
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
