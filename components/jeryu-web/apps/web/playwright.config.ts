// Playwright configuration for the JeRyu Web Forge SPA (W-T-08).
//
// `webServer` blocks bring up either Vite alone (mocked UI action matrix) or
// the Axum HTTP edge (targeted live BFF smoke). The default is UI-only so an
// isolated jeryu-web checkout does not need the sibling jeryu-deploy workspace.
//
// NOTE: BFF mode runs `cargo run` for the jeryu-api web edge that serves REST
// routes and `/api/v1/ws`. Keep that mode for `@bff` smoke only; matrix-tagged
// UI tests must mock `/api/v1/*` and tolerate/abort WS locally.
//
// Trace / screenshot / video are captured only on failure to keep the
// happy-path runs cheap; the a11y scans (`e2e/10-a11y.spec.ts`) reuse the
// same trace artifacts.

import { defineConfig, devices } from '@playwright/test';

// `JERYU_PLAYWRIGHT_E2E_MODE` toggles the test target:
//   - `ui-only` (default): launch Vite on a dedicated strict port.
//     Matrix-tagged tests mock all API and WS traffic and are safe in
//     isolated jeryu-web CI.
//   - `bff-only`: point baseURL at the BFF, which serves the built SPA from
//     `web/dist`. Use this only for `@bff` live edge smoke.
//   - `with-vite`: launch both BFF and Vite for local debugging against a
//     live edge with the dev bundle.
//   - `ui-mocked`: serve built `dist` via Vite preview for CI action coverage.
const E2E_MODE = process.env.JERYU_PLAYWRIGHT_E2E_MODE ?? 'ui-only';
const useUiOnly = E2E_MODE === 'ui-only';
const useViteDev = E2E_MODE === 'with-vite';
const useUiMocked = E2E_MODE === 'ui-mocked';
const bffBaseURL = process.env.JERYU_PLAYWRIGHT_API_URL ?? 'http://127.0.0.1:8787';
const bffHealthURL = `${bffBaseURL.replace(/\/$/, '')}/health`;
const bffBind = new URL(bffBaseURL).host;
const externalApi = process.env.JERYU_PLAYWRIGHT_EXTERNAL_API === '1';
const previewBaseURL = 'http://127.0.0.1:4173';
const vitePort = process.env.JERYU_PLAYWRIGHT_WEB_PORT ?? '5175';
const viteBaseURL = `http://127.0.0.1:${vitePort}`;
const baseURL = useUiOnly || useViteDev
  ? viteBaseURL
  : useUiMocked
    ? previewBaseURL
    : bffBaseURL;
const apiCwd = process.env.JERYU_PLAYWRIGHT_API_CWD ?? '../../../jeryu-deploy';
const spaDir =
  process.env.JERYU_PLAYWRIGHT_SPA_DIR ?? '../jeryu-web/apps/web/dist';

const apiWebServer = {
  // Axum HTTP edge (jeryu-api web edge). `JERYU_WEB_TRUST_LOCAL=1`
  // short-circuits the cookie session check so the SPA can fetch
  // `/api/v1/bootstrap` without provisioning a session.
  command:
    `JERYU_WEB_TRUST_LOCAL=1 cargo run --features web -p jeryu-api -- web serve --bind ${bffBind} --spa-dir ${spaDir}`,
  url: bffHealthURL,
  timeout: 180_000,
  reuseExistingServer: !process.env.CI,
  cwd: apiCwd,
};

const viteWebServer = {
  // Vite dev server. Cwd is the `web/` package dir; `npm run dev` runs
  // Vite bound to a dedicated strict port so unrelated dev servers cannot
  // satisfy Playwright's readiness probe.
  command: `npm run dev -- --port ${vitePort} --strictPort`,
  url: viteBaseURL,
  timeout: 60_000,
  reuseExistingServer: !process.env.CI,
  cwd: '.',
};

const previewWebServer = {
  // Vite preview serves the already-built dist tree. The e2e lane builds
  // first, then starts this server with no backend proxy.
  command: 'npm run preview -- --strictPort',
  url: previewBaseURL,
  timeout: 60_000,
  reuseExistingServer: false,
  cwd: '.',
};

const webServer = useUiOnly
  ? [viteWebServer]
  : useViteDev
  ? [apiWebServer, viteWebServer]
  : useUiMocked
    ? [previewWebServer]
  : externalApi
    ? undefined
    : [apiWebServer];

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  ...(useUiOnly || useUiMocked ? { grepInvert: /@bff/ } : {}),
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
