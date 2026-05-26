# JeRyu TUI Reset Master Plan

Status: controlling implementation plan
Product name: JeRyu Flight Deck
Internal shorthand: Hyperdeck
Baseline date: 2026-05-26
Target command: `jeryu tui`

## Summary

JeRyu's TUI will be rebuilt into Flight Deck: a Rust terminal-native operating room for software delivery. It must show the whole fleet, every repo family, CI queue, pipeline, runner, cache, VTI decision, agent, bug, release gate, security finding, artifact, and proof trail as one realtime, evidence-backed entity graph.

This reset is not a visual cleanup. It is an architecture reset around typed contracts, backend inspection APIs, deterministic app state, reusable widgets, safe action execution, and broad Tuiwright proof coverage.

## Non-Negotiable Outcomes

| Outcome | Requirement |
|---|---|
| Total fleet awareness | Opening `jeryu tui` answers what is healthy, blocked, stale, risky, and actionable in under five seconds. |
| Every visible thing addressable | Rows, cards, graph nodes, badges, logs, evidence, grants, artifacts, and actions map to `EntityRef` or explicit proof IDs. |
| Proof-backed status | Green requires evidence; missing proof renders as `NO PROOF`, `HEUR`, `STALE`, `PARTIAL`, or `SOURCE DOWN`. |
| One mutation path | All mutating work uses action registry -> preview -> proof/confirmation -> execute -> stream -> receipt. |
| Renderer purity | Draw code consumes immutable view models only. No DB, GitLab, Docker, Vault, filesystem, MCP, or network calls during render. |
| Source freshness everywhere | Every screen exposes source, age, cursor, last error, confidence, and degraded state. |
| Tuiwright proof | Primary UX behavior is black-box tested through Tuiwright captures, interactions, degradation, redaction, and performance scenarios. |
| File layout quality | Reset must split oversized TUI files and keep new modules small, reusable, and replaceable. |
| DB boundary | SQLite remains default and RedlineDB remains explicit feature/config only. Raw SQL stays inside DB-owned code. |

## Reset Baseline

This audit freezes the starting point for the reset.

