# Release Control Surface

Jeryu releases are local-first and evidence-backed. A release candidate cannot
be signed from hosted CI state alone.
This is the canonical release process doc for version source, changelog,
release commands, integrity/provenance evidence, and rollback guidance.
The step-by-step operator process lives in `docs/release-process.md`.

Release process documentation: [docs/release-process.md](release-process.md).
That document is the executable operator release process doc for required
local gates, PR-backed publication metadata, receipt contents, tagging, and
rollback.
SignRail artifact-support signing details:
[docs/signrail-release-signing.md](signrail-release-signing.md).

## Release Structure

The release structure is intentionally artifact-backed:

- Version source: `Cargo.toml`, `Cargo.lock`, and `jeryu-wsversion`.
- Changelog source: `CHANGELOG.md`.
- Release process doc: `docs/release-process.md`.
- Release control surface: `docs/release.md`.
- CI or script evidence: `target/ci-fast/publish.json` and `target/jankurai/`.
- Integrity and provenance evidence: `target/artifact-support/signrail/`.
- Rollback guidance: `docs/release.md#rollback`.

## Version Source

- `jeryu-wsversion` owns workspace version decisions. `decide` classifies the
  commit range, `apply` rewrites only `[workspace.package].version` and
  `CHANGELOG.md`, and `inherit-guard` rejects member manifests that pin their
  own version.
- Rust crate versions live in the root workspace manifest and `Cargo.lock`;
  workspace members must use `version.workspace = true`.
- User-facing changes are summarized in `CHANGELOG.md`.
- Release candidates record the Git commit SHA, artifact checksums, SBOM
  digests, and rollback target.
- SignRail artifact-support evidence uses the Git commit SHA as its release
  version unless the caller sets `SIGNRAIL_RELEASE_VERSION`.

### Main Push Version Bridge

When local Jeryu receives a non-delete update to `refs/heads/main`, the API
push bridge runs `jeryu-wsversion` in a temporary clone of the bare repo. It
decides the bump from the pushed range, applies the root workspace version and
changelog update, commits `chore(release): vX [skip-version]`, and pushes that
commit back to local `main`. The `[skip-version]` marker is the recursion guard:
the generated release commit is evidence for the next release receipt, but it
does not trigger another version bump.

This bridge does not tag, sign, publish artifacts, or bypass the PR-backed
release process. It only keeps the workspace version source aligned after local
`main` advances. Concurrent deliveries may race to write the same bump; the
first push wins and rejected duplicate pushes leave the single release bump on
`main`.

## Required Gates

- `bash ci-fast-push.sh --full --no-push`
- `JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash ci-fast-push.sh --full --no-push`
- `bash scripts/ci-phases.sh`
- `./ops/ci/full.sh`
- `SIGNRAIL_ROLLBACK_TARGET=<previous-signed-release> bash ops/ci/artifact_support.sh`
- `bash ops/ci/release.sh`
- `just security`
- `just audit`
- `bash ops/ci/proof-evidence.sh`
- `cargo test -p jeryu-runnerd workcell --jobs 40` when the workcell control plane, tar safety, or CI repair snapshot helpers change.
- `cargo test -p jeryu-readmodel -p jeryu-tui --jobs 40` when the workcells,
  agent-runs, codegraph/oracle dashboard, or TUI projection contract changes.
- `cargo test -p jeryu-readmodel --jobs 40 && cd web && npm run typecheck` when the generated web bootstrap contract changes.
- `cargo test -p jeryu-api --features web --jobs 40` when compatibility routes
  or guided repair bodies change.
- `cargo test -p jeryu-api --features web --jobs 40 r5_jail_loop` when the
  jailed workcell edit, namespaced branch export, PR creation, or CI evidence
  flow changes.
- `cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent` when
  the workcell run-agent route, typed path denial, structured event response, or
  sandbox-unavailable repair evidence changes.
- `cargo test -p jeryu-api --features web --jobs 40 agent_runs` when the
  high-level live agent-run route, PTY controls, failed-CI repair source, or
  typed control denials change.
