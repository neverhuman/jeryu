// types.ts — re-exports the generated DTO types (W-FE-03).
//
// All wire types live in `contracts/generated/*.ts` (produced by ts-rs from
// the Rust API surface). We re-export the subset used by the SPA so app code
// imports from `@/api/types` (logical boundary) rather than reaching into the
// generated tree directly.
//
// When a new DTO is needed, add a new export here — do not edit the generated
// files.

export type { WebBootstrap } from '../../../../contracts/generated/WebBootstrap';
export type { Viewer } from '../../../../contracts/generated/Viewer';
export type { WebFeatureFlags } from '../../../../contracts/generated/WebFeatureFlags';
export type { RepositorySummary } from '../../../../contracts/generated/RepositorySummary';
export type { RepositoryId } from '../../../../contracts/generated/RepositoryId';
export type { RepositoryVisibility } from '../../../../contracts/generated/RepositoryVisibility';
export type { RepositoryListResponse } from '../../../../contracts/generated/RepositoryListResponse';
export type { RefSelectorItem } from '../../../../contracts/generated/RefSelectorItem';
export type { RefKind } from '../../../../contracts/generated/RefKind';
export type { TreeEntry } from '../../../../contracts/generated/TreeEntry';
export type { TreeEntryKind } from '../../../../contracts/generated/TreeEntryKind';
export type { BlobResponse } from '../../../../contracts/generated/BlobResponse';
export type { BlobEncoding } from '../../../../contracts/generated/BlobEncoding';
export type { RenderedMarkdown } from '../../../../contracts/generated/RenderedMarkdown';
export type { MarkdownHeading } from '../../../../contracts/generated/MarkdownHeading';
export type { MarkdownLink } from '../../../../contracts/generated/MarkdownLink';
export type { PullRequestSummary } from '../../../../contracts/generated/PullRequestSummary';
export type { PullRequestDetail } from '../../../../contracts/generated/PullRequestDetail';
export type { PullRequestState } from '../../../../contracts/generated/PullRequestState';
export type { MergePassport } from '../../../../contracts/generated/MergePassport';
export type { MergePassportBlocker } from '../../../../contracts/generated/MergePassportBlocker';
export type { MergePassportStatus } from '../../../../contracts/generated/MergePassportStatus';
export type { Mergeability } from '../../../../contracts/generated/Mergeability';
export type { ReviewThread } from '../../../../contracts/generated/ReviewThread';
export type { ReviewComment } from '../../../../contracts/generated/ReviewComment';
export type { ReviewVerdict } from '../../../../contracts/generated/ReviewVerdict';
export type { ReviewPosture } from '../../../../contracts/generated/ReviewPosture';
export type { ReviewSuggestion } from '../../../../contracts/generated/ReviewSuggestion';
export type { SubmitReviewRequest } from '../../../../contracts/generated/SubmitReviewRequest';
export type { CreateReviewCommentRequest } from '../../../../contracts/generated/CreateReviewCommentRequest';
export type { CreateRepositoryRequest } from '../../../../contracts/generated/CreateRepositoryRequest';
export type { CreateRepositoryPreview } from '../../../../contracts/generated/CreateRepositoryPreview';
export type { IssueSummary } from '../../../../contracts/generated/IssueSummary';
export type { IssueState } from '../../../../contracts/generated/IssueState';
export type { AgentPosture } from '../../../../contracts/generated/AgentPosture';
export type { AgentSettings } from '../../../../contracts/generated/AgentSettings';
export type { AccessSettings } from '../../../../contracts/generated/AccessSettings';
export type { CheckPosture } from '../../../../contracts/generated/CheckPosture';
export type { CiSettings } from '../../../../contracts/generated/CiSettings';
export type { GeneralSettings } from '../../../../contracts/generated/GeneralSettings';
export type { FeatureSettings } from '../../../../contracts/generated/FeatureSettings';
export type { MergeSettings } from '../../../../contracts/generated/MergeSettings';
export type { NotificationSettings } from '../../../../contracts/generated/NotificationSettings';
export type { RepositorySettings } from '../../../../contracts/generated/RepositorySettings';
export type { RepositoryHostKind } from '../../../../contracts/generated/RepositoryHostKind';
export type { RepositoryFacets } from '../../../../contracts/generated/RepositoryFacets';
export type { RetentionSettings } from '../../../../contracts/generated/RetentionSettings';
export type { SecuritySettings } from '../../../../contracts/generated/SecuritySettings';
export type { BranchProtectionRule } from '../../../../contracts/generated/BranchProtectionRule';
export type { SettingsPatch } from '../../../../contracts/generated/SettingsPatch';
export type { SettingsDiffPreview } from '../../../../contracts/generated/SettingsDiffPreview';
export type { SettingsFieldChange } from '../../../../contracts/generated/SettingsFieldChange';
export type { ClientWsMessage } from '../../../../contracts/generated/ClientWsMessage';
export type { ServerWsMessage } from '../../../../contracts/generated/ServerWsMessage';
export type { WebEvent } from '../../../../contracts/generated/WebEvent';
export type { SubscriptionSpec } from '../../../../contracts/generated/SubscriptionSpec';

