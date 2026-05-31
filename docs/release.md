# Release Control Surface

Jeryu releases are local-first and evidence-backed. A release candidate cannot
be signed from hosted CI state alone.

## Version Source

- Rust crate versions live in workspace manifests and `Cargo.lock`.
- User-facing changes are summarized in `CHANGELOG.md`.
- Release candidates record the Git commit SHA, artifact checksums, SBOM
  digests, and rollback target.

## Required Gates

- `bash ci-fast-push.sh --no-push`
- `bash scripts/ci-phases.sh`
- `./ops/ci/full.sh`
- `just security`
- `just audit`
- `bash ops/ci/proof-evidence.sh`

## Release Process

1. Run the required gates locally and keep the emitted artifacts under
   `target/jankurai/` until the release receipt is signed.
2. Verify the SQLite migration and restore receipts for the candidate commit.
3. Build release artifacts from the signed commit only, then record checksums,
   SBOM digests, provenance paths, and the rollback target in the release
   receipt.
4. Tag only after the release receipt names the exact commit, prior rollback
   artifact, and gate evidence paths.

## Integrity And Provenance

The security lane writes SBOM, vulnerability scan, provenance, and signing
artifacts under `target/jankurai/security/sbom`. Release receipts must include
the SPDX SBOM checksum, CycloneDX checksum when generated, provenance checksum,
and cosign transcript path.

## Rollback

Every release receipt names the previous signed artifact and checksum. Rollback
means restoring the previous signed artifact, restoring the pre-migration SQLite
copy when schema changed, and re-running the smoke commands for API, TUI, and
Git fetch/clone before reopening write traffic.

## Local-Only Boundary

The current live runtime is bound to `127.0.0.1`. LAN or public exposure waits
for auth, TLS, token rotation, backup restore evidence, and abuse-control
receipts.
