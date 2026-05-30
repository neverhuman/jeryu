# Port Spec 01 — TUI Subsystem → `crates/jeryu-tui`

Status: AUTHORITATIVE EXECUTION SPEC. Owner of output paths: TUI worker.
Scope: the entire ratatui Flight Deck (18 lenses, 34 actions, widgets, focus,
runtime, theme, testing, tuiwright), its read-model contract, the inspection
HTTP plane it consumes, and the `tui-capture` PNG renderer.

LOCKED DECISIONS honored here: D1 (zero `gitlab`/`jitforge`/`JitForge`/`Nitro`
literals; only `jeryu`/`jeryu-*` survive), D2 (engine crate renames), D3 (full
fusion: keep SQLite+RedlineDB `db/`, HTTP daemons, ratatui TUI, React web; the
GitLab backend is replaced 100% by `jeryu-*` core), D4 (MR/merge-request →
PullRequest/PR), D5 (runners OCI-first then native).

Source product (read-only): `/home/ubuntu/jeryu` (Rust 2024, ~159k LOC).
Fused product: `/home/ubuntu/jeryuRUST`, crate edition 2024.

> KEY ARCHITECTURAL FINDING — the rewire is shallow, not deep.
> The 18 lenses are **pure projections** (`*LensInput::from_read_model(&TuiReadModel)`,
> "No I/O" invariant in every `lenses/*/data.rs`). They do NOT call GitLab.
> The GitLab coupling lives ENTIRELY in three places below the lenses:
> (a) the read-model assembler / inspection daemon that fills `TuiReadModel`,
> (b) `App::new(store, docker, gitlab: GitlabClient)` + `app_runtime_sync.rs` +
> `flow/collector.rs::collect_once(session, docker, gitlab)`, and
> (c) one hard field `SystemHealth.gitlab` in `src/api/read_model.rs:178`.
> Therefore: porting the lenses is a near-mechanical move; the real work is
> rewiring the `App` data spine and the inspection daemon onto `jeryu-*` core,
> plus scrubbing 132 `gitlab` literal occurrences across 99 files (count from
> `grep -rno gitlab src/tui src/api/read_model.rs src/api/dashboards src/inspection`).

---

## 1. Source Inventory

All paths under `/home/ubuntu/jeryu`. 309 files under `src/tui/**` + the API/
inspection backplane + `crates/tui-capture`. Grouped by role.

### 1.1 Module roots & entry points
| Source path | Purpose |
|---|---|
| `src/tui.rs` | TUI crate module root; re-exports `run_tui`, `run_tui_once`, `run_tui_screenshot`, `capture_tui_png`, `smoke_render_once`. |
| `src/tui/runner.rs` | Terminal lifecycle (raw mode, alt screen, mouse capture) + the 5 entrypoints. Takes `DockerCtl` + `GitlabClient`. **Primary GitLab seam.** |
| `src/tui/runner_tests.rs` | Runner entrypoint tests. |
| `src/cli_defs.rs` (lines 29–48) | `Tui` clap subcommand: `--once --demo --capture --screenshot --tab --output --width --height --screenshot-hold-ms`. Defaults: tab=`jobs`, output=`paper/assets/jeryu-tui.png`, width=140, height=44, hold=1100ms. |
| `src/dispatch.rs` | Dispatches `Commands::Tui{..}` → the 5 runner entrypoints. |
| `src/cli_tests.rs` (line 67) | CLI flag parse tests (`--output` etc.). |

### 1.2 Application state & runtime spine
| Source path | Purpose |
|---|---|
| `src/tui/app/mod.rs` | `App`/`TuiStateSnapshot` re-exports; imports `gitlab_client::GitlabClient`, `docker::DockerCtl`, `state::{JobEvent,Pool,TrackedPipeline,TuiSession}`. **Seam.** |
| `src/tui/app/state.rs` | `App` struct + `TuiStateSnapshot` (the big mutable state). |
| `src/tui/app/types.rs` | `ActivePane, ActiveTab, NodeSummary, PendingApproval, PipelineMetrics, PipelineProgressView, ReleaseStage*, RunnerFeed, StageProgress, StorageBreakdown` (note `docker_images_bytes` etc. at lines 104–106). |
| `src/tui/app/builder.rs`, `app/channels.rs`, `app/reducer.rs`, `app/selectors.rs` | App builder, mpsc/watch channel bundle, reducer, selectors. |
| `src/tui/app_runtime.rs` | `App::new(store, docker, gitlab)`, `new_render_only(docker, gitlab)`, `build(...)`. **Seam: constructor signature carries `GitlabClient`.** |
| `src/tui/app_runtime_sync.rs` | Background refresh: pulls `store.list_pools()`, `gitlab.active_jobs`, `store.recent_job_events`, `store.recent_ci_job_runs`, `gitlab.is_ready()`, `store.list_tracked_repositories`, `store.latest_evidence_by_job_id`, `store.list_managers_for_node`. **Largest GitLab/DB seam.** |
| `src/tui/app_runtime_sync_actions/{mod,delivery,focus,navigation,queue,runners,tabs,workflow}.rs` | Per-domain sync-action handlers. |
| `src/tui/app_runtime_sync_background.rs`, `_progress.rs`, `_tests.rs` | Background sync loop, progress sync, tests. |
| `src/tui/app_demo.rs`, `app_runtime_demo*.rs`, `app_runtime_demo_fixtures.rs`, `app_runtime_demo_state.rs` | `--demo` fixture wiring (`apply_demo_fixture`). |
| `src/tui/live.rs`, `src/tui/activity.rs` | Live event/activity feed plumbing. |

