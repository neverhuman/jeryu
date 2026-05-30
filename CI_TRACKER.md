# jeryu — Local CI Tracker (confidence ledger)

Shared living dashboard of **local** test + gate health (maintained by both agents).
Working policy: local validation, frequent merges to `main`, frequent pushes to
`git@github.com:neverhuman/jeryu.git`. **PR-based CI is intentionally not required** —
we push straight to `main` until this is 100% healthy and done.

Run locally: `bash scripts/ci-phases.sh` (per-phase gates) · `./ops/ci/full.sh` (foundation)
· `cargo nextest run --workspace` (raw tests). A gate is **PASS / FAIL / PENDING**
(capability not built yet — never silently green).

Identity law: jeryu reads as a self-hosted GitHub-compatible forge. CI is GitHub-Actions +
native only; zero retired-provider evidence (enforced by the zero-evidence gate).

_Last updated: 2026-05-30 · `cargo test --workspace` = **957 passed / 0 failed** · targeted PR-only cleanup test set = **616 passed / 0 failed** · `bash scripts/ci-phases.sh` = **OK** (PASS=6, PENDING=1, FAIL=0)._

## Per-phase gate status (`scripts/ci-phases.sh`)

| Gate | Spec phase | Status | Notes |
|---|---|---|---|
| foundation | all | **PASS** | fmt + check + clippy(-D warnings) + `test --workspace` + zero-evidence + docs + release-gate + score + ci-doctor |
| github-conformance | 2 (forge/API) | **PASS** | GitHub REST shape (`jeryu-api`) + GitHub vocabulary asserts (no retired domain terms) |
| ir-determinism | 3 (CI IR) | **PASS** | identical pipeline ⇒ identical IR hash; DAG validity; trust tiers; policy preservation |
| proof-gate | 7 (proof/merge) | **PASS** | owner/test-map matching; proof plan; generated-zones; no-proof-no-merge |
| git-oracle | 1 (Git service) | **PASS** | in-repo `jeryu-gitd` suite PASS; local differential-vs-stock bare Git oracle PASS |
| runner-sandbox | 4 (runners) | **PENDING** | in-repo runner suites PASS with strengthened static guard matrix; live seccomp/Landlock/cgroups escape matrix needs the native sandbox runtime |
| cache-safety | 6 (cache/CAS) | **PASS** | in-repo cache suites PASS; local poisoning/false-hit harness PASS |

**Totals: PASS=6 · PENDING=1 · FAIL=0** → local CI result OK.

## Foundation gate sub-checks (`ops/ci/full.sh`)

| Check | Status |
|---|---|
| `cargo metadata` (workspace shape, 42 pkgs, one root) | PASS |
| `cargo fmt --all -- --check` | PASS _(rechecked by Codex 2026-05-30T21:36Z)_ |
| `cargo check --workspace --all-targets` | PASS |
| `cargo test --workspace` (957) | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `jeryu-evidence .` · `jeryu-mapcheck docs` · `jeryu-repogate release-gate` · `jeryu-repogate score` · `ci-doctor.sh` | PASS |

## Passing test growth

| Checkpoint | Added confidence | Passing |
|---|---|---:|
| Foundation cleanup | initial fused workspace tests | 195 |
| P14 scheduler leases | lease/retry/idempotency foundations | 199 |
| P15 runner bridge | runner protocol adapter + policy bridge | 215 |
| Shell batch 1 (`43fb400`) | MCP / readmodel / bugtracker | 276 |
| Shell batch 2 (`5e416b8`) | autonomy / review / TUI | 613 |
| Core coverage (`764d556`) | domain / CI-IR / proof (+222) | 835 → 842 |
| Codex fail-closed slice | cache / scheduler / gitd / signrail behavior | 853 |
| Foundation health (this) | rustfmt + remove test-only unsafe + per-phase CI gate harness | 853 |
| Codex input-boundary slice | pre-receive / cache-law / runner workspace guards | 860 |
| Codex local-live gates | git oracle differential + cache poisoning harness | 862 |
| Claude GitHub/TUI/vibe stack | GitHub correctness, CI compiler coverage, TUI lenses, jankurai vibe cleanup | 942 |
| Codex runner-sandbox contract | seccomp/Landlock/cgroup contract guards, env scrubbing, OCI/socket denial | 957 |
| Codex PR-only cleanup | mirror model/docs, agent-facing comments, conformance gate storage, repair hints | 616 targeted |

