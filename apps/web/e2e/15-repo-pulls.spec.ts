// 15-repo-pulls.spec.ts - W-FE-11 repo-scoped pull request list.

import { expect, test, type Page } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import { mockBootstrap, mockPullRequestList, mockRepoList } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' } as const;

async function blockWebSocket(page: Page): Promise<void> {
  await page.context().route('**/api/v1/ws', (route) =>
    route.abort('failed').catch(() => undefined)
  );
}

test('repo pull list renders PR cards without the W-FE-11 placeholder', async ({
  page,
}) => {
  await blockWebSocket(page);
  await mockBootstrap(page);
  await mockRepoList(page, [
    {
      id: REPO,
      default_branch: 'main',
      description: 'Dogfood repo',
      visibility: 'public',
      open_pull_requests: 1,
    },
  ]);
  await mockPullRequestList(page, [
    {
      number: '31',
      title: 'Wire repo pull list',
      state: 'open',
      head_sha: 'abc123456789',
      approvals: 0,
      approvals_required: 1,
    },
  ]);

  const shell = new AppShellPage(page);
  await shell.goto(`/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}/pulls`);
  await shell.assertShellLoaded();

  await expect(page.getByTestId('repo-pulls-page')).toBeVisible();
  await expect(page.getByText('Wire repo pull list')).toBeVisible();
  await expect(page.getByText(/W-FE-11/i)).toHaveCount(0);

  await page.screenshot({
    path: 'playwright-report/repo-pulls-page.png',
    fullPage: true,
  });
});

test('repo pull list empty state says No pull requests', async ({ page }) => {
  await blockWebSocket(page);
  await mockBootstrap(page);
  await mockRepoList(page, [
    {
      id: REPO,
      default_branch: 'main',
      description: 'Dogfood repo',
      visibility: 'public',
      open_pull_requests: 0,
    },
  ]);
  await mockPullRequestList(page, []);

  const shell = new AppShellPage(page);
  await shell.goto(`/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}/pulls`);
  await shell.assertShellLoaded();

  await expect(page.getByText('No pull requests')).toBeVisible();
});