### 1.3 Data transport (the abstraction that already isolates GitLab)
| Source path | Purpose |
|---|---|
| `src/tui/runtime/mod.rs` | Runtime router: `data`, `input`, `maintenance`, `render`, `stream`. |
| `src/tui/runtime/data/mod.rs` | `DataTransport` enum (`Http/McpResource/Local/Fixture`) + transport submodules. |
| `src/tui/runtime/data/client.rs` | `DataClient` trait — the single async surface every screen calls. Methods: `fetch_read_model`, `fetch_events`, `fetch_proof`, `fetch_entity`, `fetch_runtime_profile`, `fetch_action_registry`, `preview_action`, `execute_action`. **This is the clean port boundary.** |
| `src/tui/runtime/data/http.rs` | `HttpDataClient` → `/api/v1/*`. Strips `InspectionEnvelope<T>`. |
| `src/tui/runtime/data/fixture.rs` | `FixtureDataClient` (demo/screenshot/tests). |
| `src/tui/runtime/data/local.rs` | `LocalDataClient` DB fallback stub (returns typed errors → degraded badge). |
| `src/tui/runtime/data/mcp.rs` | `McpResourceDataClient` read-only MCP resource stub. |
| `src/tui/runtime/data/trace.rs` | `TraceDataClient` recording/replay transport. |

### 1.4 Runtime input / render / stream / maintenance
| Source path | Purpose |
|---|---|
| `src/tui/runtime/input.rs` + `input/{mouse,palette}.rs`, `input/navigation.rs` + `navigation/{general,jobs,tabs,workflow}.rs` | Keyboard/mouse event loop (`run_loop`, `hydrate_smoke_state`), command palette, nav routing. |
| `src/tui/runtime/render.rs` + `render/{png,tab_tests,tests}.rs` | Frame render driver; `write_buffer_png`, `parse_capture_tab`, `cleanup_screenshot_terminal`. PNG path used by capture/screenshot. |
| `src/tui/runtime/stream/mod.rs` | `StreamMode` (`Sse/Poll/Degraded/Ws`) → header badge. |
| `src/tui/runtime/stream/{sse,poll,ws,degraded}.rs` | SSE primary (`/api/v1/events/stream`), HTTP poll fallback (`/api/v1/events?cursor=N`), WS reserved, `DegradedTimeline`/`DegradedReason` typed degradation. |
| `src/tui/runtime/maintenance.rs` | `cache_maintenance_loop(DockerCtl)` background task. **Seam: takes DockerCtl.** |

### 1.5 The 18 lenses (`src/tui/lenses/`) — canonical 5-file shape
`mod.rs` defines `LensId` (the 18 variants) + `route()`/`label()`/`CORE`.
Every lens: `mod.rs` / `view.rs` / `data.rs` / `nav.rs` (+ optional `tests.rs`).
`data.rs` is a pure projector from `TuiReadModel` or `App` state. Detail per
lens in §3. Notable per-lens extras: `evidence/{bundle,graph}.rs`,
`queue/lab.rs`, `repos/shell.rs`, and the large `workflow/` tree
(`canvas/`, `delivery/`, `inspector/`, `logs/`, `model/`, `rails/`, `regions.rs`,
`nav.rs`, `view.rs`).

### 1.6 Workflow engine (the PR/delivery DAG — already PR-named)
| Source path | Purpose |
|---|---|
| `src/tui/workflow/model/pr_view.rs` | `PrStatus` (Draft/Open/Running/Merged/Blocked/Closed), `PullRequestView`, `FleetSummary`, `DeliverySnapshot`. **Already uses PR vocabulary (D4 partially done).** |
| `src/tui/workflow/model/{mod,edge,node_kind,phase,snapshot,status,tests}.rs` | DAG node/edge/phase/snapshot/status types. |
| `src/tui/workflow/{builder,collector,intelligence,live_delivery,minimap,mission_strip,phase_rail,pr_rail,regions,nav,hit_map}.rs` | Atlas builders, live delivery collector, minimap, mission strip, phase/PR rails. |
| `src/tui/workflow/delivery/{agent_review,auto_merge,ci,post_merge,promotion,mod}.rs` | Delivery stages (agent review, auto-merge, CI, post-merge, promotion). |
| `src/tui/workflow/inspector/{actions,agent,card,log_tail,tabs,tests,mod}.rs` | PR inspector panes. |
| `src/tui/workflow/widget/{canvas,hit_map,layout,render,mod}.rs` | DAG canvas widget. |
| `src/tui/workflow/action_adapter/{fake,helpers,production,mod}.rs` + `tests/*` | Action adapter: `FakeActionAdapter` (default) vs `ProductionActionAdapter` (`App::try_install_production_adapter`). **Seam: production adapter wires to backend mutations.** |

### 1.7 Flow / collectors (live pipeline → CI-run snapshots)
| Source path | Purpose |
|---|---|
| `src/tui/flow/collector.rs` | `run_collector(session, docker, gitlab, ...)` + `collect_once(session, docker, gitlab)` → `FlowSnapshot`. **Seam: GitLab pipeline poll.** |
| `src/tui/flow/recovery.rs` | `gitlab_job_to_event`, `pipeline_flow_from_jobs`. **Seam: GitLab job decode (rename + retype).** |
| `src/tui/flow/{builder,eta,inspector,model,widget,mod}.rs` | Flow model, ETA, inspector, widget. |

