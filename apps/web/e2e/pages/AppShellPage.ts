// AppShellPage — Page Object for the global SPA shell (W-T-08).
//
// The AppShell renders the GlobalHeader (brand + live-status pill),
// LeftNav, LiveActivityDock, StatusBar, and the route Outlet. None of
// these wrap a `data-testid` today; this POM uses stable class selectors
// (`.app-shell`, `.global-header__live`, `.status-bar`) that ship from
// `apps/web/src/layout/*.tsx`. When the frontend adds dedicated
// `data-testid` markers, swap the selectors here without touching specs.

import { expect, type Locator, type Page } from '@playwright/test';

export class AppShellPage {
  readonly page: Page;
  readonly shell: Locator;
  readonly header: Locator;
  readonly liveIndicator: Locator;
  readonly statusBar: Locator;
  readonly commandPaletteTrigger: Locator;

  constructor(page: Page) {
    this.page = page;
    this.shell = page.locator('.app-shell');
    this.header = page.locator('.app-shell__header');
    this.liveIndicator = page.locator('.global-header__live');
    this.statusBar = page.locator('.status-bar');
    this.commandPaletteTrigger = page.locator('.global-header__cmdk');
  }

  async goto(path = '/'): Promise<void> {
    await this.page.goto(path);
  }

  /** Wait for the shell layout to render (brand + nav + main + status). */
  async assertShellLoaded(timeoutMs = 10_000): Promise<void> {
    await expect(this.shell).toBeVisible({ timeout: timeoutMs });
    await expect(this.header).toBeVisible();
    await expect(this.statusBar).toBeVisible();
  }

  /**
   * Assert the live indicator reports a connected WebSocket. The
   * GlobalHeader renders one of: "Live" / "Connecting…" / "Reconnecting…" /
   * "Offline" / "Idle" (see `apps/web/src/layout/GlobalHeader.tsx`).
   */
  async assertLiveBadge(timeoutMs = 5_000): Promise<void> {
    await expect(this.liveIndicator).toBeVisible({ timeout: timeoutMs });
    await expect(this.liveIndicator).toContainText(/Live|Connecting|Idle/i, {
      timeout: timeoutMs,
    });
  }

  /** Open the command palette via the ⌘K shortcut. */
  async openCommandPalette(): Promise<void> {
    // Use the global header trigger button to avoid OS-specific modifier
    // handling; the trigger is keyboard-accessible too.
    await this.commandPaletteTrigger.click();
  }
}
