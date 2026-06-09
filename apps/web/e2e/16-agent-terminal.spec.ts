// 16-agent-terminal.spec.ts — live agent terminal end-to-end (watch + drive).
//
// Uses Playwright's `page.routeWebSocket` to fully mock `/api/v1/ws` (no
// backend / Docker): the fixture speaks the `jeryu.ws.v1` protocol — answers
// the client `hello`, and whenever the client subscribes to an `agent_run.*`
// scope it streams scripted base64 TTY chunks (a prompt, an ANSI-colored line,
// a trailing prompt). It also CAPTURES every client→server `agent_control`
// frame so we can assert the operator's keystrokes / interrupt / resize are
// driven back over the wire.
//
// Asserts:
//   (a) the terminal renders the scripted text (`.xterm-rows` has "cargo test");
//   (b) typing sends `input` controls carrying the correct decoded bytes;
//   (c) the Ctrl-C toolbar button AND a Control+C keystroke send `interrupt`;
//   (d) a viewport resize sends a `resize` control;
//   (e) a server-side ws close → reconnect recovers and re-renders.

import { expect, test, type Page } from '@playwright/test';
import type { WebSocketRoute } from '@playwright/test';

import { AppShellPage } from './pages/AppShellPage';
import { mockBootstrap, mockRepoAgentRuns, mockRepoList } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' } as const;
const RUN_ID = 'run-42';
const REPO_PATH = `/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}`;
const AGENT_PATH = `${REPO_PATH}/agents/${RUN_ID}`;

interface CapturedControl {
  type: string;
  run_id: string;
  control: { kind: string; bytes_b64?: string; cols?: number; rows?: number };
}

function b64(text: string): string {
  return Buffer.from(text, 'binary').toString('base64');
}

function decodeControlInput(controls: CapturedControl[]): string {
  return controls
    .filter((c) => c.control.kind === 'input' && c.control.bytes_b64)
    .map((c) => Buffer.from(c.control.bytes_b64 as string, 'base64').toString('binary'))
    .join('');
}

/**
 * Install the scripted WS server. Returns the live captured-controls array and
 * a `closeActive()` to simulate a server-side disconnect.
 */
