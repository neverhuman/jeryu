// 24-work-tracker.spec.ts — Work Tracker route smoke.
//
// The Work pages are routed through both split-wide and repo-scoped URLs.
// This spec mocks the Work REST payloads at the browser boundary so it locks
// the SPA route/render contract without depending on a seeded local forge.

import { expect, test, type Page, type Route } from '@playwright/test';

import { mockBootstrap, mockRepoList } from './fixtures/mocks';
import { AppShellPage } from './pages/AppShellPage';

const REPO = { host: 'jeryu', owner: 'alice', name: 'jeryu' };
const REPO_ID = `${REPO.host}:${REPO.owner}/${REPO.name}`;

test.describe('Work Tracker routes', () => {
  test.beforeEach(async ({ page }) => {
    await page.context().route('**/api/v1/ws', (route) => route.abort());
    await mockBootstrap(page);
    await mockRepoList(page, [{ id: REPO, default_branch: 'main' }]);
    await mockWorkApi(page);
  });

  test('smokes split-wide, detail, repo Work, and repo issues alias routes @action:work.routes @action:work.repo_alias', async ({
    page,
  }) => {
    const shell = new AppShellPage(page);

    await page.goto('/work');
    await shell.assertShellLoaded();
    await expect(page.getByRole('heading', { level: 1, name: 'Work' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'Fix cache key' })).toBeVisible();
    await expect(page.getByRole('link', { name: '#42' })).toHaveAttribute(
      'href',
      '/repos/jeryu/alice/jeryu/issues#42'
    );

    await page.goto('/work/JRY-1');
    await expect(page.getByTestId('work-detail-page')).toBeVisible();
    await expect(page.getByRole('heading', { level: 1, name: 'JRY-1' })).toBeVisible();
    await expect(page.getByText('Existing Work note')).toBeVisible();
    await expect(page.getByRole('link', { name: 'alice/jeryu#7' })).toHaveAttribute(
      'href',
      '/repos/jeryu/alice/jeryu/pulls/7'
    );

    await clientNavigate(page, '/repos/jeryu/alice/jeryu/work');
    await expect(page.getByTestId('work-page')).toBeVisible();
    await expect(page.getByText('alice/jeryu work items.')).toBeVisible();
    await expect(page.getByRole('link', { name: 'Repo scoped follow-up' })).toBeVisible();

    await clientNavigate(page, '/repos/jeryu/alice/jeryu/issues');
    await expect(page.getByTestId('work-page')).toBeVisible();
    await expect(page.getByText('alice/jeryu issue-compatible work items.')).toBeVisible();
    await expect(page.getByRole('link', { name: 'Repo scoped follow-up' })).toBeVisible();
  });

  test('filters, creates, edits, comments, links, and surfaces errors @action:work.filters @action:work.create @action:work.detail_save @action:work.comment @action:work.link_pull @action:work.error', async ({
    page,
  }) => {
    const createBodies: unknown[] = [];
    const patchBodies: unknown[] = [];
    const commentBodies: unknown[] = [];
    const linkBodies: unknown[] = [];

    await page.route('**/api/v1/work', async (route: Route, request) => {
      if (request.method() !== 'POST') {
        await route.fallback();
        return;
      }
      const body = request.postDataJSON() as { title?: string };
      createBodies.push(body);
      if (body.title?.includes('Fail')) {
        await route.fulfill({
          status: 503,
          contentType: 'application/json',
          body: JSON.stringify({
            error: {
              code: 'work_create_failed',
              message: 'Work backend failed.',
            },
          }),
        });
        return;
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(workItem({ title: body.title })),
      });
    });
    await page.route('**/api/v1/work/JRY-1', async (route: Route, request) => {
      if (request.method() !== 'PATCH') {
        await route.fallback();
        return;
      }
      const body = request.postDataJSON() as Record<string, unknown>;
      patchBodies.push(body);
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(workItem(body)),
      });
    });
    await page.route(
      '**/api/v1/work/JRY-1/comments',
      async (route: Route, request) => {
        if (request.method() !== 'POST') {
          await route.fallback();
          return;
        }
        const body = request.postDataJSON() as { body: string };
        commentBodies.push(body);
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            id: 'comment-2',
            work_key: 'JRY-1',
            author: { kind: 'human', id: 'alice', display_name: null },
            body: body.body,
            created_at: '2026-07-02T01:00:00Z',
          }),
        });
      }
    );
    await page.route(
      '**/api/v1/work/JRY-1/links',
      async (route: Route, request) => {
        if (request.method() !== 'POST') {
          await route.fallback();
          return;
        }
        const body = request.postDataJSON();
        linkBodies.push(body);
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify(workItem()),
        });
      }
    );

    const shell = new AppShellPage(page);
    await page.goto('/work');
    await shell.assertShellLoaded();

    const createRegion = page.getByRole('region', { name: 'Create work item' });
    const titleInput = createRegion.getByLabel('Title');
    const createButton = createRegion.getByRole('button', { name: 'Create' });

    const filters = page.locator('section[aria-label="Work filters"]');
    await filters.getByLabel('Status').selectOption('ready');
    await filters.getByLabel('Priority').selectOption('p1');
    await page.getByLabel('Search work').fill('cache');
    await expect(page.getByRole('link', { name: 'Fix cache key' })).toBeVisible();

    await titleInput.fill('New tracked action');
    await createButton.click();
    await expect
      .poll(() => createBodies.length, { timeout: 10_000 })
      .toBeGreaterThanOrEqual(1);
    expect(createBodies[0]).toMatchObject({
      title: 'New tracked action',
      status: 'ready',
      kind: 'task',
      priority: 'p2',
    });

    await expect(titleInput).toHaveValue('');
    await titleInput.fill('Fail tracked action');
    await createButton.click();
    await expect(page.getByText('Work backend failed.')).toBeVisible({
      timeout: 10_000,
    });

    await page.goto('/work/JRY-1');
    await expect(page.getByTestId('work-detail-page')).toBeVisible();
    await page.getByLabel('Title').fill('Fix cache key with saved title');
    await page.getByRole('button', { name: 'Save' }).click();
    await expect.poll(() => patchBodies.length, { timeout: 10_000 }).toBe(1);
    expect(patchBodies[0]).toMatchObject({
      title: 'Fix cache key with saved title',
    });

    await page.getByLabel('Comment body').fill('New browser comment');
    await page.getByRole('button', { name: 'Comment' }).click();
    await expect.poll(() => commentBodies.length, { timeout: 10_000 }).toBe(1);
    expect(commentBodies[0]).toMatchObject({ body: 'New browser comment' });

    await page.getByLabel('Pull request owner').fill('alice');
    await page.getByLabel('Pull request repo').fill('jeryu');
    await page.getByLabel('Pull request number').fill('9');
    await page.getByRole('button', { name: 'Link' }).click();
    await expect.poll(() => linkBodies.length, { timeout: 10_000 }).toBe(1);
    expect(linkBodies[0]).toMatchObject({
      issue: null,
      pull_request: { owner: 'alice', repo: 'jeryu', number: 9 },
    });
  });
});