| Area | Current fact | Reset consequence |
|---|---|---|
| Root artifact | `TUI_RESET_PLAN.md` did not exist at audit time. | This file is the coordination artifact for the reset. |
| TUI root | `src/tui.rs` is a thin module root exporting `run_tui`, `run_tui_once`, `run_tui_screenshot`, and `capture_tui_png`. | Preserve public entry points and keep the root thin. |
| TUI module count | `src/tui/` contains 119 Rust files across `aer`, `flow`, `jankurai`, `proof_lanes`, `runtime`, `vrc`, `widgets`, `witness`, and `workflow`. | Migrate incrementally behind compatibility exports; do not big-bang delete working surfaces. |
| TUI app state | `src/tui/app.rs` owns broad state, clients, focus, channels, snapshots, action adapter, and selected tab. | Split state/reducer/selectors/data-client concerns before adding feature depth. |
| Runtime | Existing runtime files include input, render, maintenance, mouse, palette, and navigation handlers. | Keep behavior, but move toward deterministic reducer/event paths. |
| Existing screens | Active tabs include Workflow, Mission, Release, Approvals, Jobs, Agents, Tests, Pools, Cache, Evidence, Bugs, LLMs, Git, Secrets, and Jankurai. | Preserve command compatibility while expanding to Flight Deck screen set. |
| Workflow | Existing workflow has DAG, PR rail, phase rail, minimap, delivery, live delivery, inspector, action adapter, and render tests. | Reuse workflow model/render work; split oversized files before adding more logic. |
| Widgets | Existing widgets include attention, inspector, mission, sparkline, status badge, timeline, VTI proof, and agent fleet. | Promote reusable widgets into the target widget library. |
| API contracts | `src/api` already has `actions`, `agent_session`, `entity`, `events`, `read_model`, and `snapshot`. | Expand `src/api` as the canonical shared contract; do not create parallel DTOs. |
| Entity taxonomy | Current `EntityKind` covers Job, Pipeline, Agent, AgentTask, MergeRequest, TestPlan, TestCase, EvidenceCapsule, ReleaseAttempt, ReleaseGate, CacheTaint, CacheObject, Bug, BugAttempt, Project, SecretAccess, Grant, Pool, Runner, System. | Expand taxonomy to the full fleet graph while keeping serialization stable. |
| Read model | Current `TuiReadModel` has schema version, generated time, cursor, freshness, mission, attention, next action, and system health. | Preserve shape and expand to domain dashboards instead of per-page private models. |
| Freshness | Current `DataFreshness` is age-only by source plus `overall_stale`. | Replace with source state, cursor, confidence, TTL, last error, degraded reason, and redaction-aware runtime profile. |
| Events | Current `TuiEventKind` covers system, pipeline/job/log, test/VTI, agent, grant, admission, cache, release, security, action, and snapshot events. | Add missing event families and proof/source metadata; keep cursor semantics monotonic. |
| Actions | Current registry has 34 entries, four risk tiers (`ReadOnly`, `Low`, `High`, `Production`), side-effect classes, grants, dry-run flag, and surfaces. | Evolve to R0-R5 risk tiers and richer action metadata; generate command palette/action menus from registry data. |
| Current action IDs | `open_logs`, `requeue_job`, `remove_record`, `pause_pool`, `explain_blockers`, `fetch_capsule`, `get_system_snapshot`, `get_pipeline_jobs`, `get_ci_bottlenecks`, `propose_patch`, `race_patches`, `request_merge`, `plan_validation`, `bug_submit`, `bug_list`, `bug_show`, `bug_ready`, `bug_update`, `bug_record_attempt`, `run_tests`, `next_action`, tab navigation, `toggle_audit_ledger`, `quit`. | Preserve IDs where possible; add versioned aliases only when semantics change. |
| Daemon HTTP | Autonomy HTTP currently serves `GET /metrics`, `GET /health`, and `POST /events`; no versioned inspection API exists. | Add `/api/v1/*` inspection plane rather than overloading shallow health/webhook routes. |
| MCP HTTP | MCP HTTP route is `/mcp` with POST and DELETE; GET returns method-not-allowed because Streamable HTTP GET is disabled. | Keep action-oriented MCP tools; add read-only MCP resources after HTTP inspection stabilizes. |
| MCP tools | Capability MCP manifest is generated from action registry capability entries. Autonomy has separate descriptors for Evidence Gate operations. | Align action registry, capability policy, MCP manifest, and future resources through one contract. |
| State DB | DB schema is embedded in `db/state.rs`; typed repos exist under `src/db/`; audit found 54 durable tables across state, cache, release, secrets, bugs, autonomy, LLM budget, and evidence domains. | All durable truth must be added through DB-owned schema/repo modules. |
| Backend profile | `Cargo.toml`, `db/config.rs`, and `db/state.rs` keep SQLite default and RedlineDB behind `redlinedb-backend` / explicit Redline URLs. | Maintain DB boundary and feature gating through the reset. |
| Current Tuiwright | `tests/tui_tuiwright.rs` is centralized at 1251 lines and covers capture, primary tabs, bugs, workflow focus/drilldown, fleet bar, Jankurai, overlays, command palette, and some repo discovery. | Split into focused suites and broaden coverage to degradation, redaction, source doctor, actions, streams, and performance. |
| Command compatibility | CLI supports `jeryu tui`, `--demo`, `--capture`, `--screenshot`, `--tab`, `--output`, `--width`, `--height`, and `--screenshot-hold-ms`. | Preserve these flags. |
| Dirty worktree | Audit observed existing uncommitted TUI changes. | Reset work must not revert unrelated local changes. |

## Oversized File Inventory

Reset work touching these files must split them or avoid adding meaningful logic to them.

