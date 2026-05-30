# jeryu — Local CI Tracker

Living dashboard of **local** test + gate health. We push straight to `main` and
keep going until this is 100% healthy and done. PR-based CI is **not** required.

- Run everything locally with one command: `bash scripts/ci-phases.sh` (per-phase gates)
  or `./ops/ci/full.sh` (foundation gate). `cargo nextest run --workspace` for raw tests.
- A gate is **PASS**, **FAIL**, or **PENDING** (capability not built yet — never silently green).
- Identity law: jeryu reads as a self-hosted GitHub-compatible forge. CI is GitHub-Actions +
  native only. Zero legacy-provider evidence (enforced by the zero-evidence gate).

_Last updated: 2026-05-30 · `main` @ the merge below · `cargo nextest run --workspace` = **842 passed / 0 failed**._

## Per-phase gate status (`scripts/ci-phases.sh`)

| Gate | Spec phase | Status | Notes |
|---|---|---|---|
| foundation | all | **PASS** | fmt + check + clippy(-D warnings) + `test --workspace` + zero-evidence + docs + release-gate + score |
| github-conformance | 2 (forge/API) | **PASS** | GitHub REST shape (`jeryu-api` conformance) + GitHub vocabulary asserts (no `iid`/legacy terms) |
| ir-determinism | 3 (CI IR) | **PASS** | identical pipeline ⇒ identical IR hash; DAG validity; trust tiers; policy preservation |
| proof-gate | 7 (proof/merge) | **PASS** | owner/test-map matching; proof plan; generated-zones; no-proof-no-merge |
| git-oracle | 1 (Git service) | **PENDING** | in-repo `jeryu-gitd` suite PASS; live differential-vs-stock-git suite needs the running git daemon |
| runner-sandbox | 4 (runners) | **PENDING** | in-repo runner suites PASS; live seccomp/Landlock/cgroups escape matrix needs the native sandbox runtime |
| cache-safety | 6 (CrateVault) | **PENDING** | in-repo cache suites PASS; live poisoning/false-hit harness needs the running cache service |

**Totals: PASS=4 · PENDING=3 · FAIL=0** → local CI result OK.

## Test coverage by crate (passing)

### Product shell (Claude)
| Crate | Tests | Covers |
|---|---:|---|
| jeryu-autonomy | 145 | Evidence-Gate: condition registry, judge fusion, quorum, sha-bind, ledger, kill-bell, escalation, auto-rejudge, freeze |
| jeryu-review | 105 | multi-reviewer orchestrator, LLM seam, prompt-replay, quorum veto, sha-bind mismatch |
| jeryu-tui | 87 | Flight-Deck runtime/widgets/theme/focus + mission/queue/repos lens snapshots from the read-model |
| jeryu-readmodel | 36 | TUI/web contract types, serde round-trip, provider-neutral fields |
| jeryu-bugtracker | 13 | bug domain + triage behind a store trait |
| jeryu-mcp | 12 | stdio+HTTP MCP transport, 16-tool catalog, JSON-RPC conformance |

### Core engine — coverage added by Claude
| Crate | Tests | Covers |
|---|---:|---|
| jeryu-core | 129 | domain CRUD, PR state machine, branch protection, checks/statuses, webhooks, serde shapes |
| jeryu-ci-ir | 63 | deterministic IR hash, DAG validity, trust tiers, policy preservation |
| jeryu-proof | 44 | owner/test-map matching, proof plan, generated-zone enforcement, no-proof-no-merge |

### Core engine — Codex
`jeryu-gitd`, `jeryu-rustjet`, `jeryu-runnerd`, `jeryu-ci-scheduler`, `jeryu-cache*`,
`jeryu-signrail`, `jeryu-runner-*`, `jeryu-mirror`, `jeryu-enterprise`, `jeryu-obs`,
`jeryu-bench`, `jeryu-tenant`, `jeryu-phase11-*`, `jeryu-kernel`, … — the balance of the
**842** workspace total. (Several engine crates still thin; deepening tracked below.)

## Increment log (merged to `main`, pushed to remote)

| When | Commit | What | Workspace tests |
|---|---|---|---|
| 2026-05-30 | (rename) | renamed `jeryu-*` core, edition 2024 | 215 |
| 2026-05-30 | `43fb400` | shell batch 1: mcp + readmodel + bugtracker | 276 |
| 2026-05-30 | `5e416b8` | shell batch 2: autonomy + review + tui | 613 |
| 2026-05-30 | `764d556` | core test coverage: core + ci-ir + proof (+222) | 835 → 842 |
| 2026-05-30 | (this) | rustfmt + remove test-only unsafe + per-phase CI gate harness | 842 |

## Toward 100% healthy / done

- [x] Workspace compiles (edition 2024) + `cargo nextest run --workspace` green.
- [x] foundation gate green (fmt/clippy/zero-evidence/docs).
- [x] github-conformance · ir-determinism · proof-gate PASS.
- [ ] Lift **git-oracle** PENDING → PASS (wire the live Git daemon + differential-vs-stock suite).
- [ ] Lift **runner-sandbox** PENDING → PASS (native sandbox runtime + escape matrix).
- [ ] Lift **cache-safety** PENDING → PASS (live cache service + poisoning harness).
- [ ] Fix GitHub-correctness defects surfaced by tests: PR `Closed` stickiness; enforce persisted branch-protection fields (CODEOWNERS / linear history / signed commits); CI-IR multi-node cycle detection.
- [ ] Consolidate the duplicated decision core (conditions/quorum/sha-bind/judge) into `jeryu-proof`.
- [ ] Deepen coverage on the thin engine crates; remaining TUI lenses + live backend wiring.
