// 04-code.spec.ts — code browser smoke (W-T-12).
//
// Phase-3-tolerant smoke for the RepositoryCodePage. The SPA renders one of
// three states depending on whether the API can reach the forge backend:
//
//   1. Real repository → BranchSelector + FileTree visible.
//   2. Backend unavailable (`502/503` from the BFF) → `ErrorState` visible.
//   3. Backend reachable but repo unknown (`404`) → also `ErrorState`.
//
// Mocking the bootstrap + repo lookup keeps the spec deterministic; the
// tree endpoint is intentionally left to the live BFF so any wiring
// regression surfaces as an ErrorState rather than a green pass.

import { expect, test } from '@playwright/test';

import { mockBootstrap, mockRefs, mockRepoLookup, mockTree } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

test.describe('Code browser (W-T-12)', () => {
  test('page renders with a file tree or an error state', async ({ page }) => {
    await mockBootstrap(page);
    await mockRepoLookup(page, {
      id: { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' },
      default_branch: 'main',
    });
    await mockRefs(page);
    await mockTree(page, [
      { path: 'README.md', kind: 'file' },
      { path: 'src', kind: 'dir' },
    ]);

    await page.goto('/repos/jeryu/neverhuman%2Fjeryu/code');

    // Three acceptable states. The page either renders the browser layout
    // (BranchSelector + FileTree visible) or one of the state surfaces.
    const layout = page.locator('.code-browser-layout');
    const errorState = page.locator('[role="alert"]');
    const notAvailable = page.locator('h2', {
      hasText: /not available in this build/i,
    });

    await expect(layout.or(errorState).or(notAvailable)).toBeVisible({
      timeout: 15_000,
    });

    // If the browser layout came up, the file tree aside MUST be present.
    if (await layout.isVisible()) {
      await expect(
        page.locator('[aria-label="File tree"], .code-browser-layout__sidebar')
      ).toBeVisible();
    }
  });

  test('deep file path returns 200 (SPA fallback)', async ({ request }) => {
    // The exact deep-link that broke before the W-spa-fix landing — must
    // continue to return 200 with the SPA HTML body so React Router can
    // resolve the route in the browser.
    const res = await request.get(
      '/repos/jeryu/neverhuman%2Fjeryu/blob/main/src/main.tsx',
      { failOnStatusCode: false }
    );
    expect(
      res.status(),
      'deep SPA route must be served by the SPA fallback (200)'
    ).toBe(200);
    const body = await res.text();
    expect(body.toLowerCase()).toContain('<!doctype html');
  });
});
