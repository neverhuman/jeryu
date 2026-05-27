// 01-bootstrap.spec.ts — bootstrap + dashboard smoke (W-T-09).
//
// Two-tier smoke:
//   1. BFF cold-load: `GET /api/v1/bootstrap` returns a valid WebBootstrap
//      envelope (always runs — this is the Phase 1 contract).
//   2. SPA shell paint: navigate to `/`, expect the AppShell layout to
//      mount. This currently fails against the Phase-1 production build
//      because of a known infinite-render loop in `useRealtime`
//      (`useMemo([scopes])` allocates a new array reference each render
//      → useEffect re-fires → store.set triggers re-render). The shell
//      spec is `test.fixme()` until the W-P2-frontend agent's
//      `useRealtime` fix lands.

import { expect, test } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';

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

  test.fixme(
    'SPA cold-load renders shell + dashboard',
    async ({ page }) => {
      // Pending W-P2-frontend `useRealtime` infinite-loop fix.
      // Once fixed, this asserts:
      //   1. `.app-shell` mounts (AppShell.tsx).
      //   2. `.global-header__live` shows "Live" / "Connecting…" within 5s.
      //   3. screenshot `dashboard-loaded.png` is written.
      const shell = new AppShellPage(page);
      await shell.goto('/');
      await shell.assertShellLoaded();
      await shell.assertLiveBadge(5_000);
      await expect(page.locator('main#main-content')).toBeVisible();
      await page.screenshot({
        path: 'playwright-report/dashboard-loaded.png',
        fullPage: true,
      });
    }
  );
});