- `cargo test -p jeryu-api --features web --jobs 40 workcell_export_slice`
  when workcell export gating, `jeryu-codegraph`, or the export PR changed-file
  derivation changes. The release receipt must include the typed denial evidence
  proving an out-of-slice diff creates no pull request.
- `bash ops/ci/codegraph-oracle.sh` when the codegraph schema, MCP catalog,
  oracle impact-pack contract, or `/api/v1/repos/{id}/codegraph/query` facade
  changes.
- `cargo test -p jeryu-api --features web --jobs 40 control_plane`,
  `cargo test -p jeryu-mcp --jobs 40`,
  `cargo test -p jeryu-cli --jobs 40`, and
  `npm --workspace @jeryu/web run typecheck && npm --workspace @jeryu/web run test`
  when the JMCP control-plane REST, MCP, CLI, or `/intelligence` web surface
  changes.
- `cargo clippy -p jeryu-api --features web --all-targets --jobs 40 -- -D warnings`
  when public API response contracts, `/api/v1/ecosystem`, or
  `/api/v1/ci/runs/{id}/evidence` change. Evidence digest or canonicalization
  changes must attach the route test transcript, clippy transcript, and
  Jankurai audit score to the release receipt.
- `cargo test -p jeryu-signrail --test release_witness`,
  `cargo test -p jeryu-signrail --jobs 40 verify_release`, and
  `cargo clippy -p jeryu-signrail --all-targets -- -D warnings` when release
  signing, release verification, artifact provenance, witness, or stage-receipt
  behavior changes.
- `cargo test -p jeryu-wsversion --jobs 40`,
  `cargo run -q -p jeryu-wsversion -- inherit-guard`, and
  `cargo run -q -p jeryu-wsversion -- decide --range origin/main..HEAD --json`
  when workspace versioning, changelog roll-forward, or release version source
  behavior changes.
- `cargo run -p jeryu-sandbox-linux --example jail_demo` and
  `cargo test -p jeryu-runnerd jailgun` when the workcell cell jail (the
  `jeryu-sandbox-linux` launch path) or the jailgun tar validators change.
- `cargo test -p jeryu-agentbridge` and `cargo test -p jeryu-egress` when the
  in-cell agent driver or the allowlist egress proxy changes.
  Workcell- and jailed-agent-authored changes flow through these same release
  gates and CI evidence with no privileged path; see `docs/workcell.md`.
- `cargo test -p jeryu-sandbox-linux` (escape_suite + cgroup_confinement +
  secret_paths_denied + memory_oom_kill) when the sandbox cgroup/Landlock
  enforcement or `ops/security/jeryu-runnerd.service` delegation unit changes —
  agent jobs must stay fail-closed on resource caps and the jail must keep
  denying secret/other-repo reads.
- `bash ops/ci/coverage.sh` when workcell crate tests change: it enforces the
  per-crate src-coverage ratchet (`ops/ci/coverage-baseline.json`) over
  `jeryu-api`, `jeryu-egress`, and `jeryu-codegraph`. Coverage may not drop below
  the recorded floor; raise it deliberately with
  `JERYU_COVERAGE_UPDATE_BASELINE=1` and commit the updated baseline.

## Release Receipt

Every final release receipt is `jeryu.release-receipt/v2`. It is built from
signed-commit provenance and fails closed unless the candidate is safe to tag:

- source commit SHA, tag name, `signed-commit.txt`, and a locally successful
  `git verify-commit --raw <sha>` transcript;
- publication metadata from `target/ci-fast/publish.json`, written by the
  PR-backed `ci-fast-push.sh` publish path, including branch, base, PR URL,
  PR number, and commit;
- previous signed release artifact evidence: release tag, `jeryu` checksum,
  `jeryu.sig` checksum, and `jeryu.pem` checksum;
- `target/jankurai/` proof artifacts, including the release lane transcript,
  SBOM digests, provenance checksum, and any API route evidence for changed
  endpoints;
- MCP/catalog trust evidence for changed local agent-facing commands:
  `agent/tool-adoption.toml`, the pinned `ops/ci/security-tools.sh` transcript,
  `cargo test -p jeryu-mcp --test mcp_conformance --jobs 40`, and any composed
  route/tool contract lane such as `bash ops/ci/codegraph-oracle.sh`;
