# Jeryu master hardening plan — robust runners, agent-first CI, redesigned TUI

## Context

`jeryu` runs on a single SSH-gated master node (xbabe2) and orchestrates Docker
runners across `xbabe0/1/3` (xbabe2 reserved) for **16 registered projects**:
7 root (`jeryu`, `jekko`, `jankurai`, `jansu`, `jnoccio`, `redlineDB`,
`redline-testing`) + 9 GitHub-backed under `~/veox-split/*`. `openQG` is present
but **unregistered**. The goal is to make jeryu the world's friendliest
agent-first CI/git tool: runners that never silently die or sit idle, CI that
screams (hyper-parallel, short MR chains), a completely redesigned TUI you can
launch and immediately understand, and a loop agents heal without restarting.

Five live problems drive this plan (all ground-truthed this session):

1. **The runner paradox.** The DB reports `active=10/40` while **1,976 containers
   are actually running** with **1,936 over-capacity zombies** eating host disk —
   so the fleet is *underfilled on paper and bloated in reality*. The 300s
   reconcile loop (`engine_background_reconcile.rs:79-196`) never reconciles the
   drift, has no idle/hung detection, no utilization metric, and no fast alert.
   This is the root of "runners crash / get stuck / spin down."
2. **CI is over-serialized.** jeryu's `.gitlab-ci.yml` is a 7-stage barrier
   pipeline; `rust_test_lib` is gated on `rust_test_select` completion (~line 92),
   `release_ready` runs the full release pre-flight on *every MR* (~line 256), and
   stage barriers force `rust_build` to wait on `jankurai_sbom`. Estimated MR
   critical path ~45 min; a `needs:`-DAG cuts it to ~18 min (≈60%).
3. **The redesigned TUI never shipped.** `recovery/phase0-tui` (28 commits, 0
   conflicts vs main) carries the lens-model "Flight Deck" reset inspired by
   `tips/tui_reset/*`, but it was never merged and the Runners lens (U22) is a
   placeholder. No multinode live-runner pane exists.
4. **Per-project footguns.** redlineDB uses runner `tags: [default]` +
   `[docker-build]` (breaks untagged policy); 4/7 root repos paper over **sccache
   instability** at the job level (a *platform* root cause in
   `~/.jeryu/runners/*/config.toml`, not per-repo); `redline-testing` floats its
   image. No `glab`/PAT/`curl api/v4` bypass exists in any CI script today — a good
   baseline we must protect. `jekko`/`jnoccio` are registered but lack `.jeryu/`;
   `jekko` uses an http:// remote.
5. **No post-merge GitHub relay.** After a local-GitLab MR merges to main, nothing
   pushes to the external GitHub remote. `github.rs` has no `create_pull_request`,
   so the protected-main PR fallback doesn't exist. Two anti-patterns push to
   GitHub *before* CI succeeds (`release/full_path.rs:268-279`,
   `engine_webhook_push.rs:46-55`) — must not be extended.

