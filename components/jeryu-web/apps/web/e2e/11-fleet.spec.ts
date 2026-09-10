// 11-fleet.spec.ts — Fleet runner-network smoke (Slice C-web).
//
// The Fleet page now keeps the existing pool summary and adds a live
// runner-network drilldown sourced from `/api/v1/control-plane/runners`.
// This spec exercises the rendered node cards, active task preview, last TTY
// line, and the rule that `local` only appears when the backend payload
// actually includes it.

import { readFileSync } from 'node:fs';

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

  test('retains observed jobs and tasks without inventing runner capacity @action:fleet.render', async ({ page }, testInfo) => {
    await blockFleetWebSocket(page);
    await mockBootstrap(page);
    const now = new Date().toISOString();
    const bootstrap = JSON.parse(readFileSync(
      new URL('./fixtures/data/bootstrap.json', import.meta.url), 'utf8'
    )) as Record<string, unknown>;
    bootstrap.tui = {
      generated_at: now,
      pool_activity: {
        repos: [{ repo: 'jeryu/veox', pools: ['trusted'] }],
        pools: [{
          pool: 'trusted', tags: [], trust_tier: 'unverified', paused: false,
          queued_jobs: 5, running_jobs: 2, failed_jobs: 3,
          active_slots: 0, configured_max_slots: 0, online_runners: 0,
          stuck_runners: 0,
        }],
        unplaceable: [],
        freshness: {
          source: 'broker', state: 'unknown', observed_at: null, age_ms: null,
          cursor: null, ttl_ms: null, confidence: 0, last_error: null,
          degraded_reason: 'runner capacity registry is not connected',
        },
      },
      system: {},
    };
    await page.route('**/api/v1/bootstrap', (route) => route.fulfill({
      status: 200, contentType: 'application/json', body: JSON.stringify(bootstrap),
    }));
    const runners = runnerFabric(false);
    runners.local = {
      ...runners.local,
      state: 'unknown', onlineRunners: 0, offlineRunners: 0, busyRunners: 0,
      idleRunners: 0, totalSlots: 0, activeSlots: 0, utilization: 0,
      lastUpdated: now,
      nodeDetails: runners.local.nodeDetails.map((node) => ({
        ...node, source: 'workcell', state: 'unknown', capacity: 0,
        inFlight: 0, lastUpdated: now,
      })),
    };
    await mockControlPlaneRunners(page, runners);
    const shell = new AppShellPage(page);
    await shell.goto('/fleet');
    await shell.assertShellLoaded();

    const fleet = page.getByTestId('fleet-page');
    const pool = page.getByTestId('fleet-pool-trusted');
    await expect(pool).toBeVisible();
    await expect(pool).toContainText('capacity unknown');
    await expect(pool).not.toHaveClass(/is-saturated/);
    await expect(pool.getByRole('progressbar')).toHaveCount(0);
    for (const [label, value] of [['Queued', '5'], ['Running', '2'], ['Failed', '3']]) {
      await expect(pool.locator('.fleet__stat').filter({ has: page.getByText(label, { exact: true }) }).locator('dd')).toHaveText(value);
    }
    await expect(page.getByTestId('fleet-banner')).toContainText('Awaiting fleet telemetry.');
    await expect(page.getByTestId('fleet-banner')).not.toHaveAttribute('role', 'alert');
    await expect(fleet).toContainText('Runner capacity unknown');
    await expect(fleet).not.toContainText(/0%|0 slots|0 online|All pools healthy/);
    const node = page.getByTestId('fleet-node-xbabe0');
    await expect(node).toContainText(/Capacity\s*unknown/);
    await expect(page.getByTestId('fleet-node-box-xbabe0')).toHaveClass(/is-unknown/);
    await expect(page.getByTestId('fleet-node-box-xbabe0').locator('.fleet__slot-grid')).toHaveCount(0);
    const task = page.getByTestId('fleet-task-ar-000001');
    await expect(task).toContainText('publishing patch');
    await expect(task).toHaveAttribute('href', '/repos/jeryu/jeryu%2Fveox/agents/ar-000001');

    const screenshot = testInfo.outputPath('fleet-capacity-unknown.png');
    await page.screenshot({ path: screenshot, fullPage: true });
    await testInfo.attach('fleet-capacity-unknown', { path: screenshot, contentType: 'image/png' });
  });

});
