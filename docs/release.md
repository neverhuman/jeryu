# Release Readiness

This document is the short release-readiness reference for `jeryu`.
The release path is owned by `src/release.rs` and `src/secrets.rs`; production
promotion is pipeline-driven, not an ad hoc shell flow.

**For the canonical agent-first release policy, branch protection settings,
risk tiers, evidence directory contract, rollback ladder, and the
`jeryu/release-ready` composite gate, see
[`docs/release-policy.md`](release-policy.md). Machine-readable source of truth:
[`release.policy.toml`](../release.policy.toml).**

## What must be true

- The target ref or SHA has a green upstream pipeline.
- `release reconcile` has claimed the release attempt and attached the
  release-execution pipeline state.
- `release preflight` passes the infrastructure checks it actually enforces:
  SSH, Vault, registry, and disk.
- Canary evidence is complete for the version being promoted.
- The release handoff artifacts exist under `ops/releases/<version>/`.
- Production promotion is only attempted after the canary path is complete and
  the handoff/validation artifacts are present.

## Evidence and state

Release state is recorded in the database, mainly through:

- `release_attempts`
- `release_secret_sets`
- `secret_audit_events`

The default release repo root is `/home/ubuntu/dougx`, unless overridden by
`JERYU_RELEASE_REPO_ROOT` or `settings.release.repo_root`.

## Commands

- `jeryu release status`
- `jeryu release watch`
- `jeryu release reconcile`
- `jeryu release preflight`
- `jeryu release doctor`
- `jeryu release promote-prod`
- `jeryu secrets rotate`
- `jeryu secrets finalize`
- `jeryu secrets report`
- `jeryu secrets recover`

## Source of truth

- Version: `VERSION`
- Changelog: `CHANGELOG.md`
- Release process notes: `docs/release.md`
- Release evidence: `ops/releases/<version>/`
- Security evidence: `target/jankurai/security/evidence.json`
- Backup evidence: `just state-proof`
- Monitoring evidence: `target/jankurai/ux-qa.json`
- Abuse-control evidence: `src/admission.rs`, `src/secrets.rs`, `agent/security-policy.toml`

## Release structure

The release structure is intentionally artifact-backed:

- Version source: `VERSION`
- Changelog source: `CHANGELOG.md`
- Release process doc: `docs/release.md`
- Release policy doc: `docs/release-policy.md`
- Release workflow: `.github/workflows/release.yml`
- Local release script: `ops/ci/release-lane.sh`
- Release-ready receipts: `.jeryu/release-ready/receipts/*.json`
- Published release evidence: `ops/releases/<version>/`

Do not ship from an ad hoc checklist. The release gate requires the receipt
directory before a non-dry-run release can pass.

## Release process

The release process is pipeline-driven via GitHub Actions. The single source
of truth lives at [`ops/ci/release-lane.sh`](../ops/ci/release-lane.sh); local
rehearsals and CI both call into it. End-to-end stages, gates, and approvals
are defined in [`docs/release-policy.md`](release-policy.md).

## Changelog

All releases are tracked in [`CHANGELOG.md`](../CHANGELOG.md). For each
release we record: the version, date, summary of changes, the release PR,
and the rollback target. Releases are immutable — never re-tag; ship a
patch instead.

## Rollback

- Stop promotion immediately if `release doctor` or `release preflight` fails.
- Prefer promoting the previous known-good release over ad hoc manual fixes.
- Keep the previous handoff and canary artifacts until the next release is proven safe.
- Record the rollback reason in the release attempt row before retrying.

## Launch gates

- Security gate: secret scan, dependency review, and SBOM evidence must be green.
- Backup gate: the state proof must pass before a release attempt is promoted.
- Monitoring gate: the release canary and UX smoke output must be available for the current version.
- Abuse-control gate: admission policy and secrets rotation must still pass before any prod handoff.

## Stop conditions

- Stop on a failed preflight.
- Stop on missing canary or handoff evidence.
- Stop on a `release doctor` result that says the version is not safe to
  reconcile or promote.
- Do not bypass the pipeline gate with manual production commands.

## Cost stop conditions and release sign-off

<!-- evidence-kind: cost-budget -->

This repo has no paid AI calls and no per-request billing surface in
product code, so there is no per-token spend cap and no paid-API quota
to declare. The only spend surface is GitHub Actions runner minutes;
the effective budget is the per-job `timeout-minutes` quota plus the
GitHub plan minutes pool, with no further internal quota tracker
because there is no paid call to meter. Source-of-truth budgets:

- `.github/workflows/rust.yml` — `timeout-minutes` per lane (fmt 10,
  clippy 25, build 30, install-smoke 30, test-select 20, test-lib 30,
  test-integration 45, tui-smoke 15, supply-chain 20, witness 20,
  vrc-map / vrc-plan / aer-scan 15 each, scheduled-hardening 30).
- `.github/workflows/jankurai.yml` — `timeout-minutes: 30` for the
  weekly audit lane.
- Both workflows pin `concurrency.cancel-in-progress: true`, so
  superseded refs free their minutes immediately.
- See `docs/testing.md` "Cost budgets and stop conditions" for the full
  table and per-test iteration caps.

Real env-var kill switches enforced by product code:

- `JERYU_RELEASE_REPO_ROOT` — pins the release repo root
  (`src/release.rs`, `src/secrets.rs`); also configurable via
  `settings.release.repo_root`.
- `JERYU_POOL_SHUTDOWN_TIMEOUT_SECS` — bounds runner-manager SIGQUIT
  wait (`src/config.rs`).
- `JERYU_POOL_CARGO_LEASE_RECOVERY_SECS` — caps stale cargo-lease
  recovery wait (`src/cache.rs`).
- `JERYU_TUI_CAPTURE_MAX_WAIT_MS`, `JERYU_TUI_CAPTURE_MIN_WAIT_MS` —
  bound TUI capture jobs.
- `JERYU_SYSTEM_GIT`, `JERYU_GIT_MODE`, `JERYU_MIRROR_ENABLED`,
  `JERYU_MIRROR_REMOTE` — pin git subprocess target and disable
  mirroring in tests.
- `JERYU_CARGO_CACHE`, `JERYU_CARGO_CACHE_ROOT`,
  `JERYU_CARGO_TARGET_ISOLATE`, `JERYU_CARGO_INCREMENTAL` — gate the
  shared cargo cache surface.

### Release checklist line

- Confirm no new paid AI or external network surface has been added
  without a matching entry in `docs/testing.md` "Cost budgets and stop
  conditions" (budget, kill switch, owner). If a new surface is added,
  this release does not ship until that entry lands.
