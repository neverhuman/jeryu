// 11-fleet.spec.ts — Fleet runner-network smoke (Slice C-web).
//
// The Fleet page now keeps the existing pool summary and adds a live
// runner-network drilldown sourced from `/api/v1/control-plane/runners`.
// This spec exercises the rendered node cards, active task preview, last TTY
// line, and the rule that `local` only appears when the backend payload
// actually includes it.

import { expect, test, type Page } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import {
  mockBootstrap,
  mockControlPlaneRunners,
  mockFleetBootstrap,
} from './fixtures/mocks';
import type { RunnerFabricResponse } from '../src/api/types';

test.describe.configure({ retries: 1 });

async function blockFleetWebSocket(page: Page): Promise<void> {
  await page.context().route('**/api/v1/ws', (route) =>
    route.abort('failed').catch(() => undefined)
  );
}

function runnerFabric(includeLocal: boolean): RunnerFabricResponse {
  return {
    schemaVersion: 'jeryu.runner_fabric/v1',
    local: {
      state: 'fresh',
      nodes: includeLocal ? 3 : 2,
      onlineRunners: includeLocal ? 2 : 1,
      offlineRunners: 1,
      busyRunners: includeLocal ? 2 : 1,
      idleRunners: includeLocal ? 1 : 0,
      totalSlots: includeLocal ? 32 : 30,
      activeSlots: includeLocal ? 22 : 20,
      utilization: includeLocal ? 0.091 : 0.05,
      lastUpdated: '2026-06-05T00:05:00Z',
      nodeDetails: [
        {
          runnerId: 'xbabe0',
          source: 'runnerd',
          state: 'active',
          capacity: 10,
          inFlight: 1,
          labels: ['rust', 'dogfood'],
          classes: ['native-rust-clean', 'native-rust-hot'],
          activeTaskCount: 1,
          lastUpdated: '2026-06-05T00:05:00Z',
          activeTasks: [
            {
              taskId: 'ar-000001',
              jobId: 'wc-0001',
              agentRunId: 'ar-000001',
              workcellId: 'wc-0001',
              repo: 'jeryu/veox',
              label: 'editbot',
              program: '/workspace/repair.sh',
              state: 'running',
              startedAt: '2026-06-05T00:00:00Z',
              updatedAt: '2026-06-05T00:05:00Z',
              ttyPreview: {
                state: 'fresh',
                lines: ['$ repair.sh', 'running tests', 'publishing patch'],
              },
            },
          ],
        },
        {
          runnerId: 'xbabe1',
          source: 'runnerd',
          state: 'draining',
          capacity: 10,
          inFlight: 0,
          labels: ['rust', 'dogfood'],
          classes: ['native-rust-clean', 'native-rust-hot'],
          activeTaskCount: 0,
          lastUpdated: '2026-06-05T00:04:00Z',
          activeTasks: [],
        },
        ...(includeLocal
          ? [
              {
                runnerId: 'local',
                source: 'local',
                state: 'active',
                capacity: 2,
                inFlight: 1,
                labels: ['local'],
                classes: ['native-rust-hot'],
                activeTaskCount: 1,
                lastUpdated: '2026-06-05T00:03:00Z',
                activeTasks: [
                  {
                    taskId: 'ar-local-1',
                    jobId: 'wc-local',
                    agentRunId: 'ar-local-1',
                    workcellId: 'wc-local',
                    repo: null,
                    label: 'local-repair',
                    program: '/workspace/local.sh',
                    state: 'running',
                    startedAt: '2026-06-05T00:01:00Z',
                    updatedAt: '2026-06-05T00:03:00Z',
                    ttyPreview: {
                      state: 'missing',
                      lines: [],
                    },
                  },
                ],
              },
            ]
          : []),
      ],
    },
    mirror: {
      name: 'github_actions_runners',
      state: 'missing',
      reason: 'optional GitHub mirror runner adapter is not configured',
      docsUrl: 'docs/agent-native-standard.md',
    },
  };
}

