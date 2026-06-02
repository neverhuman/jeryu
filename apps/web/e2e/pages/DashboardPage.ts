// DashboardPage — Page Object for the root `/` dashboard (W-T-08).
//
// Phase 2 dashboard shows a heading + attention cards once the bootstrap
// resolves. Until W-FE-07 implements the full cards, the page renders the
// 5 demo state variants from `apps/web/src/pages/DashboardPage.tsx`.

import { expect, type Locator, type Page } from '@playwright/test';

export class DashboardPage {
  readonly page: Page;
  readonly heading: Locator;

  constructor(page: Page) {
    this.page = page;
    this.heading = page.locator('h1');
  }

  async goto(): Promise<void> {
    await this.page.goto('/');
  }

  /**
   * Assert the welcome heading mentions the bootstrap viewer's login.
   * Phase 1 dashboard renders the login in the UserMenu (top-right);
   * `login` is also surfaced in attention-card copy once W-FE-07 lands.
   */
  async assertWelcomeRendered(login: string): Promise<void> {
    // UserMenu shows the login as plain text inside the global header.
    await expect(this.page.locator('.global-header')).toContainText(login);
  }
}
