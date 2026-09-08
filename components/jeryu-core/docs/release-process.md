# Release Process Doc

This release process doc is the step-by-step operator surface for Jeryu
releases. Releases are local-first. Hosted CI may confirm the same lanes, but it
does not replace local release proof.

## Required Local Gates

Run these from the canonical repository root before creating a release receipt:

- `bash ci-fast-push.sh --full --no-push`
- `JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash ci-fast-push.sh --full --no-push`
- `bash ci-fast-push.sh --full` from a non-main release branch to push the
  branch, open or report the PR, and write `target/ci-fast/publish.json`
- `bash scripts/ci-phases.sh`
- `SIGNRAIL_ROLLBACK_TARGET=<previous-signed-release> bash ops/ci/artifact_support.sh`
- `bash ops/ci/release.sh`
- `bash ops/ci/proof-evidence.sh`
- `cargo test -p jeryu-wsversion --jobs 40`,
  `cargo run -q -p jeryu-wsversion -- inherit-guard`, and
  `cargo run -q -p jeryu-wsversion -- decide --range origin/main..HEAD --json`
  when the workspace version source, changelog roll-forward, or release bump
  policy changes.
- `cargo test -p jeryu-runnerd workcell --jobs 40` when the workcell control plane, tar safety, or frozen CI repair helpers change.
- `cargo test -p jeryu-readmodel -p jeryu-tui --jobs 40` when the workcells,
  agent-runs, codegraph/oracle dashboard, or TUI projection contract changes.
- `cargo test -p jeryu-readmodel --jobs 40 && cd web && npm run typecheck` when bootstrap feature flags or generated web contracts change.
- `cargo test -p jeryu-api --features web --jobs 40`
- `cargo test -p jeryu-api --features web --jobs 40 agent_runs` when the high-level agent-run route or PTY controls change.
- `cargo test -p jeryu-api --features web --jobs 40 r5_jail_loop` when the jailed workcell edit, namespaced branch export, PR creation, or CI evidence flow changes.
- `cargo clippy -p jeryu-api --features web --all-targets --jobs 40 -- -D warnings`
  when public API routes or repair bodies change.
- `bash ops/ci/codegraph-oracle.sh` when the schema-v3 codegraph oracle API or
  MCP contract changes.
- `cargo test -p jeryu-api --features web --jobs 40 control_plane`,
  `cargo test -p jeryu-mcp --jobs 40`,
  `cargo test -p jeryu-cli --jobs 40`,
  `npm --workspace @jeryu/web run typecheck`, and
  `npm --workspace @jeryu/web run test` when the JMCP control-plane REST, MCP,
  CLI, or web Intelligence surface changes.
- `cargo test -p jeryu-signrail --jobs 40 verify_release` when SignRail release
  verification changes.
- `just security`
- `just audit`

Full mode runs `ops/ci/verify-jeryu-env.sh --build-local --release-guard`.
Stop or quarantine retired-provider runners, `~/.jeryu`, old
`/home/ubuntu/jeryu`, local `:2224`, and monitored retired listeners before
recording release evidence.

## Local Merge Authority

Open the release or consolidation PR with `ci-fast-push.sh`. The push path
records `target/ci-fast/publish.json` with the branch, base, PR URL, PR number,
and commit that the final receipt must name. Local Jeryu mergeability plus the
gates above are the release authority; hosted GitHub Actions are mirror evidence
only. Direct wire pushes to `main` are not a supported release path; Jeryu
advances `main` only through the gated PR merge path or declared internal
post-merge automation, using server-side compare-and-swap ref updates.

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

## Rollback

Rollback restores the previous signed artifact, restores the pre-migration
SQLite copy when schema changed, reruns API/TUI/git smoke checks, and keeps
write traffic closed until the rollback receipt is attached.
