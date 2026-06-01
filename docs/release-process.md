# Release Process Doc

This release process doc is the step-by-step operator surface for Jeryu
releases. Releases are local-first. Hosted CI may confirm the same lanes, but it
does not replace local release proof.

## Required Local Gates

Run these from the canonical repository root before creating a release receipt:

- `bash ci-fast-push.sh --full --no-push`
- `JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash ci-fast-push.sh --full --no-push`
- `bash scripts/ci-phases.sh`
- `bash ops/ci/release.sh`
- `bash ops/ci/proof-evidence.sh`
- `cargo test -p jeryu-runnerd workcell --jobs 40` when the workcell control plane, tar safety, or frozen CI repair helpers change.
- `cargo test -p jeryu-readmodel --jobs 40 && cd web && npm run typecheck` when the workcells dashboard or bootstrap feature flags change.
- `cargo test -p jeryu-api --features web --jobs 40`
- `cargo clippy -p jeryu-api --features web --all-targets --jobs 40 -- -D warnings`
  when public API routes or repair bodies change.
- `just security`
- `just audit`

Full mode runs `ops/ci/verify-jeryu-env.sh --build-local --release-guard`.
Stop or quarantine retired-provider runners, `~/.jeryu`, old
`/home/ubuntu/jeryu`, local `:2224`, and monitored retired listeners before
recording release evidence.

## Receipt Contents

Each release receipt records:

- source commit SHA and tag name;
- workspace version and changelog entry;
- `target/jankurai/` proof artifacts;
- SPDX and CycloneDX SBOM digests;
- provenance checksum and cosign transcript path;
- migration, restore, and rollback evidence;
- public API route evidence for changed endpoints, including response-contract
  tests, typed repair guidance, and digest-verifiable CI evidence payloads;
- previous signed artifact checksum.

## Tagging

Tags are cut only after the receipt names the exact source commit and all gates
above are green. Publish closeout changes through a PR branch first; direct
`main` pushes require explicit `--push-main` and are not the default release
path. Do not tag from an uncommitted worktree or from hosted-only state.

## Rollback

Rollback restores the previous signed artifact, restores the pre-migration
SQLite copy when schema changed, reruns API/TUI/git smoke checks, and keeps
write traffic closed until the rollback receipt is attached.
