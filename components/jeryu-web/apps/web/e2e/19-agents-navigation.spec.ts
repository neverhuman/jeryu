// 19-agents-navigation.spec.ts — Agents page discoverability and navigation.
//
// Tests that the operator can reach the Agents page from:
// 1. The repo card "Agents" quick-action link on the Repositories page
// 2. The left-nav repo-context sub-navigation
// 3. The repo overview "Agents" button and sidebar link

import { expect, test, type Page } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import {
  mockBootstrap,
  mockRepoAgentRuns,
  mockRepoList,
} from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = { host: 'jeryu', owner: 'jeryu', name: 'veox' } as const;
const REPO_PATH = `/repos/${REPO.host}/${REPO.owner}/${REPO.name}`;

async function blockWebSocket(page: Page): Promise<void> {
  await page.context().route('**/api/v1/ws', (route) =>
    route.abort('failed').catch(() => undefined)
  );
}

async function seed(page: Page): Promise<void> {
  await blockWebSocket(page);
  await mockBootstrap(page);
  await mockRepoList(page, [
    {
      id: REPO,
      default_branch: 'main',
      visibility: 'public',
      active_agents: 3,
    },
  ]);
  await mockRepoAgentRuns(page, [
    {
      run_id: 'run-1',
      branch: 'agent/run-1',
      runner: 'xbabe0',
      status: 'running',
      tty_live: true,
    },
    {
      run_id: 'run-2',
      branch: 'agent/run-2',
      runner: 'xbabe1',
      status: 'exited',
      tty_live: false,
    },
  ]);
}

test.describe('Agents page navigation and discoverability', () => {
  test('repo card shows an "Agents" quick-action link on the Repositories page @action:agents.quick_action_visible', async ({
    page,
  }) => {
    await seed(page);

    const shell = new AppShellPage(page);
    await shell.goto('/repos');
    await shell.assertShellLoaded();

    // Find the agents link on the repo card.
    const agentsLink = page.getByTestId(`repo-agents-link-${REPO.owner}-${REPO.name}`);
    await expect(agentsLink).toBeVisible();
    await expect(agentsLink).toContainText('Agents');
    await expect(agentsLink).toContainText('3');
  });

  test('left-nav shows repo-context navigation with Agents link when inside a repo @action:agents.left_nav_visible', async ({
    page,
  }) => {
    await seed(page);

    const shell = new AppShellPage(page);
    await shell.goto(`${REPO_PATH}`);
    await shell.assertShellLoaded();

    // The left nav should now show the repo-context sub-navigation.
    const agentsNavLink = page.getByTestId('left-nav-agents');
    await expect(agentsNavLink).toBeVisible();
    await expect(agentsNavLink).toContainText('Agents');
  });

  test('left-nav Agents link navigates to the agents URL @action:agents.left_nav_navigate', async ({
    page,
  }) => {
    await seed(page);

    const shell = new AppShellPage(page);
    await shell.goto(`${REPO_PATH}`);
    await shell.assertShellLoaded();

    // Verify the Agents link in the left-nav has the correct href.
    const agentsNavLink = page.getByTestId('left-nav-agents');
    await expect(agentsNavLink).toHaveAttribute('href', /\/agents$/);

    // Click the link and verify the URL updates.
    await agentsNavLink.click();
    await expect(page).toHaveURL(/\/agents/, { timeout: 10000 });
  });
});
