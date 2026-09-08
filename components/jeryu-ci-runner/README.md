# jeryu-ci-runner

[![Release status: candidate; required check pending](docs/status-candidate.svg)](docs/release.md)

CI IR, scheduler, runner fabric, workcells, sandboxing, agent execution substrate.

Start with the repository's canonical [`AGENTS.md`](AGENTS.md) before changing
source or running a release lane. It defines the hosted dependency identity,
required evidence, and generated-file boundaries that this README summarizes.

The strict endpoint-neutral `jeryu.runner.v1` JSON contract is documented in
[`docs/runner-wire-v1.md`](docs/runner-wire-v1.md). It is protocol-only; no AtomicSoul runner is
installed or registered by this repository state.

This repository was seeded from Jeryu source commit `cbecf7caa0e932c76a341b2521e66e911233860d` by
`ops/split/materialize.py`. It is part of the seven-repo Jeryu split family and keeps source
paths stable where practical so ownership remains auditable.

## Status

This source remains a **candidate**, not a runtime registration or public
release. `git.neverhuman.org` is the Git transport source of truth. A topic is
green only when its exact commit has the real `jeryu-ci-runner/required` check,
the fleet score and hard-finding gates pass, and an independent review is bound
to that same head. A `jankurai/proof` check is useful corroboration, but it does
not substitute for the protected required context.

## Quick Start

Use the pinned Rust toolchain and locked dependency graph from the repository
root:

```bash
just fast
just check
just contract-drift
```

Run `just security` before proposing a release-relevant change. It is the
networked supply-chain lane and therefore also verifies that dependency
transport reaches only `git.neverhuman.org` without changing Cargo's stable
source identities.

## Owned Cargo Packages

- `crates/jeryu-ci-ir`
- `crates/jeryu-ci-compiler`
- `crates/jeryu-ci-scheduler`
- `crates/jeryu-runner-core`
- `crates/jeryu-runner-native`
- `crates/jeryu-runner-microvm`
- `crates/jeryu-runner-oci`
- `crates/jeryu-runner-protocol`
- `crates/jeryu-runner-registry`
- `crates/jeryu-runnerd`
- `crates/jeryu-sandbox-linux`
- `crates/jeryu-agentbridge`
- `crates/jeryu-agent-auth`
- `crates/jeryu-agent-stream`
- `crates/jeryu-egress`
- `crates/jeryu-artifact-metadata`
- `crates/jeryu-cache-policy`
- `crates/jeryu-ci-governor`
- `crates/jeryu-phase7-cli`
- `bins/jeryu-ci-bin`

## Source Coverage

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
- `contracts/**`
- `schemas/**`
- `tests/fixtures/github/**`
- `tests/fixtures/native/**`
- `tests/sandbox_escape_matrix.sh`
- `examples/jobs/**`
- `ops/agent-sandbox/**`
- `images/agent-sandbox/**`
- `policies/**`
- `configs/runnerd.toml`

## Local Commands

- `just fast`
- `just check`
- `just score`
- `just security`
- `just contract-drift`
- `just artifact-support`

`just security` is the full networked supply-chain gate. It requires pinned
Gitleaks, Actionlint, Cargo Audit, Cargo Deny, and Syft binaries; validates the
immutable Cargo lock and Core tag/support ref; proves the effective Git
destination is `git.neverhuman.org`; and emits a CycloneDX SBOM. Cargo's
historical source spelling remains unchanged so the Rust graph keeps one crate
identity. Host CI sources `ops/ci/hosted-git-env.sh` before every Cargo entrypoint;
caller-supplied Git configuration is accepted only when it is byte-identical to
the checked-in policy, then canonicalized to that reviewed path.

`just contract-drift` binds the checked JSON Schema to all serialized Rust fields,
message discriminators, wire enums, and public collection bounds. Rust serde
and `ValidateWire` remain the runtime authority for cross-field, identity,
credential-name, digest, timestamp, and path invariants.
