# Release Control Surface

Jeryu releases are local-first and evidence-backed. A release candidate cannot
be signed from hosted CI state alone.
This is the canonical release process doc for version source, changelog,
release commands, integrity/provenance evidence, and rollback guidance.
The step-by-step operator process lives in `docs/release-process.md`.

Release process doc: [docs/release-process.md](release-process.md).
SignRail artifact-support signing details:
[docs/signrail-release-signing.md](signrail-release-signing.md).

## Version Source

- Rust crate versions live in workspace manifests and `Cargo.lock`.
- User-facing changes are summarized in `CHANGELOG.md`.
- Release candidates record the Git commit SHA, artifact checksums, SBOM
  digests, and rollback target.
- SignRail artifact-support evidence uses the Git commit SHA as its release
  version unless the caller sets `SIGNRAIL_RELEASE_VERSION`.

## Required Gates

- `bash ci-fast-push.sh --full --no-push`
- `JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash ci-fast-push.sh --full --no-push`
- `bash scripts/ci-phases.sh`
- `./ops/ci/full.sh`
- `bash ops/ci/release.sh`
- `just security`
- `just audit`
- `bash ops/ci/proof-evidence.sh`
- `cargo test -p jeryu-runnerd workcell --jobs 40` when the workcell control plane, tar safety, or CI repair snapshot helpers change.
- `cargo test -p jeryu-readmodel --jobs 40 && cd web && npm run typecheck` when the workcells dashboard or generated web bootstrap contract changes.
- `cargo test -p jeryu-api --features web --jobs 40` when compatibility routes
  or guided repair bodies change.
- `cargo test -p jeryu-api --features web --jobs 40 r5_jail_loop` when the
  jailed workcell edit, namespaced branch export, PR creation, or CI evidence
  flow changes.
- `cargo clippy -p jeryu-api --features web --all-targets --jobs 40 -- -D warnings`
  when public API response contracts, `/api/v1/ecosystem`, or
  `/api/v1/ci/runs/{id}/evidence` change.
- `cargo test -p jeryu-signrail --test release_witness` and
  `cargo clippy -p jeryu-signrail --all-targets -- -D warnings` when release
  signing, artifact provenance, witness, or stage-receipt behavior changes.
- `cargo run -p jeryu-sandbox-linux --example jail_demo` and
  `cargo test -p jeryu-runnerd jailgun` when the workcell cell jail (the
  `jeryu-sandbox-linux` launch path) or the jailgun tar validators change.
- `cargo test -p jeryu-agentbridge` and `cargo test -p jeryu-egress` when the
  in-cell agent driver or the allowlist egress proxy changes.
  Workcell- and jailed-agent-authored changes flow through these same release
  gates and CI evidence with no privileged path; see `docs/workcell.md`.
- `cargo test -p jeryu-sandbox-linux` (escape_suite + cgroup_confinement) when
  the sandbox cgroup/Landlock enforcement or `ops/security/jeryu-runnerd.service`
  delegation unit changes — agent jobs must stay fail-closed on resource caps.

## Release Receipt

Every release receipt must be built from signed-commit provenance and record
the evidence that proves the candidate is safe to publish:

- source commit SHA, tag name, and the previous signed artifact checksum;
- `target/jankurai/` proof artifacts, including the release lane transcript,
  SBOM digests, provenance checksum, and any API route evidence for changed
  endpoints;
- SignRail `release.json`, `sbom.json`, `provenance.json`, `witness.json`, and
  `stage-receipts/{local,dev-canary,prod}.json` for artifact-support bundles;
- migration, restore, and rollback evidence, including the exact rollback
  target and the pre-migration SQLite copy when schema changed;
- the exact rerun command for any lane that failed during closeout, plus the
  local artifact path when one exists.

Latest closeout validation used explicit `--full` mode with 40 workers in both
local-native and GitHub-clean profiles: 1175 nextest tests, phase gates
PASS=9/PENDING=0/FAIL=0, proof-evidence Jankurai full scan score 92 caps 0, and
changed-file Jankurai diff/audit hard 0 caps 0. The GitHub-clean proof is
`JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash ci-fast-push.sh --full --no-push`.
Full mode runs `ops/ci/verify-jeryu-env.sh --build-local --release-guard` and
accepts either the canonical GitHub remote or the loopback local Jeryu remote
on `127.0.0.1:8787`. It rejects retired-provider runners, stale `~/.jeryu`
binaries, old `/home/ubuntu/jeryu`, and local `:2224` listener/remotes so
release evidence cannot be produced against the retired system. The local API
install under `~/.jeryu/bin/jeryu-api` is accepted only when it byte-matches the
repo-built API binary. Retired-CI sweeps of additional source roots run only
when `JERYU_CI_SOURCE_ROOTS` is set.

## Release Process

1. Run the required gates locally and keep the emitted artifacts under
   `target/jankurai/` until the release receipt is signed.
2. Verify the SQLite migration and restore receipts for the candidate commit.
3. For public API additions, attach the route-level test commands and response
   contract evidence, including typed repair fields and any digest-verifiable
   payload contract.
4. Build release artifacts from the signed commit only, then record checksums,
   SBOM digests, provenance paths, and the rollback target in the release
   receipt.
5. Sign artifact-support evidence with `jeryu-signrail sign-release`; local
   runs require `JERYU_SIGNRAIL_ED25519_SEED`, and GitHub Actions requires
   `SIGNRAIL_ED25519_SEED`.
6. Publish through a PR branch in local Jeryu first; local Jeryu mergeability
   plus green gates are authoritative, and GitHub is updated only as an
   explicit mirror after local `main` has the merge. Direct `main` pushes from
   `ci-fast-push.sh` require explicit `--push-main` and are not the default
   closeout path.
7. Run `bash ops/ci/release.sh` before signing the receipt so the release lane
   produces the build and receipt artifacts.
8. Tag only after the release receipt names the exact commit, prior rollback
   artifact, and gate evidence paths.

## Autonomy Gate

`jeryu/autonomy` check-runs are release evidence only. They record whether a PR
head is CI/risk eligible, human-required, or blocked, but they do not merge.
Release receipts must treat `Neutral` autonomy verdicts as advisory until the
auto-merge safety rework has explicit author/fork trust, signed reviewer
verification, populated changed-file evidence, and head-pinned merge tests.

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
For SignRail artifact-support evidence, rollback also requires the prior
stage-receipt set and matching artifact digest so `prod` receipts never point
at an unsigned or unverifiable bundle.

## Local-Only Boundary

The current live runtime is bound to `127.0.0.1`. LAN or public exposure waits
for auth, TLS, token rotation, backup restore evidence, and abuse-control
receipts.

## Production Readiness

Production launch is gated behind the local-only boundary above. When the
server moves off `127.0.0.1`, the launch checklist is:

- **Launch / production rollout:** promote only a signed, gate-green commit;
  deploy behind the unified `jeryu serve` listener; canary one node, widen on
  green, and keep the prior signed artifact staged for rollback.
- **Rate limiting:** the GitHub-shaped edge emits `X-RateLimit-*` headers
  already; enforce per-token rate limits and abuse-control 429s before any
  public exposure.
- **Monitoring:** scrape the `system.health` WS channel plus the runner-pool /
  queue read-model for liveness, queue depth, stuck nodes, and tag starvation;
  alert on `safe_to_merge=false` and sustained failed-job ratios.
- **Backup / restore:** snapshot the SQLite forge store and the gitd bare-repo
  root on a schedule; the rollback receipt above names the exact restore target.
