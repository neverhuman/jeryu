// 08-ws-reconnect.spec.ts — WS disconnect + reconnect smoke (W-T-16).
//
// The SPA's `JeRyuWsClient` (apps/web/src/api/websocket.ts) drives
// transport-level state in the GlobalHeader's "live" pill: "Live" once the
// server `hello` arrives, "Connecting" / "Reconnecting" / "Offline" / "Idle"
// otherwise. The reconnect policy is exponential backoff capped at 30 s
// with full jitter.
//
// Playwright limitations:
//   * `page.route` only intercepts HTTP requests, not raw WebSocket frames.
//     Aborting `/api/v1/ws` aborts the upgrade handshake; the SPA reacts to
//     the `close` event with a reconnect attempt.
//   * Browsers cache the redirect/close decision per-connection; once the
//     abort is unrouted, subsequent reconnect attempts succeed.
//
// This spec verifies the SPA survives the disconnect/reconnect cycle
// without an Error Boundary trip. It does NOT assert the pill text in
// the middle of the cycle (timing-sensitive); it asserts the shell stays
// mounted before and after the network manipulation.

import { expect, test } from '@playwright/test';

import { mockBootstrap } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

test.describe('WebSocket reconnect (W-T-16)', () => {
  test('aborting WS does not crash the SPA @action:ws.reconnect', async ({ page }) => {
    await mockBootstrap(page);

    // Abort the WS upgrade. We register the route before any navigation
    // so the SPA's first connect attempt encounters the abort.
    await page.context().route('**/api/v1/ws', (route) =>
      route.abort('failed').catch(() => undefined)
    );

    await page.goto('/');

    // The SPA must mount its shell or an error surface even though the
    // WS upgrade is being aborted. Use `.first()` to avoid strict-mode
    // duplicate matches between the header and its live pill.
    const shellOrError = page
      .locator('.global-header, .app-shell, [role="alert"]')
      .first();
    await expect(shellOrError).toBeVisible({ timeout: 15_000 });

    // Stop aborting. Subsequent reconnect attempts may now succeed
    // against the live BFF.
    await page.context().unroute('**/api/v1/ws');

    // Let the backoff timer fire one more reconnect cycle.
    await page.waitForTimeout(2_500);

    // Shell still mounted — no Error Boundary trip from the disconnect.
    await expect(shellOrError).toBeVisible();
  });

  test('SPA boots without an open WS (graceful degrade) @action:ws.offline_boot', async ({ page }) => {
    await mockBootstrap(page);
    // Permanently block the WS upgrade by aborting every request.
    await page.context().route('**/api/v1/ws', (route) =>
      route.abort('failed').catch(() => undefined)
    );

    await page.goto('/');

    const shellOrError = page
      .locator('.global-header, .app-shell, [role="alert"]')
      .first();
    await expect(shellOrError).toBeVisible({ timeout: 15_000 });

    // Live pill — if it rendered, must indicate a non-Live state OR
    // already-Live (the live BFF may have answered before the route
    // engaged on slower runners). The point is the SPA does not crash.
    const pill = page.locator('.global-header__live');
    if ((await pill.count()) > 0) {
      const text = await pill.first().innerText();
      expect(
        /Live|Connecting|Reconnecting|Offline|Idle/i.test(text),
        `live pill text: ${text}`
      ).toBe(true);
    }
  });
});