| File | Lines | Status |
|---|---:|---|
| `tests/tui_tuiwright.rs` | 1251 | Split into suites. |
| `src/tui/workflow/widget.rs` | 1063 | Hard-cap violation. |
| `src/tui/workflow/delivery.rs` | 1058 | Hard-cap violation. |
| `src/tui/workflow/action_adapter.rs` | 1003 | Hard-cap violation. |
| `src/tui/workflow/model.rs` | 963 | Hard-cap violation. |
| `src/tui/app_runtime_sync_actions.rs` | 804 | Hard-cap violation. |
| `src/tui/workflow/inspector.rs` | 728 | Hard-cap violation. |
| `src/tui/app_runtime.rs` | 649 | Hard-cap violation. |
| `src/tui/workflow/live_delivery.rs` | 582 | Hard-cap violation. |
| `src/tui/runtime/input/navigation/general.rs` | 573 | Hard-cap violation. |
| `src/tui/app_runtime_sync.rs` | 564 | Hard-cap violation. |
| `src/tui/app_runtime_demo_state.rs` | 525 | Hard-cap violation. |
| `src/tui/ui_panels_body.rs` | 482 | Hard-cap violation. |
| `src/tui/focus.rs` | 479 | Hard-cap violation. |
| `src/tui/repo_fleet_bar.rs` | 473 | Hard-cap violation. |
| `src/tui/app.rs` | 466 | Hard-cap violation. |
| `src/tui/workflow/nav.rs` | 451 | Complex-file cap exceeded. |
| `src/tui/app_runtime_sync_background.rs` | 440 | Complex-file cap exceeded. |
| `src/tui/bugs.rs` | 438 | Complex-file cap exceeded. |
| `src/tui/activity.rs` | 437 | Complex-file cap exceeded. |
| `src/tui/app_runtime_sync_tests.rs` | 414 | Test-file target exceeded. |
| `src/tui/ui.rs` | 401 | Complex-file target exceeded. |
| `src/tui/action_registry_entries.rs` | 368 | Normal-file cap exceeded. |
| `src/tui/ui_chrome.rs` | 369 | Normal-file cap exceeded. |
| `src/tui/ui_panels_body_bugs.rs` | 363 | Normal-file cap exceeded. |
| `src/tui/ui_panels_body_llms.rs` | 360 | Normal-file cap exceeded. |

## Preserve, Migrate, Delete

| Decision | Items |
|---|---|
| Preserve | `jeryu tui` command and flags, `src/tui.rs` public exports, demo/capture mode, existing action IDs where semantics remain valid, typed `src/api` contracts, DB feature gating, current MCP GET-disabled behavior, existing workflow/bug/fleet Tuiwright behavior, and all DB-owned SQL boundaries. |
| Migrate | `App` mutable state to `app/state.rs` plus reducer/selectors, runtime input/render loops to structured runtime modules, tab routing to `nav`, transport logic to `data`, action preview/execution to `actions`, per-screen drawing to `pages`, reusable view pieces to `widgets`, theme constants to `theme`, and Tuiwright helpers to test support modules. |
| Deprecate then delete | Ad hoc page DTOs that duplicate `src/api`, hardcoded screen action lists, render-time backend/filesystem calls, monolithic `ui_panels_*` once page modules replace them, centralized Tuiwright file after split, and any fallback state that can silently render stale/partial truth as healthy. |
| Do not add | Raw SQL outside `db/`, ad hoc SQLite usage outside DB-owned modules, RedlineDB defaults, backend/network/file calls during render, mutating TUI shortcuts that bypass preview/receipt, or meaningful new logic in files above the file-size caps. |

## Target Architecture

```text
GitLab REST/Webhooks  \
State DB               \
Docker/remotes          -> collectors/projections -> Inspection API/events -> TUI DataClient
SmartCache             /
Vault/secrets         /
Agents/autonomy      /
Jankurai/artifacts  /
Git/admission      /

TUI DataClient -> reducer/store/selectors -> pure Ratatui pages/widgets
                              |
                              +-> action flow: preview/proof/execute/receipt
```

## Target File Layout

Keep `src/tui.rs` as the public module root.

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
  runtime/
    mod.rs
    loop.rs
    render.rs
    terminal.rs
    tasks.rs
    backpressure.rs
    input/
      mod.rs
      keymap.rs
      keyboard.rs
      mouse.rs
      command.rs
  nav/
    mod.rs
    route.rs
    breadcrumbs.rs
    focus.rs
    focus_graph.rs
    history.rs
  data/
    mod.rs
    client.rs
    http.rs
    mcp.rs
    local.rs
    fixtures.rs
    stream.rs
    trace.rs
  actions/
    mod.rs
    registry.rs
    preview.rs
    execute.rs
    modal.rs
    risk.rs
  pages/
    mission.rs
    queue.rs
    repos.rs
    repo.rs
    workflow.rs
    logs.rs
    runners.rs
    cache.rs
    vti.rs
    agents.rs
    autonomy.rs
    bugs.rs
    git_sync.rs
    bottlenecks.rs
    jankurai.rs
    security.rs
    artifacts.rs
    release.rs
    evidence.rs
    settings.rs
    llms.rs
  widgets/
    header.rs
    tabs.rs
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
    freshness.rs
    log_viewer.rs
    proof.rs
    forms.rs
  theme/
    mod.rs
    palette.rs
    glyphs.rs
    terminal_caps.rs
  testing/
    fixtures.rs
    golden.rs
    interaction.rs
    assertions.rs
