# JERYU TUI Design and Functionality

This document describes the current `jeryu tui` implementation in `/home/ubuntu/JeRyu` as of April 27, 2026. It is intentionally technical: it is meant to give outside engineers and AI advisors enough context to reason about the terminal UI, the data model, the refresh loop, release visibility, live log behavior, screenshot capture, and current implementation limits without needing to reverse engineer the Rust code first.

## Quick Start

The TUI is launched through the normal `jeryu` CLI:

```bash
cargo run -p jeryu -- tui
```

There is also a smoke-render mode used by tests and CI:

```bash
cargo run -p jeryu -- tui --once
```

The smoke mode renders one frame with Ratatui's `TestBackend` and exits. It is intended to prove that the TUI can render even with empty or partial state.

Publication and review screenshots can be generated without an interactive terminal:

```bash
cargo run -p jeryu -- tui --capture --tab jobs --output paper/assets/jeryu-tui-jobs-flow.png
cargo run -p jeryu -- tui --capture --tab tests --output paper/assets/jeryu-tui-tests-vti.png
```

`--capture` accepts `workflow`, `mission`, `release`, `approvals`, `jobs`, `agents`, `tests`, `pools`, `cache`, `evidence`, `llms`, `secrets`, and `git`. The capture path renders the same Ratatui layout through `TestBackend` and writes a PNG.

## Source Map

The active TUI implementation is split across these files:

| File | Responsibility |
| --- | --- |
| `src/tui/mod.rs` | Terminal setup, raw mode, alternate screen, event loop, keybindings, `--once` smoke render, deterministic `--capture` PNG render. |
| `src/tui/app.rs` | Stateful application model, background refresh workers, live log polling, selection state, actions. |
| `src/tui/ui.rs` | Ratatui rendering: tabs, release banner, flow board, live jobs list, logs, storage, cache, tests. |
| `src/tui/flow/model.rs` | Flow board data model: snapshots, pipelines, graph columns, graph nodes, lanes, ETA records. |
| `src/tui/flow/collector.rs` | Background collector that builds `FlowSnapshot` objects from DB and GitLab. |
| `src/tui/flow/builder.rs` | Converts job events into a lane/column graph. |
| `src/tui/flow/widget.rs` | Low-level board widget that paints columns and nodes into the terminal buffer. |
| `src/tui/flow/eta.rs` | Simple lane-based ETA estimator. |
| `src/tui/flow/inspector.rs` | Job inspector renderer. This exists, but is not in the active main Flow layout right now. |

The CLI definitions live in `src/cli.rs`, and the command dispatcher calls `jeryu::tui::run_tui(...)`, `jeryu::tui::run_tui_once(...)`, or `jeryu::tui::capture_tui_png(...)` from `src/dispatch.rs`.

## High-Level Design

The TUI is a Ratatui/crossterm application. It uses crossterm raw mode and the terminal alternate screen. The main loop draws a frame, polls for input every 250ms, handles keys, then calls `app.tick().await` to drain background state channels.

The application state is owned by `App` in `src/tui/app.rs`. `App` is not just view state: it also owns handles to the database, Docker controller, and GitLab client so it can run operational actions such as retrying jobs, deleting local records, and pausing/resuming pools.

The TUI has thirteen top-level tabs:

1. `Workflow`
2. `Mission`
3. `Release`
4. `Approvals`
5. `Jobs`
6. `Agents`
7. `Tests`
8. `Pools`
9. `Cache`
10. `Evidence`
11. `Secrets`
12. `LLMs`
13. `Git`

The default tab is `Mission`. The default active pane is `Jobs`, which keeps
job/log keyboard behavior predictable when the operator switches to the Jobs
tab.

The current Mission screen is the landing surface. It is designed as an
action-first cockpit rather than a passive status dashboard:

```text
+------------------------------------------------------------+
| header: online/offline, containers, mirror, release, tabs   |
+------------------------------------------------------------+
| Top Signal / next action             | Readiness            |
+------------------------------------------------------------+
| Autonomy | Active Work | Release | Cache Trust              |
+------------------------------------------------------------+
| Attention Queue        | Proof Stack      | Next Actions     |
+------------------------------------------------------------+
| footer help text                                            |
+------------------------------------------------------------+
```

The Jobs screen remains the main CI/release flow screen. It is a vertical
layout:

```text
+------------------------------------------------------------+
| header: online/offline, containers, mirror, release, tabs   |
+------------------------------------------------------------+
| Release Watch                                              |
+------------------------------------------------------------+
| Flow Board                                                 |
+------------------------------------------------------------+
| Live Jobs list                         | Log Preview       |
+------------------------------------------------------------+
| footer help text                                            |
+------------------------------------------------------------+
```

The older pool and pipeline list renderers still exist in `src/tui/ui.rs` as
`draw_pools()` and `draw_pipelines()`, but they are currently marked
`#[allow(dead_code)]` and are not part of the active Jobs/Flow layout. The
current active list pane is the bottom-left `Live Jobs` pane.

## Terminal Lifecycle

`run_tui()` does this:

1. Enables raw mode.
2. Enters the alternate screen.
3. Enables mouse capture.
4. Constructs `App`.
5. Runs `hydrate_smoke_state()` for initial DB/GitLab state.
6. Starts background sync workers.
7. Runs the 250ms draw/input/tick loop.
8. On exit, disables raw mode, leaves alternate screen, disables mouse capture, and restores the cursor.

Important code shape:

```rust
let tick_rate = Duration::from_millis(250);

loop {
    terminal.draw(|f| ui::draw(f, app))?;

    if crossterm::event::poll(tick_rate)?
        && let Event::Key(key) = event::read()?
    {
        // handle key
    }

    app.tick().await;
}
```

