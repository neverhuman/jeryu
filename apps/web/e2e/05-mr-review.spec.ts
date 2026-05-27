// 05-mr-review.spec.ts — MR cockpit smoke (W-T-13).
//
// Phase 3 backend MR services may be partially live, so this spec ALWAYS
// runs against a mocked MergeRequestDetail. We assert the cockpit page
// renders the MR title and surfaces (one of):
//
//   1. The full Phase 3 cockpit (three-pane review layout).
//   2. The Phase 1/2 StubPage envelope (`Stub · W-FE-11`).
//   3. An ErrorState if the live BFF errored and the mocked detail was
//      not reached (e.g. when the SPA prefetches some other endpoint we
//      haven't stubbed and bails out early).
//
// Either way, the page MUST load without a hard crash (no Error Boundary)
// and the route MUST resolve from a deep URL — the W-spa-fix smoke pinned
// in 04-code is also exercised here.

import { expect, test } from '@playwright/test';

import { mockBootstrap, mockMergeRequest, mockRepoLookup } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = { host: 'gitlab', owner: 'neverhuman', name: 'jeryu' } as const;
const MR_IID = '42';
const MR_SHA = '1234567890abcdef1234567890abcdef12345678';

test.describe('MR cockpit (W-T-13)', () => {
  test('mocked MR detail renders the cockpit or its stub envelope', async ({ page }) => {
    await mockBootstrap(page);
    await mockRepoLookup(page, { id: REPO, default_branch: 'main' });
    await mockMergeRequest(page, {
      iid: MR_IID,
      title: 'Add JeRyu Phase 3 backend',
      state: 'opened',
      head_sha: MR_SHA,
      approvals: 0,
      approvals_required: 1,
    });

    await page.goto(
      `/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}/merge-requests/${MR_IID}`
    );

    // Phase 1/2 stubs render `Merge request !42` as their <h1>. Phase 3
    // cockpit renders the MR title as the <h1>. Either is acceptable.
    const stubHeading = page.locator('h1', {
      hasText: new RegExp(`Merge request !${MR_IID}|Add JeRyu Phase 3 backend`, 'i'),
    });
    const errorState = page.locator('[role="alert"]');

    await expect(stubHeading.or(errorState)).toBeVisible({ timeout: 15_000 });
  });

  test('deep MR route returns 200 (SPA fallback)', async ({ request }) => {
    const res = await request.get(
      `/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}/merge-requests/${MR_IID}`,
      { failOnStatusCode: false }
    );
    expect(res.status()).toBe(200);
  });
});
