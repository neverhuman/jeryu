# jeryu-deploy

Integration, end-user binary build, split lock, and release bundle logic.

This repository was seeded from Jeryu source commit `cbecf7caa0e932c76a341b2521e66e911233860d`.
It is part of the Jeryu split family and keeps source paths stable where practical so ownership
remains auditable.

## Agent Navigation

Read `AGENTS.md` before changing this repository; `CLAUDE.md` points to the same
authority. Durable detail is intentionally routed rather than duplicated:

- Architecture and trust boundaries: `docs/architecture.md` and
  `docs/boundaries.md`.
- Tests and required proof: `docs/testing.md`, `agent/test-map.json`, and
  `agent/proof-lanes.toml`.
- Ownership and generated files: `agent/owner-map.json`,
  `agent/generated-zones.toml`, and `docs/generated-zones.md`.
- Audit and release controls: `agent/audit-policy.toml`, `docs/audit-rubric.md`,
  `docs/release.md`, and `docs/release-process.md`.

Governed Jankurai rotations run the closed projection integration test, the
hostile-identity shell verifier, full score, and protected-base diff audit
listed in `docs/testing.md`. The monorepo-only mapcheck marker lane is not a
standalone Deploy proof.

## Owned Cargo Packages

- `crates/jeryu-api`
- `crates/jeryu-cli`
- `crates/jeryu-split-tool`

## Source Coverage

- `crates/jeryu-api/**`
- `crates/jeryu-cli/**`
- `crates/jeryu-split-tool/**`
- `.github/**`
- `ci-fast-push.sh`
- `ops/**`
- `scripts/**`
- `tools/**`
- `tests/**`
- `examples/**`
- `config/**`
- `configs/**`
- `policies/**`
- `images/**`
- `docs/**`
- `agent/**`
- `Cargo.toml`
- `Cargo.lock`
- `Justfile`

## Local Commands

- `just fast`
- `just check`
- `just check-api`
- `just test-api`
- `just agent-runs`
- `just request-id`
- `just cache-status`
- `just score`
- `just security`
- `just security-network`
- `just artifact-support`

Rust-native split transition checks are available through
`cargo run --locked --offline -p jeryu-split-tool --bin jeryu-split --
<manifest|source-coverage|fleet-ci|verify-lock|product-pipeline>`. The
`ops/split/manifest.sh` compatibility entrypoint delegates to that binary and
does not invoke a Python runtime.

Repository CI wrappers route their lock-bound release-toolkit checks through
the same local binary with `ci-lanes-check`, `ci-lanes-list`, and
`affected-plan`; they do not discover or execute source from a Cargo cache or
sibling checkout. Retired monorepo-only repository gates are rejected rather
than being applied to this standalone split.

`bash ops/ci/full.sh` and `bash ci-fast-push.sh --full --no-push` retain the
complete workspace test matrix while composing only repository-owned proof,
workflow-parity, release-receipt-contract, score, security, and doctor gates.

## Quick Start

Prerequisites are Rust 1.95 and the governed Jankurai binary described in
`docs/release.md#governed-jankurai-identity`. From a canonical checkout:

```bash
bash ops/ci/ensure-jankurai.sh
just fast
```

The verifier is read-only and never installs a tool. `just fast` is the
deterministic affected lane; use `just check`, then `just security`, before
requesting protected review. Test ownership and narrower reruns are mapped in
`agent/test-map.json` and `docs/testing.md`.

## Status

The protected repository on `git.neverhuman.org` is source and ref authority;
local checks are developer evidence, not permission to merge. Until the hosted
runner posts the protected required context, a topic remains unmergeable even
when its push-time Jankurai proof is green. This successor is not a release.
Its ratchet baseline was generated from exact hosted protected `main` with the
governed 1.6.11 binary; the baseline and every candidate result still require
detached exact-head review before protected merge. Current lane artifacts are
written under `.jankurai/` and `target/jankurai/`; neither is a source-of-truth
badge or permission to publish.

Historical Cargo source spellings remain unchanged to preserve crate identity.
`.cargo/hosted-gitconfig` maps only the exact dependency repositories to their
hosted Jeryu URLs and uses the dedicated hosted credential helper without
including ambient user configuration. It also carries the one exact host-scoped
smart-HTTP setting required by the currently deployed server. CI sources
`ops/ci/hosted-git-env.sh` before Cargo (Cargo's
own fetch does not consume its `[env]` table). Because the hosted backend
rejects Cargo's tag-only exact-object fetch, `.cargo/hosted-pin-refs.tsv` also
binds every immutable tag target to an advertised hosted preservation ref; the
refs contain no bytes beyond their existing release tags. Finally,
`bash ops/ci/dependency-sources.sh` rejects an unlisted source, mutable pin,
missing mapping/ref, or non-hosted effective destination.
