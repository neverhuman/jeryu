# jeryu-ci-runner Agent Instructions

This is a Jeryu split repository seeded from `cbecf7caa0e932c76a341b2521e66e911233860d`.

Before editing, read `README.md`, `agent/owner-map.json`,
`agent/test-map.json`, `agent/generated-zones.toml`,
`agent/proof-lanes.toml`, `agent/audit-policy.toml`, and
`agent/boundaries.toml`.

Keep split `main` clean. The legacy monorepo (`/home/ubuntu/jeryu`) is
deprecated and archived as `jeryu/jeryu-monorepo`; this split family is the
only source of truth. Land changes through PRs with green required checks.

Cross-repo Rust dependencies retain their immutable Cargo source identities and
are pinned to exact v5 split tags. Host CI must source
`ops/ci/hosted-git-env.sh`; `.cargo/hosted-gitconfig` then maps only the
authenticated dependency URL to `git.neverhuman.org`. Do not rewrite the Cargo
source spelling or add local sibling path patches, because either can create a
second crate identity.

The public runner wire mirror is `schemas/jeryu.runner.v1.schema.json`. Rust
serde plus `ValidateWire` remain authoritative; run `just contract-drift` whenever
wire source or schema bytes change.
