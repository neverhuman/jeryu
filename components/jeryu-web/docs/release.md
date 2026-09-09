# Release

This split member publishes source changes through pinned tags; `jeryu-deploy` remains the binary release authority.

Version source: `VERSION` plus the split tag recorded in
`repos.manifest.toml` when present. Release notes are recorded in
`CHANGELOG.md`.

Release process doc: this file is the frontend release control surface. The
release automation or command policy is to run the proof commands below from
this repo, then let `jeryu-deploy` package and publish the binary/web artifact.
Checksum/provenance/SBOM evidence policy: every promoted artifact must have
checksum, provenance, SBOM, and cosign evidence recorded with the release.

## Release Gate

Before a release or split tag is promoted:

- run `just fast`, `just check`, `just score`, `just security`, and `just artifact-support`
- confirm checksum, provenance, SBOM, and cosign evidence for release artifacts
- confirm monitoring is active for the promoted version
- confirm backups or reproducible source inputs exist for rollback
- confirm rate limit or abuse controls are configured for public surfaces

## Rollback

Rollback guidance: restore consumers to the previous known-good split tag and
its artifact evidence. Do not overwrite tags; publish a new repair tag or move
the deployment pointer back to the last verified tag.

## Auditor-only CI cutovers

Changing the governed auditor is not a product release and leaves `VERSION`,
split tags, and product artifacts unchanged. Its release gate requires the
exact protected `jeryu-tool` source/tag/binary receipt, clean exact-head CI,
and independent approval before protected merge. The backup, monitoring,
rate-limit or abuse, checksum, SBOM, provenance, and rollback controls above
remain mandatory for any later product promotion.
