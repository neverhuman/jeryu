# Release Control Surface

Jeryu releases are local-first and evidence-backed. A release candidate cannot
be signed from hosted CI state alone.
This is the canonical release process doc for version source, changelog,
release commands, integrity/provenance evidence, and rollback guidance.
The step-by-step operator process lives in `docs/release-process.md`.

## Version Source

- Rust crate versions live in workspace manifests and `Cargo.lock`.
- User-facing changes are summarized in `CHANGELOG.md`.
- Release candidates record the Git commit SHA, artifact checksums, SBOM
  digests, and rollback target.

## Required Gates

- `bash ci-fast-push.sh --full --no-push`
- `JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash ci-fast-push.sh --full --no-push`
- `bash scripts/ci-phases.sh`
- `./ops/ci/full.sh`
- `just security`
- `just audit`
- `bash ops/ci/proof-evidence.sh`
- `cargo test -p jeryu-api --features web --jobs 40` when compatibility routes
  or guided repair bodies change.

Latest closeout validation used explicit `--full` mode with 40 workers in both
local-native and GitHub-clean profiles: 1175 nextest tests, phase gates
PASS=7/PENDING=0/FAIL=0, proof-evidence Jankurai full scan score 92 caps 0, and
changed-file Jankurai diff/audit hard 0 caps 0. The GitHub-clean proof is
`JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash ci-fast-push.sh --full --no-push`.
Full mode runs `ops/ci/verify-jeryu-env.sh --build-local --release-guard` and
rejects retired-provider runners, `~/.jeryu`, old `/home/ubuntu/jeryu`, and
local `:2224` listener/remotes so release evidence cannot be produced against
the retired system. This host still needs an explicit retired-state bypass for
local parity runs until an operator stops the root-owned retired services.

## Release Process

1. Run the required gates locally and keep the emitted artifacts under
   `target/jankurai/` until the release receipt is signed.
2. Verify the SQLite migration and restore receipts for the candidate commit.
3. Build release artifacts from the signed commit only, then record checksums,
   SBOM digests, provenance paths, and the rollback target in the release
   receipt.
4. Publish through a PR branch; direct `main` pushes from `ci-fast-push.sh`
   require explicit `--push-main` and are not the default closeout path.
5. Tag only after the release receipt names the exact commit, prior rollback
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
