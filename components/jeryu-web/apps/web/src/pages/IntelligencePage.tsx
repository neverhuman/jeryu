// IntelligencePage.tsx - intelligence snapshot dashboard.

import { useMemo, useState } from 'react';
import {
  Activity,
  AlertTriangle,
  GitPullRequest,
  Network,
  Package,
  ServerCog,
  Terminal,
} from 'lucide-react';

import { useControlPlane } from '../hooks/useControlPlane';
import { useEcosystem, useToolBuildClusters } from '../hooks/useToolingEvidence';
import type { ControlPlaneSnapshot } from '../api/types';
import { buildOperatorGraph, type GraphFilters } from './intelligenceGraphModel';
import {
  EvidencePanel,
  MetricCard,
  OperatorGraphConsole,
  PriorityRow,
  StatePill,
  ToolBuildDossiers,
} from './intelligence';

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
        <p className="page__roadmap-note">Loading intelligence snapshot.</p>
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
          {query.error?.message ?? 'Intelligence snapshot unavailable.'}
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
        <p className="page__roadmap-note intelligence__header-note">
          Operational snapshot of priorities, graph state, and evidence.
        </p>
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
            Top priorities
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
          summaryClusters={snapshot.toolBuild.topClusters}
          unavailable={toolClusters.isError ? toolClusters.error.message : null}
        />
      </section>

      <section className="page__section" aria-labelledby="intelligence-health">
        <h2 className="page__section-title" id="intelligence-health">
          Evidence snapshot
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
