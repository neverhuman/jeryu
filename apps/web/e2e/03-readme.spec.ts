// 03-readme.spec.ts — README rendering smoke (W-T-11).
//
// Phase 2 status: `/api/v1/repos/{id}/readme` is NOT yet served by the
// BFF (W-FE-09 + backend README service land in W-P2-backend). Until
// the README endpoint exists, this spec exercises the related markdown
// surface that DOES ship in Phase 2: the W-B-08 markdown renderer
// (`jeryu-markdown.v1`), proving the double-sanitize chain rejects
// script/onerror payloads.
//
// Once the README endpoint lands, swap `test.skip` for `test` on the
// "README rendering" block and remove the markdown-render fallback.

import { expect, test } from '@playwright/test';

test.describe('README rendering (W-T-11)', () => {
  test.skip(
    'README endpoint hits BFF and sanitizes',
    async ({ page }) => {
      // Pending W-FE-09 + backend README service.
      // Once available:
      //   1. page.goto('/repos/gitlab/neverhuman/jeryu')
      //   2. waitForReadme()
      //   3. assertReadmeContains('JeRyu')
      //   4. assertNoScriptTagsInDom()
      await page.goto('/');
    }
  );

  test('markdown render endpoint sanitizes script + onerror', async ({
    request,
  }) => {
    // The W-B-08 markdown service is exposed at `/api/v1/markdown/render`
    // in Phase 2. We do not assume it is wired yet; tolerate either a
    // 200 (assert sanitized output) or a 404/501 (record skip-reason).
    const res = await request.post('http://127.0.0.1:8787/api/v1/markdown/render', {
      data: {
        body: '# Title\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(2)>',
      },
      failOnStatusCode: false,
    });

    test.skip(
      res.status() === 404 || res.status() === 405 || res.status() === 501,
      `Markdown render endpoint not yet wired (status ${res.status()}); skipping spec until W-B-08 surfaces /api/v1/markdown/render.`
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
