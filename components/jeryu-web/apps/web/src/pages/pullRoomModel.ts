import type {
  ControlPlaneSnapshot,
  ControlPullRequest,
  EvidenceState,
  PullRequestSummary,
  ToolBuildCluster,
  ToolBuildClusterSummary,
} from '../api/types';

export type CheckPosture =
  | 'missing'
  | 'failing'
  | 'running'
  | 'queued'
  | 'passing';

export type PullLaneId =
  | 'missing_checks'
  | 'failing_checks'
  | 'queued_running_checks'
  | 'ready_reviewable'
  | 'merged_closed';

export interface PullRoomFilters {
  repo: string;
  state: string;
  evidence: string;
  checkPosture: string;
  search: string;
}

export interface PullListItem {
  repo: string;
  repoHost: string;
  repoId: string | null;
  number: number;
  title: string;
  author: string | null;
  draft: boolean;
  state: string;
  headRef: string;
  headSha: string;
  baseRef: string;
  baseSha: string;
  mergeable: boolean;
  mergeableState: string;
  changedFileCount: number;
  evidenceState: EvidenceState;
  checkPosture: CheckPosture;
  checks: {
    total: number;
    queued: number;
    running: number;
    failing: number;
    successful: number;
    missing: boolean;
  };
  url: string;
  updatedAt: string | null;
}

export interface PullLane {
  id: PullLaneId;
  title: string;
  items: PullListItem[];
}

export interface ToolOpportunity {
  id: string;
  repo: string;
  score: number;
  occurrenceCount: number;
  fileCount: number;
  language: string | null;
  insight: string;
  suggestedProofLane: string;
}

export const DEFAULT_PULL_ROOM_FILTERS: PullRoomFilters = {
  repo: 'all',
  state: 'all',
  evidence: 'all',
  checkPosture: 'all',
  search: '',
};

export const PULL_LANE_TITLES: Record<PullLaneId, string> = {
  missing_checks: 'Missing checks',
  failing_checks: 'Failing checks',
  queued_running_checks: 'Queued / running',
  ready_reviewable: 'Ready / reviewable',
  merged_closed: 'Merged / closed',
};

const LANE_ORDER: PullLaneId[] = [
  'missing_checks',
  'failing_checks',
  'queued_running_checks',
  'ready_reviewable',
  'merged_closed',
];

export function fromControlPullRequest(pr: ControlPullRequest): PullListItem {
  return {
    repo: pr.repo,
    repoHost: 'jeryu',
    repoId: null,
    number: pr.number,
    title: pr.title,
    author: null,
    draft: pr.draft,
    state: pr.state,
    headRef: pr.headRef,
    headSha: pr.headSha,
    baseRef: pr.baseRef,
    baseSha: pr.baseSha,
    mergeable: pr.mergeable,
    mergeableState: pr.mergeableState,
    changedFileCount: pr.changedFiles.length,
    evidenceState: pr.stateEvidence,
    checkPosture: checkPosture(pr.checks),
    checks: pr.checks,
    url: `/repos/jeryu/${encodeURIComponent(pr.repo)}/pulls/${pr.number}`,
    updatedAt: null,
  };
}

export function fromPullRequestSummary(pr: PullRequestSummary): PullListItem {
  const repo = `${pr.repo.owner}/${pr.repo.name}`;
  const checks = {
    total: pr.checks.total,
    queued: 0,
    running: pr.checks.pending,
    failing: pr.checks.failing,
    successful: pr.checks.passing,
    missing: pr.checks.total === 0,
  };
  return {
    repo,
    repoHost: pr.repo.host,
    repoId: pr.repo.id,
    number: pr.number,
    title: pr.title,
    author: pr.author,
    draft: pr.draft,
    state: pr.state,
    headRef: pr.head_ref,
    headSha: pr.head_sha,
    baseRef: pr.base_ref,
    baseSha: pr.base_sha,
    mergeable: pr.mergeable.can_merge,
    mergeableState: pr.mergeable.level,
    changedFileCount: 0,
    evidenceState: pr.checks.total === 0 ? 'missing' : 'fresh',
    checkPosture: checkPosture(checks),
    checks,
    url: `/repos/${pr.repo.host}/${encodeURIComponent(repo)}/pulls/${pr.number}`,
    updatedAt: pr.updated_at,
  };
}

