# jeryu — Local CI Tracker (confidence ledger)

Shared living dashboard of **local** test + gate health (maintained by both agents).
Working policy: local validation, frequent merges to `main`, frequent pushes to
`https://github.com/neverhuman/jeryu/`. **PR-based CI is intentionally not required** —
we push straight to `main` until this is 100% healthy and done.

Run locally: `bash scripts/ci-phases.sh` (per-phase gates) · `./ops/ci/full.sh` (foundation)
· `cargo nextest run --workspace` (raw tests). A gate is **PASS / FAIL / PENDING**
(capability not built yet — never silently green).

Identity law: jeryu reads as a self-hosted GitHub-compatible forge. CI is GitHub-Actions +
native only; zero retired-provider evidence (enforced by the zero-evidence gate).

_Last updated: 2026-05-31 · Latest full `bash ci-fast-push.sh --no-push` after the gitd import validation tranche reports **all gates green in 24s** with 40 workers: CI profile, repo-local `jeryu` binary verification, pinned Jankurai bootstrap, affected-plan, affected changed-list, untracked parity guard, fmt, workspace clippy, **1122 nextest tests**, zero-evidence, docs markers, phase gates, Jankurai diff audit, and Jankurai audit. Jankurai reported diff audit **score 90, hard 0, caps 0** and audit **score 90, caps 0**. First-wave local import registered **28 repos/mirrors** under `~/.local/share/jeryu`, `/api/v1/repos` lists them, and the git oracle now proves imported repos materialize under `git/repos/OWNER/REPO.git` for clone/fetch. · `scripts/ci-phases.sh` reports **PASS=7 · PENDING=0 · FAIL=0**. · Remote is canonical GitHub only (`git@github.com:neverhuman/jeryu.git`; no local `:2224` forge remote)._

## Per-phase gate status (`scripts/ci-phases.sh`)

| Gate | Spec phase | Status | Notes |
|---|---|---|---|
| foundation | all | **PASS** | fmt + check + clippy(-D warnings) + `test --workspace` + zero-evidence + docs + release-gate + score + ci-doctor |
| github-conformance | 2 (forge/API) | **PASS** | GitHub REST shape (`jeryu-api`) + GitHub vocabulary asserts (no retired domain terms) |
| ir-determinism | 3 (CI IR) | **PASS** | identical pipeline ⇒ identical IR hash; DAG validity; trust tiers; policy preservation |
| proof-gate | 7 (proof/merge) | **PASS** | owner/test-map matching; proof plan; generated-zones; no-proof-no-merge |
| git-oracle | 1 (Git service) | **PASS** | in-repo `jeryu-gitd` suite PASS; local differential-vs-stock bare Git oracle PASS |
| runner-sandbox | 4 (runners) | **PASS** | in-repo runner suites PASS; live Docker-backed namespace/seccomp/no-new-privs/cgroup/read-only-root escape matrix PASS |
| cache-safety | 6 (cache/CAS) | **PASS** | in-repo cache suites PASS; local poisoning/false-hit harness PASS |

**Totals: PASS=7 · PENDING=0 · FAIL=0** → local CI result OK.

## Foundation gate sub-checks (last full recorded run)

| Check | Status |
|---|---|
| `cargo metadata` (workspace shape, 52 pkgs, one root) | PASS |
| `cargo fmt --all -- --check` | PASS _(rechecked by Codex 2026-05-31)_ |
| `cargo check --workspace --all-targets` | PASS |
| `cargo test --workspace` | PASS |
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
| Codex agent docs/error slice | typed repair hints, domain exception crate, docs index cleanup | 961 |
| Codex Jankurai docs/metadata slice | README start page, boundary manifest, tool-adoption manifest, real Jankurai evidence lane | 961 |
| 0-Python + core/CI growth (`589c765`) | 10 scripts→Rust crates, lanes rewired, accumulated core/scheduler/runner coverage | 991 |
| Plan-forward + cap-sweep (`866056e`) | P10 `jeryu-api` REST + 11 GitHub conformance tests, P20 `jeryu-cli` snapshots, genuine ts-rs `jeryu-readmodel` contracts + byte-identity drift test; cap-sweep (tui/shell/engine/repo) merges | **1094** |
| Web SPA (P23, `web/`) | typecheck + **28 vitest** + build + lint green (JS/TS, outside the Rust nextest total) | 28 (web) |
| Codex local-live slice | SQLite-backed `ForgeCore`, `jeryu-api` web feature, `jeryu-tui` API-source capture, affected-plan fast lane, and local Git import smoke | 316 focused |
| Codex live-readiness fast lane | Full `ci-fast-push.sh --no-push`: affected manifest includes untracked files, workspace clippy, phase gates, and Jankurai diff/audit lanes green | **1108** |
| Codex guided GraphQL compatibility | `/graphql` read probes, guided unsupported-operation repair hints, focused API/domain tests, live HTTP smoke, and fast lane green | focused |
| Codex guided REST repair | `/user` identity plus repair-hinted unknown-route 404s; affected API/domain fast lane and Jankurai diff/audit score 90 | 23 focused |
| Codex local/GitHub CI parity | `ci-fast-push.sh --no-push` preflights local profile, repo-built `jeryu`, pinned Jankurai, native Rust runner defaults, full workspace tests, phase gates, and Jankurai diff/audit | **1113** |
| Codex gitd import validation | `jeryu-mirror import-local` now materializes gitd storage and `ops/git-oracle` proves clone/fetch from the imported mirror | **1122** |

## Test coverage by crate (passing)

