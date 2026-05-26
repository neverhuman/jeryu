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
