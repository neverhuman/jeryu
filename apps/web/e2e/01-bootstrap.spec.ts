// 01-bootstrap.spec.ts — bootstrap + dashboard smoke (W-T-09).
//
// Two-tier smoke:
//   1. BFF cold-load: `GET /api/v1/bootstrap` returns a valid WebBootstrap
//      envelope (Phase 1 contract — always runs).
//   2. SPA shell paint: navigate to `/`, expect the AppShell layout to
//      mount, the viewer's login to surface in the GlobalHeader / UserMenu,
//      the live indicator to leave "Idle" (transition through connecting /
//      reconnecting / open), and the LiveActivityDock to render.
//
// Phase 2/3 has landed: `useRealtime` no longer infinite-loops, the BFF
// serves the SPA from `apps/web/dist`, and `JERYU_WEB_TRUST_LOCAL=1` lets
// the WebSocket handshake succeed without a session cookie.

import { expect, test } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import { mockBootstrap } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

test.describe('Bootstrap + Dashboard (W-T-09)', () => {
  test('BFF bootstrap endpoint returns a valid envelope', async ({ request }) => {
    // Hit the BFF directly so this tier passes even when the SPA shell
    // is broken. This is the Phase 1 contract the SPA depends on.
    const res = await request.get('/api/v1/bootstrap');
    expect(res.status(), 'bootstrap must return 200').toBe(200);
    const body = await res.json();
    expect(body.schema_version, 'schema_version present').toBeTruthy();
    expect(body.viewer?.login, 'viewer.login present').toBeTruthy();
    expect(Array.isArray(body.viewer?.global_permissions)).toBe(true);
    expect(body.websocket_url, 'websocket_url present').toBeTruthy();
    expect(typeof body.feature_flags?.markdown_html).toBe('boolean');
  });

  test('SPA cold-load renders shell + dashboard', async ({ page }) => {
    // Pin the bootstrap viewer to a deterministic login so the assertion
    // does not depend on the BFF's local-dev viewer string.
    await mockBootstrap(page, {
      login: '@e2e',
      display_name: 'E2E Tester',
    });

    const shell = new AppShellPage(page);
    await shell.goto('/');

    // 1. AppShell layout mounts (`<div class="app-shell">`).
    await shell.assertShellLoaded();

    // 2. GlobalHeader surfaces the viewer's login via the UserMenu.
    // UserMenu renders `displayName ?? login` so we assert the display
    // name from the bootstrap mock is visible in the header.
    await expect(page.locator('.global-header')).toContainText('E2E Tester', {
      timeout: 5_000,
    });

    // 3. Live indicator transitions away from the initial `idle` state.
    // The WS upgrade against the local BFF (with `JERYU_WEB_TRUST_LOCAL=1`)
    // is expected to succeed — but on slower runners we may see
    // "Connecting" / "Reconnecting" before "Live". All four are acceptable;
    // the contract under test is that the state leaves "Idle" within a
    // generous budget.
    await expect(shell.liveIndicator).toBeVisible({ timeout: 5_000 });
    await expect(shell.liveIndicator).toContainText(
      /Live|Connecting|Reconnecting/i,
      { timeout: 10_000 }
    );

    // 4. LiveActivityDock renders inside the app shell.
    await expect(
      page.locator('.activity-dock, [aria-label="Live activity"]').first()
    ).toBeVisible({ timeout: 5_000 });

    // 5. Main outlet renders the dashboard content.
    await expect(page.locator('main#main-content')).toBeVisible();

    // Best-effort evidence only: keep the lane green even if Chromium fails
    // to capture a screenshot on a transient run.
    try {
      await page.screenshot({
        path: 'playwright-report/dashboard-loaded.png',
        fullPage: true,
      });
    } catch {
      // Ignore screenshot capture flakiness.
    }
  });
});
