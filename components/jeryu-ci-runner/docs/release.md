# Release

This split member publishes source changes through pinned tags; `jeryu-deploy` remains the binary release authority.

Version source is `VERSION` plus its immutable split tag. Authority-manifest
binding is owned by the family control plane; this product repository must not
carry a `repos.manifest.toml`. Release notes are recorded in `CHANGELOG.md`.

## Release Gate

Before a release or split tag is promoted:

- run `just fast`, `just check`, `just score`, `just security`, `just contract-drift`,
  and `just artifact-support`
- require `ops/ci/dependency-sources.sh` to bind every immutable Cargo pin to
  its exact hosted tag, preservation ref, and effective `git.neverhuman.org`
  destination without changing Cargo source identity
- confirm checksum, provenance, SBOM, and cosign evidence for release artifacts
- confirm monitoring is active for the promoted version
- confirm backups or reproducible source inputs exist for rollback
- confirm rate limit or abuse controls are configured for public surfaces

## Rollback

Rollback uses the previous known-good split tag and its artifact evidence. Do
not overwrite tags; publish a new repair tag or restore consumers to the last
verified tag.
