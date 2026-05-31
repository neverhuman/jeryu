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