import type { PullRequestSummary } from '../../../../contracts/generated/PullRequestSummary';
import type { ReviewThread } from '../../../../contracts/generated/ReviewThread';

// ── Phase 3 frontend-local wire types (W-FE-11). ────────────────────────
// The backend (W-B-* phase 3) emits diff/checks/threads payloads that are
// not yet exported via ts-rs. These mirror the documented contract (see
// the web work spec §7.4 W-FE-11 / §35.2.4). When the backend lands its
// ts-rs export, these declarations move to `contracts/generated/` and the
// re-export here becomes a one-liner like the others.

/** Per-file diff status emitted by `GET /pulls/{number}/diff`. */
export type PullRequestFileStatus =
  | 'added'
  | 'modified'
  | 'removed'
  | 'renamed';

/** Risk tier the backend tags onto each changed file. */
export type PullRequestFileRisk = 'low' | 'medium' | 'high' | 'critical';

/** Single hunk in a unified diff. */
export interface PullRequestDiffHunk {
  header: string;
  old_start: number;
  old_lines: number;
  new_start: number;
  new_lines: number;
  /** Raw unified-diff body lines (prefixed with `+` / `-` / ` `). */
  lines: string[];
}

/** Per-file diff entry. */
export interface PullRequestDiffFile {
  path: string;
  /** When `status === 'renamed'`, the previous path. */
  old_path: string | null;
  status: PullRequestFileStatus;
  additions: number;
  deletions: number;
  risk: PullRequestFileRisk | null;
  /** Binary diffs carry no hunks; viewer renders a notice. */
  is_binary: boolean;
  hunks: PullRequestDiffHunk[];
}

/** Wire shape of `GET /pulls/{number}/diff`. */
export interface PullRequestDiff {
  head_sha: string;
  base_sha: string;
  files: PullRequestDiffFile[];
  /** True when the server truncated due to size; UI renders a warning. */
  truncated: boolean;
}

/** One CI check run on a PR. */
export interface PullRequestCheck {
  id: string;
  name: string;
  /** `success`, `failure`, `pending`, `skipped`, `cancelled`, `neutral`. */
  status: string;
  conclusion: string | null;
  details_url: string | null;
  description: string | null;
  /** RFC3339 timestamps. */
  started_at: string | null;
  completed_at: string | null;
}

/** Wire shape of `GET /pulls/{number}/checks`. */
export interface PullRequestChecks {
  total: number;
  passing: number;
  failing: number;
  pending: number;
  skipped: number;
  checks: PullRequestCheck[];
}

/** Wire shape of `GET /pulls/{number}/threads`. */
export interface PullRequestThreadList {
  /** Re-uses the canonical `ReviewThread` type. */
  threads: ReviewThread[];
}

/** Wire shape of `GET /api/v1/repos/{id}/pulls`. */
export interface PullRequestListResponse {
  items: PullRequestSummary[];
  total: number;
}

/** Body for `POST /pulls/{number}/approve`. */
export interface PullApproveRequest {
  expected_head_sha: string;
  body_markdown?: string | null;
}

