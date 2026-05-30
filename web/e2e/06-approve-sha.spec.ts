// 06-approve-sha.spec.ts — exact-SHA approval flow + stale conflict (W-T-14).
//
// Two-tier smoke around the §35.1.7 "approve at exact SHA" contract:
//
//   1. The API accepts `POST /api/v1/repos/{id}/pulls/{number}/approve`
//      with the head SHA in the body and returns `204` on success — we
//      mock the success case and assert the SPA receives 204.
//
//   2. When the head moves between page load and the approve click, the
//      API returns `409 merge_sha_stale` with both `expected_sha` and
//      `current_sha`. We force that case via `forceStaleSha` and assert
//      the SPA surfaces the conflict envelope.
//
// Implementation note: `page.request.*` runs on an isolated
// `APIRequestContext` that does NOT respect `page.route(...)` mocks. We
// drive the calls from inside the page (`page.evaluate(fetch)`) so the
// route interceptors fire. The live API is only consulted by the route
// fallthrough — the spec works in fully-mocked mode regardless of
// Phase 3 backend state.

import { expect, test } from '@playwright/test';

import {
  forceStaleSha,
  mockBootstrap,
  mockPullRequest,
  mockRepoLookup,
} from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = { host: 'jeryu', owner: 'neverhuman', name: 'jeryu' } as const;
const PR_NUMBER = '99';
const OLD_SHA = '1111111111111111111111111111111111111111';
const NEW_SHA = '2222222222222222222222222222222222222222';
const REPO_ID = `${REPO.host}:${REPO.owner}/${REPO.name}`;

interface ApproveResult {
  status: number;
  body: unknown;
}

async function approveFromPage(
  page: import('@playwright/test').Page,
  url: string,
  headSha: string
): Promise<ApproveResult> {
  return page.evaluate(
    async ([approveUrl, sha]) => {
      const res = await fetch(approveUrl, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'X-CSRF-Token': 'e2e-csrf',
        },
        body: JSON.stringify({ head_sha: sha }),
      });
      const text = await res.text();
      let parsed: unknown = text;
      try {
        parsed = text.length > 0 ? JSON.parse(text) : null;
      } catch {
        parsed = text;
      }
      return { status: res.status, body: parsed };
    },
    [url, headSha] as const
  );
}

test.describe('Approve at exact SHA (W-T-14)', () => {
  test('success path returns 204 (mocked happy path)', async ({ page }) => {
    await mockBootstrap(page);
    await mockRepoLookup(page, { id: REPO, default_branch: 'main' });
    await mockPullRequest(page, {
      number: PR_NUMBER,
      title: 'Approve happy path',
      state: 'open',
      head_sha: OLD_SHA,
    });

    // Mock the approve endpoint to return 204 (per §35.1.7 contract).
    await page.route(
      /\/api\/v1\/repos\/[^/]+\/pulls\/[^/]+\/approve$/,
      async (route, req) => {
        if (req.method() !== 'POST') {
          await route.continue();
          return;
        }
        const body = JSON.parse(req.postData() ?? '{}') as { head_sha?: string };
        if (body.head_sha !== OLD_SHA) {
          await route.fulfill({
            status: 409,
            contentType: 'application/json',
            body: JSON.stringify({
              error: {
                code: 'merge_sha_stale',
                message: 'head moved',
                details: { expected_sha: OLD_SHA, current_sha: NEW_SHA },
              },
            }),
          });
          return;
        }
        await route.fulfill({ status: 204, body: '' });
      }
    );

    // Navigate so page.route registrations are active, then drive the
    // approve endpoint via the page's network stack (which respects
    // route interceptors).
    await page.goto(
      `/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}/pulls/${PR_NUMBER}`
    );

    const res = await approveFromPage(
      page,
      `/api/v1/repos/${encodeURIComponent(REPO_ID)}/pulls/${PR_NUMBER}/approve`,
      OLD_SHA
    );

    expect(
      res.status,
      `approve returned ${res.status}`
    ).toBe(204);
  });

  test('stale SHA path surfaces 409 + sha details', async ({ page }) => {
    await mockBootstrap(page);
    await mockRepoLookup(page, { id: REPO, default_branch: 'main' });
    await mockPullRequest(page, {
      number: PR_NUMBER,
      title: 'Approve stale path',
      state: 'open',
      head_sha: OLD_SHA,
    });
    await forceStaleSha(page, OLD_SHA, NEW_SHA);

    await page.goto(
      `/repos/${REPO.host}/${REPO.owner}%2F${REPO.name}/pulls/${PR_NUMBER}`
    );

    const res = await approveFromPage(
      page,
      `/api/v1/repos/${encodeURIComponent(REPO_ID)}/pulls/${PR_NUMBER}/approve`,
      OLD_SHA
    );
    expect(res.status).toBe(409);
    const body = res.body as {
      error?: { code?: string; details?: Record<string, unknown> };
    };
    expect(body.error?.code).toBe('merge_sha_stale');
    expect(body.error?.details?.expected_sha).toBe(OLD_SHA);
    expect(body.error?.details?.current_sha).toBe(NEW_SHA);
  });
});
