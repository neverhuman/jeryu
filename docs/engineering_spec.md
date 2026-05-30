# JitForge Nitro Engineering Spec

This document is the sanitized engineering overview for the fused Rust
workspace. It records product invariants that are currently represented by the
checked-in crates, scripts, and local verification gates.

## Core Invariants

- One workspace root owns every product crate and binary.
- Runtime-facing commands stay under the `jeryu` product surface while service
  internals use JitForge components.
- Cache correctness beats cache hit rate.
- CI inputs are native JitForge TOML, GitHub Actions workflows, API-created
  runs, scheduled runs, agent dry runs, hotfix runs, release runs, and
  merge-queue synthetic runs.
- Release paths use hermetic cache policy, provenance receipts, checksums, and
  signed witnesses.
- Agent writes require scoped capability checks, proof receipts, and auditable
  mutation records.

## Current Workspace Scope

- `forge-core` and `jitforge-api` provide typed forge domain models and API
  facades.
- `ci-ir`, `ci-compiler`, `ci-scheduler`, and `jit-ci` provide CI compilation
  and scheduling foundations.
- `runner-*` crates define runner fabric and sandbox policy surfaces.
- `cratevault-*` crates provide cache, CAS, quarantine, and receipt behavior.
- `proofcore` and `agentbridge` provide proof and agent-control foundations.
- `signrail` provides release artifact, SBOM, provenance, and witness logic.

## Acceptance Baseline

The foundation gate for this workspace is:

- `cargo fmt --all --check`
- `cargo check --workspace --all-targets`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `scripts/zero-evidence-guard.py .`
