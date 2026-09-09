// 18-new-session.spec.ts — Agents lens "New Session" → live terminal.
//
// Mocks `POST /api/v1/repos/{id}/sessions` and asserts the "New Session" button
// POSTs a session, deep-links to the returned run's terminal route, and mounts
// the live `<AgentTerminal>` on it.
//
// The terminal-streaming behavior itself is covered by 16-agent-terminal.

import { expect, test, type Page } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import {
  mockBootstrap,
  mockCreateSession,
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
  await mockRepoAgentRuns(page, []);
  // Mock SSE + control so the terminal component doesn't hit the real backend.
  await mockTtyStream(page);
  await mockAgentControl(page);
  await mockCompanionShell(page);
}

test('New Session POSTs a session and opens the run terminal @action:agents.new_session_success', async ({
  page,
}) => {
  await seed(page);
  await mockCreateSession(page, { run_id: 'run-new', branch: 'agent/run-new' });

  const createRequest = page.waitForRequest(
    (req) =>
      req.method() === 'POST' && /\/api\/v1\/repos\/[^/]+\/sessions$/.test(req.url())
  );

  const shell = new AppShellPage(page);
  await shell.goto(`${REPO_PATH}/agents`);
  await shell.assertShellLoaded();

  await expect(page.getByTestId('repo-agents-page')).toBeVisible();

  const button = page.getByTestId('new-session-button');
  await expect(button).toBeEnabled();
  await button.click();
  await createRequest;

  const terminal = page.getByTestId('agent-terminal').first();
  await expect(terminal).toBeVisible();
  await expect(terminal).toHaveAttribute('data-run-id', 'run-new');
  await expect(page.getByTestId('agent-terminal-toolbar').first()).toBeVisible();
  await expect(page).toHaveURL(/\/agents\/run-new$/);

  const ariaTree = await terminal.ariaSnapshot();
  expect(ariaTree).toContain('Agent terminal for run run-new');
  await terminal.screenshot({
    path: 'test-results/ux-qa/new-session-terminal.png',
  });
});

test('New Session surfaces an error when the create fails @action:agents.new_session_error', async ({ page }) => {
  await seed(page);
  await page.route(
    /\/api\/v1\/repos\/[^/]+\/sessions(\?.*)?$/,
    async (route, request) => {
      if (request.method() !== 'POST') {
        await route.continue();
        return;
      }
      await route.fulfill({
        status: 503,
        contentType: 'application/json',
        body: JSON.stringify({
          error: { code: 'capacity_exhausted', message: 'No runner slots free.' },
        }),
      });
    }
  );

  const shell = new AppShellPage(page);
  await shell.goto(`${REPO_PATH}/agents`);
  await shell.assertShellLoaded();

  await page.getByTestId('new-session-button').click();

  const alert = page.getByTestId('new-session-error');
  await expect(alert).toBeVisible();
  await expect(alert).toContainText('No runner slots free.');
  await expect(page.getByTestId('agent-terminal')).toHaveCount(0);
});
