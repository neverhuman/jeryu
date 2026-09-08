// 20-repo-routing.spec.ts — Verify repo sub-page routing works for all paths.
//
// The core issue: repo URLs have 3 segments after /repos/ (provider/owner/name)
// but the router uses a splat route. This test verifies that every sub-page
// (agents, code, pulls, settings) resolves correctly for the URL pattern.

import { expect, test, type Page } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import {
  mockBootstrap,
  mockRepoAgentRuns,
  mockRepoList,
} from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = { host: 'jeryu', owner: 'jeryu', name: 'jankurai' } as const;
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
      active_agents: 2,
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
  ]);
}

test.describe('Repository sub-page routing', () => {
  test('navigating to /repos/jeryu/jeryu/jankurai shows the repo overview @action:repo.overview', async ({
    page,
  }) => {
    await seed(page);
    const shell = new AppShellPage(page);
    await shell.goto(REPO_PATH);
    await shell.assertShellLoaded();

    // The repo overview should render (not "Repository not found").
    await expect(page.getByTestId('repo-overview-page')).toBeVisible({ timeout: 10000 });
    await expect(page.getByText(/Repository not found/i)).toHaveCount(0);
  });

  test('navigating to /repos/jeryu/jeryu/jankurai/agents shows the agents page @action:repo.route_agents', async ({
    page,
  }) => {
    await seed(page);
    const shell = new AppShellPage(page);
    await shell.goto(`${REPO_PATH}/agents`);
    await shell.assertShellLoaded();

    // The agents page should render.
    await expect(page.getByTestId('repo-agents-page')).toBeVisible({ timeout: 10000 });
    await expect(page.getByText(/Repository not found/i)).toHaveCount(0);
  });

  test('navigating to /repos/jeryu/jeryu/jankurai/agents/run-1 opens a specific agent run @action:repo.route_agent_run', async ({
    page,
  }) => {
    await seed(page);
    const shell = new AppShellPage(page);
    await shell.goto(`${REPO_PATH}/agents/run-1`);
    await shell.assertShellLoaded();

    // The agents page should render with the run selected.
    await expect(page.getByTestId('repo-agents-page')).toBeVisible({ timeout: 10000 });
    // The agent terminal should mount for run-1.
    await expect(page.getByTestId('agent-terminal')).toBeVisible({ timeout: 10000 });
    await expect(page.getByTestId('agent-terminal')).toHaveAttribute('data-run-id', 'run-1');
  });

  test('navigating to /repos/jeryu/jeryu/jankurai/code shows the code browser @action:repo.route_code', async ({
    page,
  }) => {
    await seed(page);
    const shell = new AppShellPage(page);
    await shell.goto(`${REPO_PATH}/code`);
    await shell.assertShellLoaded();

    await expect(page.getByTestId('repo-code-page')).toBeVisible({ timeout: 10000 });
    await expect(page.getByText(/Repository not found/i)).toHaveCount(0);
  });

  test('navigating to /repos/jeryu/jeryu/jankurai/pulls shows pull requests @action:repo.route_pulls', async ({
    page,
  }) => {
    await seed(page);
    const shell = new AppShellPage(page);
    await shell.goto(`${REPO_PATH}/pulls`);
    await shell.assertShellLoaded();

    await expect(page.getByTestId('repo-pulls-page')).toBeVisible({ timeout: 10000 });
    await expect(page.getByText(/Repository not found/i)).toHaveCount(0);
  });

  test('navigating to /repos/jeryu/jeryu/jankurai/settings shows settings @action:repo.route_settings', async ({
    page,
  }) => {
    await seed(page);
    const shell = new AppShellPage(page);
    await shell.goto(`${REPO_PATH}/settings`);
    await shell.assertShellLoaded();

    await expect(page.getByTestId('repo-settings-page')).toBeVisible({ timeout: 10000 });
    await expect(page.getByText(/Repository not found/i)).toHaveCount(0);
  });

  test('left-nav shows repo context (Code, Agents, Pulls, Settings) inside a repo @action:repo.context_nav', async ({
    page,
  }) => {
    await seed(page);
    const shell = new AppShellPage(page);
    await shell.goto(REPO_PATH);
    await shell.assertShellLoaded();

    // The left nav should show the repo-context navigation.
    const agentsLink = page.getByTestId('left-nav-agents');
    await expect(agentsLink).toBeVisible();
    // The agents link should point to the correct URL.
    await expect(agentsLink).toHaveAttribute('href', `${REPO_PATH}/agents`);
  });

  test('clicking Agents quick-action on repo card navigates to agents page @action:repo.agents_quick_nav', async ({
    page,
  }) => {
    await seed(page);
    const shell = new AppShellPage(page);
    await shell.goto('/repos');
    await shell.assertShellLoaded();

    // Find and click the agents link on the repo card.
    const agentsLink = page.getByTestId(`repo-agents-link-${REPO.owner}-${REPO.name}`);
    await expect(agentsLink).toBeVisible();
    await agentsLink.click();

    // Should navigate to the agents page.
    await expect(page).toHaveURL(new RegExp(`${REPO_PATH}/agents$`));
    await expect(page.getByTestId('repo-agents-page')).toBeVisible({ timeout: 10000 });
  });
});
