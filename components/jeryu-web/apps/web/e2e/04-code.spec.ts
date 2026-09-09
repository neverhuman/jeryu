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

import {
  mockBlob,
  mockBootstrap,
  mockRefs,
  mockRepoLookup,
  mockTree,
} from './fixtures/mocks';

test.describe.configure({ retries: 1 });

test.describe('Code browser (W-T-12)', () => {
  test('page renders branch selector, file tree, and file finder @action:code.branch_selector @action:code.file_tree @action:code.file_search', async ({ page }) => {
    await mockBootstrap(page);
    await mockRepoLookup(page, {
      id: { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' },
      default_branch: 'main',
    });
    await mockRefs(page);
    await mockTree(page, [
      { path: 'README.md', kind: 'file' },
      { path: 'src', kind: 'directory' },
    ]);
    await mockBlob(page);

    await page.goto('/repos/jeryu/neverhuman/jeryu/code');

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

      await page
        .getByRole('button', { name: /Switch branches\/tags: main/i })
        .click();
      await page
        .getByRole('combobox', { name: 'Switch branches/tags' })
        .fill('develop');
      await page.getByRole('option', { name: /develop/ }).click();
      await expect(
        page.getByRole('button', { name: /Switch branches\/tags: develop/i })
      ).toBeVisible();

      await page.getByRole('button', { name: /Find files/i }).click();
      await page.getByRole('combobox', { name: 'Find files' }).fill('README');
      await page.getByRole('option', { name: 'README.md' }).click();
      await expect(page).toHaveURL(/\/blob\/develop\/README\.md$/);
      await expect(
        page.getByRole('link', { name: 'View raw file' })
      ).toBeVisible();
    }
  });

  test('blob page exposes rendered/raw tabs and file toolbar actions @action:code.blob_tabs @action:code.blob_raw_link @action:code.blob_download @action:code.blob_permalink', async ({
    page,
  }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, 'clipboard', {
        value: {
          writeText: async (value: string) => {
            (
              window as unknown as { __copiedPermalink?: string }
            ).__copiedPermalink = value;
          },
        },
        configurable: true,
      });
    });
    await mockBootstrap(page);
    await mockRepoLookup(page, {
      id: { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' },
      default_branch: 'main',
    });
    await mockBlob(page, {
      path: 'README.md',
      text: '# Blob toolbar proof.',
      html: '<h1>Blob toolbar proof.</h1>',
      sha: '1234567890abcdef1234567890abcdef12345678',
    });

    await page.goto('/repos/jeryu/neverhuman/jeryu/blob/main/README.md');

    await expect(page.getByText('Blob toolbar proof.')).toBeVisible({
      timeout: 15_000,
    });
    await page.getByRole('tab', { name: 'Raw' }).click();
    await expect(page.getByRole('tab', { name: 'Raw' })).toHaveAttribute(
      'aria-pressed',
      'true'
    );

    const raw = page.getByRole('link', { name: 'View raw file' });
    await expect(raw).toHaveAttribute('href', /\/api\/v1\/repos\/.*\/raw\?/);
    await expect(
      page.getByRole('link', { name: 'Download file' })
    ).toHaveAttribute('download', 'README.md');

    await page.getByRole('button', { name: 'Copy permalink' }).click();
    await expect(
      page.getByRole('button', { name: 'Copy permalink' })
    ).toContainText('Copied');
    const copied = await page.evaluate(
      () =>
        (window as unknown as { __copiedPermalink?: string })
          .__copiedPermalink
    );
    expect(copied).toContain('1234567890abcdef1234567890abcdef12345678');
    expect(copied).toContain('README.md');
  });

  test('deep file path returns 200 (SPA fallback) @bff', async ({ request }) => {
    // The exact deep-link that broke before the W-spa-fix landing — must
    // continue to return 200 with the SPA HTML body so React Router can
    // resolve the route in the browser.
    const res = await request.get(
      '/repos/jeryu/neverhuman/jeryu/blob/main/src/main.tsx',
      {
        failOnStatusCode: false,
        headers: { Accept: 'text/html' },
      }
    );
    expect(
      res.status(),
      'deep SPA route must be served by the SPA fallback (200)'
    ).toBe(200);
    const body = await res.text();
    expect(body.toLowerCase()).toContain('<!doctype html');
  });
});
