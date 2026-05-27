// Lighthouse CI config for the JeRyu Web Forge SPA (W-T-20).
//
// Run `npm --workspace @jeryu/web run perf` after `npm run build` to
// collect a Lighthouse pass against the production bundle and assert the
// per-resource budgets defined in `./lighthouse-budget.json`.
//
// Notes:
//   * `staticDistDir: 'dist'` points lhci at the Vite build output. lhci
//     spins up a static server on localhost automatically.
//   * `numberOfRuns: 1` keeps CI quick; bump to 3 if you need a median.
//   * `temporary-public-storage` is the default lhci upload target; swap
//     to `'filesystem'` if you want artifacts in `apps/web/.lighthouseci/`.
//
// If `@lhci/cli` is not installed locally (e.g. lockfile-only environment),
// run:
//
//     npm install --workspace @jeryu/web @lhci/cli@latest
//
// then re-run `npm --workspace @jeryu/web run perf`.

module.exports = {
  ci: {
    collect: {
      staticDistDir: 'dist',
      url: ['http://localhost/'],
      numberOfRuns: 1,
    },
    assert: {
      preset: 'lighthouse:recommended',
      assertions: {
        'first-contentful-paint': ['warn', { maxNumericValue: 1500 }],
        interactive: ['warn', { maxNumericValue: 3000 }],
      },
      budgetsFile: './lighthouse-budget.json',
    },
    upload: { target: 'temporary-public-storage' },
  },
};
