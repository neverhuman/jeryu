// Auth fixture for Playwright (W-T-08).
//
// Phase 2 boots the BFF with `JERYU_WEB_TRUST_LOCAL=1` (see
// `playwright.config.ts` webServer command), which lets the BFF skip the
// `__Host-jeryu-session` cookie check and attach a local-dev `Viewer`
// carrying every canonical permission (see `src/web/auth.rs`).
//
// `setupTrustedSession(context)` is a no-op against that mode; it exists so
// a CI session cookie can later replace the trust-local short-circuit
// without touching call sites. It also injects a fixed session cookie so
// manual probes at `http://127.0.0.1:5173` exercise the same
// session-present codepath.

import type { BrowserContext, Page } from '@playwright/test';

/** Cookie name used by the BFF for session pinning (see §35.1.18). */
export const SESSION_COOKIE_NAME = '__Host-jeryu-session';

/**
 * Inject a fixed session cookie so the BFF auth middleware takes the
 * session-cookie code path (rather than the trust-local fallback). The
 * value is a fixed constant because the BFF accepts ANY non-empty cookie
 * value when `JERYU_WEB_TRUST_LOCAL=1`; the real session-table lookup
 * ships with the W-B-* auth service.
 */
export async function setupTrustedSession(context: BrowserContext): Promise<void> {
  // Cookies under `__Host-` MUST be Secure; in dev (http://127.0.0.1:5173)
  // Playwright still accepts the cookie when the SameSite policy is Lax
  // and the cookie is host-only. We omit Secure=true so the cookie sticks
  // on the http baseURL the dev server exposes.
  await context.addCookies([
    {
      name: SESSION_COOKIE_NAME,
      value: 'fixed-e2e-session',
      domain: '127.0.0.1',
      path: '/',
      httpOnly: true,
      secure: false,
      sameSite: 'Lax',
    },
  ]);
}

/**
 * Convenience wrapper for tests that only need a page: ensures the
 * session cookie is present before `page.goto(...)`.
 */
export async function withTrustedSession(page: Page): Promise<void> {
  await setupTrustedSession(page.context());
}
