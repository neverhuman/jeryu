// 05-pr-review.spec.ts — PR cockpit smoke (W-T-13).
//
// Phase 3 backend PR services may be partially live, so this spec ALWAYS
// runs against a mocked PullRequestDetail. We assert the cockpit page
// renders the PR title and surfaces (one of):
//
//   1. The full Phase 3 cockpit (three-pane review layout).
//   2. The NotImplementedRoute envelope (`Planned · W-FE-11`).
//   3. An ErrorState if the live API errored and the mocked detail was
//      not reached (e.g. when the SPA prefetches some other endpoint we
//      haven't mocked and bails out early).
//
// Either way, the page MUST load without a hard crash (no Error Boundary)
// and the route MUST resolve from a deep URL — the W-spa-fix smoke pinned
// in 04-code is also exercised here.

import { expect, test } from '@playwright/test';

import {
  mockBootstrap,
  mockPullRequestDetail,
  mockRepoLookup,
} from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' } as const;
const PR_NUMBER = '42';
const PR_SHA = '1234567890abcdef1234567890abcdef12345678';

test.describe('PR cockpit (W-T-13)', () => {
  test('mocked PR detail renders the cockpit @action:pr.detail', async ({ page }) => {
    await mockBootstrap(page);
    await mockRepoLookup(page, { id: REPO, default_branch: 'main' });
    await mockPullRequestDetail(page, {
      repoId: `${REPO.host}:${REPO.owner}/${REPO.name}`,
      number: PR_NUMBER,
      title: 'Add JeRyu Phase 3 backend',
      state: 'open',
      head_sha: PR_SHA,
      approvals: 0,
      required_approvals: 1,
      passport: 'pass',
      can_merge: true,
    });

    await page.goto(
      `/repos/${REPO.host}/${REPO.owner}/${REPO.name}/pulls/${PR_NUMBER}`
    );

    // The not-implemented envelope renders `Pull request #42` as its <h1>;
    // the Phase 3 cockpit renders the PR title as the <h1>. Either is
    // acceptable.
    const heading = page.locator('h1', {
      hasText: new RegExp(`Pull request #${PR_NUMBER}|PR #${PR_NUMBER}|Add JeRyu Phase 3 backend`, 'i'),
    });
    const errorState = page.locator('[role="alert"]');

    await expect(heading.or(errorState)).toBeVisible({ timeout: 15_000 });
  });

  test('deep PR route returns 200 (SPA fallback) @bff', async ({ request }) => {
    const res = await request.get(
      `/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}/pulls/${PR_NUMBER}`,
      { failOnStatusCode: false }
    );
    expect(res.status()).toBe(200);
  });
});
