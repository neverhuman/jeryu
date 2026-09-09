import { describe, expect, it } from 'vitest';

import type { ControlPlaneSnapshot, EcosystemResponse, ToolBuildCluster } from '../../api/types';
import {
  buildOperatorGraph,
  emptyGraph,
  nodeColorClass,
  nodeShape,
} from '../intelligenceGraphModel';

describe('intelligenceGraphModel', () => {
  it('maps node state and kind to deterministic visual classes', () => {
    expect(nodeColorClass('failed')).toBe('is-failed');
    expect(nodeShape('repo')).toBe('circle');
    expect(nodeShape('runner_capacity')).toBe('diamond');
    expect(nodeShape('tool_build')).toBe('hex');
    expect(nodeShape('pull_request')).toBe('rect');
  });

  it('selects node details with inbound, outbound, clusters, and evidence counts', () => {
    const graph = buildOperatorGraph(
      snapshot(),
      ecosystem(),
      [cluster()],
      { kinds: [], states: [], query: '' },
      'repo:alice/jeryu'
    );

    expect(graph.selected?.node.label).toBe('alice/jeryu');
    expect(graph.selected?.outbound.map((edge) => edge.kind)).toContain('owns_pr');
    expect(graph.selected?.evidenceCount).toBeGreaterThan(0);
    expect(graph.kindOptions).toContain('ecosystem_tool');
    expect(graph.kindOptions).toContain('tool_build');
  });

  it('filters by kind, state, and query without inventing missing overlays', () => {
    const graph = buildOperatorGraph(
      snapshot(),
      null,
      [],
      { kinds: ['check_run'], states: ['failed'], query: 'ci' },
      null
    );

    expect(graph.nodes.map((node) => node.id)).toEqual(['check:ci']);
    expect(graph.selected?.node.id).toBe('check:ci');
  });

  it('reports an empty base graph and suppresses ignored tool-build clusters', () => {
    const base = snapshot({ nodes: [], edges: [], clusters: [] });
    expect(emptyGraph(base)).toBe(true);
    const graph = buildOperatorGraph(
      base,
      null,
      [{ ...cluster(), ignored: { cluster_id: 'tb-1', reason: 'fixture', ignored_by: 'test', ignored_at: '0' } }],
      { kinds: [], states: [], query: '' },
      null
    );
    expect(graph.nodes).toEqual([]);
    expect(graph.selected).toBeNull();
  });
});

function snapshot(
  graph: Partial<ControlPlaneSnapshot['repoGraph']> = {}
): ControlPlaneSnapshot {
  return {
    schemaVersion: 'jeryu.control_plane/v1',
    generatedAt: '2026-06-05T00:00:00Z',
    localAuthority: { sourceOfTruth: 'local_jeryu', state: 'fresh', docsUrl: '' },
    summary: {
      repoCount: 1,
      openPrCount: 1,
      draftPrCount: 0,
      queuedCheckCount: 0,
      runningCheckCount: 0,
      failingCheckCount: 1,
      missingCheckPrCount: 0,
      priorityCount: 0,
      criticalPriorityCount: 0,
      highPriorityCount: 0,
      mirrorState: 'missing',
      artifactState: 'missing',
      runnerState: 'fresh',
    },
    repos: [],
    pullRequests: [],
    checkRuns: [],
    workflows: [],
    releases: {},
    artifacts: {
      schemaVersion: 'jeryu.artifacts.latest/v1',
      state: 'missing',
      latestBuild: { state: 'missing', artifactCount: 0, reason: '', sourceLinks: [] },
      latestRelease: { state: 'missing', artifactCount: 0, reason: '', sourceLinks: [] },
      mirrorArtifacts: { state: 'missing', artifactCount: 0, reason: '', sourceLinks: [] },
      docsUrl: '',
      absenceIsSuccess: false,
    },
    runners: {
      schemaVersion: 'jeryu.runner_fabric/v1',
      local: {
        state: 'fresh',
        nodes: 0,
        onlineRunners: 0,
        offlineRunners: 0,
        busyRunners: 0,
        idleRunners: 0,
        totalSlots: 0,
        activeSlots: 0,
        utilization: 0,
        lastUpdated: null,
        nodeDetails: [],
      },
      mirror: { name: 'mirror', state: 'missing', reason: '', docsUrl: '' },
    },
    workcells: {},
    agentRuns: [],
    codegraph: {
      state: 'fresh',
      indexedSymbols: 2,
      indexedReferences: 4,
      crateEdges: 1,
      indexedFiles: 2,
      latestIndexRun: null,
      reason: 'fresh',
    },
    toolBuild: { state: 'fresh', clusterCount: 1, ignoredCount: 0, topClusters: [] },
    mcp: { state: 'fresh', toolCount: 0, liveBackedTools: [], degradedTools: [] },
    mirror: {
      schemaVersion: 'jeryu.remote.status/v1',
      state: 'missing',
      mirrors: [],
      divergence: {
        state: 'unknown',
        reason: '',
        localDefaultBranches: [],
        mirrorDefaultBranches: [],
      },
    },
    priorities: [],
    repoGraph: {
      schemaVersion: 'jeryu.repo_graph/v1',
      generatedAt: '2026-06-05T00:00:00Z',
      nodes: [
        {
          id: 'repo:alice/jeryu',
          label: 'alice/jeryu',
          kind: 'repo',
          state: 'fresh',
          weight: 2,
          metadata: { owner: 'alice' },
        },
        {
          id: 'pr:alice/jeryu#1',
          label: 'PR #1',
          kind: 'pull_request',
          state: 'missing',
          weight: 2,
          metadata: { repo: 'alice/jeryu' },
        },
        {
          id: 'check:ci',
          label: 'ci/fast',
          kind: 'check_run',
          state: 'failed',
          weight: 1,
          metadata: { repo: 'alice/jeryu' },
        },
      ],
      edges: [
        {
          source: 'repo:alice/jeryu',
          target: 'pr:alice/jeryu#1',
          kind: 'owns_pr',
          state: 'fresh',
          weight: 1,
        },
        {
          source: 'pr:alice/jeryu#1',
          target: 'check:ci',
          kind: 'has_check',
          state: 'failed',
          weight: 1,
        },
      ],
      clusters: [
        {
          id: 'cluster:ci',
          label: 'CI blockers',
          kind: 'ci_blocker',
          state: 'failed',
          severity: 'high',
          nodeIds: ['check:ci'],
          insights: ['check failed'],
        },
      ],
      insights: [],
      ...graph,
    },
  };
}

function ecosystem(): EcosystemResponse {
  return {
    live: true,
    degradedReason: '',
    tools: [
      {
        name: 'jeryu.get_system_snapshot',
        className: 'GetSystemSnapshot',
        conformance: 'read-only',
        sideEffects: ['read-only'],
        dataClasses: [],
        dependsOn: [],
      },
      {
        name: 'jeryu.propose_patch',
        className: 'ProposePatch',
        conformance: 'mutating',
        sideEffects: ['mutating'],
        dataClasses: ['repo'],
        dependsOn: ['jeryu.get_system_snapshot'],
      },
    ],
  };
}

function cluster(): ToolBuildCluster {
  return {
    cluster_id: 'tb-1',
    repo_id: 'alice/jeryu',
    commit_sha: 'abc',
    fingerprint: 'fp',
    score: 90,
    occurrence_count: 3,
    repo_count: 1,
    file_count: 2,
    total_lines: 40,
    language: 'rust',
    insight: 'repeated retry loop',
    normalized_preview: 'loop retry call',
    occurrences: [],
  };
}
