// 11-fleet.spec.ts — /fleet operator dashboard smoke (Slice C-web).
//
// The /fleet page renders the server-wide runner-pool fabric: per-pool cards
// (utilization / slots / queued-running-failed jobs / online-stuck runners /
// paused-saturated state), the scm/database/sandbox/cache/vault system-health
// strip, and a derived bottleneck/health alert banner. It hydrates from two
// sources that speak the same Rust read-model contract:
//
//   * first paint  — `WebBootstrap.tui.pool_activity` + `tui.system`
//   * live deltas  — the WS event spine (`global.activity` / `system.health` /
//                    `pool.{name}` frames)
//
// Playwright's `page.route` intercepts HTTP only, never raw WebSocket frames
// (see 08-ws-reconnect.spec.ts), so the live-delta path is exercised by the
// vitest render test (`src/pages/__tests__/FleetPage.test.tsx`), which drives a
// mocked `Event` payload through the realtime store. This spec asserts the
// deterministic first-paint surface: the page mounts and paints pool cards +
// the health strip from a populated bootstrap snapshot, and a saturated pool
// raises the alert banner.

import { expect, test, type Page } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import { mockFleetBootstrap } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

async function blockFleetWebSocket(page: Page): Promise<void> {
  await page.context().route('**/api/v1/ws', (route) =>
    route.abort('failed').catch(() => undefined)
  );
}

test.describe('Fleet operator dashboard (Slice C-web)', () => {
  test('renders pool cards + system-health strip from the bootstrap snapshot', async ({
    page,
  }) => {
    await blockFleetWebSocket(page);
    await mockFleetBootstrap(page, [
      {
        pool: 'trusted',
        tags: ['rust-hot'],
        running_jobs: 1,
        active_slots: 4,
        online_runners: 4,
      },
      // A saturated pool (queued work, no idle slot) must drive the banner.
      {
        pool: 'isolated',
        running_jobs: 2,
        active_slots: 2,
        queued_jobs: 5,
        online_runners: 2,
      },
    ]);

    const shell = new AppShellPage(page);
    await shell.goto('/fleet');
    await shell.assertShellLoaded();

    // The page mounts.
    await expect(page.getByTestId('fleet-page')).toBeVisible({ timeout: 10_000 });

    // Both pool cards paint.
    await expect(page.getByTestId('fleet-pool-trusted')).toBeVisible();
    await expect(page.getByTestId('fleet-pool-isolated')).toBeVisible();

    // The saturated pool is flagged on first paint; the live-delta banner text
    // is covered by the websocket reconnect and pure projection tests.
    await expect(page.getByTestId('fleet-pool-isolated')).toHaveClass(
      /is-saturated/
    );
    const banner = page.getByTestId('fleet-banner');
    await expect(banner).toBeVisible();

    // The system-health strip renders the five provider-neutral components.
    await expect(page.getByTestId('fleet-health-strip')).toBeVisible();
    for (const name of ['scm', 'database', 'sandbox', 'cache', 'vault']) {
      await expect(page.getByTestId(`fleet-health-${name}`)).toBeVisible();
    }
  });

  test('Fleet nav link routes to the page', async ({ page }) => {
    await mockFleetBootstrap(page, [{ pool: 'trusted', active_slots: 2 }]);

    const shell = new AppShellPage(page);
    await shell.goto('/');
    await shell.assertShellLoaded();

    await page.getByRole('link', { name: 'Fleet' }).click();
    await expect(page).toHaveURL(/\/fleet$/);
  });
});
