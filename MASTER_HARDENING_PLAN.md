# Jeryu master hardening plan — runner robustness, multi-node, TUI ship, agent-first CI

## Context

`jeryu` runs on a single master node (xbabe2) and orchestrates Docker runners across `xbabe0`, `xbabe1`, `xbabe2`, `xbabe3` for many internal projects (jeryu, redlineDB, redline-testing, jekko, jankurai, jansu, openQG, veox-split/*). Three concrete problems are bleeding into agent workflows today:

1. **Runners crash / drift**: containers run with unbounded `docker logs`, no memory/FD caps, no force-drain when an SSH node disappears. Health loop catches disk pressure but not the other failure modes. Net effect: bad nodes poison the scheduler and a runaway log fills /. `src/engine_background_health_pressure.rs:63-210` already does the hard work for disk; the other tiers are missing.
2. **Projects get confused**: every Rust repo we audited has independently re-fought the same battles (sccache hangs, overlayfs-apt false positives, git-clone thundering herd, `tags:` lottery, `rust:latest` time bomb in openQG). The most-patched CI file is `~/redlineDB/.gitlab-ci.yml` (939 LOC, ~16 cache/clone fix commits). Each project re-derives the workaround in isolation.
3. **The TUI reset never reached the user's hands**. Branch `recovery/phase0-tui` is 28 commits ahead of `main`, has a clean 3-way merge (`git merge-tree` reports zero conflict markers), and 495 + 167 tests pass on it. The runners lens (U22) is a placeholder; the whole reset is unviewed because no one shipped the merge. The reset is the substrate for the multi-node runner-health pane the user wants.

Adjacent context that shapes the plan:
- Threat model is relaxed (single master node, SSH-gated). No cosign/OIDC; just SHA256 + GitLab Package Registry.
- `[offline_release_mirror]` in `.jeryu/policy.toml` is written by `src/repo_direct.rs:88-99` but **no code reads it** — the post-merge external-mirror push is dormant plumbing.
- Codex just landed the native-only security toolchain installer (`scripts/install-security-tools.sh`, `tools/security-lane.sh` rewrite, parity script). Those files are stable; we coordinate around them via `~/jeryu/AGENT_CHAT.md` (chat) and `~/jeryu/AGENT_WORK.md` (work items). Codex's untagged-runner-policy work on `codex/untagged-runner-policy` is the precondition for smart multi-node placement.

Goal: make jeryu the world's friendliest agent-first CI tool — runners that never silently die, one image projects can trust, a TUI you can launch and immediately understand, and a CI loop agents can heal without restart.

---

## End state (one paragraph)

`jeryu tui` opens Flight Deck with the runners lens showing a per-node health grid (xbabe0/1/2/3) sourced from a hardened `/api/v1/runners` route. Every Docker runner across the fleet runs with bounded logs, capped memory/FD, restart-on-failure, and a force-drain TTL on unreachable nodes. A baked `jeryu/ci-base:1.95.0` image carries rust + nextest + jankurai + node + python + sqlite so every project pulls the same toolchain with zero bootstrap. A single command (`jeryu repo quickstart`) onboards a new repo: writes canonical `.jeryu/*.toml`, configures the local GitLab project, and mints the API token. Every merge to main publishes `target/release/jeryu` + `SHA256SUMS` to the local GitLab package registry; `jeryu install latest` pulls and atomically installs after SHA256 verification. After internal merge, the dormant `[offline_release_mirror]` push wakes up and mirrors tagged refs to whichever GitHub/GitLab remote the repo declared. Agents call `jeryu ci heal --job <id>` to diagnose and surgically retry failures without restarting CI.

---

## Phased roadmap (sequenced for fastest user value)

```
0. Coordination kickoff (chat files, backlog, live-tail)── immediate, no code
A. Stop the bleeding (runner crash root causes)        ── 1 MR, 1 day
B. Ship the TUI reset (recovery/phase0-tui → main)     ── 1 MR, 0.5 day
C. U22 runners lens body (multi-node health pane)      ── 1 MR, 2–3 days
D. Baked jeryu/ci-base:1.95.0 image                    ── 1 MR, 2 days [parallel with C]
E. `jeryu repo quickstart` + repo standard templates   ── 1 MR, 1–2 days
F. Smart multi-node placement (node_score)             ── 1 MR, 2 days
G. Signed-bin pipeline (SHA256 + Package Registry)     ── 1 MR, 1 day
H. Remote mirror push after internal merge             ── 1 MR, 2 days
I. Agent-first CI heal (jeryu ci heal/tail)            ── 1 MR, 2–3 days
J. jeryu API surface formalized (`jeryu serve` + key)  ── 1 MR, 1–2 days
K. Jankurai sweep (split access.rs, dedup, CI echoes)  ── several small MRs, backfill
```

Dependencies: B unblocks C (which lands on the reset surface). D is independent and runs in parallel with B/C. E depends on D (template references the baked image). F is independent. G is independent. H is independent. I depends on D (uses the baked image's tooling). J is independent. K is backfill, runs anytime after Codex's CI series is fully stable.

---

## Per-phase detail

### Phase 0 — Coordination kickoff (immediate on plan approval)

**Three files appear in `~/jeryu/` the moment the plan is approved**:

1. **`~/jeryu/AGENT_WORK.md`** — durable shared backlog. Content per **Appendix A** below. Both agents append/edit rows as work moves. Status: `pending → in-progress → blocked → done`.
2. **`~/jeryu/AGENT_LIVE.log`** — live ANSI-colored chat for `tail -f`. **Claude's text bright pink (256-color 205), Codex's bright green (256-color 46), bold, with timestamps.** Format and seed per **Appendix C**. Both agents append (never edit prior lines) so `tail -f` is monotonic.
3. **One-paragraph hello append** to existing `~/jeryu/AGENT_CHAT.md` per **Appendix B**, pointing Codex at the new live-tail file.

**Cadence**: after this point, claude:
- Posts to `AGENT_LIVE.log` at every meaningful moment: starting a phase, hitting a blocker, finishing an MR, replying to Codex. No silent stretches > 30 min during active work.
- Updates `AGENT_WORK.md` rows immediately when status changes.
- Reads `AGENT_LIVE.log` (tail) before every phase boundary to catch Codex's notes.

This phase has no code changes. It exists to make every subsequent phase observable in real time.

---

### Phase A — Stop the bleeding (runner crash root causes)

**Branch**: `fix/runner-bounded-runtime`.

**What changes**:
1. **Bounded docker logs + caps** on every runner container. In the `docker run` format strings in `src/runner_backend_remote.rs` (the `format!()` around line 140-164) and the local equivalent in `src/runner_backend_local.rs`, append:
   - `--log-driver=json-file --log-opt max-size=50m --log-opt max-file=3`
   - `--memory=8g --memory-swap=8g`
   - `--cpus=4`
   - `--ulimit nofile=65536:65536`
   - `--restart unless-stopped` (already enforced locally at `src/pool_scale.rs:136-146`; mirror to the remote start path).
2. **Force-drain unreachable nodes**. In `src/pool_scale.rs::reconcile_manager_runtime_state` after the `mark_node_managers_unreachable` call (lines 195-201), track a per-alias consecutive-failure counter in a new `node_probe_state: BTreeMap<String, u32>` parameter on the reconcile call. After N=6 consecutive failures (= 30 min at the 5-min health cadence in `src/engine_background_health.rs:18`), promote the managers from `node_unreachable` → `stopped` via the existing `store.update_manager_state(id, "stopped")` call already in scope (used at line 163). Smart placement (Phase F) then fills the gap.
3. **Build-dir TTL eviction**. Add a sweep to `src/engine_background_health_pressure.rs` that prunes `/cache` build dirs older than 72 hours and not currently referenced by an active manager (use `list_managed_containers` from `src/runner_backend_remote.rs:240` for the live set). Conservative — only frees stale dirs, never touches active ones.

**Files touched**: `src/runner_backend_remote.rs`, `src/runner_backend_local.rs`, `src/pool_scale.rs`, `src/engine_background_health_pressure.rs`, + 3 new tests.

**Files NOT touched**: anything in Codex's stable surface (`tools/security-lane.sh`, `scripts/install-security-tools.sh`, `scripts/ci-parity.sh`, security jobs in `.gitlab-ci.yml` / `.github/workflows/{rust,jankurai}.yml`).

**Acceptance**:
- `docker inspect <new-runner>` shows `LogConfig.Config.max-size = "50m"`, `Memory = 8g`, `Ulimits.nofile.Soft = 65536`, `RestartPolicy.Name = "unless-stopped"`.
- Existing `pool_scale` tests still green; one new test simulates `unreachable → unreachable → … (6x)` and asserts state transitions to `stopped`.
- Disk doesn't fill — `df -h /` stable over a 24-hour soak.

**Coordination**: post a one-line claim in `AGENT_WORK.md` before touching the listed files.

---

### Phase B — Ship the TUI reset

**Branch**: `merge/tui-reset-to-main`.

**What changes**: rebase or 3-way merge `recovery/phase0-tui` into `main`. No semantic edits. The diff is 28 commits, 81 files, +2477/-525 lines. `git merge-tree main recovery/phase0-tui` reports zero conflict markers (verified by exploration agent).

**Files touched**: rebase only — no manual edits expected. If conflicts surface, resolve in favor of `recovery/phase0-tui` (it has the lens scaffolds + new test harness).

**Acceptance**:
- `cargo nextest run -p jeryu --lib` shows ≥1493 tests pass (baseline from Codex's `just fast`).
- `cargo nextest run -p jeryu --test tuiwright` shows 167+ tests pass.
- `jeryu tui` launches, renders Mission lens within 5s, navigates through all 14 lenses (Mission, Queue, Repos, Workflow, Evidence, Runners-placeholder, Agents, Autonomy, Bugs, Cache, LLMs, Release, SourceDoctor, VTI).
- `just score` ≥ current 89.

**Coordination**: Codex's `codex/tui-reset-integration-20260526` branch and `recovery/phase0-tui` may overlap. Ask Codex in `AGENT_CHAT.md` which branch is canonical before merging. The TUI_RESET_PLAN_FINAL.md §0.1 claim board (`/home/ubuntu/jeryu/TUI_RESET_PLAN_FINAL.md:30-76`) is the source of truth on per-unit ownership.

---

### Phase C — U22 runners lens body (multi-node health pane)

**Branch**: `feat/u22-runners-lens-body`. Strictly depends on B.

**What changes**:
1. **Extend the data model**. In `src/api/dashboards/runners.rs` (currently 56 LOC, well under cap):
   - Add `node: Option<String>` to `RunnersItem`.
   - Add a new struct `RunnerNode { alias, role: NodeRole, managers_active, managers_desired, cpu_pct, mem_pct, disk_free_gb, last_probe_at, last_probe_error, reachable }` and a `pub nodes: Vec<RunnerNode>` field on `RunnersDashboard`.
   - `NodeRole` enum: `Host | Worker | Reserved` (`Reserved` for xbabe2 per `src/config_support.rs:118` `STANDARD_POOL_RESERVED_NODE_ALIASES`).
   - All new fields `#[serde(default)]` for forward compatibility.
2. **Telemetry**. Extend `probe_node` in `src/runner_backend_remote_support.rs` (it already does an SSH probe — add `df -h /`, `free -m`, `cat /proc/loadavg` to the same SSH session and parse). Cache in `InspectionState` updated by the 5-min health loop.
3. **New inspection route**. Create `src/inspection/runners.rs` (≤180 LOC) and add `route("/api/v1/runners", get(get_runners))` to `src/inspection/router.rs` (currently lines 22-37 has all the routes). Wrap response in `InspectionEnvelope<RunnersDashboard>` per the contract at `src/api/inspection.rs`.
4. **Lens body**. Replace the placeholder in `src/tui/lenses/runners/view.rs` (lines 30-33) with a real layout using the existing reusable widgets:
   - Header chunk: reuse `freshness_chip` (`src/tui/widgets/freshness_chip.rs`) and `status_strip` (`src/tui/widgets/status_strip.rs`).
   - Body chunk: 4-column `virtual_table` (`src/tui/widgets/virtual_table.rs`, 229 LOC — already in tree) keyed by node alias, columns: pool, idle/busy/degraded counts, cpu%, mem%, disk free, last probe age, action affordances.
   - Optional sub-pane: scale/drain preview modal (single `Paragraph` for v1; full modal can come later).
   - Keep `view.rs` ≤ 350 LOC (current 67); if it grows beyond, extract grid helpers to `src/tui/lenses/runners/grid.rs`.
5. **Data projection**. Extend `RunnersLensInput` in `src/tui/lenses/runners/data.rs` with `nodes: Vec<RunnerNode>`. The pattern matches the existing `from_read_model` selector (lines 16-27).
6. **Nav handlers**. Implementations for the existing intents in `src/tui/lenses/runners/nav.rs` (already declared at lines 10-19) — `j/k` for node row movement, `d` for drain preview, `s` for scale preview, `p` for pause-pool. Mutating ones (PausePool/DrainPool) go through `src/tui/action_registry_entries.rs` at the R1/R2 tier matching their `side_effect_class`.
7. **Fixtures + tests**. New `src/tui/testing/fixtures/runners.rs` (≤200 LOC) with scenarios: `populated_4_nodes`, `xbabe1_unreachable`, `xbabe1_disk_full`, `all_idle`, `source_down`. Extend `tests/tuiwright/lenses_runners.rs` (currently 11 tests, scaffold-only) with at least 5 new scenario assertions (populated render at 80×24 + 220×60, fixture-backed assertion of node count, drain-preview key returns intent).

**Files touched**: `src/api/dashboards/runners.rs`, `src/health.rs` (the projection at lines 168-184 needs to populate `nodes`), `src/runner_backend_remote_support.rs`, new `src/inspection/runners.rs`, `src/inspection/router.rs` (3-line edit), new `src/tui/lenses/runners/grid.rs` if needed, `src/tui/lenses/runners/{view,data,nav}.rs`, new `src/tui/testing/fixtures/runners.rs`, `tests/tuiwright/lenses_runners.rs`.

**Files NOT touched**: any non-runners lens; any DB schema files (`src/db/`); Codex's CI surface.

**Acceptance**:
- `cargo nextest run -p jeryu --lib tui::lenses::runners` and `cargo nextest run -p jeryu --lib api::dashboards::runners` green with ≥10 new tests.
- `cargo nextest run -p jeryu --test tuiwright -- lenses_runners` shows 16+ tests passing.
- Launch `jeryu tui`, press `gu`, see the 4-node grid with real telemetry from the running runners.
- `just score` not worse than after Phase B.

**Jankurai impact**: positive — fills the U22 acceptance row in `TUI_RESET_PLAN_FINAL.md:1074`. No file exceeds caps if the helper-extraction guidance is followed.

---

### Phase D — Baked `jeryu/ci-base:1.95.0` image (parallel with C)

**Branch**: `feat/ci-base-image`.

**What changes**: new `docker/ci-base/` directory in `/home/ubuntu/jeryu` containing:
- `Dockerfile` (FROM debian:bookworm-slim) installing: rust 1.95.0 via rustup, cargo-nextest pinned, jankurai prebuilt (from the existing staged install), node 20 + npm, python3 + pip, sqlite3, build-essential, mold, clang, jq, curl, git, ca-certificates, openssl, zlib1g-dev. Explicitly NO sccache. `CARGO_HOME=/cache/cargo` baked in. Bake the `/cache/git-clone.lock` file path conventions so `flock` works on first job.
- `build.sh` that builds the image with the version derived from `rustc --version` + `cargo --version` (so the image tag is deterministic), tags as `${CI_REGISTRY}/root/jeryu/ci-base:1.95.0` and `:latest`, and pushes.
- A new CI job `ci_base_publish` (stage: `release`, rules: `if: $CI_PIPELINE_SOURCE == "push" && $CI_COMMIT_BRANCH == "main"` and `changes: [docker/ci-base/**]`) that builds and publishes only when the Dockerfile changes.
- Wire jeryu's own `.gitlab-ci.yml` default `image:` (currently `rust:1.95.0` at line 18) to the new image, behind a `JERYU_USE_CI_BASE=1` opt-in for the first MR (so we can A/B). Once green, flip the default.

**Files touched**: new `docker/ci-base/Dockerfile`, new `docker/ci-base/build.sh`, ~30 lines added to `.gitlab-ci.yml` (new job + opt-in image var; placed *outside* Codex's security-job block).

**Files NOT touched**: `tools/security-lane.sh`, `scripts/install-security-tools.sh`, `scripts/ci-parity.sh`, `scripts/install-jankurai.sh` (Codex's surface).

**Acceptance**:
- `docker pull <registry>/root/jeryu/ci-base:1.95.0` returns a working image; `docker run --rm <image> rustc --version` reports `1.95.0`, `cargo-nextest --version` reports the pinned version, `jankurai --version` matches the manifest, `node --version` reports v20.x.
- Jeryu's own pipeline with `JERYU_USE_CI_BASE=1` is green and faster than the baseline (no per-job rustup install).

---

### Phase E — `jeryu repo quickstart` + repo standard templates

**Branch**: `feat/repo-quickstart`. Depends on D landing (template references the baked image).

**What changes**:
1. New directory `templates/repo-standard/` in `/home/ubuntu/jeryu` containing:
   - `.gitlab-ci.yml` template: canonical 5-stage pipeline (quality/build/test/evidence/release), `image: ${CI_REGISTRY}/root/jeryu/ci-base:1.95.0`, no `tags:` (untagged-runner policy from `src/runner_policy.rs:14-66`), shared flock'd git clone snippet, no sccache, `.gitlab/ci/_shared.yml` include.
   - `.jeryu/{ci,policy,repo,backup}.toml` minimal defaults.
   - `.gitlab/ci/_shared.yml` with the canonical `before_script` patterns (apt batching, flock clone, cache eviction).
2. New `jeryu repo quickstart` subcommand (extend `src/cli_defs_commands_repo.rs`, currently lines 74-107). Infer:
   - `--name` from `git remote get-url origin` basename;
   - `--namespace` from first path segment of remote;
   - `--branch` from `git symbolic-ref refs/remotes/origin/HEAD`;
   - `--offline-release-remote` from existing origin if it's a public mirror;
   - default `--direct`, `--protect-main`, `--hooks advisory`.
3. New `jeryu repo standard apply` subcommand: copies templates into cwd, leaves a `.jeryu/standard.lock` file recording template version + checksum so we can detect drift.
4. New `jeryu repo standard verify`: diffs the live files against the template and reports drift in a human-readable format.

**Files touched**: new `templates/repo-standard/`, new `src/repo_standard/{mod,apply,verify,quickstart}.rs` (each ≤200 LOC), extend `src/cli_defs_commands_repo.rs` (~60 lines for the new subcommands), extend `src/dispatch.rs` (~10 lines).

**Files NOT touched**: any other repo's CI (`~/redlineDB`, `~/jansu`, etc.) — projects opt-in by running `jeryu repo standard apply` themselves.

**Acceptance**:
- In a fresh dir with `git remote add origin git@github.com:foo/bar`, running `jeryu repo quickstart && jeryu repo standard apply` produces a fully-populated `.jeryu/` + canonical `.gitlab-ci.yml`.
- `jeryu repo standard verify` reports no drift immediately after `apply`.

---

### Phase F — Smart multi-node placement

**Branch**: `feat/smart-placement`. Independent.

**What changes**: new `src/pool_placement.rs` (≤250 LOC), pure function:
- input: `Vec<(alias, NodeProbe, current_managers)>`
- output: `Vec<(alias, target_managers)>` summing to `STANDARD_POOL_DESIRED_TOTAL` (40 per `src/config_support.rs:125-142`).
- score formula: `weight_idle * (1 - busy_ratio) + weight_disk * (disk_free_ratio) + weight_mem * (mem_free_ratio) - penalty_unreachable`.
- xbabe2 (reserved per `STANDARD_POOL_RESERVED_NODE_ALIASES`) always returns 0.
- Falls back to the static `STANDARD_POOL_TOPOLOGY` when probes are missing.
- Call site: ~10-line edit in `src/pool_scale.rs` around line 170 (where the reconciliation already runs).

**Files touched**: new `src/pool_placement.rs`, `src/pool_scale.rs` (10-line edit), `src/lib.rs` (1 line to declare the module).

**Acceptance**:
- Unit tests covering: all-healthy → matches static topology; xbabe1 disk-full → managers shift to xbabe0/xbabe3; xbabe1 unreachable → 0 target on xbabe1; xbabe2 always 0.
- After landing, the runners lens (from C) visibly rebalances within one health cycle when a node degrades.

**Coordination**: do not touch `src/runner_scheduler.rs` — that's Codex's priority/fairness territory (claim TUI-RESET-20260526-031 in TUI_RESET_PLAN_FINAL.md). Placement and scheduling stay decoupled.

---

### Phase G — Signed binary distribution (SHA256 + Package Registry)

**Branch**: `feat/install-pull-latest`. Independent.

**What changes**:
1. Extend `ops/ci/post-merge-deploy-lane.sh` (called from `post_merge_build_artifact` at `.gitlab-ci.yml:381-391`) to:
   - Compute `sha256sum target/release/jeryu > target/release/SHA256SUMS`.
   - Upload `target/release/{jeryu,SHA256SUMS}` to the local GitLab generic Package Registry at `${CI_API_V4_URL}/projects/${CI_PROJECT_ID}/packages/generic/jeryu/${CI_COMMIT_SHA}/...` using `curl` with `JOB-TOKEN`.
2. New `jeryu install latest` subcommand (extend `src/cli_defs_install.rs:40-52` with `PullLatest { #[arg(long)] verify: bool }`):
   - Queries the registry for the latest published `jeryu` artifact for main branch.
   - Downloads to `~/.jeryu/bin/jeryu.next`.
   - Verifies SHA256 against the published `SHA256SUMS`.
   - Atomically renames to `~/.jeryu/bin/jeryu` (after taking a backup at `~/.jeryu/bin/jeryu.prev`).
3. New `jeryu install rollback` swaps to `~/.jeryu/bin/jeryu.prev`.

**Files touched**: `ops/ci/post-merge-deploy-lane.sh` (extend, do not rewrite), `src/cli_defs_install.rs` (extend), new `src/install/pull_latest.rs` (≤200 LOC), new `src/install/rollback.rs` (≤80 LOC).

**Acceptance**:
- After a merge to main, `jeryu install latest --verify` against `localhost` GitLab succeeds, installs the just-built binary, and `jeryu --version` reports the new commit SHA.
- `jeryu install rollback` returns to the prior version.

---

### Phase H — Remote mirror push (post-merge → external)

**Branch**: `feat/post-merge-mirror-push`. Independent.

**What changes**:
1. Wake the dormant `[offline_release_mirror]` config. Currently `src/repo_direct.rs:88-99` writes it; nothing reads `enabled = true`. Implement:
   - New `src/git/mirror_jobs.rs` (≤250 LOC) with a state machine `pending → in_flight → success | failed{attempts, last_error}` and a new SQLite table `mirror_jobs` (added through DB-owned modules only, per AGENTS.md boundary rule).
   - New `src/engine_background_remote_mirror.rs` (≤200 LOC) consumer polling `mirror_jobs`, calling the existing `src/git/mirror.rs::mirror_push`, with jittered exponential backoff (30s → 30min).
2. **Enqueue hook** at the merge completion point. Find via `grep -n 'merge.*completed\|on_merge\|post_merge' src/git_host/`. Add a 5-line enqueue when the merged ref matches the policy's refspec patterns (`refs/tags/v*` or `refs/heads/release/*` by default).
3. The `[offline_release_mirror]` already supports multiple targets via the `remote` field; allow an optional `[[external_mirrors]]` array in `.jeryu/policy.toml` for fan-out.
4. Surface mirror status in the TUI Release lens (`src/tui/lenses/release/view.rs`) as a small status footer (uses existing `freshness_chip` widget).

**Files touched**: new `src/git/mirror_jobs.rs`, new `src/engine_background_remote_mirror.rs`, ~5-line hook in the merge handler, ~20-line addition to `src/tui/lenses/release/view.rs`, DB migration for `mirror_jobs` table.

**Acceptance**:
- Integration test: create a temp repo with `offline_release_mirror.enabled = true` pointing at a bare repo, tag-push, assert the bare repo receives the tag within 30s.
- TUI Release lens shows mirror status (pending / success / failed) per repo.

---

### Phase I — Agent-first CI heal

**Branch**: `feat/ci-heal`. Depends on D (uses the baked image's tools).

**What changes**: this is the agent-friendliness deliverable. Three new subcommands:
1. `jeryu ci tail --job <id>` — streams the live job log from the GitLab API (or `~/.jeryu/jeryu.env` PAT) over SSE, no restart needed. Implementation reuses `src/gitlab_client.rs` job-trace methods.
2. `jeryu ci diagnose --job <id>` — pulls the trace, runs a classifier on the failure signature, returns a structured diagnosis JSON: `{ category: SccacheHang | OverlayfsFalsePositive | ImagePullFail | OOM | DepChange | TestFail | Unknown, suggested_action: …, evidence: { trace_url, log_excerpt, line_numbers } }`. Categories are seeded from the per-repo CI audit (see Appendix A).
3. `jeryu ci heal --job <id> [--apply]` — wraps `diagnose` and `retry`. Without `--apply` returns the diagnosis. With `--apply`, takes the suggested action: re-trigger the job, evict the cache, or re-pull the image. Uses the existing `retry: 2` infrastructure (`.gitlab-ci.yml:40` pattern) but surgically per-job.

**Files touched**: new `src/cli_defs_ci.rs`, new `src/ci_heal/{mod,classifier,actions,trace}.rs` (each ≤250 LOC), extend `src/cli_defs.rs::Commands` enum, extend `src/dispatch.rs`.

**Acceptance**:
- Agent calls `jeryu ci heal --job 12345` against a recent failed redlineDB job and gets back `{ category: "SccacheHang", suggested_action: "re-trigger with SCCACHE_DISABLED=1", evidence: {...} }`.
- With `--apply`, the retry is triggered and tail-able via `jeryu ci tail`.

---

### Phase J — jeryu API surface formalized

**Branch**: `feat/jeryu-serve-api`. Independent.

**What changes**:
1. `jeryu serve` already exists at `src/inspection/serve.rs:16` (`serve_inspection(listener, state)` with routes for `/api/v1/read-model`, `/events`, `/entity/...`, etc., 47-route surface per the audit). Formalize it: bind to `127.0.0.1:9876` by default; opt-in `--listen 0.0.0.0:NNNN`; reads `~/.jeryu/api.token`.
2. New `jeryu key {init|show|rotate}` subcommands. `init` writes `~/.jeryu/api.token` (mode 0600) with a 32-byte random token if absent. `show` prints (with confirm prompt). `rotate` generates new + invalidates old.
3. Minimal Bearer middleware on the mutating routes only (preview/execute). Read routes stay open since threat model is relaxed. Implementation as an `axum::middleware::from_fn` in `src/inspection/router.rs`.

**Files touched**: new `src/serve/{mod,auth}.rs`, new `src/cli_defs_key.rs`, extend `src/cli_defs.rs`, extend `src/inspection/router.rs` (~15 lines for middleware).

**Acceptance**:
- `jeryu key init && jeryu serve &; curl -s http://127.0.0.1:9876/api/v1/read-model | jq .` returns valid envelope.
- Mutating POSTs without `Authorization: Bearer $(cat ~/.jeryu/api.token)` return 401.

---

### Phase K — Jankurai sweep (backfill)

Several small MRs, sequenced after Codex's CI series is fully stable:
- `cleanup/ci-secret-echoes` — remove the 14 secret-echo violations in `.github/workflows/{post-merge-deploy,rust,web}.yml` and `.gitlab-ci.yml`. Score impact: +6.
- `refactor/access-split` — split `src/access.rs` (1183 LOC) into `src/access/{mod,policy,grants,relay,mirror}.rs`. Score impact: shape +20.
- `cleanup/dedup-db-state-accessors` — extract the 12 near-identical accessor arms in `db/state.rs` (around lines 1871, 1970, 2005, ...) into a macro. Score impact: +2.
- `cleanup/dead-code-markers` — wire or delete the 21 `#[allow(dead_code)]` stubs. Score impact: +8.

Each MR < 200 LOC of churn. Sequence to drive `just score` from 64 → 85+.

---

## Coordination protocol with Codex

- **`~/jeryu/AGENT_CHAT.md`** is conversation. Sign every entry, ask questions, propose changes. No big-ticket work items here.
- **`~/jeryu/AGENT_WORK.md`** is the durable shared backlog. Every claimed work item gets a row (branch, scope, status, ETA). Other agents can grab unclaimed items or critique the workload. Updated before each MR opens, after each MR merges.
- **TUI_RESET_PLAN_FINAL.md §0.1** (`/home/ubuntu/jeryu/TUI_RESET_PLAN_FINAL.md:30-76`) remains the source of truth on TUI unit ownership. Phases B/C reuse that table.
- **Stay-out list** (Codex's stable surface, do not edit without an explicit Codex hand-off): `tools/security-lane.sh`, `scripts/install-security-tools.sh`, `scripts/ci-parity.sh`, `scripts/ci/security-lane-native-smoke.sh`, security-lane jobs in `.gitlab-ci.yml` (`jankurai_security`, `jankurai_audit`, `jankurai_proof`, `jankurai_tools`, `jankurai_bad_behavior`, `jankurai_sbom`), and the security jobs in `.github/workflows/jankurai.yml` / `.github/workflows/rust.yml`. We can add *new* jobs alongside without touching these.

---

## Verification (end-to-end smoke after each phase)

| Phase | Smoke |
|---|---|
| A | New runner container has `LogConfig.max-size = 50m`; an SSH-down node drains its managers within 30 min; `df -h /` stable after 24h soak. |
| B | `jeryu tui` renders Mission lens within 5s; `gu` navigates to runners lens (placeholder); `cargo nextest run -p jeryu --lib --test tuiwright` ≥ baseline. |
| C | `jeryu tui` → `gu` shows 4-node grid with live telemetry; press `d` shows drain preview; fixtures cover unreachable/disk-full scenarios. |
| D | `docker pull <registry>/root/jeryu/ci-base:1.95.0` works; jeryu's own pipeline green with `JERYU_USE_CI_BASE=1`; >20% wall-clock improvement vs baseline. |
| E | `jeryu repo quickstart && jeryu repo standard apply` in a fresh dir produces a fully working jeryu repo; `verify` reports no drift. |
| F | Inject a fake disk-full probe on xbabe1; observe managers shift to xbabe0/xbabe3 in one health cycle. |
| G | After merge to main, `jeryu install latest --verify` installs the just-built bin and `jeryu --version` shows the new SHA. |
| H | `git push --tags` triggers a mirror push that lands on the configured external bare repo within 30s. |
| I | `jeryu ci diagnose --job <recent-failed-job>` returns a structured category + suggested action; `jeryu ci heal --apply` re-triggers and `jeryu ci tail` streams the new run. |
| J | `jeryu key init && jeryu serve &; curl …/api/v1/read-model | jq .` returns 200; unauthenticated mutating POST returns 401. |
| K | `just score` increases monotonically with each MR; no regressions in tests. |

---

## Critical files (touch list, by phase)

Phase A:
- `/home/ubuntu/jeryu/src/runner_backend_remote.rs` (docker run flags)
- `/home/ubuntu/jeryu/src/runner_backend_local.rs` (docker run flags)
- `/home/ubuntu/jeryu/src/pool_scale.rs` (unreachable-force-drain)
- `/home/ubuntu/jeryu/src/engine_background_health_pressure.rs` (build-dir TTL)

Phase C (the runners lens):
- `/home/ubuntu/jeryu/src/api/dashboards/runners.rs`
- `/home/ubuntu/jeryu/src/health.rs`
- `/home/ubuntu/jeryu/src/runner_backend_remote_support.rs`
- new `/home/ubuntu/jeryu/src/inspection/runners.rs`
- `/home/ubuntu/jeryu/src/inspection/router.rs`
- `/home/ubuntu/jeryu/src/tui/lenses/runners/{view,data,nav}.rs`
- new `/home/ubuntu/jeryu/src/tui/testing/fixtures/runners.rs`
- `/home/ubuntu/jeryu/tests/tuiwright/lenses_runners.rs`

Phase D:
- new `/home/ubuntu/jeryu/docker/ci-base/{Dockerfile,build.sh}`
- `/home/ubuntu/jeryu/.gitlab-ci.yml` (new `ci_base_publish` job, default image var)

Phase E:
- new `/home/ubuntu/jeryu/templates/repo-standard/`
- new `/home/ubuntu/jeryu/src/repo_standard/{mod,apply,verify,quickstart}.rs`
- `/home/ubuntu/jeryu/src/cli_defs_commands_repo.rs`

Reusable existing functions to call (don't reimplement):
- `src/runner_backend_remote.rs::list_managed_containers` — runner inventory dedupe.
- `src/runner_backend_remote_support.rs::probe_node` — SSH probe (extend, don't replace).
- `src/git/mirror.rs::mirror_push` and `parse_push_mirror_plan` — already exist, currently dormant.
- `src/inspection/serve.rs::serve_inspection` — axum mount for the API.
- `src/tui/widgets/{virtual_table,freshness_chip,status_strip,heatmap}` — for runners lens body.
- `src/api/inspection.rs::InspectionEnvelope` — wire shape for `/api/v1/runners`.

---

## Risk register

| Risk | Mitigation |
|---|---|
| TUI reset rebase needs more than the predicted clean merge | If conflicts surface, resolve in favor of reset branch (it has the new scaffold); abort and ask user if scope balloons. |
| Codex pushes more CI changes mid-flight | AGENT_WORK.md handshake before each phase; stay out of the listed Codex surface. |
| Force-drain duplicates runners on SSH flap | Container labels (`jeryu.managed=true`) make `list_managed_containers` dedupe automatic; default grace = 30 min (6 × 5-min cycles) tunable via env. |
| Baked image breaks for a project that needed a different toolchain | Opt-in flag `JERYU_USE_CI_BASE=1` for the first MR; only flip the default after jeryu's own pipeline is green for a week. |
| Mirror push leaks credentials to external remotes | SSH-only mirrors per AGENTS.md access contract; config validation rejects HTTPS+token. |
| `jeryu ci heal --apply` mis-classifies and re-triggers wrong job | Default to dry-run (no `--apply`); classifier is conservative (errs on `Unknown`); each apply records to a heal_log table for review. |
| Untagged-runner-policy regression | `scripts/ci/no-runner-tags.sh` is already enforced by the `ci_runner_policy` job (`.gitlab-ci.yml:42-46`); any template change runs through that gate. |

---

## Anti-goals

- Do not redesign `src/runner_scheduler.rs` (Codex's territory).
- Do not introduce a new HTTP framework — reuse axum via `serve_inspection`.
- Do not edit other repos directly from this plan (`~/redlineDB` etc.) — templates land in jeryu, repos adopt opt-in.
- Do not add cosign/SLSA infrastructure — SHA256 + Package Registry is enough for the relaxed threat model.
- Do not break the untagged-runner-policy.
- Do not introduce new oversized files; respect lens/template caps in TUI_RESET_PLAN_FINAL.md §7.

---

## Appendix A — `~/jeryu/AGENT_WORK.md` content (to be written immediately after ExitPlanMode)

```markdown
# Agent Work Backlog

Coordination protocol:
- Claim items by setting `owner` and `status: in-progress`.
- Mark `status: blocked` with a reason if you need help / coordination.
- Mark `status: done` with the merged commit SHA.
- Chat via AGENT_CHAT.md; this file is for durable work claims.

## Active workstreams

| ID | Owner | Branch | Phase | Scope | Status | ETA | Notes |
|---|---|---|---|---|---|---|---|
| HARDEN-A | claude | `fix/runner-bounded-runtime` | A | bounded docker logs + mem/cpu/fd caps; unreachable-node force-drain (TTL=6 cycles); build-dir TTL sweep | in-progress | 1d | files: `src/runner_backend_{remote,local}.rs`, `src/pool_scale.rs`, `src/engine_background_health_pressure.rs` |
| TUI-MERGE-B | claude | `merge/tui-reset-to-main` | B | rebase recovery/phase0-tui (28 commits, 0 conflicts per git merge-tree) onto main | pending | 0.5d | needs Codex confirm whether `codex/tui-reset-integration-20260526` or `recovery/phase0-tui` is canonical |
| TUI-U22-C | claude | `feat/u22-runners-lens-body` | C | runners lens body: per-node breakdown, virtual_table grid, drain/scale preview, fixtures | blocked on B | 2-3d | files: `src/api/dashboards/runners.rs`, `src/inspection/runners.rs` (new), `src/tui/lenses/runners/*`, `src/tui/testing/fixtures/runners.rs`, `tests/tuiwright/lenses_runners.rs` |
| CI-BASE-D | claude | `feat/ci-base-image` | D | baked `jeryu/ci-base:1.95.0` (rust + nextest + jankurai + node + python + sqlite, no sccache); opt-in via `JERYU_USE_CI_BASE=1` | pending | 2d | files: `docker/ci-base/{Dockerfile,build.sh}` (new), `.gitlab-ci.yml` (new `ci_base_publish` job) — does NOT touch security-lane scripts |
| REPO-STD-E | claude | `feat/repo-quickstart` | E | `jeryu repo quickstart/standard/{apply,verify}`; templates in `templates/repo-standard/` referencing the baked image | blocked on D | 1-2d | files: `templates/repo-standard/` (new), `src/repo_standard/` (new), `src/cli_defs_commands_repo.rs` |
| SMART-PLACEMENT-F | claude | `feat/smart-placement` | F | `src/pool_placement.rs::node_score` called from `pool_scale.rs::reconcile_manager_runtime_state`; respects xbabe2 reserved | pending | 2d | files: `src/pool_placement.rs` (new), `src/pool_scale.rs` (10-line edit). Does NOT touch `runner_scheduler.rs` (Codex). |
| INSTALL-PULL-G | claude | `feat/install-pull-latest` | G | `jeryu install latest --verify`: SHA256 + GitLab Package Registry; `jeryu install rollback` | pending | 1d | files: `ops/ci/post-merge-deploy-lane.sh` (extend), `src/cli_defs_install.rs`, `src/install/pull_latest.rs` (new) |
| MIRROR-PUSH-H | claude | `feat/post-merge-mirror-push` | H | consume dormant `[offline_release_mirror]`; new `mirror_jobs` table + `engine_background_remote_mirror.rs` | pending | 2d | files: `src/git/mirror_jobs.rs` (new), `src/engine_background_remote_mirror.rs` (new), 5-line hook in `src/git_host/gitlab_merge.rs`, DB migration |
| CI-HEAL-I | claude | `feat/ci-heal` | I | `jeryu ci {tail,diagnose,heal --apply}`; classifier seeded from per-repo audit | blocked on D | 2-3d | files: `src/cli_defs_ci.rs` (new), `src/ci_heal/` (new). Categories: SccacheHang, OverlayfsFalsePositive, ImagePullFail, OOM, DepChange, TestFail. |
| API-SERVE-J | claude | `feat/jeryu-serve-api` | J | `jeryu serve` formalized; `jeryu key {init,show,rotate}`; Bearer middleware on mutating routes only | pending | 1-2d | files: `src/serve/` (new), `src/cli_defs_key.rs` (new), `src/inspection/router.rs` (~15-line middleware) |

## Backlog (jankurai sweep, lower priority)

| ID | Owner | Branch | Scope | Status |
|---|---|---|---|---|
| JANK-K1 | unclaimed | `cleanup/ci-secret-echoes` | Remove 14 secret-echo violations in `.github/workflows/{post-merge-deploy,rust,web}.yml` and `.gitlab-ci.yml`. ⚠️ ONLY after Codex's CI series is stable. | pending |
| JANK-K2 | unclaimed | `refactor/access-split` | Split `src/access.rs` (1183 LOC) into `access/{mod,policy,grants,relay,mirror}.rs` | pending |
| JANK-K3 | unclaimed | `cleanup/dedup-db-state-accessors` | Macro-extract 12 near-identical accessor arms in `db/state.rs` | pending |
| JANK-K4 | unclaimed | `cleanup/dead-code-markers` | Wire or delete 21 `#[allow(dead_code)]` stubs | pending |

## Stay-out list (Codex's stable surface; do not edit without explicit hand-off)

- `tools/security-lane.sh`
- `scripts/install-security-tools.sh`
- `scripts/ci-parity.sh`
- `scripts/ci/security-lane-native-smoke.sh`
- Security-lane jobs in `.gitlab-ci.yml`: `jankurai_security`, `jankurai_audit`, `jankurai_proof`, `jankurai_tools`, `jankurai_bad_behavior`, `jankurai_sbom`
- Security jobs in `.github/workflows/jankurai.yml` and `.github/workflows/rust.yml`
- `src/runner_scheduler.rs` (Codex's claim TUI-RESET-20260526-031)
```

---

## Appendix C — `~/jeryu/AGENT_LIVE.log` format and seed

**File**: `~/jeryu/AGENT_LIVE.log` (created on plan approval; both agents append).

**Tail it**: `tail -f ~/jeryu/AGENT_LIVE.log` shows colored stream live in any terminal that supports 256-color (every modern one).

**Color scheme** (ANSI 256-color, bold):
- claude  → `\033[1;38;5;205m` (bright hot pink) … `\033[0m`
- codex   → `\033[1;38;5;46m` (bright green) … `\033[0m`
- system  → `\033[1;38;5;245m` (grey) … `\033[0m` (for separator lines, plan deltas, etc.)

**Line format** (one logical message per line; for multi-line content, prefix every line with the color escape so `tail` doesn't lose color on wrap):
```
\033[1;38;5;205m[YYYY-MM-DDTHH:MM:SSZ] claude › <message>\033[0m
\033[1;38;5;46m[YYYY-MM-DDTHH:MM:SSZ] codex › <message>\033[0m
\033[1;38;5;245m──────── <separator> ────────\033[0m
```

**Seed content** (written verbatim on plan approval):
```
\033[1;38;5;245m═══════════════════════════════════════════════════════════════════════\033[0m
\033[1;38;5;245m  jeryu agent live coordination log — tail -f to follow\033[0m
\033[1;38;5;245m  pink = claude · green = codex · grey = system\033[0m
\033[1;38;5;245m  durable backlog: ~/jeryu/AGENT_WORK.md\033[0m
\033[1;38;5;245m  long-form chat: ~/jeryu/AGENT_CHAT.md\033[0m
\033[1;38;5;245m═══════════════════════════════════════════════════════════════════════\033[0m
\033[1;38;5;205m[2026-05-29T<HH:MM:SS>Z] claude › hello codex — live log online. backlog seeded in AGENT_WORK.md (10 active items + 4 jankurai backfills). starting HARDEN-A (bounded docker runtime). pinging you before TUI-MERGE-B to confirm `recovery/phase0-tui` is the canonical reset branch vs `codex/tui-reset-integration-20260526`. respect Codex stay-out list per AGENT_WORK.md.\033[0m
```

**Operational rules**:
- Never edit prior lines (would break `tail -f`); only append.
- Replace `<HH:MM:SS>` with the actual UTC timestamp at write time.
- For long messages, prefer one line. If wrapping is unavoidable, every wrapped continuation starts a new line and re-applies the color escape.
- A daily separator (`\033[1;38;5;245m── 2026-MM-DD ──\033[0m`) is fine; not required.

---

## Appendix B — `~/jeryu/AGENT_CHAT.md` kickoff note (to be appended immediately after ExitPlanMode)

```markdown
---

Hi Codex — claude here.

Picked up the user's master hardening ask. Full backlog in **AGENT_WORK.md**, live coordination chat in **AGENT_LIVE.log** (ANSI-colored, tail -f friendly: my text is bright pink, yours is bright green — please use `\033[1;38;5;46m…\033[0m` when you post). I'll ping you there before every phase boundary.

Immediate planned actions (in order):
1. HARDEN-A: bounded docker logs + mem/fd/cpu caps on runner containers, unreachable-node force-drain, build-dir TTL. (~1 day, branch `fix/runner-bounded-runtime`.) Will not touch your security-lane surface.
2. TUI-MERGE-B: I'd like to merge `recovery/phase0-tui` into `main`. Question: is that the canonical reset integration branch, or do you want me to use `codex/tui-reset-integration-20260526` instead? Per TUI_RESET_PLAN_FINAL.md §0.1, both exist. `git merge-tree` reports zero conflicts on `recovery/phase0-tui...main`; intent is a clean rebase merge with no semantic edits. **Please confirm before EOD or I'll default to `recovery/phase0-tui`.**
3. TUI-U22-C: runners lens body (multi-node health pane) after B lands.
4. CI-BASE-D in parallel with C.

What I will NOT touch:
- `tools/security-lane.sh`, `scripts/install-security-tools.sh`, `scripts/ci-parity.sh`, `scripts/ci/security-lane-native-smoke.sh`
- Security-lane jobs in `.gitlab-ci.yml` and `.github/workflows/{jankurai,rust}.yml`
- `src/runner_scheduler.rs` (your TUI-RESET-20260526-031 territory)

Ping me here if any of my claimed scopes step on yours. I'll update AGENT_WORK.md before each MR opens and after each merges.

— claude
```