### 1.8 Widgets / theme / focus / nav / chrome
| Source path | Purpose |
|---|---|
| `src/tui/widgets/` (40+ files: `agent_fleet*`, `attention`, `command_palette`, `dag`, `entity_link`, `event_tape`, `forms`, `freshness_chip`, `header`, `heatmap`, `help`, `inspector*`, `log_viewer`, `mission*`, `modal`, `progress_bar`, `proof_chip`, `shared`, `sparkline`, `status_badge`, `status_strip`, `tabs`, `timeline`, `virtual_table`, `vti_proof`, `action_dispatch`) | Reusable ratatui widgets. Pure render; no backend. |
| `src/tui/theme/{mod,palette,badges,glyphs,legacy,progress,terminal_caps,tests}.rs` | Color palette, glyphs, badges, terminal capability detection. |
| `src/tui/focus/{mod,chrome,map,pane,state,tests}.rs` | Focus map / pane focus state machine. |
| `src/tui/nav/{mod,direction}.rs` | Navigation primitives. |
| `src/tui/ui/{mod,draw,flight_deck,overlay,overlays}.rs`, `ui_chrome.rs`, `ui_chrome_footer.rs` | Top-level draw entry (`ui::draw`), Flight Deck composition, overlays, chrome/footer. |
| `src/tui/repo_fleet_bar.rs` | Repo fleet bar (aliases `nht`/`shared`/`warp`). **Reads fleet snapshot/`JERYU_WORKSPACE_ROOT`.** |
| `src/tui/proof_lanes/mod.rs`, `aer/mod.rs`, `vrc/mod.rs`, `witness/mod.rs`, `bugs.rs` | Proof lanes, AER, VRC, witness, bugs helpers. |

### 1.9 Action registry (34 actions)
| Source path | Purpose |
|---|---|
| `src/tui/action_registry.rs` | `RiskTier` + registry core. |
| `src/tui/action_registry_entries.rs` | The 34 action entries (preview/execute contracts). |
| `src/tui/action_registry_tests.rs` | Registry tests (HTTP client asserts `>= 30` actions). |
| `src/tui/actions/mod.rs` | Action enum / dispatch glue. |

### 1.10 Jankurai (UX-audit lens data source)
| Source path | Purpose |
|---|---|
| `src/tui/jankurai/{mod,model,parse,root}.rs` | Parses `agent/repo-score.json` + `agent/score-history.jsonl` into `JankuraiSnapshot`. Local-file source, no backend. |

### 1.11 Testing harness & fixtures
| Source path | Purpose |
|---|---|
| `src/tui/testing/mod.rs`, `scenarios.rs`, `scenarios_tests.rs`, `repo_scenarios.rs` | Scenario builders producing `TuiReadModel`s. |
| `src/tui/testing/fixtures/{agents,bugs,cache,incident,jankurai,mission,queue,release,repos,security,vti,workflow,mod}.rs` | Per-domain demo/test fixtures (back `--demo`, screenshots, tuiwright). |

### 1.12 Read-model contract + dashboards + inspection plane (the data backplane)
| Source path | Purpose |
|---|---|
| `src/api/read_model.rs` | `TuiReadModel` (schema `tui.v1.0`), `MissionSnapshot`, `AttentionItem`, `NextActionRecommendation`, `ActionSafety`, `SystemHealth`. **Contains `SystemHealth.gitlab: ComponentHealth` (line 178) — D1 violation, must rename.** |
| `src/api/read_model_health.rs` | `ComponentHealth`, `RunnerHealth` (`ComponentHealth::ok("gitlab", 12)` at line 58 — scrub). |
| `src/api/read_model_queue.rs`, `_repos.rs` | `QueueSnapshot`/`QueuePoolSnapshot`/`QueueJobSummary`, `ReposSnapshot`/`RepoSummary`/`RepoFamilySummary`. |
| `src/api/dashboards/*.rs` (20 files) | Per-lens typed dashboard contracts: `runners`, `source_doctor`, `evidence`, `release`, `queue`, `cache`, `vti`, `security`, `autonomy`, `llms`, `agents`, `bugs`, `git_sync`, `jankurai`, `aer`, `artifacts`, `bottlenecks`, `fleet`, `workflow`, `mod`. Each is "pure data + `SourceFreshness`". |
| `src/inspection/mod.rs`, `router.rs`, `serve.rs`, `state.rs` | axum `/api/v1/*` router + `serve_inspection(listener, state)` + `InspectionState` (Arc<RwLock<TuiReadModel>>). |
| `src/inspection/read_model.rs` | `GET /api/v1/read-model` → `InspectionEnvelope<TuiReadModel>`. |
| `src/inspection/repos.rs` | `GET /api/v1/repos`, `GET /api/v1/families`. |
| `src/inspection/proof.rs` | `GET /api/v1/proof` (proof timeline). |
| `src/inspection/events.rs` | `GET /api/v1/events` (`EventPage`, caps 500) + `GET /api/v1/events/stream` (SSE). |
| `src/inspection/entity.rs` | `GET /api/v1/entity/{kind}/{id}` (`EntityDetail`; rejects unknown kind 400). |
| `src/inspection/health.rs` | `GET /api/v1/runtime/profile`, `GET /api/v1/health/deep`. |
| `src/inspection/actions.rs`, `actions_tests.rs` | `POST /api/v1/action/preview`, `POST /api/v1/action/execute`, `GET /api/v1/action/{run_id}/stream`, `GET /api/v1/action-registry`. |

