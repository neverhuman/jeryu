// 02-repos.spec.ts — repositories list smoke (W-T-10).
//
// Phase 2/3 frontend has shipped the real `RepositoriesPage`: search input,
// filter chips, sort dropdown, view toggle (card/table), grouped family
// cards, and the 2-step `CreateRepoDialog`. The BFF returns 502
// `upstream_unavailable` without a live forge backend; the SPA-side spec mocks
// `/api/v1/repos` to a deterministic list so the cards render every run.

import { expect, test } from '@playwright/test';
import { loginBff } from './bff-auth';

import { AppShellPage } from './pages/AppShellPage';
import { RepositoriesPage } from './pages/RepositoriesPage';
import { mockBootstrap, mockRepoList } from './fixtures/mocks';
import type { CreateRepositoryPreview, CreateRepositoryRequest } from '../src/api/types';

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
  test.describe('Creation recovery', () => {
    test.describe.configure({ retries: 0 });
    test('creation retry keeps its request identity after a server interruption @action:repos.create_dialog', async ({ page }, testInfo) => {
      await mockBootstrap(page);
      await mockRepoList(page, REPOS);
      await page.route('**/api/v1/repos/preview', async (route) => {
        const request = route.request().postDataJSON() as CreateRepositoryRequest;
        const preview: CreateRepositoryPreview = {
          normalized_name: request.name,
          target_owner: request.owner,
          visibility: request.visibility,
          initial_files: ['README.md'],
          settings_to_apply: ['Default branch: main'],
          side_effects: ['Create a repository'],
          warnings: [],
        };
        await route.fulfill({ json: preview });
      });
      const attempts: { key: string | undefined; body: string | null }[] = [];
      await page.route('**/api/v1/repos', async (route) => {
        if (route.request().method() !== 'POST') {
          await route.fallback();
          return;
        }
        attempts.push({ key: route.request().headers()['idempotency-key'], body: route.request().postData() });
        await route.fulfill({ status: 500, json: { error: { code: 'creation_failed', message: 'Creation is pending. Retry with the same settings.' } } });
      });
      await page.goto('/repos');
      await page.getByRole('button', { name: /create repository/i }).click();
      const dialog = page.getByRole('dialog');
      await dialog.getByLabel('Owner', { exact: true }).fill('neverhuman');
      await dialog.getByLabel('Name', { exact: true }).fill('recoverable');
      await dialog.getByRole('button', { name: /preview/i }).click();
      const create = dialog.getByRole('button', { name: 'Create', exact: true });
      await create.click();
      await expect(dialog).toContainText('Creation is pending. Retry with the same settings.');
      await create.click();
      await expect.poll(() => attempts.length).toBe(2);
      expect(attempts[0].key).toMatch(/^[a-zA-Z0-9-]{16,128}$/);
      expect(attempts[1]).toEqual(attempts[0]);
      await expect(create).toBeEnabled();
      await testInfo.attach('repository-creation-pending-retry', {
        body: await page.screenshot({ fullPage: true }), contentType: 'image/png',
      });
      await dialog.getByRole('button', { name: 'Back', exact: true }).click();
      await dialog.getByLabel('Name', { exact: true }).fill('another-request');
      await dialog.getByRole('button', { name: /preview/i }).click();
      await create.click();
      await expect.poll(() => attempts.length).toBe(3);
      expect(attempts[2].key).not.toBe(attempts[0].key);
    });
  });

  test('BFF /api/v1/repos returns the durable repository list @bff', async ({
    request,
  }) => {
    await loginBff(request);
    const res = await request.get('/api/v1/repos', { failOnStatusCode: false });
    expect(res.status()).toBe(200);

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
  }, testInfo) => {
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
    await expect(dialog.getByRole('option', { name: 'local (unavailable)', exact: true })).toBeDisabled();
    await expect(dialog.getByRole('option', { name: 'internal (unavailable)', exact: true })).toBeDisabled();
    await expect(dialog.getByLabel('Topics (unavailable)', { exact: true })).toBeDisabled();
    const visibility = dialog.getByLabel('Visibility', { exact: true });
    await visibility.selectOption('public');
    await expect(visibility).toHaveValue('public');
    await visibility.selectOption('private');
    await expect(visibility).toHaveValue('private');

    await testInfo.attach('repository-create-controls', {
      body: await page.screenshot({ fullPage: true }),
      contentType: 'image/png',
    });
  });
});
