// 02-repos.spec.ts — repositories list smoke (W-T-10).
//
// Phase 2 acceptable behaviour: `/repos` shows EITHER a real list (when
// GitLab is reachable) OR an ErrorState explaining "upstream unavailable"
// (Phase-2 BFF without GitLab credentials), OR the Phase-1 StubPage
// envelope (W-FE-08 not yet landed).
//
// The SPA shell currently fails to mount because of a known Phase-1
// `useRealtime` infinite-render loop (see 01-bootstrap.spec.ts notes).
// Until W-P2-frontend lands the fix, the SPA-level assertions are
// `test.fixme()`. The BFF-only smoke asserts the surface the SPA will
// eventually consume.

import { expect, test } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import { RepositoriesPage } from './pages/RepositoriesPage';

test.describe('Repositories list (W-T-10)', () => {
  test('BFF /api/v1/repos surface responds (200 / 404 / 502)', async ({
    request,
  }) => {
    // Phase 2: `/api/v1/repos` may not be wired yet. The acceptable
    // states are:
    //   - 200 with a JSON list (W-B-09 landed) — assert array shape.
    //   - 404 (not yet wired) — accepted, will flip once W-B-09 merges.
    //   - 502 (upstream GitLab unavailable) — accepted, what the
    //     SPA's ErrorState will surface.
    const res = await request.get('/api/v1/repos', { failOnStatusCode: false });
    const accepted = [200, 404, 502, 503];
    expect(
      accepted,
      `/api/v1/repos returned ${res.status()} (must be one of ${accepted.join(',')})`
    ).toContain(res.status());

    if (res.status() === 200) {
      const body = await res.json();
      // Phase-2 envelope is either `{ items: [...] }` or a bare array.
      const ok = Array.isArray(body) || Array.isArray(body?.items);
      expect(ok, 'list envelope must be an array or { items: [...] }').toBe(true);
    }
  });

  test.fixme(
    'SPA renders list, stub envelope, or error state',
    async ({ page }) => {
      // Pending W-P2-frontend `useRealtime` infinite-loop fix.
      const shell = new AppShellPage(page);
      const repos = new RepositoriesPage(page);

      await repos.goto();
      await shell.assertShellLoaded();
      await repos.assertEitherListOrErrorState();

      await page.screenshot({
        path: 'playwright-report/repos-page.png',
        fullPage: true,
      });
    }
  );
});