### 1.13 `crates/tui-capture` (standalone PNG renderer)
| Source path | Purpose |
|---|---|
| `crates/tui-capture/Cargo.toml` | Bin crate manifest. |
| `crates/tui-capture/src/main.rs` | `tui-capture` CLI: `--cols --rows --out --font --font-size --cell-w --cell-h --padding --bg --fg --brighten --respect-dim --min-wait-ms --max-wait-ms --quiet-ms --send-after-ms --send --dump-text --ready-file -- <cmd>`. Spawns a TUI in a PTY and rasterizes to PNG. |
| `crates/tui-capture/src/{capture_runtime,support,support_utils}.rs` | PTY capture loop, font discovery, glyph validation, hex parsing. |

### 1.14 tuiwright snapshot suite (`tests/tuiwright/`, 27 files)
| Source path | Purpose |
|---|---|
| `tests/tuiwright/harness.rs` | Shared harness: `capture_tui(tab)` / `capture_tui_size` shell out to `jeryu tui --capture --tab .. --output .. --width .. --height ..` with `TERM=xterm-256color`, `JERYU_DATABASE_URL=<sqlite memory>`; PNG cell-grid assertions (`CAPTURE_COLS=120`, `CAPTURE_ROWS=36`, `CELL_W=8`, `CELL_H=12`), `assert_main_layout_regions`, `tuiwright_lock` (serial). |
| `tests/tuiwright/helpers.rs` | Interactive PTY helpers, focus assertions, text-order/absent waits. |
| `tests/tuiwright/{capture,navigation*,tabs,palette,overlays,drilldown,discovery,fleet_bar,workflow,bugs,jankurai}.rs` | Behavior suites. |
| `tests/tuiwright/lenses_*.rs` (14 files) | Per-lens render/behavior snapshots: `agents, autonomy, bugs, cache, evidence, llms, mission, queue, release, repos, runners, source_doctor, vti, workflow`. |
| `tests/tuiwright/README.md` | Split migration map; proof command `TERM=xterm-256color cargo test --test tuiwright -- --test-threads=1`. |

---

## 2. Target Layout in `/home/ubuntu/jeryuRUST`

New crate: **`crates/jeryu-tui`** (lib + the `jeryu tui` bin path stays in the
main product binary; this crate exposes `run_tui*`/`capture_tui_png`). Edition
2024. The read-model contract + inspection plane move into a thin TUI-facing
crate so `jeryu-tui` does not depend on the whole product binary.

```
crates/jeryu-tui/
  Cargo.toml                      # name="jeryu-tui", edition="2024"
  src/
    lib.rs                        # was src/tui.rs; re-exports run_tui*, capture_tui_png
    runner.rs                     # was tui/runner.rs; constructor now takes Arc<dyn DataClient> + ControlPlane handle (NO GitlabClient)
    app/{mod,state,types,builder,channels,reducer,selectors}.rs
    app_runtime.rs                # App::new(store, control_plane, data_client) — GitlabClient REMOVED
    app_runtime_sync*.rs          # rewired onto jeryu-core/jeryu-runnerd/jeryu-cache (see §3)
    app_demo.rs, app_runtime_demo*.rs
    live.rs, activity.rs, bugs.rs, repo_fleet_bar.rs
    proof_lanes/, aer/, vrc/, witness/
    runtime/
      data/{mod,client,http,fixture,local,mcp,trace}.rs   # DataClient trait UNCHANGED
      input/, render/, stream/, maintenance.rs
    lenses/                       # all 18 lenses moved verbatim (pure)
    workflow/                     # PR DAG engine; PrStatus/PullRequestView kept
    flow/                         # collector rewired off GitLab → jeryu-core CI runs
    widgets/, theme/, focus/, nav/, ui/
    action_registry*.rs, actions/
    jankurai/
    testing/                      # fixtures + scenarios
  tests/
    tuiwright/                    # 27 files moved; harness shells `jeryu tui --capture`

crates/jeryu-readmodel/           # NEW: TUI<->backend contract crate (was src/api/{read_model*,dashboards,entity,freshness,inspection,proof,runtime_profile})
  src/
    read_model.rs                 # TuiReadModel; SystemHealth.gitlab RENAMED -> SystemHealth.scm
    read_model_health.rs read_model_queue.rs read_model_repos.rs
    dashboards/*.rs               # 20 dashboard contracts
    entity.rs freshness.rs inspection.rs proof.rs runtime_profile.rs

crates/jeryu-api/                 # = renamed jitforge-api (D2). MUST grow the inspection HTTP plane:
  src/inspection/{mod,router,serve,state,read_model,repos,proof,events,entity,health,actions}.rs
  # serves /api/v1/{read-model,repos,families,proof,events,events/stream,entity,runtime/profile,health/deep,action/*,action-registry}

crates/jeryu-tui-capture/         # = renamed crates/tui-capture; CLI flags unchanged
```

Crate dependency direction: `jeryu-tui → jeryu-readmodel`, `jeryu-tui →
jeryu-api` (only for the `HttpDataClient` envelope/handler types it decodes),
and the product binary wires `jeryu-tui`'s `App` to live `jeryu-core` /
`jeryu-runnerd` / `jeryu-cache` / `jeryu-proof` / `jeryu-signrail` handles.
`jeryu-tui` MUST NOT depend on `gitd`/`forge-core`/etc. by their old names.

Naming scrub (D1/D2), applied across the move:
- module `gitlab_client` / type `GitlabClient` → **removed** (replaced by a
  `jeryu-core` control-plane handle, see §3); no `Gitlab*` symbol survives.
- `SystemHealth.gitlab` field + `"gitlab"` component name → `SystemHealth.scm`
  / `"scm"` (source-control component; or drop entirely if backend has no SCM
  edge). `SystemHealth::components()` updated.
