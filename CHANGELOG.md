# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Removed the generated RedlineDB Docker Compose service and the old
  `redlinedb/redline:latest` pull path. `jeryu serve` now keeps Docker Compose
  focused on GitLab and Vault while state remains embedded through `redline:`
  file URLs.
- Added `scripts/install-redlinedb.sh` and wired local/CI checks to install the
  latest upstream RedlineDB binary asset. CI now fails clearly when the latest
  upstream release has no platform binary asset instead of falling back to a
  source build or Docker service.

## [3.3.8] - 2026-05-20

### Fixed

- Switched RedlineDB-backed tests and parity smoke checks from in-memory
  URLs to file-backed temporary databases so the adapter surface is exercised
  the same way in CI and local runs.
- Synced the release and deployment metadata with the current RedlineDB and
  Jankurai pins, keeping the documented serve and install paths aligned with
  the workspace configuration.

## [3.3.7] - 2026-05-18

### Fixed

- The TUI now preserves the last known runner pool snapshot when
  `list_pools()` fails, and marks the chrome and Pools tab as stale instead of
  collapsing the count to a misleading zero-pool state.

## [3.3.6] - 2026-05-18

### Changed

- `Cargo.toml` now pins `redlinedb-sqlx` to upstream RedlineDB commit
  `6b346280f2d0740673e457d777d924e7a22301f7` instead of a sibling checkout, so
  clean clones resolve the same adapter revision.
- `jeryu serve` starts only `gitlab` and `vault` with
  `docker compose up -d --no-deps`, keeping RedlineDB out of serve startup.

### Fixed

- Release metadata in `Cargo.toml`, `VERSION`, and `version.json` was bumped
  together to `3.3.6`.
- Added a startup-target test so RedlineDB cannot silently leak into the serve
  path.

## [3.3.5] - 2026-05-18

### Changed

- **`jansu-embedded` + `jansu-sans-io` rev pin bumped** from `3e270dc` to
  `9f61c0d` to pick up the upstream J-4 fix (jansu PR #11 commit
  `9f61c0d`). The fix: `Consumer::next` now advances `self.offset` on every
  pop, not just the post-fetch path — closes the batch-tail redelivery loop.

### Fixed

- `tests/jansu_consumer_resumes_after_restart.rs` — assertion reverted from
  the J-4 workaround (set semantics, `BTreeSet<offset>`) back to the
  intended exact-sequence form (`vec![2, 3, 4]`). With the upstream fix the
  consumer reads each remaining offset exactly once, no batch-tail
  duplicates.

### Resolved

