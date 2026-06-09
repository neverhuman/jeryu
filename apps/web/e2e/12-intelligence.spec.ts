// 12-intelligence.spec.ts - JMCP/control-plane smoke.

import { expect, test, type Page } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import { mockBootstrap } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

async function blockWebSocket(page: Page): Promise<void> {
  await page.context().route('**/api/v1/ws', (route) =>
    route.abort('failed').catch(() => undefined)
  );
}

async function mockControlPlane(page: Page): Promise<void> {
  await page.route('**/api/v1/control-plane/status', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
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
          priorityCount: 1,
          criticalPriorityCount: 0,
          highPriorityCount: 1,
          mirrorState: 'missing',
          artifactState: 'missing',
          runnerState: 'fresh',
        },
        repos: [],
        pullRequests: [],
        checkRuns: [
          {
            id: 'check:ci',
            repo: 'jeryu/demo',
            name: 'ci/fast',
            headSha: 'abc',
            status: 'completed',
            conclusion: 'failure',
            state: 'failed',
          },
        ],
        workflows: [],
        releases: {
          state: 'missing',
          latestRelease: null,
          releaseCount: 0,
          reason: 'release persistence absent',
          docsUrl: 'docs/release.md',
        },
        artifacts: {
          schemaVersion: 'jeryu.artifacts.latest/v1',
          state: 'missing',
          latestBuild: {
            state: 'missing',
            artifactCount: 0,
            reason: 'local build artifacts absent',
            sourceLinks: [],
          },
          latestRelease: {
            state: 'missing',
            artifactCount: 0,
            reason: 'release artifact evidence absent',
            sourceLinks: [],
          },
          mirrorArtifacts: {
            state: 'missing',
            artifactCount: 0,
            reason: 'mirror artifact adapter missing',
            sourceLinks: [],
          },
          docsUrl: 'docs/release.md#release-receipt',
          absenceIsSuccess: false,
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
            reason: 'runner mirror missing',
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
          state: 'fresh',
          clusterCount: 1,
          ignoredCount: 0,
          topClusters: [
            {
              clusterId: 'tb-routing',
              repoId: 'jeryu/demo',
              score: 91,
              occurrenceCount: 5,
              fileCount: 3,
              insight: 'Repeated route glue can become a local tool.',
            },
          ],
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
          mirrors: [],
          divergence: {
            state: 'unknown',
            reason: 'GitHub mirror evidence unavailable',
            localDefaultBranches: [],
            mirrorDefaultBranches: [],
          },
        },
        priorities: [
          {
            id: 'pr-63-checks-missing',
            title: 'PR #63 has no head checks',
            severity: 'high',
            score: 840,
            confidence: 1,
            owner: 'forge-api',
            proofLane:
              'cargo test -p jeryu-api --features web --jobs 40 control_plane',
            recommendedAction: 'refresh PR head check-runs',
            evidence: ['head_sha=abc'],
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
              id: 'pr:jeryu/demo#63',
              label: 'PR #63',
              kind: 'pull_request',
              state: 'missing',
              weight: 2,
              metadata: { repo: 'jeryu/demo' },
            },
            {
              id: 'check:ci',
              label: 'ci/fast',
              kind: 'check_run',
              state: 'failed',
              weight: 1,
              metadata: { repo: 'jeryu/demo' },
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
          edges: [
            {
              source: 'repo:jeryu/demo',
              target: 'pr:jeryu/demo#63',
              kind: 'owns_pr',
              state: 'fresh',
              weight: 1,
            },
            {
              source: 'pr:jeryu/demo#63',
              target: 'check:ci',
              kind: 'has_check',
              state: 'failed',
              weight: 1,
            },
          ],
          clusters: [
            {
              id: 'cluster:stale-mirror',
              label: 'Mirror evidence',
              kind: 'stale_mirror',
              state: 'missing',
              severity: 'medium',
              nodeIds: ['mirror:github'],
              insights: ['GitHub mirror evidence unavailable'],
            },
          ],
          insights: [],
        },
      }),
    });
  });
}

async function mockToolingEvidence(page: Page): Promise<void> {
  await page.route('**/api/v1/ecosystem', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        live: true,
        degradedReason: '',
        tools: [
          {
            name: 'jeryu.control_plane.status',
            className: 'ControlPlaneStatus',
            conformance: 'read-only',
            sideEffects: [],
            dataClasses: ['control-plane'],
            repo: 'jeryu/demo',
            dependsOn: [],
          },
          {
            name: 'jeryu.repo_graph.clusters',
            className: 'RepoGraphClusters',
            conformance: 'read-only',
            sideEffects: [],
            dataClasses: ['repo-graph'],
            repo: 'jeryu/demo',
            dependsOn: ['jeryu.control_plane.status'],
          },
        ],
      }),
    });
  });
  await page.route(
    '**/api/v1/codegraph/tool-build/clusters**',
    async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
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
        }),
      });
    }
  );
}

test.describe('Intelligence control-plane page', () => {
  test('renders priority, graph, and explicit absence evidence', async ({
    page,
  }) => {
    await blockWebSocket(page);
    await mockBootstrap(page);
    await mockControlPlane(page);
    await mockToolingEvidence(page);

    const shell = new AppShellPage(page);
    await shell.goto('/intelligence');
    await shell.assertShellLoaded();

    await expect(page.getByTestId('intelligence-page')).toBeVisible({
      timeout: 10_000,
    });
    await expect(page.getByTestId('priority-pr-63-checks-missing')).toBeVisible();
    await expect(page.getByText('absence=evidence')).toBeVisible();
    await expect(page.getByTestId('operator-graph-console')).toBeVisible();
    await expect(page.getByTestId('repo-graph-preview')).toBeVisible();
    await expect(page.getByTestId('graph-node-repo:jeryu/demo')).toBeVisible();
    await expect(
      page.getByTestId('repo-graph-preview').getByText('failed')
    ).toBeVisible();
    await expect(page.getByTestId('node-inspector')).toContainText('Selected node');
    await expect(page.getByTestId('tool-build-dossiers')).toContainText('tb-routing');
    await expect(page.getByText('Mirror evidence').first()).toBeVisible();
  });

  test('Intelligence nav link routes to the page', async ({ page }) => {
    await blockWebSocket(page);
    await mockBootstrap(page);
    await mockControlPlane(page);
    await mockToolingEvidence(page);

    const shell = new AppShellPage(page);
    await shell.goto('/');
    await shell.assertShellLoaded();

    await page.getByRole('link', { name: 'Intelligence' }).click();
    await expect(page).toHaveURL(/\/intelligence$/);
  });
});
