// 06-approve-sha.spec.ts — exact-SHA approval flow + SHA drift conflict (W-T-14).
//
// Drives the §35.1.7 "approve at exact SHA" contract through the REAL PR
// cockpit UI (no `page.evaluate(fetch)`): we hydrate the page with the full
// `PullRequestDetail` wire shape, click the "Approve exact SHA <sha>" button
// in the Review sidebar, and assert the SPA's resulting surface.
//
//   1. Success path — the approve endpoint returns the updated detail (200);
//      the SPA swaps in the new copy and shows NO recovery banner.
//
//   2. Stale path — when the head moved since page load, the approve endpoint
//      returns `409 merge_sha_stale` with `expected_sha` / `current_sha`. The
//      cockpit must surface its recovery banner (role="alert") naming the SHA
//      drift (old → new) with a Refresh button.
//
// The Approve button always carries the head SHA the reviewer saw
// (`ReviewSidebar` reads `detail.summary.head_sha`), so clicking it is the
// real driver of the exact-SHA body the backend gates on.

import { expect, test } from '@playwright/test';

import {
  forceDriftSha,
  mockBootstrap,
  mockPullRequestDetail,
  mockRepoList,
} from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' } as const;
const REPO_ID = `${REPO.host}:${REPO.owner}/${REPO.name}`;
const PR_NUMBER = '99';
const OLD_SHA = '1111111111111111111111111111111111111111';
const NEW_SHA = '2222222222222222222222222222222222222222';
const PR_URL = `/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}/pulls/${PR_NUMBER}`;

/** Locate the Approve button by its exact-SHA label (`head_sha.slice(0,7)`). */
function approveButton(page: import('@playwright/test').Page) {
  return page.getByRole('button', {
    name: new RegExp(`Approve exact SHA ${OLD_SHA.slice(0, 7)}`, 'i'),
  });
}

test.describe('Approve at exact SHA (W-T-14)', () => {
  test('clicking Approve on the cockpit succeeds and shows no recovery banner', async ({
    page,
  }) => {
    await mockBootstrap(page);
    // The list mock lets `useResolveRepo` map the URL to the backend repo id.
    await mockRepoList(page, [{ id: REPO, default_branch: 'main' }]);
    await mockPullRequestDetail(page, {
      repoId: REPO_ID,
      number: PR_NUMBER,
      title: 'Approve happy path',
      head_sha: OLD_SHA,
      passport: 'blocked',
    });

    // Approve endpoint accepts the exact head SHA and returns the (now
    // approved) detail. We echo back a 1-approval posture so the success
    // path is observable in the sidebar.
    await page.route(
      /\/api\/v1\/repos\/[^/]+\/pulls\/[^/]+\/approve$/,
      async (route, req) => {
        if (req.method() !== 'POST') {
          await route.continue();
          return;
        }
        const body = JSON.parse(req.postData() ?? '{}') as {
          expected_head_sha?: string;
        };
        // The body must carry the exact SHA the reviewer saw.
        expect(body.expected_head_sha).toBe(OLD_SHA);
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            summary: {
              repo: {
                id: REPO_ID,
                host: 'jeryu',
                owner: 'neverhuman',
                name: 'jeryu',
              },
              number: Number(PR_NUMBER),
              entity: { kind: 'pull_request', id: `${REPO_ID}#${PR_NUMBER}` },
              title: 'Approve happy path',
              author: '@author',
              head_ref: 'feature/x',
              base_ref: 'main',
              head_sha: OLD_SHA,
              base_sha: 'base000000000000000000000000000000000000',
              state: 'open',
              draft: false,
              mergeable: {
                level: 'blocked',
                can_merge: false,
                reason: 'Passport blocked',
                exact_head_sha: OLD_SHA,
                required_gate: 'passport',
              },
              review: {
                required_approvals: 1,
                approvals: 1,
                changes_requested: 0,
                unresolved_threads: 0,
                user_review_state: 'approved',
              },
              checks: {
                total: 2,
                passing: 2,
                failing: 0,
                pending: 0,
                skipped: 0,
              },
              agents: {
                active_sessions: 0,
                proposed_patches: 0,
                evidence_packets: 0,
                blockers: 0,
              },
              labels: [],
              updated_at: '2026-05-26T00:00:00Z',
              passport_hash: 'passport-hash-0001',
              available_actions: [],
            },
            description: null,
            merge_passport: {
              status: 'blocked',
              head_sha: OLD_SHA,
              blockers: [
                {
                  code: 'passport_blocked_checks',
                  message: 'Required checks failing.',
                  details: null,
                },
              ],
              evaluated_at: '2026-05-26T00:00:00Z',
            },
            passport_hash: 'passport-hash-0001',
          }),
        });
      }
    );

    await page.goto(PR_URL);

    // The cockpit hydrates: the PR title heading + the exact-SHA approve CTA.
    await expect(
      page.getByRole('heading', { name: /PR #99: Approve happy path/i })
    ).toBeVisible({ timeout: 15_000 });
    const approve = approveButton(page);
    await expect(approve).toBeVisible();

    await approve.click();

    // Success: the sidebar reflects the new approval posture and NO recovery
    // banner appears (the banner only renders on a 409 drift).
    await expect(page.locator('.review-sidebar__approvals')).toHaveText(
      /1\/1 approvals/,
      { timeout: 10_000 }
    );
    await expect(page.locator('.pr-cockpit__recovery')).toHaveCount(0);
  });

  test('clicking Approve on a stale head surfaces the SHA-drift recovery banner', async ({
    page,
  }) => {
    await mockBootstrap(page);
    await mockRepoList(page, [{ id: REPO, default_branch: 'main' }]);
    await mockPullRequestDetail(page, {
      repoId: REPO_ID,
      number: PR_NUMBER,
      title: 'Approve stale path',
      head_sha: OLD_SHA,
      passport: 'blocked',
    });
    // The approve POST returns 409 merge_sha_stale with expected/current SHA.
    await forceDriftSha(page, OLD_SHA, NEW_SHA);

    await page.goto(PR_URL);
    await expect(
      page.getByRole('heading', { name: /PR #99: Approve stale path/i })
    ).toBeVisible({ timeout: 15_000 });

    await approveButton(page).click();

    // The cockpit recovery banner appears (role="alert"), names the SHA
    // drift, and renders both the old + new short SHAs with a Refresh CTA.
    const banner = page.locator('.pr-cockpit__recovery');
    await expect(banner).toBeVisible({ timeout: 10_000 });
    await expect(banner).toHaveAttribute('role', 'alert');
    await expect(banner).toContainText(/Head SHA changed/i);
    await expect(banner.locator('code').first()).toHaveText(OLD_SHA.slice(0, 7));
    await expect(banner.locator('code').nth(1)).toHaveText(NEW_SHA.slice(0, 7));
    await expect(banner.getByRole('button', { name: /Refresh/i })).toBeVisible();
  });
});
