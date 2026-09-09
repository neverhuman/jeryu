# Audit Rubric

`jeryu-web` audits prioritize evidence over claims.

## Required shape

- Source paths are mapped in `agent/owner-map.json`.
- Test paths are mapped in `agent/test-map.json`.
- Generated files are declared in `agent/generated-zones.toml`.
- Boundaries are named in `agent/boundaries.toml` (see `docs/boundaries.md`).

## Proof lanes

The `jeryu-web/required` check runs `ops/ci/pr-ci.sh`: the fast and full check
lanes (typecheck), the enforcing jankurai score (`ops/ci/score.sh`, floor 85),
the security lane, unit tests, the `tsd` contract-drift lane, the production
build, the ui-only Playwright action lane, the Storybook build, and the
rendered UX QA lane (Playwright + axe + Lighthouse).

## Language rule

Keep product copy and comments free of retired-provider vocabulary; the SPA is
a client for a local GitHub-compatible forge. A vocabulary guard unit test
protects the user-facing surface.
