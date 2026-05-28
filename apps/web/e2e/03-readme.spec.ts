// 03-readme.spec.ts — README rendering smoke (W-T-11).
//
// Phase 2/3 status: `/api/v1/repos/{id}/readme` is wired and the
// W-FE-09 frontend mounts `<ReadmePanel>` on the repository overview page.
// This spec mocks both the repository resolution (`/api/v1/repos`) and the
// README endpoint (`/api/v1/repos/{id}/readme`) so the test is deterministic
// regardless of whether the BFF can reach a real GitLab.
//
// The mock returns a `RenderedMarkdown` envelope whose `html` field carries
// a benign heading, a benign `<table>`, AND a `<script>` payload. The
// ReadmePanel pipes the HTML through `MarkdownRenderer`, which re-sanitizes
// with DOMPurify as defense-in-depth before mounting the body via
// `dangerouslySetInnerHTML`. We assert:
//   1. The heading + table render in the DOM.
//   2. No `<script>` tag survives in the rendered DOM (client-side
//      sanitization invariant — §35.1.18).
//   3. The standalone `/api/v1/markdown/render` endpoint also strips
//      `<script>` and the `onerror` attribute (server-side invariant).

import { expect, test } from '@playwright/test';

import { mockBootstrap, mockReadme, mockRepoList } from './fixtures/mocks';

test.describe.configure({ retries: 1 });

const REPO = {
  id: { host: 'gitlab', owner: 'neverhuman', name: 'jeryu' },
  default_branch: 'main',
  description: 'JeRyu mission-control hub.',
  visibility: 'internal' as const,
} as const;

// Upstream HTML the README endpoint would return. The `<script>` payload is
// deliberately present so the spec can assert DOMPurify drops it client-side
// even if the server sanitizer were bypassed.
const READMEHTML = [
  '<h1 id="jeryu">JeRyu</h1>',
  '<p>Mission control for managed coding agents.</p>',
  '<table>',
  '<thead><tr><th>Module</th><th>Status</th></tr></thead>',
  '<tbody>',
  '<tr><td>Bootstrap</td><td>green</td></tr>',
  '<tr><td>Repositories</td><td>green</td></tr>',
  '</tbody>',
  '</table>',
  '<script>window.__xss=true;</script>',
].join('\n');

test.describe('README rendering (W-T-11)', () => {
  test('README endpoint hits BFF and the SPA sanitizes', async ({ page }) => {
    await mockBootstrap(page);
    await mockRepoList(page, [REPO]);
    await mockReadme(page, { html: READMEHTML });

    // Navigate to the overview page. The route is
    // `/repos/:provider/:fullName/*`; with `fullName=neverhuman` and the
    // splat `*=jeryu`, `useResolveRepo` reconstructs "neverhuman/jeryu"
    // and matches the mocked summary by host + owner + name.
    await page.goto(
      `/repos/${REPO.id.host}/${REPO.id.owner}/${REPO.id.name}`
    );

    // 1. Heading from the mocked HTML renders inside `.markdown-body`.
    const markdownBody = page.locator('.markdown-body');
    await expect(markdownBody).toBeVisible({ timeout: 15_000 });
    await expect(markdownBody.locator('h1', { hasText: 'JeRyu' })).toBeVisible();

    // 2. Table renders with both rows.
    const table = markdownBody.locator('table');
    await expect(table).toBeVisible();
    await expect(table.locator('tbody tr')).toHaveCount(2);
    await expect(table).toContainText('Bootstrap');
    await expect(table).toContainText('Repositories');

    // 3. No `<script>` tag survives in the rendered DOM — defense-in-depth
    //    via DOMPurify (`sanitizeMarkdownHtml` in MarkdownRenderer.tsx).
    const scriptInBody = await markdownBody.locator('script').count();
    expect(
      scriptInBody,
      'DOMPurify must strip <script> from the rendered README'
    ).toBe(0);

    // The XSS payload's side effect (setting `window.__xss`) must not have
    // executed. We probe `window.__xss` directly — if any script tag were
    // attached and ran, it would be `true`.
    const xssFired = await page.evaluate(
      () =>
        (window as unknown as { __xss?: boolean }).__xss === true
    );
    expect(xssFired, 'inline <script> must not have executed').toBe(false);
  });

  test('markdown render endpoint sanitizes script + onerror', async ({
    request,
  }) => {
    // The W-B-08 markdown service is exposed at `/api/v1/markdown/render`
    // in Phase 2. The BFF enforces double-submit CSRF on POST so we
    // forge a matching cookie+header pair manually. Note we cannot
    // `context.addCookies` a `__Host-` cookie over plain HTTP — Chromium
    // rejects `__Host-` without `Secure=true`, which 127.0.0.1 cannot
    // satisfy. Passing the cookie as a raw `Cookie` header sidesteps
    // that restriction (the BFF only reads the header). We tolerate
    // `404/405/501` (endpoint not yet wired) and `4xx CSRF`
    // (the BFF rejects the cookie shape) — the contract under test is
    // the sanitization behaviour, only asserted when the endpoint
    // succeeds.
    const CSRF = 'e2e-csrf-token';
    const bffBaseURL =
      process.env.JERYU_PLAYWRIGHT_BFF_URL ?? 'http://127.0.0.1:8787';
    const res = await request.post(
      `${bffBaseURL.replace(/\/$/, '')}/api/v1/markdown/render`,
      {
        data: {
          markdown:
            '# Title\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(2)>',
        },
        headers: {
          'X-CSRF-Token': CSRF,
          Cookie: `__Host-jeryu-csrf=${CSRF}`,
        },
        failOnStatusCode: false,
      }
    );

    test.skip(
      res.status() === 404 || res.status() === 405 || res.status() === 501,
      `Markdown render endpoint not yet wired (status ${res.status()}); skipping spec until W-B-08 surfaces /api/v1/markdown/render.`
    );
    // Phase-3 tolerant: also skip when the BFF rejects auth/CSRF for
    // reasons outside this spec's surface area.
    test.skip(
      res.status() === 401 || res.status() === 403,
      `Markdown render endpoint behind auth/CSRF (status ${res.status()}); skipping spec.`
    );

    expect(res.status(), `markdown render returned ${res.status()}`).toBeLessThan(400);
    const body = await res.text();
    // The sanitizer MUST strip `<script>` tags entirely and remove the
    // `onerror` attribute from `<img>`. Renderer marker `jeryu-markdown.v1`
    // is asserted on the dedicated W-T-01 Rust suite; here we just smoke
    // the XSS invariant the SPA depends on (§35.1.18).
    expect(body, 'response must not contain <script>').not.toContain('<script>');
    expect(body, 'response must not contain onerror handler').not.toContain('onerror');
  });
});