## Keybindings

Global and normal-mode keys:

| Key | Behavior |
| --- | --- |
| `q` | Quit the TUI. |
| `Esc` | Quit, unless maximized log view is open; then it closes/maximizes back down. |
| `Tab` | Cycle active pane: Pools -> Pipelines -> Jobs -> Pools. |
| `Left` | Cycle active pane backward. |
| `Right` | Cycle active pane forward. |
| `Up` | Move selection up in the active pane or selected test list. Wraps at top. |
| `Down` | Move selection down in the active pane or selected test list. Wraps at bottom. |
| `Enter` on Jobs pane | Open maximized log view for the selected job. |
| `Enter` on Tests tab | Load history for the selected test. |
| `d` or `Delete` | Delete selected pipeline/job record from local DB, depending on active pane. |
| `r` | Retry selected failed job when active pane is Jobs. |
| `p` | Pause/resume selected pool when active pane is Pools. |
| `1` | Switch to Mission tab. |
| `2` | Switch to Release tab. |
| `3` | Switch to Jobs tab. |
| `4` | Switch to Agents tab. |
| `5` | Switch to Tests tab. |
| `v` or `t` on Tests tab | Toggle test bottleneck mode: Average vs Latest. |

Maximized log view keys:

| Key | Behavior |
| --- | --- |
| `Esc` | Close maximized log view. |
| `Up` | Scroll logs up by one line and disable follow-tail mode. |
| `Down` | Scroll logs down by one line and disable follow-tail mode. |
| `PageUp` | Scroll logs up by 20 lines. |
| `PageDown` or `Space` | Scroll logs down by 20 lines. |
| `Home` | Jump to top of logs and disable follow-tail mode. |
| `G` or `End` | Jump back to bottom/latest and re-enable follow-tail mode. |
| `q` | Quit. |

## State Model

The central state is `TuiStateSnapshot` in `src/tui/app.rs`. It contains:

```rust
pub struct TuiStateSnapshot {
    pub pools: Vec<Pool>,
    pub gitlab_ready: bool,
    pub active_containers: usize,
    pub recent_jobs: Vec<JobEvent>,
    pub pipelines: Vec<PipelineMetrics>,
    pub flow: crate::tui::flow::FlowSnapshot,
    pub live_log: LiveLogState,
    pub hot_cache_usage_bytes: i64,
    pub cache_hits: i64,
    pub cache_objects_count: i64,
    pub proxy_healthy: bool,
    pub registry_healthy: bool,
    pub mirror_enabled: bool,
    pub ca_mounted: bool,
    pub singleflight_requests: i64,
    pub hit_ratio: f64,
    pub miss_count: i64,
    pub total_requests: i64,
    pub active_taint_count: i64,
    pub detonation_breaches: i64,
    pub cold_execution_downgrades: i64,
    pub cas_disk_bytes: i64,
    pub crate_cache_disk_bytes: i64,
    pub storage_breakdown: StorageBreakdown,
    pub pipeline_eta: Option<String>,
    pub pipeline_progress: u16,
    pub release_status: Option<release::ReleaseAttemptView>,
    pub release_status_generated_at: Option<String>,
    pub test_bottlenecks_avg: Vec<crate::state::TestBottleneck>,
    pub test_bottlenecks_latest: Vec<crate::state::TestBottleneck>,
}
```

The interactive state held directly by `App` includes:

| Field | Meaning |
| --- | --- |
| `active_tab` | Current top-level tab. |
| `active_pane` | Current focus among Pools, Pipelines, Jobs. |
| `selected_pool_index` | Selection index for pools. |
| `selected_pipeline_index` | Selection index for tracked pipelines. |
| `selected_job_index` | Selection index for the live/recent job list. |
| `selected_job_id` | Stable selected job identity used to survive list refresh/reordering. |
| `maximize_logs` | Whether the TUI is showing the full-screen log view. |
| `log_scroll_offset` | Vertical scroll position for logs. |
| `follow_log_tail` | Whether logs auto-scroll to the bottom/latest output. |
| `test_view_mode` | Average or Latest bottleneck mode. |
| `selected_test_index` | Current selected test bottleneck. |
| `selected_test_history` | Loaded drill-down history for the selected test. |
| `log_target` | The GitLab job currently being tailed. |

## Background Workers and Refresh Cadence

`App::start_background_sync()` starts three asynchronous workers:

1. Flow collector worker.
2. Live log polling worker.
3. General snapshot sync worker.

The general snapshot worker runs roughly every 1500ms. It refreshes:

- pools from `db.list_pools()`
- managed containers from Docker
- running/pending/created GitLab jobs for the default release project
- recent local DB job events
- tracked pipeline metrics
- release status report for `project_id=2`, `ref_name=main`
- cache proxy and registry health
- Docker mirror and CA mount signals
- cache metrics and taint/verdict counters
- CAS and crate cache disk usage
- test bottleneck lists
- root filesystem usage from `df -k /`
- runner data and DB file sizes

The flow collector also ticks about every 1500ms and emits a `FlowSnapshot`.

The live log worker ticks every 650ms. It reads the current selected log target from a `watch` channel and calls GitLab `job_trace(project_id, job_id)`.

Current live logs are polling-based. There is no websocket transport in the current TUI implementation.

## Snapshot Merge Semantics

The TUI has multiple background data streams. `App::tick()` drains channels in this order:

1. General snapshot channel.
2. Flow snapshot channel.
3. Live log channel.

