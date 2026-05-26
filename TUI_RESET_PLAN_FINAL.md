# JeRyu TUI Reset Final Plan

Status: final controlling reset plan
Product name: JeRyu Flight Deck
Internal shorthand: Hyperdeck
Baseline date: 2026-05-26
Target command: `jeryu tui`
Source plans reviewed: `TUI_RESET_PLAN.md`, `TUI_RESET_PLAN_CLAUDE.md`

## 0. Review Verdict

This final plan merges the strongest parts of both source plans and corrects stale or risky details found during review.

| Source | Strongest parts to keep | Gaps corrected in this final |
|---|---|---|
| `TUI_RESET_PLAN.md` | Accurate baseline audit, broad non-negotiable outcomes, complete backend endpoint inventory, full screen inventory, DB-boundary discipline, proof command table, and U01-U26 end-state coverage. | Too high-level for implementation sequencing; flat `pages/*.rs` layout would recreate oversized screen files; no risk register or current-to-new file mapping. |
| `TUI_RESET_PLAN_CLAUDE.md` | Better executable structure: lens template, per-output-file budgets, reducer flow, migration map, anti-flicker invariants, Tuiwright split plan, risk register, and explicit dependency graph. | Several facts are stale or over-asserted: current tree has 119 TUI Rust files, 31,440 TUI LOC, 34 action IDs, 20 `EntityKind` variants, 43 `TuiEventKind` variants, 54 durable DB tables, and CLI also has `--once`. The plan also assumes some owners/deps/router style before repo acceptance. |

Final decisions:

1. Use **lenses** rather than flat pages. A screen is too large for one file, so every screen gets `view.rs`, `data.rs`, `nav.rs`, `tests.rs`, and focused subcomponents.
2. Keep `src/tui.rs` as a thin public root and migrate behind compatibility exports. Do not delete existing TUI files until replacement tests are green.
3. Extend `src/api` in place. Do not create private DTOs in TUI or inspection modules for the same concepts.
4. Add `/api/v1/*` inspection HTTP first. Add MCP resources later, after HTTP contracts are stable.
5. Treat render purity, source freshness, proof-backed status, one mutation path, and DB boundaries as blocking invariants.
6. Use fixture-first delivery so UI, backend, and test work can progress in parallel.
7. Preserve `jeryu tui`, `--once`, `--demo`, `--capture`, `--screenshot`, `--tab`, `--output`, `--width`, `--height`, and `--screenshot-hold-ms`.
8. Use the current factual baseline in this file, not stale counts from either source.

## 0.1 Active Coordination Claims

Last updated: 2026-05-26T12:00:00Z

Branch/worktree for this reset pass:

- Branch: `codex/tui-reset-20260526`
- Base: `github/main` at `05e9f289fe6bc81937aa8651495dd97f80142538`
- Worktree: `/home/ubuntu/jeryu-tui-reset`
- Original dirty worktree preserved at `/home/ubuntu/jeryu`

Claim protocol:

- Add or update a row here before editing reset files.
- Keep write scopes disjoint; do not edit another active claim's paths without coordination.
- Mark rows `done` with proof commands before releasing the claim.
- Units not listed here are available but still governed by the dependency graph in section 18.