### Decisions locked with the user (2026-05-29)
- **TUI**: rebase `recovery/phase0-tui` onto current main, then merge (captures
  Codex's recent runner/CI work).
- **Runner autonomy**: auto-recover *with guardrails* — auto scale-up, restart
  hung runners, AND rate-limited/idempotent/refuse-on-partial zombie GC; every
  action emits an alert.
- **CI rollout**: ship the tooling AND auto-open fix-MRs into every registered
  repo (each repo's own CI gates its MR).
- Earlier locked: relaxed threat model (SHA256 + GitLab Package Registry, no
  cosign); baked `jeryu/ci-base:1.95.0`; templates opt-in tooling now extended to
  auto-MR rollout.

### Hard constraints (apply to every phase)
- **Only work in `~/jeryu/` using git tools. NEVER create temp folders.** Use
  `git worktree add ~/jeryu/.worktrees/<name>` for isolation and in-repo
  `target/scratch/` for scratch. (Earlier this session I used `/tmp` for smoke
  tests — that is now forbidden; a CI smoke asserts no `/tmp`/`mktemp` writes in
  our scripts.)
- **Agents act only through jeryu** (CLI/MCP/`/api/v1/*`) — never `glab`, never
  PAT+`curl .../api/v4`, never token-in-URL remotes. New agent-facing tooling must
  never leak those patterns into examples.
- **Untagged-runner policy** is sacred — no `tags:` anywhere; the `ci_runner_policy`
  gate enforces it.
- **Frequent jankurai audits**: every phase MR runs `just score` + `jankurai
  audit` on its own diff at the acceptance gate; results posted to AGENT_LIVE.log.
- **Codex stay-out** (consume read-only, never edit without a live-log hand-off):
  `tools/security-lane.sh`, `scripts/install-security-tools.sh`,
  `scripts/ci-parity.sh`, the `jankurai_*` security-lane jobs, `src/ci_policy.rs`,
  `src/runner_fleet.rs`, `src/runner_scheduler.rs`, `src/pool_doctor.rs`, the
  hook renderers (`repo_standard_render.rs`, `access.rs`, `repo_direct_hooks.rs`),
  and `.github/workflows/{rust,jankurai}.yml` security jobs.

### Coordination
- **`~/jeryu/AGENT_LIVE.log`** — ANSI live chat (`tail -f`). claude = **bright
  yellow `\033[1;38;5;226m`** (new, Opus 4.8 ultracode), Codex = bright green
  (46), system = grey (245). Append-only; post on every phase boundary. (First
  action on plan approval: a bright-yellow burst to Codex covering the three
  locked decisions, the no-temp-folder rule, and my shippable-now queue — plan
  mode currently blocks me from posting.)
- **`~/jeryu/AGENT_WORK.md`** — durable backlog/claims/proof.
- **`~/jeryu/MASTER_HARDENING_PLAN.md`** — this plan, mirrored in-repo for Codex.

### Already shipped this session (in flight)
- **MR #9** `feat/ci-base-image` — baked `jeryu/ci-base:1.95.0` (rust 1.95 +
  nextest + jankurai + node 20 + python + sqlite, **no sccache**); local build +
  smoke green.
- **MR #10** `recovery/phase0-tui` → main — the TUI reset (will switch to
  rebase-then-merge per decision).
- **MR #11** `feat/post-merge-mirror-push` — MIRROR-PUSH-H producer
  (`src/git/mirror_jobs.rs`, hook at `gitlab_merge.rs:159`, 5 tests).
- `feat/repo-standard-templates` — REPO-STD-E template tree + `apply.sh` shim
  (commit fbcf1c5-era; the rejected commit will be re-landed cleanly with the
  no-temp-folder smoke instead of `/tmp`).
- Codex shipped: bounded probes, `jeryu runner fleet doctor/repair` (safe,
  partial-inventory guarded), `jeryu ci doctor/fleet-doctor/template`,
  `.jeryu/ci.toml` policy fields, `src/ci_policy.rs` bypass scanner
  (`scan_for_agent_bypass_lines`), summary-only TUI counts.

---

## End state (one paragraph)

`jeryu tui` opens the redesigned Flight Deck: a **Repos lens** where `~/veox-*`
collapses into one drill-down family and single projects stay flat (≈8 rows, not
16), and a **Runners lens** showing a live per-node grid (xbabe0/1/2/3) — what's
running, idle, hung, failed, and each node's disk/utilization — fed by a hardened
`/api/v1/runners`. A **smart runner manager** keeps utilization high: it detects
the DB-vs-reality drift, scales underfilled nodes up, restarts hung runners, and
GCs zombie containers (rate-limited, idempotent, refuse-on-partial), emitting a
fast alert (TUI banner + `health --json` exit code + event ledger) on every issue
and action. MR pipelines are a `needs:`-DAG with release/artifact build off the
critical path (~45→~18 min), baked into the `ci-base` image and the
`repo-standard` template, then auto-MR'd across all registered repos. After a
local MR merges to main, jeryu pushes to the external GitHub main — or opens a PR
if main is protected. Agents do all of this through `jeryu` alone; nothing
hand-curls GitLab.