## Test coverage by crate (passing)

**Product shell (Claude):** jeryu-autonomy 145 (Evidence-Gate: conditions/judge/quorum/sha-bind/ledger/kill-bell/escalation/auto-rejudge), jeryu-review 105 (multi-reviewer orchestrator + LLM seam + quorum/sha-bind), jeryu-tui 87 (runtime/widgets/theme/focus + mission/queue/repos lenses), jeryu-readmodel 36 (contract types + serde), jeryu-bugtracker 13, jeryu-mcp 12 (stdio+HTTP transport + 16-tool catalog).

**Core engine coverage by Claude:** jeryu-core 129 (domain CRUD, PR state machine, branch protection, checks, webhooks), jeryu-ci-ir 63 (deterministic IR hash, DAG validity), jeryu-proof 44 (owner/test-map, generated-zones, no-proof-no-merge).

**Core engine (Codex):** jeryu-gitd, jeryu-rustjet, jeryu-runnerd, jeryu-ci-scheduler, jeryu-cache*, jeryu-signrail, jeryu-runner-*, jeryu-mirror, jeryu-enterprise, jeryu-obs, jeryu-bench, jeryu-tenant, jeryu-phase11-*, jeryu-kernel — the balance of the 957 total (incl. fail-closed gates: scheduler cycle, runner trust, protected-ref force, duplicate provenance, pre-receive validation, cache-law allowlists, dangerous workspace denial, git differential oracle, and runner sandbox contract guards).

## Added local CI coverage (log)

| Date UTC | Owner | Area |
|---|---|---|
| 2026-05-30 | Codex | scheduler duplicate-job/cycle failure, runner release/agent trust edges, protected-ref force/bypass, duplicate release-provenance digest |
| 2026-05-30 | Claude | shell crates (mcp/readmodel/bugtracker/autonomy/review/tui) + core coverage (core/ci-ir/proof) + per-phase gate harness (`ops/ci/gates/*`, `scripts/ci-phases.sh`) + GitHub-REST conformance gate |
| 2026-05-30 | Codex | git pre-receive malformed ref/OID rejection; explicit-shared compiled cache requires allowlist; required cache fingerprint inputs; runner dangerous workspace denial |
| 2026-05-30 | Codex | lifted git-oracle to PASS with local differential-vs-stock bare Git oracle; lifted cache-safety to PASS with local poisoning/false-hit harness; runner-sandbox now includes runnerd and remains honestly PENDING; Jankurai maps/audits pass with fixed git-oracle smoke route |
| 2026-05-30 | Codex | strengthened runner-sandbox runnable coverage: sandbox plan contract, native seccomp/Landlock/cgroup validation, all denied env scrubbing, direct job validation, OCI dangerous workspace denial, and mapped static sandbox matrix |
| 2026-05-30 | Codex | enforced PR-only product language by removing retired request/provider vocabulary from mirror archives, docs, tests, comments, and conformance gate text; repo-wide scan clean |

## Toward 100% healthy / done

- [x] Workspace compiles (edition 2024) + `cargo test --workspace` green (957).
- [x] foundation gate green (fmt/clippy/zero-evidence/docs/security-scan).
- [x] github-conformance · ir-determinism · proof-gate · git-oracle · cache-safety PASS.
- [ ] Lift **runner-sandbox** PENDING → PASS (native sandbox runtime + escape matrix).
- [ ] Build daemon/network transport hardening beyond the local git/cache PASS gates.
- [x] GitHub-correctness defects FIXED + tested: PR `Closed`/`Merged` stickiness; **enforced** branch protection (CODEOWNERS, linear history, signed commits, force-push/delete, enforce_admins); CI-IR multi-node cycle detection (Kahn's). Workspace 876 green.
- [ ] **Jankurai standard score = 40/100** (pervasive, mostly pre-existing): fallback-soup density + dead-language heuristic flags on GitHub-required terms (`stale`, etc.) + repo-level gaps (CI audit lane, fast lane). Shared remediation + a policy call on GitHub-term false-positives — see chat.
- [ ] Consolidate duplicated decision core (conditions/quorum/sha-bind/judge) into `jeryu-proof`.
- [ ] Deepen the thin engine crates; remaining TUI lenses + live backend wiring.
