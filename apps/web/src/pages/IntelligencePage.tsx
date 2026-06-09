// IntelligencePage.tsx - JMCP/control-plane operator surface.

import { useMemo, useState, type ReactNode } from 'react';
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  Circle,
  GitPullRequest,
  Network,
  Package,
  ServerCog,
  Terminal,
} from 'lucide-react';

import { useControlPlane } from '../hooks/useControlPlane';
import { useEcosystem, useToolBuildClusters } from '../hooks/useToolingEvidence';
import type {
  ControlPlaneSnapshot,
  EvidenceState,
  GraphEdge,
  InsightSeverity,
  PriorityInsight,
  ToolBuildCluster,
} from '../api/types';
import {
  GRAPH_STATE_ORDER,
  buildOperatorGraph,
  type GraphFilters,
  type OperatorGraph,
  type OperatorGraphNode,
} from './intelligenceGraphModel';

import './page.css';
import './IntelligencePage.css';

export function IntelligencePage(): JSX.Element {
  const query = useControlPlane();

  if (query.isLoading) {
    return (
      <div className="page intelligence" data-testid="intelligence-page">
        <header className="page__header">
          <h1 className="page__title">Intelligence</h1>
        </header>
        <p className="page__roadmap-note">Loading control-plane snapshot.</p>
      </div>
    );
  }

  if (query.isError || !query.data) {
    return (
      <div className="page intelligence" data-testid="intelligence-page">
        <header className="page__header">
          <h1 className="page__title">Intelligence</h1>
        </header>
        <p className="page__roadmap-note">
          {query.error?.message ?? 'Control-plane snapshot unavailable.'}
        </p>
      </div>
    );
  }

  return <IntelligenceSnapshot snapshot={query.data} />;
}