/** Body for `POST /pulls/{number}/merge`. */
export interface MergePullRequest {
  expected_head_sha: string;
  expected_passport_hash: string | null;
  merge_method: 'merge' | 'squash' | 'rebase';
  commit_title?: string | null;
  commit_message?: string | null;
}

// JMCP/control-plane local wire types. These are intentionally frontend-local
// until the Rust read-model exporter owns this contract.
export type EvidenceState =
  | 'fresh'
  | 'missing'
  | 'queued'
  | 'failed'
  | 'unknown';

export type InsightSeverity = 'critical' | 'high' | 'medium' | 'low' | 'info';

export interface SourceLink {
  label: string;
  url: string;
}

export interface ControlPlaneSummary {
  repoCount: number;
  openPrCount: number;
  draftPrCount: number;
  queuedCheckCount: number;
  runningCheckCount: number;
  failingCheckCount: number;
  missingCheckPrCount: number;
  priorityCount: number;
  criticalPriorityCount: number;
  highPriorityCount: number;
  mirrorState: EvidenceState;
  artifactState: EvidenceState;
  runnerState: EvidenceState;
}

export interface LocalAuthority {
  sourceOfTruth: string;
  state: EvidenceState;
  docsUrl: string;
}

export interface ControlRepo {
  id: string;
  fullName: string;
  owner: string;
  name: string;
  defaultBranch: string;
  openPullRequests: number;
  draftPullRequests: number;
  queuedChecks: number;
  runningChecks: number;
  failingChecks: number;
  latestHeadSha: string | null;
  state: EvidenceState;
}

export interface ControlPullRequest {
  repo: string;
  number: number;
  title: string;
  draft: boolean;
  state: string;
  headRef: string;
  headSha: string;
  baseRef: string;
  baseSha: string;
  mergeable: boolean;
  mergeableState: string;
  changedFiles: string[];
  stateEvidence: EvidenceState;
  sourceLinks: SourceLink[];
  checks: {
    total: number;
    queued: number;
    running: number;
    failing: number;
    successful: number;
    missing: boolean;
  };
}

export interface ArtifactEvidence {
  state: EvidenceState;
  artifactCount: number;
  reason: string;
  sourceLinks: SourceLink[];
}

export interface ArtifactLatestResponse {
  schemaVersion: string;
  state: EvidenceState;
  latestBuild: ArtifactEvidence;
  latestRelease: ArtifactEvidence;
  mirrorArtifacts: ArtifactEvidence;
  docsUrl: string;
  absenceIsSuccess: boolean;
}

export interface MirrorEvidence {
  name: string;
  state: EvidenceState;
  reason: string;
  docsUrl: string;
}

export interface RemoteStatusResponse {
  schemaVersion: string;
  state: EvidenceState;
  mirrors: MirrorEvidence[];
  divergence: {
    state: EvidenceState;
    reason: string;
    localDefaultBranches: SourceLink[];
    mirrorDefaultBranches: SourceLink[];
  };
}

export interface RunnerFabricResponse {
  schemaVersion: string;
  local: {
    state: EvidenceState;
    nodes: number;
    onlineRunners: number;
    offlineRunners: number;
    busyRunners: number;
    idleRunners: number;
    totalSlots: number;
    activeSlots: number;
    utilization: number;
    lastUpdated: string | null;
    nodeDetails: RunnerNodeSummary[];
  };
  mirror: MirrorEvidence;
}

export interface RunnerNodeSummary {
  runnerId: string;
  source: string;
  state: string;
  capacity: number;
  inFlight: number;
  labels: string[];
  classes: string[];
  activeTaskCount: number;
  lastUpdated: string | null;
  activeTasks: RunnerTaskSummary[];
}

export interface RunnerTaskSummary {
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
  ttyPreview: RunnerTtyPreview;
}

export interface RunnerTtyPreview {
  state: EvidenceState;
  lines: string[];
}

export interface CodegraphControlSummary {
  state: EvidenceState;
  indexedSymbols: number;
  indexedReferences: number;
  crateEdges: number;
  indexedFiles: number;
  latestIndexRun: string | null;
  reason: string;
}