```

Add backend inspection modules outside the TUI.

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
```

## File Size Standard

| File type | Target | Hard cap |
|---|---:|---:|
| Normal implementation file | 150-250 lines | 350 lines |
| Complex renderer/model | 250-350 lines | 450 lines |
| Test file | 200-350 lines | 500 lines |
| Fixture file | Prefer split by scenario | 500 lines |
| Exceptions | Generated/declared artifacts only | Must be documented |

## Backend Interfaces

Add a versioned inspection API.

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
GET  /api/v1/security/dashboard
GET  /api/v1/artifacts/dashboard
GET  /api/v1/release/dashboard
GET  /api/v1/llms/dashboard
```

Add read-only MCP resources only after HTTP inspection stabilizes.

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
jeryu://security/dashboard
jeryu://artifacts/dashboard
jeryu://release/latest
jeryu://jobs/{project_id}/{job_id}/trace
jeryu://pipelines/{project_id}/{pipeline_id}/jobs
jeryu://settings/effective
```

## Core Contracts

Expand `EntityKind` to include:

```text
Fleet, RepoFamily, Repo, Branch, Commit, MergeRequest, Pipeline, PipelineBridge,
Stage, Job, RunnerPool, RunnerManager, Runner, Node, CacheObject, CacheRequest,
CacheVerdict, CacheTaint, TestPlan, TestCase, SelectorMiss, Agent, AgentSession,
AgentTask, AgentRace, Bug, BugAttempt, GitRefUpdate, AdmissionDecision,
CapabilityIntent, CapabilityGrant, JankuraiRun, JankuraiFinding, SecurityFinding,
SecretAuthority, ReleaseSecretSet, Artifact, Signature, Sbom, ReleaseAttempt,
ReleaseGate, Evidence, AuditEvent, LlmCall, Source, Action, System
```

`TuiReadModel` must include:

```text
schema_version, generated_at, event_cursor, runtime, freshness, mission, fleet,
repo_families, repos, workflow, queue, runners, cache, vti, agents, autonomy,
bugs, git_sync, bottlenecks, jankurai, security, artifacts, release, evidence,
llms, system, attention, next_action
```

`TuiEvent` must cover:

```text
system health, source freshness, pipeline lifecycle, job lifecycle, job trace,
test/VTI, selector miss, agent lifecycle, patch race, capability grant,
admission decision, cache hit/miss/taint/lease/verdict/GC, release gate,
secret audit, bug lifecycle, Jankurai findings, security findings,
artifact signatures, action lifecycle
```

Action tiers:

| Tier | Meaning |
|---|---|
| R0 | Read-only inspection. |
| R1 | Local safe state mutation. |
| R2 | CI mutation. |
| R3 | Repo mutation. |
| R4 | Release mutation. |
| R5 | Production/security/destructive mutation. |

## Shared Implementation Rules

| Rule | Requirement |
|---|---|
| Render purity | Draw functions take view data and focus only. |
| Backend access | Raw system access only in adapters/projections/repos. |
| DB boundary | No ad hoc SQL outside DB-owned modules. |
| Mutation | No mutating action without preview and receipt. |
| Testing | Every visible feature gets unit/render/Tuiwright coverage. |
| Fixture-first | Every screen works with deterministic fixtures before live wiring. |
| Redaction | Tests prove secrets do not render or export. |
| File size | Split before exceeding caps. |

## Parallel Work Model

| Wave | Units | Parallelism rule |
|---|---|---|
| Foundation | U01-U06 | Mostly serial; defines contract and module boundaries. |
| Shell/Data | U07-U12 | Parallel after contracts compile. |
| Core Cockpit | U13-U17 | Parallel by page after widgets and fixtures exist. |
| Domain Cockpits | U18-U24 | Parallel by domain using shared dashboard contracts. |
| Hardening | U25-U26 | Parallel after user-facing pages stabilize. |

## Units Of Work

### U01. Baseline Audit And Reset Cut Line

Goal: Freeze exactly what is being reset and what current code must preserve.

Work: Inventory current TUI modules, `src/api` contracts, endpoints, MCP tools, DB tables, action registry entries, Tuiwright coverage, and oversized files.