When a general snapshot arrives, the app preserves the existing `flow` and `live_log` state before replacing the rest of the snapshot:

```rust
while let Ok(mut state) = self.sync_rx.try_recv() {
    state.flow = self.state.flow.clone();
    state.live_log = self.state.live_log.clone();
    self.state = state;
}
```

This prevents the slower general refresh loop from wiping the more frequently updated flow board or log pane.

Flow snapshots go through `apply_flow_snapshot()`. This is important because it prevents the board from blinking back to an empty state when GitLab or the collector briefly returns no flow data:

```rust
fn apply_flow_snapshot(&mut self, mut flow_snap: crate::tui::flow::FlowSnapshot) {
    if flow_snap.active_pipelines.is_empty() && !self.state.flow.active_pipelines.is_empty() {
        flow_snap.active_pipelines = self.state.flow.active_pipelines.clone();
        flow_snap.stale = true;
        flow_snap.last_non_empty_at = self
            .state
            .flow
            .last_non_empty_at
            .or(Some(self.state.flow.generated_at));
        flow_snap.selected_pipeline_id = self.state.flow.selected_pipeline_id;
    } else if flow_snap.active_pipelines.is_empty()
        && let Some(fallback) = self.flow_from_recent_jobs(flow_snap.generated_at)
    {
        flow_snap.active_pipelines = vec![fallback];
        flow_snap.stale = true;
        flow_snap.last_non_empty_at = Some(flow_snap.generated_at);
    } else if !flow_snap.active_pipelines.is_empty() {
        flow_snap.last_non_empty_at =
            flow_snap.last_non_empty_at.or(Some(flow_snap.generated_at));
    }

    self.state.flow = flow_snap;
}
```

There are two anti-blanking behaviors:

1. If a new flow snapshot is empty but the UI already has visible pipelines, retain the visible pipelines and mark the board stale.
2. If there is no visible flow yet but recent jobs exist, synthesize a fallback `PipelineFlow` from recent job events.

This design intentionally favors a stable board with a stale marker over a board that oscillates between real data and empty state.

## Header

The header is always visible, including maximized log mode. It shows:

- Product title: `jeryu Mission Control`
- GitLab connectivity: `ONLINE` or `OFF/BOOTING`
- Active container count
- Jeryu sync summary when available
- Upstream remote display and gap when available
- Release identity: ref, version, and canary state when available
- Tab selector labels

The upstream URL display strips credentials by showing only the substring after `@` when present.

The tab labels are:

```text
0: Workflow  1: Mission  2: Release  3: Approvals  4: Jobs  5: Agents  6: Tests  7: Pools  8: Cache  9: Evidence  Secrets  LLMs  Git
```

The active tab is cyan and bold.

## Delivery Tab (default — `0`)

The Delivery tab (also reached as `Workflow`) is the CI Production Manager's
mission control. It renders every active pull request flowing through the
canonical pipeline end-to-end so a single screen answers "what is shipping,
what is blocked, and what can I roll back?".

### Canonical pipeline

Per PR, the DAG always covers the same ten phases, even when one or more is
still `Waiting`:

```
Pre-merge CI → Agent review (pre, stub) → Auto-merge to main
            → Post-merge CI → Agent review (post, stub)
            → Build immutable artifact
            → Promote local → Promote dev (canary) → Promote prod
            → Monitor / rollback
```

Two policy stubs ship in this round and will be replaced as JeRyu's agent
layer lands:

- **Agent review** — auto-passes ~5s after upstream CI is green; blocks on
  upstream errors. See `src/tui/workflow/delivery.rs`.
- **Auto-merge** — passes whenever pre-merge CI + the pre-merge agent
  review are both green (the stated policy: PRs auto-merge when pre-merge
  CI passes).

### Region layout (≥ 160 cols)

```
┌─ Mission strip (3 rows) ─ identity + ship % + blocker + critical path ─┐
├─ PR rail (3 rows) ─ [✗ #1842 …] [● #1841 …] [✎ #1839 …] [✓ #1837 …] … ─┤
│ Phase rail │  ────────── DAG canvas (selected PR) ─────────  │ Minimap │
│ PreCI  ●   │                                                  │   ██    │
│ Agent▲ ✓   │                                                  │   ░░    │
│ Merge  ⛔  │                                                  │   ░░    │
│ ...        │                                                  │   ...   │
├─ Footer (1 row) ─ keybinds + last-action feedback ──────────────────────┤
```

Below ~160 cols the minimap collapses; below ~120 cols the phase rail
collapses; below ~80 cols the PR rail collapses. Region math lives in
`src/tui/workflow/regions.rs` (with no-overlap tests).

### Mission strip

Two lines that always answer the canonical PM questions:

1. Selected PR identity — `[status #N title]  by author  ·  STATUS  ·  at PHASE  ·  ship X%`, followed by `blocker: NAME (blocks K)` in red and `crit: TAIL (~Ts)` in amber when work remains.
2. Fleet rollup — `OPEN N · RUN N · BLOCK N · MERGED N · READY N` plus `CANARY ◉` / `PROD ◉` beacons and a canary URL when a deployment is live.

### Side-pane inspector

`Enter` toggles an inspector pane on the right (or the legacy modal at
narrow widths). Five sub-tabs cycle with `Tab` / `Shift+Tab`:

- **Overview** — status, kind, command, progress, ETA, duration, VTI,
  cache verdict, reason, tags.
