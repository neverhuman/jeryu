// 22-repo-danger-zone.spec.ts — two-tier repository removal from the overview.
//
// The overview page renders a danger zone with two destructive rows:
// registry-only removal (bare storage on disk is kept) and the purge tier
// (registry + storage, typed-name confirmation, cannot be undone). The
// removal endpoint (HTTP DELETE) is mocked via `mockDeleteRepo`, which captures the
// request so the spec can assert the confirmation contract:
//   * `{ confirm_full_name, delete_storage }` JSON body,
//   * `Idempotency-Key` header,
//   * receipt handling → navigation back to /repos.
// Negative paths: a 422 `confirm_mismatch` and a 403 `permission_denied`
// (a viewer who is not allowed to delete) both surface the backend message
// inside the dialog (role="alert") and provably do NOT leave the page —
// the destructive call did not succeed.

import { expect, test } from '@playwright/test';

import {
  mockBootstrap,
  mockDeleteRepo,
  mockReadme,
  mockRefs,
  mockRepoList,
} from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = {
  id: { host: 'jeryu', owner: 'veox', name: 'redline' },
  default_branch: 'main',
  description: 'Edge router for VEOX.',
  visibility: 'internal' as const,
  jankurai_score: 92,
  jankurai_decision: 'pass',
  jankurai_scored_at: '2026-06-09T08:30:00Z',
  available_actions: [
    {
      action_id: 'repo.delete_registry',
      label: 'Remove from registry',
      risk: 'destructive',
    },
    {
      action_id: 'repo.delete_storage',
      label: 'Purge repository and storage',
      risk: 'destructive',
    },
  ],
};

const FULL_NAME = 'veox/redline';

// NOTE: `mockDeleteRepo` must be registered AFTER `mockRepoList` — route
// handlers run LIFO and the list mock `route.continue()`s non-GET traffic
// straight to the network (see the fixture doc comment).
async function openOverview(
  page: import('@playwright/test').Page,
  deleteOpts: Parameters<typeof mockDeleteRepo>[2] = {}
): Promise<Awaited<ReturnType<typeof mockDeleteRepo>>> {
  await mockBootstrap(page);
  await mockRepoList(page, [REPO]);
  await mockRefs(page);
  await mockReadme(page, { html: '<h1>redline</h1>' });
  const captured = await mockDeleteRepo(page, REPO.id, deleteOpts);
  await page.goto('/repos/jeryu/veox/redline');
  await expect(page.getByTestId('repo-danger-zone')).toBeVisible({
    timeout: 10_000,
  });
  return captured;
}

test.describe('Repository danger zone (delete)', () => {
  test('purge tier gates on the typed name and the receipt navigates to /repos', async ({
    page,
  }) => {
    const captured = await openOverview(page, { storageDeleted: true });

    const zone = page.getByTestId('repo-danger-zone');
    // Both rows use the backend-advertised action labels.
    await expect(
      zone.getByRole('button', { name: 'Remove from registry' })
    ).toBeVisible();
    await zone
      .getByRole('button', { name: 'Purge repository and storage' })
      .click();

    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText('cannot be undone');
    await expect(dialog.getByLabel('Risk: Critical')).toBeVisible();

    // Confirm is disabled until the exact owner/name is typed.
    const confirm = dialog.getByRole('button', {
      name: 'Purge repository and storage',
    });
    await expect(confirm).toBeDisabled();
    const input = dialog.getByLabel(/to confirm/);
    await input.fill('veox/wrong');
    await expect(confirm).toBeDisabled();
    await input.fill(FULL_NAME);
    await expect(confirm).toBeEnabled();

    await page.screenshot({
      path: 'playwright-report/repo-danger-zone-purge.png',
      fullPage: true,
    });

    await confirm.click();

    // Receipt handled → list invalidated and SPA navigates to /repos.
    await expect(page).toHaveURL(/\/repos$/, { timeout: 10_000 });

    expect(captured).toHaveLength(1);
    const req = captured[0]!;
    expect(req.url).toContain('/api/v1/repos/');
    expect(req.body).toEqual({
      confirm_full_name: FULL_NAME,
      delete_storage: true,
    });
    expect(req.idempotencyKey).toBeTruthy();
  });

  test('422 confirm_mismatch from the backend stays in the dialog with the error', async ({
    page,
  }) => {
    const captured = await openOverview(page, {
      error: {
        status: 422,
        code: 'confirm_mismatch',
        message: 'confirm_full_name does not match the repository.',
      },
    });

    await page
      .getByTestId('repo-danger-zone')
      .getByRole('button', { name: 'Remove from registry' })
      .click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByLabel('Risk: High')).toBeVisible();
    await expect(dialog).toContainText('bare storage on disk is kept');
    await dialog
      .getByRole('button', { name: 'Remove from registry' })
      .click();

    await expect(dialog.getByRole('alert')).toContainText(
      'confirm_full_name does not match the repository.'
    );
    expect(captured).toHaveLength(1);
    expect(captured[0]!.body).toEqual({
      confirm_full_name: FULL_NAME,
      delete_storage: false,
    });
    // Still on the overview — nothing was deleted.
    await expect(page).toHaveURL(/\/repos\/jeryu\/veox\/redline/);
  });

  test('negative authorization: 403 for a viewer without removal rights', async ({
    page,
  }) => {
    await openOverview(page, {
      error: {
        status: 403,
        code: 'permission_denied',
        message: 'You are not allowed to remove veox/redline.',
      },
    });

    await page
      .getByTestId('repo-danger-zone')
      .getByRole('button', { name: 'Remove from registry' })
      .click();
    const dialog = page.getByRole('dialog');
    await dialog
      .getByRole('button', { name: 'Remove from registry' })
      .click();

    await expect(dialog.getByRole('alert')).toContainText(
      'You are not allowed to remove veox/redline.'
    );
    // The forbidden viewer keeps zero side effects: same route, dialog open.
    await expect(page).toHaveURL(/\/repos\/jeryu\/veox\/redline/);
    await expect(dialog).toBeVisible();
  });
});
