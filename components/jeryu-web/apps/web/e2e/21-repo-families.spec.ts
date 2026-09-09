// 21-repo-families.spec.ts — repository family tiles + drill-down.
//
// The repos card view rolls repos that share `family` into one tile
// (rollup health, member count, summed activity) and clicking the tile
// drills into `/repos/family/:family`, which renders only the member
// repos inside the boxed panel. Repos without a family stay plain cards.
// The list mock honours `?family=` like the real backend, so the
// drill-down page exercises the same filter path the SPA ships.

import { expect, test, type Page, type Route } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import {
  mockBootstrap,
  mockReadme,
  mockRefs,
  mockRepoList,
  mockTree,
} from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPOS = [
  {
    id: { host: 'jeryu', owner: 'veox', name: 'redline' },
    description: 'Edge router for VEOX.',
    family: 'veox-split',
    open_pull_requests: 2,
    failing_checks: 1,
  },
  {
    id: { host: 'jeryu', owner: 'veox', name: 'bluebird' },
    description: 'Telemetry pipeline.',
    family: 'veox-split',
    open_pull_requests: 1,
    failing_checks: 0,
  },
  {
    id: { host: 'jeryu', owner: 'neverhuman', name: 'solo' },
    description: 'No family on this one.',
    open_pull_requests: 0,
    failing_checks: 0,
  },
];