- **Logs** — live tail from `LiveLogState` (last N lines that fit).
- **Deps** — incoming + outgoing dependency lists with status glyphs.
- **Evidence** — capsule + backend reference (capsule wiring is stubbed).
- **Actions** — context-sensitive: `[Rerun]`, `[Rollback]` (only on
  Promote{dev|prod}), `[View prompt]` (agent review nodes), `[Open in
  GitLab]`, `[View capsule]`. The last-action feedback line lands here.

### Intelligence

`src/tui/workflow/intelligence.rs` provides:

- `compute_first_blocker(snap)` — earliest Error/Blocked node.
- `compute_critical_path(snap)` — longest-ETA path through remaining work.
- `compute_downstream_impact(snap, id)` — transitive child count.
- `detect_stalls(snap, now)` — running nodes past `eta*1.5` (90s floor).
- `compute_ship_readiness(snap)` — % of canonical phases fully terminal-pass.

Stalled running nodes get an amber pulsing border and a `[STALL]` marker.
Failed/blocked cards gain a `⚠ blocks K · reason` chip on the badge line.

### Visual polish

- Block-character progress bar: `███░░░░░ 41%` on running nodes.
- Kind accents on the title row: 🤖 (agent review), ⇲ (auto-merge),
  📦 (build artifact), 🚀 (promote), 📈 (monitor).
- `[ROLLBACK]` amber chip on rollback-eligible Promote{dev|prod} nodes
  that are Running or Ran.
- Selection: `[SEL]` marker + bright border. Critical path: `[CRIT]`.
- Zoom modes cycled with `z`: Overview (status + title only),
  Cards (default), Dense (no command line).

### Keymap

```
↑↓←→ / hjkl  select node within / across phases
Tab          next node (or next inspector sub-tab when inspector is open)
Shift+Tab    previous inspector sub-tab
Enter        toggle inspector pane (side pane ≥140 cols; modal otherwise)
PgUp/PgDn    pan viewport ½-screen vertically
Space        pan down ½-screen
[  /  ]      pan viewport ½-screen horizontally
Home / End   jump viewport top / bottom
f            toggle follow-active mode (auto-pan to first running node)
b            jump selection to first blocker
c            jump to critical-path tail
z            cycle zoom (Overview → Cards → Dense)
< / >        cycle pull request (previous / next)
r            trigger rollback (only fires on Promote{dev|prod})
?            help overlay
```

### Mouse

Capture is on by default (`runner.rs` enables `EnableMouseCapture`). The
Delivery tab routes events through `src/tui/runtime/input/mouse.rs`:

- Wheel inside canvas: pan viewport vertically (Shift+Wheel: horizontal).
- Drag (left-down + move): pan viewport from the drag origin.
- Click on a node card: select; second click on selected → toggle inspector.
- Click on the PR rail: switch PR via `pr_at_column`.
- Click on the minimap: jump cursor via `locate_minimap_click`.

Hit boxes are stored on `app.delivery_hit_map` (populated by the renderer
each frame via `draw_dag_canvas_with_hits`).

### Source files

| File | Responsibility |
| --- | --- |
| `src/tui/workflow/model.rs` | Snapshot types: WorkflowNode, CanonicalPhase, PullRequestView, DeliverySnapshot, FleetSummary, PrStatus. |
| `src/tui/workflow/delivery.rs` | `collect_delivery_snapshot` + `build_demo_delivery` (5-PR story) + canonical-pipeline builder + agent-review / auto-merge stubs. |
| `src/tui/workflow/builder.rs` | Topological phase assignment + reusable `build_snapshot`. |
| `src/tui/workflow/intelligence.rs` | Blocker, critical-path, downstream, stall, ship-readiness compute functions. |
| `src/tui/workflow/widget.rs` | `draw_delivery_tab` + `draw_dag_canvas_with_hits` + node cards. |
| `src/tui/workflow/regions.rs` | Region layout / collapse rules. |
| `src/tui/workflow/mission_strip.rs` | Sticky two-line banner. |
| `src/tui/workflow/pr_rail.rs` | Horizontal PR chip list + click hit-test. |
| `src/tui/workflow/phase_rail.rs` | Vertical canonical-phase index. |
| `src/tui/workflow/minimap.rs` | Bird's-eye DAG + click hit-test. |
| `src/tui/workflow/inspector.rs` | Side-pane inspector + 5 sub-tabs. |
| `src/tui/workflow/nav.rs` | Spatial navigation + viewport + zoom + persistent selection. |
| `src/tui/workflow/hit_map.rs` | Renderer → mouse handoff. |
| `src/tui/runtime/input/mouse.rs` | Wheel/drag/click dispatch. |

## Mission Tab

The Mission tab (now `1:` after Delivery) is an operational summary surface.
It condenses the state model into an actionable view:

- `Top Signal`: the highest-priority blocker or green-path status.
- `Next Action`: the safest immediate operator action inferred from current
  taints, release state, failed jobs, running jobs, and GitLab readiness.
- `Readiness`: GitLab, release, cache, and evidence posture.
- `Metric tiles`: autonomy, active work, release progress, and cache trust.
- `Attention Queue`: failed jobs, active taints, release blockers, and stale
  data.
- `Proof Stack`: VTI, CI, cache trust, evidence, merge gate, and release gate
  states with compact color badges.
- `Next Actions`: command-palette-oriented actions plus a compact recent-work
  sparkline.

The design goal is that an operator or supervising agent can answer these
questions from the first screen:

```text
Can we ship?
What is blocking us?
Which proof is missing?
Which system should I inspect next?
What action is safest right now?
```

## Jobs / Flow Tab

The Jobs tab is the main CI/release flow screen. It combines release deployment
status, CI/release flow graph, live job list, and live log preview.

### Release Watch Pane

