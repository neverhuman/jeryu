// 07-settings.spec.ts — repository settings preview flow (W-T-15).
//
// Smoke for the §35.1.5 settings preview chain:
//   1. `GET /api/v1/repos/{id}/settings` returns the current values.
//   2. `POST /api/v1/repos/{id}/settings/preview` returns a `dry_run` diff
//      + optional `warnings`.
//   3. (Phase 3) `PATCH /api/v1/repos/{id}/settings` commits.
//
// Phase-3-tolerant: both endpoints are mocked. The SPA's
// `RepositorySettingsPage` may still render the not-implemented envelope
// (W-FE-12), so we exercise the surface via `request` and assert the BFF
// round-trips the dry-run payload without losing warning copy. The
// route-level smoke confirms the SPA fallback still resolves the deep
// settings URL.

import { expect, test } from '@playwright/test';

import {
  mockBootstrap,
  mockRepoLookup,
  mockSettings,
  mockSettingsPreview,
} from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' } as const;
const REPO_ID = `${REPO.host}:${REPO.owner}/${REPO.name}`;

test.describe('Settings preview (W-T-15)', () => {
  test('preview round-trips diff + warnings', async ({ page }) => {
    await mockBootstrap(page);
    await mockRepoLookup(page, { id: REPO, default_branch: 'main' });
    await mockSettings(page);
    await mockSettingsPreview(page, [
      'Enabling `merge.squash` will rewrite the merge button copy.',
    ]);

    await page.goto(
      `/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}/settings/merge`
    );

    // SPA renders either the not-implemented envelope or the real settings
    // page; both must put SOMETHING readable on screen.
    const heading = page.locator('h1', { hasText: /settings/i });
    const errorState = page.locator('[role="alert"]');
    await expect(heading.or(errorState)).toBeVisible({ timeout: 15_000 });

    // Drive the preview endpoint from the page's network stack so the
    // `page.route(...)` mock fires. `page.request.*` uses an isolated
    // APIRequestContext that bypasses route interception.
    const url = `/api/v1/repos/${encodeURIComponent(REPO_ID)}/settings/preview`;
    const previewRes = await page.evaluate(
      async (postUrl) => {
        const r = await fetch(postUrl, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'X-CSRF-Token': 'e2e-csrf',
          },
          body: JSON.stringify({ merge: { squash: true } }),
        });
        const text = await r.text();
        let body: unknown = text;
        try {
          body = text.length > 0 ? JSON.parse(text) : null;
        } catch {
          body = text;
        }
        return { status: r.status, body };
      },
      url
    );

    expect(previewRes.status, `preview returned ${previewRes.status}`).toBe(200);
    const body = previewRes.body as {
      warnings?: string[];
      dry_run?: boolean;
      requires_confirmation?: boolean;
      patch?: Record<string, unknown>;
    };
    expect(body.dry_run, 'preview must flag dry_run').toBe(true);
    expect(Array.isArray(body.warnings)).toBe(true);
    expect(body.warnings?.length ?? 0).toBeGreaterThan(0);
    expect(body.requires_confirmation).toBe(true);
    expect(body.patch).toMatchObject({ merge: { squash: true } });
  });

  test('deep settings URL returns 200 (SPA fallback)', async ({ request }) => {
    const res = await request.get(
      `/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}/settings/merge`,
      { failOnStatusCode: false }
    );
    expect(res.status()).toBe(200);
  });
});