- env vars `JERYU_*` are already neutral — keep. (`JERYU_DATABASE_URL`,
  `JERYU_WORKSPACE_ROOT`, `JERYU_TUI_WORKFLOW_INSPECT_OPEN`,
  `JERYU_TUI_SCREENSHOT_LIVE_FLEET`, `TUI_READY_FILE` — all preserved.)
- `flow/recovery.rs::gitlab_job_to_event` → `ci_run_to_event`;
  `flow/collector.rs` `gitlab.is_ready()`/`snap.gitlab_online` → `control_plane
  .is_ready()` / `snap.control_plane_online`.
- demo aliases (`nht`/`shared`/`warp`) and the literal `"redlinedb"` (bugs lens)
  are product data, not forbidden literals — keep.

---

## 3. Rewire Map

### 3.1 The 18 lenses
Source-path column is the lens dir under `src/tui/lenses/`. "Current source" is
where the underlying data is produced TODAY (almost always `TuiReadModel`, which
is itself assembled from GitLab/DB by the daemon — that assembler is the rewire
target). "Rewire target" is the `jeryu-*` core that must fill the corresponding
field of `TuiReadModel`/dashboard once the daemon is rewired.

| # | Lens (LensId / route) | Source path | Current GitLab/DB data source | Rewire target (`jeryu-*`) |
|---|---|---|---|---|
| 1 | Mission (`mission`) | `lenses/mission/` | `TuiReadModel.mission` (`MissionSnapshot`) assembled from GitLab job counts + DB capsules/grants/cache | `jeryu-core` (safe_to_code/merge/release, job/agent counts) + `jeryu-proof` (open_capsules/evidence_count) + `jeryu-cache` (cache_hit_ratio/taints) + `jeryu-runnerd` (active/total_runners) |
| 2 | Queue (`queue`) | `lenses/queue/` (+`lab.rs`) | `mission.{queued,running,failed}_jobs` + `system.runners` from GitLab pipelines + DB pools | `jeryu-core` CI run queue + `jeryu-runnerd` `RunnerHealth` |
| 3 | Repos (`repos`) | `lenses/repos/` (+`shell.rs`) | `TuiReadModel.repos` (`ReposSnapshot`) from `store.list_tracked_repositories()` / fleet snapshot | `jeryu-core` repo registry (`/repos`, `/families`) + fleet snapshot (`repo_fleet_bar`) |
| 4 | Workflow (`workflow`) | `lenses/workflow/` + `tui/workflow/` engine | `DeliverySnapshot`/`PullRequestView` from GitLab MR + pipeline poll via `flow/collector.rs` | `jeryu-core` PR + CI-run DAG (`pipeline`→`ci run`, MR→PR already done in `pr_view.rs`); delivery stages → `jeryu-core` auto-merge/promotion + `jeryu-ci-*` |
| 5 | Evidence (`evidence`) | `lenses/evidence/` (+`bundle,graph`) | `mission.{evidence_count,open_capsules}` + `model.attention[].evidence` | `jeryu-proof` (proof ledger / capsules) via `GET /api/v1/proof` |
| 6 | Release (`release`) | `lenses/release/` | fleet snapshot repo status + `release_hint(session)` | `jeryu-core` release state + `jeryu-signrail` (signed-release readiness) |
| 7 | Runners (`runners`) | `lenses/runners/` | `App` `NodeSummary` fleet synced from `store.list_managers_for_node` + `DockerCtl` | `jeryu-runnerd` (node/manager fleet, OCI-first per D5) — `RunnersDashboard` |
| 8 | Agents (`agents`) | `lenses/agents/` | `mission.{active_agents,blocked_agents,active_grants,agents_can_code}` | `jeryu-agentbridge` (agent fleet/lifecycle) + `jeryu-core` grants |
| 9 | Bugs (`bugs`) | `lenses/bugs/` | `mission.top_blocker` + ranked `attention` + `failed_jobs`; bugs detail from DB (`redlinedb`) | `jeryu-core` bug/blocker store (RedlineDB kept per D3) |
| 10 | Cache (`cache`) | `lenses/cache/` | `mission.{cache_hit_ratio,active_taints,taint_count}` + `system.cache` | `jeryu-cache` (was cratevault*) metrics + component health |
| 11 | VTI (`vti`) | `lenses/vti/` | `mission.selector_misses_24h` + job counts | `jeryu-core` test-impact/selector telemetry (`VtiDashboard`) |
| 12 | Source Doctor (`source-doctor`) | `lenses/source_doctor/` | `SystemHealth.components()` = {gitlab,database,docker,cache,vault} + `RunnerHealth` | `jeryu-core` component health: **drop `gitlab`, add `scm`/control-plane**; `database`(SQLite/RedlineDB), runners=`jeryu-runnerd`, cache=`jeryu-cache`, vault=`jeryu-signrail` |
| 13 | Approvals (`approvals`) | `lenses/approvals/` | `App` `PendingApproval{pr_number,..}` queue (already PR-named) | `jeryu-core` PR approval queue + `jeryu-signrail` (risk-tier gating) |
| 14 | Git (`git`) | `lenses/git/` | `state::GitCommandEventRecord` ledger (`argv_redacted`, `mirror_status`) | `jeryu-gitd` (git daemon command ledger) + `jeryu-mirror` (mirror sync status) |
| 15 | Jankurai (`jankurai`) | `lenses/jankurai/` + `tui/jankurai/` | local files `agent/repo-score.json`, `agent/score-history.jsonl` | unchanged (local-file UX-audit); just move. No backend dependency. |
| 16 | Autonomy (`autonomy`) | `lenses/autonomy/` | `mission.{active_grants,agents_can_code,safe_to_code,blocked_agents}` + `DeliverySnapshot.kill_bell_state` | `jeryu-core` autonomy/kill-bell + `jeryu-agentbridge` guardrails |
| 17 | Secrets (`secrets`) | `lenses/secrets/` | `state::SecretAuditEvent` ledger (metadata ONLY: action/status/repo/created_at) | `jeryu-signrail` (secret/credential audit; SECURITY: never carry value/target/version) |
| 18 | LLMs (`llms`) | `lenses/llms/` | proxy from `mission.{active_agents,active_grants,agents_can_code}` (no dedicated telemetry yet) | `jeryu-agentbridge` LLM telemetry (provider/model/token/latency) once `LlmsSnapshot` lands; surface unchanged |