export function checkPosture(item: PullListItem['checks']): CheckPosture {
  if (item.missing || item.total === 0) return 'missing';
  if (item.failing > 0) return 'failing';
  if (item.running > 0) return 'running';
  if (item.queued > 0) return 'queued';
  return 'passing';
}

export function filterPullRequests(
  items: PullListItem[],
  filters: PullRoomFilters
): PullListItem[] {
  const needle = filters.search.trim().toLowerCase();
  return items.filter((item) => {
    if (filters.repo !== 'all' && item.repo !== filters.repo) return false;
    if (filters.state !== 'all' && item.state !== filters.state) return false;
    if (
      filters.evidence !== 'all' &&
      item.evidenceState !== filters.evidence
    ) {
      return false;
    }
    if (
      filters.checkPosture !== 'all' &&
      item.checkPosture !== filters.checkPosture
    ) {
      return false;
    }
    if (!needle) return true;
    return [
      item.repo,
      String(item.number),
      item.title,
      item.headRef,
      item.baseRef,
      item.headSha,
      item.author ?? '',
    ]
      .join(' ')
      .toLowerCase()
      .includes(needle);
  });
}

export function laneForPullRequest(item: PullListItem): PullLaneId {
  if (item.state === 'merged' || item.state === 'closed') {
    return 'merged_closed';
  }
  if (item.checkPosture === 'missing') return 'missing_checks';
  if (item.checkPosture === 'failing') return 'failing_checks';
  if (item.checkPosture === 'queued' || item.checkPosture === 'running') {
    return 'queued_running_checks';
  }
  return 'ready_reviewable';
}

export function groupPullRequests(items: PullListItem[]): PullLane[] {
  const grouped = new Map<PullLaneId, PullListItem[]>(
    LANE_ORDER.map((lane) => [lane, []])
  );
  for (const item of items) {
    grouped.get(laneForPullRequest(item))?.push(item);
  }
  return LANE_ORDER.map((id) => ({
    id,
    title: PULL_LANE_TITLES[id],
    items: grouped.get(id) ?? [],
  }));
}

export function repoOptions(items: PullListItem[]): string[] {
  return Array.from(new Set(items.map((item) => item.repo))).sort();
}

export function rankToolBuildOpportunities(
  snapshot: ControlPlaneSnapshot,
  clusters: ToolBuildCluster[] = []
): ToolOpportunity[] {
  const fromClusters = clusters.map((cluster) => ({
    id: cluster.cluster_id,
    repo: cluster.repo_id,
    score: cluster.score,
    occurrenceCount: cluster.occurrence_count,
    fileCount: cluster.file_count,
    language: cluster.language || null,
    insight: cluster.insight,
    suggestedProofLane: suggestedProofLane(cluster.language),
  }));
  const seedClusters = snapshot.toolBuild.topClusters.map(summaryOpportunity);
  return [...fromClusters, ...seedClusters]
    .filter((item, index, all) => all.findIndex((x) => x.id === item.id) === index)
    .sort(
      (a, b) =>
        b.score - a.score ||
        b.occurrenceCount - a.occurrenceCount ||
        b.fileCount - a.fileCount ||
        a.id.localeCompare(b.id)
    );
}

function summaryOpportunity(cluster: ToolBuildClusterSummary): ToolOpportunity {
  return {
    id: cluster.clusterId,
    repo: cluster.repoId,
    score: cluster.score,
    occurrenceCount: cluster.occurrenceCount,
    fileCount: cluster.fileCount,
    language: null,
    insight: cluster.insight,
    suggestedProofLane: 'bash ops/ci/codegraph-tool-build.sh',
  };
}

function suggestedProofLane(language: string | null): string {
  if (language === 'rust') {
    return 'cargo test -p jeryu-codegraph --jobs 40 tool_build';
  }
  return 'bash ops/ci/codegraph-tool-build.sh';
}
