// 13-left-nav.spec.ts — primary navigation smoke (Slice C-web).

import { expect, test, type Page } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import {
  mockBootstrap,
  mockControlPlaneRunners,
  mockFleetBootstrap,
  mockRepoList,
} from './fixtures/mocks';

test.describe.configure({ retries: 1 });

async function blockWebSocket(page: Page): Promise<void> {
  await page.context().route('**/api/v1/ws', (route) =>
    route.abort('failed').catch(() => undefined)
  );
}

async function mockIntelligence(page: Page): Promise<void> {
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
            nodes: 1,
            onlineRunners: 1,
            offlineRunners: 0,
            busyRunners: 0,
            idleRunners: 1,
            totalSlots: 10,
            activeSlots: 10,
            utilization: 0,
            lastUpdated: '2026-06-05T00:00:00Z',
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
          mirrors: [],
          divergence: {
            state: 'unknown',
            reason: 'GitHub mirror evidence unavailable',
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
      }),
    });
  });
}

test.describe('Primary left navigation', () => {
  test('routes every left-nav destination without hitting NotFound @action:chrome.sidebar_nav @action:notifications.page @action:audit.render @action:settings.render @action:tools.nav @action:tool_fleet.nav', async ({
    page,
  }) => {
    await blockWebSocket(page);
    await mockBootstrap(page);
    await mockRepoList(page, [
      {
        id: { host: 'jeryu', owner: 'neverhuman', name: 'veox' },
        default_branch: 'main',
        description: 'Dogfood repo',
        visibility: 'public',
      },
    ]);
    await mockIntelligence(page);
    await mockFleetBootstrap(page, [
      {
        pool: 'trusted',
        tags: ['rust-hot'],
        running_jobs: 1,
        active_slots: 4,
        online_runners: 4,
      },
    ]);
    await mockControlPlaneRunners(page, {
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
        lastUpdated: '2026-06-05T00:00:00Z',
        nodeDetails: [],
      },
      mirror: {
        name: 'github_actions_runners',
        state: 'missing',
        reason: 'optional GitHub mirror runner adapter is not configured',
        docsUrl: 'docs/agent-native-standard.md',
      },
    });

    const shell = new AppShellPage(page);
    await shell.goto('/');
    await shell.assertShellLoaded();

    const routes = [
      {
        label: 'Dashboard',
        path: '/repos/family/jeryu-split',
        testId: 'repository-family-page',
      },
      {
        label: 'Repositories',
        path: '/repos',
        testId: 'repositories-page',
      },
      {
        label: 'Pull Room',
        path: '/pull-room',
        testId: 'pull-room-page',
      },
      {
        label: 'Intelligence',
        path: '/intelligence',
        testId: 'intelligence-page',
      },
      {
        label: 'Fleet',
        path: '/fleet',
        testId: 'fleet-page',
      },
      {
        label: 'Tool Fleet',
        path: '/tool-fleet',
        testId: 'tool-fleet-page',
      },
      {
        label: 'Tools',
        path: '/tools',
        testId: 'tools-page',
      },
      {
        label: 'Notifications',
        path: '/notifications',
        testId: 'notifications-page',
      },
      {
        label: 'Audit',
        path: '/audit',
        testId: 'audit-page',
      },
      {
        label: 'Settings',
        path: '/settings',
        testId: 'settings-page',
      },
      {
        label: 'Recent events',
        path: '/audit',
        testId: 'audit-page',
      },
    ] as const;

    for (const route of routes) {
      const expectedUrl = new RegExp(`${route.path.replace(/\//g, '\\/')}$`);
      await Promise.all([
        page.waitForURL(expectedUrl),
        page.getByRole('link', { name: route.label, exact: true }).click(),
      ]);
      await expect(page).toHaveURL(expectedUrl);
      await shell.assertShellLoaded();
      if ('testId' in route) {
        await expect(page.getByTestId(route.testId)).toBeVisible();
      }
      await expect(page.getByText(/Page not found/i)).toHaveCount(0);
    }
  });
});