- the `jeryu-wsversion decide --json` output for the released range, plus
  inherit-guard evidence proving every workspace member inherits the root
  version;
- `artifact-support-evidence.tar.gz` plus SignRail `release.json`,
  `sbom.json`, `provenance.json`, `witness.json`, `summary.json`, and
  `stage-receipts/{local,dev-canary,prod}.json`, copied into the release bundle
  under `artifact-support-signrail/`;
- migration, restore, and rollback evidence, including the exact rollback
  target and the pre-migration SQLite copy when schema changed;
- `SHA256SUMS`, generated by `scripts/emit-release-receipt.sh` over every
  bundle file except `release-receipt.json` and `SHA256SUMS` itself; the
  receipt records the checksum manifest digest.

Release receipts must include current local-native and GitHub-clean full-mode
transcripts rather than historical score claims. The GitHub-clean proof command
is
`JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash ci-fast-push.sh --full --no-push`.
Full mode runs `ops/ci/verify-jeryu-env.sh --build-local --release-guard` and
accepts either the canonical GitHub remote or the loopback local Jeryu remote on
`127.0.0.1:8787`. It rejects retired-provider runners, stale `~/.jeryu`
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
   Workcell run-agent additions must include the typed path-denial evidence and
   either structured event output or honest sandbox-unavailable evidence.
   Agent-run additions must include live PTY event/control evidence and typed
   denial evidence for stale, finished, or unsupported control paths.
   Codegraph oracle additions must include `bash ops/ci/codegraph-oracle.sh`.
   JMCP control-plane additions must include route tests for explicit mirror
   and artifact absence, MCP catalog evidence, CLI grammar evidence, and web
   route smoke for `/intelligence`.
4. Build only from a signed commit. `scripts/emit-release-receipt.sh` verifies
   the commit with `git verify-commit --raw <sha>` and writes the transcript to
   `signed-commit.txt`.
5. Sign artifact-support evidence with `jeryu-signrail sign-release` through
   `bash ops/ci/artifact_support.sh`. Set `SIGNRAIL_ROLLBACK_TARGET` to the
   previous signed release tag; local runs require `JERYU_SIGNRAIL_ED25519_SEED`,
   and GitHub Actions requires `SIGNRAIL_ED25519_SEED`.
6. Publish through the PR-backed `ci-fast-push.sh` path before tagging:
   `bash ci-fast-push.sh --full` from a release branch pushes the branch, opens
   or reports the PR, and writes `target/ci-fast/publish.json`. Direct `main`
   pushes require explicit `--push-main`; final receipts reject that metadata
   unless `JERYU_RELEASE_DIRECT_MAIN_ESCAPE=1` is also set.
7. Run `bash ops/ci/release.sh`. It preflights the signed commit and
   `target/ci-fast/publish.json`, runs `ops/ci/artifact_support.sh` if the
   signed artifact-support evidence is not already present, copies that
   evidence into the bundle, records binary/SBOM/provenance/cosign digests,
   writes `rollback.json`, emits `release-receipt.json`, and writes
   `SHA256SUMS`.
8. Tag only after the v2 receipt names the exact signed commit, PR publication
   path, prior signed rollback artifact, stage receipts, and checksum manifest.

## Autonomy Gate

`jeryu/autonomy` check-runs are release evidence only. They record whether a PR
head is CI/risk eligible, human-required, or blocked, but they do not merge.
Release receipts must treat `Neutral` autonomy verdicts as advisory until the
auto-merge safety rework has explicit author/fork trust, signed reviewer
verification, populated changed-file evidence, and head-pinned merge tests.

## Integrity And Provenance

The security lane writes SBOM, vulnerability scan, provenance, and signing
artifacts under `target/jankurai/security/sbom`. Final release receipts require
the SPDX SBOM checksum, CycloneDX checksum, provenance checksum, and cosign
transcript checksum.

## Rollback

Every release receipt names the previous signed artifact, binary checksum,
signature checksum, and certificate checksum. Rollback means restoring the
previous signed artifact, restoring the pre-migration SQLite copy when schema
changed, and re-running the smoke commands for API, TUI, and Git fetch/clone
before reopening write traffic.
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