Acceptance: The reset team can name the baseline without re-reading every spec or spelunking the repo. This section is the first U01 deliverable.

### U02. File Layout And Module Budget Migration

Goal: Create reusable file layout before adding more UI.

Work: Keep `src/tui.rs` as root, create target `app`, `nav`, `data`, `actions`, `pages`, `widgets`, `theme`, and `testing` directories, and move logic behind compatibility exports.

Proof: `cargo check -p jeryu --message-format=json`; `cargo nextest run -p jeryu --lib tui::`.

Acceptance: Existing behavior compiles while new modules are ready for parallel page work.

### U03. Canonical API Contract Expansion

Goal: Make `src/api` the single source of truth for TUI and inspection contracts.

Work: Expand entity taxonomy, read model, event model, source freshness, runtime profile, proof query, action request/preview/result, dashboard views, and schema versioning.

Proof: Serialization round trips, default value tests, taxonomy tests, JSON fixture shape tests.

Acceptance: No page or endpoint invents private DTOs for the same domain concepts.

### U04. Runtime Profile And Freshness Model

Goal: Make "can I trust this screen?" answerable everywhere.

Work: Implement runtime profile fields, source freshness states, TTL policy, cursor metadata, degraded-source reasons, redacted config paths, feature flags, schema version, action registry hash, and MCP manifest hash.

Proof: Freshness transition tests, redaction tests, runtime profile tests for SQLite and Redline feature profiles.

Acceptance: Stale or partial data cannot silently masquerade as live truth.

### U05. Action Registry And Proof-Gated Action Model

Goal: Make action metadata strong enough for UI, CLI, MCP, capability, safety tests, and docs.

Work: Add complete risk tiers, side-effect classes, grant requirements, dry-run flags, confirmation policy, expected evidence, idempotency policy, and action surfaces.

Proof: Unique IDs, all mutating actions require grants, all production/security actions require typed confirmation, MCP/capability parity.

Acceptance: No hardcoded action lists remain in TUI screen code.

### U06. Inspection API Foundation

Goal: Add typed read endpoints to the daemon.

Work: Add `/api/v1/read-model`, `/api/v1/events`, `/api/v1/entity`, `/api/v1/proof`, `/api/v1/runtime/profile`, `/api/v1/health/deep`, and `/api/v1/action-registry`.

Proof: Route tests with in-memory SQLite, auth tests where required, redaction tests.

Acceptance: A simple client can fetch the full read model and at least one entity detail.

### U07. Event Ledger And Streaming

Goal: Replace polling-only behavior with cursor-based realtime.

Work: Normalize durable events, expose cursor paging, add SSE stream, add action stream, add trace stream fallback, coalesce high-volume updates, and preserve cursor correctness.

Proof: Cursor resume, disconnect/reconnect, event ordering, stream gap, coalescing, action stream tests.

Acceptance: Dropped frames do not lose data, and source gaps are visible.

### U08. Data Client Layer

Goal: Hide transport choice from UI pages.

Work: Add `DataClient` trait with HTTP, MCP-resource fallback, local fallback, and fixture implementations.

Proof: Mock server tests, fallback-order tests, fixture determinism tests.

Acceptance: No page knows whether data came from HTTP, MCP, local DB, or fixtures.

### U09. Reducer Store And App State Reset

Goal: Replace scattered mutable TUI state with deterministic state transitions.

Work: Add `AppState`, stores, reducer, selectors, event cursor, entity cache, route stack, focus state, filters, pending action state, diagnostics, and bounded buffers.

Proof: Reducer tests, selector tests, cache invalidation tests, route history tests.

Acceptance: Rendering reads selectors; input/data/action events mutate through reducer paths.

### U10. Navigation, Focus, And Command Palette

Goal: Make the universal keyboard grammar consistent.

Work: Implement route stack, breadcrumbs, macro/micro focus graph, configurable keymap, command palette, global search shell, contextual help, and focus restoration.

Proof: Key routing tests, focus traversal tests, Tuiwright drilldown/back tests.

Acceptance: `Enter` drills, `Esc` goes up, arrows move spatially, and `Tab` switches focus worlds.

### U11. Shared Widgets, Theme, And Accessibility

Goal: Prevent per-page UI duplication.

