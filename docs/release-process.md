# Release Process Doc

This release process doc is the step-by-step operator surface for Jeryu
releases. Releases are local-first. Hosted CI may confirm the same lanes, but it
does not replace local release proof.

## Required Local Gates

Run these from the canonical repository root before creating a release receipt:

- `bash ci-fast-push.sh --full --no-push`
- `JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash ci-fast-push.sh --full --no-push`
- `bash scripts/ci-phases.sh`
- `bash ops/ci/proof-evidence.sh`
- `just security`
- `just audit`

## Receipt Contents

Each release receipt records:

- source commit SHA and tag name;
- workspace version and changelog entry;
- `target/jankurai/` proof artifacts;
- SPDX and CycloneDX SBOM digests;
- provenance checksum and cosign transcript path;
- migration, restore, and rollback evidence;
- previous signed artifact checksum.

## Tagging

Tags are cut only after the receipt names the exact source commit and all gates
above are green. Do not tag from an uncommitted worktree or from hosted-only
state.

## Rollback

Rollback restores the previous signed artifact, restores the pre-migration
SQLite copy when schema changed, reruns API/TUI/git smoke checks, and keeps
write traffic closed until the rollback receipt is attached.
