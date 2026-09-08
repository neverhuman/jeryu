# jeryu-web Agent Instructions

This is a Jeryu split repository seeded from `cbecf7caa0e932c76a341b2521e66e911233860d`.

Before editing, read `README.md`, `agent/owner-map.json`,
`agent/test-map.json`, `agent/generated-zones.toml`,
`agent/proof-lanes.toml`, `agent/audit-policy.toml`, and
`agent/boundaries.toml`.

Keep split `main` clean. The legacy monorepo (`/home/ubuntu/jeryu`) is
deprecated and archived as `jeryu/jeryu-monorepo`; this split family is the
only source of truth. Land changes through PRs with green required checks.

Cross-repo Rust dependencies are pinned Git dependencies using
`*-v4.0.0-split.0` tags. Only `jeryu-deploy` may use local sibling path patches
for split-family development.

## Docs

Durable guidance lives in `docs/`: `architecture.md`, `boundaries.md`,
`generated-zones.md`, `testing.md`, `release.md`, and `audit-rubric.md`. The
release control surface is `docs/release.md` with the version source in
`VERSION` / `agent/standard-version.toml` and notes in `CHANGELOG.md`.