The Release Watch pane is the top pane on the Flow tab. It renders the latest `ReleaseAttemptView` from `release::build_release_status_report(...)`.

When a release exists, it shows:

| Field | Source |
| --- | --- |
| Version | `release.attempt.version` |
| State | `release.canary_state` |
| Eligibility | `release.eligibility` |
| Upstream | `release.attempt.upstream_status` |
| Prod | `release.attempt.production_pipeline_status` and `production_pipeline_id` |
| Evidence | `release.gate_canary_e2e_path` |
| Note | `release.attempt.canary_note` |

Rendered body shape:

```text
Version:   ci-fa51a52a7882
State:     running (waiting)
Upstream:  success
Prod:      not-triggered None
Evidence:  /path/to/gate-canary-e2e.json
Note:      (none)
```

When no release attempts exist, it says that no release attempts exist yet and that the TUI is waiting for the first green main pipeline.

Release color is derived from `canary_state`:

| State | Color |
| --- | --- |
| `green`, `released` | Green |
| `in-flight`, `canary-authorized` | Cyan |
| `waiting`, `ready-for-canary` | Yellow |
| `blocked`, `blocked-by-upstream` | Magenta |
| `failed` | Red |
| anything else | Dark gray |

The release view is designed to answer: "Where are we on canary and production deployment?"

### Flow Board Pane

The Flow Board pane is the central visual graph. It renders the first active pipeline in `app.state.flow.active_pipelines`.

The model is:

```rust
pub struct FlowSnapshot {
    pub generated_at: DateTime<Utc>,
    pub gitlab_online: bool,
    pub active_pipelines: Vec<PipelineFlow>,
    pub stale: bool,
    pub last_non_empty_at: Option<DateTime<Utc>>,
    pub selected_pipeline_id: Option<i64>,
    pub release: Option<crate::release::ReleaseAttemptView>,
    pub pools: Vec<crate::state::Pool>,
    pub active_containers: usize,
    pub cache_metrics: CacheMetrics,
}

pub struct PipelineFlow {
    pub pipeline_id: i64,
    pub project_id: i64,
    pub ref_name: String,
    pub sha: Option<String>,
    pub status: String,
    pub graph: FlowGraph,
    pub current_blocker: Option<i64>,
    pub critical_path: Vec<i64>,
    pub eta: Option<EtaEstimate>,
    pub progress_pct: u16,
}
```

If `flow.stale` is true, the board title includes an age:

```text
FLOW BOARD [stale 12s]
```

This means the TUI is intentionally retaining the last non-empty board rather than blanking it while fresh data is unavailable.

If no pipeline is available at all, the board shows:

```text
Waiting for active pipelines...
```

If a pipeline is available but the graph has not populated columns yet, the widget shows:

```text
Waiting for job graph...
```

### Flow Graph Construction

The flow graph is built from `JobEvent` rows and GitLab job records. Each job becomes a `FlowNode`.

Columns are inferred from job names:

| Column | Job-name matching |
| --- | --- |
| Admission | `hook`, `policy`, `admission` |
| Impact | `impact`, `plan` |
| Build | `build`, `compile`, `image` |
| Tests | `test`, `unit`, `integration`, `e2e`, `lint`, `fmt` |
| Security | `security`, `secret`, `honeypot`, `guard` |
| Package | `package`, `publish` |
| Release Gates | `gate`, `telemetry` |
| Canary | `canary` |
| Production | `prod`, `deploy` |
| Other | fallback |

Lanes are also inferred from job names:

| Lane | Job-name matching |
| --- | --- |
| Unit | `unit`, `local`, `lib` |
| Integration | `integration`, `e2e`, `live` |
| Security | `security`, `secret`, `guard` |
| Build | `build`, `compile` |
| Admission | `admission`, `hook` |
| ReleaseExecution | `canary`, `prod`, `deploy`, `gate` |
| Other | fallback |

Graph builder code shape:

```rust
let column = classify_column(&name);
let lane = classify_lane(&name);

let active = job.status == "running" || job.status == "pending" || job.status == "created";
let eta = if active {
    Some(super::eta::estimate_job_eta(&name, lane, elapsed_secs))
} else {
    None
};

let node = FlowNode {
    id: i as i64,
    job_id: Some(job.job_id),
    label: name.clone(),
    column,
    lane,
    status: job.status.clone(),
    progress_pct,
    eta,
    is_required: true,
    is_critical_path,
    backend: Some(backend),
    elapsed_secs,
};
```

The widget paints columns left-to-right. Tests and Security columns show lane group headers. Nodes use status icons:

| Status | Icon | Color |
| --- | --- | --- |
| `success` | check mark | Green |
| `running` | filled circle | Blue |
| `failed` | x mark | Red |
| `pending`, `created` | open circle | Yellow |
| `canceled` | canceled symbol | Dark gray |
| other | diamond | Gray |

When the selected job in the Live Jobs list maps to a graph node, the graph widget highlights the corresponding node.

### Live Jobs Pane

The bottom-left pane on the Flow tab is the active list view. It is titled:

```text
[*] Live Jobs (N)
```

It shows active GitLab jobs plus recent DB job events. Jobs are sorted by status rank first, then timestamp, then job id:

| Status | Rank |
| --- | --- |
| `running` | highest |
| `waiting_for_resource`, `preparing` | high |
| `pending` | medium |
| `created` | low |
| anything else | lowest |

Each row includes:

- selection prefix (`>>` when selected)
- compact status icon text (`RUN`, `WAIT`, `FAIL`, `OK`, etc.)
- raw status
- estimated percentage
- elapsed duration
- pipeline id, if known
- pool/runner/stage label
- job name

