// 18-new-session.spec.ts — Agents lens "New Session" → live terminal.
//
// Mocks `POST /api/v1/repos/{id}/sessions` (a separate backend workstream; see
// fixtures/mocks.ts `mockCreateSession`) and asserts the "New Session" button
// POSTs a session, deep-links to the returned run's terminal route, and mounts
// the live `<AgentTerminal>` on it.
//
// The WebSocket is blocked here (the assertion is that the terminal mounts, not
// that it streams); the terminal-streaming behavior itself is covered by
// 16-agent-terminal.

import { expect, test, type Page } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import {
  mockBootstrap,
  mockCreateSession,
  mockRepoAgentRuns,
  mockRepoList,
} from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' } as const;
const REPO_PATH = `/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}`;

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
  // Start with an empty fleet so the only way to a terminal is the button.
  await mockRepoAgentRuns(page, []);
}

test('New Session POSTs a session and opens the run terminal', async ({
  page,
}) => {
  await seed(page);
  await mockCreateSession(page, { run_id: 'run-new', branch: 'agent/run-new' });

  // Capture the create-session POST so we can assert it actually fired.
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

  // The terminal mounts on the freshly created run…
  const terminal = page.getByTestId('agent-terminal');
  await expect(terminal).toBeVisible();
  await expect(terminal).toHaveAttribute('data-run-id', 'run-new');
  await expect(page.getByTestId('agent-terminal-toolbar')).toBeVisible();
  // …and the URL deep-links to that run.
  await expect(page).toHaveURL(/\/agents\/run-new$/);

  // Rendered UX-QA evidence: capture the mounted terminal's accessibility tree
  // (locator.ariaSnapshot) and a screenshot (locator.screenshot) so the
  // session surface has a layered rendered receipt, not just DOM assertions.
  const ariaTree = await terminal.ariaSnapshot();
  expect(ariaTree).toContain('Agent terminal for run run-new');
  await terminal.screenshot({
    path: 'test-results/ux-qa/new-session-terminal.png',
  });
});

test('New Session surfaces an error when the create fails', async ({ page }) => {
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
