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
