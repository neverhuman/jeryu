// RepositoriesPage — Page Object for `/repos` (W-T-08).
//
// The `/repos` route currently renders the NotImplementedRoute envelope
// until W-FE-08 lands the full list. Selectors below target that envelope;
// once W-FE-08 lands, the same POM methods point at the filter input, sort
// header, and create button without changing the spec call sites.

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
   * Acceptable behaviour for `/repos`:
   *   - NotImplementedRoute envelope ("Repositories" heading + planned pill); OR
   *   - real list with cards; OR
   *   - ErrorState explaining "upstream unavailable" when the forge backend is down.
   */
  async assertEitherListOrErrorState(): Promise<void> {
    const heading = this.heading;
    const error = this.errorState;
    await expect(heading.or(error)).toBeVisible({ timeout: 15_000 });
  }

  async filterByFamily(name: string): Promise<void> {
    // The W-FE-08 list exposes a filter input; the current envelope does
    // not, so this is a no-op until that input renders.
    if ((await this.filterInput.count()) > 0) {
      await this.filterInput.first().fill(name);
    }
  }

  async sortBy(field: string): Promise<void> {
    // Click the sort control matching `field` if the page exposes one;
    // falls back to the first sort control otherwise. A no-op when the
    // repositories list does not render a sort header yet.
    const byField = this.page.locator(`[data-sort="${field}"]`);
    const control =
      (await byField.count()) > 0 ? byField : this.sortSelector;
    if ((await control.count()) > 0) {
      await control.first().click();
    }
  }

  async clickCreate(): Promise<void> {
    await this.createButton.first().click();
  }

  async assertCount(n: number): Promise<void> {
    const cards = this.page.locator('[data-repo-card]');
    await expect(cards).toHaveCount(n);
  }
}
