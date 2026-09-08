# jeryu-release-ops

[![Jankurai score: 93/100](https://img.shields.io/badge/Jankurai-93%2F100-brightgreen)](ops/ci/score.sh)

Release, signing, governance, observability, and compliance tooling.

This repository is also the single authority for the independently released
Jeryu family. [`repos.manifest.toml`](repos.manifest.toml) binds the exact
`/home/ubuntu/jain-split/jeryu-split` root, `jeryu/*` forge identities, v5 tag
lineage, and the canonical sibling `jain-redline` dependency. Container and
portal manifests are projections of this authority, not competing sources.

The active authority profile is the private hosted Jeryu forge at
`https://git.neverhuman.org`; every control-plane and product `remote` in the
manifest uses that host. The loopback profile is retained only to describe the
transition boundary and is rejected as an authority selector. Existing Cargo
Git source spellings remain unchanged to preserve crate identity, while the
host's longest-match Git rewrites and CLI transport route those fetches to the
hosted forge.

Live read-only hosted-forge verification on 2026-09-02 resolved protected `main` and
the immutable `jeryu-release-ops-v5.0.0-split.6` tag to the same commit,
`0772dca04bbdfbcaac8ee10fafd8789d01fd1cab`. A control-plane source successor
binds that exact released base as `predecessor_tag`; after protected merge it
receives the next immutable tag. Product rows instead bind their current
released identities with `current_tag`.

The authority is currently in compliance inventory. Its non-waivable contract
requires Jankurai score 85 or higher; zero caps, hard findings, soft findings,
disabled rules, or allowed score drop; hand-authored files no longer than 500
lines with 300 as the refactoring target; and ordinary Git blobs no larger than
1,048,576 bytes. Every repository explicitly declares whether authenticated
local Jeryu LFS is required. Inventory records migration debt but never exempts
it; the state may advance to `enforced` and may not roll back.

This repository was seeded from Jeryu source commit `cbecf7caa0e932c76a341b2521e66e911233860d` by
`ops/split/materialize.py`. It is part of the 11-repository Jeryu split family and keeps source
paths stable where practical so ownership remains auditable.

Repository-specific agent instructions and ownership boundaries start in
[`AGENTS.md`](AGENTS.md).

## Getting Started

Install the Rust toolchain declared by `rust-toolchain.toml` and
[`just`](https://github.com/casey/just), then run the deterministic local gate:

```bash
just fast
```

Before proposing release evidence, run the complete source-readiness lane:

```bash
just release-readiness
```

That lane is non-promoting: deployment and production mutation remain owned by
`jeryu-deploy`.

## Owned Cargo Packages

- `crates/jeryu-signing`
- `crates/jeryu-signrail`
- `crates/jeryu-wsversion`
- `crates/jeryu-repogate`
- `crates/jeryu-mapcheck`
- `crates/jeryu-evidence`
- `crates/jeryu-obs`
- `crates/jeryu-bench`
- `crates/jeryu-ops`
- `crates/jeryu-phase11-core`
- `crates/jeryu-phase11-audit`
- `crates/jeryu-compliance-export`
- `crates/jeryu-lifecycle`
- `crates/jeryu-tenant`
- `crates/jeryu-kernel`
- `crates/jeryu-replay-verifier`
- `crates/jeryu-git-guard`
- `bins/jeryu-phase11-bin`

## Source Coverage

- `crates/jeryu-signing/**`
- `crates/jeryu-signrail/**`
- `crates/jeryu-wsversion/**`
- `crates/jeryu-repogate/**`
- `crates/jeryu-mapcheck/**`
- `crates/jeryu-evidence/**`
- `crates/jeryu-obs/**`
- `crates/jeryu-bench/**`
- `crates/jeryu-ops/**`
- `crates/jeryu-phase11-core/**`
- `crates/jeryu-phase11-audit/**`
- `crates/jeryu-compliance-export/**`
- `crates/jeryu-lifecycle/**`
- `crates/jeryu-tenant/**`
- `crates/jeryu-kernel/**`
- `crates/jeryu-replay-verifier/**`
- `crates/jeryu-git-guard/**`
- `bins/jeryu-phase11-bin/**`
- `bench/**`
- `dashboards/**`
- `ops/signrail-verify/**`
- `ops/upgrade/**`
- `ops/bench/**`
- `ops/security/**`
- `ops/chaos/**`
- `tools/**`
- `fixtures/upgrade/**`
- `fixtures/security/**`
- `fixtures/chaos/**`
- `fixtures/sso/**`
- `fixtures/benchmarks/**`
- `configs/signrail.example.toml`

## Local Commands

- `just fast`
- `just check`
- `just score`
- `just security`
- `just artifact-support`
- `just redline-consumer-test`

`ops/ci/redline-consumer.sh` emits checksummed Redline compatibility evidence
only from clean, forge-equal `main`, a passing family receipt, and the exact
immutable engine tag resolved by `Cargo.lock`. The operator must also pass the
canonical Jeryu split manifest with `--consumer-manifest`; the script hashes it
alongside this repository's `agent/audit-policy.toml` instead of inferring a
manifest from the consumer checkout. The current reviewed dependency is
`redline-core-v4.1.0-jain.6` at
`d0de59930141baffcfa2b514480e75b14627f24d`.