### 3.2 Symbol / vocabulary rewire (D4 + D2)
| Source symbol / data | Current (GitLab) source | Target `jeryu-*` type/API |
|---|---|---|
| `GitlabClient` (ctor arg of `App::new`/`run_tui*`/`collect_once`) | `gitlab_client::GitlabClient` HTTP client | `jeryu-core` control-plane handle (trait, e.g. `ControlPlane`); TUI App ctor signature changes to `App::new(store, control_plane, data_client)` |
| `gitlab.active_jobs(..)` (`app_runtime_sync.rs:92`) | GitLab pipelines API | `jeryu-core` CI run list (`pipeline` → **ci run**) |
| `gitlab.is_ready()` / `snap.gitlab_online` (`collector.rs:68`) | GitLab readiness ping | `control_plane.is_ready()` / `snap.control_plane_online` |
| `gitlab_job_to_event`, `pipeline_flow_from_jobs` (`flow/recovery.rs`) | GitLab job JSON → events | `ci_run_to_event`, `ci_flow_from_runs` over `jeryu-core` CI-run types |
| MR / merge-request | GitLab MR | **PullRequest / PR** (already in `workflow/model/pr_view.rs`: `PrStatus`, `PullRequestView`, `PendingApproval.pr_number`); `cli_defs.rs` `Mr(MrCommands)` subcommand → `Pr(PrCommands)` (coordinate with CLI worker) |
| `pipeline` / `TrackedPipeline` / `PipelineMetrics` / `PipelineProgressView` | GitLab pipeline | `jeryu-core` **CI run** types; rename to `CiRun*` where they cross the wire, keep TUI-internal aliases if churn-risky but scrub user-visible "pipeline" labels |
| `SystemHealth.gitlab: ComponentHealth` (`read_model.rs:178,202`) | GitLab health probe | `SystemHealth.scm` (or remove); `ComponentHealth::ok("scm", ..)` in `read_model_health.rs:58` |
| `RunnersDashboard` / `NodeSummary` / `RunnerFeed` | DB `list_managers_for_node` + Docker | `jeryu-runnerd` runner fleet (OCI-first, D5); `jeryu-runner-oci`/`jeryu-runner-native` for sandbox kind |
| `DockerCtl` (ctor arg + `cache_maintenance_loop`) | Bollard Docker | `jeryu-runnerd` sandbox controller; `docker_images_bytes`/`docker_volumes_bytes`/`docker_build_cache_bytes` (`types.rs:104-106`) → `sandbox_*_bytes` |
| `store: TuiSession` (SQLite/RedlineDB) | jeryu `db/` layer | **KEPT (D3)** — only the GitLab/Docker reads it currently performs alongside are rerouted |
| `ProductionActionAdapter` (`workflow/action_adapter/production.rs`) | mutates via GitLab/DB | `jeryu-core` action execution + `jeryu-api` `/action/execute` |

### 3.3 Inspection HTTP routes `jeryu-api` MUST serve
These are consumed by `HttpDataClient` (`runtime/data/http.rs`) and the SSE/poll
stream. Port the router from `src/inspection/router.rs` into `jeryu-api`
verbatim (paths unchanged; only the data assembler behind them is rewired):

| Method + path | Handler (src) | DataClient method | Backing `jeryu-*` |
|---|---|---|---|
| `GET /api/v1/read-model` | `inspection/read_model.rs` | `fetch_read_model` | assembler over jeryu-core/proof/cache/runnerd |
| `GET /api/v1/repos` | `inspection/repos.rs` | (repos lens) | `jeryu-core` repo registry |
| `GET /api/v1/families` | `inspection/repos.rs` | (repos lens) | `jeryu-core` repo families |
| `GET /api/v1/events` | `inspection/events.rs` | `fetch_events` (caps 500) | `jeryu-core` event ledger |
| `GET /api/v1/events/stream` (SSE) | `inspection/events.rs` | stream (`StreamMode::Sse`) | `jeryu-core` event stream |
| `GET /api/v1/entity/{kind}/{id}` | `inspection/entity.rs` | `fetch_entity` (400 unknown) | `jeryu-core` entity projection |
| `GET /api/v1/proof` | `inspection/proof.rs` | `fetch_proof` | `jeryu-proof` |
| `GET /api/v1/runtime/profile` | `inspection/health.rs` | `fetch_runtime_profile` | `jeryu-core` runtime profile |
| `GET /api/v1/health/deep` | `inspection/health.rs` | (system) | `jeryu-core` component health (`/system/runners` rolls up here) |
| `POST /api/v1/action/preview` | `inspection/actions.rs` | `preview_action` | `jeryu-core` action registry |
| `POST /api/v1/action/execute` | `inspection/actions.rs` | `execute_action` | `jeryu-core` action exec |
| `GET /api/v1/action/{run_id}/stream` | `inspection/actions.rs` | (action stream) | `jeryu-core` run stream |
| `GET /api/v1/action-registry` | `inspection/actions.rs` | `fetch_action_registry` (≥30) | `jeryu-tui` registry (34 entries) |

