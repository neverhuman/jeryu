# JeRyu TUI Reset Plan — Claude's Edition

> **Target deliverable file**: `~/jeryu/TUI_RESET_PLAN_CLAUDE.md`
> **Author**: Claude Opus 4.7 (1M ctx)
> **Sibling plan**: Codex authored `TUI_RESET_PLAN.md` (741 lines, 26 units). Read it alongside this.
> **Source vision**: `tips/tui_reset/*.md` (8 design specs + 9 API inventories)
> **Standard**: `agent/JANKURAI_STANDARD.md` + `docs/agent-native-standard.md` + `agent/boundaries.toml` + `agent/proof-lanes.toml`
> **Baseline date**: 2026-05-26
> **Branch**: `feat/rustup-home-fix-v3.3.23` (work to be done on a new `feat/tui-reset` integration branch)

---

## 0. Where This Plan Differs From Codex's

Codex and this plan agree on most outcomes. The differences below are the reasons for reading this version.

| Topic | Codex | This Plan | Why this is better |
|---|---|---|---|
| Per-file LOC budgets | One global table for "normal / complex / test / fixture" | Per-output-file LOC cap in every unit (`≤ 200`, `≤ 250`, etc.) | Audit-able at PR review time; no judgement calls |
| Lens layout | Flat `pages/<name>.rs` files | Canonical lens directory: `mod.rs`, `view.rs`, `data.rs`, `nav.rs`, `tests.rs` + sub-components | Symmetry across 14 lenses; reading one teaches all |
| Parallelism | "Wave" table at high level | Explicit unit-dependency gantt; file-level conflict map | Two contributors can pick up units without coordination |
| Action tiers | "Evolve to R0-R5" (one line) | Explicit migration: `ReadOnly→R0`, `Low→R1/R2`, `High→R3/R4`, `Production→R4/R5` with alias preservation | Doesn't break the 37 existing action IDs |
| App state | Reducer + selectors (good) | Same, with explicit `Intent → Reducer → State → Selectors → View` flow + sketch | Same architecture, but with the seams pre-cut |
| Tuiwright | 1251-line file "to split" | Explicit 9-suite target with per-suite size cap and a unit-by-unit migration plan | Doesn't risk a single mega-PR |
| Backend `src/inspection/` | Listed as `src/inspection/` directory | Same layout; **but with risk register** noting daemon-team coordination is the blocker | Calls out the real risk |
| Open questions | None | Explicit at end | Surfaces decisions for the reviewer |
| Migration map | None | Current file → new destination table | Mechanical step for the executor |
| Anti-flicker | Mentioned | Codified as 5 invariants, each backed by a named tuiwright test | Verifiable, not aspirational |

This plan is **not** a critique — Codex's structure is solid and worth executing. The intent is to add the verifier-grade detail that Codex's plan leaves implicit.

---

## 1. Context: Why a Reset

### 1.1 The mess (current state, audited 2026-05-26)

| Metric | Value |
|---|---|
| `.rs` files under `src/tui/**` | 110 (Codex audited a different cut at 119; difference is in-flight `queue.rs` and modified-but-uncommitted set) |
| Total LOC under `src/tui/` | 29,464 |
| Files > 300 LOC (red zone) | 25 |
| Files > 500 LOC (egregious) | 14 |
| Largest single file | `workflow/widget.rs` @ 1,063 LOC |
| Largest builder | `workflow/delivery.rs` @ 1,058 LOC |
| Largest action surface | `workflow/action_adapter.rs` @ 1,003 LOC |
| Largest sync surface | `app_runtime_sync_actions.rs` @ 804 LOC |
| `App` struct field count | ~400 (single file: `app.rs` @ 466 LOC) |
| Existing tuiwright suite | `tests/tui_tuiwright.rs` @ **1,251 LOC** (centralized) |
| Existing recording suite | `tests/tui_recording.rs` @ ~220 LOC |
| Existing `src/api/` modules | 14 files, 1,616 LOC; contains `EntityKind` (20 variants), `TuiReadModel`, `TuiEventKind`, `actions`, `snapshot`, `agent_session`, `read_model_health` |
| Existing action registry entries | 37 entries, 4 tiers (`ReadOnly`/`Low`/`High`/`Production`) |
| Existing daemon HTTP routes | `GET /metrics`, `GET /health`, `POST /events` — no `/api/v1/*` |
| Existing MCP HTTP routes | `/mcp` POST/DELETE only — no resources, no GET |
| State DB default | **SQLite** (RedlineDB is feature-gated `redlinedb-backend` only) |

### 1.2 What's broken (quoting `tips/tui_reset/`)

- **Log polling, not streaming** — "650ms polling fallback; target WebSocket/SSE with graceful degradation".
- **Flow board first-active-pipeline bias** — multi-pipeline fleet is invisible.
- **Graph edges incomplete** — miss explicit `needs`, bridge/downstream, artifact, cache, VTI, security-gate deps; edge confidence (`explicit` vs `inferred`) not shown.
- **ETA heuristic only** — no confidence band; cannot distinguish `MEAS` / `STRUCT` / `HIST` / `HEUR` / `MISS`.
- **Evidence not searchable timeline** — proof ledger is fragmented; no unified query API.
- **Agents lack lifecycle table** — no dedicated `agent_sessions` / `agent_tasks` / `agent_steps`; data reconstructed from side effects; should display `INFERRED`.
- **MR webhooks logged, not acted on** — MR state is `PARTIAL` until ingestion lands.
- **No unified agent/autonomy view** — kill-bell, freeze, verdicts, launch-ledger, LLM budget spread across domains.
- **Cache / summary sparse** — lacks category breakdown, taint detail, GC plan, miss-storm attribution.
- **API drift** — `/cache/summary` auth docs stale; `ListAllowedActions` may lag registry; MCP lists hardcoded.
- **Header doesn't show actual backend** (SQLite vs RedlineDB ambiguity).
- **MCP HTTP has only tools, no resources / subscriptions** — agents can't safely observe.
- **No proof modal** for merge / release / secret / destructive operations.
- **No freshness badging** anywhere in the UI.

### 1.3 The vision (one paragraph)

A realtime terminal operating room for software delivery. Five nested scopes (Fleet → Family → Repo → Workflow/Pipeline → Job → Proof) browsable spatially. Every visible object is an `EntityRef`, drillable to its proof. Motion is truthful: animation only for live data; stale freezes and shows age. Status never lies — green requires proof. Mutations always preview their risk, side-effects, grants, and rollback target; merge/release/secret require typed confirmation. Stream-first WebSocket/SSE with explicit `[poll]` fallback. Single typed read model is the source of truth across TUI, CLI, MCP. Three capacity floors (physics / fleet / policy) and a SCREAM index quantify "how far from theoretical we are running." 14+ lenses with consistent shape, `g<letter>` shortcuts, breadcrumb scope, and an inspector that shadows main pane selection without stealing focus.

---

## 2. Goals

| # | Goal | Verified by |
|---|---|---|
| G1 | Every `.rs` file in `src/tui/**` ≤ 350 LOC (target ≤ 250). | `wc -l` audit in CI |
| G2 | Single typed `TuiReadModel` (extended from existing `src/api/`) is the only state the render layer reads. | Grep: no `sqlx::`, `reqwest::`, `std::fs::` under `src/tui/lenses/**` |
| G3 | All 14 lenses reachable via `g<letter>` and tab cycling: Global Mission (g0), Queue (gq), Repos (gr), Workflow (gw), Runners (gu), Cache (gc), VTI (gv), Agents (ga), Bugs (gb), Release (gR), Evidence (ge), Security (gS), Jankurai (gj), AER (gA), LLM (gl), Git Sync (gg), Source Doctor (gd). | tuiwright nav suite |
| G4 | High-risk actions (merge, release, secret, destructive cache, autonomy freeze) require proof modal with exact SHA / version / digest and typed confirmation. | tuiwright safety suite |
| G5 | Stream-first ingest (WebSocket/SSE) with explicit `[poll]` fallback badge when degraded. | Source Doctor lens + integration test |
| G6 | Freshness badges on every data family; worst-source freshness shown in header. | tuiwright header golden |
| G7 | Anti-flicker invariants preserved: empty snapshot does not blank; selection survives reorder; log follow-tail is sticky. | tuiwright `flicker_*` suite |
| G8 | Every lens carries ≥ 1 tuiwright golden snapshot, ≥ 1 nav test, ≥ 1 data-empty test, ≥ 1 stale-data test. | CI lane `cargo tuiwright` |
| G9 | Jankurai score ≥ 85 on every PR in the reset. | `just score` |
| G10 | Three-tier capacity model (physics / fleet / policy) and SCREAM index visible on Queue lens with explained math. | tuiwright golden of Queue / Limit screen |
| G11 | All 37 existing action registry IDs preserved (or aliased) — no breaking change to CLI/MCP/capability surfaces. | grep `action_registry_entries`; alias test |
| G12 | All existing `jeryu tui` CLI flags preserved (`--demo`, `--capture`, `--screenshot`, `--tab`, `--output`, `--width`, `--height`, `--screenshot-hold-ms`). | CLI smoke test |
| G13 | Backend inspection API serves `/api/v1/read-model`, `/api/v1/events`, `/api/v1/entity/{kind}/{id}`, `/api/v1/proof`, `/api/v1/action/preview`, `/api/v1/action/execute`, `/api/v1/action/{id}/stream`. | API integration tests |
| G14 | The new TUI runs end-to-end with empty backend (fixtures only) so backend and TUI can land in any order. | `jeryu tui --demo` smoke |
| G15 | SQLite stays default; RedlineDB feature-gated. Header shows actual backend. | `just runtime-sqlite-kafka` and `just runtime-redlinedb-jansu` profiles pass |

---

## 3. Non-Goals

- **No CLI breaking changes.** All `jeryu tui` flags preserved.
- **No new state DB.** Existing `src/db/` schema is the truth.
- **No new mutation paths.** All writes through action registry.
- **No reinvention of `src/api/`.** Existing `EntityKind`, `TuiReadModel`, `TuiEventKind`, `actions` modules are extended in place. We do not create parallel DTOs.
- **No theming changes outside semantic palette** in `tips/tui_reset/`.
- **No mouse-first interaction redesign.** Keyboard primary; mouse augmentation.
- **No replacing ratatui or crossterm.**
- **No web mirror.**