Work: Build header, tabs, status strip, freshness badge, attention queue, virtual table, inspector, timeline, DAG, progress, sparkline, heatmap, event tape, log viewer, proof modal, forms, command palette, and help widgets.

Proof: Render tests for every widget, no-color tests, ASCII fallback tests, reduced-motion tests.

Acceptance: Domain pages reuse shared components instead of ad hoc panels.

### U12. Fixture Backend And Demo Scenarios

Goal: Make every screen testable without live services.

Work: Add deterministic fixture scenarios for healthy, degraded, saturated, stale, release-blocked, security-blocked, cache-pressure, VTI-miss, agent-race, bug-ready, Jankurai-regression, and incident states.

Proof: Fixture determinism, fixture schema, capture smoke tests.

Acceptance: Every page can render populated, empty, loading, stale, and degraded states from fixtures.

### U13. Mission Control

Goal: Answer global posture immediately.

Work: Build Mission page with safe-to-code, safe-to-merge, safe-to-release, rollback readiness, top blocker, active agents, running/queued/failed jobs, cache/VTI/security/release status, and next action.

Proof: Attention ranking tests, Tuiwright healthy/degraded captures, proof navigation tests.

Acceptance: Every red/yellow/green state explains itself and links to proof.

### U14. Queue And Theoretical Limit Lab

Goal: Answer "should we add runners?" correctly.

Work: Build queue/capacity page with raw slots, effective slots, theoretical bounds, DAG bounds, resource headroom, tag fragmentation, trust tiers, policy gates, cache misses, VTI expansion, and recommendations.

Proof: Capacity formula tests, scenario matrix tests, Tuiwright saturated/idle/serial-DAG captures.

Acceptance: The UI distinguishes real capacity shortage from DAG/cache/VTI/policy/release blockers.

### U15. Repo Families And Repo Drilldown

Goal: Make repo families first-class navigation scopes.

Work: Build fleet/family/repo views, family aggregates, scoped attention, scoped queue, repo health, repo activity, and repo detail route.

Proof: Family grouping, repo filter, route persistence, Tuiwright family drilldown.

Acceptance: Family and repo scope affects every relevant page consistently.

### U16. Workflow Atlas, Pipeline DAG, And Logs

Goal: Replace disconnected workflow/job/log surfaces with one drillable atlas.

Work: Build multi-pipeline atlas, MR/PR rail, phase rail, DAG canvas, minimap, critical path, inspector, job detail, trace viewer, artifact links, failure capsules, and evidence links.

Proof: DAG layout, critical path, trace streaming, route drilldown, Tuiwright job trace path.

Acceptance: User can drill repo -> workflow -> job -> log -> capsule -> evidence.

### U17. Evidence Flight Recorder

Goal: Make evidence the truth spine.

Work: Build searchable proof timeline, entity proof graph, event filters, evidence detail, action receipts, grant/intents view, artifact/signature proof, VTI/test receipts, and redacted bundle export.

Proof: Proof query tests, redaction tests, evidence bundle tests, Tuiwright proof search captures.

Acceptance: `e` on any important entity opens relevant proof.

### U18. Runners, Nodes, And System Utilization

Goal: Show real execution capacity and node health.

Work: Build pools, managers, runners, nodes, tags, trust tiers, paused/draining state, CPU/memory/disk, Docker health, storage, GC, broker lag, and scale/drain previews.

Proof: Node fixture tests, runner health tests, action preview tests, Tuiwright capacity captures.

Acceptance: User can tell where jobs can and cannot run, and why.

### U19. Cache Observatory

Goal: Explain SmartCache speed, trust, fullness, misses, and GC.

Work: Build cache categories, hot objects, requests, hit/miss, singleflight, taints, leases, verdicts, promotions, material objects, epochs, toolchain fingerprints, and GC plan.

Proof: Cache metrics tests, taint tests, GC preview tests, Tuiwright cache-pressure captures.

Acceptance: User can identify full, stale, tainted, cold, or wasteful cache states.

### U20. VTI, Tests, And Flake Radar

Goal: Prove smart test skipping is safe.

Work: Build VTI dashboard with selected/skipped tests, confidence, selector misses, savings, escalation, full-run recommendations, flake radar, quarantine state, and repair actions.

Proof: Plan validation, selector misses, flake classification, Tuiwright VTI miss captures.

Acceptance: Low-confidence VTI never renders as safe green.

### U21. Agents, Autonomy, And LLM Governance

