# Local CI Parity

`scripts/ci-parity.sh` is the canonical full CI parity gate. It mirrors the
GitHub workflow command set closely enough to use as the final local check
before opening a PR.

`scripts/ci-local.sh` remains the lane-level helper when you want to rehearse
one CI slice at a time.

## Before PR

- `just ci-parity`
- `scripts/ci-parity.sh --fast` for the faster local mirror when you want to
  skip the slow integration-style checks

## Lane Helper

- `scripts/ci-local.sh doctor`
- `scripts/ci-local.sh rust fmt`
- `scripts/ci-local.sh rust clippy`
- `scripts/ci-local.sh rust build`
- `scripts/ci-local.sh rust test-lib`
- `scripts/ci-local.sh rust test-integration`
- `scripts/ci-local.sh rust hardening`
- `scripts/ci-local.sh security`
- `scripts/ci-local.sh release-ready`
- `scripts/ci-local.sh release-preflight <version>`

## Canonical Scripts

- Rust workflow stages: `ops/ci/rust-lane.sh`
- Jankurai audit and security stages: `ops/ci/jankurai-lane.sh`
- Release readiness: `ops/ci/release-ready-lane.sh`
- Release pipeline stages: `ops/ci/release-lane.sh`

Workflow-only shell is not allowed. New CI behavior must land in `ops/ci/`
first, then the workflow can call it.

## Hardening

`scripts/ci-local.sh rust hardening` runs the shared scheduled hardening lane:
`cargo semver-checks check-release` followed by AER. The AER findings are
written to `target/hardening/aer-findings.json` for local proof and full
parity artifacts.

Semver uses `origin/main` as the default baseline for unpublished local crates;
set `SEMVER_BASELINE_REV=<rev>` to compare against a different Git revision.

`scripts/ci-parity.sh` includes hardening in full mode. `scripts/ci-parity.sh
--fast` skips it and prints `scheduled hardening skipped in fast mode`.
