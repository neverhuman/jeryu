# CI_TRACKER: Local Confidence Ledger

This file tracks local CI and targeted gates that are added or strengthened while the fused Jeryu workspace is built. PR CI is intentionally not required yet; the working policy is local validation, frequent merges to `main`, and frequent pushes to `git@github.com:neverhuman/jeryu.git`.

## Current Local Gate Set

| Gate | Scope | Status | Last Known Evidence |
|---|---|---|---|
| `cargo metadata --format-version 1 --no-deps` | Workspace shape | Passing | Rename checkpoint, 40 packages, one workspace root |
| `cargo fmt --all --check` | Workspace formatting | Needs refresh | Red on Claude-owned shell/TUI/autonomy/review files as of 2026-05-30T21:25Z |
| `cargo check --workspace --all-targets` | Workspace compile | Passing | Codex slice 2026-05-30T21:25Z |
| `cargo test --workspace` | Workspace tests | Passing | 853 tests on Codex slice 2026-05-30T21:25Z |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Workspace lint | Passing at rename checkpoint | Scoped clippy passing for later Codex-owned crates |
| `scripts/zero-evidence-guard.py .` | Retired-provider/product evidence | Passing | Codex slice 2026-05-30T21:25Z |
| `scripts/check-docs.py` | Documentation policy | Passing | Rename checkpoint |
| `scripts/release-gate.py` | Release metadata gate | Passing | Rename checkpoint |
| `scripts/score-repo.py` | Repository score gate | Passing | Rename checkpoint |
| `scripts/ci-doctor.sh` | Local CI environment | Passing | Rename checkpoint |
| `scripts/ci-local.sh` | Local aggregate CI | Passing | Rename checkpoint |
| `ops/ci/fast.sh` | Fast local gate | Passing | Rename checkpoint |
| `ops/ci/security.sh` | Security/cache safety gate | Passing | Rename checkpoint |
| `ops/ci/full.sh` | Full local gate | Passing at rename checkpoint | Needs re-run after shell-crate formatting refresh |

## Passing Test Growth

| Checkpoint | Added Confidence | Passing Count |
|---|---|---:|
| Foundation cleanup | Initial fused workspace tests | 195 |
| P14 scheduler leases | Lease/retry/idempotency foundations | 199 |
| P15 runner bridge | Runner protocol adapter and policy bridge | 215 |
| Shell foundation merge | MCP/readmodel/bugtracker shell tests | 276 |
| Shell autonomy/review/TUI merge | Evidence-gate, review quorum, TUI bootstrap tests | 613 |
| Core coverage merge | Domain, CI IR, proof tests | 835 |
| Codex-owned core gates | Scheduler, runner, gitd, signrail tests | 842 |
| Codex fail-closed slice | Cache, scheduler, gitd, signrail behavior tests | 853 |

## Added Local CI Coverage

| Date UTC | Owner | Area | Passing Gate |
|---|---|---|---|
| 2026-05-30T21:18Z | Codex | Scheduler duplicate-job/cycle failure, runner release/agent trust policy edges, protected-ref force/bypass policy, duplicate release provenance digest | Targeted package tests, workspace check/test, scoped clippy, zero-evidence guard |
| 2026-05-30T21:25Z | Codex | Cache corrupt-CAS/release-lane/agent-write/promotion-denial tests, scheduler run-id and orphaned-lease guards, gitd service-level delete/non-fast-forward protected-ref tests, signrail rollback/signer-identity gates | 61 targeted tests, workspace check, 853 workspace tests, scoped clippy, scoped fmt, zero-evidence guard |

## Open Local CI Work

- Add Claude's phase-gate harness once his claimed `ops/ci/gates/*` and `scripts/ci-phases.sh` work lands.
- Refresh global formatting after Claude-owned shell-crate formatting is settled, then re-run `ops/ci/full.sh`.
- Expand git oracle tests toward clone/fetch/push/protected-ref/LFS coverage.
- Expand runner gates toward sandbox escape, secret isolation, fork denial, and crash/requeue coverage.
- Expand cache gates toward false-hit, poisoning, cross-project denial, release-hermetic safe-miss, and quarantine promotion boundaries.
- Expand release gates toward signer identity, rollback metadata, SBOM/provenance, and OIDC fail-closed coverage.
