// RepositoriesPage — Page Object for `/repos` (W-T-08).
//
// Phase 2 Repositories page is the Phase 1 StubPage (W-FE-08 not yet
// landed). Selectors below target the stub envelope; once W-FE-08 lands
// the real implementation, the same POM methods point at the filter
// input, sort header, and create button.

import { expect, type Locator, type Page } from '@playwright/test';

export class RepositoriesPage {
  readonly page: Page;
  readonly heading: Locator;
  readonly filterInput: Locator;
  readonly sortSelector: Locator;
  readonly createButton: Locator;
  readonly errorState: Locator;

  constructor(page: Page) {
    this.page = page;
    this.heading = page.locator('h1', { hasText: /Repositories/i });
    this.filterInput = page.locator('input[name="filter"], input[placeholder*="filter" i]');
    this.sortSelector = page.locator('[data-sort]');
    this.createButton = page.locator('button', { hasText: /Create/i });
    // ErrorState component class from `components/state`.
    this.errorState = page.locator('.state, .error-state, [role="alert"]');
  }

  async goto(): Promise<void> {
    await this.page.goto('/repos');
  }

  /**
   * Phase 2 acceptable behaviour:
   *   - StubPage envelope ("Repositories" heading + stub pill); OR
   *   - real list with cards; OR
   *   - ErrorState explaining "upstream unavailable" when the forge backend is down.
   */
  async assertEitherListOrErrorState(): Promise<void> {
    const heading = this.heading;
    const error = this.errorState;
    await expect(heading.or(error)).toBeVisible({ timeout: 15_000 });
  }

  async filterByFamily(name: string): Promise<void> {
    // Reserved for W-FE-08; current stub does not expose a filter input.
    if ((await this.filterInput.count()) > 0) {
      await this.filterInput.first().fill(name);
    }
  }

  async sortBy(field: string): Promise<void> {
    // Reserved for W-FE-08.
    if ((await this.sortSelector.count()) > 0) {
      await this.sortSelector.first().click();
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const _ = field;
  }

  async clickCreate(): Promise<void> {
    await this.createButton.first().click();
  }

  async assertCount(n: number): Promise<void> {
    const cards = this.page.locator('[data-repo-card]');
    await expect(cards).toHaveCount(n);
  }
}
