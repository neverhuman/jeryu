# Rendered UX QA Evidence

This file records the rendered UX proof surface for `apps/web`.

## Required lanes

- Storybook state coverage for loading, empty, error, success, and permission-denied states.
- Playwright screenshot capture with `page.screenshot`, `locator.screenshot`, `artifactPath`, and `ariaSnapshot`.
- Visual review or geometry runtime checks via `@jankurai/ux-qa`, `getBoundingClientRect`, and edge-clearance / target-size assertions.
- Accessibility automation with `axe-core`, `pa11y`, and `storybook-addon-a11y`.
- Layout stability checks with `web-vitals`, CLS, and Lighthouse.
- Generated API mocks with MSW or Orval.
- Design token discipline through `tokens/` and `style-dictionary`.
- Artifact-backed proof receipts in `ux-qa-artifacts/`, `playwright-report/`, and `test-results/`.

## Harness (W-T-19)

`ux-qa-check.mjs` is a real proof collector. Per-run it verifies the following
checks and writes a JSON receipt to `target/jankurai/ux-qa/web-forge.<ISO>.json`
plus a sibling `web-forge.latest.json` symlink-equivalent for downstream tools.

| Check | Verifies |
|---|---|
| `vite_build` | `apps/web/dist/index.html` and `apps/web/dist/assets/` exist. |
| `storybook_build` | `apps/web/storybook-static/index.html` exists. |
| `playwright_report` | `apps/web/playwright-report/index.html` exists. |
| `axe_scans` | Any `*.axe.json` / `playwright-axe-*.json` artifact under `target/jankurai/ux-qa/` has zero `critical` or `serious` violations. |
| `markdown_xss` | `cargo nextest run -p jeryu --test web_markdown_tests` passes; receipt written to `target/jankurai/ux-qa/markdown-xss.json`. |
| `ws_replay` | Playwright HTML report references the `08-ws-reconnect` spec. |
| `bundle_size` | Total gzipped size of every `dist/assets/*.js` file is below 350 KB. |

Each check produces a `{ name, pass, details }` entry in the receipt. The
top-level `pass` is true if and only if every check passes. The harness exits
`0` on full pass; non-zero exit codes surface in CI as a hard failure with a
human-readable summary printed to stderr.

If a check fails because the upstream artifact is not yet produced (e.g. the
Playwright report is missing because the e2e suite did not run), the harness
still writes the receipt with `pass: false` plus a `reason:` and a `hint:`
indicating which command to run next.

## Storybook story coverage (W-T-07)

The Storybook build emits stories for the components in the §6.14 matrix:

- `RepoCard` — healthy / warning / critical / archived / private.
- `ReadmePanel` — loading / empty / rendered / malicious-HTML-sanitized.
- `DiffViewer` — small / huge / binary / generated / with-comments.
- `MergeGatePanel` — pass / blocked / stale-SHA / approval-required / agent-evidence.
- `SettingsDiffPreview` — safe / reversible / irreversible / production-impact.
- `RiskBadge` — low / medium / high / critical.
- `CommandPalette` — closed / open-empty / open-typing / open-many.

The addon-a11y panel scans every story; critical / serious violations are
surfaced in the panel and gate CI when CI runs `storybook test-runner`.

## Perf budget (W-T-20)

`apps/web/perf/lighthouse.config.js` plus `lighthouse-budget.json` define the
Lighthouse CI run executed by `npm --workspace @jeryu/web run perf`. The
budgets mirror Appendix D in `WEB_WORK_CLAUDE.md`:

| Resource type | Budget (KB) |
|---|---:|
| script | 358 |
| stylesheet | 30 |
| image | 100 |
| document | 18 |
| total | 600 |

Timing thresholds:

| Metric | Budget (ms) |
|---|---:|
| first-contentful-paint | 1500 |
| interactive | 3000 |
| speed-index | 2000 |

If `@lhci/cli` is not installed (lockfile-only environment), install it with
`npm install --workspace @jeryu/web @lhci/cli@latest` and re-run the script.
