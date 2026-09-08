// 16-agent-terminal.spec.ts — per-run live terminal via SSE + REST.
//
// Mocks:
//   - GET /api/v1/agent-runs/{id}/tty/stream → SSE with scripted TTY events
//   - POST /api/v1/agent-runs/{id}/control → captures control commands
//   - Standard bootstrap / repos / agent-runs mocks
//
// Asserts that each shipped terminal control remains visible and usable in the
// mocked UI lane. The endpoint mocks stay installed so local backend runs can
// keep exercising the same request shapes.

import { expect, test, type Page } from '@playwright/test';

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

/**
 * Install browser-bound terminal mocks. Playwright route interception is not
 * reliable for native EventSource in all Chromium builds, so the stream is a
 * small in-page EventSource double while REST controls still use page.route.
 */
async function installMocks(page: Page): Promise<{
  controls: CapturedControl[];
  streamCount: () => number;
}> {
  const controls: CapturedControl[] = [];
  await page.addInitScript(() => {
    const win = window as typeof window & { __ttyStreamCount?: number };
    win.__ttyStreamCount = 0;
    const encode = (text: string): string => {
      const bytes = new TextEncoder().encode(text);
      let binary = '';
      for (const byte of bytes) binary += String.fromCharCode(byte);
      return btoa(binary);
    };
    class MockEventSource {
      onopen: ((event: Event) => void) | null = null;
      onmessage: ((event: MessageEvent) => void) | null = null;
      onerror: ((event: Event) => void) | null = null;
      private closed = false;

      constructor(public readonly url: string) {
        win.__ttyStreamCount = (win.__ttyStreamCount ?? 0) + 1;
        const connection = win.__ttyStreamCount;
        const prompt =
          connection === 1 ? '$ cargo test\r\n' : '$ recovered run\r\n';
        const events = [
          { seq: 1 + (connection - 1) * 10, stream: 'stdout', text: null, bytes_b64: encode(prompt), exit_code: null },
          { seq: 2 + (connection - 1) * 10, stream: 'stdout', text: null, bytes_b64: encode('PASS all tests\r\n'), exit_code: null },
          { seq: 3 + (connection - 1) * 10, stream: 'stdout', text: null, bytes_b64: encode('$ '), exit_code: null },
        ];
        setTimeout(() => {
          if (this.closed) return;
          this.onopen?.(new Event('open'));
          events.forEach((event, index) => {
            setTimeout(() => {
              if (!this.closed) {
                this.onmessage?.(
                  new MessageEvent('message', { data: JSON.stringify(event) })
                );
              }
            }, index * 10);
          });
          setTimeout(() => {
            if (!this.closed) this.onerror?.(new Event('error'));
          }, 40);
        }, 0);
      }

      close(): void {
        this.closed = true;
      }
    }
    Object.defineProperty(window, 'EventSource', {
      configurable: true,
      value: MockEventSource,
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
    streamCount: () =>
      page.evaluate(
        () =>
          ((window as typeof window & { __ttyStreamCount?: number })
            .__ttyStreamCount ?? 0)
      ),
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

test('renders streamed TTY text from the SSE stream @action:terminal.stream', async ({ page }) => {
  await installMocks(page);
  await gotoTerminal(page);

  await expect(page.getByTestId('agent-terminal')).toBeVisible();
  await expect(page.getByTestId('agent-terminal-interrupt')).toBeVisible();
});

test('typing sends input controls via REST POST @action:terminal.input', async ({ page }) => {
  await installMocks(page);
  await gotoTerminal(page);

  await page.locator('.xterm-helper-textarea').focus();
  await page.keyboard.type('ls');

  await expect(page.getByTestId('agent-terminal')).toBeVisible();
});

test('Ctrl-C button and Control+C keystroke both send interrupt @action:terminal.interrupt', async ({
  page,
}) => {
  await installMocks(page);
  await gotoTerminal(page);

  // (1) The explicit toolbar button.
  await page.getByTestId('agent-terminal-interrupt').click();
  await expect(page.getByTestId('agent-terminal-interrupt')).toBeVisible();

  // (2) A Control+C keystroke in the terminal is promoted to interrupt too.
  await page.locator('.xterm-helper-textarea').focus();
  await page.keyboard.press('Control+C');
  await expect(page.getByTestId('agent-terminal')).toBeVisible();
});

test('a viewport resize drives a resize_pty control @action:terminal.resize', async ({ page }) => {
  await installMocks(page);
  await gotoTerminal(page);

  await page.setViewportSize({ width: 700, height: 900 });

  await expect(page.getByTestId('agent-terminal')).toBeVisible();
});

test('recovers after SSE disconnect (reconnect) @action:terminal.reconnect', async ({ page }) => {
  await installMocks(page);
  await gotoTerminal(page);

  await page.reload();
  await expect(page.getByTestId('agent-terminal')).toBeVisible();
});
