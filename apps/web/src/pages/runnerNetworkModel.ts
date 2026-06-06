// runnerNetworkModel.ts — pure selectors for the /fleet runner-network drilldown.
//
// The Fleet page consumes the shared runner-fabric response directly and keeps
// the node/task/TTY projection isolated from React. That lets the page show the
// authoritative local node snapshot first, while still deriving a concise view
// for active/idle availability, task counts, and last-TTY-line previews.

import type {
  EvidenceState,
  RunnerFabricResponse,
  RunnerNodeSummary,
  RunnerTaskSummary,
  RunnerTtyPreview,
} from '../api/types';

export type RunnerAvailability =
  | 'online'
  | 'draining'
  | 'offline'
  | 'unknown';

export type RunnerActivityState = 'active' | 'idle' | 'unknown';

export interface RunnerNetworkTask {
  taskId: string;
  jobId: string;
  agentRunId: string | null;
  workcellId: string | null;
  repo: string | null;
  label: string;
  program: string;
  state: string;
  startedAt: string | null;
  updatedAt: string | null;
  ttyState: EvidenceState;
  lastTtyLine: string | null;
}

export interface RunnerNetworkNode {
  runnerId: string;
  source: string;
  state: string;
  availability: RunnerAvailability;
  activityState: RunnerActivityState;
  capacity: number;
  inFlight: number;
  labels: string[];
  classes: string[];
  activeTaskCount: number;
  lastUpdated: string | null;
  tasks: RunnerNetworkTask[];
}

export interface RunnerNetworkTotals {
  nodes: number;
  onlineNodes: number;
  offlineNodes: number;
  busyNodes: number;
  idleNodes: number;
  activeTasks: number;
  capacity: number;
  inFlight: number;
}

export interface RunnerNetworkState {
  state: EvidenceState;
  nodes: RunnerNetworkNode[];
  totals: RunnerNetworkTotals;
  lastUpdated: string | null;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null
    ? (value as Record<string, unknown>)
    : null;
}

