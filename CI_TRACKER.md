# jeryu — Local CI Tracker (confidence ledger)

Shared living dashboard of **local** test + gate health (maintained by both agents).
Working policy: local validation first, then branch push + PR review through
`https://github.com/neverhuman/jeryu/`. Direct `main` pushes are no longer the
default closeout path; `ci-fast-push.sh` requires explicit `--push-main` or
`JERYU_CI_PUSH_MAIN=1` for that escape hatch.

Run locally: `bash ci-fast-push.sh --full --no-push` (core parity gate) ·
`bash scripts/ci-phases.sh` (per-phase gates) · `./ops/ci/full.sh` (foundation)
· `cargo nextest run --workspace` (raw tests). A gate is **PASS / FAIL / PENDING**
(capability not built yet — never silently green).

Identity law: jeryu reads as a self-hosted GitHub-compatible forge. CI is GitHub-Actions +
native only; zero retired-provider evidence (enforced by the zero-evidence gate).

_Last updated: 2026-05-31 · Latest full parity gates are green with 40 workers in both profiles: local-native `JERYU_CI_ALLOW_RETIRED_PROCESSES=1 JERYU_CI_ALLOW_RETIRED_LISTENERS=1 bash ci-fast-push.sh --full --no-push` passed in **91s**, and GitHub-clean `JERYU_CI_ALLOW_RETIRED_PROCESSES=1 JERYU_CI_ALLOW_RETIRED_LISTENERS=1 JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash ci-fast-push.sh --full --no-push` passed in **87s**. The explicit retired-state bypass is only for this host because root-owned retired-provider services and the Docker-backed `:2224` listener remain active; authentic `ops/ci/verify-jeryu-env.sh --build-local --release-guard` fails closed until an operator stops them. The full gate verifies the repo-local `jeryu` binary, pins Jankurai 1.6.10, proves the GitHub vanilla profile, installs the pinned open security toolchain, runs workspace clippy and **1175 nextest tests**, phase gates, and every manifest lane in `agent/ci-lanes.toml` (`ci-fast`, `jankurai`, `security`, `proof-evidence`). Latest evidence: phase gates **PASS=7 · PENDING=0 · FAIL=0**, manifest proof-evidence Jankurai full scan **score 92, caps 0**, final Jankurai diff audit **score 83, hard 0, caps 0**, and final changed-file audit **score 83, caps 0**. First-wave local import registered **28 repos/mirrors** under `~/.local/share/jeryu`, `/api/v1/repos` lists them, and the git oracle proves imported repos materialize under `git/repos/OWNER/REPO.git` for clone/fetch. Remote is canonical GitHub only (`git@github.com:neverhuman/jeryu.git`; no local `:2224` forge remote)._

## v4.0.0 closeout — DONE vs DEFERRED (honest ledger)

**DONE (landed on main, green locally):**

- GitHub REST edge mounted on `jeryu-api` (`github_forward` + `github/mod.rs` `handle()` dispatch + `github/pulls.rs`), in-process `routes::Response.headers` with `json_response_with_headers`, returning `route_to_existing` and `X-Jeryu-Reused-PR` on PR reuse.
- `X-Jeryu` steering middleware and `GET /.jeryu/capabilities`.
- WebSocket event spine; live assembler projecting pool/health state.
- Operator surfaces: TUI Pools/Health views and the Web `/fleet` page from the live read-model.
- Autonomy FULL-AUTO loader (`FullAutoProfile`: R0-R4 auto, R5 fail-closed) plus pre-merge CI-check gating (`EvidencePack.ci_status`, `required_ci_lanes`, `missing_/failed_required_ci_check` hard-stops in `conditions/ci.rs`) in the judge veto walk.
- PR-overlap engine (`crates/jeryu-core/src/overlap.rs`) with route-to-existing reuse wiring.
- Real MCP backend.
- GitLab/MR vocabulary purge (GitHub-native PR vocabulary only).
- Codex CI-parity: `--full` and `github-vanilla` lanes, the drift guard, and the `ops/ci` manifest.
- 18-repo fleet prep: ownership/test maps extended, workspace versioned at 4.0.0, root Apache-2.0 LICENSE.

**DEFERRED (never green — do not claim as done):**

- **Phase A** — native sandbox, multi-node scheduling, and the `runnerd` daemon path (in-process runner contracts pass; the daemonized native/multi-node path is not green).
- **Phase B** — crate-cache-in-runner (cache-law gates pass; in-runner crate cache reuse is not wired/green).
- **Spine engine half** — gitd auth, create-repo-to-disk, and the push→CI bridge are not landed; the live forge is read/import-capable but cannot author-and-build a repo end to end from a push.
- **Fleet CUTOVER** — gated on the Spine engine half above **and** teardown of the `:2224` Docker-stack listener (root-owned, retired-state). This is an **owner action**: authentic `ops/ci/verify-jeryu-env.sh --build-local --release-guard` fails closed until those services are stopped.

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
| Codex full CI manifest parity | `agent/ci-lanes.toml`, workflow drift guard, explicit `ci-fast-push.sh --full`, GitHub fallback-profile proof, pinned security toolchain, manifest lane union, PR-default publishing, retired-state release guard, and added guard coverage green locally | **1175** |

## Test coverage by crate (passing)

**Product shell (Claude):** jeryu-autonomy 145 (Evidence-Gate: conditions/judge/quorum/sha-bind/ledger/kill-bell/escalation/auto-rejudge), jeryu-review 105 (multi-reviewer orchestrator + LLM seam + quorum/sha-bind), jeryu-tui 87 (runtime/widgets/theme/focus + mission/queue/repos lenses), jeryu-readmodel 36 (contract types + serde), jeryu-bugtracker 13, jeryu-mcp 12 (stdio+HTTP transport + 16-tool catalog).