test.describe('Repository families', () => {
  test('list shows one family tile + plain cards, tile drills into the family page @action:repos.family_drilldown @action:repos.split_repo_switch @action:code.file_select @action:code.readme_preview', async ({
    page,
  }) => {
    // Regression net for the keyboard-registry re-render loop: it kept
    // interrupting router transitions (pushState landed but the outlet kept
    // the previous route) and crashed with React #185 on history back. Any
    // page error during the click → back round trip fails this test.
    const pageErrors: string[] = [];
    page.on('pageerror', (error) => pageErrors.push(error.message));

    await mockBootstrap(page);
    await mockRepoList(page, REPOS);
    await mockRefs(page);
    await mockTree(page, [
      { path: 'README.md', kind: 'file' },
      { path: 'src/main.rs', kind: 'file' },
    ]);
    await mockReadme(page, {
      html: '<h1>Family README</h1><p>README rendering proof.</p>',
    });
    await mockBlob(page);

    const shell = new AppShellPage(page);
    await page.goto('/repos');
    await shell.assertShellLoaded();

    // 1. One family tile for the two veox-split repos.
    const tile = page.locator('a.repo-family-card');
    await expect(tile).toHaveCount(1, { timeout: 10_000 });
    await expect(tile).toContainText('veox');
    await expect(tile).toHaveAttribute('href', '/repos/family/veox-split');
    await expect(tile).toContainText('2 repos');

    // 2. The familyless repo renders as a plain card (no tile membership).
    const plainCards = page.locator('a.repo-card:not(.repo-family-card)');
    await expect(plainCards).toHaveCount(1);
    await expect(plainCards.first()).toContainText('solo');

    // 3. Clicking the tile commits the SPA transition: URL AND rendered
    //    outlet move to the family page (not just pushState).
    await tile.click();
    await expect(page).toHaveURL(/\/repos\/family\/veox-split/, {
      timeout: 10_000,
    });
    await expect(
      page.getByRole('heading', { level: 1, name: 'veox' })
    ).toBeVisible({ timeout: 10_000 });

    // 4. History back re-renders the repos list (this crashed with React
    //    #185 before the keyboard-registry fix) — then return forward via a
    //    fresh full load to keep asserting the family page contract.
    await page.goBack();
    await expect(page).toHaveURL(/\/repos$/, { timeout: 10_000 });
    await expect(
      page.getByRole('heading', { level: 1, name: 'Repositories' })
    ).toBeVisible({ timeout: 10_000 });
    await expect(page.locator('a.repo-family-card')).toHaveCount(1);
    expect(pageErrors).toEqual([]);
    await page.screenshot({
      path: 'playwright-report/repo-family-back-nav.png',
      fullPage: true,
    });

    // Full-load the drill-down URL and assert the page contract.
    await page.goto('/repos/family/veox-split');
    await expect(
      page.getByRole('heading', { level: 1, name: 'veox' })
    ).toBeVisible({ timeout: 10_000 });

    // 4. The split browser contains exactly the member repos (the mock
    //    honours `?family=` so non-members never reach the page).
    const browser = page.locator('section.split-browser');
    await expect(browser).toBeVisible();
    const repoButtons = browser.locator('.split-browser__repo');
    await expect(repoButtons).toHaveCount(2);
    await expect(repoButtons).toContainText(['bluebird', 'redline']);
    await expect(browser).not.toContainText('solo');

    // 5. The default preview renders the README, then repo switching keeps
    //    the browser in the same split family.
    await expect(browser.locator('.markdown-body')).toContainText(
      'README rendering proof.'
    );
    await browser.getByRole('button', { name: /redline/i }).click();
    await expect(
      browser.locator('.split-browser__title').filter({ hasText: 'veox/redline' })
    ).toBeVisible();

    // 6. Tree selection requests the blob endpoint and renders the file
    //    preview instead of leaking the previous README panel.
    await browser.getByRole('treeitem', { name: /README\.md/ }).click();
    await expect(browser.getByRole('tab', { name: 'Rendered' })).toBeVisible();
    await expect(browser.locator('.markdown-body')).toContainText(
      'Selected file proof.'
    );

    await page.screenshot({
      path: 'playwright-report/repo-family-page.png',
      fullPage: true,
    });
  });

  test('family page renders permission denied for a non-owner viewer (403 forbidden) @action:repos.family_permission_denied', async ({
    page,
  }) => {
    await mockBootstrap(page);
    // Negative authorization proof (owner/non-owner): the list endpoint
    // answers 403 forbidden for a viewer without repo.read on this family.
    await page.route('**/api/v1/repos**', async (route) => {
      await route.fulfill({
        status: 403,
        contentType: 'application/json',
        body: JSON.stringify({
          error: { code: 'permission_denied', message: 'missing repo.read' },
        }),
      });
    });

    await page.goto('/repos/family/veox-split');

    await expect(page.getByText('Permission denied')).toBeVisible({
      timeout: 10_000,
    });
    await expect(page.getByText(/missing: repo\.read/)).toBeVisible();
    // The non-owner viewer sees zero repository data.
    await expect(page.locator('a.repo-card')).toHaveCount(0);
    await expect(page.locator('section.split-browser')).toHaveCount(0);
  });

  test('searching collapses tiles into flat repo cards @action:repos.search', async ({ page }) => {
    await mockBootstrap(page);
    await mockRepoList(page, REPOS);

    await page.goto('/repos');
    await expect(page.locator('a.repo-family-card')).toHaveCount(1, {
      timeout: 10_000,
    });

    // Typing a search disables grouping — every repo renders flat. The
    // mock does not filter on `q`, so all three repos stay visible; the
    // assertion under test is the tile collapse, not the result set.
    await page.getByLabel('Search repositories').fill('red');
    await expect(page.locator('a.repo-family-card')).toHaveCount(0, {
      timeout: 10_000,
    });
    await expect(
      page.locator('a.repo-card:not(.repo-family-card)')
    ).toHaveCount(REPOS.length);
  });
});

async function mockBlob(page: Page): Promise<void> {
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/blob(\?.*)?$/,
    async (route: Route, request) => {
      if (request.method() !== 'GET') {
        await route.continue();
        return;
      }
      const url = new URL(request.url());
      const path = url.searchParams.get('path') ?? 'README.md';
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          repo: {
            id: 'jeryu:veox/redline',
            host: 'jeryu',
            owner: 'veox',
            name: 'redline',
          },
          path,
          ref_name: url.searchParams.get('ref') ?? 'main',
          sha: '0'.repeat(40),
          size_bytes: 42,
          mime: 'text/markdown',
          encoding: 'utf8',
          text: '# Selected file proof.',
          base64: null,
          rendered_markdown: {
            html: '<h1>Selected file proof.</h1>',
            toc: [],
            links: [],
            renderer_version: 'jeryu-md-renderer.v1',
            sanitizer_version: 'jeryu-md-sanitizer.v1',
            rendered_at: '2026-05-26T00:00:00Z',
          },
          is_binary: false,
        }),
      });
    }
  );
}