Goal: Make autonomous work observable and governable.

Work: Build agent sessions, tasks, steps, messages, artifacts, patch races, grants, branches, MRs, logs, Evidence Gate verdicts, kill bell, freeze windows, provider health, budget, token usage, and data-use policy.

Proof: Agent lifecycle, grant display, kill bell, budget redaction, Tuiwright race captures.

Acceptance: User can answer what each agent is doing, why, with what authority, and at what cost.

### U22. Bugs, Git Sync, And Review Queue

Goal: Connect bugs, Git state, MR/PR state, agent attempts, and review duties.

Work: Build bug board, ready bugs, bug detail, attempts, branch/MR links, Git sync dashboard, admissions, mirrors, stale approvals, review queue, and MR webhook ingestion.

Proof: Bug workflow tests, MR ingestion tests, Git sync tests, Tuiwright bug assignment preview.

Acceptance: User can trace bug -> attempt -> branch/MR -> pipeline -> evidence.

### U23. Release, Security, Secrets, And Artifacts

Goal: Make ship/rollback decisions safe and auditable.

Work: Build release page, rollback modal, release gates, canaries, production state, security findings, secret metadata, Vault health, artifacts, SBOM, signatures, provenance, and typed proof confirmations.

Proof: Typed confirmation, secret redaction, release rollback, artifact verification, Tuiwright production-proof captures.

Acceptance: Promote/rollback/secret recovery paths are proof-gated and audited.

### U24. Jankurai, Churn, Bottlenecks, Governance, And Source Doctor

Goal: Surface quality, drift, and governance risk.

Work: Build Jankurai score/findings/trends, code churn/risk, CI bottleneck lab, config drift, hook drift, action/MCP/schema/docs drift, and Source Doctor.

Proof: Drift detection, Jankurai fixture, bottleneck score, Tuiwright Source Doctor captures.

Acceptance: Runtime/profile/docs/action/schema mismatches are visible and block risky actions where appropriate.

### U25. Tuiwright Suite Expansion

Goal: Make Tuiwright coverage maintainable and broad.

Work: Split current Tuiwright test into capture, navigation, responsive, streams, actions, redaction, source doctor, accessibility, and performance suites.

Proof: The unit itself is the test lane.

Acceptance: Every page has deterministic captures at `80x24`, `100x30`, `120x36`, `160x48`, and `220x60`.

### U26. Performance, Resilience, Capture, And Final Gate

Goal: Prove the reset works at real scale and fails safely.

Work: Add performance fixtures, long-session leak tests, panic terminal-restore tests, stream reconnect tests, screenshot mode, redacted evidence bundle mode, and final release checklist.

Proof: 500 repos, 10k jobs, 1k events/sec, 100 trace subscriptions, 100k evidence/bug records, terminal restore, redaction.

Acceptance: Input p95 under 50 ms, normal render p95 under 16 ms, large-list render p95 under 33 ms, memory bounded.

## Screen Goals

| Screen | Goal |
|---|---|
| Mission | Global posture and next action. |
| Queue | Current capacity, bottleneck class, and runner-scaling truth. |
| Repos | Family/repo health and drilldown. |
| Workflow | Live planned/executing DAG across pipelines and gates. |
| Logs | Cursor-aware bounded trace viewing. |
| Runners | Pool/node/tag/resource health. |
| Cache | SmartCache performance, trust, taints, fullness, and GC. |
| VTI | Test selection safety, confidence, misses, and flake risk. |
| Agents | Agent work, grants, races, branches, evidence, and logs. |
| Autonomy | Kill bell, freeze windows, verdicts, policy, and launch ledger. |
| Bugs | Ready/blocked/racing/fixed bugs and attempts. |
| Git Sync | Local/remote/MR/mirror/admission state. |
| Bottlenecks | Historical and structural CI slowdowns. |
| Jankurai | Quality score, findings, caps, duplicates, generated-zone drift. |
| Security | Scans, grants, policy violations, admission denials. |
| Artifacts | Signatures, SBOM, provenance, verification, release linkage. |
| Release | Candidate, canary, production, gates, rollback, approvals. |
| Evidence | Searchable proof timeline and entity proof graph. |
| Settings | Runtime profile, source doctor, docs/action/MCP/schema drift. |
| LLMs | Provider health, key policy, spend, budget, usage, failures. |
| Churn | Change volume correlated with risk and failures. |
| Incident | High-contrast pinned emergency view with decision ledger. |
| Replay | Event-cursor replay for postmortem and demo. |