function IntelligenceSnapshot({
  snapshot,
}: {
  snapshot: ControlPlaneSnapshot;
}): JSX.Element {
  const ecosystem = useEcosystem();
  const toolClusters = useToolBuildClusters(10);
  const [graphFilters, setGraphFilters] = useState<GraphFilters>({
    kinds: [],
    states: [],
    query: '',
  });
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const topPriorities = snapshot.priorities.slice(0, 8);
  const operatorGraph = useMemo(
    () =>
      buildOperatorGraph(
        snapshot,
        ecosystem.data ?? null,
        toolClusters.data?.clusters ?? [],
        graphFilters,
        selectedNodeId
      ),
    [ecosystem.data, graphFilters, selectedNodeId, snapshot, toolClusters.data]
  );

  return (
    <div className="page intelligence" data-testid="intelligence-page">
      <header className="page__header intelligence__header">
        <div className="intelligence__title-line">
          <h1 className="page__title">Intelligence</h1>
          <StatePill state={snapshot.localAuthority.state} label="local" />
          <span className="page__pill">{snapshot.schemaVersion}</span>
        </div>
        <div className="intelligence__status-strip">
          <MetricCard
            icon={<GitPullRequest size={18} aria-hidden="true" />}
            label="Open PRs"
            value={snapshot.summary.openPrCount}
            detail={`${snapshot.summary.missingCheckPrCount} missing checks`}
            state={
              snapshot.summary.missingCheckPrCount > 0 ? 'missing' : 'fresh'
            }
          />
          <MetricCard
            icon={<AlertTriangle size={18} aria-hidden="true" />}
            label="Priorities"
            value={snapshot.summary.priorityCount}
            detail={`${snapshot.summary.highPriorityCount} high`}
            state={
              snapshot.summary.highPriorityCount > 0 ? 'failed' : 'fresh'
            }
          />
          <MetricCard
            icon={<ServerCog size={18} aria-hidden="true" />}
            label="Runners"
            value={snapshot.runners.local.onlineRunners}
            detail={`${snapshot.runners.local.offlineRunners} offline`}
            state={
              snapshot.runners.local.offlineRunners > 0 ? 'failed' : 'fresh'
            }
          />
          <MetricCard
            icon={<Package size={18} aria-hidden="true" />}
            label="Artifacts"
            value={snapshot.artifacts.latestRelease.artifactCount}
            detail={`absence=${snapshot.artifacts.absenceIsSuccess ? 'success' : 'evidence'}`}
            state={snapshot.artifacts.state}
          />
          <MetricCard
            icon={<Network size={18} aria-hidden="true" />}
            label="Graph"
            value={operatorGraph.nodes.length}
            detail={`${snapshot.codegraph.indexedSymbols} symbols · ${snapshot.toolBuild.clusterCount} tool clusters`}
            state={snapshot.codegraph.state}
          />
        </div>
      </header>

      <section className="page__section" aria-labelledby="intelligence-priority">
        <div className="intelligence__section-head">
          <h2 className="page__section-title" id="intelligence-priority">
            Priority Queue
          </h2>
          <span className="page__pill">{snapshot.priorities[0]?.rulesVersion ?? 'rules-v1'}</span>
        </div>
        {topPriorities.length === 0 ? (
          <p className="page__roadmap-note">No ranked priorities.</p>
        ) : (
          <div className="intelligence__priority-table">
            {topPriorities.map((priority) => (
              <PriorityRow key={priority.id} priority={priority} />
            ))}
          </div>
        )}
      </section>

      <section className="page__section" aria-labelledby="intelligence-graph">
        <div className="intelligence__section-head">
          <h2 className="page__section-title" id="intelligence-graph">
            Operator Graph
          </h2>
          <span className="page__pill">{operatorGraph.nodes.length} nodes</span>
          <span className="page__pill">{operatorGraph.edges.length} edges</span>
          <span className="page__pill">{operatorGraph.clusters.length} clusters</span>
        </div>
        <OperatorGraphConsole
          graph={operatorGraph}
          filters={graphFilters}
          onFiltersChange={setGraphFilters}
          onSelectNode={setSelectedNodeId}
        />
        <ToolBuildDossiers
          clusters={toolClusters.data?.clusters ?? []}
          fallback={snapshot.toolBuild.topClusters}
          unavailable={toolClusters.isError ? toolClusters.error.message : null}
        />
      </section>

      <section className="page__section" aria-labelledby="intelligence-health">
        <h2 className="page__section-title" id="intelligence-health">
          Health Evidence
        </h2>
        <div className="intelligence__evidence-grid">
          <EvidencePanel
            icon={<Network size={18} aria-hidden="true" />}
            title="Mirror"
            state={snapshot.mirror.state}
            body={snapshot.mirror.divergence.reason}
          />
          <EvidencePanel
            icon={<Package size={18} aria-hidden="true" />}
            title="Artifacts"
            state={snapshot.artifacts.state}
            body={snapshot.artifacts.latestRelease.reason}
          />
          <EvidencePanel
            icon={<Activity size={18} aria-hidden="true" />}
            title="MCP"
            state={snapshot.mcp.state}
            body={`${snapshot.mcp.toolCount} tools, ${snapshot.mcp.liveBackedTools.length} live-backed`}
          />
          <EvidencePanel
            icon={<Terminal size={18} aria-hidden="true" />}
            title="Agent Runs"
            state={snapshot.agentRuns.length > 0 ? 'fresh' : 'missing'}
            body={`${snapshot.agentRuns.length} recorded run(s)`}
          />
          <EvidencePanel
            icon={<Network size={18} aria-hidden="true" />}
            title="Codegraph"
            state={snapshot.codegraph.state}
            body={snapshot.codegraph.reason}
          />
        </div>
      </section>
    </div>
  );
}

function MetricCard({
  icon,
  label,
  value,
  detail,
  state,
}: {
  icon: ReactNode;
  label: string;
  value: number;
  detail: string;
  state: EvidenceState;
}): JSX.Element {
  return (
    <article className={`intelligence__metric is-${state}`}>
      <div className="intelligence__metric-icon">{icon}</div>
      <div>
        <div className="intelligence__metric-label">{label}</div>
        <div className="intelligence__metric-value">{value}</div>
        <div className="intelligence__metric-detail">{detail}</div>
      </div>
    </article>
  );
}

