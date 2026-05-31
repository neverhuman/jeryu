# Release Process Doc

This file is the operator release process doc referenced by
`docs/release.md`. It exists so release readiness has a concrete process
surface, not only a checklist.

## Inputs

- Candidate Git commit SHA from `origin/main`.
- Workspace version from `Cargo.toml` and crate manifests.
- User-facing change summary from `CHANGELOG.md`.
- Previous signed artifact, checksum, and rollback target.

## Local Gate Evidence

Run these commands from the repository root and keep their artifacts until the
release receipt is signed:

```bash
bash ci-fast-push.sh --no-push
bash scripts/ci-phases.sh
./ops/ci/full.sh
just security
just audit
bash ops/ci/proof-evidence.sh
```

Required evidence paths:

- `target/jankurai/security/evidence.json`
- `target/jankurai/proof-evidence/`
- `target/jankurai/diff/diff-score.md`
- `.jankurai/repo-score.md`
- SQLite backup or restore dry-run receipt when migrations changed.

## Latest Local Validation

The gitd import validation tranche was pushed with `bash ci-fast-push.sh` on
40 workers after the repo-local `jeryu` binary and Jankurai 1.6.10 were
verified. The push-mode gate passed 1122 nextest tests, phase gates
PASS=7/PENDING=0/FAIL=0, Jankurai diff audit score 90 with hard 0 and caps 0,
and Jankurai repository audit score 92 with caps 0.

## Build And Sign

1. Verify the candidate SHA matches the checked-out tree.
2. Build artifacts from that SHA only.
3. Generate SBOM, checksum, provenance, and signing evidence.
4. Write a release receipt naming the SHA, artifact paths, checksums, SBOM
   digest, provenance digest, signer identity, and rollback target.
5. Tag only after the release receipt and rollback evidence exist.

## Rollback

Rollback restores the previous signed artifact and checksum. If a migration was
part of the candidate, restore the pre-migration SQLite copy first, then rerun
API, TUI, and Git clone/fetch smoke commands before write traffic is reopened.