---

## 4. Design Principles (load-bearing)

1. **Only adapters touch raw systems.** Render reads `TuiReadModel`, not Git/GitLab/DB.
2. **No second mutation path.** All writes flow through `ActionAdapter::execute(action, args)` → registry → backend.
3. **Stream default, poll honestly.** WebSocket/SSE preferred. If degraded, show `[poll]` badge; never imply live motion on stale data.
4. **Every fact has provenance + freshness.** No green without proof.
5. **One contract surface.** `src/api/` is the only place `EntityKind`, `TuiReadModel`, `TuiEvent`, `Action` definitions live. TUI and inspection plane both consume them.
6. **Pane state is local; routing is global.** Focus stack, scope breadcrumb, and active tab are App-level; inspector/selection/scroll are per-pane.
7. **No modal dead ends.** Esc always pops; every detail card has parent breadcrumb + evidence link.
8. **Symmetry across lenses.** Same file layout, same hook points, same test shape.
9. **Cohesion > convention.** Files are small because each does one thing — not because of arbitrary line caps. Splits follow seams.
10. **Reducer-driven state.** Intents from input/sync/actions flow through a pure reducer to update App state; selectors derive view data. Render is pure projection.
11. **Fixture-first lens shipping.** Every lens renders from `TuiReadModel::demo_*()` before live wiring; backend can lag.
12. **Tests describe behavior, not internals.** tuiwright drives the TUI as a user; rendering tests assert glyphs and layout, not field names.

---

## 5. Existing Surfaces We Must Honor

Before slicing units, anchor the parts of the codebase we are **extending, not rewriting.**

### 5.1 `src/api/` (1,616 LOC across 14 files)

Already defines:

- `EntityKind` (20 variants — `Job`, `Pipeline`, `Agent`, `AgentTask`, `MergeRequest`, `TestPlan`, `TestCase`, `EvidenceCapsule`, `ReleaseAttempt`, `ReleaseGate`, `CacheTaint`, `CacheObject`, `Bug`, `BugAttempt`, `Project`, `SecretAccess`, `Grant`, `Pool`, `Runner`, `System`) — **extend** to add `Fleet`, `RepoFamily`, `Repo`, `Branch`, `Commit`, `PipelineBridge`, `Stage`, `RunnerPool`, `RunnerManager`, `Node`, `CacheRequest`, `CacheVerdict`, `SelectorMiss`, `AgentSession`, `AgentRace`, `GitRefUpdate`, `AdmissionDecision`, `CapabilityIntent`, `CapabilityGrant`, `JankuraiRun`, `JankuraiFinding`, `SecurityFinding`, `SecretAuthority`, `ReleaseSecretSet`, `Artifact`, `Signature`, `Sbom`, `Evidence`, `AuditEvent`, `LlmCall`, `Source`, `Action`. Total target: ~50 variants.
- `TuiReadModel` — extend with `repo_families`, `repos`, `workflow`, `queue`, `runners`, `cache`, `vti`, `agents`, `autonomy`, `bugs`, `git_sync`, `bottlenecks`, `jankurai`, `security`, `artifacts`, `release`, `evidence`, `llms`, `source_doctor`. Preserve existing `schema_version`, `generated_at`, `event_cursor`, `freshness`, `mission`, `attention`, `next_action`, `system`.
- `TuiEventKind` — extend with cache-verdict, selector-miss, agent-session/race, admission-decision, capability-grant, artifact-attested, source-resumed, stream-degraded events.
- `actions.rs` — 37 entries, 4 tiers; **migrate** to R0..R5 tiers with aliases.
- `read_model_health.rs` — already has freshness; **extend** with `SourceKind`, per-source `last_error`, cursor, TTL, degraded reason.
- `snapshot.rs` — keep.

### 5.2 Action Registry (37 entries, 4 tiers)

Existing IDs to **preserve verbatim**:
```
open_logs, requeue_job, remove_record, pause_pool, explain_blockers,
fetch_capsule, get_system_snapshot, get_pipeline_jobs, get_ci_bottlenecks,
propose_patch, race_patches, request_merge, plan_validation,
bug_submit, bug_list, bug_show, bug_ready, bug_update, bug_record_attempt,
run_tests, next_action, toggle_audit_ledger, quit
```
…plus tab-navigation actions.

**Tier migration map** (with `#[serde(alias=...)]`):
```
ReadOnly   → R0  (read_only)
Low        → R1  (local_write)  or  R2 (ci_write)   per-action
High       → R3  (repo_write)   or  R4 (merge/release)
Production → R4  (release)      or  R5 (destructive/secret/production)
```

A unit-test enforces every existing ID still resolves and every action has a defined tier in the new scheme.

### 5.3 Existing tuiwright suite (`tests/tui_tuiwright.rs` @ 1251 LOC)

Already covers (in one mega-file):
- Capture
- Primary tabs render
- Bugs
- Workflow focus + drilldown
- Fleet bar
- Jankurai
- Overlays
- Command palette
- Repo discovery

**Strategy**: split into 9 suites under `tests/tuiwright/*.rs`, preserving every assertion; add new suites for stream/safety/source-doctor/perf as new units. **No assertion is deleted in the split.**

### 5.4 CLI flags (preserved)

```
jeryu tui                      # default interactive
jeryu tui --demo               # fixture-only render
jeryu tui --capture            # write PNG and exit
jeryu tui --screenshot         # full snapshot
jeryu tui --tab=<name>         # start on a specific tab
jeryu tui --output=<path>      # screenshot output file
jeryu tui --width=N            # capture width
jeryu tui --height=N           # capture height
jeryu tui --screenshot-hold-ms=N  # delay before capture
```

### 5.5 DB profile

- `SQLite` is **default**.
- `RedlineDB` is feature-gated via `--features redlinedb-backend` and explicit Redline URL/profile.
- TUI header shows actual backend (`Backend: SQLite v3.45 (default)` or `Backend: RedlineDB v0.4.2 (feature)`).
- No new SQLite-direct code outside `src/db/`.

---

## 6. Target Architecture

### 6.1 Module Map (top level)

```
src/
├── api/                    # CONTRACTS (extended, not rewritten)
│   ├── entity.rs           # EntityKind (extend to ~50 variants)
│   ├── read_model.rs       # TuiReadModel (extend with dashboards)
│   ├── events.rs           # TuiEventKind (extend with new kinds)
│   ├── actions.rs          # Action, RiskTier (R0..R5 + alias map)
│   ├── snapshot.rs         # existing
│   ├── freshness.rs        # NEW: SourceFreshness, SourceKind, StalenessClass
│   ├── proof.rs            # NEW: ProofRef, EvidenceRef
│   ├── capacity.rs         # NEW: PhysicsFloor, FleetFloor, PolicyFloor, ScreamIndex
│   ├── runtime_profile.rs  # NEW: backend kind, version, build SHA, feature flags, schema hash
│   └── dashboards/         # NEW: per-lens view types (FleetView, QueueView, etc.)
│       ├── mod.rs
│       ├── fleet.rs
│       ├── queue.rs
│       ├── runners.rs
│       └── ...one per lens
│
├── inspection/             # NEW: backend inspection plane (separate from TUI)
│   ├── mod.rs
│   ├── router.rs           # /api/v1/* router
│   ├── read_model.rs       # GET /api/v1/read-model
│   ├── events.rs           # GET /api/v1/events + GET /api/v1/events/stream (SSE)
│   ├── entity.rs           # GET /api/v1/entity/{kind}/{id}
│   ├── proof.rs            # GET /api/v1/proof?...
│   ├── health.rs           # GET /api/v1/health/deep, /api/v1/runtime/profile
│   ├── action.rs           # GET /action-registry, POST /preview, POST /execute, GET /action/{id}/stream
│   ├── streams.rs          # SSE / WS infrastructure
│   └── projections/        # per-domain projection builders
│       ├── repos.rs
│       ├── workflow.rs
│       ├── queue.rs
│       └── ...
│
└── tui/                    # PRESENTATION ONLY
    ├── app/                # App state + reducer + selectors
    ├── runtime/            # event loop, input, render driver, sync, stream
    ├── ui/                 # top-level layout + chrome + overlays
    ├── focus/              # focus state machine
    ├── theme/              # palette + glyphs + badges + progress
    ├── widgets/            # reusable ratatui components
    ├── action/             # ActionAdapter trait + proof gate orchestration
    ├── lenses/             # one folder per tab
    │   ├── _template/      # canonical lens layout (README only)
    │   ├── global_mission/
    │   ├── queue/
    │   ├── repos/
    │   ├── workflow/
    │   ├── logs/
    │   ├── runners/
    │   ├── cache/
    │   ├── vti/
    │   ├── agents/
    │   ├── autonomy/
    │   ├── bugs/
    │   ├── git_sync/
    │   ├── bottlenecks/
    │   ├── jankurai/
    │   ├── aer/
    │   ├── security/
    │   ├── artifacts/
    │   ├── release/
    │   ├── evidence/
    │   ├── llms/
    │   ├── settings/
    │   ├── source_doctor/
    │   ├── churn/
    │   ├── incident/
    │   └── replay/
    ├── testing/            # #[cfg(test)] tuiwright + fixtures + goldens
    └── runner/             # entry points (run_tui, run_tui_once, capture)
```

### 6.2 Reducer / State / Selectors (App architecture)

