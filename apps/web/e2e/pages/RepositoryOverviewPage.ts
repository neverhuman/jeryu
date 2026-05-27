// RepositoryOverviewPage — Page Object for `/repos/{provider}/{owner}/{repo}` (W-T-08).
//
// Phase 2 overview page is the Phase 1 StubPage (W-FE-09 not yet landed).
// Once W-FE-09 lands, the same POM methods locate the README panel and
// branch selector.

import { expect, type Locator, type Page } from '@playwright/test';

export class RepositoryOverviewPage {
  readonly page: Page;
  readonly readmePanel: Locator;
  readonly readmeContent: Locator;

  constructor(page: Page) {
    this.page = page;
    this.readmePanel = page.locator('[data-readme-panel], .readme-panel');
    this.readmeContent = this.readmePanel.locator('article, .readme-html');
  }

  async goto(slug: string): Promise<void> {
    // `slug` is the path tail after `/repos/`, e.g.
    // `gitlab/neverhuman/jeryu`. Phase 2 router lives at
    // `/repos/:provider/:fullName*` (see `apps/web/src/app/router.tsx`).
    await this.page.goto(`/repos/${slug}`);
  }

  async waitForReadme(timeoutMs = 10_000): Promise<void> {
    await expect(this.readmePanel).toBeVisible({ timeout: timeoutMs });
  }

  async assertReadmeContains(text: string): Promise<void> {
    await expect(this.readmeContent).toContainText(text);
  }

  /**
   * XSS guard: the rendered DOM must never contain `<script>` tags or
   * event-handler attributes regardless of upstream README content. The
   * server sanitizer (ammonia, W-B-08) + client DOMPurify (jeryu-markdown.v1)
   * enforce this; spec asserts both layers stripped the offending nodes.
   */
  async assertNoScriptTagsInDom(): Promise<void> {
    const scriptCount = await this.readmePanel.locator('script').count();
    expect(scriptCount).toBe(0);
  }
}
