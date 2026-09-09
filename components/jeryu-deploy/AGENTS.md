# jeryu-deploy Agent Instructions

This is a Jeryu split repository seeded from `cbecf7caa0e932c76a341b2521e66e911233860d`.

Before editing, read `README.md`, `agent/owner-map.json`,
`agent/test-map.json`, `agent/generated-zones.toml`,
`agent/proof-lanes.toml`, `agent/audit-policy.toml`, and
`agent/boundaries.toml`.

Keep split `main` clean. The legacy monorepo (`/home/ubuntu/jeryu`) is
deprecated and archived as `jeryu/jeryu-monorepo`; this split family is the
only source of truth. Land changes through PRs with green required checks.

Canonical agent-readable detail is routed through `docs/architecture.md`,
`docs/boundaries.md`, `docs/testing.md`, `docs/generated-zones.md`, and
`docs/audit-rubric.md`. Deploy's release proof is its mapped standalone lanes;
the monorepo-only `jeryu-mapcheck docs` marker check is not a Deploy gate.

Cross-repo Rust dependencies are pinned to the exact immutable v5 tags and
commits recorded in `Cargo.lock`. Historical source spellings remain part of
Cargo package identity, but CI must transport them through the exact
`git.neverhuman.org` mappings in `.cargo/hosted-gitconfig`; committed or
release-CI sibling path patches are not permitted.
