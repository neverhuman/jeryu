// Lighthouse CI config for the JeRyu Web Forge SPA (W-T-20 / W-lhci).
//
// This config drives `lhci autorun` against the production bundle served by
// the JeRyu BFF (`jeryu web serve`). The BFF must already be running at
// `JERYU_LIGHTHOUSE_URL` (default: http://127.0.0.1:8787/) with
// `--spa-dir web/dist` when this config is used —
// `lhci collect` will not boot the server itself (we point at a live URL
// rather than a `staticDistDir` because Lighthouse otherwise spins up its
// own server which bypasses our BFF middleware/headers).
//
// Run:
//   1. `npm --workspace @jeryu/web run build`
//   2. `cargo build --release --features web -p jeryu`
//   3. `JERYU_WEB_TRUST_LOCAL=1 ./target/release/jeryu web serve --bind 127.0.0.1:8787 --spa-dir web/dist &`
//   4. `JERYU_LIGHTHOUSE_URL=http://127.0.0.1:8787/ npm --workspace @jeryu/web run perf`
//
// Assertions:
//   * Per-resource size budgets from `./lighthouse-budget.json`.
//   * `categories:performance >= 0.7` (warn).
//   * FCP <= 1500ms, TTI <= 3000ms, Speed Index <= 2000ms (warn).
//
// Artifacts (LHR JSON + HTML reports) are written to
// `<repo>/target/jankurai/ux-qa/lighthouse/`. The UX-QA harness in
// `apps/ux-qa/ux-qa-check.mjs` looks for `lhr-*.json` there.

const targetUrl = process.env.JERYU_LIGHTHOUSE_URL || 'http://127.0.0.1:8787/';

module.exports = {
  ci: {
    collect: {
      url: [targetUrl],
      numberOfRuns: 3,
      settings: {
        preset: 'desktop',
        chromeFlags: '--no-sandbox --headless=new',
      },
    },
    assert: {
      // NOTE: lhci forbids combining `budgetsFile` with inline `assertions`
      // ("Cannot use both budgets AND assertions"), so we encode the
      // per-resource size budgets from `lighthouse-budget.json` as inline
      // `resource-summary:*:size` assertions alongside the timing budgets.
      // The original budgets file is preserved for tooling that consumes it
      // directly (e.g. `lhci collect --upload.target=lhci`).
      assertions: {
        'categories:performance': ['warn', { minScore: 0.7 }],
        'categories:accessibility': ['warn', { minScore: 0.9 }],
        'first-contentful-paint': ['warn', { maxNumericValue: 1500 }],
        'interactive': ['warn', { maxNumericValue: 3000 }],
        'speed-index': ['warn', { maxNumericValue: 2000 }],
        // Resource size budgets (KB → bytes for lhci numeric assertions).
        'resource-summary:script:size': ['warn', { maxNumericValue: 358000 }],
        'resource-summary:stylesheet:size': ['warn', { maxNumericValue: 30000 }],
        'resource-summary:image:size': ['warn', { maxNumericValue: 100000 }],
        'resource-summary:document:size': ['warn', { maxNumericValue: 18000 }],
        'resource-summary:total:size': ['warn', { maxNumericValue: 600000 }],
      },
    },
    upload: {
      target: 'filesystem',
      outputDir: '../../target/jankurai/ux-qa/lighthouse',
    },
  },
};