export interface ToolBuildControlSummary {
  state: EvidenceState;
  clusterCount: number;
  ignoredCount: number;
  topClusters: ToolBuildClusterSummary[];
}

export interface ToolBuildClusterSummary {
  clusterId: string;
  repoId: string;
  score: number;
  occurrenceCount: number;
  fileCount: number;
  insight: string;
}

export interface PriorityInsight {
  id: string;
  title: string;
  severity: InsightSeverity;
  score: number;
  confidence: number;
  owner: string;
  proofLane: string;
  recommendedAction: string;
  evidence: string[];
  sourceLinks: SourceLink[];
  state: EvidenceState;
  rulesVersion: string;
}

export interface GraphNode {
  id: string;
  label: string;
  kind: string;
  state: EvidenceState;
  weight: number;
  metadata: Record<string, string>;
}

export interface GraphEdge {
  source: string;
  target: string;
  kind: string;
  state: EvidenceState;
  weight: number;
}

export interface GraphCluster {
  id: string;
  label: string;
  kind: string;
  state: EvidenceState;
  severity: InsightSeverity;
  nodeIds: string[];
  insights: string[];
}

export interface RepoGraphResponse {
  schemaVersion: string;
  generatedAt: string;
  nodes: GraphNode[];
  edges: GraphEdge[];
  clusters: GraphCluster[];
  insights: Array<{
    id: string;
    clusterId: string;
    title: string;
    evidence: string[];
  }>;
}

export interface McpToolHealth {
  state: EvidenceState;
  toolCount: number;
  liveBackedTools: string[];
  degradedTools: string[];
}

export interface ControlPlaneSnapshot {
  schemaVersion: string;
  generatedAt: string;
  localAuthority: LocalAuthority;
  summary: ControlPlaneSummary;
  repos: ControlRepo[];
  pullRequests: ControlPullRequest[];
  checkRuns: Array<{
    id: string;
    repo: string;
    name: string;
    headSha: string;
    status: string;
    conclusion: string | null;
    state: EvidenceState;
  }>;
  artifacts: ArtifactLatestResponse;
  runners: RunnerFabricResponse;
  workflows: unknown[];
  releases: unknown;
  workcells: unknown;
  agentRuns: unknown[];
  codegraph: CodegraphControlSummary;
  toolBuild: ToolBuildControlSummary;
  mcp: McpToolHealth;
  mirror: RemoteStatusResponse;
  priorities: PriorityInsight[];
  repoGraph: RepoGraphResponse;
}

export interface EcosystemToolAsset {
  name: string;
  className: string;
  conformance: string;
  sideEffects: string[];
  dataClasses: string[];
  repo?: string;
  provider?: string;
  health?: string;
  dependsOn: string[];
  queue?: string;
}

export interface EcosystemResponse {
  tools: EcosystemToolAsset[];
  live: boolean;
  degradedReason: string;
}

export interface ToolBuildOccurrence {
  repo_id: string;
  commit_sha: string;
  path: string;
  start_line: number;
  end_line: number;
  language: string;
  normalized_token_count: number;
}

export interface ToolBuildIgnore {
  cluster_id: string;
  reason: string;
  ignored_by: string;
  ignored_at: string;
}

export interface ToolBuildCluster {
  cluster_id: string;
  repo_id: string;
  commit_sha: string;
  fingerprint: string;
  score: number;
  occurrence_count: number;
  repo_count: number;
  file_count: number;
  total_lines: number;
  language: string;
  insight: string;
  normalized_preview: string;
  occurrences: ToolBuildOccurrence[];
  ignored?: ToolBuildIgnore;
}

export interface ToolBuildClustersResponse {
  schema_version: string;
  repo: string | null;
  include_ignored: boolean;
  clusters: ToolBuildCluster[];
}

// Live agent terminal types (defined in ./agentTerminal).
export type { AgentTtyFrame, AgentControl, AgentControlClientMessage, RepoAgentSummary, RepoAgentRunsResponse, CreateSessionResponse } from './agentTerminal';
