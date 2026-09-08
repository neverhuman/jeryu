# jeryu-core

Forge/domain truth, git storage, read models, TUI, durable DB migrations.

The `jeryu-core` crate exports object-safe `ForgeReadService` domain contracts
for repository, pull-request, check, protection, and audit reads. Transport
adapters consume those contracts without depending on the concrete `ForgeCore`
store or introducing HTTP types into the domain crate.

This repository was seeded from Jeryu source commit `cbecf7caa0e932c76a341b2521e66e911233860d` by
`ops/split/materialize.py`. It is part of the independent Jeryu split family and keeps source paths
stable where practical so ownership remains auditable; family membership is derived from the
Jeryu authority manifest rather than a count embedded here.

The Phase 12 JeryuCache contract remains documented in `docs/PHASE12_SPEC.md`. Runtime cache/CAS
behavior is owned by the separately released `jeryu-cache` repository; this repository consumes
that boundary through pinned interfaces.

## Quick start

Use the pinned Rust toolchain and run the same local entry points used by the
protected gate:

```bash
just fast
just focused
just check
just score
just security
```

The release-supporting full gate is `bash ops/ci/pr-ci.sh`; it additionally
requires the governed Jankurai binary and the pinned security tools documented
by `ops/ci/security.sh`.

## Owned Cargo Packages

- `crates/jeryu-core`
- `crates/domain`
- `crates/jeryu-gitd`
- `crates/jeryu-mirror`
- `crates/jeryu-mirror-cli`
- `crates/jeryu-readmodel`
- `crates/jeryu-tui`
- `crates/jeryu-bugtracker`
- `crates/jeryu-enterprise`
- `crates/jeryu-proof`

## Source Coverage

- `crates/jeryu-core/**`
- `crates/domain/**`
- `crates/jeryu-gitd/**`
- `crates/jeryu-mirror/**`
- `crates/jeryu-mirror-cli/**`
- `crates/jeryu-readmodel/**`
- `crates/jeryu-tui/**`
- `crates/jeryu-bugtracker/**`
- `crates/jeryu-enterprise/**`
- `crates/jeryu-proof/**`
- `db/**`
- `contracts/generated/**`
- `contracts/AGENTS.md`

## Local Commands

- `just fast`
- `just check`
- `just score`
- `just security`
- `just artifact-support`