async function installScriptedWs(page: Page): Promise<{
  controls: CapturedControl[];
  closeActive: () => Promise<void>;
}> {
  const controls: CapturedControl[] = [];
  let connectionCount = 0;
  let activeWs: WebSocketRoute | null = null;

  await page.routeWebSocket('**/api/v1/ws', (ws) => {
    connectionCount += 1;
    const conn = connectionCount;
    activeWs = ws;
    const emitted = new Set<string>();
    let seq = 0;

    const emitFrame = (scope: string, chunkSeq: number, text: string): void => {
      seq += 1;
      ws.send(
        JSON.stringify({
          type: 'event',
          event: {
            seq,
            timestamp: '2026-01-01T00:00:00Z',
            scope,
            kind: 'agent.tty',
            entity: scope,
            summary: 'tty',
            payload: { chunk_seq: chunkSeq, stream: 'pty', bytes_b64: b64(text) },
          },
        })
      );
    };

    const emitScript = (scope: string): void => {
      if (emitted.has(scope)) return;
      emitted.add(scope);
      // Different banner per connection so the reconnect assertion is unambiguous.
      const prompt = conn === 1 ? '$ cargo test\r\n' : '$ recovered run\r\n';
      emitFrame(scope, 1, prompt);
      // ANSI green "PASS" then reset.
      emitFrame(scope, 2, '\x1b[32mPASS\x1b[0m all tests\r\n');
      emitFrame(scope, 3, '$ ');
    };

    const handleScopes = (specs: Array<{ scope?: unknown }> | undefined): void => {
      for (const s of specs ?? []) {
        if (typeof s?.scope === 'string' && s.scope.startsWith('agent_run.')) {
          emitScript(s.scope);
        }
      }
    };

    ws.onMessage((message) => {
      const text = typeof message === 'string' ? message : message.toString('utf8');
      let msg: Record<string, unknown>;
      try {
        msg = JSON.parse(text);
      } catch {
        return;
      }
      switch (msg.type) {
        case 'hello':
          ws.send(
            JSON.stringify({
              type: 'hello',
              server_time: '2026-01-01T00:00:00Z',
              current_seq: 0,
              protocol: 'jeryu.ws.v1',
            })
          );
          handleScopes(msg.subscriptions as Array<{ scope?: unknown }>);
          break;
        case 'subscribe':
          handleScopes(msg.subscriptions as Array<{ scope?: unknown }>);
          break;
        case 'agent_control':
          controls.push(msg as unknown as CapturedControl);
          break;
        default:
          break;
      }
    });
  });

  return {
    controls,
    closeActive: async () => {
      await activeWs?.close();
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

test('renders streamed TTY text from the scripted WS', async ({ page }) => {
  await installScriptedWs(page);
  await gotoTerminal(page);

  await expect(page.locator('.xterm-rows')).toContainText('cargo test', {
    timeout: 15_000,
  });
  await expect(page.locator('.xterm-rows')).toContainText('PASS');
});

test('typing sends input controls with the correct decoded bytes', async ({
  page,
}) => {
  const { controls } = await installScriptedWs(page);
  await gotoTerminal(page);
  await expect(page.locator('.xterm-rows')).toContainText('cargo test', {
    timeout: 15_000,
  });

  await page.locator('.xterm-helper-textarea').focus();
  await page.keyboard.type('ls');

  await expect
    .poll(() => decodeControlInput(controls), { timeout: 10_000 })
    .toContain('ls');
});

test('Ctrl-C button and Control+C keystroke both send interrupt', async ({
  page,
}) => {
  const { controls } = await installScriptedWs(page);
  await gotoTerminal(page);
  await expect(page.locator('.xterm-rows')).toContainText('cargo test', {
    timeout: 15_000,
  });

  // (1) The explicit toolbar button.
  await page.getByTestId('agent-terminal-interrupt').click();
  await expect
    .poll(() => controls.filter((c) => c.control.kind === 'interrupt').length, {
      timeout: 10_000,
    })
    .toBeGreaterThanOrEqual(1);

  // (2) A Control+C keystroke in the terminal is promoted to interrupt too.
  await page.locator('.xterm-helper-textarea').focus();
  await page.keyboard.press('Control+C');
  await expect
    .poll(() => controls.filter((c) => c.control.kind === 'interrupt').length, {
      timeout: 10_000,
    })
    .toBeGreaterThanOrEqual(2);
});

test('a viewport resize drives a resize control', async ({ page }) => {
  const { controls } = await installScriptedWs(page);
  await gotoTerminal(page);
  await expect(page.locator('.xterm-rows')).toContainText('cargo test', {
    timeout: 15_000,
  });

  const before = controls.filter((c) => c.control.kind === 'resize').length;
  await page.setViewportSize({ width: 700, height: 900 });

  await expect
    .poll(() => controls.filter((c) => c.control.kind === 'resize').length, {
      timeout: 10_000,
    })
    .toBeGreaterThan(before);
  const last = controls.filter((c) => c.control.kind === 'resize').at(-1);
  expect(last?.control.cols).toBeGreaterThan(0);
  expect(last?.control.rows).toBeGreaterThan(0);
});

test('recovers after a server-side ws close (reconnect)', async ({ page }) => {
  const { closeActive } = await installScriptedWs(page);
  await gotoTerminal(page);
  await expect(page.locator('.xterm-rows')).toContainText('cargo test', {
    timeout: 15_000,
  });

  // Drop the socket from the server side; the client backs off and reconnects,
  // re-subscribes, and the fixture streams the second-connection banner.
  await closeActive();

  await expect(page.locator('.xterm-rows')).toContainText('recovered run', {
    timeout: 20_000,
  });
});
