// 21-repo-families.spec.ts — repository family tiles + drill-down.
//
// The repos card view rolls repos that share `family` into one tile
// (rollup health, member count, summed activity) and clicking the tile
// drills into `/repos/family/:family`, which renders only the member
// repos inside the boxed panel. Repos without a family stay plain cards.
// The list mock honours `?family=` like the real backend, so the
// drill-down page exercises the same filter path the SPA ships.

import { expect, test } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import { mockBootstrap, mockRepoList } from './fixtures/mocks';

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
  test('list shows one family tile + plain cards, tile drills into the family page', async ({
    page,
  }) => {
    await mockBootstrap(page);
    await mockRepoList(page, REPOS);

    const shell = new AppShellPage(page);
    await page.goto('/repos');
    await shell.assertShellLoaded();

    // 1. One family tile for the two veox-split repos.
    const tile = page.locator('a.repo-family-card');
    await expect(tile).toHaveCount(1, { timeout: 10_000 });
    await expect(tile).toContainText('veox-split');
    await expect(tile).toContainText('2 repos');

    // 2. The familyless repo renders as a plain card (no tile membership).
    const plainCards = page.locator('a.repo-card:not(.repo-family-card)');
    await expect(plainCards).toHaveCount(1);
    await expect(plainCards.first()).toContainText('solo');

    // 3. Clicking the tile routes to the family drill-down URL. (Like
    //    02-repos, only the URL is asserted post-click: the production
    //    bundle has a pre-existing SPA-transition gap where pushState
    //    lands but the outlet keeps the previous route — repro: clicking
    //    any repo card on /repos against main's bundle. The full-load
    //    path below proves the family page itself.)
    await tile.click();
    await expect(page).toHaveURL(/\/repos\/family\/veox-split/, {
      timeout: 10_000,
    });

    // Full-load the drill-down URL and assert the page contract.
    await page.goto('/repos/family/veox-split');
    await expect(
      page.getByRole('heading', { level: 1, name: 'veox-split' })
    ).toBeVisible({ timeout: 10_000 });

    // 4. The boxed panel contains exactly the member repos (the mock
    //    honours `?family=` so non-members never reach the page).
    const panel = page.locator('section.repo-family-panel');
    await expect(panel).toBeVisible();
    await expect(panel.locator('a.repo-card')).toHaveCount(2);
    await expect(panel).toContainText('redline');
    await expect(panel).toContainText('bluebird');
    await expect(panel).not.toContainText('solo');

    await page.screenshot({
      path: 'playwright-report/repo-family-page.png',
      fullPage: true,
    });
  });

  test('family page renders permission denied for a non-owner viewer (403 forbidden)', async ({
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
    await expect(page.locator('section.repo-family-panel')).toHaveCount(0);
  });

  test('searching collapses tiles into flat repo cards', async ({ page }) => {
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