## Tuiwright Coverage Matrix

| Test suite | Required coverage |
|---|---|
| Capture | Every page at all target sizes with non-empty ink, layout regions, and page-specific labels. |
| Navigation | Route drilldown, `Esc` unwind, focus restoration, pane movement, command palette navigation. |
| Responsive | Tiny, compact, medium, wide, ultra-wide layouts. |
| Streams | Live events, disconnect, stale marker, reconnect, cursor resume. |
| Actions | Preview required, typed confirmation, idempotency, key-repeat protection, receipt navigation. |
| Redaction | No tokens/secrets in screenshots, text dumps, bundles, panic output, or copied paths. |
| Source Doctor | API down, MCP drift, schema mismatch, stale docs, DB profile mismatch. |
| Accessibility | ASCII fallback, no-color mode, high contrast, reduced motion, stable focus order. |
| Performance | Large fixtures, event bursts, huge tables, trace subscriptions, long session. |

## Proof Commands

| Scope | Command |
|---|---|
| TUI library | `cargo nextest run -p jeryu --lib tui::` |
| API contracts | `cargo nextest run -p jeryu --lib api::` |
| Engine/API | `cargo test -p jeryu --tests -- --test-threads=1` |
| Tuiwright | `TERM=xterm-256color cargo test --test tui_tuiwright -- --test-threads=1` until split |
| Fast gate | `just fast` |
| Audit | `just score` |
| Security/redaction | `just security` |
| SQLite/Kafka profile | `just runtime-sqlite-kafka` |
| RedlineDB/Jansu profile | `just runtime-redlinedb-jansu` |
| Full merge gate | `just check` |

## Final Acceptance Criteria

1. `jeryu tui` shows fleet posture, top blocker, source freshness, and next action within five seconds.
2. Every visible operational object is addressable, drillable, explainable, or explicitly non-interactive.
3. `Enter`, `Esc`, arrows, `Tab`, `:`, `/`, `a`, `e`, `l`, `x`, and `?` are consistent everywhere.
4. No renderer performs backend/system calls.
5. All mutating actions use preview, proof, confirmation, execute, stream, and receipt.
6. Stale/partial/degraded/down data is visible and blocks risky actions where appropriate.
7. Queue page explains whether adding runners helps.
8. Workflow supports family -> repo -> MR/PR -> pipeline -> job -> trace -> evidence.
9. Evidence can explain every green, warning, failure, and action.
10. Cache, VTI, agents, bugs, release, security, artifacts, Jankurai, LLMs, and settings are integrated into one entity/proof graph.
11. Tuiwright covers pages, layouts, interactions, degraded backends, safety, redaction, and performance.
12. No new source file violates the file-size budget without an explicit exception.
13. SQLite remains default and RedlineDB remains feature/config gated.
14. The TUI remains useful during backend failure through stale/degraded/empty/fixture states.
15. Redacted evidence bundles can be exported for selected entities/actions.
16. Action, MCP, CLI, DB schema, and docs drift are detectable in Source Doctor.

## Assumptions

| Assumption | Default |
|---|---|
| Product name | Use "JeRyu Flight Deck" in user-facing UI; keep "Hyperdeck" as internal reset shorthand. |
| Command compatibility | Preserve `jeryu tui`, `--demo`, `--capture`, `--tab`, `--width`, and `--height`. |
| Transport order | Prefer HTTP inspection API, then SSE/WebSocket where needed, then MCP resources, then local fallback. |
| Data strategy | Fixture-first for every page, live wiring second. |
| DB strategy | SQLite default; RedlineDB only behind `redlinedb-backend` and explicit URL/profile. |
| Test strategy | Unit/render tests prove internals; Tuiwright proves user-visible behavior. |
| Mutation strategy | TUI never bypasses action registry/capability safety even if backend has older direct paths. |

## Immediate Next Steps

1. Land U02 as a mechanical compatibility move with no product behavior changes.
2. Land U03-U05 as typed contract changes with serialization/action/freshness tests.
3. Add U06 inspection endpoints behind existing backend ownership boundaries.
4. Only after U06 compiles, wire U08 data clients and U09 reducer state.
5. Begin page work fixture-first, starting with Mission, Queue, Repos, Workflow, and Evidence.
