// 02-repos.spec.ts — repositories list smoke (W-T-10).
//
// Phase 2/3 frontend has shipped the real `RepositoriesPage`: search input,
// filter chips, sort dropdown, view toggle (card/table), grouped family
// cards, and the 2-step `CreateRepoDialog`. The BFF returns 502
// `upstream_unavailable` without a live forge backend; the SPA-side spec mocks
// `/api/v1/repos` to a deterministic list so the cards render every run.

import { expect, test } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import { RepositoriesPage } from './pages/RepositoriesPage';
import { mockBootstrap, mockRepoList } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPOS = [
  {
    id: { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' },
    default_branch: 'main',
    description: 'JeRyu mission-control hub.',
    visibility: 'internal' as const,
    open_pull_requests: 3,
    failing_checks: 1,
  },
  {
    id: { host: 'jeryu', owner: 'neverhuman', name: 'forge' },
    default_branch: 'main',
    description: 'Web Forge SPA + BFF.',
    visibility: 'private' as const,
    open_pull_requests: 0,
    failing_checks: 0,
  },
];

test.describe('Repositories list (W-T-10)', () => {
  test('BFF /api/v1/repos surface responds (200 / 404 / 502) @bff', async ({
    request,
  }) => {
    // Phase 2/3: `/api/v1/repos` is wired but the local API has no forge backend,
    // so 502 `upstream_unavailable` is the canonical no-creds response. The
    // 200 / 404 alternatives are accepted so the spec stays green when the
    // CI environment configures a working upstream or seeded mock profile.
    const res = await request.get('/api/v1/repos', { failOnStatusCode: false });
    const accepted = [200, 404, 502, 503];
    expect(
      accepted,
      `/api/v1/repos returned ${res.status()} (must be one of ${accepted.join(',')})`
    ).toContain(res.status());

    if (res.status() === 200) {
      const body = await res.json();
      const ok = Array.isArray(body) || Array.isArray(body?.repositories);
      expect(
        ok,
        'list envelope must be an array or { repositories: [...] }'
      ).toBe(true);
    }
  });

  test('SPA renders mocked list, filters/sorts/toggles view, navigates, opens Create dialog @action:repos.filter @action:repos.sort @action:repos.view_toggle @action:repos.open_repo @action:repos.create_dialog', async ({
    page,
  }) => {
    await mockBootstrap(page);
    await mockRepoList(page, REPOS);
    // Mock the overview-page resolver (which re-fetches /repos with a host
    // filter) by reusing the same list — the route-mock matches on the
    // base URL pattern regardless of query string.

    const shell = new AppShellPage(page);
    const repos = new RepositoriesPage(page);

    await repos.goto();
    await shell.assertShellLoaded();

    // 1. List renders both repository cards.
    const cards = page.locator('a.repo-card');
    await expect(cards).toHaveCount(REPOS.length, { timeout: 10_000 });
    await expect(cards.first()).toContainText('jeryu');

    // 2. Toolbar actions: search/filter/sort and table/card view toggles.
    await page.getByLabel('Search repositories').fill('forge');
    await expect(page.getByLabel('Search repositories')).toHaveValue('forge');
    await page.getByLabel('Visibility: private').click();
    await expect(page.getByLabel('Visibility: private')).toHaveAttribute(
      'aria-pressed',
      'true'
    );
    await page.getByLabel('Sort repositories').selectOption('name');
    await expect(page.getByLabel('Sort repositories')).toHaveValue('name');
    await page.getByRole('radio', { name: 'Table view' }).click();
    await expect(page.getByRole('grid', { name: 'Repositories' })).toBeVisible();
    await page.getByRole('radio', { name: 'Card view' }).click();
    await expect(cards.first()).toBeVisible({ timeout: 10_000 });

    // 3. Click a repo card and assert the SPA transition COMMITS: the URL
    //    changes AND the overview outlet renders (regression net for the
    //    keyboard-registry re-render loop that kept interrupting router
    //    transitions, leaving the old route on screen after pushState).
    await cards
      .filter({ hasText: 'jeryu' })
      .first()
      .click();

    await expect(page).toHaveURL(/\/repos\/jeryu\/neverhuman\/jeryu/, {
      timeout: 10_000,
    });
    await expect(
      page.getByRole('heading', { level: 1, name: 'jeryu' })
    ).toBeVisible({ timeout: 10_000 });

    // 4. Return to the list and open the Create repo dialog.
    await page.goto('/repos');
    await expect(cards.first()).toBeVisible({ timeout: 10_000 });

    const createButton = page.getByRole('button', {
      name: /create repository/i,
    });
    await expect(createButton).toBeVisible();
    await createButton.click();

    // The dialog mounts as a role="dialog" panel — assert it appears.
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible({ timeout: 5_000 });

    await page.screenshot({
      path: 'playwright-report/repos-page.png',
      fullPage: true,
    });
  });
});
