# Architecture

`jeryu-ci-runner` is part of the Jeryu split family.

The public portal is `neverhuman/jeryu`. Release authority remains
`neverhuman/jeryu-deploy`; split member repositories own bounded product
surfaces and consume sibling crates from pinned public Git tags.

## Boundaries

- Profile: `rust-workspace`
- Required check: `jeryu-ci-runner/required`
- Local release source of truth: `agent/boundaries.toml`

## Owned Surface

- `crates/jeryu-ci-ir/**`
- `crates/jeryu-ci-compiler/**`
- `crates/jeryu-ci-scheduler/**`
- `crates/jeryu-runner-core/**`
- `crates/jeryu-runner-native/**`
- `crates/jeryu-runner-microvm/**`
- `crates/jeryu-runner-oci/**`
- `crates/jeryu-runner-protocol/**`
- `crates/jeryu-runner-registry/**`
- `crates/jeryu-runnerd/**`
- `crates/jeryu-sandbox-linux/**`
- `crates/jeryu-agentbridge/**`
- `crates/jeryu-agent-auth/**`
- `crates/jeryu-agent-stream/**`
- `crates/jeryu-egress/**`
- `crates/jeryu-artifact-metadata/**`
- `crates/jeryu-cache-policy/**`
- `crates/jeryu-ci-governor/**`
- `crates/jeryu-phase7-cli/**`
- `bins/jeryu-ci-bin/**`
- `tests/fixtures/github/**`
- `tests/fixtures/native/**`
- `tests/sandbox_escape_matrix.sh`
- `examples/jobs/**`
- `ops/agent-sandbox/**`
- `images/agent-sandbox/**`
- `policies/**`
- `configs/runnerd.toml`
