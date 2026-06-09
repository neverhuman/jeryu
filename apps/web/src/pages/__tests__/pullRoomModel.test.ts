import { describe, expect, it } from 'vitest';

import type { ControlPlaneSnapshot, ControlPullRequest } from '../../api/types';
import {
  DEFAULT_PULL_ROOM_FILTERS,
  filterPullRequests,
  fromControlPullRequest,
  groupPullRequests,
  rankToolBuildOpportunities,
} from '../pullRoomModel';

describe('pullRoomModel', () => {
  it('groups PRs into check posture lanes', () => {
    const items = [
      pr({ number: 1, checks: { total: 0, missing: true } }),
      pr({ number: 2, checks: { total: 2, failing: 1 } }),
      pr({ number: 3, checks: { total: 1, running: 1 } }),
      pr({ number: 4, checks: { total: 2, successful: 2 } }),
      pr({ number: 5, state: 'merged', checks: { total: 2, successful: 2 } }),
    ].map(fromControlPullRequest);

    const lanes = groupPullRequests(items);

    expect(lanes.find((lane) => lane.id === 'missing_checks')?.items).toHaveLength(1);
    expect(lanes.find((lane) => lane.id === 'failing_checks')?.items).toHaveLength(1);
    expect(lanes.find((lane) => lane.id === 'queued_running_checks')?.items).toHaveLength(1);
    expect(lanes.find((lane) => lane.id === 'ready_reviewable')?.items).toHaveLength(1);
    expect(lanes.find((lane) => lane.id === 'merged_closed')?.items).toHaveLength(1);
  });

  it('filters by repo, evidence state, check posture, and text', () => {
    const items = [
      pr({ repo: 'alice/jeryu', title: 'Fix cache key', stateEvidence: 'missing' }),
      pr({ repo: 'bob/api', title: 'Runner drain', checks: { total: 1, successful: 1 } }),
    ].map(fromControlPullRequest);

    expect(
      filterPullRequests(items, {
        ...DEFAULT_PULL_ROOM_FILTERS,
        repo: 'alice/jeryu',
        evidence: 'missing',
        checkPosture: 'missing',
        search: 'cache',
      }).map((item) => item.title)
    ).toEqual(['Fix cache key']);
  });

  it('ranks populated tool-build clusters before lower-scored summary fallbacks', () => {
    const ranked = rankToolBuildOpportunities(snapshot(), [
      {
        cluster_id: 'cluster-live',
        repo_id: 'alice/jeryu',
        commit_sha: 'abc',
        fingerprint: 'fp',
        score: 99,
        occurrence_count: 4,
        repo_count: 1,
        file_count: 3,
        total_lines: 80,
        language: 'rust',
        insight: 'normalized polling loop',
        normalized_preview: 'loop call retry',
        occurrences: [],
      },
    ]);

    expect(ranked[0]).toMatchObject({
      id: 'cluster-live',
      score: 99,
      suggestedProofLane: 'cargo test -p jeryu-codegraph --jobs 40 tool_build',
    });
    expect(ranked.map((item) => item.id)).toContain('cluster-summary');
  });
});

type PrOverrides = Omit<Partial<ControlPullRequest>, 'checks'> & {
  checks?: Partial<ControlPullRequest['checks']>;
};

function pr(overrides: PrOverrides = {}): ControlPullRequest {
  return {
    repo: overrides.repo ?? 'alice/jeryu',
    number: overrides.number ?? 1,
    title: overrides.title ?? `PR ${overrides.number ?? 1}`,
    draft: overrides.draft ?? false,
    state: overrides.state ?? 'open',
    headRef: overrides.headRef ?? 'feature',
    headSha: overrides.headSha ?? 'head',
    baseRef: overrides.baseRef ?? 'main',
    baseSha: overrides.baseSha ?? 'base',
    mergeable: overrides.mergeable ?? false,
    mergeableState: overrides.mergeableState ?? 'blocked',
    changedFiles: overrides.changedFiles ?? [],
    stateEvidence: overrides.stateEvidence ?? 'fresh',
    sourceLinks: overrides.sourceLinks ?? [],
    checks: {
      total: 0,
      queued: 0,
      running: 0,
      failing: 0,
      successful: 0,
      missing: false,
      ...overrides.checks,
    },
  };
}

function snapshot(): ControlPlaneSnapshot {
  return {
    schemaVersion: 'jeryu.control_plane/v1',
    generatedAt: '2026-06-05T00:00:00Z',
    localAuthority: {
      sourceOfTruth: 'local_jeryu',
      state: 'fresh',
      docsUrl: 'docs/architecture.md',
    },
    summary: {
      repoCount: 0,
      openPrCount: 0,
      draftPrCount: 0,
      queuedCheckCount: 0,
      runningCheckCount: 0,
      failingCheckCount: 0,
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
      docsUrl: 'docs/release.md',
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
      state: 'missing',
      indexedSymbols: 0,
      indexedReferences: 0,
      crateEdges: 0,
      indexedFiles: 0,
      latestIndexRun: null,
      reason: 'empty',
    },
    toolBuild: {
      state: 'fresh',
      clusterCount: 1,
      ignoredCount: 0,
      topClusters: [
        {
          clusterId: 'cluster-summary',
          repoId: 'bob/api',
          score: 10,
          occurrenceCount: 2,
          fileCount: 2,
          insight: 'summary cluster',
        },
      ],
    },
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
      nodes: [],
      edges: [],
      clusters: [],
      insights: [],
    },
  };
}