---

## Phase roadmap

```
0.  Coordination (chat files, backlog, live-tail) ........ done; refresh on approval
A.  Stop the bleeding: bounded runtime + drift alert ..... carve-out shippable now
B.  Ship the TUI reset (rebase recovery/phase0-tui→main) .. user: rebase-then-merge
C.  TUI bodies: U22 live runner pane + U18 family pane .... blocked on B
D.  Baked jeryu/ci-base:1.95.0 image ..................... MR #9 (shippable now)
E.  repo quickstart/standard + AUTO-MR rollout to repos ... blocked on D
F.  Smart multi-node placement (node_score) .............. Codex-adjacent
G.  Signed-bin install (SHA256 + Package Registry) ....... independent
H.  Post-merge GitHub push + PR-fallback ................. MR #11 producer done
I.  Agent-first CI heal (jeryu ci tail/diagnose/heal) .... blocked on D
J.  jeryu serve API + key ................................ independent
K.  Jankurai sweep (access.rs split, dedup, CI echoes) ... backfill
L.  Runner Utilization Manager (NEW) ..................... metric→hung→self-heal
M.  CI Acceleration (NEW) ................................ de-serialize + needs-DAG
N.  Fast Notification (NEW) .............................. runner events + alerts
```

Dependency spine: B→C; D→E,I; A→L; L→N; M proven on jeryu → folds into D/E template.

---

## Per-phase detail (new + materially-changed phases first)

### Phase L — Runner Utilization Manager (NEW) — the user's top priority

North star: **runners never idle, fleet never silently spins down, hung runners
detected+recovered fast, smart manager fixes without causing new problems.**
Claude *layers* on top of Codex's classifier/topology — consumes
`runner_fleet::RunnerFleetReport`, never mutates the classifier or
`runner_scheduler.rs`.

- **L0 — Drift alert (the 10-vs-1,976 fix), attaches to Phase A.** One health
  check comparing `db.count_active_managers()` vs live `RunnerFleetTotals` running
  count; emit `runners.drift` + a `FleetDrift` alert. Files:
  `src/engine_background_health.rs`, `src/health.rs`. Read-only consumer of an
  existing report. **Shippable now.**
- **L1 — Utilization metric.** New `src/runner_utilization.rs` (≤200 LOC, pure):
  per-pool/per-node `busy/online/idle/stuck` from the fleet report + GitLab runner
  `last_contact`. Keep ≥2 warm idle/node for burst; flag under-utilization
  (busy<0.5·online & queue empty) and over-utilization. Surface in `health --json`
  as `runners.utilization_ratio/idle_count/stuck_count`. **Shippable now.**
- **L2 — Hung-runner detection.** Extend `runner_utilization.rs`: stuck = online
  >30 min AND queue non-empty AND no job pickup in T; distinct from
  legitimately-idle (queue empty). Emits an advisory `hung` state; does NOT mutate
  manager DB. **Shippable now; remediation gated by L3.**
- **L3 — Self-heal loop (auto-recover with guardrails — user choice).** New
  `src/engine_background_runner_heal.rs` (≤220 LOC), 60s cadence, state machine
  detect→classify→act→verify→notify:
  - scale underfilled nodes up to topology (reuse `pool::scale_*`),
  - restart hung runners (reuse `stop_manager_for_node` SIGQUIT-drain),
  - GC zombie/`stale_created` containers (the 1,936) — **rate-limited
    (≤N/node/cycle), idempotent, refuse-on-partial-inventory, TOCTOU re-check,
    reserved-node skip, grace age** — reuse Codex's `PoolOrchestrationLeaseGuard`
    (`pool_scale.rs:174-179`).
  - **Codex hand-off required** before mutating drain/scale lands (adjacent to
    Codex force-drain). Ship dry-run/preview-emitting-events FIRST, flip to
    executing after Codex ACK in AGENT_WORK.md.

Acceptance: unit tests for drift detection, utilization math, hung classification,
and a chaos test (inject partial inventory → heal loop refuses to act). After
landing, `jeryu health --json` reports accurate `active≈40` and the zombie count
trends to ~0.

