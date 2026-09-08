# Release Process Doc

This release process doc is the step-by-step operator surface for Jeryu
releases. The commands are reproducible locally, while the protected hosted
exact-head required check is merge authority.

## Required Local Gates

Run these from the canonical repository root before creating a release receipt:

- `bash ci-fast-push.sh --full --no-push`
- `JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash ci-fast-push.sh --full --no-push`
- `bash ci-fast-push.sh --full` from a non-main release branch to push the
  branch, open or report the PR, and write `target/ci-fast/publish.json`
- `bash scripts/ci-phases.sh`
- `SIGNRAIL_ROLLBACK_TARGET=<previous-signed-release> bash ops/ci/artifact_support.sh`
- `bash ops/ci/release.sh`
- `bash ops/deploy/test-atomicsoul-release.sh` when production deploy helpers
  or this atomicsoul process changes.
- `bash ops/ci/proof-evidence.sh`
- `bash ops/ci/web.sh` for the immutable staged web bundle.
- `bash ops/ci/gates/workcells.sh` and
  `bash ops/ci/gates/runner-sandbox.sh` when those Deploy integrations change.
- `cargo test -p jeryu-api --features web --jobs 40`
- `cargo test -p jeryu-api --features web --jobs 40 agent_runs` when the high-level agent-run route or PTY controls change.
- `cargo test -p jeryu-api --features web --jobs 40 r5_jail_loop` when the jailed workcell edit, namespaced branch export, PR creation, or CI evidence flow changes.
- `cargo clippy -p jeryu-api --features web --all-targets --jobs 40 -- -D warnings`
  when public API routes or repair bodies change.
- `cargo test -p jeryu-api --features web --jobs 40 codegraph` and
  `cargo test -p jeryu-api --features web --jobs 40 control_plane` for those
  Deploy-owned facades, plus `cargo test -p jeryu-cli --jobs 40` for CLI dispatch.
- `just security`
- `just audit`

This checkout owns only `jeryu-api`, `jeryu-cli`, and `jeryu-split-tool` source.
Changes to versioning, SignRail, runner, read-model, MCP, codegraph, sandbox,
agentbridge, egress, or web source must land through that component's protected
standalone repository before Deploy updates its immutable pin or staged bundle.

Full mode runs `ops/ci/verify-jeryu-env.sh --build-local --release-guard`.
It requires the exact hosted origin and rejects retired-provider runners, old
`/home/ubuntu/jeryu` source roots, local `:2224`, and monitored experimental
listeners. The accepted predecessor runtime may remain live; tests use the
fresh repository binary and dependency-source evidence proves hosted-only Git
transport.

## Merge Authority

Open the release or consolidation PR with `ci-fast-push.sh`. The push path
records `target/ci-fast/publish.json` with the branch, base, PR URL, PR number,
and commit that the final receipt must name. Protected hosted branch rules and
the exact-head required result are merge authority; local gate transcripts are
reproducible evidence. Direct wire pushes to `main` are not a supported release
path.

## Receipt Contents

Each final release receipt uses schema `jeryu.release-receipt/v2` and records:

- source commit SHA and tag name;
- `signed-commit.txt` proving `git verify-commit --raw <sha>` succeeded;
- PR publication metadata from `target/ci-fast/publish.json`;
- workspace version and changelog entry;
- `jeryu-wsversion decide --json` evidence for the released commit range and
  `inherit-guard` evidence for workspace member manifests;
- `target/jankurai/` proof artifacts;
- MCP/catalog trust evidence for changed local agent-facing commands, including
  `agent/tool-adoption.toml`, the pinned `ops/ci/security-tools.sh` transcript,
  `cargo test -p jeryu-mcp --test mcp_conformance --jobs 40`, and any composed
  route/tool contract lane such as `bash ops/ci/codegraph-oracle.sh`;
- SPDX and CycloneDX SBOM digests;
- provenance checksum and cosign transcript path;
- migration, restore, and rollback evidence;
- previous signed release binary, signature, and certificate checksums;
- artifact-support `artifact-support-evidence.tar.gz`, SignRail
  `release.json`, `sbom.json`, `provenance.json`, `witness.json`, and
  `stage-receipts/{local,dev-canary,prod}.json`;
- `SHA256SUMS`, generated over every bundle file except `release-receipt.json`
  and `SHA256SUMS` itself;
- public API route evidence for changed endpoints, including response-contract
  tests, typed repair guidance, and digest-verifiable CI evidence payloads;
- JMCP control-plane evidence when that surface changes, including explicit
  mirror/artifact absence states, MCP catalog conformance, CLI grammar,
  fail-closed dispatch, and `/intelligence` web smoke;
- agent-run and codegraph-oracle route evidence when those public endpoints
  change;

## Tagging

Tags are cut only after the receipt names the exact signed source commit, the
PR-backed publication path, the previous signed rollback artifact, and all gates
above are green. Publish closeout changes through a PR branch first; do not tag
from an uncommitted worktree, an unsigned commit, placeholder rollback evidence,
or hosted-only state.

## Atomicsoul Autonomous Deploy Handoff

Production deploys to `atomicsoul` are AI-runnable only after fresh per-release
env material exists. Do not reuse a prior release's env, admin password,
SignRail seed, or deploy signing key.

1. Generate release-local production material:

   ```bash
   ops/deploy/make-production-env.sh --release <release-tag>
   ```

   The secret output is written under
   `~/.jeryu/deploy-env/<release-tag>/production.env`. It includes a fresh
   `JERYU_BOOTSTRAP_ADMIN_PASSWORD`, a fresh
   `JERYU_SIGNRAIL_ED25519_SEED`, and a fresh Ed25519 key used to sign the
   deployment checksum manifest.

2. Source the generated env before artifact-support signing and release bundle
   creation:

   ```bash
   set -a
   . ~/.jeryu/deploy-env/<release-tag>/production.env
   set +a
   SIGNRAIL_ROLLBACK_TARGET=<previous-signed-release> bash ops/ci/artifact_support.sh
   bash ops/ci/release.sh
   ```

   For a first production install on a host with no previous Jeryu production
   artifact, use an explicit rollback marker instead of inventing previous
   digests:

   ```bash
   export JERYU_RELEASE_INITIAL_DEPLOY=1
   export SIGNRAIL_ROLLBACK_TARGET=atomicsoul-initial-install
   export JERYU_RELEASE_ROLLBACK_TAG=atomicsoul-initial-install
   ```

3. Sign the deploy SHA manifest and push/install artifacts on `atomicsoul`:

   ```bash
   ops/deploy/sign-and-push-atomicsoul.sh \
     --env ~/.jeryu/deploy-env/<release-tag>/production.env
   ```

   The push creates `target/release/bundle/atomicsoul-deploy/SHA256SUMS`,
   signs it with the per-release Ed25519 key, pushes the bundle, web dist,
   split manifest, runtime env, and user systemd unit to `atomicsoul`, then
   verifies the checksum manifest and signature on the host. The default path
   updates `~/.jeryu/bin/jeryu`, `~/.jeryu/share/web-dist`, and
   `~/.jeryu/share/repos.manifest.toml`, but does not restart the service.
   Pass `--restart` only when the release is approved for live activation.

## Rollback

Rollback restores the previous signed artifact, restores the pre-migration
SQLite copy when schema changed, reruns API/TUI/git smoke checks, and keeps
write traffic closed until the rollback receipt is attached.