```
        ┌──────────────────────────────────────────┐
        │     Intents (sources of change)          │
        │  • Input::Key / Input::Mouse             │
        │  • Sync::ReadModelArrived(TuiReadModel)  │
        │  • Stream::EventArrived(TuiEvent)        │
        │  • Action::Receipt(ActionReceipt)        │
        │  • Stream::Degraded(reason)              │
        └──────────────────┬───────────────────────┘
                           │
                           ▼
                ┌────────────────────┐
                │     reducer(s)     │  pure: (AppState, Intent) -> AppState
                └─────────┬──────────┘
                           │
                           ▼
                ┌────────────────────┐
                │     AppState       │  owned by App; never mutated outside reducer
                │  • read_model      │
                │  • route_stack     │
                │  • focus_state     │
                │  • selection_by_pane │
                │  • filter (scope)  │
                │  • pending_action  │
                │  • streams_status  │
                │  • diagnostics     │
                └─────────┬──────────┘
                           │
                           ▼
                ┌────────────────────┐
                │    selectors       │  pure: AppState -> LensInput
                │ (per lens, in      │
                │  lenses/*/data.rs) │
                └─────────┬──────────┘
                           │
                           ▼
                ┌────────────────────┐
                │     render         │  ratatui draw, pure projection of LensInput
                └────────────────────┘
```

- **Reducer files**: one root reducer in `app/reducer.rs` delegates to per-domain reducers (`app/reducers/{focus,filter,action,stream,read_model}.rs`).
- **State immutability**: each reducer returns `AppState` by value; cloning is cheap because hot data lives behind `Arc`.
- **Determinism**: every reducer is tested with `(state, intent) -> state` table tests.
- **No I/O in reducer**: I/O lives in the runtime layer that produces intents.

### 6.3 Canonical Lens Template

Every lens lives at `src/tui/lenses/<name>/` and has **exactly** these files. Anyone reading one lens can navigate any other.

| File | LOC budget | Purpose |
|---|---|---|
| `mod.rs` | ≤ 80 | Re-exports + module declarations. No logic. |
| `view.rs` | ≤ 250 | `pub fn draw(frame, state, area)` — orchestrates sub-components. Reads `LensInput` from selectors. |
| `data.rs` | ≤ 200 | `pub fn select(state: &AppState) -> LensInput` — pure selector. |
| `nav.rs` | ≤ 200 | `pub fn handle_key(key, &LensInput) -> Option<Intent>`, `pub fn handle_mouse(...)`. |
| `tests.rs` | ≤ 300 | unit + render tests; ≥ 1 tuiwright golden; ≥ 1 nav assertion; ≥ 1 empty-state assertion; ≥ 1 stale-data assertion. |

Sub-components (e.g., `canvas.rs`, `inspector/`, `pulse.rs`) live as siblings with their own ≤ 250 LOC budget. If a lens grows past one sub-component file, **split don't expand**.

### 6.4 Data Client (transport-agnostic)

`runtime/data/` owns transport selection:

```
trait DataClient {
    async fn fetch_read_model(&self) -> Result<TuiReadModel>;
    async fn subscribe_events(&self, cursor: EventCursor) -> EventStream;
    async fn fetch_entity(&self, kind: EntityKind, id: &str) -> Result<EntityDetail>;
    async fn fetch_proof(&self, query: ProofQuery) -> Result<ProofTimeline>;
    async fn action_preview(&self, action_id: &str, args: ActionArgs) -> Result<ActionPreview>;
    async fn action_execute(&self, action_id: &str, args: ActionArgs) -> Result<ActionReceipt>;
}

impls:
  HttpDataClient   // /api/v1/* via reqwest
  McpDataClient    // jeryu://* resources (after backend lands)
  LocalDataClient  // direct DB read (degraded fallback)
  FixtureDataClient // for --demo and tests
```

Transport selection at startup; UI never knows which is active. Header shows the active transport.

### 6.5 Streaming Pipeline

```
runtime/stream/mod.rs orchestrates:
  1. Try WebSocket /api/v1/events/stream (SSE in disguise via Streamable HTTP)
  2. On failure: SSE plain (GET text/event-stream)
  3. On failure: HTTP polling every 1s with [poll] header badge
  4. Hard fail: CLI JSON dump every 5s, header shows "DEGRADED — CLI fallback"

Reconnect:
  - emits Stream::Resumed { last_cursor }
  - runtime fetches gap via /api/v1/events?since=cursor
```

### 6.6 Theme + Glyphs + Badges

```
Palette (semantic, truecolor-first with 256/16 fallbacks):
  ok       — green     #50FA7B    running  — cyan      #8BE9FD
  queued   — amber     #FFB86C    warn     — amber-2   #F1FA8C
  crit     — red       #FF5555    agent    — purple    #C792EA
  cache    — teal      #7FDBCA    vti      — green-2   #69DD8E
  release  — magenta   #FF79C6    evidence — gold      #F8C471
  stale    — gray-1    #6E7681    unknown  — gray-2    #4A5057

Glyphs:
  ● running   ▶ active   ○ queued   … waiting
  ✓ passed    ✗ failed   ⛔ blocked  ↷ skipped/VTI
  ◆ proof     ⏸ paused   ⚿ grant    ⬢ release
  ! warn      ~ stale

Freshness badges (text + color, no animation when stale):
  LIVE        bright cyan, animated dot
  FRESH <age> cyan, no animation
  STALE <age> dimmed amber
  LAST KNOWN  dimmed gray
  INFERRED    dotted outline, gray
  UNKNOWN     gray italic
  NO PROOF    red outline
  UNVERIFIED  amber outline
  [poll]      tiny gray suffix on header

Risk badges (corner-of-card on actions):
  R0 ▸ read    R1 ◇ local   R2 ◉ ci
  R3 ▣ repo    R4 ▮ release R5 ✦ destructive
```

### 6.7 Action Path (with proof gate)

```
User selects entity → presses action key
  → lens emits Intent::ActionRequested { action_id, target }
  → reducer transitions AppState.pending_action
  → runtime calls data_client.action_preview(...)
  → preview returned with RiskTier, side effects, grants, expected evidence
  → router opens proof_modal if tier ≥ R3
  → tier-specific gate:
       R0/R1 → execute immediately, show toast
       R2    → preview modal, Enter to confirm
       R3    → preview modal + named confirmation
       R4    → proof modal: target SHA + grants + typed confirm
       R5    → proof modal + dry-run + secondary approver + typed
  → data_client.action_execute(...)
  → receipt returned; intent Stream::ActionFinished
  → reducer updates state; new read_model refresh shows receipt
```

The proof modal is one widget (`ui/overlays/proof_modal.rs`) reused for all R3+ actions.

---

## 7. Jankurai Compliance

### 7.1 File Headers (mandatory)

```rust
//! Owner: Interactive TUI subsystem — <specific concern>
//! Proof: `cargo nextest run -p jeryu -- <test pattern>`
//! Invariants: <key invariant in one sentence>
```

### 7.2 File Size Standard

| File type | Target | Hard cap | Notes |
|---|---:|---:|---|
| Normal implementation | 150–250 | 350 | Above 250 needs a sibling-split plan |
| Complex renderer / model | 250–350 | 450 | Renderers can be denser; still split when seams emerge |
| Test file | 200–350 | 500 | Allowed to be denser; assertions cluster |
| Fixture file | n/a | 500 | Prefer split by scenario; this matches Codex's standard |
| Generated / declared artifacts | document | n/a | Must carry `Waiver:` line |

A CI lint compares each touched file's LOC to the table. Over-cap touches a file without splitting → block PR.

### 7.3 Forbidden Imports in `src/tui/**`

```
std::fs           — flows through App / Db / adapters / DataClient
std::env          — flows through Config (loaded at startup)
std::net          — flows through DataClient / stream client
std::time::SystemTime — use Instant + freshness model
rand::            — flows through deterministic seeds in tests
sqlx::, diesel::  — flows through DataClient::Local only
reqwest::         — flows through DataClient::Http only
jansu::           — flows through ActionAdapter
tracing::, log::  — flows through App::log_event
```

Exemption: `src/tui/runner/` may touch `std::env`, `std::fs` for terminal restore + screenshot path; documented in its header.

### 7.4 Anti-Flicker Invariants (verified by `tuiwright_flicker.rs`)

1. **No blank-screen on empty refresh**: when a snapshot arrives empty for a domain, keep the prior non-empty view and mark it `stale: true` with age. — `flicker::empty_preserves_prior`
2. **Selection survives reorder**: if a job list reorders, the selected `EntityRef` follows by id, not by index. — `flicker::selection_id_anchored`
3. **Log follow-tail is sticky**: scroll-to-bottom mode persists across snapshot updates. — `flicker::log_tail_sticky`
4. **Stale dims, never deletes**: stale data dims (`stale` palette) and shows age badge; never replaced by empty. — `flicker::stale_dims_not_blanks`
5. **Frame stability under burst**: 100 events/second does not cause the selected row to jump. — `flicker::burst_no_jump`

### 7.5 Owner Map / Proof Lanes

- All new files routed to `evidence-gate` owner.
- Test files routed to `tools`.
- `src/inspection/**` routed to `daemon-team` owner (new owner, add to `agent/owner-map.json`).
- `src/api/**` routed to `contracts` owner (shared between daemon-team and evidence-gate).
- Each PR includes proof lane (`leaf-bugfix`, `state-change`, `cross-module`, `full`).
- TUI smoke (`cargo run -p jeryu -- tui --once`) stays under 2 s; capture under 8 s.

---

## 8. Migration Strategy

**Coexistence, not big-bang.** The reset lands as a parallel module tree under `src/tui/` (new layout) and `src/inspection/` (new backend plane). The old `src/tui/{ui.rs, app.rs, workflow/widget.rs, ...}` remain compiling until each is replaced. A feature flag `jeryu_tui_reset` toggles routing.

Order within each migration:
1. **Stub**: create new module with empty types and a minimal `draw()` returning placeholder.
2. **Tests-first**: write tuiwright golden + nav + empty + stale tests against the stub.
3. **Port**: move logic from old file to new module(s), splitting along seams.
4. **Wire**: route `ui::draw` to the new lens when the flag is set.
5. **Verify**: tuiwright green; manual smoke; golden updated.
6. **Delete**: remove the old file in a follow-up PR with no behavior change.

A single migration unit covers steps 1–5 for one lens (or one Workflow sub-area). Step 6 is bundled into the final cleanup unit.

---

## 9. Units of Work

**28 units across 5 tracks.** Each ends in a PR. Each carries:

- **ID** — `U<track>.<n>`
- **Goal** — one sentence
- **Inputs** — units that must precede it
- **Outputs** — new/modified files with LOC budget
- **Test surface** — required assertions
- **Acceptance** — verifier-readable checklist
- **Owner suggestion** — `evidence-gate` / `tools` / `daemon-team` / `contracts`
- **Effort** — S (≤ 1 day), M (1–2 days), L (3–5 days)

---

### Track 0 — Foundation Lock-In (1 unit, blocking)

#### U0.1 — Module Manifest & Header Templates

- **Goal**: Lay down the entire new directory skeleton so Track 1+ units land in parallel without merge conflicts on `tui.rs` or `app.rs`.
- **Inputs**: none.
- **Outputs**:
  - `src/tui.rs` — rewritten as a minimal re-export root (≤ 80 LOC) with new module list under `cfg(feature = "tui_reset")`; existing modules compile when flag is off.
  - `src/tui/{runner,app,runtime,ui,focus,theme,widgets,action,lenses,testing}/mod.rs` — empty manifests (≤ 30 LOC each).
  - All 24 lens directories created with empty `mod.rs`, `view.rs`, `data.rs`, `nav.rs`, `tests.rs` stubs that compile.
  - `src/api/freshness.rs`, `src/api/proof.rs`, `src/api/capacity.rs`, `src/api/runtime_profile.rs`, `src/api/dashboards/mod.rs` — type stubs.
  - `src/inspection/mod.rs` — empty manifest under `cfg(feature = "tui_reset")`.
  - `src/tui/lenses/_template/README.md` — canonical lens layout doc.
  - `Cargo.toml` — add `tuiwright`, `insta`, `pretty_assertions`, `httpmock` as `[dev-dependencies]`; add `tui_reset` feature.
  - `agent/owner-map.json` — route new dirs.
  - `agent/boundaries.toml` — add `[tui]` forbidden-imports list.
  - `scripts/loc_audit.sh` — file-size lint (used by CI).
- **Test surface**: `cargo check -p jeryu --features tui_reset` compiles; LOC lint script returns 0 on stubs.
- **Acceptance**:
  - `cargo build --features tui_reset` succeeds with zero warnings.
  - `find src/tui -name 'mod.rs' | wc -l` ≥ 35.
  - Owner map lints clean.
- **Owner**: evidence-gate.
- **Effort**: M.

---

### Track 1 — Contracts & Inspection Plane (5 units, parallel after U0.1)

These units extend `src/api/` and stand up the backend inspection plane. Without these, the TUI has no data source other than fixtures.

#### U1.1 — `src/api/` Contract Expansion

- **Goal**: Extend `EntityKind`, `TuiReadModel`, `TuiEventKind` to the full fleet graph without breaking existing serialization.
- **Inputs**: U0.1.
- **Outputs**:
  - `src/api/entity.rs` (≤ 300) — extended `EntityKind` to ~50 variants; all existing variants retained.
  - `src/api/read_model.rs` (≤ 300) — extended `TuiReadModel` with `repo_families`, `repos`, `workflow`, `queue`, `runners`, `cache`, `vti`, `agents`, `autonomy`, `bugs`, `git_sync`, `bottlenecks`, `jankurai`, `security`, `artifacts`, `release`, `evidence`, `llms`, `source_doctor` fields. Sub-types in `src/api/dashboards/{repos,queue,runners,cache,vti,agents,bugs,release,evidence,...}.rs` (≤ 250 each).
  - `src/api/events.rs` (≤ 250) — extended `TuiEventKind` with new categories.
- **Test surface**: serde roundtrip every variant; backward-compat test confirms existing JSON still deserializes.
- **Acceptance**: no existing test in `src/api/**_tests.rs` regresses; new variants have round-trip tests.
- **Owner**: contracts.
- **Effort**: L.

#### U1.2 — Freshness, Proof, Capacity, Runtime Profile

- **Goal**: First-class freshness + provenance + capacity math types.
- **Inputs**: U0.1.
- **Outputs**:
  - `src/api/freshness.rs` (≤ 250) — `SourceKind` (GitLab, Db, Docker, Cache, Vault, Broker, Autonomy, McpHttp, McpWs, StateDb, Webhook, ActionRegistry, Capability, Jankurai, Security), `SourceFreshness { kind, observed_at, latency_p50, cursor, last_error, ttl, degraded_reason }`, `StalenessClass`. Helper: `worst_of(...)`.
  - `src/api/proof.rs` (≤ 200) — `ProofRef`, `EvidenceRef`, `ProofTimeline`, `ProofQuery`.
  - `src/api/capacity.rs` (≤ 250) — `PhysicsFloor`, `FleetFloor`, `PolicyFloor`, `ScreamIndex`, `BottleneckClass`.
  - `src/api/runtime_profile.rs` (≤ 200) — `RuntimeProfile { backend, version, build_sha, schema_version, action_registry_hash, mcp_manifest_hash, feature_flags, redacted_config }`.
  - Unit tests in each module.
- **Test surface**: classifier maps ages → classes; `worst_of` picks correctly; redaction tests for `redacted_config`.
- **Owner**: contracts.
- **Effort**: M.

#### U1.3 — Action Tier Migration (R0..R5)

- **Goal**: Migrate 4-tier action model to R0..R5, preserve all 37 IDs.
- **Inputs**: U0.1.
- **Outputs**:
  - `src/api/actions.rs` (≤ 250) — `RiskTier` enum `R0..R5`; existing `ReadOnly/Low/High/Production` aliased via `#[serde(alias=...)]`.
  - `src/tui/action_registry_entries.rs` — re-classified entries (no new IDs); LOC ≤ 350 (was 344).
  - `src/api/actions_tests.rs` (≤ 200) — table test: every existing ID resolves, every tier mapping documented.
- **Test surface**: id-resolution test; alias deserialization; CI parity test with capability/MCP registries.
- **Acceptance**: no existing call site breaks; CLI/MCP/capability surfaces still resolve every action.
- **Owner**: contracts.
- **Effort**: M.

#### U1.4 — Inspection API Foundation (`src/inspection/`)

- **Goal**: Stand up `/api/v1/*` routes serving the read model and event stream.
- **Inputs**: U1.1, U1.2, U1.3.
- **Outputs**:
  - `src/inspection/mod.rs` (≤ 100), `router.rs` (≤ 200) — axum-style router for `/api/v1/*`.
  - `src/inspection/read_model.rs` (≤ 250) — `GET /api/v1/read-model`.
  - `src/inspection/events.rs` (≤ 250) — `GET /api/v1/events` paged + `GET /api/v1/events/stream` SSE.
  - `src/inspection/entity.rs` (≤ 200) — `GET /api/v1/entity/{kind}/{id}`.
  - `src/inspection/proof.rs` (≤ 200) — `GET /api/v1/proof?...`.
  - `src/inspection/health.rs` (≤ 200) — `GET /api/v1/health/deep`, `GET /api/v1/runtime/profile`.
  - `src/inspection/action.rs` (≤ 250) — `GET /api/v1/action-registry`, `POST /api/v1/action/preview`, `POST /api/v1/action/execute`, `GET /api/v1/action/{id}/stream`.
  - `src/inspection/streams.rs` (≤ 250) — SSE/WS infra.
  - `src/inspection/projections/{repos,workflow,queue,runners,cache,vti,agents,bugs,release,evidence,...}.rs` (≤ 250 each) — per-domain projection builders that produce the dashboard substructs.
- **Test surface**: integration tests with in-memory SQLite; cursor monotonicity; SSE reconnect; auth/redaction.
- **Acceptance**: A test client can fetch `/api/v1/read-model` and `/api/v1/entity/*` against SQLite fixture data.
- **Owner**: daemon-team.
- **Effort**: L.

#### U1.5 — Action Preview / Execute / Stream Backend

- **Goal**: Wire the inspection API's `POST /action/preview`, `POST /action/execute`, `GET /action/{id}/stream` to the existing action registry + capability dispatcher.
- **Inputs**: U1.3, U1.4.
- **Outputs**:
  - `src/inspection/action.rs` extended (≤ 350) — preview/execute/stream handlers.
  - `src/action_runtime/` (if not already present) — adapter to existing dispatchers.
- **Test surface**: preview returns risk + grants; execute writes receipt; stream produces lifecycle events; cancellation does not mutate.
- **Acceptance**: every R3+ action exercised by an integration test.
- **Owner**: daemon-team.
- **Effort**: L.

---

### Track 2 — Pure TUI Foundations (6 units, parallel after Track 1 except U2.6)

#### U2.1 — Theme System

- **Goal**: Single source of truth for palette, glyphs, badges, progress; lenses never hardcode colors.
- **Inputs**: U0.1.
- **Outputs**:
  - `src/tui/theme/palette.rs` (≤ 150) — `Palette` struct + color constants + truecolor/256/16 fallback.
  - `src/tui/theme/glyphs.rs` (≤ 100) — `Glyph` enum + `glyph_for(WorkflowStatus)` helpers.
  - `src/tui/theme/badges.rs` (≤ 250) — `FreshnessBadge`, `ProofBadge`, `RiskBadge`, `DegradedBadge`, `PollBadge`.
  - `src/tui/theme/progress.rs` (≤ 200) — `ProgressBar { value, kind: Confidence::{Meas,Struct,Hist,Heur,Miss,Stale,Unknown} }`.
  - `src/tui/theme/terminal_caps.rs` (≤ 150) — truecolor/256/16/ASCII detection.
  - `src/tui/theme/mod.rs` (≤ 80) — `Theme` aggregate.
- **Test surface**: render every glyph + badge; truecolor + 256 + ASCII tests; golden snapshot.
- **Owner**: cockpit-theme.
- **Effort**: S.

#### U2.2 — Focus Refactor