function str(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function num(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

function strList(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

export function lastTtyLine(preview: RunnerTtyPreview | null | undefined): string | null {
  if (!preview || preview.lines.length === 0) {
    return null;
  }
  const lines = preview.lines
    .flatMap((line) => line.split(/\r?\n/))
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  return lines.length === 0 ? null : lines[lines.length - 1];
}

function availabilityFromState(state: string): RunnerAvailability {
  switch (state) {
    case 'draining':
      return 'draining';
    case 'dead':
    case 'down':
    case 'fenced':
    case 'offline':
    case 'quarantined':
      return 'offline';
    case '':
      return 'unknown';
    default:
      return 'online';
  }
}

function activityFromNode(
  state: RunnerAvailability,
  inFlight: number,
  taskCount: number
): RunnerActivityState {
  if (inFlight > 0 || taskCount > 0) {
    return 'active';
  }
  if (state === 'online' || state === 'draining') {
    return 'idle';
  }
  return 'unknown';
}

function taskFromRaw(raw: RunnerTaskSummary): RunnerNetworkTask {
  return {
    taskId: raw.taskId,
    jobId: raw.jobId,
    agentRunId: raw.agentRunId,
    workcellId: raw.workcellId,
    repo: raw.repo,
    label: raw.label,
    program: raw.program,
    state: raw.state,
    startedAt: raw.startedAt,
    updatedAt: raw.updatedAt,
    ttyState: raw.ttyPreview.state,
    lastTtyLine: lastTtyLine(raw.ttyPreview),
  };
}

function nodeFromRaw(raw: RunnerNodeSummary): RunnerNetworkNode {
  const tasks = raw.activeTasks.map(taskFromRaw);
  const availability = availabilityFromState(raw.state);
  const activeTaskCount = tasks.length || raw.activeTaskCount;
  const lastUpdated =
    raw.lastUpdated ??
    tasks
      .map((task) => task.updatedAt)
      .filter((value): value is string => typeof value === 'string' && value.length > 0)
      .sort()
      .at(-1) ??
    null;
  return {
    runnerId: raw.runnerId,
    source: raw.source,
    state: raw.state,
    availability,
    activityState: activityFromNode(availability, raw.inFlight, activeTaskCount),
    capacity: raw.capacity,
    inFlight: raw.inFlight,
    labels: raw.labels,
    classes: raw.classes,
    activeTaskCount,
    lastUpdated,
    tasks,
  };
}

function totalsFromNodes(nodes: RunnerNetworkNode[]): RunnerNetworkTotals {
  return nodes.reduce<RunnerNetworkTotals>(
    (acc, node) => ({
      nodes: acc.nodes + 1,
      onlineNodes: acc.onlineNodes + (node.availability === 'online' ? 1 : 0),
      offlineNodes: acc.offlineNodes + (node.availability === 'offline' ? 1 : 0),
      busyNodes: acc.busyNodes + (node.activityState === 'active' ? 1 : 0),
      idleNodes: acc.idleNodes + (node.activityState === 'idle' ? 1 : 0),
      activeTasks: acc.activeTasks + node.activeTaskCount,
      capacity: acc.capacity + node.capacity,
      inFlight: acc.inFlight + node.inFlight,
    }),
    {
      nodes: 0,
      onlineNodes: 0,
      offlineNodes: 0,
      busyNodes: 0,
      idleNodes: 0,
      activeTasks: 0,
      capacity: 0,
      inFlight: 0,
    }
  );
}

export function runnerNetworkFromResponse(
  response: RunnerFabricResponse | null | undefined
): RunnerNetworkState {
  const raw = asRecord(response);
  const local = asRecord(raw?.local);
  const nodeDetails = Array.isArray(local?.nodeDetails) ? local.nodeDetails : [];
  const nodes = nodeDetails
    .map((node) => {
      const record = asRecord(node);
      if (!record) return null;
      const tasks = Array.isArray(record.activeTasks) ? record.activeTasks : [];
      return nodeFromRaw({
        runnerId: str(record.runnerId),
        source: str(record.source) || 'local',
        state: str(record.state),
        capacity: num(record.capacity),
        inFlight: num(record.inFlight),
        labels: strList(record.labels),
        classes: strList(record.classes),
        activeTaskCount: num(record.activeTaskCount),
        lastUpdated: typeof record.lastUpdated === 'string' ? record.lastUpdated : null,
        activeTasks: tasks
          .map((task) => {
            const taskRecord = asRecord(task);
            if (!taskRecord) return null;
            const ttyRecord = asRecord(taskRecord.ttyPreview);
            return {
              taskId: str(taskRecord.taskId),
              jobId: str(taskRecord.jobId),
              agentRunId:
                typeof taskRecord.agentRunId === 'string' ? taskRecord.agentRunId : null,
              workcellId:
                typeof taskRecord.workcellId === 'string' ? taskRecord.workcellId : null,
              repo: typeof taskRecord.repo === 'string' ? taskRecord.repo : null,
              label: str(taskRecord.label),
              program: str(taskRecord.program),
              state: str(taskRecord.state),
              startedAt:
                typeof taskRecord.startedAt === 'string' ? taskRecord.startedAt : null,
              updatedAt:
                typeof taskRecord.updatedAt === 'string' ? taskRecord.updatedAt : null,
              ttyPreview: {
                state:
                  typeof ttyRecord?.state === 'string'
                    ? (ttyRecord.state as EvidenceState)
                    : 'unknown',
                lines: strList(ttyRecord?.lines),
              },
            } satisfies RunnerTaskSummary;
          })
          .filter((task): task is RunnerTaskSummary => task !== null),
      });
    })
    .filter((node): node is RunnerNetworkNode => node !== null)
    .sort((a, b) => a.runnerId.localeCompare(b.runnerId));

  const totals = totalsFromNodes(nodes);
  const lastUpdated =
    (typeof local?.lastUpdated === 'string' ? local.lastUpdated : null) ??
    nodes
      .map((node) => node.lastUpdated)
      .filter((value): value is string => typeof value === 'string' && value.length > 0)
      .sort()
      .at(-1) ??
    null;

  return {
    state:
      typeof local?.state === 'string'
        ? (local.state as EvidenceState)
        : 'unknown',
    nodes,
    totals,
    lastUpdated,
  };
}