**Core engine coverage by Claude:** jeryu-core 129 (domain CRUD, PR state machine, branch protection, checks, webhooks), jeryu-ci-ir 63 (deterministic IR hash, DAG validity), jeryu-proof 44 (owner/test-map, generated-zones, no-proof-no-merge).

**Core engine (Codex):** jeryu-gitd, jeryu-rustjet, jeryu-runnerd, jeryu-ci-scheduler, jeryu-cache*, jeryu-signrail, jeryu-runner-*, jeryu-mirror, jeryu-enterprise, jeryu-obs, jeryu-bench, jeryu-tenant, jeryu-phase11-*, jeryu-kernel — the remaining Rust coverage behind the 1175-test full fast-lane total. It includes fail-closed gates for scheduler cycles, runner trust, protected-ref force, duplicate provenance, pre-receive validation, cache-law allowlists, dangerous workspace denial, git differential oracle, imported-repo clone/fetch, runner sandbox contract guards, release-lane drift guards, and zero-evidence artifact filtering.

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
| 2026-05-31 | Codex | added `agent/ci-lanes.toml`, `jeryu-repogate ci-lanes-check/list`, explicit `ci-fast-push.sh --full`, GitHub fallback-profile proof, and pinned local/hosted security tool bootstrap |
| 2026-05-31 | Codex | switched `ci-fast-push.sh` publishing to branch/PR by default and added the full-release retired process/listener guard |
| 2026-05-31 | Codex | closed the local/GitHub-profile full parity loop at 1175 nextest tests, added guard self-tests, and made generated Jankurai audit reports non-source for zero-evidence |

## Toward 100% healthy / done

- [x] Workspace compiles (edition 2024); latest full workspace fast lane was green at 1175 nextest tests.
- [x] foundation gate green (fmt/clippy/zero-evidence/docs/security-scan).
- [x] github-conformance · ir-determinism · proof-gate · git-oracle · cache-safety · runner-sandbox PASS.
- [x] GitHub REST edge mounted on `jeryu-api` with `X-Jeryu` steering, `/.jeryu/capabilities`, WS event spine, live assembler, and TUI Pools/Health + Web `/fleet` surfaces.
- [x] Autonomy FULL-AUTO loader (R0-R4 auto / R5 fail-closed) with pre-merge CI-check gating in the judge veto walk.
- [x] PR-overlap engine with route-to-existing reuse and `X-Jeryu-Reused-PR` header.
- [x] Real MCP backend; GitLab/MR vocabulary purged.
- [x] Workspace versioned at 4.0.0; root Apache-2.0 LICENSE added; ownership/test maps extended for the new modules.
- [ ] **DEFERRED** Phase A: native sandbox, multi-node scheduling, and the `runnerd` daemon path (in-process runner contracts pass; daemonized path not green).
- [ ] **DEFERRED** Phase B: crate-cache-in-runner reuse.
- [ ] **DEFERRED** Spine engine half: gitd auth, create-repo-to-disk, push→CI bridge.
- [ ] **DEFERRED** Fleet CUTOVER: gated on the Spine engine half + `:2224` Docker-stack teardown (owner action).
- [ ] Build daemon/network transport hardening beyond the local git/cache PASS gates.
- [x] GitHub-correctness defects FIXED + tested: PR `Closed`/`Merged` stickiness; **enforced** branch protection (CODEOWNERS, linear history, signed commits, force-push/delete, enforce_admins); CI-IR multi-node cycle detection (Kahn's).
- [ ] **Jankurai audit score target ≥85 — NOT met in strict mode** (pre-v4 follow-up). The strict `jankurai audit .` (standard) gate is **pre-existingly red at score 70** on `main`: raw is 89–91, but a known **false-positive dead-language cap** pins the headline regardless of raw (confirmed identical 70 on `9ae1ebd` baseline and on the v4 wrap-up `b26c873`). The v4 wrap-up's repeated pagination helpers also added duplication findings (`hard` 1→4) without moving the capped headline. `--mode advisory`/`ratchet` report the more lenient 92 (caps 0) and the diff audit 83 (hard 0). Fix belongs in jankurai (the cap), optionally consolidate the pagination application — and this strict-mode red is one reason **v4.0.0 is staged-but-untagged**.
- [ ] Consolidate duplicated decision core (conditions/quorum/sha-bind/judge) into `jeryu-proof`.
- [x] Durable SQLite forge truth for core forge resources with reopen and rollback tests.
- [x] Local-only live API/TUI path on `127.0.0.1` with `~/.local/share/jeryu` as the Rust data dir.
- [x] Guided GitHub-compatible `/user`, REST repair-hint 404s, and `/graphql` read probes with repair-hint 501s.
- [x] TUI has a `jeryu-tui --once` binary, API-source mode, and all 18 tabs rendering from the read model.
- [x] Local and hosted fast CI both route through `ci-fast-push.sh` with 40 workers, native Rust default execution, pinned Jankurai, repo-local binary verification, explicit `--full` manifest-lane parity, and GitHub fallback-profile proof.
- [x] Direct-main publishing is no longer the default; closeout changes go through a branch + PR path.
- [ ] Retired-provider and old Jeryu listener/process guard is implemented; local host still needs shutdown/quarantine proof before release validation can pass.
- [ ] Deepen the thin engine crates and complete authenticated LAN/public deployment hardening.
