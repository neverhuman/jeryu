# Jeryu Engineering Spec

This document is the sanitized engineering overview for the fused Rust
workspace. It records product invariants that are currently represented by the
checked-in crates, scripts, and local verification gates.

## Core Invariants

- One workspace root owns every product crate and binary.
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

- `jeryu-core` and `jeryu-api` provide typed forge domain models and API
  facades.
- `jeryu-ci-ir`, `jeryu-ci-compiler`, `jeryu-ci-scheduler`, and `jeryu-ci-bin` provide CI compilation
  and scheduling foundations.
- `runner-*` crates define runner fabric and sandbox policy surfaces.
- `jeryu-cache-*` crates provide cache, CAS, quarantine, and receipt behavior.
- `jeryu-proof` and `jeryu-agentbridge` provide proof and agent-control foundations.
- `jeryu-signrail` provides release artifact, SBOM, provenance, and witness logic.

## Acceptance Baseline

The foundation gate for this workspace is:

- `cargo fmt --all --check`
- `cargo check --workspace --all-targets`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `jeryu-evidence .`