Note: the task names `/system/runners`, `/repos`, `/proof`, SSE `/events` — in
this codebase `/repos`,`/proof`, SSE `/events` exist as `/api/v1/repos`,
`/api/v1/proof`, `/api/v1/events/stream`; runner health is served via
`/api/v1/read-model` (`system.runners`/`RunnersDashboard`) and `/api/v1/health/
deep`. If a dedicated `GET /api/v1/system/runners` is desired, add it as a thin
projection of `RunnersDashboard` (non-breaking; `runners` lens can keep reading
the read-model field). Envelope shape (`InspectionEnvelope<T>`: `api_version`
=`api.v1`, `generated_at`, `sources`, `data`) is preserved.

---

## 4. Dependencies & Ordering

This subsystem is a CONSUMER. It blocks on Codex's core/engine renames +
persistence, which the TUI worker MUST NOT edit. Ordering:

1. **(blocks everything) D2 crate renames land in `/home/ubuntu/jeryuRUST`**:
   `forge-core→jeryu-core`, `gitd→jeryu-gitd`, `jitforge-api→jeryu-api`,
   `runnerd→jeryu-runnerd`, `cratevault*→jeryu-cache*`, `proofcore→jeryu-proof`,
   `agentbridge→jeryu-agentbridge`, `signrail→jeryu-signrail`,
   `ci-*→jeryu-ci-*`, `runner-*→jeryu-runner-*`, `mirrorvault→jeryu-mirror`.
   (Today these dirs still carry old names — confirmed in
   `/home/ubuntu/jeryuRUST/crates/`.)
2. **`jeryu-core` exposes a control-plane handle/trait** to replace
   `GitlabClient`: CI-run list, repo registry, PR/delivery snapshot, event
   ledger + SSE, action registry/exec, entity projection, component health.
   Until this exists, `jeryu-tui` can be ported in **fixture/HTTP-only mode**
   (lenses + `FixtureDataClient` + `HttpDataClient` against a stub `jeryu-api`),
   which is enough to make tuiwright green.
3. **Persistence kept (D3)**: jeryu's SQLite + RedlineDB `db/` layer is ported
   as-is into the fused repo; `TuiSession` stays. No blocker beyond it
   compiling under the renamed crates.
4. **`jeryu-readmodel` crate extracted** (contract types) — can be done by the
   TUI worker independently; it is pure types + serde, no engine dependency
   except `jeryu-proof`/`jeryu-core` enums it references (`RiskTier` is in
   `jeryu-tui::action_registry`, so keep that dependency edge `jeryu-readmodel →
   jeryu-tui::action_registry` OR move `RiskTier` into `jeryu-readmodel`).
5. **`jeryu-api` gains the inspection HTTP plane** (Codex owns engine crates;
   coordinate: the router/serve/state + handlers are TUI-contract code and may
   live in `jeryu-api` or a `jeryu-inspection` sub-crate to keep ownership
   clean). The 13 routes above must answer with the envelope.
6. **THEN** rewire `App::new` ctor + `app_runtime_sync.rs` + `flow/collector.rs`
   off `GitlabClient`/`DockerCtl` onto the `jeryu-core`/`jeryu-runnerd` handles.
7. **THEN** scrub the 132 `gitlab` literal occurrences + the `SystemHealth.gitlab`
   field (the only forbidden-literal in the contract) and verify zero-evidence.

Hard blockers: step 1 (renames) blocks compilation; step 2 (`jeryu-core` handle)
blocks LIVE mode but NOT the fixture/tuiwright port. Do steps 3–5 in parallel
with the verbatim lens move.

---

## 5. Tests / Acceptance Gate

Run from `/home/ubuntu/jeryuRUST` (crate `jeryu-tui`). Exact commands:

```bash
# A. Lens unit tests (pure projections) — must be green after the move:
cargo nextest run -p jeryu-tui --lib tui::lenses::
cargo nextest run -p jeryu-tui --lib -- tui::

# B. DataClient transport contract (HTTP envelope round-trips, object-safety):
cargo nextest run -p jeryu-tui --lib tui::runtime::data::
cargo nextest run -p jeryu-tui --lib tui::runtime::stream::   # StreamMode labels

# C. Read-model + dashboards contract:
cargo nextest run -p jeryu-readmodel
cargo test -p jeryu-api --lib inspection::          # router/serve/handlers

# D. tuiwright snapshot suite (PNG capture via the bin — serial, real PTY):
TERM=xterm-256color cargo test -p jeryu-tui --test tuiwright -- --test-threads=1

# E. Action registry size invariant (HTTP client asserts >= 30; product has 34):
cargo nextest run -p jeryu-tui --lib tui::action_registry

# F. Capture/screenshot CLI flag parity (clap):
cargo nextest run -p jeryu --lib -- cli            # --once/--demo/--capture/...
jeryu tui --capture --tab mission --output /tmp/m.png --width 120 --height 36
jeryu tui --screenshot --tab workflow --screenshot-hold-ms 200   # holds then cleans up
jeryu tui --once --tab queue                       # smoke render, prints "ok"
jeryu tui --demo --tab mission                      # demo fixtures
```

