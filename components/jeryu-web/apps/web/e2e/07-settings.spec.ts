// 07-settings.spec.ts — repository settings preview flow (W-T-15).
//
// Smoke for the §35.1.5 settings preview chain:
//   1. `GET /api/v1/repos/{id}/settings` returns the current values.
//   2. `POST /api/v1/repos/{id}/settings/preview` returns a typed
//      `SettingsDiffPreview` with diffs + optional `warnings`.
//   3. (Phase 3) `PATCH /api/v1/repos/{id}/settings` commits.
//
// Phase-3-tolerant: both endpoints are mocked. The SPA's
// `RepositorySettingsPage` renders the real settings studio; the first test
// pins the preview contract and the second drives preview/apply/discard/drift
// through visible controls.

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
  test('preview round-trips diff + warnings @action:settings.preview_contract', async ({ page }) => {
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
          body: JSON.stringify({ description: 'new description' }),
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
      current_hash?: string;
      diffs?: Array<{ field: string; after: string | null }>;
      side_effects?: string[];
    };
    expect(body.current_hash).toBe('settings-hash-1');
    expect(Array.isArray(body.warnings)).toBe(true);
    expect(body.warnings?.length ?? 0).toBeGreaterThan(0);
    expect(body.diffs).toEqual([
      { field: 'description', before: 'mocked', after: 'new description' },
    ]);
    expect(body.side_effects?.length ?? 0).toBeGreaterThan(0);
  });

  test('settings UI previews, applies, discards, and recovers drift @action:settings.section_nav @action:settings.edit @action:settings.preview_ui @action:settings.apply @action:settings.discard @action:settings.drift_recovery', async ({
    page,
  }) => {
    await mockBootstrap(page);
    await mockRepoLookup(page, { id: REPO, default_branch: 'main' });
    const settings = await mockSettings(page);
    await mockSettingsPreview(page, []);

    let patchCount = 0;
    await page.route(
      /\/api\/v1\/repos\/[^/]+\/settings$/,
      async (route, request) => {
        if (request.method() !== 'PATCH') {
          await route.fallback();
          return;
        }
        patchCount += 1;
        if (patchCount === 2) {
          await route.fulfill({
            status: 409,
            contentType: 'application/json',
            body: JSON.stringify({
              error: {
                code: 'settings_hash_stale',
                message: 'Settings changed since preview.',
                details: { current_hash: 'settings-hash-2' },
              },
            }),
          });
          return;
        }
        const body = JSON.parse(request.postData() ?? '{}') as {
          description?: string | null;
        };
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            ...settings,
            general: {
              ...(settings.general as Record<string, unknown>),
              description: body.description ?? 'mocked',
            },
          }),
        });
      }
    );

    await page.goto(
      `/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}/settings/general`
    );
    await expect(
      page.getByRole('heading', { name: /Settings · jeryu/i })
    ).toBeVisible({ timeout: 15_000 });

    await page.getByRole('link', { name: /Merge policy/i }).click();
    await expect(page).toHaveURL(/\/settings\/merge-policy$/);
    await page.getByRole('link', { name: /^General$/ }).click();

    await page.getByLabel('Description').fill('updated description');
    await page.getByRole('button', { name: 'Preview changes' }).click();
    await expect(
      page.getByRole('rowheader', { name: 'description' })
    ).toBeVisible();
    await page.getByRole('button', { name: 'Apply changes' }).click();
    await expect(page.getByText(/Changes applied/i)).toBeVisible({
      timeout: 10_000,
    });

    await page.getByLabel('Description').fill('discard me');
    await page.getByRole('button', { name: 'Discard' }).click();
    await expect(page.getByLabel('Description')).toHaveValue(
      'updated description'
    );

    await page.getByLabel('Description').fill('drift me');
    await page.getByRole('button', { name: 'Preview changes' }).click();
    await page.getByRole('button', { name: 'Apply changes' }).click();
    const drift = page.getByRole('alert').filter({
      hasText: /Settings changed since you opened this page/i,
    });
    await expect(drift).toBeVisible({ timeout: 10_000 });
    await drift.getByRole('button', { name: 'Refresh' }).click();
    await expect(drift).toHaveCount(0);
  });

  test('deep settings URL returns 200 (SPA fallback) @bff', async ({ request }) => {
    const res = await request.get(
      `/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}/settings/merge`,
      { failOnStatusCode: false }
    );
    expect(res.status()).toBe(200);
  });
});
