# Local CI Parity

`scripts/ci-local.sh` is the local entrypoint for CI-equivalent proof. GitHub
Actions jobs call the same `ops/ci/*.sh` scripts, so a lane can be rehearsed
before opening a PR.

## Required Commands

- `scripts/ci-local.sh doctor`
- `scripts/ci-local.sh rust fmt`
- `scripts/ci-local.sh rust clippy`
- `scripts/ci-local.sh rust build`
- `scripts/ci-local.sh rust test-lib`
- `scripts/ci-local.sh rust test-integration`
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