### Phase M — CI Acceleration (NEW) — "CI screams"

Prove on jeryu's own `.gitlab-ci.yml` first, then bake into D's image + E's
template + the auto-MR rollout.

- **M1 — De-serialize `rust_test_lib`.** Replace `needs:[rust_test_select]`
  (completion gate) with artifact-only `needs:` so test-lib starts immediately and
  blocks only on reading `test-plan.json`. Files: `.gitlab-ci.yml` test stage,
  `ops/ci/rust-lane.sh`. (Not a security job — confirm in AGENT_WORK before edit.)
  **Shippable now.**
- **M2 — `needs:`-DAG (kill stage barriers) + smoke gate before evidence.** Add
  `needs:` to non-security jobs so the pipeline collapses 7 sequential stages →
  ~4 concurrent levels; a fast smoke (tui-smoke + health) gates the slow evidence
  lanes. **Keep the `jankurai_*` security jobs byte-unchanged** (Codex). **Blocked
  on Codex ACK** that adding `needs:` to their *downstream* is fine.
- **M3 — Release/artifact build OFF the MR path.** `release_ready` →
  `rules: main-only` (or `allow_failure:true`); confirm `post_merge_build_artifact`
  stays main-only. **Shippable now.**
- **M4 — Hyperparallel form into template + image.** Bake the proven DAG into
  `templates/repo-standard/.gitlab-ci.yml` (Phase E) and rely on `ci-base` (Phase
  D) to remove per-job bootstrap from the path. **Phase D now; E after D.**

Acceptance: measured MR wall-clock before/after on jeryu (target ≥50% cut);
critical-path depth 7→≤4.

### Phase N — Fast Notification (NEW)

Zero outbound alerting exists today (pull-only). Make every runner/CI issue
surface immediately and low-noise.

- **N1 — Runner-lifecycle event variants.** Extend `TuiEventKind`
  (`src/api/events.rs`, currently 44 kinds, none runner-lifecycle) with
  `RunnerNodeUnreachable / RunnerNodeBackOnline / FleetUnderfilled /
  RunnerDiskCritical / RunnerOrphanedDetected / HungRunnerDetected`; emit only on
  state *transitions*. In-memory `TuiEvent` — no DB schema change (respects
  AGENTS.md boundary). **Shippable now.**
- **N2 — Alert taxonomy + health exit code + hysteresis.** 6 alerts with
  CRITICAL→`exit(1)` / WARNING→`exit(0)` and anti-flap (disk <10GB warn, <5GB
  crit; ignore drift<2 or <60s churn). Files: `src/health.rs`. Depends on L0 drift
  field. **Shippable after L0.**
- **N3 — Optional webhook-out push.** New `src/notify/webhook.rs` (≤150 LOC):
  CRITICAL events fan out to a configured Slack/Discord webhook. Lower priority
  (TUI live pane + event ledger satisfy "fast notify" first). **Shippable now.**

### Phase C — TUI bodies: live runner pane (U22) + family pane (U18) — blocked on B

The redesigned TUI's read model is already correct; the gaps are render/nav.

- **C1 — U22 live multinode runner pane.** Extend `RunnersDashboard`
  (`src/api/dashboards/runners.rs`, flat today) with `nodes: Vec<RunnerNode>`
  (`alias, role: Host|Worker|Reserved, online/busy/idle/degraded, cpu/mem/disk,
  last_probe_at, last_probe_error, reachable`) sourced from
  `runner_fleet::RunnerFleetNodeReport` (already carries
  `node_alias/reachable/max_managers/expected/db_active/live_running` + per-container
  classification). Build the pane body in `src/tui/lenses/runners/{view,data,nav}.rs`
  using existing widgets (`virtual_table`, `heatmap`, `status_strip`,
  `freshness_chip`); fleet utilization banner + per-node rows + drain/scale
  preview; subscribe to N1 events. New `/api/v1/runners` route
  (`src/inspection/runners.rs`). Fixtures + tuiwright tests. **Codex confirmed its
  runners-lens touches are summary-counts only — boundary clean.**