**Product shell (Claude):** jeryu-autonomy 145 (Evidence-Gate: conditions/judge/quorum/sha-bind/ledger/kill-bell/escalation/auto-rejudge), jeryu-review 105 (multi-reviewer orchestrator + LLM seam + quorum/sha-bind), jeryu-tui 87 (runtime/widgets/theme/focus + mission/queue/repos lenses), jeryu-readmodel 36 (contract types + serde), jeryu-bugtracker 13, jeryu-mcp 12 (stdio+HTTP transport + 16-tool catalog).

**Core engine coverage by Claude:** jeryu-core 129 (domain CRUD, PR state machine, branch protection, checks, webhooks), jeryu-ci-ir 63 (deterministic IR hash, DAG validity), jeryu-proof 44 (owner/test-map, generated-zones, no-proof-no-merge).

**Core engine (Codex):** jeryu-gitd, jeryu-rustjet, jeryu-runnerd, jeryu-ci-scheduler, jeryu-cache*, jeryu-signrail, jeryu-runner-*, jeryu-mirror, jeryu-enterprise, jeryu-obs, jeryu-bench, jeryu-tenant, jeryu-phase11-*, jeryu-kernel — the remaining Rust coverage behind the 1122-test full fast-lane total. It includes fail-closed gates for scheduler cycles, runner trust, protected-ref force, duplicate provenance, pre-receive validation, cache-law allowlists, dangerous workspace denial, git differential oracle, imported-repo clone/fetch, and runner sandbox contract guards.

## Added local CI coverage (log)

| Date UTC | Owner | Area |
|---|---|---|
| 2026-05-30 | Codex | scheduler duplicate-job/cycle failure, runner release/agent trust edges, protected-ref force/bypass, duplicate release-provenance digest |
| 2026-05-30 | Claude | shell crates (mcp/readmodel/bugtracker/autonomy/review/tui) + core coverage (core/ci-ir/proof) + per-phase gate harness (`ops/ci/gates/*`, `scripts/ci-phases.sh`) + GitHub-REST conformance gate |
| 2026-05-30 | Codex | git pre-receive malformed ref/OID rejection; explicit-shared compiled cache requires allowlist; required cache fingerprint inputs; runner dangerous workspace denial |
| 2026-05-30 | Codex | lifted git-oracle to PASS with local differential-vs-stock bare Git oracle; lifted cache-safety to PASS with local poisoning/false-hit harness; runner-sandbox now includes runnerd and remains honestly PENDING; Jankurai maps/audits pass with fixed git-oracle smoke route |
| 2026-05-30 | Codex | strengthened runner-sandbox runnable coverage: sandbox plan contract, native seccomp/Landlock/cgroup validation, all denied env scrubbing, direct job validation, OCI dangerous workspace denial, and mapped static sandbox matrix |
| 2026-05-30 | Codex | enforced PR-only product language by removing retired request/provider vocabulary from mirror archives, docs, tests, comments, and conformance gate text; repo-wide scan clean |
| 2026-05-30 | Codex | cleared Jankurai docs cap, added boundary/tool-adoption manifests, expanded budget controls, and made the local Jankurai lane produce proof/security/rust-witness evidence before diff-audit |
| 2026-05-31 | Codex | cleared Jankurai findings to 0, added DB migration evidence, maxed tool adoption to 20/20/20/20, and lifted runner-sandbox to PASS with a live escape matrix |
| 2026-05-31 | Codex | added durable SQLite local-live forge store, Axum live API, API-backed TUI captures, local Git import, untracked-aware affected CI, and green `ci-fast-push.sh --no-push` evidence |
| 2026-05-31 | Codex | added guided `/graphql` support for read probes plus GitHub-shaped 501 repair hints for unsupported operations |
| 2026-05-31 | Codex | added `GET /user` and repair-hinted GitHub-shaped 404s for unsupported REST routes |
| 2026-05-31 | Codex | added local/GitHub `ci-fast-push.sh` parity bootstrap, repo-local `jeryu` binary verification, native Rust runner defaults, and pinned Jankurai setup |
| 2026-05-31 | Codex | added gitd materialization for local imports and clone/fetch proof in the git oracle smoke lane |
| 2026-05-31 | Codex | hardened `ci-fast-push.sh` Jankurai changed-list generation and added a fail-closed untracked-file parity guard before Jankurai diff audit |

## Toward 100% healthy / done

- [x] Workspace compiles (edition 2024); latest full workspace fast lane was green at 1122 nextest tests.
- [x] foundation gate green (fmt/clippy/zero-evidence/docs/security-scan).
- [x] github-conformance · ir-determinism · proof-gate · git-oracle · cache-safety · runner-sandbox PASS.
- [ ] Build daemon/network transport hardening beyond the local git/cache PASS gates.
- [x] GitHub-correctness defects FIXED + tested: PR `Closed`/`Merged` stickiness; **enforced** branch protection (CODEOWNERS, linear history, signed commits, force-push/delete, enforce_admins); CI-IR multi-node cycle detection (Kahn's).
- [x] **Jankurai audit score target ≥85**: latest full fast lane reports smart audit score 90 with caps 0; diff audit is score 90 with hard 0 and caps 0.
- [ ] Consolidate duplicated decision core (conditions/quorum/sha-bind/judge) into `jeryu-proof`.
- [x] Durable SQLite forge truth for core forge resources with reopen and rollback tests.
- [x] Local-only live API/TUI path on `127.0.0.1` with `~/.local/share/jeryu` as the Rust data dir.
- [x] Guided GitHub-compatible `/user`, REST repair-hint 404s, and `/graphql` read probes with repair-hint 501s.
- [x] TUI has a `jeryu-tui --once` binary, API-source mode, and all 18 tabs rendering from the read model.
- [x] Local and hosted fast CI both route through `ci-fast-push.sh --no-push` with 40 workers, native Rust default execution, pinned Jankurai, and repo-local binary verification.
- [ ] Deepen the thin engine crates and complete authenticated LAN/public deployment hardening.