| Claim | Owner | Units | Write scope | Status | Proof target |
|---|---|---|---|---|---|
| TUI-RESET-20260526-001 | Codex parent | U00, U01 | `TUI_RESET_PLAN_FINAL.md`, `src/tui.rs`, new non-conflicting `src/tui/{actions,lenses,nav,testing}` modules, new `src/tui/runtime/{data,stream}` modules | done | `cargo check -p jeryu --message-format=json` passed; `git diff --check` pending final diff |
| TUI-RESET-20260526-002 | Codex parent | U04, U05 seed contracts | `src/api/{entity.rs,entity_tests.rs,freshness.rs,runtime_profile.rs,mod.rs}` | done | `cargo test -p jeryu --lib api::` passed |
| TUI-RESET-20260526-003 | Codex MCP worker Mill | U02 | `scripts/loc_audit.sh`, optional doc note in this section only | done | `sh -n` passed; reset thresholds fail on known oversized baseline; high-threshold smoke passed |
| TUI-RESET-20260526-004 | Codex MCP worker Pauli | U03 planning/harness scout | `tests/tuiwright/` only unless promoted after review | done | `git diff --check -- tests/tuiwright/README.md` passed; current assertions inventoried |
| TUI-RESET-20260526-005 | Codex parent | U06 seed | `src/tui/action_registry.rs` only | claimed | `cargo test -p jeryu --lib tui::action_registry` |
| TUI-RESET-20260526-006 | Codex parent | Audit cleanup (per codex's worktree state board) | `README.md`, `agent/repo-score.{json,md}`, `agent/score-history.{csv,jsonl}` | claimed (per codex worktree; not yet on branch claude/tui-reset-u06-20260526) | `just score` |
| TUI-RESET-20260526-007 | Codex parent | Score issue fixes (per codex's worktree state board) | `agent/{owner-map.json,test-map.json}`, `src/api/{entity.rs,entity_kind.rs,entity_support.rs,freshness.rs}`, `src/tui/testing/mod.rs` | claimed (per codex worktree; not yet on branch claude/tui-reset-u06-20260526) | `cargo test -p jeryu --lib api::`; `just score` |
| TUI-RESET-20260526-008 | Claude orchestrator | U07 inspection read plane (action-registry route deferred until U06 lands on integration branch) | new `src/inspection/{mod,router,read_model,events,entity,proof,health,serve,state}.rs`; new `src/api/proof.rs`; mirrored `src/api/{freshness,runtime_profile,mod}.rs` from codex worktree | done | `cargo nextest run -p jeryu --lib inspection:: api::proof` → 19/19 passed; `cargo check -p jeryu` green; LOC budget respected (every new file ≤ 200) |
| TUI-RESET-20260526-010 | Claude orchestrator (U12 agent) | U12 | new `src/tui/theme/{mod,palette,glyphs,badges,progress,terminal_caps}.rs`; existing `src/tui/theme.rs` migrated to `src/tui/theme/legacy.rs` (file move only; mod.rs re-exports its public surface so all 30+ `crate::tui::theme::Theme` callers see no API break); no edit to `src/tui.rs` (the `pub mod theme;` line is already present) | done | `cargo nextest run -p jeryu --lib tui::theme::` → 26/26 passed; sibling `tui::` 228/228 + `inspection:: api::` 47/47 still pass; LOC budgets respected (mod 50, palette 126, glyphs 93, badges 229, progress 166, terminal_caps 126; all ≤ stated caps) |

## 1. Mission

JeRyu Flight Deck is a terminal-native software delivery operating room. Opening `jeryu tui` must answer, within five seconds:

- What is healthy, blocked, stale, risky, and actionable.
- Which repo family, repo, branch, MR, pipeline, job, runner, cache object, test decision, bug, agent, release gate, artifact, or proof caused that state.
- Whether the screen is live, stale, partial, inferred, source-down, or fixture-backed.
- What the next safe action is and what proof makes it safe.

The reset is not a visual cleanup. It is an architecture reset around typed contracts, an inspection plane, deterministic state transitions, pure rendering, reusable widgets, proof-gated actions, and black-box Tuiwright coverage.

## 2. Non-Negotiable Outcomes

| Outcome | Requirement |
|---|---|
| Total fleet awareness | Default Mission lens shows fleet posture, top blocker, source freshness, and next action within five seconds. |
| Every visible thing addressable | Rows, cards, graph nodes, badges, logs, evidence, grants, artifacts, and actions map to `EntityRef` or explicit proof IDs. |
| Proof-backed status | Green requires evidence. Missing proof renders as `NO PROOF`, `HEUR`, `STALE`, `PARTIAL`, `SOURCE DOWN`, or `INFERRED`. |
| One mutation path | All mutating work uses action registry -> preview -> proof/confirmation -> execute -> stream -> receipt. |
| Renderer purity | Draw code consumes immutable view models only. No DB, GitLab, Docker, Vault, filesystem, MCP, or network calls during render. |
| Source freshness everywhere | Header and every lens expose source, age, cursor, last error, confidence, and degraded state. |
| Deterministic state | Input, data, stream, and action events become intents and mutate state only through reducers. |
| Fixture-first delivery | Every lens renders populated, empty, stale, degraded, and source-down states before live wiring is required. |
| Tuiwright proof | Primary UX behavior is tested through captures, keyboard/mouse interaction, degradation, redaction, accessibility, and performance suites. |
| File-size discipline | Reset work splits oversized files and prevents new oversized modules. |
| DB boundary | SQLite remains default. RedlineDB remains explicit feature/config only. Raw SQL stays inside DB-owned modules. |

## 3. Baseline Audit

Current facts were rechecked from the workspace on 2026-05-26.

| Area | Current fact | Planning consequence |
|---|---|---|
| TUI root | `src/tui.rs` is a thin module root that exports `run_tui`, `run_tui_once`, `run_tui_screenshot`, and `capture_tui_png`. | Preserve public entry points; keep the root under 80 lines. |
| TUI footprint | 119 Rust files under `src/tui/**`, 31,440 total LOC. | Migration must be incremental and file-budgeted. |
| Oversized files | 39 TUI files are above 300 LOC; 12 are above 500 LOC. | Any touched oversized file must be split or receive only routing/deletion edits. |
| Largest files | `workflow/widget.rs` 1063, `workflow/delivery.rs` 1058, `workflow/action_adapter.rs` 1003, `workflow/model.rs` 963. | Workflow reset must be split across model, delivery, canvas, rails, inspector, logs, action, and tests. |
| Tuiwright | `tests/tui_tuiwright.rs` is 1251 LOC; `tests/tui_recording.rs` is 204 LOC. | Split monolith into focused suites and preserve existing assertions. |
| Active tabs | Workflow, Mission, Release, Approvals, Jobs, Agents, Tests, Pools, Cache, Evidence, Bugs, LLMs, Git, Secrets, Jankurai. | Preserve existing tab names and expand to new lenses through aliases/routes. |
| API modules | `src/api` has 14 Rust files and 1616 LOC. | Extend, do not fork, the API contracts. |
| Entity taxonomy | Current `EntityKind` has 20 variants. | Expand to full fleet taxonomy with serde compatibility. |
| Event taxonomy | Current `TuiEventKind` has 43 variants. | Extend with source, proof, cache verdict, security, artifact, and action lifecycle detail. |
| Read model | Current `TuiReadModel` has schema version, generated time, cursor, freshness, mission, attention, next action, and system health. | Expand into a full dashboard graph and bump schema version. |
| Freshness | Current `DataFreshness` is source age plus `overall_stale`. | Replace with explicit source state, cursor, TTL, confidence, degraded reason, and last error. |
| Actions | Current action registry has 34 IDs and four tiers: `ReadOnly`, `Low`, `High`, `Production`. | Migrate to R0-R5 with aliases; preserve all 34 current IDs. |
| Daemon HTTP | Autonomy HTTP serves `GET /metrics`, `GET /health`, and `POST /events`. No `/api/v1/*` inspection routes exist. | Add a versioned inspection plane without overloading health/webhook routes. |
| MCP HTTP | `/mcp` supports POST/DELETE; GET returns method-not-allowed because Streamable HTTP GET is disabled. | Keep MCP tools action-oriented for now; add read-only resources after HTTP stabilizes. |
| DB schema | 54 durable tables audited across `db/state.rs` and typed repo schema files. | New durable truth must be added through DB-owned schema/repo modules only. |
| Runtime profile | SQLite is default; RedlineDB is behind `redlinedb-backend` and explicit Redline URLs/profile. | Header and Source Doctor must expose actual backend and feature profile. |
| CLI flags | `jeryu tui` supports `--once`, `--demo`, `--capture`, `--screenshot`, `--tab`, `--output`, `--width`, `--height`, `--screenshot-hold-ms`. | Preserve all flags and add tests before refactoring runner code. |
| Worktree | Existing uncommitted TUI, audit, and plan changes are present. | Reset PRs must not revert unrelated local work. |

### 3.1 Current Action IDs

These IDs must resolve after the R0-R5 migration:

```text
open_logs
requeue_job
remove_record
pause_pool
explain_blockers
fetch_capsule
get_system_snapshot
get_pipeline_jobs
get_ci_bottlenecks
propose_patch
race_patches
request_merge
plan_validation
bug_submit
bug_list
bug_show
bug_ready
bug_update
bug_record_attempt
run_tests
next_action
tab_mission
tab_release
tab_jobs
tab_agents
tab_tests
tab_pools
tab_cache
tab_evidence
tab_bugs
tab_secrets
tab_llms
toggle_audit_ledger
quit
```

### 3.2 Oversized Files To Split

| File | LOC | Destination |
|---|---:|---|
| `tests/tui_tuiwright.rs` | 1251 | `tests/tuiwright/{capture,navigation,responsive,streams,actions,redaction,source_doctor,accessibility,performance,flicker,replay}.rs` |
| `src/tui/workflow/widget.rs` | 1063 | `src/tui/lenses/workflow/{view,canvas/*,rails/*}` |
| `src/tui/workflow/delivery.rs` | 1058 | `src/tui/lenses/workflow/delivery/{mod,ci,agent_review,auto_merge,promotion,post_merge}.rs` |
| `src/tui/workflow/action_adapter.rs` | 1003 | `src/tui/actions/{adapter,preview,execute,gate,risk,prod,fake}.rs` |
| `src/tui/workflow/model.rs` | 963 | `src/tui/lenses/workflow/model/{mod,status,node_kind,phase,snapshot,pr_view,edge}.rs` |
| `src/tui/runtime/render/tests.rs` | 775 | render test suites split by capture/layout/accessibility/perf |
| `src/tui/app_runtime_sync_actions.rs` | 804 | `src/tui/runtime/sync/{workflow,queue,runners,cache,vti,agents,bugs,release,evidence,security,jankurai,llms,git_sync,source_doctor}.rs` |
| `src/tui/workflow/inspector.rs` | 728 | `src/tui/lenses/workflow/inspector/{mod,card,actions,log_tail,tabs}.rs` |
| `src/tui/app_runtime.rs` | 649 | `src/tui/app/{builder,channels}.rs` and `src/tui/runtime/*` |
| `src/tui/workflow/live_delivery.rs` | 582 | `src/tui/lenses/workflow/live_collector.rs` and `src/tui/runtime/stream/*` |
| `src/tui/runtime/input/navigation/general.rs` | 573 | `src/tui/runtime/input/{keyboard,keymap}.rs` and per-lens nav |
| `src/tui/app_runtime_sync.rs` | 564 | `src/tui/runtime/sync/mod.rs` and per-domain hydrators |
| `src/tui/app_runtime_demo_state.rs` | 525 | `src/tui/testing/fixtures/{mod,mission,queue,workflow,...}.rs` |
| `src/tui/ui_panels_body.rs` | 482 | per-lens `view.rs` files |
| `src/tui/focus.rs` | 479 | `src/tui/focus/{pane,state,map,chrome,graph}.rs` |
| `src/tui/repo_fleet_bar.rs` | 473 | `src/tui/ui/sidebar.rs` and `src/tui/ui/overlays/repo_detail.rs` |
| `src/tui/app.rs` | 466 | `src/tui/app/{mod,state,reducer,selectors,diagnostics,config}.rs` |
| `src/tui/workflow/nav.rs` | 451 | `src/tui/lenses/workflow/nav.rs` and shared `src/tui/nav/*` |
| `src/tui/app_runtime_sync_background.rs` | 440 | `src/tui/runtime/sync/background.rs` |
| `src/tui/bugs.rs` | 438 | `src/tui/lenses/bugs/*` |
| `src/tui/activity.rs` | 437 | `src/tui/ui/activity.rs` and event-tape widget |
| `src/tui/ui.rs` | 401 | `src/tui/ui/{mod,layout,router,overlays}.rs` |
| `src/tui/action_registry_entries.rs` | 368 | `src/tui/actions/registry.rs` plus generated/static entries split if still needed |
| `src/tui/ui_chrome.rs` | 369 | `src/tui/widgets/{header,tabs,status_strip}.rs` |
| `src/tui/ui_panels_body_bugs.rs` | 363 | `src/tui/lenses/bugs/view.rs` and subcomponents |
| `src/tui/ui_panels_body_llms.rs` | 360 | `src/tui/lenses/llms/*` |

## 4. Architecture

```text
GitLab REST/Webhooks  \
State DB               \
Docker/remotes          -> collectors/projections -> Inspection API/events -> TUI DataClient
SmartCache             /
Vault/secrets         /
Agents/autonomy      /
Jankurai/artifacts  /
Git/admission      /

TUI DataClient -> reducer/store/selectors -> pure Ratatui lenses/widgets
                              |
                              +-> action flow: preview/proof/execute/receipt
```

Hard boundaries:

- `src/api`: shared typed contracts.
- `src/inspection`: backend read/action inspection plane.
- `src/db` and `db`: durable SQL/schema ownership.
- `src/tui/runtime/data`: transport adapters.
- `src/tui/app`: deterministic app state and reducers.
- `src/tui/lenses`: screen composition only.
- `src/tui/widgets`: reusable drawing blocks.
- `src/tui/actions`: TUI-side preview/gate/execution orchestration, never direct mutation.

## 5. Target Layout

Use a lens-oriented layout. "Lens" means a full operator screen or route family.

```text
src/tui.rs
src/tui/
  app/
    mod.rs
    state.rs
    reducer.rs
    selectors.rs
    diagnostics.rs
    config.rs
    channels.rs
    builder.rs
    reducers/
      focus.rs
      route.rs
      read_model.rs
      stream.rs
      action.rs
      filters.rs
  runtime/
    mod.rs
    loop.rs
    render/
      mod.rs
      frame.rs
      capture.rs
      tests.rs
    terminal.rs
    maintenance.rs
    tasks.rs
    backpressure.rs
    input/
      mod.rs
      keymap.rs
      keyboard.rs
      mouse.rs
      command.rs
      navigation/
        mod.rs
        general.rs
        workflow.rs
        bugs.rs
        command_palette.rs
    data/
      mod.rs
      client.rs
      http.rs
      mcp.rs
      local.rs
      fixture.rs
      stream.rs
      trace.rs
    stream/
      mod.rs
      sse.rs
      poll.rs
      degraded.rs
  nav/
    mod.rs
    route.rs
    breadcrumbs.rs
    focus.rs
    focus_graph.rs
    history.rs
  ui/
    mod.rs
    layout.rs
    header.rs
    footer.rs
    sidebar.rs
    activity.rs
    overlays/
      mod.rs
      command_palette.rs
      help.rs
      repo_detail.rs
      proof_modal.rs
  actions/
    mod.rs
    registry.rs
    preview.rs
    execute.rs
    adapter.rs
    gate.rs
    risk.rs
    fake.rs
    prod.rs
  lenses/
    _template/README.md
    mission/
    queue/
    repos/
    repo/
    workflow/
    logs/
    runners/
    cache/
    vti/
    agents/
    autonomy/
    bugs/
    git_sync/
    bottlenecks/
    jankurai/
    aer/
    security/
    artifacts/
    release/
    evidence/
    settings/
    source_doctor/
    llms/
    churn/
    incident/
    replay/
  widgets/
    header.rs
    tabs.rs
    status_strip.rs
    freshness.rs
    attention.rs
    table.rs
    virtual_table.rs
    inspector.rs
    graph.rs
    dag.rs
    progress.rs
    sparkline.rs
    heatmap.rs
    event_tape.rs
    command_palette.rs
    help.rs
    modal.rs
    log_viewer.rs
    proof.rs
    forms.rs
    entity_link.rs
  focus/
    mod.rs
    pane.rs
    state.rs
    map.rs
    chrome.rs
    graph.rs
  theme/
    mod.rs
    palette.rs
    glyphs.rs
    badges.rs
    progress.rs
    terminal_caps.rs
  testing/
    mod.rs
    fixtures/
      mod.rs
      mission.rs
      queue.rs
      workflow.rs
      release.rs
      incident.rs
    golden.rs
    interaction.rs
    assertions.rs
```

Add backend inspection modules outside TUI:

```text
src/inspection/
  mod.rs
  router.rs
  read_model.rs
  events.rs
  entity.rs
  proof.rs
  health.rs
  actions.rs
  streams.rs
  projections/
    repos.rs
    workflow.rs
    queue.rs
    runners.rs
    cache.rs
    vti.rs
    agents.rs
    bugs.rs
    release.rs
    evidence.rs
    security.rs
    artifacts.rs
    llms.rs
```

Router implementation must respect the hosting surface. If mounted in the existing autonomy HTTP server, keep that server's zero-new-framework parsing constraints. If mounted in an existing Axum surface, reuse the existing Axum dependency. Do not introduce a new HTTP stack just for this reset.

## 6. Canonical Lens Contract

Every lens directory follows this shape:

| File | Target LOC | Hard cap | Purpose |
|---|---:|---:|---|
| `mod.rs` | 20-80 | 100 | Module declarations and re-exports only. |
| `view.rs` | 150-250 | 350 | Draw orchestration from immutable `LensInput`. |
| `data.rs` | 100-200 | 300 | Pure selectors from `AppState` to `LensInput`. |
| `nav.rs` | 100-200 | 300 | Lens-local key/mouse handling returning intents. |
| `tests.rs` | 200-350 | 500 | Unit/render tests for data, nav, empty, stale, degraded states. |
| subcomponents | 100-250 | 350 | Focused panels, cards, tables, graphs, inspectors. |

Lens rules:

- `view.rs` cannot import DB, GitLab, Docker, Vault, MCP, `reqwest`, `sqlx`, or filesystem APIs.
- `data.rs` cannot perform I/O. It selects from already-loaded `AppState`.
- `nav.rs` returns intents only. It does not mutate state directly.
- Each lens must have fixture coverage for populated, empty, stale, degraded, and source-down states.
- Each lens must expose evidence routes for important green, yellow, or red status.

## 7. File Size Standard

| File type | Target | Hard cap |
|---|---:|---:|
| Normal implementation | 150-250 lines | 350 lines |
| Complex renderer/model | 250-350 lines | 450 lines |
| Test file | 200-350 lines | 500 lines |
| Fixture file | Prefer split by scenario | 500 lines |
| Generated/declared artifact | Documented exception | Must carry source command and waiver |

Touch rule:

- If a reset PR touches an oversized TUI file, it must either split that file or limit changes to mechanical routing/deletion.
- New files above the hard cap are rejected unless they are generated/declared artifacts with an explicit waiver.

## 8. Core Contracts

### 8.1 Entity Taxonomy

Expand `EntityKind` to include:

```text
Fleet, RepoFamily, Repo, Branch, Commit, MergeRequest, Pipeline, PipelineBridge,
Stage, Job, RunnerPool, RunnerManager, Runner, Node, CacheObject, CacheRequest,
CacheVerdict, CacheTaint, TestPlan, TestCase, SelectorMiss, Agent, AgentSession,
AgentTask, AgentRace, Bug, BugAttempt, GitRefUpdate, AdmissionDecision,
CapabilityIntent, CapabilityGrant, JankuraiRun, JankuraiFinding, AerFinding,
SecurityFinding, SecretAuthority, ReleaseSecretSet, Artifact, Signature, Sbom,
ReleaseAttempt, ReleaseGate, Evidence, AuditEvent, LlmCall, Source, Action, System
```

Compatibility:

- Preserve all current variant spellings and serde names.
- Add migration tests for old JSON fixtures.
- Add taxonomy tests proving every visible object kind has a label, route, and default icon/badge.

### 8.2 Read Model

`TuiReadModel` target fields:

```text
schema_version
generated_at
event_cursor
runtime
freshness
mission
fleet
repo_families
repos
workflow
queue
runners
cache
vti
agents
autonomy
bugs
git_sync
bottlenecks
jankurai
aer
security
artifacts
release
evidence
settings
source_doctor
llms
churn
incident
replay
system
attention
next_action
```

Rules:

- Schema version bumps to `tui.v2.0`.
- Domain fields use stable dashboard structs under `src/api/dashboards/*`.
- Unknown or unavailable domains render explicit degraded/empty state, not silent omission.
- Every dashboard carries source/freshness metadata.

### 8.3 Freshness And Runtime Profile

Add `src/api/freshness.rs`:

```text
SourceKind:
  GitLab, StateDb, Docker, Cache, Vault, Broker, Autonomy, Webhook,
  ActionRegistry, Capability, Jankurai, Aer, Security, ArtifactStore,
  McpHttp, McpResource, InspectionHttp, Fixture

FreshnessState:
  Live, Fresh, Stale, LastKnown, Inferred, Partial, SourceDown, Unknown

SourceFreshness:
  source
  state
  observed_at
  age_ms
  cursor
  ttl_ms
  confidence
  last_error
  degraded_reason
```

Add `src/api/runtime_profile.rs`:

```text
RuntimeProfile:
  runtime_profile
  state_backend
  state_backend_url_redacted
  broker_backend
  build_sha
  feature_flags
  schema_version
  db_schema_hash
  action_registry_hash
  mcp_manifest_hash
  inspection_api_version
  config_paths_redacted
```

Acceptance:

- Header displays worst-source freshness and actual backend.
- Source Doctor shows every source's age, cursor, last error, and degraded reason.
- Stale/partial/down sources block R4/R5 actions unless a typed override policy explicitly allows proceeding.

### 8.4 Events

`TuiEvent` must cover:

```text
system health
source freshness
pipeline lifecycle
pipeline bridge lifecycle
job lifecycle
job trace chunk/annotation
test/VTI lifecycle
selector miss
agent lifecycle
agent race
capability grant
admission decision
cache hit/miss/taint/lease/verdict/GC
release gate
secret audit
bug lifecycle
Jankurai and AER findings
security findings
artifact signature/SBOM/provenance
LLM call and budget events
action preview/execute/receipt/failure
```

Event rules:

- Monotonic cursor.
- Every event has at least one `EntityRef`.
- Every event has source freshness context.
- Event gaps are visible in the TUI and recoverable via paged fetch.

### 8.5 Actions

Risk tiers:

| Tier | Meaning | Confirmation |
|---|---|---|
| R0 | Read-only inspection. | None. |
| R1 | Local safe state mutation. | Inline preview or one-key confirm. |
| R2 | CI/test mutation. | Preview with expected side effects. |
| R3 | Repo mutation. | Preview plus named confirmation. |
| R4 | Release/merge mutation. | Proof modal plus typed target SHA/version. |
| R5 | Production/security/destructive mutation. | Proof modal, typed confirmation, dry-run proof, and secondary approval where policy requires. |

Migration:

```text
ReadOnly   -> R0
Low        -> R1 or R2 per action
High       -> R3 or R4 per action
Production -> R4 or R5 per action
```

Action metadata must include:

```text
id
label
risk_tier
side_effect_class
surfaces
dry_run_supported
grant_requirements
confirmation_policy
expected_evidence
idempotency_policy
undo_or_compensating_action
source_freshness_requirements
redaction_policy
```

Acceptance:

- All 34 current action IDs resolve.
- Mutating actions require grants or explicit local authorization.
- R4/R5 actions require typed proof confirmation.
- TUI, CLI, MCP, and capability registry share one action contract.

## 9. Inspection API

Add versioned HTTP endpoints:

```text
GET  /api/v1/read-model
GET  /api/v1/events?cursor=&limit=&kind=&entity_kind=&entity_id=
GET  /api/v1/events/stream?cursor=
GET  /api/v1/entity/{kind}/{id}
GET  /api/v1/proof?entity=&kind=&since=&actor=&cursor=&limit=
GET  /api/v1/runtime/profile
GET  /api/v1/health/deep
GET  /api/v1/action-registry
POST /api/v1/action/preview
POST /api/v1/action/execute
GET  /api/v1/action/{run_id}/stream
GET  /api/v1/repos
GET  /api/v1/families
GET  /api/v1/queue
GET  /api/v1/runners/capacity
GET  /api/v1/cache/dashboard
GET  /api/v1/vti/dashboard
GET  /api/v1/agents/dashboard
GET  /api/v1/autonomy/dashboard
GET  /api/v1/bugs/dashboard
GET  /api/v1/git-sync/dashboard
GET  /api/v1/bottlenecks/dashboard
GET  /api/v1/jankurai/dashboard
GET  /api/v1/aer/dashboard
GET  /api/v1/security/dashboard
GET  /api/v1/artifacts/dashboard
GET  /api/v1/release/dashboard
GET  /api/v1/llms/dashboard
GET  /api/v1/source-doctor/dashboard
```

Backend rules:

- Routes return typed `src/api` contracts.
- Projections read from DB-owned repos, collectors, or adapters only.
- No raw SQL outside DB-owned modules.
- Route tests use in-memory SQLite.
- Redaction tests cover runtime profile, proof, traces, LLM data, and config paths.
- Health/deep never returns "healthy" for missing proof or source-down critical data.

## 10. MCP Resources

Keep MCP tools focused on actions during HTTP stabilization. Add read-only resources after HTTP contracts are stable:

```text
jeryu://system/snapshot
jeryu://runtime/profile
jeryu://events?cursor=N
jeryu://proof?entity=...
jeryu://repos
jeryu://repos/{slug}
jeryu://families/{family}
jeryu://queue
jeryu://runners/capacity
jeryu://cache/dashboard
jeryu://vti/dashboard
jeryu://agents/dashboard
jeryu://autonomy/dashboard
jeryu://bugs/dashboard
jeryu://git-sync/dashboard
jeryu://bottlenecks/dashboard
jeryu://jankurai/dashboard
jeryu://aer/dashboard
jeryu://security/dashboard
jeryu://artifacts/dashboard
jeryu://release/latest
jeryu://jobs/{project_id}/{job_id}/trace
jeryu://pipelines/{project_id}/{pipeline_id}/jobs
jeryu://settings/effective
```

MCP rules:

- Do not enable MCP HTTP GET as an accidental side effect.
- Resource names mirror inspection contract names.
- Resource output uses `src/api` contracts.
- Action tools continue through capability policy and action registry.

## 11. App State, Reducer, And Selectors

Intent flow:

```text
Input::Key / Input::Mouse
Sync::ReadModelArrived
Stream::EventArrived
Stream::SourceDegraded
Action::PreviewArrived
Action::ReceiptArrived
Runtime::Tick
          |
          v
Reducer: (AppState, Intent) -> AppState
          |
          v
Selectors: AppState -> LensInput
          |
          v
Pure Ratatui render
```

`AppState` owns:

```text
read_model
event_cursor
entity_cache
route_stack
breadcrumbs
focus_state
selection_by_pane
filters
pending_action
stream_status
diagnostics
bounded_logs
bounded_events
```

Rules:

- Reducers are pure and table-tested.
- I/O produces intents but does not mutate state directly.
- Selectors are pure and lens-local unless shared.
- Rendering reads selector output only.
- Hot buffers are bounded.
- Selection is keyed by `EntityRef`, not row index.

## 12. Data Client And Streaming

`DataClient` hides transport choice:

```text
fetch_read_model()
subscribe_events(cursor)
fetch_events(cursor, limit, filters)
fetch_entity(kind, id)
fetch_proof(query)
fetch_action_registry()
preview_action(request)
execute_action(request)
subscribe_action(run_id)
fetch_trace(entity)
```

Implementations:

| Client | Purpose |
|---|---|
| `HttpDataClient` | Primary `/api/v1/*` transport. |
| `McpResourceDataClient` | Read-only MCP resources after HTTP stabilization. |
| `LocalDataClient` | Degraded local fallback through approved DB-owned adapters only. |
| `FixtureDataClient` | `--demo`, tests, screenshots, and offline development. |

Streaming order:

1. Events stream over inspection HTTP/SSE when available.
2. Fallback to paged HTTP polling with visible `[poll]` badge.
3. Fallback to local/fixture state with `LAST KNOWN` or `FIXTURE` badge.

Streaming acceptance:

- Cursor resume works after disconnect.
- Gap fetch happens before new events are applied.
- High-volume bursts coalesce without losing cursor correctness.
- Dropped frames do not drop durable events.
- Source gaps are visible in header and Source Doctor.

## 13. Navigation And Keyboard Grammar

Global keys:

| Key | Behavior |
|---|---|
| `Enter` | Drill into focused entity/pane. |
| `Esc` | Pop overlay, drilldown, route, or focus scope. Never dead-ends. |
| Arrows | Move spatially inside the active focus graph. |
| `Tab` / `BackTab` | Move between focus worlds or major lenses. |
| `:` | Command palette. |
| `/` | Search/filter in current scope. |
| `a` | Open action menu for focused entity. |
| `e` | Open evidence/proof for focused entity. |
| `l` | Open logs/trace for focused job/pipeline/entity where available. |
| `x` | Expand/collapse focused pane or toggle incident action affordance by context. |
| `?` | Contextual help. |
| `g<key>` | Jump to a lens. |

Lens jump defaults:

```text
g0 mission
gq queue
gr repos
gw workflow
gl logs or llms by command palette disambiguation
gu runners
gc cache
gv vti
ga agents
go autonomy
gb bugs
gg git sync
gB bottlenecks
gj jankurai
gA aer
gS security
gf artifacts
gR release
ge evidence
gs settings/source doctor
gC churn
gi incident
gp replay
```

Tuiwright acceptance:

- `Enter` drills and `Esc` unwinds on every lens.
- Focus restores after tab/lens switch.
- Command palette can route to every lens and entity type.
- Tiny terminal mode preserves breadcrumb and worst-freshness; lower-priority chips collapse first.

## 14. Widgets, Theme, And Badges

Theme modules own all colors, glyphs, badges, progress styles, and terminal capability fallbacks.

Freshness badges:

```text
LIVE
FRESH <age>
STALE <age>
LAST KNOWN
INFERRED
PARTIAL
SOURCE DOWN
UNKNOWN
NO PROOF
UNVERIFIED
[poll]
FIXTURE
```

Proof confidence:

```text
MEAS   measured proof
STRUCT structurally derived
HIST   historical estimate
HEUR   heuristic
MISS   missing
STALE  stale proof
```

Widget library:

```text
header
tabs
status strip
freshness chip
attention queue
virtual table
inspector
timeline
DAG
progress
sparkline
heatmap
event tape
log viewer
proof chip/modal
forms
entity link
command palette
help
```

Accessibility requirements:

- ASCII fallback.
- No-color mode.
- High-contrast mode.
- Reduced-motion mode.
- Stable focus order.
- Text must fit in all target terminal sizes.

## 15. Action Path

```text
Focused entity -> action key/menu
  -> Intent::ActionRequested { action_id, target }
  -> reducer records pending action
  -> DataClient.preview_action
  -> preview modal renders risk, source freshness, side effects, grants, proof requirements
  -> confirmation policy by R tier
  -> DataClient.execute_action
  -> action stream updates progress
  -> receipt arrives with evidence IDs and event cursor
  -> reducer clears pending action and links receipt to entity/proof
```

Action safety invariants:

- No R1+ action executes without preview.
- No R3+ action executes without a proof modal.
- No R4/R5 action executes with stale critical source data unless policy explicitly allows an override and records it.
- Canceling a modal does not mutate state.
- Repeated keypresses require idempotency keys and cannot double-execute.
- Receipts are addressable and searchable in Evidence.

## 16. Screens And Lens Goals

| Lens | Goal |
|---|---|
| Mission | Global posture, top blocker, source freshness, attention, next action. |
| Queue | Capacity truth, theoretical limits, bottleneck class, whether adding runners helps. |
| Repos | Family/repo health, scope, drilldown, attention by family. |
| Workflow | Multi-pipeline atlas, DAG, MR/PR rail, critical path, job drilldown. |
| Logs | Cursor-aware bounded trace viewing with source and gap state. |
| Runners | Pool/node/tag/resource/trust health and scale/drain previews. |
| Cache | SmartCache performance, fullness, taints, misses, hot objects, GC plan. |
| VTI | Test selection safety, confidence, misses, flakes, repair recommendations. |
| Agents | Agent sessions, tasks, grants, races, branches, evidence, logs. |
| Autonomy | Kill bell, freeze windows, verdicts, policy, launch ledger. |
| Bugs | Ready/blocked/racing/fixed bugs and attempts. |
| Git Sync | Local/remote/MR/mirror/admission state. |
| Bottlenecks | Historical and structural CI slowdowns with decomposition. |
| Jankurai | Score, findings, caps, duplicates, generated-zone drift. |
| AER | Agent evidence/review findings and repair links. |
| Security | Scans, grants, policy violations, admission denials, secrets metadata. |
| Artifacts | Signatures, SBOM, provenance, verification, release linkage. |
| Release | Candidate, canary, production, gates, rollback, approvals. |
| Evidence | Searchable proof timeline and entity proof graph. |
| Settings | Runtime profile, transport, keymap, redacted config. |
| Source Doctor | Source health, schema/action/MCP/docs drift, DB profile mismatch. |
| LLMs | Provider health, key policy, spend, budget, usage, failures. |
| Churn | Change volume correlated with risk and failures. |
| Incident | Pinned emergency view with high-contrast decision ledger. |
| Replay | Event-cursor replay for postmortem and demo. |

## 17. Work Plan

The reset lands as small PR-sized units. The old TUI keeps compiling until replacement routes are proven.

### Track 0: Foundation Lock

| Unit | Goal | Outputs | Proof |
|---|---|---|---|
| U00 | Final baseline and routing lock. | `TUI_RESET_PLAN_FINAL.md`; owner/test-map follow-up issue if needed. | `git diff --check`; plan review. |
| U01 | Module skeleton and compatibility feature. | New directories, manifests, stubs, no behavior change. | `cargo check -p jeryu --message-format=json`. |
| U02 | File-size lint and header templates. | `scripts/loc_audit.sh`; header examples; CI hook proposal. | lint self-test; `just score` when owner-map touched. |
| U03 | Tuiwright helper harness. | shared spawn/capture/assertion helpers; no test deletion. | current `tests/tui_tuiwright.rs` still passes. |

### Track 1: Contracts And Inspection Plane

| Unit | Goal | Dependencies | Acceptance |
|---|---|---|---|
| U04 | Expand `EntityKind`, dashboard structs, and schema version. | U01 | serde round trips; legacy JSON still deserializes. |
| U05 | Freshness, proof, capacity, and runtime profile contracts. | U01 | state classifier tests; redaction tests; backend profile tests. |
| U06 | R0-R5 action model and registry parity. | U04 | all 34 current IDs resolve; alias tests pass; mutating actions require grants. |
| U07 | Inspection API read endpoints. | U04-U06 | read-model, events, entity, proof, runtime, deep health, action-registry route tests. |
| U08 | Inspection action endpoints and streams. | U06-U07 | preview, execute, action stream, idempotency, cancellation, receipt tests. |

### Track 2: TUI Foundation

| Unit | Goal | Dependencies | Acceptance |
|---|---|---|---|
| U09 | App state, reducers, selectors. | U04-U06 | reducer/selector/route tests; render-only app builds. |
| U10 | Data client and stream fallback. | U07-U08 | HTTP/fixture clients; poll fallback; cursor resume tests. |
| U11 | Focus and navigation split. | U09 | Enter/Esc/arrows/Tab tests; focus restore tests. |
| U12 | Theme and badges. | U05 | render tests for freshness/proof/risk badges; ASCII/no-color tests. |
| U13 | Shared widgets baseline. | U09, U12 | widget render tests at canonical sizes. |
| U14 | UI shell, runner, and CLI compatibility. | U09-U13 | `--once`, `--demo`, `--capture`, `--screenshot`, `--tab`, size flags smoke tests. |
| U15 | Fixture backend and scenarios. | U10, U13 | deterministic fixtures for healthy/degraded/stale/security/release/cache/VTI/agent/bug/Jankurai/incident. |

### Track 3: Core Cockpit Lenses

| Unit | Goal | Dependencies | Acceptance |
|---|---|---|---|
| U16 | Mission lens. | U13-U15 | posture, top blocker, freshness, next action, proof links. |
| U17 | Queue and theoretical limit lab. | U05, U13-U15 | capacity formulas; "does adding runners help" scenarios. |
| U18 | Repos/families/repo drilldown. | U11, U13-U15 | fleet -> family -> repo routing; scoped attention/filtering. |
| U19 | Workflow atlas model and delivery split. | U04, U15 | model tests; demo PR/pipeline story preserved. |
| U20 | Workflow canvas, rails, inspector, logs. | U19 | drill repo -> workflow -> job -> log -> evidence. |
| U21 | Evidence flight recorder. | U07, U13-U15 | proof search, entity proof graph, receipts, redacted bundle stub. |

### Track 4: Domain Lenses

| Unit | Goal | Dependencies | Acceptance |
|---|---|---|---|
| U22 | Runners and nodes. | U17 | pool/node/tag/resource/trust views and scale/drain preview. |
| U23 | Cache observatory. | U13-U15 | hit/miss/taint/fullness/GC/lease/verdict scenarios. |
| U24 | VTI, tests, flake radar. | U21 | low-confidence VTI never renders green. |
| U25 | Agents, autonomy, LLM governance. | U21 | sessions/tasks/grants/races/kill bell/budget redaction. |
| U26 | Bugs, git sync, review queue. | U18, U21 | bug -> attempt -> branch/MR -> pipeline -> evidence trace. |
| U27 | Release, security, secrets, artifacts. | U06, U21, U22 | typed confirmations; secret redaction; artifact verification. |
| U28 | Jankurai, AER, bottlenecks, churn. | U05, U21 | drift/finding/churn/bottleneck fixtures and source links. |
| U29 | Settings, Source Doctor, incident, replay. | U05, U07, U21 | API down, MCP drift, schema mismatch, stale docs, replay scenarios. |

### Track 5: Hardening And Final Gate

| Unit | Goal | Dependencies | Acceptance |
|---|---|---|---|
| U30 | Split Tuiwright monolith. | U03, U15, active lenses | existing assertions preserved; suites under size cap. |
| U31 | Expand black-box coverage. | U16-U29 | capture, navigation, responsive, streams, actions, redaction, source doctor, accessibility, performance, flicker, replay. |
| U32 | Performance and resilience. | U10, U31 | 500 repos, 10k jobs, event bursts, trace subscriptions, panic terminal restore, memory bounded. |
| U33 | Remove old routes and feature flag. | U16-U32 | old oversized files deleted or minimized; default `jeryu tui` uses Flight Deck. |

## 18. Parallelism Rules

```text
U00 -> U01 -> U04/U05/U06
U07 depends on U04-U06
U08 depends on U06-U07
U09 depends on U04-U06
U10 depends on U07-U08
U11-U13 depend on U09 and/or U05
U14-U15 depend on U10-U13
U16-U21 depend on U13-U15
U22-U29 run mostly in parallel after their listed core dependencies
U30-U32 run as lenses stabilize
U33 is final cleanup
```

File conflict rules:

- `src/tui.rs`, `src/api/entity.rs`, `src/api/read_model.rs`, and `src/tui/app/mod.rs` are single-owner files during their units.
- Lens units should only touch their own lens directory, shared widgets they need, and fixture files for that lens.
- Shared fixtures are split per lens to avoid merge contention.
- Backend projection files are split per domain.
- No page/lens unit modifies DB schema directly; schema changes happen in DB-owned units.

## 19. Tuiwright Coverage Matrix

| Suite | Required coverage |
|---|---|
| Capture | Every lens at `80x24`, `100x30`, `120x36`, `160x48`, `220x60`, with non-empty ink and page labels. |
| Navigation | Route drilldown, `Esc` unwind, focus restoration, pane movement, command palette navigation. |
| Responsive | Tiny, compact, medium, wide, ultra-wide layouts without text overlap. |
| Streams | Live events, disconnect, stale marker, reconnect, cursor resume, gap fetch. |
| Actions | Preview required, typed confirmation, idempotency, key-repeat protection, receipt navigation. |
| Redaction | No tokens/secrets in screenshots, text dumps, bundles, panic output, copied paths. |
| Source Doctor | API down, MCP drift, schema mismatch, stale docs, DB profile mismatch, source-down critical data. |
| Accessibility | ASCII fallback, no-color mode, high contrast, reduced motion, stable focus order. |
| Performance | Large fixtures, event bursts, huge tables, trace subscriptions, long session. |
| Flicker | Empty refresh preserves prior, selection by id, sticky log tail, stale dims not blanks, bursts do not jump. |
| Replay | Recorded event streams for job fail/retry, OOM, cache miss storm, VTI miss, agent race, canary rollback. |
| Incident | Pinned emergency view, high contrast, decision ledger, action proof links. |

## 20. Anti-Flicker Invariants

| Invariant | Test |
|---|---|
| Empty refresh does not blank a populated lens; prior data is marked stale. | `flicker_empty_preserves_prior` |
| Selection follows `EntityRef`, not row index. | `flicker_selection_id_anchored` |
| Follow-tail mode stays sticky across trace updates. | `flicker_log_tail_sticky` |
| Stale data dims and shows age; it is not silently deleted. | `flicker_stale_dims_not_blanks` |
| Event bursts do not move focused row/pane unexpectedly. | `flicker_burst_no_jump` |

## 21. Risk Register

| Risk | Mitigation |
|---|---|
| Inspection API work lags behind TUI work. | Fixture-first lenses and `FixtureDataClient` keep TUI work unblocked. |
| App refactor cascades across call sites. | Compatibility tree and small reducer units; old app compiles until route flip. |
| Tuiwright suite split loses assertions. | Count and map every existing `#[test]`; split first, expand second. |
| Backend router style conflicts with autonomy HTTP invariants. | Route adapter follows the hosting surface; no new HTTP stack without explicit owner approval. |
| Source freshness makes UI noisy. | Header shows worst source; details live in Source Doctor and per-lens badges. |
| Proof modal blocks legitimate emergency work. | R5 emergency override requires typed reason, stale-source disclosure, and receipt. |
| R0-R5 migration breaks CLI/MCP/capability users. | Alias tests, action ID parity, CLI smoke, MCP manifest parity. |
| Over-splitting creates too many files. | Lens template is fixed; subcomponent splits require real cohesion; file count tracked. |
| RedlineDB path regresses. | Runtime profile tests and `just runtime-redlinedb-jansu` before final gate. |
| Raw SQL leaks into TUI/inspection projections. | DB boundary grep and proof-lane review; projections use DB-owned repos/adapters. |
| Secret/LLM data leaks into captures. | Redaction suite covers screenshots, text, bundles, panic output, and copied paths. |

## 22. Proof Commands

| Scope | Command |
|---|---|
| TUI library | `cargo nextest run -p jeryu --lib tui::` |
| API contracts | `cargo nextest run -p jeryu --lib api::` |
| Inspection/API | `cargo test -p jeryu --tests -- --test-threads=1` |
| Current Tuiwright monolith | `TERM=xterm-256color cargo test --test tui_tuiwright -- --test-threads=1` |
| Future Tuiwright split | `cargo nextest run -p jeryu --test tuiwright` or repo-approved alias |
| Fast gate | `just fast` |
| Audit | `just score` |
| Security/redaction | `just security` |
| SQLite/Kafka profile | `just runtime-sqlite-kafka` |
| RedlineDB/Jansu profile | `just runtime-redlinedb-jansu` |
| Full merge gate | `just check` |

For root planning-doc edits, run at minimum `git diff --check` on the edited document. Run `just score` when owner/test-map/generated audit routing changes or before merge readiness; it updates generated score artifacts by design.

## 23. Final Acceptance Criteria

The reset is complete when:

1. `jeryu tui` opens Flight Deck and shows fleet posture, top blocker, source freshness, and next action within five seconds.
2. Every visible operational object is addressable, drillable, explainable, or explicitly non-interactive.
3. `Enter`, `Esc`, arrows, `Tab`, `:`, `/`, `a`, `e`, `l`, `x`, `?`, and `g<key>` are consistent everywhere.
4. No renderer performs backend/system calls.
5. All mutating actions use preview, proof when required, confirmation, execute, stream, and receipt.
6. Stale/partial/degraded/source-down data is visible and blocks risky actions where appropriate.
7. Queue lens explains whether adding runners helps.
8. Workflow supports family -> repo -> MR/PR -> pipeline -> job -> trace -> evidence.
9. Evidence can explain every green, warning, failure, and action.
10. Cache, VTI, agents, autonomy, bugs, release, security, artifacts, Jankurai, AER, LLMs, settings, Source Doctor, incident, and replay are integrated into one entity/proof graph.
11. Tuiwright covers pages, layouts, interactions, degraded backends, safety, redaction, accessibility, replay, flicker, and performance.
12. No new source file violates file-size budgets without a documented exception.
13. Existing oversized TUI files are split, deleted, or reduced to compatibility shims.
14. SQLite remains default and RedlineDB remains feature/config gated.
15. The TUI remains useful during backend failure through stale/degraded/empty/fixture states.
16. Redacted evidence bundles can be exported for selected entities/actions.
17. Action, MCP, CLI, DB schema, docs, and source freshness drift are detectable in Source Doctor.
18. Old compatibility routes and feature flags are removed after the Flight Deck path is default and tested.

## 24. Immediate Next Steps

1. Treat this file as the controlling plan and stop maintaining separate competing reset plans.
2. Land U01 as a mechanical skeleton/compatibility PR with no product behavior changes.
3. Land U04-U06 contract work before backend/TUI integration.
4. Add U07 inspection read endpoints with in-memory SQLite tests.
5. Build U09-U15 fixture-first TUI foundations.
6. Start lenses with Mission, Queue, Repos, Workflow, and Evidence.
7. Split `tests/tui_tuiwright.rs` before broadening the suite, preserving existing assertions first.