- **C2 — U18 Repos family pane (progressive disclosure).** The read model already
  has `ReposSnapshot{ families, repos }` with `infer_family` (`read_model_repos.rs:197`)
  and `EntityKind::{RepoFamily,Repo,Branch}` (`entity_kind.rs:8`). The gap is the
  lens: it renders families+repos side-by-side with no collapse/expand and only a
  `DrillSelectedRepo` intent. Add to `src/tui/lenses/repos/{data,view,nav}.rs`:
  `expanded_families` state, `DrillFamily`/`ToggleFamilyExpand` intents, and
  hierarchical render (families collapsed by default, ▶/▼, repos nested on expand).
  **veox-* must collapse into one row; singles stay flat; high-level view never
  floods.** Improve `infer_family` to group by `local_root` parent
  (`/home/ubuntu/veox-split/*` → family `veox-split`) — that read-model tweak is
  the only `read_model_repos.rs` touch (coordinate with Codex; else do TUI-side
  grouping). Mission lens already rolls up by family — no change.

### Phase H — Post-merge GitHub push + PR-fallback (producer shipped in MR #11)

- **H1 — `create_pull_request` on `github.rs`.** Add `POST /repos/{o}/{r}/pulls`
  (github.rs has `post_check_run/list_open_prs/get_pr_state` but no create). Reuse
  its `req()`/`map_http_err()`.
- **H2 — Consumer `src/engine_background_remote_mirror.rs`.** Reads
  `~/.jeryu/mirror_intents.jsonl` (MR #11 producer), claims by offset, and for each
  configured GitHub remote: fast-forward push to main; on protected-branch
  rejection (probe `/branches/{b}/protection` or catch 403) → push ephemeral
  branch + `create_pull_request`. Idempotent (track consumed SHAs). Reuse
  `git/mirror.rs::mirror_push`. **Runs only post-merge, never on push hook / before
  CI.** Do NOT extend `release/full_path.rs::perform_github_handoff` or
  `engine_webhook_push.rs` (the anti-patterns).
- **H3 — Policy.** New `[main_relay.github]` block (`enabled/remote/branch/
  fallback_to_pr`) parsed by new `src/policy_main_relay.rs` — NOT an overload of
  `offline_release_mirror` and NOT `ci_policy.rs`.
- **H4 — TUI Release lens** shows mirror status (pending/pushed/PR-opened/failed).

### Phase E — repo quickstart/standard + AUTO-MR rollout (user: auto-MR all repos)

- **E1 — Re-land the template tree cleanly.** `templates/repo-standard/` (done) +
  `apply.sh`, but replace the `/tmp` smoke with a `git worktree`/`target/scratch/`
  smoke (no-temp-folder rule).
- **E2 — `jeryu repo quickstart`** (infer name/namespace/branch from git remote) +
  **`jeryu repo standard {apply,verify}`** (new `src/repo_standard/*`, extend
  `src/cli_defs_commands_repo.rs`). Blocked on D (template references the image).
- **E3 — CI remediation lane** `src/repo_ci_remediate.rs` (+ subcommand in our
  `src/commands/ci.rs`): consumes `ci_policy::doctor_repo` findings read-only and
  *applies* fixes — strip `tags:`, remove job-level `RUSTC_WRAPPER`/`SCCACHE_DISABLED`
  overrides, re-pin floating images, convert http→ssh remote, flag missing
  `.jeryu/`.
- **E4 — Auto-MR rollout (user choice).** `jeryu repo standard rollout` opens a
  fix-MR into each registered repo (redlineDB tags+sccache first; then
  redline-testing, jansu, jnoccio, jekko, the veox family) via the jeryu MR API
  (never glab). Each repo's own CI gates its MR; nothing merges without green.
- **E5 — Platform sccache fix (Phase A-adjacent).** The real root cause is
  `~/.jeryu/runners/*/config.toml` auto-wiring `RUSTC_WRAPPER=sccache`. Add a
  runner-config audit/repair so projects stop papering over it per-job. Extend
  `src/pool_doctor.rs`? No — that's Codex's; add a standalone check in our
  `src/commands/ci.rs` that reports "fix at runner config, not per-job."

### Phase A — Stop the bleeding (carve-out) — shippable now
- **RUNNER-LOG-CAPS**: bounded `docker run` flags only (log rotation 50m×3,
  `--memory=8g`, `--cpus=4`, `--ulimit nofile=65536`, `--restart unless-stopped`)
  in `src/runner_backend_{remote,local}.rs` format strings. Codex ACKed this
  carve-out; force-drain/build-TTL stay Codex's.
- **L0 drift alert** (above) lands here.

### Phase B — Ship the TUI reset (rebase-then-merge, user choice)
Rebase `recovery/phase0-tui`'s 28 commits onto current `main` (captures Codex's
runner/CI work), resolve conflicts favoring the reset's lens scaffolds, run the
full test baseline (≥1493 lib + 167 tuiwright), then merge. Supersedes the
merge-as-is MR #10. Coordinate the rebase window with Codex (shared worktree
hygiene).

