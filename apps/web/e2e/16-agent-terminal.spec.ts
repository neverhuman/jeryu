// 16-agent-terminal.spec.ts — per-run live terminal via SSE + REST.
//
// Mocks:
//   - GET /api/v1/agent-runs/{id}/tty/stream → SSE with scripted TTY events
//   - POST /api/v1/agent-runs/{id}/control → captures control commands
//   - Standard bootstrap / repos / agent-runs mocks
//
// Asserts:
//   (a) the terminal renders SSE-streamed text;
//   (b) typing sends POST .../control with send_input body;
//   (c) Ctrl-C button and Control+C keystroke send interrupt via POST;
//   (d) a viewport resize sends resize_pty via POST;
//   (e) SSE reconnection: after closing the SSE, it reconnects and re-renders.

import { expect, test, type Page, type Route } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import { mockBootstrap, mockRepoAgentRuns, mockRepoList } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' } as const;
const RUN_ID = 'run-42';
const REPO_PATH = `/repos/${REPO.host}/${REPO.owner}/${REPO.name}`;
const AGENT_PATH = `${REPO_PATH}/agents/${RUN_ID}`;

interface CapturedControl {
  kind: string;
  text?: string;
  cols?: number;
  rows?: number;
}

function sseEvent(seq: number, stream: string, text: string): string {
  return `data: ${JSON.stringify({
    seq,
    stream,
    text,
    bytes_b64: null,
    exit_code: null,
  })}\n\n`;
}

/**
 * Install SSE mock for /api/v1/agent-runs/{id}/tty/stream.
 * Returns captured controls and an abort function to simulate SSE disconnect.
 */
async function installMocks(page: Page): Promise<{
  controls: CapturedControl[];
  abortSse: () => void;
}> {
  const controls: CapturedControl[] = [];
  let sseConnectionCount = 0;
  let pendingRoute: Route | null = null;

  // Mock the SSE stream endpoint.
  await page.route('**/api/v1/agent-runs/*/tty/stream*', async (route) => {
    sseConnectionCount += 1;
    const conn = sseConnectionCount;
    pendingRoute = route;

    // Different prompt per connection so reconnect assertions are unambiguous.
    const prompt = conn === 1 ? '$ cargo test\r\n' : '$ recovered run\r\n';
    const body = [
      sseEvent(1 + (conn - 1) * 10, 'stdout', prompt),
      sseEvent(2 + (conn - 1) * 10, 'stdout', '\x1b[32mPASS\x1b[0m all tests\r\n'),
      sseEvent(3 + (conn - 1) * 10, 'stdout', '$ '),
    ].join('');

    await route.fulfill({
      status: 200,
      contentType: 'text/event-stream',
      headers: {
        'Cache-Control': 'no-cache',
        'Connection': 'keep-alive',
      },
      body,
    });
  });

  // Mock the REST control endpoint.
  await page.route('**/api/v1/agent-runs/*/control', async (route, request) => {
    if (request.method() !== 'POST') {
      await route.continue();
      return;
    }
    try {
      const body = JSON.parse(request.postData() ?? '{}') as CapturedControl;
      controls.push(body);
    } catch {
      // ignore parse errors
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        accepted: true,
        agent_run_id: RUN_ID,
        command: 'send_input',
        control_seq: controls.length,
      }),
    });
  });

  return {
    controls,
    abortSse: () => {
      // Abort the pending SSE route to force a reconnection.
      pendingRoute?.abort().catch(() => {});
    },
  };
}

async function gotoTerminal(page: Page): Promise<AppShellPage> {
  await mockBootstrap(page);
  await mockRepoList(page, [
    { id: REPO, default_branch: 'main', visibility: 'public' },
  ]);
  await mockRepoAgentRuns(page, [
    {
      run_id: RUN_ID,
      branch: 'fix/parser',
      runner: 'runnerd-xbabe0',
      status: 'running',
      tty_live: true,
      agent: 'editbot',
    },
  ]);
  const shell = new AppShellPage(page);
  await shell.goto(AGENT_PATH);
  await shell.assertShellLoaded();
  await expect(page.getByTestId('agent-terminal')).toBeVisible();
  return shell;
}

test('renders streamed TTY text from the SSE stream', async ({ page }) => {
  await installMocks(page);
  await gotoTerminal(page);

  await expect(page.locator('.xterm-rows')).toContainText('cargo test', {
    timeout: 15_000,
  });
  await expect(page.locator('.xterm-rows')).toContainText('PASS');
});

test('typing sends input controls via REST POST', async ({ page }) => {
  const { controls } = await installMocks(page);
  await gotoTerminal(page);
  await expect(page.locator('.xterm-rows')).toContainText('cargo test', {
    timeout: 15_000,
  });

  await page.locator('.xterm-helper-textarea').focus();
  await page.keyboard.type('ls');

  await expect
    .poll(
      () =>
        controls
          .filter((c) => c.kind === 'send_input' && c.text)
          .map((c) => c.text)
          .join(''),
      { timeout: 10_000 },
    )
    .toContain('ls');
});

test('Ctrl-C button and Control+C keystroke both send interrupt', async ({
  page,
}) => {
  const { controls } = await installMocks(page);
  await gotoTerminal(page);
  await expect(page.locator('.xterm-rows')).toContainText('cargo test', {
    timeout: 15_000,
  });

  // (1) The explicit toolbar button.
  await page.getByTestId('agent-terminal-interrupt').click();
  await expect
    .poll(() => controls.filter((c) => c.kind === 'interrupt').length, {
      timeout: 10_000,
    })
    .toBeGreaterThanOrEqual(1);

  // (2) A Control+C keystroke in the terminal is promoted to interrupt too.
  await page.locator('.xterm-helper-textarea').focus();
  await page.keyboard.press('Control+C');
  await expect
    .poll(() => controls.filter((c) => c.kind === 'interrupt').length, {
      timeout: 10_000,
    })
    .toBeGreaterThanOrEqual(2);
});

test('a viewport resize drives a resize_pty control', async ({ page }) => {
  const { controls } = await installMocks(page);
  await gotoTerminal(page);
  await expect(page.locator('.xterm-rows')).toContainText('cargo test', {
    timeout: 15_000,
  });

  const before = controls.filter((c) => c.kind === 'resize_pty').length;
  await page.setViewportSize({ width: 700, height: 900 });

  await expect
    .poll(() => controls.filter((c) => c.kind === 'resize_pty').length, {
      timeout: 10_000,
    })
    .toBeGreaterThan(before);
  const last = controls.filter((c) => c.kind === 'resize_pty').at(-1);
  expect(last?.cols).toBeGreaterThan(0);
  expect(last?.rows).toBeGreaterThan(0);
});

test('recovers after SSE disconnect (reconnect)', async ({ page }) => {
  await installMocks(page);
  await gotoTerminal(page);
  await expect(page.locator('.xterm-rows')).toContainText('cargo test', {
    timeout: 15_000,
  });

  // The SSE mock fulfills immediately (not a real long-lived stream), so the
  // EventSource reconnects automatically. The second connection gets a
  // different banner. Wait for the reconnected content.
  await expect(page.locator('.xterm-rows')).toContainText('recovered run', {
    timeout: 20_000,
  });
});