### Invariants (no-regression)
1. **tuiwright**: all primary tabs render PNGs with exact cell-grid dimensions
   (`cols*8 × rows*36→rows*12 px`), `>1000` non-background pixels, and ink in
   header/content/activity/footer regions (`assert_main_layout_regions`).
   Harness still shells `jeryu tui --capture` with `JERYU_DATABASE_URL=<sqlite
   memory>` and `tuiwright_lock` serialization.
2. **CLI flags preserved exactly**: `--once --demo --capture --screenshot --tab
   --output --width --height --screenshot-hold-ms` with the same defaults
   (tab=`jobs`/per-tab, width=140, height=44, hold=1100ms; tuiwright uses
   120×36). `tui-capture` bin flags unchanged.
3. **DataClient is the only backend surface** the lenses touch; lenses remain
   pure (`from_read_model` / no I/O). `dyn DataClient` stays object-safe.
4. **Envelope stable**: every `/api/v1/*` read route returns
   `InspectionEnvelope` with `api_version="api.v1"`; `read-model.data.schema_
   version` present; `events` caps at 500; unknown entity kind → 400.
5. **SSE→poll→degraded** fallback intact (`StreamMode` labels `SSE`/`[poll]`/
   `LAST KNOWN`/`WS` unchanged; header badge derives from `DegradedTimeline`).
6. **MCP tools-call / verdict-replay**: not present as a TUI test today (the MCP
   transport is a read-only stub `McpResourceDataClient`). Acceptance for those
   harnesses is N/A for this subsystem EXCEPT that `McpResourceDataClient` and
   `TraceDataClient` must still compile and return typed errors (degraded badge,
   not panic). Playwright applies to the React web (separate port spec), not the
   TUI; the TUI's "playwright" equivalent is tuiwright (command D).

### Zero-evidence gate (D1) — MUST be empty:
```bash
# No forbidden literals anywhere in the ported TUI + contract surface:
grep -rniE 'gitlab|jitforge|nitro' \
  crates/jeryu-tui crates/jeryu-readmodel \
  crates/jeryu-api/src/inspection 2>/dev/null
# Expect: 0 matches. (Source has 132 'gitlab' occurrences across 99 files today.)

# No MR/merge-request user-facing vocabulary (D4) outside comments:
grep -rniE 'merge.request|\bMR\b' crates/jeryu-tui/src 2>/dev/null
# Expect: only PR/PullRequest survives.
```

---

## 6. Risks & Hardest Seams

1. **`SystemHealth.gitlab` is a serialized contract field** (`read_model.rs:178`,
   default `"gitlab"` at :202, `read_model_health.rs:58`). Renaming it to `scm`
   changes the JSON wire shape AND every fixture/scenario/tuiwright snapshot that
   asserts on Source Doctor component rows. This is the single highest-churn
   D1 fix. Mitigation: rename field + all `MissionSnapshot`/`SystemHealth`
   fixtures + Source Doctor lens row order in one atomic change; regenerate
   tuiwright baselines.
2. **`App::new(store, docker, gitlab)` ctor signature change** ripples through
   `runner.rs` (5 entrypoints), `runner_tests.rs`, `app_runtime.rs`,
   `app_runtime_sync.rs`, and every test that builds an `App`. Replacing
   `GitlabClient` with a `jeryu-core` control-plane trait object requires that
   trait to exist first (§4 step 2) — until then, port behind a `#[cfg]`/fixture
   path so tuiwright stays green.
3. **`flow/collector.rs` + `flow/recovery.rs`** are the live pipeline poll. They
   decode GitLab job JSON directly (`gitlab_job_to_event`). Retyping to
   `jeryu-core` CI-run structs is real work, not a rename — the field shapes
   differ. Keep `FlowSnapshot` shape stable so the workflow Atlas/rails don't
   churn.
4. **"pipeline" is everywhere** (`TrackedPipeline`, `PipelineMetrics`,
   `PipelineProgressView`, `PipelineFlow`, `Pipeline(PipelineCommands)` CLI).
   Per the spec's "pipeline→ci run" rule, user-visible labels must change but
   wholesale type renames risk a huge diff. Decision: rename **user-visible
   strings + new wire types** to "CI run"; keep internal type names where a
   rename buys nothing, but ensure no `gitlab`-namespaced type leaks.
5. **`tui-capture` PTY determinism**: the renderer spawns the real binary in a
   PTY and rasterizes with a bundled font. Font discovery (`find_font`) and
   glyph validation must survive the move; PNG dimensions are asserted to the
   pixel. Any change to `CELL_W/CELL_H` or default font breaks every baseline.
6. **DockerCtl → jeryu-runnerd (D5 OCI-first)**: the runners lens + `cache_
   maintenance_loop` + `storage breakdown` assume a Docker controller. The
   runnerd OCI controller must expose equivalent `list_managed_containers`,
   `list_managers_for_node`, and storage-usage probes, else the Runners/Cache
   lenses degrade. Map `docker_*_bytes` → `sandbox_*_bytes`.
7. **Inspection plane ownership**: Codex owns engine crates; the inspection
   router is TUI-contract code. Land it in `jeryu-api` (or a `jeryu-inspection`
   sub-crate) without editing Codex's core modules — coordinate the boundary so
   the read-model assembler (which DOES read jeryu-core) is owned by whoever owns
   `jeryu-core`, while the router/handlers/envelope stay TUI-side.
8. **Action adapter production path**: `ProductionActionAdapter` performs real
   mutations; until `jeryu-core` action exec + `/api/v1/action/execute` are
   live, `try_install_production_adapter` must keep failing soft (FakeAdapter,
   "no adapter wired" per click) so the cockpit still renders.