test.describe('Fleet runner-network dashboard (Slice C-web)', () => {
  test('renders node cards, active task preview, and local only when present @action:fleet.render', async ({
    page,
  }) => {
    await blockFleetWebSocket(page);
    await mockBootstrap(page);
    await mockFleetBootstrap(page, [
      {
        pool: 'trusted',
        tags: ['rust-hot'],
        running_jobs: 1,
        active_slots: 4,
        online_runners: 4,
      },
    ]);
    await mockControlPlaneRunners(page, runnerFabric(true));

    const shell = new AppShellPage(page);
    await shell.goto('/fleet');
    await shell.assertShellLoaded();

    await expect(page.getByTestId('fleet-page')).toBeVisible({ timeout: 10_000 });
    await expect(page.getByTestId('fleet-network')).toBeVisible();
    await expect(page.getByTestId('fleet-node-board')).toBeVisible();
    await expect(page.getByTestId('fleet-node-box-xbabe0')).toBeVisible();
    await expect(page.getByTestId('fleet-node-xbabe0')).toContainText(
      /xbabe0/
    );
    await expect(page.getByTestId('fleet-node-xbabe1')).toContainText(
      /draining/
    );
    await expect(page.getByTestId('fleet-node-local')).toContainText(/local/);
    await expect(page.getByTestId('fleet-task-ar-000001')).toContainText(
      'publishing patch'
    );
    await expect(page.getByTestId('fleet-task-ar-local-1')).toContainText(
      /TTY preview unavailable/i
    );

    await page.screenshot({
      path: 'playwright-report/fleet-runner-network.png',
      fullPage: true,
    });
  });

  test('does not invent a local node when the backend omits it @action:fleet.no_local_absence', async ({
    page,
  }) => {
    await blockFleetWebSocket(page);
    await mockBootstrap(page);
    await mockFleetBootstrap(page, [
      {
        pool: 'trusted',
        tags: ['rust-hot'],
        running_jobs: 1,
        active_slots: 4,
        online_runners: 4,
      },
    ]);
    await mockControlPlaneRunners(page, runnerFabric(false));

    const shell = new AppShellPage(page);
    await shell.goto('/fleet');
    await shell.assertShellLoaded();

    await expect(page.getByTestId('fleet-node-xbabe0')).toBeVisible();
    await expect(page.getByTestId('fleet-node-xbabe1')).toBeVisible();
    await expect(page.getByTestId('fleet-node-local')).toHaveCount(0);
  });

  test('clicking a task card with repo + agentRunId navigates to the agent terminal @action:fleet.task_navigation', async ({
    page,
  }) => {
    await blockFleetWebSocket(page);
    await mockBootstrap(page);
    await mockFleetBootstrap(page, [
      {
        pool: 'trusted',
        tags: ['rust-hot'],
        running_jobs: 1,
        active_slots: 4,
        online_runners: 4,
      },
    ]);
    await mockControlPlaneRunners(page, runnerFabric(true));

    const shell = new AppShellPage(page);
    await shell.goto('/fleet');
    await shell.assertShellLoaded();

    const taskCard = page.getByTestId('fleet-task-ar-000001');
    await expect(taskCard).toBeVisible();
    await expect(taskCard).toContainText('Open terminal');

    const localTask = page.getByTestId('fleet-task-ar-local-1');
    await expect(localTask).toBeVisible();
    await expect(localTask).not.toContainText('Open terminal');

    await taskCard.click();
    await page.waitForURL(/\/repos\/jeryu\/jeryu%2Fveox\/agents\/ar-000001/);
  });

  test('task card without repo remains non-interactive @action:fleet.noninteractive_card', async ({ page }) => {
    await blockFleetWebSocket(page);
    await mockBootstrap(page);
    await mockFleetBootstrap(page, [
      {
        pool: 'trusted',
        tags: ['rust-hot'],
        running_jobs: 1,
        active_slots: 4,
        online_runners: 4,
      },
    ]);
    await mockControlPlaneRunners(page, runnerFabric(true));

    const shell = new AppShellPage(page);
    await shell.goto('/fleet');
    await shell.assertShellLoaded();

    const localTask = page.getByTestId('fleet-task-ar-local-1');
    await expect(localTask).toBeVisible();
    const tagName = await localTask.evaluate((el) => el.tagName.toLowerCase());
    expect(tagName).toBe('article');
  });

});