function PriorityRow({
  priority,
}: {
  priority: PriorityInsight;
}): JSX.Element {
  return (
    <article
      className={`intelligence__priority is-${priority.severity}`}
      data-testid={`priority-${priority.id}`}
    >
      <div className="intelligence__priority-score">{priority.score}</div>
      <div className="intelligence__priority-main">
        <div className="intelligence__priority-title">
          <SeverityIcon severity={priority.severity} />
          <span>{priority.title}</span>
        </div>
        <div className="intelligence__priority-meta">
          <span>{priority.owner}</span>
          <span>{priority.proofLane}</span>
          <span>{priority.recommendedAction}</span>
        </div>
      </div>
      <StatePill state={priority.state} />
    </article>
  );
}

function OperatorGraphConsole({
  graph,
  filters,
  onFiltersChange,
  onSelectNode,
}: {
  graph: OperatorGraph;
  filters: GraphFilters;
  onFiltersChange: (filters: GraphFilters) => void;
  onSelectNode: (id: string) => void;
}): JSX.Element {
  return (
    <div className="intelligence__operator" data-testid="operator-graph-console">
      <div className="intelligence__graph-controls">
        <GraphToggles
          title="Kinds"
          options={graph.kindOptions}
          selected={filters.kinds}
          onToggle={(value) =>
            onFiltersChange({ ...filters, kinds: toggle(filters.kinds, value) })
          }
        />
        <GraphToggles
          title="States"
          options={GRAPH_STATE_ORDER}
          selected={filters.states}
          onToggle={(value) =>
            onFiltersChange({
              ...filters,
              states: toggle(filters.states, value as EvidenceState),
            })
          }
        />
        <label className="intelligence__graph-search">
          Search
          <input
            type="search"
            value={filters.query}
            onChange={(event) =>
              onFiltersChange({ ...filters, query: event.target.value })
            }
            placeholder="node, repo, state"
          />
        </label>
      </div>
      <div className="intelligence__graph-console">
        <GraphSvg graph={graph} onSelectNode={onSelectNode} />
        <NodeInspector graph={graph} />
      </div>
      <div className="intelligence__graph-bottom">
        <EdgeList edges={graph.edges} />
        <ClusterChips graph={graph} />
      </div>
    </div>
  );
}