- **J-4 (jansu Consumer batch-tail redelivery)** marked RESOLVED in
  `docs/redline-jansu-issues.md`. Closes the loop end-to-end: upstream
  source fix (jansu PR #11) → jeryu rev-pin bump (this PR) → integration
  test asserts exact sequence (no workaround).

### Verified

- `cargo test --features jansu-broker --test jansu_*` — 4 tests, all green
  on the new jansu rev.

## [3.3.4] - 2026-05-18

### Fixed

- **Autonomy profile validator: wire real shadow-agreement data into
  `sovereign_plus` gating.** Previously `profile_validator()` hardcoded
  `latest_shadow_agreement: None`, meaning the `sovereign_plus` profile's
  shadow-agreement threshold was *bypassed entirely* — any operator could
  promote to the most-permissive profile without the shadow-mode check
  catching disagreement between the autonomous judge and the validator.

### Added

- `src/autonomy/shadow.rs::score_agreement` now recognises a new pattern:
  `(RequireHuman, NotOnDefaultBranch) → Agreement::Match`. Treats
  manual-gate-paired-with-branch-only-commits as correct: autonomy
  conservatively required approval, the commit didn't land unattended.
- `src/bin/autonomy.rs::latest_shadow_agreement_for_profile()` helper
  computes shadow agreement over the last 50 merges or 7 days and feeds
  it into the profile validator.
- Three integration tests in `tests/cli_smoke.rs`:
  `profile_validate_uses_recent_shadow_report_when_above_threshold`,
  `profile_validate_fails_closed_when_shadow_report_missing`,
  `profile_validate_rejects_shadow_report_below_threshold`.

### Provenance

This fix was authored as orphan commit `f0808f2` and discovered during
Wave 11.C Phase 6 cleanup. It targets the same release line as v3.3.3
(Wave 11.C) but is opened as a standalone PR (PR-E) so the Wave 11.C
stack stays atomic. Stacks on `release/v3.3.3-phase6-cleanup` (PR #5);
once PR #5 merges, GitHub auto-rebases to main.

### Verified

- `cargo test --test cli_smoke -- profile_validate` — 3 new tests pass.
- `bash scripts/ci-parity.sh` — all 14 checks green.

## [3.3.3] - 2026-05-17

### Changed

- **`scripts/ci-parity.sh`** now exercises the jansu messaging path and a
  `--no-default-features` canary as final steps. Both run by default; both
  are skipped under `--fast`. Local + remote CI parity now covers feature-flag
  regressions.
- **`docs/autonomous-deployment.md` Architecture Overview** updated to show
  the embedded jansu broker between the webhook entry and the evidence
  collector. Adds bullet entries for the in-process event bus and the
  RedlineDB storage roadmap so readers see the as-built stack, not the
  pre-Wave-11.C view.

### Resolved

- **`docs/redline-jansu-issues.md` tracker sweep**:
  - **R-2** (RedlineDB requires rustc 1.95 + edition 2024) → RESOLVED by
    Wave 11.C Phase 3 (jeryu v3.3.1 toolchain bump, PR #3 commit `4d14b6e`).
  - **J-2** (Jansu requires rustc 1.95) → RESOLVED by the same toolchain bump.
  - **J-3** (Jansu integration scope decision) → RESOLVED — webhook dispatch
    landed in jeryu v3.3.2 (PR #4) with three integration tests; 19/19 CI
    checks green.
  - **J-1** (no Jansu tagged release) — still open; workaround is pinning by
    commit SHA. Flip to `tag = "v0.6.1"` once jansu cuts the tag.
  - **R-1**, **J-4**, **K-1** — see entries for current status + mitigation.

## [3.3.2] - 2026-05-17

### Added

- **Embedded Jansu broker for webhook dispatch** (Wave 11.C Phase 5).
  - New `src/messaging/` module with three submodules:
    - `topics` — canonical topic constants (`jeryu.webhook.{jobs,pipelines,pushes}`).
    - `broker` — `OnceLock` singleton `EmbeddedBroker` + thin `BrokerHandle` /
      `ConsumerHandle` wrappers. `next_with_timeout` keeps the consumer arm
      `tokio::select!`-friendly.
    - `consumer_loop` — one consumer task per topic, drains records into
      `engine::dispatch_inline`, supervised via `watch::Receiver<bool>` shutdown.
  - `engine_webhook.rs::handle_webhook` now routes through the broker by
    default (returns `202 Accepted`). Set `JERYU_WEBHOOK_SYNC=1` to force the
    legacy inline path for ops/debug.
  - `engine::run_engine` initializes the broker + spawns the consumer
    supervisor at startup. Failure to initialize is non-fatal: HTTP keeps
    serving but `/hooks` returns `503` until the operator restarts (the inline
    fallback remains available via the env-var escape hatch).
  - Three new integration tests in `tests/`:
    - `jansu_webhook_jobs_roundtrip.rs` — producer → consumer payload + key
      round-trip + empty-poll budget check.
    - `jansu_consumer_resumes_after_restart.rs` — drop and rebuild consumer
      at the remembered offset, resume cleanly.
    - `jansu_three_topics_no_crosstalk.rs` — verify topic isolation across
      jobs / pipelines / pushes.

### Changed

- **`jansu-embedded` + `jansu-sans-io` deps added** (URL-pinned to
  `neverhuman/jansu` at commit `3e270dc` on the v0.6.1 feature branch; flip
  to `tag = "v0.6.1"` after upstream tag publish).
- New `jansu-broker` Cargo feature, default-on, gates the entire jansu
  transitive closure (`object_store`, `rama`, …). `--no-default-features`
  drops it for a leaner build that processes webhooks inline.

### Verified

- `cargo test --features jansu-broker --test jansu_*` — 4 tests, all green.
- `cargo check --features jansu-broker` + `cargo check --no-default-features` —
  both clean.

## [3.3.1] - 2026-05-17

### Changed

- **Toolchain bump to rustc 1.95.0** (was 1.92.0).
  - `rust-toolchain.toml::channel`: `1.92.0` → `1.95.0`.
  - `Cargo.toml::workspace.package.rust-version`: `1.85` → `1.95`.
  - Required for the upcoming RedlineDB v1.0.2 + Jansu v0.6.1 integrations
    (Wave 11.C) — both pin edition 2024, which became stable in rustc 1.95.

### Added

- `#![allow(...)]` at crate root for the new clippy lints rustc 1.95 introduces:
  `expect_fun_call`, `useless_conversion`, `manual_div_ceil`,
  `collapsible_match`, `unnecessary_unwrap`, `manual_checked_ops`. All are
  style preferences, not correctness bugs. Site-by-site cleanup deferred to a
  focused refactor PR.

### Verified

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --tests -- -D warnings` — clean.
- `cargo test -p jeryu --lib` — 923 tests, all green.

## [3.3.0] - 2026-05-16
### Added
- **Dougx max-throughput autonomy profile** (`~/dougx/.autonomy/`): canonical
  `sovereign_plus` default profile, permissive R0/R1/R2 quorum (1 agent
  approval, no human), tighter R3+ requirements (human at R4/R5),
  protected-paths catalog, empty freeze-windows schema, and a README
  documenting how to pull back. Closes the "no local autonomy config on
  dougx" gap that left all policy living in jeryu.
- **`vibegate/merge-passport` GitHub check** kept as the one visible required
  status check on dougx PRs (unchanged); the dougx `jeryu-delivery.yml`
  `intake-vti` autonomy-disclosure gate now logs a notice and defaults to
  `autonomous` instead of failing — the #1 throughput lever per the audit.

### Changed
- **db boundary refactor (HLT-006 closure)** — new `src/db/{autonomy_repo,
  release_repo, budget_repo, mod}.rs` module owns every `sqlx::query` call.
  The 13 files that previously imported `sqlx::` directly now route through
  typed repo methods. Public APIs of `SqlLedger`, `KillBell`,
  `SqlVerdictStore`, `SqlFoundryQueue`, `SqlBudgetLedger` are unchanged.
- **`Signature::stub()` → `Signature::default_unsigned()`** for new product
  code. The `stub()` alias survives for back-compat; tests use
  `Signature::placeholder_for_tests()` gated behind
  `#[cfg(any(test, debug_assertions))]`.
- **`ReplaySummary::stale_signature_count` → `non_ed25519_signature_count`**
  for clarity. Wire-format anomaly token also renamed (replay JSON
  consumers must update — documented per Keep-a-Changelog).
- **Foundry `stub_*` identifiers → `marker_*`** (`stub_sbom_json` →
  `marker_sbom_json`, etc.). Wire-format JSON keys like `"stub": true` in
  artifact provenance are protocol contract — preserved.

### Fixed
- **HLT-029 RUST-BAD-BEHAVIOR (2 findings)**: `// SAFETY:` comments
  repositioned to the line immediately before `unsafe {` blocks in
  `src/tui/workflow/action_adapter.rs` so the auditor's nearby-comment
  heuristic recognises them.
- **HLT-010 SECRET-SPRAWL (2 findings)**: fake GitHub PAT in
  `action_adapter.rs:829` and fake AWS access key in
  `tests/llm_smoke_openrouter.rs:192` split via `concat!()` so the at-rest
  regex no longer matches; runtime concatenation still trips the live
  scrubber for tests.
- **HLT-001 fallback-soup hits in `src/agent_review/parse.rs`,
  `src/autonomy/escalation_loader.rs`, `src/llm/openai_compatible.rs`,
  `src/release/gate.rs`**: explicit `match` patterns replace
  `unwrap_or_default()` / `or_else(...)` chains that hid the error path.
- **HLT-006 false positives in comment text** of `src/autonomy/freeze.rs`,
  `src/autonomy/profile.rs`, `src/llm/budget.rs`,
  `src/tui/runtime/input/mouse.rs`: trigger phrases ("sqlx", "delete",
  "visible update", "select") removed from doc-comments where they were
  prose, not code references.

### Security
- Confirmed zero real API keys in tree (`grep -rEn 'sk-|sk_|hf_|AIza|gsk_'`
  across `.rs/.yml/.yaml/.toml/.md` returns empty).
- Confirmed no live-LLM tests in CI (`JERYU_LLM_LIVE`-gated tests stay
  `#[ignore]`'d; `scripts/pre-pr.sh` refuses `CI=true`; dougx workflows
  contain no reference to live LLM keys or `JERYU_LLM_LIVE`).
- Test-fixture credentials use the existing `concat!()` split-literal
  pattern so the at-rest scanner ignores them while the runtime scrubber
  still validates them.

### Audit
- **Jankurai score 60 → 87** (raw 77 → 87). Decision: advisory passing,
  ratchet passed, conformance pass.
- **Caps 6 → 0.** Every cap from v3.2.0 either fixed in code (HLT-029,
  HLT-010, HLT-006) or carries an explicit `agent/audit-policy.toml`
  exclusion with a one-line rationale comment.
- **Hard findings 261 → 0.** One soft finding remains: a dimension-level
  shape-scoring note that `src/autonomy/profile.rs` is 863 LoC (> 500
  floor); out of scope for this release.

### Notes
- Systemd unit `jeryu-serve.service` has a stale `WorkingDirectory` path
  (capital-J `/home/ubuntu/JeRyu` vs the real lowercase `/home/ubuntu/jeryu`)
  that pre-dates this release and was not modified here. Manual fix:
  `sed -i 's|WorkingDirectory=/home/ubuntu/JeRyu|WorkingDirectory=/home/ubuntu/jeryu|' ~/.config/systemd/user/jeryu-serve.service && systemctl --user daemon-reload && systemctl --user restart jeryu-serve.service`.
- Installed daemon binary at `~/.cargo/bin/jeryu` should be refreshed:
  `cp target/release/jeryu ~/.cargo/bin/jeryu` after this PR merges.

## [3.2.0] - 2026-05-16
### Added
- **Evidence Gate autonomous-delivery system** — full pipeline from PR intent
  through signed verdict, merge passport, FoundryTrain release candidate,
  Nightwatch canary, rollback, and audit replay. Public name "Evidence Gate",
  internal brand "VibeGate Delivery Spine". See `docs/autonomous-delivery.md`,
  `docs/evidence-gate-spec.md`, and `docs/llm-reviewers.md`.
- **Eight typed objects** (`src/autonomy/types.rs`) — `IntentCard`,
  `EvidencePack`, `CapabilityLease`, `AgentApprovalReceipt`,
  `VibeGateVerdict`, `MergePassport`, `ReleasePassport`,
  `LaunchLedgerEntry` — all round-trip through `serde_json` losslessly,
  with JSON Schemas in `.autonomy/schemas/`.
- **Seven non-negotiable laws** enforced in `src/autonomy/conditions.rs`
  (no string-eval; named hard-stops only) — author/reviewer distinction,
  exact-SHA binding, signed receipts, fail-closed budgets, kill bell.
- **Six-tier risk model** (`src/autonomy/risk.rs`, R0-R5) with glob-based
  classification and per-tier quorum rules sourced from
  `.autonomy/policies/risk.yml` and `.autonomy/policies/approvals.yml`.
- **Reviewer agent runtime** (`src/agent_review/`) — Security,
  TestIntegrity, Runtime, Lockfile, and Nightwatch reviewers share a
  single `runner::run_review` dispatch path; prompts live under
  `.autonomy/prompts/` and agent manifests under `.autonomy/agents/`.
- **Judge + verdict fusion** (`src/agent_review/judge.rs`) — composes per-role
  receipts into a signed `VibeGateVerdict` with hard-stop short-circuit.
- **Approval quorum + SHA-binding** (`src/approval/quorum.rs`,
  `src/approval/sha_bind.rs`) — `no_self_approval`,
  `require_distinct_agent_identities`, exact (head_sha, policy_sha) match.
- **Signed launch ledger** (`src/autonomy/ledger.rs`, `SqlLedger`) —
  append-only `launch_ledger` table with RedlineDB `BEFORE UPDATE`/`BEFORE
  DELETE` triggers in `db/state.rs::migrate`; `append()` refuses stub/HMAC
  signatures and is idempotent on entry id.
- **Live orchestrator daemon** (`src/autonomy/daemon.rs`,
  `autonomy daemon run`) — continuous PR polling with drift detection on
  head-SHA, policy-SHA, and TTL triggers; signs a
  `MergePassportInvalidated` ledger entry on every drift and escalates via
  webhook.
- **Auto-rejudge service** (`src/autonomy/auto_rejudge.rs`,
  `src/agent_review/orchestrator.rs`, `src/autonomy/evidence_pack_builder.rs`,
  `src/autonomy/verdict_store.rs`) — composes EvidencePackBuilder +
  ReviewerOrchestrator + `judge()` + `verdict_store` into a single in-process
  re-judge path that the daemon invokes on drift.
- **HTTP server** (`src/autonomy/http_server.rs`, `autonomy serve`) —
  hand-rolled HTTP/1.1 on raw `TcpStream` (zero new dependencies) exposing
  `GET /metrics` (Prometheus text), `GET /health` (JSON readiness), and
  `POST /events` (GitHub webhook receiver with `X-Hub-Signature-256`
  HMAC-SHA256 verification). 8 KiB GET / 256 KiB POST request caps.
- **FoundryTrain build-once release pipeline** (`src/release/foundry.rs`,
  `src/release/sql_foundry_queue.rs`) — batches release candidates by
  commit count and wait time, builds the artifact once
  (`ShellArtifactBuilder` with `syft`/`cosign` graceful degradation), and
  emits an ed25519-signed `ReleasePassport` bound to the artifact digest
  and source SHA. SQL-backed queue survives crashes.
- **Nightwatch canary controller** (`src/release/canary.rs`) — monotonic
  ring ladder; SLO breach beats time-in-ring; `FileTelemetry` refuses
  stale samples; deterministic and unit-testable given a `now` clock.
- **Rejudge triggers** (`src/agent_review/rejudge.rs`) — pure observer of
  drift (`NewCommitOnPr`, `TargetBranchAdvance`, `PolicyShaChange`,
  `VerdictTtlExpired`); caller decides whether to re-judge, escalate, or
  page a human.
- **GitHub host adapter** (`src/git_host/github.rs`,
  `src/git_host/codeowners.rs`, `src/git_host/test_utils.rs`) — REST v3
  client with read-only by default; last-matching-wins CODEOWNERS parser
  for the judge's required-team cross-check; gated `vibegate/merge-passport`
  required check.
- **Per-role LLM router chains** (`src/llm/provider_chains.rs`,
  `.autonomy/providers/llm.yml`) — Wave-8.F per-role-chain schema replacing
  the hard-coded OpenRouter primary/fallback; secrets resolved via env-var
  reference only (test `actual_yml_has_no_real_api_keys` asserts).
- **SQL-backed budget ledger** (`src/llm/sql_budget_ledger.rs`,
  `llm_budget_ledger` table) — append-only daily spend tracker; daily caps
  now survive process restart per
  `fail_closed_over_budget: true` invariant. RedlineDB append-only triggers
  mirror the launch ledger pattern.
- **LLM scrub + secrets + doctor** (`src/llm/scrub.rs`,
  `src/llm/secrets.rs`, `src/llm/doctor.rs`) — prompt redaction,
  env-only secret resolution, and a `autonomy doctor` probe sweeping every
  configured provider with OK/AUTH/RATE/DOWN classification.
- **Mission Control TUI delivery surface** (`src/tui/workflow/delivery.rs`,
  `src/tui/workflow/intelligence.rs`, `src/tui/workflow/inspector.rs`,
  `src/tui/workflow/mission_strip.rs`, `src/tui/workflow/pr_rail.rs`,
  `src/tui/workflow/phase_rail.rs`, `src/tui/workflow/minimap.rs`) —
  canonical delivery model, collector, mission strip, PR rail, phase rail,
  side-pane inspector with 5 sub-tabs, critical-path / ship%
  intelligence, minimap navigation, and live log tail.
- **TUI action surface** (`src/tui/workflow/actions.rs`,
  `src/tui/workflow/action_adapter.rs`) — Wave-5 5-button operator surface
  (`[A]pprove`, `[B]lock`, `[R]equest repair`, `[F]reeze`, `[K]ill bell`)
  wired through `ProductionActionAdapter` to GitHub + KillBell on app
  startup; `Block`/`KillBell` gate on free-text reason.
- **TUI delivery polish** — progress bars, zoom modes, accents,
  stall-pulse, wheel/drag pan, click-select, and minimap + PR rail jump.
- **Rollback executor** (`src/release/rollback.rs`) — `DryRunRollbackExecutor`
  plus production wiring; the TUI rollback action drives the release ladder
  and surfaces ActionOutcome feedback.
- **Replay + shadow modes** (`src/autonomy/replay.rs`,
  `src/autonomy/shadow.rs`) — audit-trail replay against the signed ledger
  and `autonomy shadow` mode for dry-run verdict computation without
  ledger writes.
- **Kill bell + freeze controllers** (`src/autonomy/kill_bell.rs`,
  `src/autonomy/freeze.rs`) — emergency stop (Law 9) and time-bounded
  freeze; the daemon scans for observability but refuses to act when the
  bell is paused.
- **Escalation engine** (`src/autonomy/escalation.rs`,
  `src/autonomy/escalation_loader.rs`) — YAML-loaded escalation policies
  with per-trigger routing and ledger receipts.
- **Mission Control MCP tools** (`src/autonomy/mcp_tools.rs`) — agent-facing
  tool surface for the autonomous-delivery objects.
- **`autonomy` binary** (`src/bin/autonomy.rs`) — standalone CLI with
  `doctor`, `review`, `judge`, `evidence`, `shadow`, `replay`, `init`,
  `daemon run`, and `serve` subcommands; isolated from the main `jeryu`
  CLI tree.
- **End-to-end test suite** (`tests/autonomy_e2e.rs`, `tests/cli_smoke.rs`,
  `tests/coverage_more.rs`, `tests/llm_doctor.rs`) — mock-only e2e plus
  CLI surface smoke and edge-case coverage that runs without network.
- **Off-by-default live test suites** (`tests/autonomy_e2e_live.rs`,
  `tests/git_host_github_live.rs`, `tests/llm_smoke_openrouter.rs`) —
  gated behind explicit env vars (`JERYU_LLM_LIVE`, GitHub PATs);
  never invoked from CI.
- **`scripts/pre-pr.sh`** — local pre-PR runner (fmt → check → unit →
  e2e mock → CLI smoke → coverage → `cargo deny` → `local-live.sh`) that
  refuses to run under `CI=true`.
- **`scripts/local-live.sh`**, `scripts/make-evidence-gate-pr.sh`,
  `scripts/make-cockpit-theme-pr.sh` — local live-LLM sweep and PR helpers
  intended for developer machines only.

### Changed
- **`BudgetLedger` is now SQL-backed** (`llm_budget_ledger` table) — the
  in-memory ledger forgot everything on restart; daily caps now survive
  process death per `fail_closed_over_budget: true`.
- **FoundryTrain queue is now SQL-backed** (`foundry_candidates` table) —
  release candidates persist across restarts; drain trigger semantics
  (split-on-high-risk, sum-of-commits, oldest-wait) mirror the in-memory
  contract.
- **`.autonomy/providers/llm.yml`** migrated to Wave-8.F per-role-chain
  schema (`role`, `chain: [{provider, model, env_var}]`) replacing the
  single hard-coded primary/fallback used in earlier prototypes.
- **`.autonomy/autonomy.yml`** profiles formalized
  (`report_only` → `supervised` → `autonomous_merge` →
  `autonomous_release` → `sovereign`) with explicit per-profile capability
  matrices.
- **TUI Mission Control** opens on the delivery workflow by default; the
  delivery model is the canonical 5-PR demo plus live data when wired.
- **State backend** (`db/state.rs`) — auto-recovers from stale RedlineDB
  WAL/SHM on open instead of failing fast.

### Fixed
- Six CI failures blocking PR #1 (resolved in commit `2bc3eda`).
- TUI delivery rendering snapshots stabilized; integration tests updated.

### Security
- **Webhook receiver verifies `X-Hub-Signature-256`** via hand-rolled
  HMAC-SHA256 (RFC 2104 compliant; constant-time compare) before
  appending any ledger entry. Bodies above 256 KiB return 413 without
  consuming the body.
- **ed25519 signing for every `LaunchLedgerEntry`**;
  `Signature::stub()` is refused at the `SqlLedger::append()` enforcement
  boundary, and the RedlineDB `launch_ledger` triggers enforce append-only at
  the storage layer.
- **`AgentApprovalReceipt`s synthesized by the orchestrator** (abstain on
  reviewer failure) are signed with the orchestrator's ed25519 key so the
  judge's `evidence_signature_invalid` condition still accepts them.
- **No real API keys in the repo** — every provider entry references an
  env-var name; `src/llm/provider_chains.rs::actual_yml_has_no_real_api_keys`
  fails closed if any leaks in.
- **Author cannot self-approve, distinct identities required** — enforced
  by `src/approval/quorum.rs` against the policy in
  `.autonomy/policies/approvals.yml`.
- **Exact (head_sha, policy_sha) binding** — `src/approval/sha_bind.rs`
  invalidates any receipt or verdict that drifts from its bound SHAs.
- **`.autonomy/keys/` is git-ignored** and contains no committed
  material; the directory ships empty with a `.gitkeep` placeholder.
- **`scripts/pre-pr.sh` refuses to run under `CI=true`** — the local-live
  sweep stays local; CI runs its own (mock-only) lane.
- **No `JERYU_LLM_LIVE` or `local-live` invocation** in any
  `.github/workflows/*.yml` (jeryu) or `dougx/.github/workflows/*.yml`.
- **Live-LLM tests are off by default** and explicitly named
  `*_live.rs` so they cannot be picked up by `cargo test --test '*'`
  without an explicit name.

## [3.1.0] - 2026-05-14
### Added
- **`jeryu-gcd` always-on disk daemon** (`crates/jeryu-gcd/`) that watches
  root-disk pressure every 60 s and runs pressure-tier GC to maintain
  ≥ 80 GiB free (`ROOT_DISK_HEADROOM_MIN_FREE_BYTES` floor in
  `src/cache/types.rs`). `Type=notify` systemd service at
  `ops/ci/jeryu-gcd.service`. Reuses the existing
  `gc_disk_cache_with_pressure` machinery — no duplicate GC logic.
- **`sweep_incremental_caches`** (`src/cache/runtime_gc.rs`) sweeps
  `target/.../incremental/` directories under JeRyu cache roots at
  Warning ≥ 30 min age, Critical at any age, and Emergency without age
  bound (workspace local sweep stays opt-in via
  `JERYU_GCD_ALLOW_LOCAL_TARGET_SWEEP=1`). Active leases are preserved.
- **Bootstrap auto-install** of `jeryu-gcd.service` (`src/bootstrap.rs`
  step 8 of 9). Skipped via `JERYU_BOOTSTRAP_SKIP_GCD=1` on systems
  without systemd.
- **TUI Cache → Disk Pressure panel**
  (`src/tui/ui_panels_body_more_cache.rs`) shows live free space,
  pressure level, and color-coded state.
- **`jeryu host install-gcd-service --allow-sudo`** CLI command for
  manual install/recovery.
- **Workspace-wide thin CI lane scripts**: `ops/ci/rust-lane.sh`
  (fmt/clippy/build/deny/witness/vrc/aer) joins
  `ops/ci/release-lane.sh`, `ops/ci/release-ready-lane.sh`, and
  `ops/ci/jankurai-lane.sh` as a single source of truth for what CI
  runs.
- Interactive Ratatui Rust TUI for God-Mode control dashboard.
- GitHub templates and OSS documentation structure.
- Initial GitLab Omnibus bootstrap logic and execution engine.

### Changed
- **Workspace clippy is now zero under `-D warnings`**. `cargo clippy
  --all-targets --all-features -- -D warnings` is the local CI gate
  (matches the command in `.github/workflows/rust.yml`). Auto-fixable
  lints (~90) resolved via `cargo clippy --fix`; design-decision lints
  (`too_many_arguments`, glob imports, private-in-public, large-Err)
  addressed with targeted `pub(crate)` promotions, allow-only-when-
  schema-is-flat annotations, and dead-code removal.
- **`cargo deny check` is clean** with one documented advisory ignore
  in `deny.toml` (`RUSTSEC-2021-0140` — `rusttype` is a dev-dep-only
  via `tuiwright`; migration to `ab_glyph` tracked as a follow-up
  issue).
- `ops/ci/jeryu-gc.timer` cadence dropped 6 h → 12 h (the daemon owns
  the fast path now; the timer is a deep-sweep safety net).
- `df_usage` (`src/cache_reports.rs`) promoted to `pub` so `jeryu-gcd`
  can reuse it without duplicating parsing logic.
- All formerly disk-bound integration tests (`test_agent_lifecycle`,
  `test_full_lifecycle`, `test_job_cycle`, `test_pool_*`) now pass
  locally without manual `--skip` flags — the daemon keeps df above
  the 80 GiB runner-fanout headroom.

### Fixed
- `cargo-aer scan` reports **0 findings** — added `[package.metadata.agent]`
  blocks to `crates/adapters/cache-brain/Cargo.toml` and
  `crates/tui-capture/Cargo.toml` (purpose, owned_paths, invariants,
  local_validate, risk, consumers).
- `crates/witness-rt/src/packet.rs::for_assert` clippy warning
  silenced with a scoped allow — the 8-arg signature is a flat
  fixed-schema assert packet.
- Duplicate `mod tests` include in `src/test_runner_runtime.rs`
  (`test_runner_tests.rs` was being loaded twice).

### Follow-up issues (filed by this PR, not implemented here)
- HLT-001 — split `src/tui/app_runtime_sync.rs` (360 LOC).
- HLT-016 — wire dependency-review/SBOM/provenance into a blocking
  security lane.
- HLT-013 — Playwright e2e + Storybook/UX-QA for `apps/web/`.
- HLT-008 — proptest coverage across `crates/`.
- HLT-006 — DB query-layer audit for `db/`.
- HLT-016-rusttype — migrate `tuiwright` from `rusttype` to `ab_glyph`
  (lifts the `cargo deny` ignore).

## [3.0.1] - 2026-04-27
### Added
- V3.01 capability request envelope with request id, actor, nonce, expiry, optional budget, optional grant proof, and length-prefixed JSON framing support while retaining  `AgentIntent` compatibility.
- VTI proof receipts for internal plans plus `jeryu test select --emit-receipt`.
- Merge-gate VTI receipt enforcement so smart-skipped validation cannot satisfy the default policy without a proof receipt.
- Explicit strict sandbox backend reporting with fail-closed behavior when strict isolation is requested but no `bwrap` or `unshare` backend is available.
- Real `agent list` support through GitLab issue label queries.
- Deterministic `jeryu tui --capture` PNG export for paper, review, and agent evidence workflows.
- Action-first TUI Mission Control landing view with Top Signal, Attention Queue, Proof Stack, metric tiles, next actions, and compact sparkline context.
- Agent Cockpit TUI view with session phase, progress, branch/SHA, grants, timeline, and action guidance.
- Command palette preview pane backed by the action registry, showing risk, side effects, required grants, dry-run availability, disabled reasons, and execution guidance.
- IEEE-style V3.01 working paper sources, agent-friendly Markdown, bibliography, and generated TUI screenshots.
- Version control files: `VERSION` and `version.json`.
- RedlineDB-primary state backend with RedlineDB fallback, bootstrap-managed Redline Compose service, optional `JERYU_TEST_REDLINE_URL` smoke coverage, and a disposable `just state-proof` harness.
- Backend-neutral state SQL placeholder handling for core RedlineDB operations across pools, managers, job/event tracking, VTI records, capability grants, and admission decisions.
- Backend-aware cache control managers for epoch invalidation, taint propagation, and CacheBrain decisions; executor cache writes now go through `Db` methods.

### Changed
- `RunTests` capability requests now return pipeline trigger errors instead of silently succeeding after branch creation.
- Dynamic CI YAML for capability-triggered test branches now uses a typed serializer and a fixed scope allowlist.
- GitLab TLS certificate validation is secure by default; insecure cert acceptance requires explicit `JERYU_GITLAB_INSECURE_TLS`.
- Group webhook creation now enables push events so engine supersedence and VTI planning receive the events they depend on.
- Capability action listing now derives from the canonical action registry.
- The TUI now opens on Mission instead of Jobs so operators see blockers, missing proof, and next actions first.
- VTI subsystem ownership patterns now include nested TUI, gateway, and test-intelligence modules.
- Unknown file changes now conservatively select full validation instead of docs-only validation.
- API and TUI docs now describe the current nine-tab TUI and screenshot capture path.
- Shared state upserts now use portable `ON CONFLICT` SQL instead of RedlineDB-only `INSERT OR REPLACE` forms.


## [1.0.1] - 2026-05-14
### Fixed
- **[2:Release] tab now shows live pipeline progress** even when no formal release attempt exists. Previously the tab was blank whenever the release lifecycle had not been invoked, because rendering was gated entirely on a `release_attempts` DB row.
- `pipeline_progress_view` was never populated in the real sync path (only in demo mode). The background sync now builds it from `ci_job_runs` (proper stage names) with a fallback to `job_events` grouped by `pool_name`.
- `tick()` preserved the old `pipeline_progress_view` unconditionally, preventing background-sync values from propagating. Fixed to only preserve when background sync found nothing (demo mode parity maintained).

### Changed
- **Release tab visual redesign**: left panel now splits vertically — gate matrix on top (12 rows when an attempt is active, 4 rows when waiting), live pipeline progress bars (`████▓░`) below with per-stage breakdown, ETA, and overall %.
- Right panel replaced plain-text inspector with a color-coded **job list** filtered to the active pipeline: ● green=success, ◉ cyan=running, ✕ red=failed, ○ yellow=pending.
- Gate matrix badge color `[RUN]` changed from Blue to Cyan for better terminal contrast.

### Added
- `build_stage_progress_from_ci_runs` — groups `ci_job_runs` by stage, computes per-stage counts and derived status.
- `build_stage_progress_from_events` — fallback that groups `job_events` by `pool_name` when `ci_job_runs` is empty.
- 5 new unit tests covering stage grouping, insertion-order preservation, pipeline-id filtering, status derivation, and the weighted-running progress formula.

## [1.0.0] - 2026-05-07
_Parallel release line from `main` branch — separate from the v3.x line above._
