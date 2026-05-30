# Release

## Version Source

Release versions come from workspace crate metadata, `CHANGELOG.md`, and signed
release receipts produced by `ops/ci/release.sh`.

## Changelog

Every release updates `CHANGELOG.md` before any artifact is published. The
changelog entry names the exact commit, the local CI receipt, and any migration
or rollback note required by the release witness.

## Release Process Doc

This file is the release process doc for local operators and agents:
- Run `just ci`, `just full`, `just security`, and `just release` locally with the default 40-worker CI configuration.
- Update `CHANGELOG.md` before cutting a release.
- Verify zero-evidence, provenance, release witness, and SignRail checks before publishing any artifact.
- Emit release receipt metadata with `scripts/emit-release-receipt.sh`.

## CI And Script Evidence

`just release` delegates to `ops/ci/release.sh`; `just ci` delegates to
`scripts/ci-phases.sh`. Both are local gates and must pass from a clean tree
before an artifact can be signed.

## Integrity Evidence

- `jeryu-signrail` tests validate release witnesses, provenance, signatures, checksums, and rollback metadata.
- Release artifacts must be tied to an exact commit SHA and must not depend on mutable cache hits.

## Rollback Guidance

- Preserve the previous signed release receipt and artifact checksums.
- Use lifecycle rollback plans from `jeryu-lifecycle` before changing deployed state.
- If signer, SBOM, checksum, or witness verification fails, stop the release and keep the previous version active.