async function clientNavigate(page: Page, path: string): Promise<void> {
  await page.evaluate((nextPath) => {
    window.history.pushState({}, '', nextPath);
    window.dispatchEvent(new PopStateEvent('popstate'));
  }, path);
}

async function mockWorkApi(page: Page): Promise<void> {
  await page.route('**/api/v1/work', async (route: Route, request) => {
    if (request.method() !== 'GET') {
      await route.fallback();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ total: 1, items: [workItem()] }),
    });
  });

  await page.route('**/api/v1/work/JRY-1', async (route: Route, request) => {
    if (request.method() !== 'GET') {
      await route.fallback();
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        item: workItem(),
        comments: [
          {
            id: 'comment-1',
            work_key: 'JRY-1',
            author: { kind: 'human', id: 'alice', display_name: null },
            body: 'Existing Work note',
            created_at: '2026-07-02T00:30:00Z',
          },
        ],
      }),
    });
  });

  await page.route(
    /\/api\/v1\/repos\/[^/]+\/work$/,
    async (route: Route, request) => {
      if (request.method() !== 'GET') {
        await route.fallback();
        return;
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          total: 1,
          items: [workItem({ title: 'Repo scoped follow-up' })],
        }),
      });
    }
  );
}

function workItem(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: '7a733f01-5f6f-41b5-9f1c-b535a4b3d681',
    key: 'JRY-1',
    number: 1,
    repo: { id: REPO_ID, ...REPO },
    title: 'Fix cache key',
    body: 'Cache key drifts across split repos.',
    status: 'ready',
    kind: 'bug',
    priority: 'p1',
    labels: ['cache'],
    assignees: [{ kind: 'human', id: 'alice', display_name: null }],
    issue: {
      owner: REPO.owner,
      repo: REPO.name,
      number: 42,
      url: '/repos/jeryu/alice/jeryu/issues#42',
    },
    pull_requests: [
      {
        owner: REPO.owner,
        repo: REPO.name,
        number: 7,
        url: '/repos/jeryu/alice/jeryu/pulls/7',
      },
    ],
    created_at: '2026-07-02T00:00:00Z',
    updated_at: '2026-07-02T00:00:00Z',
    ...overrides,
  };
}
