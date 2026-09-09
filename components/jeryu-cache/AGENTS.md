# jeryu-cache Agent Instructions

This is a Jeryu split repository seeded from `cbecf7caa0e932c76a341b2521e66e911233860d`.

Before editing, read `README.md`, `agent/owner-map.json`,
`agent/test-map.json`, `agent/generated-zones.toml`,
`agent/proof-lanes.toml`, `agent/audit-policy.toml`, and
`agent/boundaries.toml`.

Keep split `main` clean. The canonical hosted repository is
`https://git.neverhuman.org/git/jeryu/jeryu-cache.git`. The legacy monorepo
(`/home/ubuntu/jeryu`) and loopback forge are not source or release authority.
Land changes through hosted PRs with green required checks.

Preserve governed Cargo source strings and immutable dependency tags. Hosted
transport is applied below Cargo through the installed longest-prefix Git
rewrites; rewriting source coordinates can create duplicate crate identities.
Only `jeryu-deploy` may use local sibling path patches for split-family
development.