Approximate row shape:

```text
>> RUN  [running ]  43%  52s    #530    build      test-rust-nextest-1
```

The selected job id is remembered across refreshes. If the jobs list reorders because a pending job starts running, the same job remains selected when possible:

```rust
if let Some(job_id) = self.selected_job_id
    && let Some(index) = self
        .state
        .recent_jobs
        .iter()
        .position(|job| job.job_id == job_id)
{
    self.selected_job_index = index;
    return;
}
```

This is important for watching logs while jobs are changing state.

### Log Preview Pane

The bottom-right pane on the Flow tab is the log preview. It shows the selected job's current log state when a log target is active. If no log target is active, it prompts the user to focus Jobs or select a job.

The title includes:

- selected job name
- log stream state: `idle`, `live`, `stale`, or `stale: <error>`
- scroll mode: `follow` or `manual`

Example:

```text
Log Preview: test-rust-nextest-1 [live | follow]
```

When logs are present, they are rendered with syntax highlighting. The renderer first tries to preserve ANSI escape sequences via `ansi_to_tui`; if no ANSI is present, it applies plain-text highlighting:

```rust
fn render_log_text(log: &str) -> Text<'_> {
    if log.contains('\x1b') {
        use ansi_to_tui::IntoText;
        if let Ok(text) = log.into_text() {
            return text;
        }
    }
    highlight_plain_log(log)
}
```

Plain log highlighting rules:

| Text pattern | Style |
| --- | --- |
| contains `error`, `failed`, `panic`, `fatal` | red, bold |
| contains `warning` or `warn` | yellow |
| contains `success`, `passed`, ends in ` ok`, contains ` finished ` | green |
| starts with `$` or `+`, contains `cargo ` or `docker ` | cyan |
| starts with `[` or contains `t00:` | gray |
| fallback | white |

Logs auto-scroll to the bottom while `follow_log_tail` is true. Manual scrolling disables follow mode until the user presses `G` or `End`.

## Maximized Log View

Pressing `Enter` while the Jobs pane is active opens the selected job in a maximized log view. This hides the Flow board and list panes and expands the log renderer to the full content area between the header and footer.

Behavior:

- `open_selected_job_log()` sets `active_pane = Jobs`.
- It remembers the selected job.
- It sets `maximize_logs = true`.
- It enables follow-tail mode.
- It sets `log_scroll_offset = u16::MAX`, which is clamped to the current bottom on render.
- It updates the log target, causing the log polling worker to fetch that job trace.

```rust
pub fn open_selected_job_log(&mut self) {
    self.active_pane = ActivePane::Jobs;
    self.remember_selected_job();
    self.maximize_logs = true;
    self.follow_log_tail = true;
    self.log_scroll_offset = u16::MAX;
    self.update_log_target();
}
```

The live log worker keeps only the tail of each trace, capped at `LIVE_LOG_MAX_BYTES`, currently 160,000 bytes. This prevents very large logs from making the TUI unusable.

```rust
const LIVE_LOG_MAX_BYTES: usize = 160_000;
```

The current log transport is not websocket-based. It polls GitLab's job trace endpoint every 650ms for the selected job.

## Pools Tab

The Pools tab has a two-column layout.

Left pane: runner pools.

It displays:

- pool name
- active/paused state
- current selection

Right pane: selected pool detail.

It displays:

- pool name
- active/paused state
- minimum warm managers
- maximum managers
- pause/resume shortcut

Pool pause/resume is an operational action, so the Pools tab is the visible
home for the `p` keybinding and the `pause_pool` command-palette preview.

## Cache Tab

The Cache tab is a four-panel dashboard:

1. Storage Overview
2. Gateway Health
3. Singleflight Analytics
4. Trust & Taint Boundaries

### Storage Overview

Shows:

- total cached objects
- hot cache bandwidth in MB
- exact hits
- total requests
- hit ratio
- misses
- CAS disk usage
- crate cache disk usage

### Gateway Health

Shows:

- Singleflight gateway online/offline
- OCI mirror online/offline
- whether CA certificates appear injected/mounted

Connectivity is currently checked by attempting TCP connections to configured local proxy and registry ports.

### Singleflight Analytics

Shows:

- coalesced cargo tarball fetch count
- estimated bandwidth saved

The saved bandwidth estimate is simple and currently uses `singleflight_requests * 5 MB`.

### Trust & Taint Boundaries

Shows:

- active taint rules
- detonation lane breaches
- downgrades to cold execution
- health text based on whether taints/breaches exist

Counts come from local DB queries against `cache_taints` and `cache_verdicts`.

## Evidence Tab

The Evidence tab has two modes:

- Evidence capsules, focused on failed jobs and failure metadata.
- Audit ledger, toggled with `a`, focused on capability, admission, and action
  events.

The evidence capsule mode has a left capsule list and right detail pane. It is
the current human-readable proof surface for failures. The audit ledger mode is
the current human-readable proof surface for actor/action history.

## Tests Tab

The Tests tab is a two-pane view:

Left pane: bottleneck list.

Right pane: selected test history drill-down.

The left pane can show either:

- Average bottlenecks
- Latest bottlenecks

Press `v` or `t` to toggle. Press `Up`/`Down` to select a test. Press `Enter` to load history for the selected test.

Rows include:

- duration
- execution count
- test name

Color rules:

| Condition | Color |
| --- | --- |
| selected row | cyan background, black text |
| duration > 300s | red |
| duration > 60s | yellow |
| otherwise | green |

The right pane shows historical executions once loaded:

- date
- status
- duration

History status colors:

| Status | Color |
| --- | --- |
| `success`, `passed` | green |
| `failed` | red |
| anything else | yellow |

## Legacy/Inactive Renderers

Several renderers exist but are not currently wired into the active tab layout:

| Renderer | Current status |
| --- | --- |
| `draw_pools()` | Exists; not rendered in active Flow layout. |
| `draw_pipelines()` | Exists; not rendered in active Flow layout. |
| `draw_top()` | Exists; old three-column layout; not active. |
| `draw_bottom()` | Exists; old jobs + pipeline overview layout; not active. |
| `draw_pipeline_overview()` | Exists; not active in current layout. |
| `draw_job_inspector()` | Exists; not active in current layout. |

The active pane state still includes `Pools`, `Pipelines`, and `Jobs`, and keybindings still cycle among those panes. In the current Flow layout, only Jobs is visibly represented as a list pane. Pipeline focus affects the Flow Board border color, but there is no visible standalone Active Pipelines list in the current active layout.

This distinction matters for advisors: some behavior is implemented as state/actions, some as renderers, and some as active visible UI.

## Actions and Side Effects

### Delete

`d` or `Delete` deletes local DB records for the selected item:

- If active pane is `Pipelines`, it deletes the selected pipeline via `db.delete_pipeline(pid)`.
- If active pane is `Jobs`, it deletes the selected job event via `db.delete_job_event(jid)`.

It also removes the item from local in-memory state immediately for a responsive UI.

### Retry

`r` retries the selected failed job, but only when:

- active pane is `Jobs`
- selected job exists
- selected job status is `failed`

It calls:

```rust
self.gitlab.retry_job(j.project_id, j.job_id).await?;
```

### Pause/Resume Pool

`p` toggles the selected pool:

- paused pool -> `crate::pool::resume_pool(...)`
- active pool -> `crate::pool::pause_pool(...)`

This action has visible selection semantics on the Pools tab.

### Command Palette Preview

`Ctrl-K` opens the command palette. V3.01 uses the canonical action registry to
render both the action list and a right-side preview pane. The preview shows:

- risk tier
- side-effect class
- required grant
- dry-run availability
- enabled/disabled status
- disabled reason or execution guidance

This intentionally separates action discoverability from action authority. For
example, `request_merge` is visible, but the preview explains that merge proof
must be requested through the evidence-bound API instead of being inferred from
green-looking UI state. Mutating agent actions such as `propose_patch`,
`race_patches`, and `run_tests` remain grant-bound and envelope-bound.

## Release and Production Deployment Visibility

The TUI reads release state from `release::build_release_status_report(...)`, using:

```rust
ReleaseStatusQuery {
    project_id: Some(release::DEFAULT_RELEASE_PROJECT_ID),
    ref_name: Some("main".into()),
    sha: None,
    limit: 1,
}
```

The release state model includes canary and production fields:

```rust
pub struct ReleaseAttempt {
    pub id: i64,
    pub project_id: i64,
    pub ref_name: String,
    pub sha: String,
    pub version: String,
    pub upstream_pipeline_id: Option<i64>,
    pub upstream_status: String,
    pub release_pipeline_id: Option<i64>,
    pub release_pipeline_status: Option<String>,
    pub production_pipeline_id: Option<i64>,
    pub production_pipeline_status: Option<String>,
    pub canary_status: String,
    pub canary_started_at: Option<String>,
    pub canary_finished_at: Option<String>,
    pub canary_note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

The Release Watch pane exposes production status as:

```rust
attempt
    .production_pipeline_status
    .as_deref()
    .unwrap_or("not-triggered")
