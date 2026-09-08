# jeryu-release-ops Agent Instructions

This is a Jeryu split repository seeded from `cbecf7caa0e932c76a341b2521e66e911233860d`.

Before editing, read `README.md`, `agent/owner-map.json`,
`agent/test-map.json`, `agent/generated-zones.toml`,
`agent/proof-lanes.toml`, `agent/audit-policy.toml`, and
`agent/boundaries.toml`.

Keep split `main` clean. The legacy monorepo (`/home/ubuntu/jeryu`) is
deprecated and archived as `jeryu/jeryu-monorepo`; this split family is the
only source of truth. Land changes through PRs with green required checks.

This repository owns the family authority at `repos.manifest.toml`. Its exact
root is `/home/ubuntu/jain-split/jeryu-split`; Jeryu keeps its independent
`jeryu-split` release identity, `jeryu/*` forge namespace, and v5 tag lineage.
Redline source and evidence resolve only through sibling canonical
`/home/ubuntu/jain-split/jain-redline`. Alternative roots, copied source,
symlink shims, and a duplicate `jeryu-redline` container are forbidden.

Cross-repo Rust dependencies are pinned Git dependencies using
`*-v4.0.0-split.0` tags. Only `jeryu-deploy` may use local sibling path patches
for split-family development.

The Redline database dependency is separately pinned to an immutable
`redline-core-v4.1.0-jain.N` tag. A revision change must update the locked
source, consumer producer, and release documentation together, then pass the
consumer and full PR proof lanes described in `ops/AGENTS.md`.
