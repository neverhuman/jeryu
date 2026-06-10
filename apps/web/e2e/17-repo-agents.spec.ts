// 17-repo-agents.spec.ts — per-repo Active Agents list → terminal.
//
// Mocks `GET /api/v1/repos/{id}/agent-runs` and asserts the active-agents list
// renders branch / runner / status / tty-live for each run, and that clicking a
// row navigates to the run route and mounts the live `<AgentTerminal>`.
//
// The terminal-streaming behavior itself is covered by 16-agent-terminal.

import { expect, test, type Page } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import {
  mockBootstrap,
  mockRepoAgentRuns,
  mockRepoList,
  mockTtyStream,
  mockAgentControl,
  mockCompanionShell,
} from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' } as const;
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
    { id: REPO, default_branch: 'main', visibility: 'public' },
  ]);
  await mockRepoAgentRuns(page, [
    {
      run_id: 'run-1',
      branch: 'fix/parser',
      runner: 'runnerd-xbabe0',
      status: 'running',
      tty_live: true,
      agent: 'editbot',
      shell_run_id: 'shell-run-1',
    },
    {
      run_id: 'run-2',
      branch: 'feat/cache',
      runner: 'local',
      status: 'blocked',
      tty_live: false,
      agent: 'planner',
      shell_run_id: 'shell-run-2',
    },
  ]);
  // Mock SSE + control so the terminal component doesn't hit the real backend.
  await mockTtyStream(page);
  await mockAgentControl(page);
  // Mock the companion shell endpoint so the split-terminal logic doesn't fail.
  await mockCompanionShell(page);
}

test('renders the active-agents list with branch / runner / status / tty-live', async ({
  page,
}) => {
  await seed(page);

  const shell = new AppShellPage(page);
  await shell.goto(`${REPO_PATH}/agents`);
  await shell.assertShellLoaded();

  await expect(page.getByTestId('repo-agents-page')).toBeVisible();
  const list = page.getByTestId('agents-list');
  await expect(list).toBeVisible();

  // Row 1: running + live TTY.
  const row1 = page.getByTestId('agent-row-run-1');
  await expect(row1).toContainText('fix/parser');
  await expect(row1).toContainText('runnerd-xbabe0');
  await expect(row1).toContainText('editbot');
  await expect(page.getByTestId('agent-status-run-1')).toHaveText('running');
  await expect(page.getByTestId('agent-tty-run-1')).toHaveClass(/is-live/);
  await expect(page.getByTestId('agent-tty-run-1')).toContainText('live');

  // Row 2: blocked, not live.
  const row2 = page.getByTestId('agent-row-run-2');
  await expect(row2).toContainText('feat/cache');
  await expect(page.getByTestId('agent-status-run-2')).toHaveText('blocked');
  await expect(page.getByTestId('agent-tty-run-2')).not.toHaveClass(/is-live/);

  // No run selected yet → the terminal pane shows the empty prompt.
  await expect(page.getByTestId('agents-no-selection')).toBeVisible();
});

test('clicking a run row opens its live terminal', async ({ page }) => {
  await seed(page);

  const shell = new AppShellPage(page);
  await shell.goto(`${REPO_PATH}/agents`);
  await shell.assertShellLoaded();

  await page.getByTestId('agent-row-run-1').click();

  const terminal = page.getByTestId('agent-terminal').first();
  await expect(terminal).toBeVisible();
  await expect(terminal).toHaveAttribute('data-run-id', 'run-1');
  await expect(page.getByTestId('agent-row-run-1')).toHaveAttribute(
    'aria-pressed',
    'true'
  );
  await expect(page.getByTestId('agent-terminal-toolbar').first()).toBeVisible();
  await expect(page.getByTestId('agent-terminal-interrupt').first()).toBeVisible();
});

test('deep-links straight to a run terminal via the splat tail', async ({
  page,
}) => {
  await seed(page);

  const shell = new AppShellPage(page);
  await shell.goto(`${REPO_PATH}/agents/run-2`);
  await shell.assertShellLoaded();

  const terminal = page.getByTestId('agent-terminal').first();
  await expect(terminal).toBeVisible();
  await expect(terminal).toHaveAttribute('data-run-id', 'run-2');
});
