import type {
  ControlPlaneSnapshot,
  EcosystemResponse,
  EvidenceState,
  GraphCluster,
  GraphEdge,
  GraphNode,
  ToolBuildCluster,
} from '../api/types';

export type GraphShape = 'circle' | 'rect' | 'diamond' | 'hex';

export interface GraphFilters {
  kinds: string[];
  states: EvidenceState[];
  query: string;
}

export interface OperatorGraphNode extends GraphNode {
  shape: GraphShape;
  colorClass: string;
  x: number;
  y: number;
}

export interface SelectedNodeDetails {
  node: OperatorGraphNode;
  inbound: GraphEdge[];
  outbound: GraphEdge[];
  clusters: GraphCluster[];
  evidenceCount: number;
}

export interface OperatorGraph {
  nodes: OperatorGraphNode[];
  edges: GraphEdge[];
  clusters: GraphCluster[];
  selected: SelectedNodeDetails | null;
  kindOptions: string[];
  stateOptions: EvidenceState[];
}

export const GRAPH_STATE_ORDER: EvidenceState[] = [
  'fresh',
  'missing',
  'queued',
  'failed',
  'unknown',
];

const KIND_ORDER = [
  'repo',
  'pull_request',
  'check_run',
  'runner_capacity',
  'remote_mirror',
  'codegraph_freshness',
  'tool_build',
  'ecosystem_tool',
];

export function nodeShape(kind: string): GraphShape {
  if (kind === 'repo') return 'circle';
  if (kind === 'runner_capacity' || kind === 'remote_mirror') return 'diamond';
  if (kind === 'tool_build' || kind === 'codegraph_freshness') return 'hex';
  return 'rect';
}

export function nodeColorClass(state: EvidenceState): string {
  return `is-${state}`;
}

export function buildOperatorGraph(
  snapshot: ControlPlaneSnapshot,
  ecosystem: EcosystemResponse | null,
  toolClusters: ToolBuildCluster[],
  filters: GraphFilters,
  selectedId: string | null
): OperatorGraph {
  const baseNodes = [
    ...snapshot.repoGraph.nodes,
    ...ecosystemNodes(ecosystem),
    ...toolClusterNodes(toolClusters),
  ];
  const nodesById = new Map(baseNodes.map((node) => [node.id, node]));
  const baseEdges = [
    ...snapshot.repoGraph.edges,
    ...ecosystemEdges(ecosystem),
    ...toolClusterEdges(toolClusters, nodesById),
  ];
  const clusters = [
    ...snapshot.repoGraph.clusters,
    ...toolClusterGraphClusters(toolClusters),
  ];
  const query = filters.query.trim().toLowerCase();
  const filteredRaw = baseNodes.filter((node) => {
    if (filters.kinds.length > 0 && !filters.kinds.includes(node.kind)) {
      return false;
    }
    if (filters.states.length > 0 && !filters.states.includes(node.state)) {
      return false;
    }
    if (!query) return true;
    return [node.id, node.label, node.kind, ...Object.values(node.metadata)]
      .join(' ')
      .toLowerCase()
      .includes(query);
  });
  const visibleIds = new Set(filteredRaw.map((node) => node.id));
  const edges = baseEdges.filter(
    (edge) => visibleIds.has(edge.source) && visibleIds.has(edge.target)
  );
  const nodes = layoutNodes(filteredRaw);
  const selectedNode =
    nodes.find((node) => node.id === selectedId) ?? nodes[0] ?? null;
  return {
    nodes,
    edges,
    clusters,
    selected: selectedNode
      ? {
          node: selectedNode,
          inbound: baseEdges.filter((edge) => edge.target === selectedNode.id),
          outbound: baseEdges.filter((edge) => edge.source === selectedNode.id),
          clusters: clusters.filter((cluster) =>
            cluster.nodeIds.includes(selectedNode.id)
          ),
          evidenceCount: evidenceCount(selectedNode, clusters),
        }
      : null,
    kindOptions: sortedKinds(baseNodes),
    stateOptions: GRAPH_STATE_ORDER,
  };
}

export function emptyGraph(snapshot: ControlPlaneSnapshot): boolean {
  return snapshot.repoGraph.nodes.length === 0;
}

