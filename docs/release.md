# Release

`neverhuman/jeryu-deploy` publishes all signed release artifacts.

Version source is `VERSION` plus the split tag recorded in
`repos.manifest.toml` when present. Release notes are recorded in
`CHANGELOG.md`.

## Release Gate

Before a release or split tag is promoted:

- run `just fast`, `just check`, `just score`, `just security`, and `just artifact-support`
- confirm checksum, provenance, SBOM, and cosign evidence for release artifacts
- confirm monitoring is active for the promoted version
- confirm backups or reproducible source inputs exist for rollback
- confirm rate limit or abuse controls are configured for public surfaces

## Rollback

Rollback uses the previous known-good split tag and its artifact evidence. Do
not overwrite tags; publish a new repair tag or restore consumers to the last
verified tag.
