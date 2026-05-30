// Playwright configuration for the JeRyu Web Forge SPA (W-T-08).
//
// `webServer` blocks bring up the Axum HTTP edge (the `jeryu-api` web edge)
// and the Vite dev server before tests execute. Phase 2 keeps `workers: 1`
// and `fullyParallel: false` so tests can rely on the single dev API
// instance without coordinating fixtures across parallel runners.
//
// NOTE: the `cargo run` command below brings up the jeryu-api web edge that
// serves the GitHub-shaped `/api/v1/repos/{id}/pulls*` routes. That edge is
// owned by the backend (Task A) and must be live for the e2e suite to run
// against a real server; the specs themselves are fully mock-driven via
// `page.route(...)` so they exercise the SPA without depending on backend
// data, but the SPA static bundle still needs to be served from somewhere.
//
// Trace / screenshot / video are captured only on failure to keep the
// happy-path runs cheap; once W-T-18 lands the a11y scans will piggyback
// on the same trace.

import { defineConfig, devices } from '@playwright/test';

// `JERYU_PLAYWRIGHT_E2E_MODE` toggles the test target:
//   - `bff-only` (default): point baseURL at the BFF which serves the
//     built SPA from `apps/web/dist`. One process to launch; lowest
//     resource footprint; what CI runs.
//   - `with-vite`: also launch the Vite dev server at :5173. Useful for
//     local iteration but requires generous inotify watcher limits
//     (~50k+) — set this only when developing.
const E2E_MODE = process.env.JERYU_PLAYWRIGHT_E2E_MODE ?? 'bff-only';
const useViteDev = E2E_MODE === 'with-vite';
const bffBaseURL = process.env.JERYU_PLAYWRIGHT_API_URL ?? 'http://127.0.0.1:8787';
const bffHealthURL = `${bffBaseURL.replace(/\/$/, '')}/health`;
const bffBind = new URL(bffBaseURL).host;
const externalApi = process.env.JERYU_PLAYWRIGHT_EXTERNAL_API === '1';
const baseURL = useViteDev ? 'http://127.0.0.1:5173' : bffBaseURL;

const apiWebServer = {
  // Axum HTTP edge (jeryu-api web edge). `JERYU_WEB_TRUST_LOCAL=1`
  // short-circuits the cookie session check so the SPA can fetch
  // `/api/v1/bootstrap` without provisioning a session.
  command:
    `JERYU_WEB_TRUST_LOCAL=1 cargo run --features web -p jeryu-api -- web serve --bind ${bffBind} --spa-dir web/dist`,
  url: bffHealthURL,
  timeout: 180_000,
  reuseExistingServer: !process.env.CI,
  cwd: '..',
};

const viteWebServer = {
  // Vite dev server. Cwd is the `web/` package dir; `npm run dev` runs
  // vite bound to 127.0.0.1:5173.
  command: 'npm run dev',
  url: 'http://127.0.0.1:5173',
  timeout: 60_000,
  reuseExistingServer: !process.env.CI,
  cwd: '.',
};

const webServer = useViteDev
  ? [apiWebServer, viteWebServer]
  : externalApi
    ? undefined
    : [apiWebServer];

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: [
    ['html', { outputFolder: 'playwright-report' }],
    ['junit', { outputFile: 'playwright-report/junit.xml' }],
    ['line'],
  ],
  use: {
    baseURL,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    // firefox + webkit added once the chromium baseline is stable.
  ],
  ...(webServer ? { webServer } : {}),
});