- **Goal**: Split current `focus.rs` (479 LOC) into cohesive sub-files; preserve API.
- **Inputs**: U0.1.
- **Outputs**:
  - `src/tui/focus/pane.rs` (≤ 200) — `PaneId` enum (all current variants).
  - `src/tui/focus/state.rs` (≤ 200) — `FocusState`, stack ops, history.
  - `src/tui/focus/map.rs` (≤ 200) — `FocusMap` hit registry.
  - `src/tui/focus/chrome.rs` (≤ 150) — `PaneChrome` border/style.
  - `src/tui/focus/graph.rs` (≤ 200) — focus-graph (macro/micro) per Codex's design.
  - `src/tui/focus/mod.rs` (≤ 60) — re-exports.
- **Test surface**: existing focus tests pass; new tests for stack push/pop/Esc; focus-graph traversal.
- **Owner**: evidence-gate.
- **Effort**: M.

#### U2.3 — Widgets Baseline

- **Goal**: Reusable widgets used across lenses, decoupled from any specific data.
- **Inputs**: U2.1.
- **Outputs**:
  - `src/tui/widgets/header.rs` (≤ 250)
  - `src/tui/widgets/tabs.rs` (≤ 200)
  - `src/tui/widgets/status_strip.rs` (≤ 200)
  - `src/tui/widgets/freshness_chip.rs` (≤ 150)
  - `src/tui/widgets/attention.rs` (≤ 200)
  - `src/tui/widgets/virtual_table.rs` (≤ 300) — for 10k-row tables
  - `src/tui/widgets/inspector_card.rs` (≤ 200)
  - `src/tui/widgets/timeline.rs` (≤ 200)
  - `src/tui/widgets/dag.rs` (≤ 300) — generic DAG canvas (used by Workflow)
  - `src/tui/widgets/progress_bar.rs` (≤ 200) — with confidence band
  - `src/tui/widgets/sparkline.rs` (≤ 150)
  - `src/tui/widgets/heatmap.rs` (≤ 250)
  - `src/tui/widgets/event_tape.rs` (≤ 250)
  - `src/tui/widgets/command_palette.rs` (≤ 300)
  - `src/tui/widgets/help.rs` (≤ 200)
  - `src/tui/widgets/modal.rs` (≤ 200) — generic dialog frame
  - `src/tui/widgets/log_viewer.rs` (≤ 300) — cursor-aware, bounded
  - `src/tui/widgets/proof_chip.rs` (≤ 150)
  - `src/tui/widgets/forms.rs` (≤ 250)
  - `src/tui/widgets/entity_link.rs` (≤ 120)
- **Test surface**: render each widget at canonical sizes; golden snapshots; ASCII-fallback tests.
- **Owner**: cockpit-theme.
- **Effort**: L.

#### U2.4 — Action Adapter Skeleton + Proof Gate Plumbing

- **Goal**: TUI-side action layer; trait definition; preview struct; gate orchestration plumbing (modal in U5.1).
- **Inputs**: U1.3.
- **Outputs**:
  - `src/tui/action/mod.rs` (≤ 120) — `ActionAdapter` trait.
  - `src/tui/action/risk.rs` (≤ 150) — local re-export of `api::RiskTier` + UI styling.
  - `src/tui/action/preview.rs` (≤ 250) — `ActionPreview` struct + builders.
  - `src/tui/action/registry.rs` (≤ 250) — local cache of action-registry fetched from `/api/v1/action-registry`.
  - `src/tui/action/gate.rs` (≤ 200) — tier-routing skeleton (the modal itself lands in U5.1).
  - `src/tui/action/fake.rs` (≤ 150) — `FakeActionAdapter` for tests.
  - `src/tui/action/prod.rs` (≤ 250) — `ProdActionAdapter` using DataClient.
- **Test surface**: every action id resolves to a tier; fake adapter returns deterministic receipt; tier-routing test.
- **Owner**: evidence-gate.
- **Effort**: M.

#### U2.5 — tuiwright Harness + Suite Split