```

and includes the production pipeline id when attached. This is the current TUI surface for canary + production rollout visibility.

The Flow Collector also gives priority to the release pipeline when available. It obtains the latest release attempt and inserts its release pipeline flow at the front of `active_pipelines` if it was not already included in tracked pipelines.

## Data Sources

The TUI uses a combination of local DB state, live GitLab API calls, Docker, filesystem checks, and release evidence files.

| Data | Source |
| --- | --- |
| GitLab online state | `gitlab.is_ready().await` |
| Active GitLab jobs | `gitlab.list_jobs(DEFAULT_RELEASE_PROJECT_ID, &["running", "pending", "created"])` |
| Job trace logs | `gitlab.job_trace(project_id, job_id)` |
| Pipeline jobs | `gitlab.list_pipeline_jobs_with_downstream(...)` |
| Pipeline details | `gitlab.get_pipeline(...)` |
| Pools | `db.list_pools()` |
| Recent job events | `db.recent_job_events(50)` |
| Tracked pipelines | `db.list_tracked_pipelines(...)` |
| Cache metrics | `db.get_cache_metrics()` |
| Taints | SQL count from `cache_taints` |
| Cache verdict misses | SQL count from `cache_verdicts` |
| Test bottlenecks | `db.get_test_bottlenecks("average"|"latest", 50)` |
| Test history | `db.get_test_history(test_name, 50)` |
| Managed containers | `docker.list_managed_containers()` |
| Root disk usage | `df -k /` |
| CAS/crate/runner sizes | Recursive directory size checks under `crate::config::data_dir()` |
| Release state | `release::build_release_status_report(...)` |

## Empty, Partial, and Stale States

The TUI has to handle data arriving out of order:

- DB can have recent jobs before GitLab pipeline jobs are available.
- GitLab can temporarily fail.
- Flow collector can emit an empty snapshot.
- Logs can be unavailable for a selected job.
- General snapshot sync can lag behind flow/log workers.

Current behavior:

- A missing release shows a clear empty release message.
- A missing pipeline flow shows "Waiting for active pipelines..."
- A pipeline with no graph columns shows "Waiting for job graph..."
- Empty flow snapshots do not erase a visible board.
- Retained flow is marked stale with age.
- Log fetch errors mark logs stale and show a short error in the log pane title.
- Selected job is preserved across job list reorder when possible.
- Follow-tail mode keeps logs pinned to latest output until the user manually scrolls.

## Progress and ETA Heuristics

Progress is heuristic. It is not a GitLab-native percent.

Pipeline-level progress in the general snapshot worker:

- Count total distinct jobs by pipeline from `job_events`.
- Count completed jobs where status is `success`, `failed`, or `canceled`.
- Count running jobs.
- Compute effective completion as `completed + running * 0.5`.

Job row progress:

- `success` -> 100%
- `failed`/`canceled` -> estimated partial progress, often using queued duration or elapsed time
- `running` -> elapsed time over an assumed 120s window, capped at 99%
- pending/created -> 0%

Flow graph ETA is lane-based:

| Lane | Default historical duration |
| --- | --- |
| Unit | 60s |
| Integration | 300s |
| Security | 120s |
| Build | 180s |
| ReleaseExecution | 120s |
| Other | 90s |

The ETA code notes that a real implementation should eventually query historical test bottleneck data instead of using these simple fallbacks.

## Rendering Design Details

The UI uses plain Ratatui primitives:

- `Layout` for screen partitioning.
- `Block` for pane borders and titles.
- `Paragraph` for textual panels and logs.
- `List`/`ListItem` for job and test lists.
- `Gauge` for progress bars.
- `Span`, `Line`, and `Text` for inline color.

Status colors are centralized:

```rust
fn status_color(status: &str) -> Color {
    match status {
        "success" | "omitted" | "vti-skipped" => Color::Green,
        "running" => Color::Blue,
        "failed" => Color::Red,
        "pending" | "created" => Color::Yellow,
        "canceled" => Color::DarkGray,
        _ => Color::Gray,
    }
}
```

Pane focus is shown primarily via border color:

```rust
fn pane_border(pane: ActivePane, app: &App) -> Color {
    if app.active_pane == pane {
        Color::Cyan
    } else {
        Color::DarkGray
    }
}
```

The active Flow layout uses the `Pipelines` border color for the Flow Board and `Jobs` border color for the jobs/log area.

## Tests and Proof Commands

The TUI has focused tests covering:

- rendering all primary tabs with empty state
- rendering maximized logs with empty state
- Mission default tab selection and action-first cockpit rendering
- command palette preview rendering from the canonical action registry
- rendering Flow with jobs list and live log
- sorting live jobs by status rank
- preserving flow/log state across general snapshots
- preventing empty flow snapshots from blanking the board
- preserving selected job across refresh reorder
- opening and scrolling logs/follow mode

Useful proof commands:

```bash
cargo check -p jeryu --message-format=json
cargo test -p jeryu -- tui -- --nocapture
cargo run -p jeryu -- tui --once
```

In this environment, commands should be run through `rtk`, for example:

```bash
rtk cargo test -p jeryu -- tui -- --nocapture
```

## Current Limitations and Open Design Questions

These are current implementation facts, not aspirational design:

- Live logs are polled from GitLab every 650ms; there is no websocket implementation yet.
- The old pool and pipeline list renderers exist but are not in the active Flow screen layout.
- The active Flow screen has a visible Live Jobs list pane and Log Preview pane.
- The Flow Board currently renders only the first active pipeline.
- Graph edges are not computed; `FlowGraph.edges` exists but is currently empty.
- ETA is heuristic and lane-based.
- Some storage numbers are approximate or unpopulated, especially Docker detailed storage classes.
- The Evidence tab is useful but is not yet a fully searchable proof timeline.
- The command palette now previews risk, grants, dry-run availability, side effects,
  and disabled reasons, but only a subset of actions execute directly in the TUI.
- The Agents tab now renders an agent cockpit with phase, progress, branch, grants,
  and actions from current pipeline/audit state, but it is not yet backed by a
  dedicated agent-run lifecycle table.
- Mouse capture is enabled, but active interactions are keyboard-driven.

## Practical Mental Model for External Reviewers

Think of the TUI as four overlapping subsystems:

1. A mission-control surface: Top Signal, Attention Queue, Proof Stack, and Next Actions.
2. A release/deployment observability surface: Release Watch plus Flow Board.
3. A real-time job operations surface: Live Jobs plus Log Preview/maximized logs.
4. A host/cache/test/evidence/model-policy surface: Pools, Cache, Tests, Evidence, Secrets, and LLMs tabs.

The biggest UX requirement in the current implementation is stability under partial data. The TUI intentionally keeps the last meaningful flow board visible and marks it stale rather than letting transient empty snapshots blank the screen. This is the key design point behind the recent anti-blink behavior.

The second key requirement is live process visibility. The current implementation gives that through the Live Jobs list, stable job selection, a selected-job log target, 650ms trace polling, syntax-highlighted logs, and follow-tail scrolling. A websocket transport could improve latency and reduce polling, but it is not required for the current behavior to be understandable or usable.

The third key requirement is action clarity. V3.01 moves the TUI toward the
strongest v3 review theme: every screen should help answer what happened, why it
matters, what proof exists, and what the safest next action is. Mission Control,
Agent Cockpit, Proof Stack, Attention Queue, and command-palette previews are
the first implementation of that direction. The next major step is a unified
event stream and durable view-model API so the TUI, CLI, and capability server
consume the same entity-linked facts instead of assembling them separately.
