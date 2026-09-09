# Release

This split member publishes source changes through pinned tags; `jeryu-deploy` remains the binary release authority.

Version source is `VERSION` plus the immutable tag on `git.neverhuman.org`
recorded in
`repos.manifest.toml` when present. Release notes are recorded in
`CHANGELOG.md`.

## Release Gate

Before a release or split tag is promoted:

- run `rtk bash scripts/ci-local.sh required`
- read back exact hosted proof, required check, score, review, and protected main
- confirm checksum, provenance, SBOM, and cosign evidence for release artifacts
- confirm monitoring is active for the promoted version
- confirm backups or reproducible source inputs exist for rollback
- confirm rate limit or abuse controls are configured for public surfaces

## Rollback

Rollback uses the previous known-good split tag and its artifact evidence. Do
not overwrite tags; publish a new repair tag or restore consumers to the last
verified tag.

## Auditor-only CI cutovers

Changing the governed auditor is not a product release and leaves `VERSION`,
split tags, and product artifacts unchanged. Its release gate requires the
exact protected `jeryu-tool` source/tag/binary receipt, clean exact-head hosted
proof, and independent approval before protected merge. The backup, monitoring,
rate-limit or abuse, checksum, SBOM, provenance, and rollback controls above
remain mandatory for any later product promotion.
