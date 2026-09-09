// IntelligencePage.test.tsx - render smoke for the JMCP page.

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';

import { IntelligencePage } from '../IntelligencePage';
import { CONTROL_PLANE_QUERY_KEY } from '../../hooks/useControlPlane';
import type {
  ControlPlaneSnapshot,
  EcosystemResponse,
  ToolBuildClustersResponse,
} from '../../api/types';

function renderIntelligence(snapshot: ControlPlaneSnapshot): void {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  client.setQueryData(CONTROL_PLANE_QUERY_KEY, snapshot);
  client.setQueryData(['ecosystem'], sampleEcosystem());
  client.setQueryData(['tool-build-clusters', 10], sampleToolBuildClusters());
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <IntelligencePage />
      </MemoryRouter>
    </QueryClientProvider>
  );
}

describe('IntelligencePage', () => {
  it('renders priorities, absence evidence, graph clusters, and tool dossiers', () => {
    renderIntelligence(sampleSnapshot());

    expect(screen.getByTestId('intelligence-page')).toBeInTheDocument();
    expect(screen.getByTestId('priority-pr-1-checks-missing')).toHaveTextContent(
      'PR #1 has no head checks'
    );
    expect(screen.getByText('absence=evidence')).toBeInTheDocument();
    expect(screen.getByTestId('repo-graph-preview')).toBeInTheDocument();
    expect(screen.getByTestId('operator-graph-console')).toBeInTheDocument();
    expect(screen.getByTestId('node-inspector')).toHaveTextContent(
      'Selected node'
    );
    expect(screen.getByTestId('tool-build-dossiers')).toHaveTextContent(
      'tb-routing'
    );
    expect(screen.getByText('Mirror evidence')).toBeInTheDocument();
    expect(
      screen.getAllByText(/GitHub mirror evidence unavailable/i).length
    ).toBeGreaterThan(0);
  });
});

