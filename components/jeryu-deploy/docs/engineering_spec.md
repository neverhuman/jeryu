# Jeryu Engineering Spec

This document is the engineering overview for the standalone Deploy Rust
workspace. It records product invariants represented by checked-in source,
immutable dependency releases, scripts, and local verification gates.

## Core Invariants

- This workspace owns exactly `jeryu-api`, `jeryu-cli`, and
  `jeryu-split-tool`; other Jeryu components are immutable Git dependencies
  owned and released by their standalone repositories.
- Runtime-facing commands stay under the `jeryu` product surface while service
  internals use Jeryu components.
- Cache correctness beats cache hit rate.
- CI inputs are native Jeryu TOML, GitHub Actions workflows, API-created
  runs, scheduled runs, agent dry runs, hotfix runs, release runs, and
  merge-queue synthetic runs.
- Release paths use hermetic cache policy, provenance receipts, checksums, and
  signed witnesses.
- Agent writes require scoped capability checks, proof receipts, and auditable
  mutation records.

## Current Workspace Scope

- `jeryu-api` owns Deploy's forge API and integration surface.
- `jeryu-cli` owns Deploy's user-facing binaries and client dispatch.
- `jeryu-split-tool` owns the local manifest, workflow-parity, and affected-plan
  commands.
- Core, CI, runner, cache, proof, agent, and signing components enter through
  immutable dependency pins. Their source gates stay in their owning repos;
  Deploy gates only the integrations it actually ships.

## Acceptance Baseline

The foundation gate for this workspace is:

- `cargo fmt --all --check`
- `cargo check --workspace --all-targets`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `bash ops/ci/proof-evidence.sh`
- `bash ops/ci/workflow-lint.sh`
- `bash scripts/test-emit-release-receipt.sh`
- `bash ops/ci/score.sh`
- `bash ops/ci/security.sh`
- `bash ops/ci/web.sh`
- `bash scripts/ci-phases.sh`