- **Goal**: Make tuiwright tests trivial to author **and** split the existing 1251-LOC monolith into focused suites.
- **Inputs**: U0.1.
- **Outputs**:
  - `src/tui/testing/tuiwright.rs` (≤ 250) — `TuiwrightSession::new()` / `.key()` / `.mouse()` / `.type()` / `.snap("name")`.
  - `src/tui/testing/backend.rs` (≤ 150) — `TestBackend` constructor at canonical sizes (80×24, 100×30, 120×36, 160×48, 220×60).
  - `src/tui/testing/fixtures.rs` (≤ 300) — `TuiReadModel::demo_*()` constructors (12 scenarios from Codex's U12: healthy, degraded, saturated, stale, release-blocked, security-blocked, cache-pressure, vti-miss, agent-race, bug-ready, jankurai-regression, incident).
  - `src/tui/testing/fixtures/<lens>.rs` — per-lens scenarios (extended in each lens unit).
  - `src/tui/testing/golden/mod.rs` (≤ 150) — golden-file loader/writer, insta integration.
  - `src/tui/testing/matchers.rs` (≤ 200) — `assert_has_glyph`, `assert_pane_focused`, `assert_freshness_badge`, `assert_no_secret_visible`.
  - **Split** `tests/tui_tuiwright.rs` (1251 LOC) into:
    - `tests/tuiwright/capture.rs` (≤ 250)
    - `tests/tuiwright/tabs.rs` (≤ 250)
    - `tests/tuiwright/bugs.rs` (≤ 250)
    - `tests/tuiwright/workflow.rs` (≤ 300)
    - `tests/tuiwright/fleet_bar.rs` (≤ 200)
    - `tests/tuiwright/jankurai.rs` (≤ 200)
    - `tests/tuiwright/overlays.rs` (≤ 250)
    - `tests/tuiwright/palette.rs` (≤ 200)
    - `tests/tuiwright/discovery.rs` (≤ 200)
  - `.cargo/config.toml` — `[alias] tuiwright = "nextest run -p jeryu --features tuiwright"`.
- **Test surface**: split preserves every assertion (verified by `grep '#\[test\]'` count); smoke test runs.
- **Acceptance**: `cargo tuiwright` runs in CI lane; no test deleted; LOC under cap per file.
- **Owner**: tools.
- **Effort**: L.

#### U2.6 — Data Client + Stream Pipeline

- **Goal**: Transport-agnostic data client; WebSocket → SSE → poll → CLI fallback.
- **Inputs**: U1.4, U1.5.
- **Outputs**:
  - `src/tui/runtime/data/mod.rs` (≤ 120) — `DataClient` trait.
  - `src/tui/runtime/data/http.rs` (≤ 250)
  - `src/tui/runtime/data/mcp.rs` (≤ 200)
  - `src/tui/runtime/data/local.rs` (≤ 250)
  - `src/tui/runtime/data/fixture.rs` (≤ 200) — backed by `testing/fixtures`.
  - `src/tui/runtime/data/trace.rs` (≤ 200) — log/trace fallback.
  - `src/tui/runtime/stream/mod.rs` (≤ 200) — `Stream::connect`, transport selection.
  - `src/tui/runtime/stream/ws.rs` (≤ 250)
  - `src/tui/runtime/stream/sse.rs` (≤ 200)
  - `src/tui/runtime/stream/poll.rs` (≤ 200)
  - `src/tui/runtime/stream/degraded.rs` (≤ 200) — backoff + reconnect + cursor.
- **Test surface**: integration tests with `httpmock`; ws-fail → sse-fail → poll path; cursor resume.
- **Acceptance**: header shows correct transport badge in each mode.
- **Owner**: evidence-gate.
- **Effort**: L.

---

### Track 3 — App / Runtime Backbone (5 units, mostly sequential)

#### U3.1 — App State + Reducer + Selectors

- **Goal**: Replace `App` (466 LOC, ~400 fields) with state + reducer + selectors.
- **Inputs**: U1.1, U1.2, U2.4.
- **Outputs**:
  - `src/tui/app/mod.rs` (≤ 200) — `App { state: AppState, channels, data_client, action_adapter, stream }`.
  - `src/tui/app/state.rs` (≤ 250) — `AppState { read_model: Arc<TuiReadModel>, route_stack, focus_state, selection_by_pane, filter, pending_action, streams_status, diagnostics }`.
  - `src/tui/app/reducer.rs` (≤ 250) — root reducer dispatching to sub-reducers.
  - `src/tui/app/reducers/focus.rs` (≤ 200)
  - `src/tui/app/reducers/filter.rs` (≤ 200)
  - `src/tui/app/reducers/action.rs` (≤ 200)
  - `src/tui/app/reducers/stream.rs` (≤ 200)
  - `src/tui/app/reducers/read_model.rs` (≤ 200)
  - `src/tui/app/selectors.rs` (≤ 200) — shared selectors used by multiple lenses.
  - `src/tui/app/diagnostics.rs` (≤ 200) — frame timings, dropped events, replay buffer.
  - `src/tui/app/config.rs` (≤ 200) — keymap, theme, transport preferences.
  - `src/tui/app/channels.rs` (≤ 200) — bundled mpsc/watch.
  - `src/tui/app/builder.rs` (≤ 250) — `App::new`, `App::new_render_only`, `App::build`.
- **Test surface**: reducer table tests; selector tests; route history tests; render-only build.
- **Acceptance**: old `App` still compiles under non-flag build; new App compiles under flag.
- **Owner**: evidence-gate.
- **Effort**: L.

#### U3.2 — Sync Refactor (per-domain hydrators)

- **Goal**: Break `app_runtime_sync_actions.rs` (804 LOC) into one file per domain. Hydrators consume `DataClient` and produce `Intent::ReadModelArrived`.
- **Inputs**: U2.6, U3.1.
- **Outputs**:
  - `src/tui/runtime/sync/mod.rs` (≤ 200) — orchestrator, spawns hydrators in parallel.
  - `src/tui/runtime/sync/{core,workflow,queue,runners,cache,vti,agents,bugs,release,evidence,security,jankurai,aer,llm,git_sync,source_doctor}.rs` (≤ 200 each).
  - `src/tui/runtime/sync/background.rs` (≤ 200) — tick spawner.
- **Test surface**: each hydrator has `#[tokio::test]` against fake DataClient; merge concurrency is safe.
- **Acceptance**: every file ≤ 200; no file > 250.
- **Owner**: evidence-gate.
- **Effort**: L.

#### U3.3 — Render Driver + Input Layer

- **Goal**: Per-frame orchestrator + input routing. Bundles `runtime/render/*` and `runtime/input/*`.
- **Inputs**: U3.1, U2.2.
- **Outputs**:
  - `src/tui/runtime/render/mod.rs` (≤ 120)
  - `src/tui/runtime/render/frame.rs` (≤ 200) — orchestrates `ui::draw`.
  - `src/tui/runtime/render/capture.rs` (≤ 200) — PNG writer.
  - `src/tui/runtime/render/tests.rs` (≤ 300) — render-determinism tests at all canonical sizes.
  - `src/tui/runtime/input/mod.rs` (≤ 100)
  - `src/tui/runtime/input/keyboard.rs` (≤ 250)
  - `src/tui/runtime/input/mouse.rs` (≤ 250)
  - `src/tui/runtime/input/palette.rs` (≤ 250)
  - `src/tui/runtime/input/keymap.rs` (≤ 200) — config-driven key bindings.
  - `src/tui/runtime/input/navigation/mod.rs` (≤ 100)
  - `src/tui/runtime/input/navigation/general.rs` (≤ 200) — split from 573 LOC.
  - `src/tui/runtime/input/navigation/<lens>.rs` (one per lens, ≤ 150 each).
- **Test surface**: golden render at each canonical size; nav unit tests per file.
- **Owner**: evidence-gate.
- **Effort**: L.

#### U3.4 — UI Shell + Runner + Maintenance

- **Goal**: Top-level layout, chrome, overlays, entry points; cache-cleanup background task.
- **Inputs**: U3.1, U2.1, U2.3.
- **Outputs**:
  - `src/tui/ui/mod.rs` (≤ 120) — `pub fn draw(frame, app)`.
  - `src/tui/ui/layout.rs` (≤ 200) — master + body + sidebar split.
  - `src/tui/ui/header.rs` (≤ 200) — tab strip + breadcrumb + worst-freshness chip + stream state + backend badge.
  - `src/tui/ui/footer.rs` (≤ 200) — status + keys + frame metrics.
  - `src/tui/ui/sidebar.rs` (≤ 250) — fleet/family sidebar (was `repo_fleet_bar.rs`).
  - `src/tui/ui/activity.rs` (≤ 250) — universal activity strip (was `activity.rs`).
  - `src/tui/ui/overlays/mod.rs` (≤ 80)
  - `src/tui/ui/overlays/palette.rs` (≤ 250)
  - `src/tui/ui/overlays/help.rs` (≤ 200)
  - `src/tui/ui/overlays/repo_detail.rs` (≤ 200)
  - `src/tui/ui/overlays/proof_modal.rs` (stub, populated by U5.1, ≤ 100)
  - `src/tui/runner/mod.rs` (≤ 80)
  - `src/tui/runner/interactive.rs` (≤ 200)
  - `src/tui/runner/once.rs` (≤ 150)
  - `src/tui/runner/capture.rs` (≤ 200)
  - `src/tui/runtime/maintenance.rs` (≤ 200) — cache cleanup background.
- **Test surface**: golden snapshots of header (with each freshness class), footer, sidebar, palette overlay; capture round-trip.
- **Acceptance**: layout test at 80×24 / 100×30 / 120×36 / 160×48 / 220×60 without panic; all CLI flags preserved.
- **Owner**: evidence-gate.
- **Effort**: L.

#### U3.5 — Nav Module (route stack, breadcrumb, focus graph)

- **Goal**: Universal keyboard grammar consistent across lenses; integrates with reducer.
- **Inputs**: U3.1, U2.2.
- **Outputs**:
  - `src/tui/nav/mod.rs` (≤ 100)
  - `src/tui/nav/route.rs` (≤ 250) — route-stack model + intents.
  - `src/tui/nav/breadcrumbs.rs` (≤ 200) — scope breadcrumb renderer.
  - `src/tui/nav/focus.rs` (≤ 200) — focus-stack <-> route bridge.
  - `src/tui/nav/focus_graph.rs` (≤ 250) — macro (lens) / micro (pane) graph.
  - `src/tui/nav/history.rs` (≤ 200) — back-stack, restore on tab return.
- **Test surface**: Enter drills; Esc unwinds; arrows move spatially; Tab switches focus worlds; `g<letter>` selects lens; route persistence across reload.
- **Owner**: evidence-gate.
- **Effort**: M.

---

### Track 4 — Lens Migrations (11 units, parallel after U3.1)

> **Common outputs (per lens)**: `mod.rs`, `view.rs`, `data.rs`, `nav.rs`, `tests.rs` + sub-component files. Each lens uses `runtime/sync/<name>.rs` from U3.2.
> **Common test surface**: 1 golden at 200×60 + 1 at 100×30; 1 nav test; 1 empty-state test; 1 stale-data test; 1 fixture-driven scenario.
> **Common acceptance**: under feature flag, lens reachable via `g<letter>` and tab cycling; jankurai score ≥ 85.

#### U4.1 — Workflow Lens: Model + Delivery Builder

- **Goal**: Replace `workflow/model.rs` (963) + `workflow/delivery.rs` (1058) with focused sub-modules.
- **Inputs**: U1.1, U3.2.
- **Outputs**:
  - `src/tui/lenses/workflow/model/mod.rs` (≤ 80)
  - `src/tui/lenses/workflow/model/{status,node_kind,canonical_phase,snapshot,pr_view,edge}.rs` (≤ 200 each)
  - `src/tui/lenses/workflow/delivery/mod.rs` (≤ 150)
  - `src/tui/lenses/workflow/delivery/{ci,agent_review,auto_merge,promotion,post_merge}.rs` (≤ 250 each)
  - `src/tui/lenses/workflow/delivery/tests.rs` (≤ 300)
- **Effort**: L.

#### U4.2 — Workflow Lens: Canvas + Rails + Inspector + Logs

- **Goal**: Replace `workflow/widget.rs` (1063), `workflow/inspector.rs` (728), `workflow/nav.rs` (451), `workflow/live_delivery.rs` (582).
- **Inputs**: U4.1, U2.6.
- **Outputs**:
  - `src/tui/lenses/workflow/view.rs` (≤ 200)
  - `src/tui/lenses/workflow/canvas/{mod,nodes,edges,regions,minimap}.rs` (≤ 250 each)
  - `src/tui/lenses/workflow/rails/{mission_strip,pr_rail,phase_rail}.rs` (≤ 250 each)
  - `src/tui/lenses/workflow/inspector/{mod,card,actions,log_tail,tabs}.rs` (≤ 250 each)
  - `src/tui/lenses/workflow/nav.rs` (≤ 250)
  - `src/tui/lenses/workflow/hit_map.rs` (≤ 200)
  - `src/tui/lenses/workflow/live_collector.rs` (≤ 250)
  - `src/tui/lenses/workflow/data.rs` (≤ 200)
  - `src/tui/lenses/workflow/tests/{render,model,nav,empty,stale}.rs` (≤ 250 each)
  - `src/tui/lenses/logs/{mod,view,data,nav,tests}.rs` (≤ 250 each) — cursor-aware trace viewer; uses `widgets/log_viewer.rs`.
- **Effort**: L.

#### U4.3 — Queue Lens (Theoretical Limit + SCREAM + Bottleneck)

- **Goal**: Implement three-tier capacity model + SCREAM gauge + bottleneck decomposition + what-if simulator.
- **Inputs**: U1.2, U3.2.
- **Outputs**:
  - Standard 5 (≤ 250 each) + `limits.rs` (≤ 250) + `scream.rs` (≤ 200) + `bottleneck.rs` (≤ 250) + `simulator.rs` (≤ 250).
- **Effort**: L.

#### U4.4 — Global Mission Lens

- **Goal**: Top-level overview — repo-family pulse + attention rail + live event tape + next action.
- **Inputs**: U3.2.
- **Outputs**:
  - Standard 5 + `pulse.rs` (≤ 200) + `attention.rs` (≤ 250) + `tape.rs` (≤ 250) + `next_action.rs` (≤ 200) + `safe_to_*.rs` (≤ 200) (safe-to-code/merge/release/rollback indicators).
- **Effort**: M.

#### U4.5 — Repos / Families Lens + Repo Drilldown

- **Goal**: Family grouping + rollup metrics + repo drilldown view; integrates scope filter with route stack.
- **Inputs**: U3.2, U3.5.
- **Outputs**:
  - Standard 5 + `family_grouping.rs` (≤ 200) + `rollup.rs` (≤ 200) + `repo_detail.rs` (≤ 250) + `family_detail.rs` (≤ 250).
- **Effort**: M.

#### U4.6 — Runners + Cache + VTI Lenses (triplet)

- **Goal**: Three sibling lenses (pool/heatmap shape).
- **Inputs**: U3.2.
- **Outputs**:
  - `lenses/runners/`: standard 5 + `pools.rs` + `nodes.rs` + `telemetry.rs` + `scale_preview.rs` (≤ 200 each).
  - `lenses/cache/`: standard 5 + `categories.rs` + `hot.rs` + `gc_plan.rs` + `taint.rs` + `requests.rs` (≤ 200 each).
  - `lenses/vti/`: standard 5 + `plan.rs` + `selector_miss.rs` + `confidence.rs` + `repair.rs` + `flake_radar.rs` (≤ 200 each).
- **Effort**: L.

#### U4.7 — Agents + Autonomy Lenses

- **Goal**: Unified autonomy view — sessions, tasks, grants, LLM spend, kill-bell, freeze.
- **Inputs**: U3.2.
- **Outputs**:
  - `lenses/agents/`: standard 5 + `sessions.rs` + `tasks.rs` + `races.rs` + `launch_ledger.rs` + `evidence_links.rs` (≤ 200 each).
  - `lenses/autonomy/`: standard 5 + `kill_bell.rs` + `freeze.rs` + `verdicts.rs` + `policy.rs` (≤ 200 each).
- **Notes**: Both lenses show `INFERRED` badge until agent lifecycle tables land upstream.
- **Effort**: L.

#### U4.8 — Bugs + Git Sync + Bottlenecks Lenses

- **Goal**: Connect bugs, Git state, MR/PR state.
- **Inputs**: U3.2.
- **Outputs**:
  - `lenses/bugs/`: standard 5 + `board.rs` + `lanes.rs` + `attempts.rs` + `ready.rs` (≤ 200 each).
  - `lenses/git_sync/`: standard 5 + `branches.rs` + `mirrors.rs` + `admissions.rs` + `hooks.rs` + `mr_ingestion.rs` (≤ 200 each).
  - `lenses/bottlenecks/`: standard 5 + `historical.rs` + `structural.rs` + `decomposition.rs` (≤ 200 each).
- **Effort**: L.

#### U4.9 — Release + Evidence + Artifacts Lenses

- **Goal**: Delivery audit chain.
- **Inputs**: U3.2, U2.4.
- **Outputs**:
  - `lenses/release/`: standard 5 + `gates.rs` + `canary.rs` + `rollback.rs` + `train.rs` (≤ 200 each).
  - `lenses/evidence/`: standard 5 + `timeline.rs` + `capsule_ledger.rs` + `query.rs` + `proof_viewer.rs` + `bundle_export.rs` (≤ 250 each).
  - `lenses/artifacts/`: standard 5 + `attestation.rs` + `sbom.rs` + `provenance.rs` + `signatures.rs` (≤ 200 each).
- **Effort**: L.

#### U4.10 — Jankurai + AER + Security Lenses

- **Goal**: Audit lenses with shared shape.
- **Inputs**: U3.2.
- **Outputs**:
  - `lenses/jankurai/`: standard 5 + `score.rs` + `findings.rs` + `duplicate_clusters.rs` + `repair_queue.rs` + `caps.rs` (≤ 200 each).
  - `lenses/aer/`: standard 5 + `findings.rs` + `evidence.rs` (≤ 200 each).
  - `lenses/security/`: standard 5 + `findings.rs` + `secrets.rs` + `policies.rs` + `admission_drift.rs` (≤ 200 each).
- **Effort**: L.

#### U4.11 — LLM + Settings + Source Doctor + Churn + Incident + Replay Lenses

- **Goal**: Remaining lenses. Source Doctor is critical for the freshness story; Incident is an emergency pinned view; Replay is for postmortem.
- **Inputs**: U3.2, U1.2.
- **Outputs**:
  - `lenses/llms/`: standard 5 + `calls.rs` + `budget.rs` + `traces.rs` + `providers.rs` + `keys.rs` (≤ 200 each).
  - `lenses/settings/`: standard 5 + `runtime_profile.rs` + `transports.rs` + `keymap.rs` + `redacted_config.rs` (≤ 200 each).
  - `lenses/source_doctor/`: standard 5 + `sources.rs` + `health.rs` + `errors.rs` + `drift.rs` + `cursor.rs` (≤ 250 each) — drives the header's worst-source badge.
  - `lenses/churn/`: standard 5 + `change_volume.rs` + `risk.rs` (≤ 200 each).
  - `lenses/incident/`: standard 5 + `pinned.rs` + `decision_ledger.rs` (≤ 200 each).
  - `lenses/replay/`: standard 5 + `cursor.rs` + `event_log.rs` (≤ 200 each).
- **Effort**: L.

---

### Track 5 — Cross-Cutting Capstone (2 units)

#### U5.1 — Proof Modal + Action Gate Wiring

- **Goal**: Single proof-modal implementation reused by R3+ actions across lenses.
- **Inputs**: U2.4, U3.4, all of Track 4.
- **Outputs**:
  - `src/tui/ui/overlays/proof_modal.rs` (≤ 350 — waiver: it's a complex modal) — renders preview + target SHA + grants + typed confirmation; dry-run section; secondary approver field (R5).
  - `src/tui/action/gate.rs` extended (≤ 300) — full gate orchestration: open modal, validate typed input, dispatch, capture receipt.
  - `src/tui/action/preview.rs` extended (≤ 300) — preview builders for `request_merge`, `release_promote`, `release_rollback`, `secret_rotate`, `cache_drop`, `autonomy_freeze`, etc.
- **Test surface**: tuiwright safety suite:
  - `safety::merge_typed_sha_required`
  - `safety::release_typed_version_required`
  - `safety::cancel_does_not_mutate`
  - `safety::stale_blocks_high_risk`
  - `safety::secret_never_renders_in_modal`
  - `safety::rollback_requires_target`
- **Acceptance**: 5 R4 + 3 R5 actions each pass a safety test.
- **Owner**: evidence-gate.
- **Effort**: L.

#### U5.2 — tuiwright Capstone (nav, replay, perf, redaction, responsive, accessibility)

- **Goal**: Centralized cross-lens tests aligned with `tips/tui_reset/§11` and Codex's coverage matrix.
- **Inputs**: U2.5, U5.1, all of Track 4.
- **Outputs**:
  - `tests/tuiwright/nav.rs` (≤ 300) — Esc-always-pops; ≤ 2 keypresses Global → selected job log; ≤ 3 Global → release blocker proof; palette route search; filter/sort persistence.
  - `tests/tuiwright/streams.rs` (≤ 300) — live events; disconnect → stale marker → reconnect → cursor resume.
  - `tests/tuiwright/responsive.rs` (≤ 250) — 80×24 / 100×30 / 120×36 / 160×48 / 220×60.
  - `tests/tuiwright/redaction.rs` (≤ 250) — no tokens/secrets in screenshots, text dumps, bundles, panic output, copied paths.
  - `tests/tuiwright/source_doctor.rs` (≤ 250) — API down, MCP drift, schema mismatch, stale docs, DB profile mismatch.
  - `tests/tuiwright/accessibility.rs` (≤ 200) — ASCII fallback, no-color, high-contrast, reduced motion, stable focus order.
  - `tests/tuiwright/performance.rs` (≤ 250) — 500 repos × 10k jobs × 1k events/sec; input p95 < 50 ms; render p95 < 16 ms; large-list p95 < 33 ms.
  - `tests/tuiwright/flicker.rs` (≤ 250) — the 5 anti-flicker invariants.
  - `tests/tuiwright/replay.rs` (≤ 300) — recorded event streams: job start/run/fail/retry, OOM, cache miss-storm, VTI selector miss, agent patch race, canary rollback.
  - `tests/tuiwright/incident.rs` (≤ 200) — pinned emergency view.
  - CI: enable `cargo tuiwright` lane on every PR; promote to `state-change` proof lane for the reset branch.
- **Acceptance**: every spec'd test in §11 of `tips/tui_reset/` has a corresponding test file/case; performance gate passes on a CI runner; old monolith deleted.
- **Owner**: tools.
- **Effort**: L.

---

## 10. Parallelism Map

```
                            U0.1  (blocking)
                              │
        ┌─────────┬───────────┼───────────┬──────────────┐
        ▼         ▼           ▼           ▼              ▼
       T1                                                T2 (TUI foundations)
   ┌──┬─┬─┬─┬──┐                                ┌──┬─┬─┬─┬──┐
   1.1 1.2 1.3 1.4 1.5                          2.1 2.2 2.3 2.4 2.5 2.6
   (contracts + backend)                        (theme/focus/widgets/action/test/data)

   1.1, 1.2, 1.3 parallel.    2.1–2.5 parallel.
   1.4 needs 1.1+1.2+1.3.    2.6 needs 1.4+1.5.
   1.5 needs 1.3+1.4.

                              │
                              ▼
                            T3 (App/Runtime backbone — mostly sequential)
                            3.1 ──► 3.2 ──┐
                                          ├──► 3.3 ──► 3.4 ──► 3.5
                                          │
                                          ▼
                            T4 (lenses — fully parallel after 3.2 minimum)
                            4.1 → 4.2     (workflow chain, sequential)
                            4.3   4.4   4.5   4.6   4.7   4.8   4.9   4.10   4.11
                            (9 parallel lens units)

                                          │
                                          ▼
                                         T5
                                       5.1 → 5.2
```

**Parallel-safety rules:**
- `tui.rs`, `app/mod.rs`, `api/entity.rs`, `api/read_model.rs` are touched **only** by U0.1, U1.1, U3.1. Other units add new modules within their own directories.
- Per-lens fixture additions go in `testing/fixtures/<lens>.rs` (per-lens file) to avoid contention on `fixtures.rs`.
- Two unit owners working in parallel rebase weekly off shared `feat/tui-reset` integration branch.
- Lens units are independent except for the Workflow chain (4.1 → 4.2).
- Backend units (1.4, 1.5) and TUI units (T2/T3/T4) **only intersect through `src/api/`** — once 1.1/1.2/1.3 land, TUI work proceeds against fixtures while daemon-team implements 1.4/1.5.

---

## 11. Verification / Done Definition

The reset is "done" when **all** of the following hold:

- [ ] All 14+ lenses present, reachable via `g<letter>` and tab cycling.
- [ ] `cargo build` produces zero warnings; jankurai score ≥ 85 on every PR.
- [ ] No `.rs` file in `src/tui/**` > 350 LOC (waivers documented in headers).
- [ ] `cargo tuiwright` runs in CI; all 9 suites green: capture, nav, responsive, streams, actions, redaction, source-doctor, accessibility, performance, plus flicker and replay.
- [ ] Every lens has ≥ 1 golden, ≥ 1 nav test, ≥ 1 empty-state test, ≥ 1 stale-data test.
- [ ] Stream client connects via WebSocket → SSE → poll → CLI; header shows correct badge in each mode.
- [ ] Proof modal exercised by 5 R4 + 3 R5 actions.
- [ ] All 37 existing action IDs resolve under new tier scheme.
- [ ] All `jeryu tui` CLI flags preserved.
- [ ] Anti-flicker invariants verified.
- [ ] `cargo run -p jeryu -- tui --once` exits under 2 s.
- [ ] `cargo run -p jeryu -- tui --capture` exits under 8 s.
- [ ] `jeryu tui --demo` renders every lens populated from fixtures.
- [ ] SQLite default profile + RedlineDB feature profile both pass `just runtime-*` lanes.
- [ ] Old `src/tui/{ui.rs, app.rs, workflow/{widget,delivery,inspector,nav,live_delivery,action_adapter,model}.rs, app_runtime*.rs, ui_panels_body*.rs, focus.rs, repo_fleet_bar.rs, activity.rs}` and old `tests/tui_tuiwright.rs` deleted in final cleanup unit; feature flag removed.

---

## 12. Risk Register & Mitigations

| Risk | Mitigation |
|---|---|
| Backend `src/inspection/` is on the daemon team's plate; if slow, TUI work stalls. | Fixture-first: every lens lands rendering from `TuiReadModel::demo_*()` before live wiring. Backend can lag without blocking. |
| `App` struct refactor (U3.1) cascades through every callsite. | Land under feature flag; old `App` compiles in parallel; flip callsites in small follow-ups. |
| tuiwright unmaintained / version drift. | Pin via `=x.y.z`; vendor thin wrapper (`testing/tuiwright.rs`) so swap is contained. |
| Read-model schema churn breaks lens tests. | Lens tests assert on rendered glyphs/layout, not field names. Only `data.rs` per lens is coupled to schema. |
| Stream client (U2.6) blocked by daemon team's SSE landing. | Polling path is the canonical default; SSE is opt-in via flag until backend is ready. Header shows `[poll]` until then. |
| Proof modal UX confuses users. | Capture proof flow in `tuiwright_safety::*` and require user-test sign-off before flipping flag to default-on. |
| File explosion from over-splitting. | LOC budget table per file is enforced via CI line-count linter (added in U0.1). Also: track total file count — flag if it exceeds 250. |
| Owner-map mis-routing. | U0.1 adds the routing; PRs auto-check via existing jankurai owner audit. |
| Existing 1251-LOC `tests/tui_tuiwright.rs` regresses on split. | U2.5 split must preserve every `#[test]` (verified by count); separate PR per category to bisect breakage. |
| Action ID alias migration breaks CLI/MCP. | Alias test (`actions_tests.rs`) covers all 37 IDs; CLI smoke test runs the dispatch. |
| Reducer pattern adds boilerplate. | Each reducer is < 200 LOC and table-tested; net LOC is smaller because state mutation is removed from 100+ scattered sites. |
| RedlineDB feature path untested. | CI runs `just runtime-redlinedb-jansu` lane on the reset branch. |

---

## 13. Open Questions

1. **Tuiwright crate** — confirm a published `tuiwright` crate exists; if unavailable, build a thin in-tree harness with the same API shape so test code is portable.
2. **MCP resource mirror** — is `/mcp/resources` + `jeryu://*` resources in-scope for this reset, or follow-up? **Default**: HTTP first (U1.4/U1.5); MCP mirror is a separate post-reset PR.
3. **Header truncation policy at 80 cols** — which chips drop first (freshness vs stream-state vs breadcrumb)? **Default**: keep breadcrumb + worst-freshness; drop stream-state to a tiny dot in tiny mode.
4. **Live-mode default** — default to `--stream` or `--poll`? **Default**: auto-detect (WS → SSE → poll), document in `--help`.
5. **Demo fixtures storage** — keep `tui::testing::fixtures` rich or move to `examples/`? **Default**: keep in `testing/` until size warrants extraction.
6. **Old `src/tui/flow/*`** — fold into Workflow lens, Queue lens, or split? **Default**: split — job lifecycle goes to Queue/data flows; pipeline DAG portion goes to Workflow.
7. **`src/inspection/` owner** — new owner needed in `agent/owner-map.json`. **Proposed**: `daemon-team`.
8. **Schema version bump for `TuiReadModel` extension** — bump `schema_version` to 2; emit `Migration` event for legacy clients? **Default**: yes, bump.
9. **Two parallel plans (Codex's vs this one)** — converge into one master plan, or pick? **Default**: human picks; this plan can be merged with Codex's by adopting his terminology in user-visible labels and this plan's structure in execution detail.

---

## 14. Appendix A — Current → New Mapping

| Current file (LOC) | New destination |
|---|---|
| `src/tui.rs` (62) | `src/tui.rs` (≤ 80) — re-exports only |
| `src/tui/app.rs` (466) | `src/tui/app/{mod,state,channels,config,builder}.rs` + reducers |
| `src/tui/app_runtime.rs` (649) | `src/tui/app/builder.rs` + `src/tui/runtime/mod.rs` |
| `src/tui/app_runtime_sync.rs` (564) | `src/tui/runtime/sync/mod.rs` + per-domain hydrators |
| `src/tui/app_runtime_sync_actions.rs` (804) | split into 16 `runtime/sync/<domain>.rs` |
| `src/tui/app_runtime_sync_background.rs` (440) | `src/tui/runtime/sync/background.rs` |
| `src/tui/app_runtime_sync_tests.rs` (414) | per-domain test files |
| `src/tui/app_runtime_demo_state.rs` (525) | `src/tui/testing/fixtures.rs` + per-lens fixtures |
| `src/tui/ui.rs` (401) | `src/tui/ui/{mod,layout,header,footer}.rs` |
| `src/tui/ui_panels_body.rs` (482) | per-lens `view.rs` files |
| `src/tui/ui_panels_body_*.rs` | per-lens view files |
| `src/tui/ui_chrome.rs` (369) | `src/tui/ui/{header,footer}.rs` |
| `src/tui/focus.rs` (479) | `src/tui/focus/{mod,pane,state,map,chrome,graph}.rs` |
| `src/tui/repo_fleet_bar.rs` (473) | `src/tui/ui/sidebar.rs` + `src/tui/ui/overlays/repo_detail.rs` |
| `src/tui/activity.rs` (437) | `src/tui/ui/activity.rs` |
| `src/tui/bugs.rs` (438) | `src/tui/lenses/bugs/` |
| `src/tui/queue.rs` (242) | `src/tui/lenses/queue_view/` (the existing list view) — keep behavior; queue/capacity stays in `lenses/queue/` |
| `src/tui/runner.rs` (156) | `src/tui/runner/{mod,interactive,once,capture}.rs` |
| `src/tui/runtime/input/*` | `src/tui/runtime/input/{keyboard,mouse,palette,keymap,navigation/*}.rs` |
| `src/tui/runtime/render/*` | `src/tui/runtime/render/{mod,frame,capture,tests}.rs` |
| `src/tui/runtime/maintenance.rs` | unchanged |
| `src/tui/widgets/*` (2,032) | `src/tui/widgets/*` (extended) |
| `src/tui/theme.rs` | `src/tui/theme/{mod,palette,glyphs,badges,progress,terminal_caps}.rs` |
| `src/tui/action_registry.rs`, `action_registry_entries.rs` | `src/tui/action/registry.rs` + tier migration |
| `src/tui/workflow/model.rs` (963) | `src/tui/lenses/workflow/model/{mod,status,node_kind,canonical_phase,snapshot,pr_view,edge}.rs` |
| `src/tui/workflow/widget.rs` (1063) | `src/tui/lenses/workflow/{view,canvas/*,rails/*}.rs` |
| `src/tui/workflow/delivery.rs` (1058) | `src/tui/lenses/workflow/delivery/{mod,ci,agent_review,auto_merge,promotion,post_merge}.rs` |
| `src/tui/workflow/action_adapter.rs` (1003) | `src/tui/action/{prod,registry,risk,preview,gate}.rs` + lens wiring |
| `src/tui/workflow/inspector.rs` (728) | `src/tui/lenses/workflow/inspector/{mod,card,actions,log_tail,tabs}.rs` |
| `src/tui/workflow/live_delivery.rs` (582) | `src/tui/lenses/workflow/live_collector.rs` + `src/tui/runtime/stream/*` |
| `src/tui/workflow/nav.rs` (451) | `src/tui/lenses/workflow/nav.rs` |
| `src/tui/flow/*` (1,527) | split between `src/tui/lenses/queue/` (job lifecycle) and `src/tui/lenses/workflow/` (pipeline DAG) |
| `src/tui/jankurai/*` (683) | `src/tui/lenses/jankurai/*` |
| `src/tui/aer/*`, `vrc/*`, `witness/*`, `proof_lanes/*` | `src/tui/lenses/{aer,vti,evidence,proof_lanes}/*` |
| `src/tui/live.rs` | `src/tui/runtime/stream/*` |
| `tests/tui_tuiwright.rs` (1251) | `tests/tuiwright/{capture,tabs,bugs,workflow,fleet_bar,jankurai,overlays,palette,discovery}.rs` (split) + new suites in U5.2 |
| `tests/tui_recording.rs` (~220) | `tests/tuiwright/replay.rs` (extended) |

---

## 15. Appendix B — Glossary

- **Lens** — one tab/view. Lives in `src/tui/lenses/<name>/`.
- **EntityRef** — universal addressable handle (kind + id + label + scope).
- **TuiReadModel** — typed snapshot consumed by render layer; lives in `src/api/`.
- **SourceFreshness** — per-source data freshness (Live/Fresh/Stale/LastKnown/Inferred/Unknown).
- **SCREAM** — 0–100 fleet inefficiency headline.
- **Three-tier capacity** — physics floor (∞ runners, hot cache), fleet floor (current pool), policy floor (with gates).
- **Edge confidence** — `Explicit` (declared `needs:`), `Inferred` (artifact/cache implied).
- **R0..R5** — risk tiers, read-only to destructive/production. R4+ require proof modal.
- **Proof modal** — gate for R3+ actions: target SHA, grants, typed confirmation, receipt.
- **tuiwright** — black-box terminal test harness.
- **Reducer** — pure `(AppState, Intent) -> AppState`. Determinism enforced by table tests.
- **Selector** — pure `AppState -> LensInput`. Lives in `lenses/<name>/data.rs`.
- **Intent** — input/sync/action/stream event that drives reducer.
- **DataClient** — transport-agnostic backend client (HTTP/MCP/Local/Fixture).
- **Tape** — live event stream rendered as a scrolling strip on Global Mission.

---

*End of plan. On ExitPlanMode → write this content to `~/jeryu/TUI_RESET_PLAN_CLAUDE.md`.*