function sampleSnapshot(): ControlPlaneSnapshot {
  return {
    schemaVersion: 'jeryu.control_plane/v1',
    generatedAt: '2026-06-05T00:00:00Z',
    localAuthority: {
      sourceOfTruth: 'local_jeryu',
      state: 'fresh',
      docsUrl: 'docs/architecture.md',
    },
    summary: {
      repoCount: 1,
      openPrCount: 1,
      draftPrCount: 1,
      queuedCheckCount: 0,
      runningCheckCount: 0,
      failingCheckCount: 0,
      missingCheckPrCount: 1,
      priorityCount: 2,
      criticalPriorityCount: 0,
      highPriorityCount: 1,
      mirrorState: 'missing',
      artifactState: 'missing',
      runnerState: 'fresh',
    },
    repos: [
      {
        id: '1',
        fullName: 'jeryu/demo',
        owner: 'jeryu',
        name: 'demo',
        defaultBranch: 'main',
        openPullRequests: 1,
        draftPullRequests: 1,
        queuedChecks: 0,
        runningChecks: 0,
        failingChecks: 0,
        latestHeadSha: null,
        state: 'fresh',
      },
    ],
    pullRequests: [
      {
        repo: 'jeryu/demo',
        number: 1,
        title: 'feature',
        draft: true,
        state: 'draft',
        headRef: 'feature',
        headSha: 'abc',
        baseRef: 'main',
        baseSha: 'def',
        mergeable: false,
        mergeableState: 'blocked',
        changedFiles: ['src/lib.rs'],
        stateEvidence: 'missing',
        sourceLinks: [],
        checks: {
          total: 0,
          queued: 0,
          running: 0,
          failing: 0,
          successful: 0,
          missing: true,
        },
      },
    ],
    checkRuns: [],
    workflows: [],
    artifacts: {
      schemaVersion: 'jeryu.artifacts.latest/v1',
      state: 'missing',
      latestBuild: {
        state: 'missing',
        artifactCount: 0,
        reason: 'local build artifacts are not stored yet',
        sourceLinks: [],
      },
      latestRelease: {
        state: 'missing',
        artifactCount: 0,
        reason: 'release evidence is absent',
        sourceLinks: [],
      },
      mirrorArtifacts: {
        state: 'missing',
        artifactCount: 0,
        reason: 'mirror artifacts unavailable',
        sourceLinks: [],
      },
      docsUrl: 'docs/release.md#release-receipt',
      absenceIsSuccess: false,
    },
    releases: {
      state: 'missing',
      latestRelease: null,
      releaseCount: 0,
      reason: 'release persistence absent',
      docsUrl: 'docs/release.md',
    },
    runners: {
      schemaVersion: 'jeryu.runner_fabric/v1',
      local: {
        state: 'fresh',
        nodes: 4,
        onlineRunners: 4,
        offlineRunners: 1,
        busyRunners: 1,
        idleRunners: 3,
        totalSlots: 40,
        activeSlots: 30,
        utilization: 0.03,
        lastUpdated: null,
        nodeDetails: [],
      },
      mirror: {
        name: 'github_actions_runners',
        state: 'missing',
        reason: 'mirror runner adapter missing',
        docsUrl: 'docs/agent-native-standard.md',
      },
    },
    workcells: {},
    agentRuns: [],
    codegraph: {
      state: 'missing',
      indexedSymbols: 0,
      indexedReferences: 0,
      crateEdges: 0,
      indexedFiles: 0,
      latestIndexRun: null,
      reason: 'codegraph empty',
    },
    toolBuild: {
      state: 'missing',
      clusterCount: 0,
      ignoredCount: 0,
      topClusters: [],
    },
    mcp: {
      state: 'fresh',
      toolCount: 42,
      liveBackedTools: ['jeryu.control_plane.status'],
      degradedTools: [],
    },
    mirror: {
      schemaVersion: 'jeryu.remote.status/v1',
      state: 'missing',
      mirrors: [
        {
          name: 'github',
          state: 'missing',
          reason: 'mirror missing',
          docsUrl: 'docs/agent-native-standard.md',
        },
      ],
      divergence: {
        state: 'unknown',
        reason: 'GitHub mirror evidence unavailable',
        localDefaultBranches: [],
        mirrorDefaultBranches: [],
      },
    },
    priorities: [
      {
        id: 'pr-1-checks-missing',
        title: 'PR #1 has no head checks',
        severity: 'high',
        score: 840,
        confidence: 1,
        owner: 'forge-api',
        proofLane: 'cargo test -p jeryu-api --features web --jobs 40 control_plane',
        recommendedAction: 'refresh check-runs',
        evidence: ['head_sha=abc'],
        sourceLinks: [],
        state: 'missing',
        rulesVersion: 'rules-v1',
      },
      {
        id: 'github-mirror-missing',
        title: 'GitHub mirror evidence unavailable',
        severity: 'medium',
        score: 600,
        confidence: 1,
        owner: 'forge-api',
        proofLane: 'cargo test -p jeryu-api --features web --jobs 40 control_plane',
        recommendedAction: 'attach read-only mirror evidence',
        evidence: ['divergence unknown'],
        sourceLinks: [],
        state: 'missing',
        rulesVersion: 'rules-v1',
      },
    ],
    repoGraph: {
      schemaVersion: 'jeryu.repo_graph/v1',
      generatedAt: '2026-06-05T00:00:00Z',
      nodes: [
        {
          id: 'repo:jeryu/demo',
          label: 'jeryu/demo',
          kind: 'repo',
          state: 'fresh',
          weight: 2,
          metadata: {},
        },
        {
          id: 'mirror:github',
          label: 'GitHub mirror',
          kind: 'remote_mirror',
          state: 'missing',
          weight: 1,
          metadata: {},
        },
      ],
      edges: [],
      clusters: [
        {
          id: 'cluster:superseded-mirror',
          label: 'Mirror evidence',
          kind: 'superseded_mirror',
          state: 'missing',
          severity: 'medium',
          nodeIds: ['mirror:github'],
          insights: ['GitHub mirror evidence unavailable'],
        },
      ],
      insights: [],
    },
  };
}

function sampleEcosystem(): EcosystemResponse {
  return {
    live: true,
    degradedReason: '',
    tools: [
      {
        name: 'jeryu.control_plane.status',
        className: 'ControlPlaneStatus',
        conformance: 'read-only',
        sideEffects: [],
        dataClasses: ['control-plane'],
        dependsOn: [],
        repo: 'jeryu/demo',
      },
    ],
  };
}

function sampleToolBuildClusters(): ToolBuildClustersResponse {
  return {
    schema_version: 'jeryu.tool_build.clusters/v1',
    repo: null,
    include_ignored: false,
    clusters: [
      {
        cluster_id: 'tb-routing',
        repo_id: 'jeryu/demo',
        commit_sha: 'abc',
        fingerprint: 'fp',
        score: 91,
        occurrence_count: 5,
        repo_count: 1,
        file_count: 3,
        total_lines: 80,
        language: 'rust',
        insight: 'Repeated route glue can become a local tool.',
        normalized_preview: 'route handler',
        occurrences: [],
      },
    ],
  };
}
