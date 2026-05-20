# Testing Surface

This is the concise testing reference for `jeryu`.
The source of truth for lane selection is `proof-lanes.toml`, especially the
`[module_hints]` section and the change-type to lane mapping.

## Lane model

- `leaf-bugfix` -> `check`, `unit`
- `state-change` -> `check`, `unit`, `integration`
- `api-change` -> `check`, `unit`, `integration`
- `release-change` -> `check`, `unit`, `integration`
- `security-relevant` -> `check`, `unit`, `integration`, `security`
- `cross-module` -> `check`, `unit`, `integration`, `full`

Docs-only changes are treated as `docs_only` by VTI when every changed path
matches `*.md`, `docs/*`, `LICENSE`, `.gitignore`, or `.editorconfig`.

## Required proof commands

- `cargo check -p jeryu --message-format=json`
- `cargo nextest run -p jeryu --lib`
- `cargo test -p jeryu --test '*' -- --test-threads=1`
- `cargo nextest run -p jeryu`

Security-relevant module changes also require:

- `cargo test -p jeryu -- secrets exec honeypot admission`

## Test-intel surface

VTI owns diff-to-test selection and audit visibility:

- `jeryu test select`
- `jeryu test select-external --workspace /home/ubuntu/dougx`
- `jeryu test audit`
- `jeryu test learn`
- `jeryu test cache-status`

The planner emits a test plan and, when requested, a GitLab child pipeline
config. Selector misses are recorded so the system can explain why a test was
selected or skipped.

## Repair receipts

When a lane fails, capture one short receipt before rerunning:

- `error`: the exact failing line or span
- `likely_cause`: one sentence, no speculation beyond the evidence
- `rerun_command`: the narrowest proof command
- `owner`: the module or boundary that owns the fix
- `docs_url`: the local doc anchor for the next agent
- `expected_artifacts`: the file(s) that should change if the fix is correct

Use this shape in notes, PR comments, or handoff docs:

```yaml
repair_receipt:
  error: "compiler span or audit rule id"
  likely_cause: "one-line evidence-backed cause"
  rerun_command: "cargo check --workspace --message-format=json"
  owner: "src/state.rs"
  docs_url: "agent/JANKURAI_STANDARD.md#ownership-boundaries"
  expected_artifacts:
    - "agent/repo-score.json"
    - "agent/repo-score.md"
```

Keep repairs on the typed boundary that owns the data.
Call `state::Db` methods from product code instead of bypassing them with raw SQL
or caller-owned connections.

## Typed repair hint

When a scan or proof needs a structured next step, emit a `RepairHint`-style
payload with:

- `purpose`
- `reason`
- `common_fixes`
- `docs_url`
- `repair_hint`

`cargo-aer` and related scans should prefer this shape over raw prose when they
need to route the next rerun to a specific local command.

## Budget proof

- Record the proof lane you ran and the smallest command that reproduces the issue.
- If a fix touches security, release, or dependency surfaces, include the proof artifact path and the rerun command in the receipt.
- Do not rerun a wide lane until the receipt explains why the narrower lane was insufficient.

## What to use when

- Use `cargo check` for a fast syntax/type pass.
- Use `cargo nextest run -p jeryu --lib` for unit coverage.
- Use `cargo test -p jeryu --test '*' -- --test-threads=1` for
  state-dependent integration tests.
- Use `cargo nextest run -p jeryu` when the change spans modules or the proof
  lane calls for a full run.
- Use VTI selection first when the diff is narrow and the change type is known.

## Budgets and stop conditions

- Local proof budget: 15 minutes for `check` + `unit`, 30 minutes for `integration`, 60 minutes for a full audit pass.
- Stop if a lane exceeds its budget without producing a narrower proof route.
- Stop if `cargo check -p jeryu --message-format=json` fails.
- Stop if the unit lane exceeds the agent’s local proof budget for the change.
- Stop if the integration lane needs a wider proof set than the declared change
  type implies.
- Stop if the security lane reports unresolved secret, dependency, or SBOM
  failures.
- Stop if the proof path would require paid or external work with no budget or
  kill switch recorded in the change notes.
- Do not treat docs-only routing as release-ready unless the generated proof
  artifacts remain visible and current.

## Escalation rules

- Any global invalidator or unmapped path requires full testing.
- Changes under `src/test_intel/*` force full testing.
- A low-confidence selector result should not be treated as release-ready
  without broader validation.

## Local CI Parity

The local parity entrypoint is `scripts/ci-local.sh`. It dispatches to the same
`ops/ci/*.sh` scripts that GitHub Actions calls:

- `scripts/ci-local.sh rust fmt`
- `scripts/ci-local.sh rust clippy`
- `scripts/ci-local.sh rust build`
- `scripts/ci-local.sh rust test-lib`
- `scripts/ci-local.sh rust test-integration`
- `scripts/ci-local.sh security`
- `scripts/ci-local.sh release-preflight <version>`

Do not add inline-only workflow logic. If CI needs a new behavior, put it in
`ops/ci/` first and call that script from both CI and local proof.

## Cost budgets and stop conditions

This repo has no paid AI calls in product code and no per-request billing
surfaces. CI cost is bounded by GitHub Actions runner minutes plus the
explicit per-job ceilings below. If a future change introduces a paid
external call, it must land with a documented budget, a kill switch, and a
matching entry in this section before merge.

### Wall-clock budgets per CI lane

Hard ceilings come from `timeout-minutes` in `.github/workflows/rust.yml`.
GitHub kills the job when exceeded:

- `fmt`: 10 minutes
- `clippy`: 25 minutes
- `build`: 30 minutes
- `install-smoke` (ubuntu + macos matrix): 30 minutes
- `test-select` (VTI): 20 minutes
- `test-lib`: 30 minutes
- `test-integration`: 45 minutes
- `tui-smoke`: 15 minutes
- `supply-chain`: 20 minutes
- `witness`: 20 minutes
- `vrc-map`, `vrc-plan`, `aer-scan`: 15 minutes each
- `scheduled-hardening` (weekly cron): 30 minutes

Concurrency on a ref is bounded by `cancel-in-progress: true` so superseded
runs free their minutes immediately.

### Test iteration budgets

- `nextest` per-test slow timeouts (from `.config/nextest.toml`):
  - `default` profile: `slow-timeout = 120s`, `terminate-after = 3` (a hung
    test is killed after 6 minutes total).
  - `ci` profile: `slow-timeout = 120s`, `terminate-after = 2`,
    `fail-fast = true`, `retries = 1`.
- Integration tests run with `--test-threads=1` (see `medium` and the
  required-proof commands above) to bound concurrent subprocess fan-out.
- No `proptest!` or `fuzz_target!` exists in the workspace today. If one is
  added, declare iteration caps in this section (recommended defaults:
  `PROPTEST_CASES=256` for CI, fuzz runs time-boxed at 5 minutes per
  target unless explicitly extended).

### Network and subprocess kill switches

Real env knobs already wired into product code:

- `JERYU_POOL_SHUTDOWN_TIMEOUT_SECS` — bounds runner-manager SIGQUIT wait;
  defaults to `30` under test/CI (`src/config.rs::runner_shutdown_timeout_secs`).
- `JERYU_POOL_CARGO_LEASE_RECOVERY_SECS` — caps stale cargo-lease recovery
  wait; default `POOL_TARGET_LEASE_RECOVERY_TTL_SECS = 2 * 60 * 60`
  (2h, `src/cache.rs`). Override downward in CI if a lane idles on a lease.
- `JERYU_TUI_CAPTURE_MAX_WAIT_MS` — TUI capture upper bound, default `8000`
  ms (`src/repo.rs`); paired with `JERYU_TUI_CAPTURE_MIN_WAIT_MS` default
  `1200` ms. Prevents `tui-smoke` and screenshot jobs from hanging.
- `JERYU_SYSTEM_GIT`, `JERYU_GIT_MODE`, `JERYU_MIRROR_ENABLED`,
  `JERYU_MIRROR_REMOTE` — pin git subprocess target and disable mirroring
  in tests (`tests/git_mirror.rs`, `tests/git_passthrough.rs`).
- `JERYU_CARGO_CACHE`, `JERYU_CARGO_CACHE_ROOT`,
  `JERYU_CARGO_TARGET_ISOLATE`, `JERYU_CARGO_INCREMENTAL` — gate the shared
  cargo cache surface (`src/exec.rs`, `src/local.rs`).

### Stop conditions (CI bails)

- Clippy runs with `-D warnings`; any new warning fails the lane.
- `cargo nextest run --profile ci` is `fail-fast = true` with a single
  retry; the second failure terminates the run.
- `test-lib` and `test-integration` are skipped entirely when the VTI plan
  reports `mode == docs_only`, so docs-only PRs spend no test minutes.
- `supply-chain` runs `cargo deny check` plus `tools/security-lane.sh`;
  any deny advisory or unresolved evidence fails the lane.
- The local proof budgets in the previous section (15m check+unit, 30m
  integration, 60m full audit) remain the agent-side stop conditions.
