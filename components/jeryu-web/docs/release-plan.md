# Release Plan

The `jeryu-web` release control surface. This split member publishes source
changes through pinned tags; `jeryu-deploy` remains the binary/web release
authority. This plan is the always-in-scope companion to `docs/release.md`.

## Version source

The version source is `VERSION` (plus `agent/standard-version.toml`) and the
split tag recorded in `repos.manifest.toml` when present. Release notes live in
`CHANGELOG.md` under `## Unreleased` until a tag is cut.

## Release process

1. Land changes through PRs with a green `jeryu-web/required` check (the
   canonical gate: `ops/ci/pr-ci.sh`).
2. Cut a split tag; `jeryu-deploy` packages and publishes the web artifact.
3. The hosted `git.neverhuman.org` forge advances `main` only through the gated
   PR-merge path. GitHub is not a release or publication path.

## Integrity and provenance

Every promoted artifact records checksum, provenance/SBOM (`syft` SPDX-JSON via
the security lane, see `docs/security-tool-matrix.md`), and cosign signing
evidence with the release. The security lane (`tools/security-lane.sh`) runs
gitleaks, `npm audit`, `zizmor`, and `syft` on every gate.

## Rollback and backfill

Rollback guidance: restore consumers to the previous known-good split tag and
its artifact evidence. Do not overwrite tags; publish a new repair tag or move
the deployment pointer back to the last verified tag. No data backfill applies —
`jeryu-web` owns no durable production tables (its `db/` holds dev/e2e fixtures
only; see `db/README.md`).

## Release gate checklist

- `just fast`, `just check`, `just score`, `just security`, `just artifact-support`
- checksum, provenance, SBOM, and cosign evidence present for artifacts
- monitoring active for the promoted version; rollback tag verified