function layoutNodes(nodes: GraphNode[]): OperatorGraphNode[] {
  const byKind = new Map<string, number>();
  return nodes.map((node, index) => {
    const lane = kindLane(node.kind);
    const laneIndex = byKind.get(node.kind) ?? 0;
    byKind.set(node.kind, laneIndex + 1);
    return {
      ...node,
      shape: nodeShape(node.kind),
      colorClass: nodeColorClass(node.state),
      x: 72 + lane * 118 + ((index * 13) % 32),
      y: 54 + ((laneIndex * 58) % 292),
    };
  });
}

function kindLane(kind: string): number {
  const index = KIND_ORDER.indexOf(kind);
  return index >= 0 ? index : KIND_ORDER.length - 1;
}

function sortedKinds(nodes: GraphNode[]): string[] {
  return Array.from(new Set(nodes.map((node) => node.kind))).sort(
    (a, b) => kindLane(a) - kindLane(b) || a.localeCompare(b)
  );
}

function ecosystemNodes(ecosystem: EcosystemResponse | null): GraphNode[] {
  if (!ecosystem) return [];
  return ecosystem.tools.slice(0, 20).map((tool) => ({
    id: `tool:${tool.name}`,
    label: tool.name,
    kind: 'ecosystem_tool',
    state: ecosystem.live && !ecosystem.degradedReason ? 'fresh' : 'unknown',
    weight: tool.conformance === 'mutating' ? 2 : 1,
    metadata: {
      className: tool.className,
      conformance: tool.conformance,
      repo: tool.repo ?? '',
      queue: tool.queue ?? '',
    },
  }));
}

function ecosystemEdges(ecosystem: EcosystemResponse | null): GraphEdge[] {
  if (!ecosystem) return [];
  const available = new Set(ecosystem.tools.map((tool) => tool.name));
  return ecosystem.tools.flatMap((tool) =>
    tool.dependsOn
      .filter((dep) => available.has(dep))
      .map((dep) => ({
        source: `tool:${dep}`,
        target: `tool:${tool.name}`,
        kind: 'tool_dependency',
        state: ecosystem.live ? 'fresh' : 'unknown',
        weight: 1,
      }))
  );
}

function toolClusterNodes(clusters: ToolBuildCluster[]): GraphNode[] {
  return clusters
    .filter((cluster) => !cluster.ignored)
    .slice(0, 12)
    .map((cluster) => ({
      id: `tool-build:${cluster.cluster_id}`,
      label: cluster.cluster_id,
      kind: 'tool_build',
      state: 'fresh',
      weight: Math.max(1, Math.min(8, cluster.score / 20)),
      metadata: {
        repo: cluster.repo_id,
        score: String(cluster.score),
        language: cluster.language,
        occurrences: String(cluster.occurrence_count),
        files: String(cluster.file_count),
      },
    }));
}

function toolClusterEdges(
  clusters: ToolBuildCluster[],
  nodesById: Map<string, GraphNode>
): GraphEdge[] {
  return clusters.flatMap((cluster) => {
    const repoId = `repo:${cluster.repo_id}`;
    if (!nodesById.has(repoId)) return [];
    return [
      {
        source: repoId,
        target: `tool-build:${cluster.cluster_id}`,
        kind: 'tool_build_opportunity',
        state: 'fresh' as EvidenceState,
        weight: Math.max(1, Math.min(5, cluster.occurrence_count)),
      },
    ];
  });
}

function toolClusterGraphClusters(clusters: ToolBuildCluster[]): GraphCluster[] {
  return clusters
    .filter((cluster) => !cluster.ignored)
    .slice(0, 12)
    .map((cluster) => ({
      id: `cluster:${cluster.cluster_id}`,
      label: `Tool-build ${cluster.cluster_id}`,
      kind: 'tool_build',
      state: 'fresh',
      severity: cluster.score >= 80 ? 'high' : 'medium',
      nodeIds: [`tool-build:${cluster.cluster_id}`],
      insights: [cluster.insight],
    }));
}

function evidenceCount(node: GraphNode, clusters: GraphCluster[]): number {
  const metadataCount = Object.values(node.metadata).filter(Boolean).length;
  const clusterCount = clusters.filter((cluster) =>
    cluster.nodeIds.includes(node.id)
  ).length;
  return metadataCount + clusterCount;
}