function GraphToggles({
  title,
  options,
  selected,
  onToggle,
}: {
  title: string;
  options: string[];
  selected: string[];
  onToggle: (value: string) => void;
}): JSX.Element {
  return (
    <fieldset className="intelligence__toggle-set">
      <legend>{title}</legend>
      <div>
        {options.map((option) => (
          <label key={option}>
            <input
              type="checkbox"
              checked={selected.includes(option)}
              onChange={() => onToggle(option)}
            />
            <span>{option}</span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}

function GraphSvg({
  graph,
  onSelectNode,
}: {
  graph: OperatorGraph;
  onSelectNode: (id: string) => void;
}): JSX.Element {
  const nodesById = new Map(graph.nodes.map((node) => [node.id, node]));
  if (graph.nodes.length === 0) {
    return (
      <div className="intelligence__graph-empty" data-testid="repo-graph-preview">
        No graph nodes available.
      </div>
    );
  }
  return (
    <div className="intelligence__graph-preview" data-testid="repo-graph-preview">
      <svg viewBox="0 0 980 420" role="img" aria-label="Operator graph">
        <rect
          x="16"
          y="16"
          width="948"
          height="388"
          rx="8"
          className="intelligence__graph-ring"
        />
        {graph.edges.map((edge) => {
          const source = nodesById.get(edge.source);
          const target = nodesById.get(edge.target);
          if (!source || !target) return null;
          return (
            <line
              key={`${edge.source}-${edge.target}-${edge.kind}`}
              x1={source.x}
              y1={source.y}
              x2={target.x}
              y2={target.y}
              className={`intelligence__graph-edge is-${edge.state}`}
              strokeWidth={Math.min(5, Math.max(1, edge.weight))}
            />
          );
        })}
        {graph.nodes.map((node) => (
          <GraphNodeMark
            key={node.id}
            node={node}
            selected={graph.selected?.node.id === node.id}
            onSelect={onSelectNode}
          />
        ))}
      </svg>
      <div className="intelligence__legend" aria-label="Graph state legend">
        {GRAPH_STATE_ORDER.map((state) => (
          <span key={state} className={`intelligence__legend-item is-${state}`}>
            {state}
          </span>
        ))}
      </div>
    </div>
  );
}

function GraphNodeMark({
  node,
  selected,
  onSelect,
}: {
  node: OperatorGraphNode;
  selected: boolean;
  onSelect: (id: string) => void;
}): JSX.Element {
  const common = `intelligence__graph-node ${node.colorClass} ${selected ? 'is-selected' : ''}`;
  const label = compactLabel(node.label);
  return (
    <g
      role="button"
      tabIndex={0}
      onClick={() => onSelect(node.id)}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') onSelect(node.id);
      }}
      data-testid={`graph-node-${node.id}`}
    >
      <title>{`${node.kind}: ${node.label}`}</title>
      {node.shape === 'circle' ? (
        <circle cx={node.x} cy={node.y} r={nodeRadius(node)} className={common} />
      ) : node.shape === 'diamond' ? (
        <polygon points={diamondPoints(node.x, node.y, nodeRadius(node))} className={common} />
      ) : node.shape === 'hex' ? (
        <polygon points={hexPoints(node.x, node.y, nodeRadius(node) + 3)} className={common} />
      ) : (
        <rect
          x={node.x - nodeRadius(node)}
          y={node.y - nodeRadius(node)}
          width={nodeRadius(node) * 2}
          height={nodeRadius(node) * 2}
          rx="3"
          className={common}
        />
      )}
      <text x={node.x + 12} y={node.y + 4} className="intelligence__graph-label">
        {label}
      </text>
    </g>
  );
}

function NodeInspector({ graph }: { graph: OperatorGraph }): JSX.Element {
  if (!graph.selected) {
    return (
      <aside className="intelligence__inspector" data-testid="node-inspector">
        <h3>Selected node</h3>
        <p>No graph nodes available.</p>
      </aside>
    );
  }
  const { node, inbound, outbound, clusters, evidenceCount } = graph.selected;
  return (
    <aside className="intelligence__inspector" data-testid="node-inspector">
      <h3>Selected node</h3>
      <div className="intelligence__inspector-title">
        <strong>{node.label}</strong>
        <StatePill state={node.state} />
      </div>
      <dl>
        <dt>Kind</dt>
        <dd>{node.kind}</dd>
        <dt>Evidence</dt>
        <dd>{evidenceCount}</dd>
        <dt>Edges</dt>
        <dd>{inbound.length} in · {outbound.length} out</dd>
        <dt>Clusters</dt>
        <dd>{clusters.length}</dd>
      </dl>
      <div className="intelligence__metadata">
        {Object.entries(node.metadata).map(([key, value]) =>
          value ? (
            <span key={key}>
              {key}: {value}
            </span>
          ) : null
        )}
      </div>
    </aside>
  );
}

function EdgeList({ edges }: { edges: GraphEdge[] }): JSX.Element {
  return (
    <section className="intelligence__edge-list">
      <h3>Edges</h3>
      {edges.length === 0 ? (
        <p>No visible edges.</p>
      ) : (
        <ol>
          {edges.slice(0, 12).map((edge) => (
            <li key={`${edge.source}-${edge.target}-${edge.kind}`}>
              <span>{edge.kind}</span>
              <code>{compactLabel(edge.source)}</code>
              <span>→</span>
              <code>{compactLabel(edge.target)}</code>
              <StatePill state={edge.state} />
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

function ClusterChips({ graph }: { graph: OperatorGraph }): JSX.Element {
  return (
    <section className="intelligence__cluster-chips">
      <h3>Clusters</h3>
      {graph.clusters.length === 0 ? (
        <p>No graph clusters.</p>
      ) : (
        <div>
          {graph.clusters.slice(0, 16).map((cluster) => (
            <span key={cluster.id} className="intelligence__cluster-chip">
              {cluster.label}
              <SeverityPill severity={cluster.severity} />
            </span>
          ))}
        </div>
      )}
    </section>
  );
}

function ToolBuildDossiers({
  clusters,
  fallback,
  unavailable,
}: {
  clusters: ToolBuildCluster[];
  fallback: ControlPlaneSnapshot['toolBuild']['topClusters'];
  unavailable: string | null;
}): JSX.Element {
  const rows =
    clusters.length > 0
      ? clusters.map((cluster) => ({
          id: cluster.cluster_id,
          repo: cluster.repo_id,
          score: cluster.score,
          occurrences: cluster.occurrence_count,
          files: cluster.file_count,
          language: cluster.language,
          insight: cluster.insight,
          proofLane:
            cluster.language === 'rust'
              ? 'cargo test -p jeryu-codegraph --jobs 40 tool_build'
              : 'bash ops/ci/codegraph-tool-build.sh',
        }))
      : fallback.map((cluster) => ({
          id: cluster.clusterId,
          repo: cluster.repoId,
          score: cluster.score,
          occurrences: cluster.occurrenceCount,
          files: cluster.fileCount,
          language: 'unknown',
          insight: cluster.insight,
          proofLane: 'bash ops/ci/codegraph-tool-build.sh',
        }));
  return (
    <div className="intelligence__dossiers" data-testid="tool-build-dossiers">
      <div className="intelligence__section-head">
        <h3>Tool-build dossiers</h3>
        {unavailable ? <span className="page__pill">unavailable</span> : null}
      </div>
      {unavailable ? <p>{unavailable}</p> : null}
      {rows.length === 0 ? (
        <p>No tool-build cluster dossiers available.</p>
      ) : (
        <div className="intelligence__dossier-grid">
          {rows.slice(0, 6).map((row) => (
            <article className="intelligence__dossier" key={row.id}>
              <div className="intelligence__dossier-top">
                <strong>{row.id}</strong>
                <span>{row.score}</span>
              </div>
              <p>{row.insight}</p>
              <div className="intelligence__dossier-meta">
                <span>{row.repo}</span>
                <span>{row.occurrences} occurrences</span>
                <span>{row.files} files</span>
                <span>{row.language}</span>
              </div>
              <code>{row.proofLane}</code>
            </article>
          ))}
        </div>
      )}
    </div>
  );
}

function EvidencePanel({
  icon,
  title,
  state,
  body,
}: {
  icon: ReactNode;
  title: string;
  state: EvidenceState;
  body: string;
}): JSX.Element {
  return (
    <article className={`intelligence__evidence is-${state}`}>
      <div className="intelligence__evidence-top">
        <span className="intelligence__evidence-icon">{icon}</span>
        <h3>{title}</h3>
        <StatePill state={state} />
      </div>
      <p>{body}</p>
    </article>
  );
}

function StatePill({
  state,
  label,
}: {
  state: EvidenceState;
  label?: string;
}): JSX.Element {
  return (
    <span className={`intelligence__state is-${state}`}>
      {label ? `${label}: ` : ''}
      {state}
    </span>
  );
}

function SeverityPill({
  severity,
}: {
  severity: InsightSeverity;
}): JSX.Element {
  return (
    <span className={`intelligence__severity is-${severity}`}>{severity}</span>
  );
}

function SeverityIcon({
  severity,
}: {
  severity: InsightSeverity;
}): JSX.Element {
  if (severity === 'critical' || severity === 'high') {
    return <AlertTriangle size={16} aria-hidden="true" />;
  }
  if (severity === 'medium') {
    return <Circle size={15} aria-hidden="true" />;
  }
  return <CheckCircle2 size={15} aria-hidden="true" />;
}

function nodeRadius(node: OperatorGraphNode): number {
  return Math.min(12, Math.max(5, 5 + node.weight));
}

function compactLabel(label: string): string {
  const tail = label.split('/').at(-1) ?? label;
  return tail.length > 18 ? `${tail.slice(0, 15)}...` : tail;
}

function diamondPoints(x: number, y: number, r: number): string {
  return `${x},${y - r} ${x + r},${y} ${x},${y + r} ${x - r},${y}`;
}

function hexPoints(x: number, y: number, r: number): string {
  const dx = r * 0.86;
  const half = r / 2;
  return [
    `${x - dx},${y - half}`,
    `${x},${y - r}`,
    `${x + dx},${y - half}`,
    `${x + dx},${y + half}`,
    `${x},${y + r}`,
    `${x - dx},${y + half}`,
  ].join(' ');
}

function toggle<T extends string>(items: T[], value: T): T[] {
  return items.includes(value)
    ? items.filter((item) => item !== value)
    : [...items, value];
}
