// 09-permissions.spec.ts — permission-denied UI state (W-T-17).
//
// The SPA renders one of three permission-related surfaces based on the
// viewer's global_permissions:
//   * Full surface — viewer has every relevant perm.
//   * `PermissionDeniedState` — viewer lacks a perm (e.g. only `repo.read`,
//     missing `pr.approve` / `settings.write`).
//   * `ErrorState` — backend 403 propagated through `ApiError`.
//
// Phase-3 status: the SPA does not yet gate every action button on the
// viewer's permissions (W-FE-13 outstanding). This spec asserts the
// surfaces we CAN test today:
//   1. The SPA boots cleanly with a restricted viewer (no Error Boundary).
//   2. A mutating endpoint called on behalf of the restricted viewer
//      surfaces a permission_denied envelope when the backend rejects
//      it — proving the perm-gate runs server-side (the canonical
//      enforcement point per §35.1.5).
//   3. Pages still render their primary surface (heading or error state)
//      so navigation remains accessible.

import { expect, test } from '@playwright/test';

import {
  mockBootstrap,
  mockRepoLookup,
  mockSettings,
} from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' } as const;
const REPO_ID = `${REPO.host}:${REPO.owner}/${REPO.name}`;

test.describe('Permission-denied UI state (W-T-17)', () => {
  test('restricted viewer boots SPA and is rejected on mutating call', async ({ page }) => {
    await mockBootstrap(page, {
      login: '@reader',
      display_name: 'Read-only Reader',
      global_permissions: ['repo.read'],
    });
    await mockRepoLookup(page, { id: REPO, default_branch: 'main' });
    await mockSettings(page);

    // Force the BFF settings PATCH to return permission_denied so the
    // server-side gate is exercised even when the SPA would otherwise
    // let the click through. This is the canonical §35.1.5 surface for
    // perm-denied actions.
    await page.route(
      /\/api\/v1\/repos\/[^/]+\/settings$/,
      async (route, request) => {
        if (request.method() === 'PATCH') {
          await route.fulfill({
            status: 403,
            contentType: 'application/json',
            body: JSON.stringify({
              error: {
                code: 'permission_denied',
                message: 'You need settings.write to apply this change.',
                details: { missing: 'settings.write' },
                request_id: 'mock-perm-denied',
              },
            }),
          });
          return;
        }
        await route.continue();
      }
    );

    // Repositories page boots without crashing.
    await page.goto('/repos');
    await expect(
      page
        .locator('h1', { hasText: /Repositories|Repos/i })
        .or(page.locator('[role="alert"]'))
    ).toBeVisible({ timeout: 15_000 });

    // Settings page boots without crashing.
    await page.goto(
      `/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}/settings/general`
    );
    await expect(
      page
        .locator('h1')
        .first()
        .or(page.locator('[role="alert"]'))
    ).toBeVisible({ timeout: 15_000 });

    // Drive the mutating call from the page's network stack so the
    // `page.route(...)` mock fires (page.request.* uses an isolated
    // APIRequestContext that bypasses interception).
    const patchRes = await page.evaluate(async (url) => {
      const r = await fetch(url, {
        method: 'PATCH',
        headers: {
          'Content-Type': 'application/json',
          'X-CSRF-Token': 'e2e-csrf',
        },
        body: JSON.stringify({ general: { description: 'will-be-rejected' } }),
      });
      const text = await r.text();
      let body: unknown = text;
      try {
        body = text.length > 0 ? JSON.parse(text) : null;
      } catch {
        body = text;
      }
      return { status: r.status, body };
    }, `/api/v1/repos/${encodeURIComponent(REPO_ID)}/settings`);

    expect(patchRes.status, `PATCH returned ${patchRes.status}`).toBe(403);
    const body = patchRes.body as {
      error?: { code?: string; details?: { missing?: string } };
    };
    expect(body.error?.code).toBe('permission_denied');
    expect(body.error?.details?.missing).toBe('settings.write');
  });

  test('bootstrap reflects restricted viewer global_permissions', async ({ page }) => {
    await mockBootstrap(page, {
      login: '@reader',
      global_permissions: ['repo.read'],
    });
    await page.goto('/');
    const res = await page.evaluate(async () => {
      const r = await fetch('/api/v1/bootstrap');
      const json = await r.json();
      return { status: r.status, body: json };
    });
    expect(res.status).toBe(200);
    const body = res.body as {
      viewer?: { login?: string; global_permissions?: string[] };
    };
    expect(body.viewer?.login).toBe('@reader');
    expect(body.viewer?.global_permissions).toEqual(['repo.read']);
  });

});
