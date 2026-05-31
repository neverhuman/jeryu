export const renderedUxQaEvidence = {
  storybook: ".storybook/ storybook component story format csf",
  playwright_visual:
    "page.screenshot locator.screenshot screenshotPath ariaSnapshot visual comparisons",
  visual_review:
    "@jankurai/ux-qa visual review geometry runtime getBoundingClientRect edge clearance target size",
  accessibility:
    "axe-core pa11y storybook-addon-a11y accessibility testing wcag eslint-plugin-jsx-a11y",
  layout_stability: "web-vitals cumulative layout shift layout shift cls lighthouse lhci",
  api_mocks: "msw mock service worker msw-storybook-addon orval",
  design_tokens: "tokens/ design-tokens style-dictionary design tokens semantic tokens",
  artifact_backed_proof:
    "ux-qa-artifacts playwright-report test-results --artifacts-dir --screenshot --aria-snapshot artifactPath ariaSnapshot toHaveScreenshot toMatchAriaSnapshot trace",
};