### Phases D, F, G, I, J, K
Unchanged from prior plan except: **D** gains the M4 hyperparallel DAG; **F**
(smart placement) stays Codex-adjacent (do not touch `runner_scheduler.rs`); **G**
SHA256+Package-Registry install; **I** `jeryu ci tail/diagnose/heal` (classifier
extends `src/ci_failure.rs` in place — 11 categories incl. SccacheHang,
OverlayfsFalsePositive, OOM, etc.); **J** `jeryu serve`+`jeryu key`; **K** jankurai
sweep (split `access.rs` 1183 LOC, dedup, CI secret-echoes — after Codex CI series
stable).

---

## Sequencing (tiers)

```
Tier 0 — ship now, no deps (parallel):
  A/RUNNER-LOG-CAPS · A/L0 drift alert · M1 test-lib · M3 release-off-MR
  N1 runner events · D ci-base image (MR #9) · H1 create_pull_request
  H3 policy parser · E1 template re-land (worktree smoke)

Tier 1 — depend on Tier 0:
  L1 utilization (reads fleet report) · N2 alert taxonomy (needs L0)
  B TUI rebase-merge · H2 mirror consumer (needs H1/H3) · N3 webhook
  G install-pull · J serve+key

Tier 2 — depend on Tier 1:
  C1 U22 live pane · C2 U18 family pane   (both need B)
  E2 quickstart/standard · E3 remediate · M4 template DAG   (need D)
  L2 hung detection (needs L1) · I ci-heal (needs D)

Tier 3 — gated on Codex hand-off / ACK:
  L3 self-heal mutating actions (dry-run first; Codex force-drain overlap)
  M2 full needs-DAG (security-job downstream ACK)
  E4 auto-MR rollout (after E2/E3 proven on one repo)
  E5 platform sccache fix · K jankurai sweep (after Codex CI stable)
```

---

## Verification (per-phase smoke)

| Phase | Smoke |
|---|---|
| A | new runner: `docker inspect` shows log/mem/fd/restart caps; `health --json` reports `runners.drift`; df stable. |
| B | `jeryu tui` renders Mission <5s; `gu`→runners, `gr`→repos navigate; ≥1493 lib + 167 tuiwright pass; `just score` ≥ prior. |
| C | `gu` shows live 4-node grid w/ real telemetry + utilization banner; `gr` shows veox collapsed to one family row, expand reveals 9; ≥10 new lens tests + tuiwright. |
| D | `docker run ci-base rustc --version`=1.95.0, all 14 tools pinned, sccache absent; `bash scripts/install-security-tools.sh` succeeds inside image. |
| E | `jeryu repo quickstart && jeryu repo standard apply` in a worktree (no /tmp) → full `.jeryu/` + DAG `.gitlab-ci.yml`; `verify` clean; rollout opens a green-gated MR into redlineDB stripping tags+sccache. |
| H | local MR merge → consumer pushes to GitHub main; against a protected-main fixture → opens a PR; idempotent on replay. |
| L | inject partial inventory → self-heal refuses; healthy → zombie count trends to 0 and `active≈40`; utilization/hung unit + chaos tests pass. |
| M | jeryu MR wall-clock measured ≥50% lower; depth 7→≤4; security jobs byte-unchanged. |
| N | node-down → CRITICAL TUI banner + `health` exit 1 + one event (not per-cycle); flap-suppressed. |
| K | `just score` rises monotonically; no test regressions. |

