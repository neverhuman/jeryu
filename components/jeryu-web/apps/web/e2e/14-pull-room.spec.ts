// 14-pull-room.spec.ts - W-FE-11 Pull Room smoke.

import { expect, test, type Page } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import { mockBootstrap } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

async function blockWebSocket(page: Page): Promise<void> {
  await page.context().route('**/api/v1/ws', (route) =>
    route.abort('failed').catch(() => undefined)
  );
}

async function mockPullRoom(page: Page): Promise<void> {
  await page.route('**/api/v1/control-plane/status', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(controlPlane()),
    });
  });
  await page.route('**/api/v1/ecosystem', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
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
      }),
    });
  });
  await page.route('**/api/v1/codegraph/tool-build/clusters**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: 'codegraph.tool_build/v1',
        repo: null,
        include_ignored: false,
        clusters: [
          {
            cluster_id: 'cluster-live',
            repo_id: 'alice/jeryu',
            commit_sha: 'abc',
            fingerprint: 'fp',
            score: 92,
            occurrence_count: 4,
            repo_count: 1,
            file_count: 3,
            total_lines: 64,
            language: 'rust',
            insight: 'normalized retry loop repeated across API clients',
            normalized_preview: 'loop retry call',
            occurrences: [],
          },
        ],
      }),
    });
  });
}

test('Pull Room renders filters, queue lanes, PR cards, tooling rail, and cockpit links @action:pull_room.filters @action:pull_room.search @action:pull_room.cockpit_link', async ({
  page,
}) => {
  await blockWebSocket(page);
  await mockBootstrap(page);
  await mockPullRoom(page);

  const shell = new AppShellPage(page);
  await shell.goto('/pull-room');
  await shell.assertShellLoaded();

  await expect(page.getByTestId('pull-room-page')).toBeVisible();
  await page.getByLabel('Search pull requests').fill('Fix');
  await page
    .locator('section[aria-label="Pull Room filters"]')
    .getByLabel('Checks')
    .selectOption('missing');
  await expect(page.getByTestId('pull-lane-missing_checks')).toBeVisible();
  await expect(page.getByTestId('pull-lane-failing_checks')).toBeVisible();
  await expect(page.getByText('Fix BFF PR list')).toBeVisible();
  await expect(page.getByText('Tooling opportunities')).toBeVisible();
  await expect(page.getByText('normalized retry loop repeated across API clients')).toBeVisible();

  const link = page.getByRole('link', { name: 'Fix BFF PR list' });
  await expect(link).toHaveAttribute(
    'href',
    '/repos/jeryu/alice%2Fjeryu/pulls/7'
  );
  await expect(page.getByText(/W-FE-11/i)).toHaveCount(0);
});

function controlPlane(): Record<string, unknown> {
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
      openPrCount: 2,
      draftPrCount: 0,
      queuedCheckCount: 1,
      runningCheckCount: 0,
      failingCheckCount: 1,
      missingCheckPrCount: 1,
      priorityCount: 0,
      criticalPriorityCount: 0,
      highPriorityCount: 0,
      mirrorState: 'missing',
      artifactState: 'missing',
      runnerState: 'fresh',
    },
    repos: [
      {
        id: 'repo-1',
        fullName: 'alice/jeryu',
        owner: 'alice',
        name: 'jeryu',
        defaultBranch: 'main',
        private: false,
        archived: false,
        disabled: false,
        openPullRequests: 2,
        draftPullRequests: 0,
        queuedChecks: 1,
        runningChecks: 0,
        failingChecks: 1,
        latestHeadSha: 'head-7',
        state: 'fresh',
      },
    ],
    pullRequests: [
      {
        repo: 'alice/jeryu',
        number: 7,
        title: 'Fix BFF PR list',
        draft: false,
        state: 'open',
        headRef: 'feature/pulls',
        headSha: 'head-7',
        baseRef: 'main',
        baseSha: 'base-7',
        mergeable: false,
        mergeableState: 'blocked',
        changedFiles: ['crates/jeryu-api/src/web/pulls.rs'],
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
      {
        repo: 'alice/jeryu',
        number: 8,
        title: 'Repair check posture',
        draft: false,
        state: 'open',
        headRef: 'feature/checks',
        headSha: 'head-8',
        baseRef: 'main',
        baseSha: 'base-8',
        mergeable: false,
        mergeableState: 'blocked',
        changedFiles: ['apps/web/src/pages/PullRoomPage.tsx'],
        stateEvidence: 'failed',
        sourceLinks: [],
        checks: {
          total: 1,
          queued: 0,
          running: 0,
          failing: 1,
          successful: 0,
          missing: false,
        },
      },
    ],
    checkRuns: [],
    workflows: [],
    releases: { state: 'missing', latestRelease: null, releaseCount: 0, reason: '', docsUrl: '' },
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
        nodes: 1,
        onlineRunners: 1,
        offlineRunners: 0,
        busyRunners: 0,
        idleRunners: 1,
        totalSlots: 10,
        activeSlots: 10,
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
      indexedSymbols: 10,
      indexedReferences: 30,
      crateEdges: 2,
      indexedFiles: 5,
      latestIndexRun: null,
      reason: 'codegraph store is reachable',
    },
    toolBuild: {
      state: 'fresh',
      clusterCount: 1,
      ignoredCount: 0,
      topClusters: [],
    },
    mcp: { state: 'fresh', toolCount: 2, liveBackedTools: [], degradedTools: [] },
    mirror: {
      schemaVersion: 'jeryu.remote.status/v1',
      state: 'missing',
      mirrors: [],
      divergence: {
        state: 'unknown',
        reason: 'mirror missing',
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
