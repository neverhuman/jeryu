import type {
  WorkItem,
  WorkItemKind,
  WorkPrincipal,
  WorkPriority,
  WorkStatus,
} from '../api/types';

export interface WorkFilters {
  repo: string;
  status: string;
  kind: string;
  priority: string;
  assignee: string;
  label: string;
  search: string;
}

export interface WorkLane {
  id: WorkStatus;
  title: string;
  items: WorkItem[];
}

export const WORK_STATUSES: WorkStatus[] = [
  'backlog',
  'ready',
  'in_progress',
  'blocked',
  'in_review',
  'done',
  'canceled',
];

export const WORK_KINDS: WorkItemKind[] = [
  'task',
  'bug',
  'chore',
  'docs',
  'ci',
];

export const WORK_PRIORITIES: WorkPriority[] = ['p0', 'p1', 'p2', 'p3', 'p4'];

export const DEFAULT_WORK_FILTERS: WorkFilters = {
  repo: 'all',
  status: 'all',
  kind: 'all',
  priority: 'all',
  assignee: 'all',
  label: 'all',
  search: '',
};

export const WORK_STATUS_LABELS: Record<WorkStatus, string> = {
  backlog: 'Backlog',
  ready: 'Ready',
  in_progress: 'In progress',
  blocked: 'Blocked',
  in_review: 'In review',
  done: 'Done',
  canceled: 'Canceled',
};

export const WORK_KIND_LABELS: Record<WorkItemKind, string> = {
  task: 'Task',
  bug: 'Bug',
  chore: 'Chore',
  docs: 'Docs',
  ci: 'CI',
};

export const WORK_PRIORITY_LABELS: Record<WorkPriority, string> = {
  p0: 'P0',
  p1: 'P1',
  p2: 'P2',
  p3: 'P3',
  p4: 'P4',
};

export function workRepoName(item: WorkItem): string {
  if (!item.repo) return 'Unscoped';
  return `${item.repo.owner}/${item.repo.name}`;
}

export function displayPrincipal(principal: WorkPrincipal): string {
  return principal.display_name?.trim() || principal.id;
}

export function filterWorkItems(
  items: WorkItem[],
  filters: WorkFilters
): WorkItem[] {
  const needle = filters.search.trim().toLowerCase();
  return items.filter((item) => {
    if (filters.repo !== 'all' && workRepoName(item) !== filters.repo) {
      return false;
    }
    if (filters.status !== 'all' && item.status !== filters.status) {
      return false;
    }
    if (filters.kind !== 'all' && item.kind !== filters.kind) {
      return false;
    }
    if (filters.priority !== 'all' && item.priority !== filters.priority) {
      return false;
    }
    if (
      filters.assignee !== 'all' &&
      !item.assignees.some((assignee) => assignee.id === filters.assignee)
    ) {
      return false;
    }
    if (filters.label !== 'all' && !item.labels.includes(filters.label)) {
      return false;
    }
    if (!needle) return true;
    return [
      item.key,
      item.title,
      item.body ?? '',
      workRepoName(item),
      item.kind,
      item.priority,
      item.status,
      ...item.labels,
      ...item.assignees.map(displayPrincipal),
      item.issue ? String(item.issue.number) : '',
      ...item.pull_requests.map((pull) => String(pull.number)),
    ]
      .join(' ')
      .toLowerCase()
      .includes(needle);
  });
}

export function groupWorkItems(items: WorkItem[]): WorkLane[] {
  const grouped = new Map<WorkStatus, WorkItem[]>(
    WORK_STATUSES.map((status) => [status, []])
  );
  for (const item of items) {
    grouped.get(item.status)?.push(item);
  }
  return WORK_STATUSES.map((status) => ({
    id: status,
    title: WORK_STATUS_LABELS[status],
    items: grouped.get(status) ?? [],
  }));
}

export function repoOptions(items: WorkItem[]): string[] {
  return Array.from(new Set(items.map(workRepoName))).sort();
}

export function labelOptions(items: WorkItem[]): string[] {
  return Array.from(new Set(items.flatMap((item) => item.labels))).sort();
}

export function assigneeOptions(items: WorkItem[]): WorkPrincipal[] {
  const byId = new Map<string, WorkPrincipal>();
  for (const item of items) {
    for (const assignee of item.assignees) {
      if (!byId.has(assignee.id)) byId.set(assignee.id, assignee);
    }
  }
  return Array.from(byId.values()).sort((a, b) =>
    displayPrincipal(a).localeCompare(displayPrincipal(b))
  );
}

export function csvTokens(value: string): string[] {
  return value
    .split(',')
    .map((token) => token.trim())
    .filter(Boolean);
}

export function principalsFromInput(value: string): WorkPrincipal[] {
  return csvTokens(value).map((raw) => {
    const kind = raw.startsWith('agent:') ? 'agent' : 'human';
    const id = kind === 'agent' ? raw.slice('agent:'.length) : raw;
    return { kind, id, display_name: null };
  });
}

export interface EditState {
  title: string;
  body: string;
  status: WorkStatus;
  kind: WorkItemKind;
  priority: WorkPriority;
  labels: string;
  assignees: string;
}

export function emptyEdit(): EditState {
  return {
    title: '',
    body: '',
    status: 'backlog',
    kind: 'task',
    priority: 'p2',
    labels: '',
    assignees: '',
  };
}

export function formatDate(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(value));
}