---

## Critical files (by phase)

- **L**: new `src/runner_utilization.rs`, new `src/engine_background_runner_heal.rs`,
  `src/health.rs`, `src/engine_background_health.rs` (read `runner_fleet.rs`,
  `pool_scale.rs` — do not edit classifier).
- **M**: `.gitlab-ci.yml`, `ops/ci/rust-lane.sh`, then
  `templates/repo-standard/.gitlab-ci.yml`, `docker/ci-base/`.
- **N**: `src/api/events.rs`, `src/health.rs`, new `src/notify/webhook.rs`.
- **C**: `src/api/dashboards/runners.rs`, new `src/inspection/runners.rs`,
  `src/inspection/router.rs`, `src/tui/lenses/runners/{view,data,nav}.rs`,
  `src/tui/lenses/repos/{view,data,nav}.rs`, `src/api/read_model_repos.rs`
  (family-by-path tweak), fixtures + `tests/tuiwright/lenses_{runners,repos}.rs`.
- **H**: `src/git_host/github.rs` (+create_pull_request), new
  `src/engine_background_remote_mirror.rs`, new `src/policy_main_relay.rs`,
  `src/tui/lenses/release/view.rs`.
- **E**: `templates/repo-standard/*`, new `src/repo_standard/*`, new
  `src/repo_ci_remediate.rs`, `src/commands/ci.rs`, `src/cli_defs_commands_repo.rs`.

Reuse (don't reimplement): `runner_fleet::RunnerFleetReport/NodeReport`,
`pool::scale_*` + `stop_manager_for_node` + `PoolOrchestrationLeaseGuard`,
`git/mirror.rs::mirror_push`, `git/mirror_jobs.rs` producer, `github.rs::req`,
`src/tui/widgets/{virtual_table,heatmap,status_strip,freshness_chip}`,
`read_model_repos::{ReposSnapshot,infer_family}`, `ci_policy::doctor_repo`.

---

## Risk register

| Risk | Mitigation |
|---|---|
| TUI rebase conflicts (main diverged) | Rebase in a dedicated worktree; resolve favoring reset scaffolds; full test baseline before merge; abort+ask if scope balloons. |
| Self-heal deletes a live runner | Whitelist `stale_created`/zombie only; never `over_capacity`-while-running; TOCTOU re-check, rate-limit, refuse-on-partial, reserved-node skip; dry-run until Codex ACK. |
| `needs:`-DAG touches security jobs | Add `needs:` only to non-security downstream; keep `jankurai_*` byte-identical; Codex ACK gate. |
| Auto-MR rollout breaks a repo's CI | Each MR gated by that repo's own green CI; one repo (redlineDB) first as canary; never auto-merge. |
| GitHub push leaks creds / pushes pre-CI | SSH-only or token-from-`~/.jeryu/jeryu.env`; consumer runs only post-merge; never extend the pre-CI handoff anti-patterns. |
| Temp-folder regression | CI smoke asserts no `/tmp`/`mktemp` in our scripts; all isolation via `git worktree`. |
| Codex scope collision | AGENT_WORK claim + AGENT_LIVE ACK before any shared-file edit; stay-out list enforced. |

## Anti-goals
- No editing Codex's stay-out surface without a hand-off.
- No new HTTP framework (reuse axum `serve_inspection`).
- No cosign/SLSA (relaxed threat model).
- No `tags:`, no GitLab bypass, no temp folders.
- No oversized new files (respect lens/template caps).
- No mutating self-heal before Codex ACK.
