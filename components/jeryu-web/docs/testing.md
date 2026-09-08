# Testing

Use the local CI entrypoints before pushing changes:

- `just fast`
- `just check`
- `just score`
- `just security`
- `just artifact-support`

The protected entrypoint is `scripts/ci-local.sh <lane>`, where `<lane>` is
exactly one of `required`, `security`, `score`, `contract-drift`, or
`artifact-support`. It anchors to the physical repository root, delegates once
to the canonical lane, and rejects missing, extra, unknown, option-shaped,
path-shaped, newline, and injection-shaped inputs before delegation.
`tests/ci_local_dispatch_test.sh` proves the mapping, foreign-working-directory
behavior, rejection matrix, and child exit-status propagation.

`just check` validates `package-lock.json` against the root `package.json` and
both literal workspace manifests, including lockfile version, root dependency
maps, workspace inventory, and workspace package names. Its hostile Node tests
cover malformed and mismatched lock data, hostile paths, and symlink containment.
The security wrapper accepts no arguments, pins its interpreter, resolves its
target from a foreign working directory, delegates once, and preserves the child
status; `ops/ci/security-wrapper-self-test.sh` proves that contract and verifies
distinct root-workspace and `apps/web` npm audits without writing canonical
security evidence.

For web UI changes, the required browser lane is the mocked Chromium action
matrix:

- `npm --workspace @jeryu/web run test:e2e:ci`
- `npm --workspace @jeryu/web run ux-qa`

`test:e2e:ci` runs `test:e2e:actions` in `JERYU_PLAYWRIGHT_E2E_MODE=ui-only`
and then verifies `apps/web/playwright-report/junit.xml` against
`apps/web/e2e/action-matrix.json`. Tests tagged `@bff` are live edge smoke and
are intentionally excluded from the action matrix; run them with
`npm --workspace @jeryu/web run test:e2e:bff` only when the BFF workspace is
available.

The `contract-drift` lane runs the existing `@jeryu/web` `tsd` contract suite
through `ops/ci/contract-drift.sh`. `scripts/ci-doctor.sh` checks the required
local tools.

Agent-readable exception guidance:

- purpose: every typed error documents the caller-facing failure purpose
- reason: failures preserve enough context for local diagnosis
- common fixes: map repeated failures to a small set of operator repairs
- docs_url: point users to this file or a narrower runbook
- repair_hint: state the next command or config change to try

Cost and bounded-operation policy: budget, quota, spend cap, kill switch, and
stop condition evidence must be added before introducing paid or unbounded
network operations.
