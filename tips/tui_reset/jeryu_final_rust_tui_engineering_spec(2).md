# JeRyu Flight Deck vNext — Final Rust CI TUI Engineering Specification

**Artifact:** final merged engineering specification  
**Product names used in this spec:** `jeryu tui`, **JeRyu Flight Deck**, **Workflow Atlas**, **Mission Control**  
**Primary goal:** build the fastest, richest, most trustworthy terminal command center for a many-repo CI/CD control plane, with live fleet activity, vivid semantic color, rapid drill-down/up, evidence-gated actions, and first-class support for repo families such as `veox-*`.

---

## 0. Source corpus studied and synthesis stance

This spec synthesizes the uploaded archive’s two kinds of source material:

1. **Inventory `.txt` files** — treated as the data-plane truth. These describe JeRyu’s current APIs, MCP tools, DB tables, CLI surfaces, GitLab integrations, cache, VTI, agents, bugs, release, Vault, Docker, and known backend gaps.
2. **Prior `.md` design attempts** — treated as competing UX and engineering proposals. Their best ideas are merged here into a single final build plan.

Files inspected from the archive:

```text
tip1.txt
tip2.txt
tip3.txt
tip4.txt
tip5.txt
tip6.txt
tip7.txt
tip8.txt
tip9.txt
jeryu_dream_rust_tui_engineering_spec.md
jeryu_dream_rust_tui_engineering_spec(1).md
jeryu_dream_rust_tui_engineering_spec(2).md
jeryu_dream_rust_tui_spec.md
jeryu_dream_rust_tui_spec(1).md
jeryu_dream_tui_engineering_spec.md
jeryu_dream_tui_engineering_spec(1).md
jeryu_dream_tui_engineering_spec(2).md
```

The final recommendation is to build a **stream-native, typed-read-model TUI** rather than a screen-by-screen pile of direct DB/GitLab calls. The UI must feel like **air traffic control + GitLab pipeline graph + htop/btop + Lazygit + Grafana + Linear issue triage + release war room**, compressed into one keyboard-driven Rust terminal application.

The spec assumes JeRyu is a Rust single-binary control plane around GitLab CI/CD, Git, GitLab Runner custom executors, runner pools, Docker/remote nodes, SmartCache, VTI smart test selection, release/canary/prod gates, Vault-backed secrets, local bug tracking, MCP/capability APIs, agent/autonomy workflows, Jankurai audits, evidence capsules, and a durable state DB.

---

## 1. North star

When `jeryu tui` opens, the operator should understand the whole system in under five seconds:

```text
What is happening across every repo?
What is blocked?
How close are we to the theoretical CI throughput limit?
Which repo family needs attention?
Which job, cache object, VTI decision, agent, bug, release gate, or security finding explains the slowdown?
What should I safely do next?
```

The answer must be visible, moving, colorful, current, and drillable.

The mental model is:

```text
Global Workflow Atlas
  -> Repo Family
    -> Repo
      -> Pipeline / Workflow DAG
        -> Job / Gate / Agent / Test / Cache / Evidence
          -> Trace / Artifact / Diff / Proof / Action
```

`Enter` always drills down. `Esc` always goes up. `Tab` changes panes. Arrow keys move spatially. `:` opens commands. `/` filters. `a` opens actions. `e` opens evidence. `l` opens logs/traces.

---

## 2. Product doctrine

### 2.1 Truth before beauty

The TUI is allowed to be vivid and animated, but it must never fake certainty. Every panel, row, graph node, metric, and log tail carries:

- source: webhook, GitLab REST, DB, Docker, cache gateway, Vault, MCP, capability API, local file, inferred, fixture;
- freshness: live, fresh, stale, last-known, disconnected;
- event cursor or timestamp;
- confidence when derived or predicted.

Status labels:

| Label | Meaning | UI treatment |
|---|---|---|
| `LIVE` | streaming or updated within TTL | bright status color, animated pulse |
| `FRESH 2s` | recently polled | normal intensity |
| `STALE 34s` | older than TTL but still useful | dim + amber badge |
| `LAST KNOWN` | backend/source is down | dim + source warning |
| `INFERRED` | derived from partial data | italic/dim or `≈` prefix |
| `UNKNOWN` | no trustworthy fact | explicit unknown, never blank |

No pane should become empty just because a transient refresh failed. Preserve the last meaningful snapshot, mark it stale, and show the failing source.

### 2.2 Every visible thing is addressable

Every row/card/node must be an entity with a stable `EntityRef`. The selected entity always supports some combination of:

```text
Enter  open detail/drilldown
Esc    up one scope
b      back in navigation stack
a      action menu
e      evidence/proof
l      logs/traces
/      filter within current scope
?      contextual help
```

This applies to repos, repo families, pipelines, jobs, runners, pools, nodes, cache objects, VTI test plans, selector misses, agents, agent tasks, bug attempts, release attempts, artifacts, security findings, Jankurai findings, evidence capsules, grants, Git refs, webhooks, and action results.

### 2.3 Plan-first, live-overlay workflow

The default screen is not “currently materialized GitLab jobs.” It is the **Workflow Atlas**: a graph of all planned validation and delivery gates, even before jobs exist. As GitLab jobs, VTI decisions, cache hits, artifact results, agent steps, and release gates arrive, the live state overlays the plan.

This solves a core operator problem: they need to see the execution shape, not only the jobs that have already started.

### 2.4 Proof before mutation

All mutating actions pass through the same action registry / capability policy path used by agents. The TUI must not call privileged GitLab or local operations directly from screen code.

Action flow:

```text
select entity
  -> open actions
    -> choose action
      -> preview / dry-run
        -> proof modal
          -> execute with idempotency key
            -> stream action events
              -> result + audit/evidence link
```

High-risk and production-risk actions require exact target proof: repo, ref, SHA, MR IID, version, gate result, grant, rollback plan, and audit destination.

### 2.5 Animation is for information, not noise

The user asked for “INCREDIBLE moving activity in realtime.” The final design should be lively, but not chaotic.

Use animation for:

- streaming event pulse on active jobs;
- animated DAG edges showing dependency flow;
- spinner/glyph changes on running jobs;
- progress bars and ETA deltas;
- queue pressure waves;
- cache hit/miss waterfall;
- agent step heartbeat;
- release/canary gate progression;
- log tail scroll markers;
- stale-source heartbeat fading out.

Never animate arbitrary decoration. Respect a setting:

```toml
[tui.animation]
mode = "rich"       # off | low | rich | scream
fps_cap = 30
pulse_live_edges = true
animate_progress = true
reduce_motion = false
```

---

## 3. Current data/control surfaces the TUI should exploit

### 3.1 Top-level surfaces

| Surface | Existing entrypoint / transport | Data and actions available |
|---|---|---|
| CLI | `jeryu <command>` | install/serve/remote/node/tui/git/save/sync/undo/system/status/pools/jobs/pipelines/cache/logs/agents/settings/tests/release/secrets/progress/repo/bug/policy/host/MCP/next/blocker/actions |
| Existing TUI | `jeryu tui` | Workflow, Mission, Release, Approvals, Jobs, Agents, Tests, Pools, Cache, Evidence, Secrets, LLMs, Git; currently DB/GitLab/Docker/cache/log polling oriented |
| MCP stdio | `jeryu mcp serve` | JSON-RPC MCP tools over stdin/stdout |
| MCP Streamable HTTP | `jeryu mcp serve-http`, default `127.0.0.1:9778` | POST `/mcp`, session headers, DELETE session, GET currently 405 |
| Webhook/API engine | Axum, default `127.0.0.1:9777` | `GET /health`, `POST /hooks`, `GET /cache/summary` |
| Capability Unix socket API | `jeryu capability serve <socket_path>` | versioned capability requests, actor, nonce, expiry, budget, grant, intent |
| GitLab REST wrapper | internal `GitlabClient` | projects, jobs, traces, artifacts, pipelines, bridges/downstream, variables, runners, runner managers, MRs, issues, branches, protected branches, webhooks |
| GitLab webhooks | `/hooks` | Job, Pipeline, Push; MR accepted/logged but not fully acted on yet |
| Message log / broker | Kafka or Jansu feature-gated | topics `jeryu.webhook.jobs`, `jeryu.webhook.pipelines`, `jeryu.webhook.pushes` |
| Custom executor | `jeryu exec config/prepare/run/cleanup` | runner lifecycle, sandbox copy, honeypots, cache env, logs, failure/quarantine capsules |
| Git server hook | `jeryu server-hook pre-receive` | pre-receive ref update admission, grants, policy verdicts, denials |
| SmartCache / gateway | proxy `19800`, registry mirror `19801` | Cargo sparse config/download proxy, CAS, singleflight, cache metrics, taints/verdicts/promotions |
| Docker/runner plane | Bollard + compose/remotes | managed runner containers, lifecycle, logs, Docker events, OOM/die, local/remote managers |
| Vault/secrets | Vault HTTP + DB | Vault health/init/unseal, KV v2, policies, rotation/finalization, release secret sets, audit metadata |
| State DB | SQLite default, RedlineDB opt-in | durable truth for pools, managers, jobs, pipelines, releases, evidence, retry, cache, grants, tests, VTI, secrets, bugs, LLM budgets, autonomy |
| Autonomy / Evidence Gate | autonomy binary/server/ledgers | verdicts, launch ledger, kill bell, freeze windows, evidence gate, LLM provider health |
| GitHost abstraction | GitLab/GitHub adapters | PR/MR state, diffs, comments, approvals, checks/workflow runs, merge passports |
| Jankurai | audit tool/action | code audit score, caps, duplicate code, security/provenance/UX/release findings |

### 3.2 Current MCP tools

The current MCP source defines these tools under `jeryu.*`:

| MCP tool | Arguments | TUI use |
|---|---|---|
| `jeryu.fetch_capsule` | `job_id` | show latest structured failure capsule for a job |
| `jeryu.get_system_snapshot` | none | seed global status: GitLab readiness, pool count, recent job events, latest release |
| `jeryu.get_pipeline_jobs` | `project_id`, `pipeline_id` | pipeline/job detail; downstream-expanded job list |
| `jeryu.get_ci_bottlenecks` | `project_id`, optional `ref_name`, optional `limit` | bottleneck lab, historical duration table |
| `jeryu.explain_blockers` | `entity_type`, `entity_id` | unified blocker explanation for jobs/releases/merge |
| `jeryu.plan_validation` | `project_id`, `ref_name`, `test_ids[]` | VTI plan validation, selector miss defense |
| `jeryu.run_tests` | `project_id`, `target_ref`, `test_scope` | targeted test pipeline action |
| `jeryu.propose_patch` | `project_id`, `branch_name`, `base_ref`, `commit_message`, `modifications[]`, optional `mr_title` | agent/human patch proposal |
| `jeryu.race_patches` | `project_id`, `base_branch`, `commit_message`, `hypotheses[]` | multi-hypothesis agent race |
| `jeryu.request_merge` | `project_id`, `mr_iid`, `source_branch`, `target_branch` | high-risk merge action; UI must force proof gate |
| `jeryu.bug_submit` | canonical bug report, optional `idempotency_key` | create local bug from selected failure/finding |
| `jeryu.bug_list` | optional `project`, `status`, `sort` | bug board |
| `jeryu.bug_show` | `bug_id` | bug detail, attempts, events, evidence |
| `jeryu.bug_ready` | optional `project` | agent-ready bug queue |
| `jeryu.bug_update` | `bug_id`, optional status/severity/priority/component/owner | triage/update bug |
| `jeryu.bug_record_attempt` | `bug_id`, attempt | append agent attempt history |

MCP protocol facts to preserve in design:

- JSON-RPC methods: `initialize`, `ping`, `tools/list`, `tools/call`.
- Accepts `notifications/initialized`.
- Stdio supports batched requests.
- HTTP streamable endpoint currently exposes `/mcp`; GET returns 405; DELETE terminates sessions.
- HTTP calls enforce protocol/session/method/name headers and reject non-loopback origins.

### 3.3 Current HTTP/webhook routes

| Route | Auth | Current behavior | vNext TUI implication |
|---|---|---|---|
| `GET /health` | none | returns `ok` | insufficient alone; add deep health |
| `POST /hooks` | `X-Gitlab-Token` equals webhook secret | consumes GitLab Job/Pipeline/Push, routes through broker | primary event source |
| `GET /cache/summary` | `X-Jeryu-Token` equals webhook secret | returns bytes/hits/objects/status | expand into rich cache endpoints |

GitLab webhook payload effects:

| Event | Consumed fields | Current effect |
|---|---|---|
| Job Hook | `build_id`, `project_id`, `pipeline_id`, `build_status`, `build_name`, `build_queued_duration`, `ref`, `runner.id`, `runner.description` | upserts job events; failed jobs may trigger recovery/retry; pending/created may trigger scale check |
| Pipeline Hook | project id, pipeline id/status/SHA/ref | upserts tracked pipelines; main success/failure can drive release/prod/canary state |
| Push Hook | project id, before/after, ref, path namespace | shadows repo, cancels superseded pipelines, computes impact, persists VTI test plan |
| Merge Request Hook | header/body accepted | currently logged only; must be promoted to first-class state |

### 3.4 GitLab REST data

GitLab client data the TUI should use:

| Type | Fields / capabilities |
|---|---|
| Project | id, name, path namespace, URL |
| Job | id, name, status, stage, allow_failure, pipeline id/ref, URL, queued duration, runtime, start/finish, runner description |
| Pipeline | id, SHA, ref, status, URL, source |
| Pipeline bridge | bridge id/name/status, downstream pipeline |
| Variables | key/value; redact sensitive values in UI |
| Runner | id, description, paused |
| Runner manager | system_id, status, contacted_at |
| Issue | id/iid, title, state, labels, URL |
| Merge request | id/iid, title, state, URL, source/target branch |
| Branch/protection | create/delete/protect MR-only |
| Webhook | create group webhook for job/pipeline/push/MR |
| Jobs actions | list by scope, fetch trace, fetch artifact, play/cancel/retry/requeue, log snippets |
| Pipelines actions | trigger, list by ref, list jobs/bridges/downstream, cancel, get variables |
| Project file actions | get/create/update files, batch commit actions, return commit SHA |

### 3.5 Durable state DB inventory

The active store is backend-neutral through SQLx Any, with SQLite default (`jeryu.sqlite`) and RedlineDB opt-in (`jeryu.redlineDB`) or `JERYU_DATABASE_URL` override. SQLite should use WAL, `synchronous=NORMAL`, memory temp store, foreign keys, busy timeout, cache size, mmap, and autocheckpoint pragmas.

Important inspectable table groups:

| Domain | Tables / records |
|---|---|
| Runner pools | `pools`, `managers` |
| CI events | `job_events`, `ci_job_runs`, `tracked_pipelines` |
| General event log | `events` |
| Capability/admission | `capability_intents`, `capability_grants`, `admission_decisions` |
| Repos/Git event plane | `tracked_repositories`, `git_command_events`, `git_ref_updates`, `git_mirror_jobs`, `git_risk_approvals`, `git_command_artifacts` |
| Release | `release_attempts`, `foundry_candidates` |
| Evidence/retry | `evidence_capsules`, `retry_decisions` |
| Secrets | `secret_authorities`, `release_secret_sets`, `secret_audit_events` |
| Cache base | `cache_objects`, `cache_requests`, `hot_cache_entries`, `build_signatures`, `image_signatures`, `force_refresh_rules` |
| Cache trust/taint | `resolved_refs`, `cache_taints`, `cache_leases`, `cache_verdicts`, `cache_promotions`, `material_objects`, `material_aliases`, `action_cache`, `cache_epochs`, `toolchain_fingerprints` |
| Test intelligence | `test_executions`, `test_plans`, `test_plan_items`, `selector_misses` |
| Autonomy/safety | `launch_ledger`, `kill_bell_state`, `verdicts` |
| LLM budgets | `llm_budget_ledger` |
| Bug tracker | `bug_projects`, `bug_project_edges`, `bugs`, `bug_events`, `bug_attempts`, `bug_links`, `bug_external_refs`, `bug_evidence` |

### 3.6 Current TUI/read-model facts

The existing TUI already has useful concepts:

- tabs: Workflow, Mission, Release, Approvals, Jobs, Agents, Tests, Pools, Cache, Evidence, Secrets, LLMs, Git;
- background workers: flow collector, general snapshot sync, selected-job log polling;
- live log polling roughly every 650 ms for selected jobs;
- anti-blanking: retain last meaningful flow snapshot and mark stale;
- typed API/read-model concepts: entity kinds, event kinds, read model snapshots, component health, action preview/result, VTI status, cache verdicts, graph edge kinds, test plan views;
- action registry previews;
- evidence/audit panes;
- cache, tests, pools, release, git, agents screens.

Known limitations to solve:

- no stream-native transport yet;
- logs are polling-oriented;
- Flow Board renders only first active pipeline in some designs;
- graph edges / `needs` are undercomputed;
- ETA is heuristic;
- Evidence is not yet a fully searchable proof timeline;
- Agents lacks a dedicated agent-run lifecycle table;
- MR hooks are logged but not first-class;
- `/cache/summary` is too small;
- broker observability is not surfaced;
- docs/action manifests drift from source.

---

## 4. Final information architecture

### 4.1 Primary tabs

The final TUI uses one global shell with these top-level tabs. Numeric keys are optimized for constant use; named keys cover specialized domains.

| Key | Tab | Purpose |
|---:|---|---|
| `0` | **Workflow Atlas** | default: global plan/live workflow graph across repos |
| `1` | **Mission** | executive fleet dashboard, attention queue, next action |
| `2` | **Repos** | repo families, single repo drilldown, saved lenses |
| `3` | **Pipelines** | active pipelines, DAGs, job traces, artifacts |
| `4` | **Runners** | pools, managers, nodes, capacity, utilization |
| `5` | **Cache** | storage, cache categories, hits/misses, taints, GC plan |
| `6` | **VTI** | smart test skipper, impact plans, selector misses, confidence |
| `7` | **Agents** | agents, tasks, races, logs, grants, configs |
| `8` | **Bugs** | cross-repo issue/bug board, ready queue, attempts |
| `9` | **Release** | release train, canary, prod, rollback, version state |
| `g` | **Git Sync** | local/remote/mirror/MR/branch/admission state |
| `j` | **Jankurai** | audit scores, blockers, duplicate clusters, adoption caps |
| `s` | **Security** | SAST/dependency/container/IaC/secrets/policy |
| `a` from tab bar | **Artifacts** | signed artifacts, SBOM, provenance, images |
| `e` from tab bar | **Evidence** | universal proof timeline and flight recorder |
| `c` from tab bar | **Config** | settings, runtime profile, workflow/agent config |
| `d` from tab bar | **Source Doctor** | backend health, data freshness, schema/action drift |
| `l` from tab bar | **LLMs/Autonomy** | provider health, spend, kill bell, evidence gate |

Contextual `a`, `e`, `l` still mean action/evidence/log when focus is inside a pane; top-level tab jumps should be available through command palette and optional leader bindings to avoid ambiguity.

### 4.2 Universal layout shell

Every screen uses the same shell:

```text
╭────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ JeRyu Flight Deck  LIVE cursor#184293 Δ42ms  GitLab✓24ms DB✓2ms Docker⚠stale Cache✓87% Vault✓     │
├────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ 0 Workflow 1 Mission 2 Repos 3 Pipelines 4 Runners 5 Cache 6 VTI 7 Agents 8 Bugs 9 Release ...    │
├───────────────┬───────────────────────────────────────────────────────────┬────────────────────────╮
│ left nav      │ main workspace                                            │ right inspector         │
│ / filters     │ tables, DAGs, timelines, charts, traces                   │ selected entity detail  │
│ repo tree     │                                                           │ actions/evidence        │
├───────────────┴───────────────────────────────────────────────────────────┴────────────────────────┤
│ breadcrumbs: Global › veox-* › veox-api › pipeline#8172 │ keys: Enter drill Esc up Tab pane : cmd │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Pane rules:

- **Header**: global freshness, event cursor, component health, queue pressure, top incident badge.
- **Tab bar**: top-level movement; badges for errors/stale counts.
- **Left nav**: scope selector for the screen: repo family tree, filters, lanes, pools, cache categories, evidence kinds.
- **Main workspace**: the primary graph/table/timeline/trace.
- **Right inspector**: always describes selected entity and offers the top actions/evidence/logs.
- **Bottom bar**: breadcrumbs, key hints, active filter, last action result, recording/demo indicator.

### 4.3 Responsive breakpoints

| Width | Layout |
|---:|---|
| `< 100 cols` | compact mode: one main pane + collapsible inspector; no side-by-side DAG/table; abbreviated labels |
| `100-139 cols` | two columns: main + inspector; left nav overlays or toggles |
| `140-199 cols` | full shell: left nav + main + inspector |
| `>= 200 cols` | war-room mode: multi-panel grid with persistent attention queue, minimap, and event ribbon |

The TUI must never require a huge terminal. Large terminals should feel spectacular; small terminals should stay useful.

---

## 5. Keyboard and interaction model

### 5.1 Universal keys

| Key | Action |
|---|---|
| `0`-`9` | switch primary numeric tabs |
| `Tab` / `Shift-Tab` | next/previous focusable pane |
| `↑↓←→` | move within current pane; spatial graph movement in DAGs |
| `h/j/k/l` | optional vim movement alias |
| `Enter` | drill/open selected entity |
| `Esc` | close modal or go up one scope |
| `b` | back in navigation stack |
| `[` / `]` | previous/next sibling entity |
| `/` | filter current pane |
| `Ctrl-/` | global search across entities/events/logs |
| `:` | command palette |
| `a` | action menu for selected entity |
| `p` | preview default next action |
| `e` | evidence/proof for selected entity |
| `l` | logs/traces for selected entity |
| `g` | Git/GitLab/GitHost context or Git Sync tab via command palette |
| `y` | copy selected entity summary as JSON/YAML/text |
| `r` | refresh current pane/source; retry selected failed job only from action menu or context where explicit |
| `Space` | select/mark row or focus subtree in graph |
| `m` | multi-select or minimap toggle depending pane |
| `z` | zoom selected graph/entity |
| `f` | follow selected live job/agent/gate |
| `B` | explain blocker/bottleneck for selected entity |
| `?` | contextual help overlay |
| `.` | repeat last safe read/low-risk action |
| `F5` | refresh all sources |
| `Ctrl-k` | kill-bell / pause autonomy overlay |
| `Ctrl-s` | save current lens/view |
| `q` | quit or close overlay |
| `Ctrl-c` | safe quit prompt if action is running |

### 5.2 Drilldown contract

Every entity implements the same route contract:

```rust
pub enum Route {
    WorkflowAtlas,
    Mission,
    RepoFamily { family_id: String },
    Repo { repo_id: String },
    Pipeline { project_id: i64, pipeline_id: i64 },
    JobTrace { project_id: i64, job_id: i64 },
    Cache { scope: Scope },
    CacheObject { key: String },
    Vti { repo_id: Option<String>, plan_id: Option<String> },
    Agent { agent_id: Option<String>, task_id: Option<String> },
    Bugs { bug_id: Option<String> },
    GitSync,
    Bottlenecks,
    Jankurai { repo_id: Option<String> },
    Runners { pool_id: Option<String>, node_id: Option<String> },
    Security { finding_id: Option<String> },
    Artifacts { artifact_id: Option<String> },
    Release { release_id: Option<String> },
    Secrets,
    Evidence { entity: Option<EntityRef>, cursor: Option<u64> },
    Config,
    SourceDoctor,
    LlmAutonomy,
}
```

`Esc` pops from child to parent. `b` moves through history. `Enter` pushes a new route. The breadcrumb bar must always show the route stack.

### 5.3 Spatial graph navigation

In Workflow/Pipeline DAG panes:

- `Left` selects nearest upstream/previous-column node.
- `Right` selects nearest downstream/next-column node.
- `Up`/`Down` selects nearest node in same lane/column by y-coordinate.
- If there is no exact neighbor, select by minimum weighted distance:

```text
distance = dx_weight * horizontal_delta + dy_weight * vertical_delta + edge_bonus + status_priority_bonus
```

- Failed/blocked nodes get a small priority bonus so navigation tends toward actionable nodes.
- `Space` pins/focuses a subtree.
- `z` zooms selected node/subgraph.
- `m` toggles minimap.
- `c` centers selected node.
- `f` follows selected live entity.

---

## 6. Visual language

### 6.1 Semantic palette

Use semantic colors, not arbitrary decoration. Support truecolor, 256-color fallback, and monochrome/accessibility mode.

| Semantic | Suggested truecolor | Fallback | Meaning |
|---|---:|---:|---|
| `success` | `#3ddc84` | green | passed, healthy, eligible |
| `running` | `#36c5f0` | cyan | active work, live stream |
| `queued` | `#a78bfa` | magenta/violet | waiting/runnable |
| `warning` | `#f5c542` | yellow | degraded, stale, nearly full |
| `failure` | `#ff5f57` | red | failed/blocking/error |
| `blocked` | `#ff9f43` | yellow/red | dependency, gate, manual block |
| `cache` | `#00d1b2` | cyan | cache hit/trust/storage |
| `vti` | `#7dd3fc` | blue/cyan | test intelligence, skipped by proof |
| `agent` | `#ff77e9` | magenta | autonomous work |
| `release` | `#f472b6` | magenta | canary/prod/release gates |
| `security` | `#fb7185` | red/magenta | security findings, secrets |
| `evidence` | `#c4b5fd` | violet | proof, audit, capsules |
| `stale` | `#7c7c7c` | dark gray | stale/last-known |
| `unknown` | `#9ca3af` | gray | unknown or disconnected |

### 6.2 Glyphs

| Glyph | Meaning |
|---|---|
| `●` | healthy/live |
| `▶` | running |
| `◌` | queued/waiting |
| `⏸` | paused/manual |
| `✓` | success |
| `✕` | failure |
| `!` | warning |
| `⛔` | denied/blocked if terminal supports; fallback `X` |
| `↷` | skipped/reused/VTI/cache |
| `◇` | evidence/proof |
| `◆` | selected evidence/proof |
| `⚑` | release gate |
| `⚙` | agent/task/config |
| `⌁` | stream/log activity |
| `⟳` | retry/requeue |
| `⌁` | cache/proxy flow |
| `~` | stale |
| `?` | unknown |

Always pair critical color with glyph/text, never color alone.

### 6.3 Progress bars and movement

Use status-specific bars:

```text
running:   ███████░░░ 73%  ETA 02:14   pulses right edge
queued:    ░░░░░░░░░░ q 04:21          shimmer/pulse slowly
blocked:   ▓▓░░░░░░░░ dep:build        amber lock/gate marker
failed:    ██████████ fail 11:42        red final, no motion
skipped:   ↷ VTI skip conf 0.94         dim cyan/teal
cached:    ↷ cache hit 87%              teal with digest/trust badge
stale:     ██████░░░░ stale 37s         dim + ~ badge
```

### 6.4 Status badges

Badges should be concise and composable:

```text
LIVE  STALE  VTI✓  VTI?  CACHE✓  CACHE!  TAINT  SIG✓  SBOM✓  SEC!  AGENT  GRANT  PROD  CANARY  MR  BUG  PROOF
```

---

## 7. Default screen: Workflow Atlas

### 7.1 Purpose

Workflow Atlas is the flagship default. It shows the whole validation/delivery journey as a graph, across repo families and repos, with planned gates plus live jobs. It must answer:

- What work exists now?
- What work is runnable next?
- What is blocking the critical path?
- Which repo family is under pressure?
- Is CI near its theoretical limit?
- Are cache/VTI/agents/release gates helping or hurting?

### 7.2 Workflow layers

A full JeRyu workflow may span:

1. local repo state and branch readiness;
2. VTI impact planning;
3. MR/PR fast validation;
4. full certification / main-candidate;
5. Jankurai audit;
6. security scans;
7. build/artifact/signature;
8. merge gate / merge witness;
9. release candidate / Foundry Train;
10. canary deploy / Nightwatch;
11. production promotion;
12. rollback readiness;
13. post-release evidence/audit.

### 7.3 Node card design

Running card:

```text
╭cargo-test-linux──────────────╮
│ ▶ RUN 63%  04:12/06:45       │
│ repo veox-api  pool rust-hi  │
│ q 00:03  cache 78%  vti sel  │
│ last: test auth::jwt_refresh │
╰──────────────────────────────╯
```

Failed card:

```text
╭jankurai-audit────────────────╮
│ ✕ FAIL  cap: duplicate-code  │
│ score 82↓  ver 0.14.2        │
│ 3 blockers  11 warnings      │
│ Enter: issues  r: rerun      │
╰──────────────────────────────╯
```

Skipped/cached card:

```text
╭ui-smoke-tests────────────────╮
│ ↷ SKIP by VTI  conf 0.94     │
│ no impacted paths            │
│ last pass main@9fd2  2h ago  │
│ Enter: proof                 │
╰──────────────────────────────╯
```

### 7.4 Global Workflow Atlas mock

```text
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ JeRyu Workflow Atlas  LIVE cursor#184293  frontier=86%  runnable=41/48 slots  queue=19 jobs  blockers=7  prod=canary-green │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ 0 Workflow  1 Mission  2 Repos  3 Pipelines  4 Runners  5 Cache  6 VTI  7 Agents  8 Bugs  9 Release  j Jankurai  s Security │
├───────────────┬──────────────────────────────────────────────────────────────────────────────────────┬───────────────────────┤
│ FAMILIES      │ LIVE WORKFLOW GRAPH                                                                  │ INSPECTOR             │
│ ▾ veox-*  18  │ veox-api main@9fd2                                                                   │ selected: test-linux  │
│   ✓ core   4  │                                                                                        │ status: RUN 63% LIVE  │
│   ▶ api    9  │  plan ─▶ build ─┬─▶ test-linux ▶▶▶────┬─▶ jankurai ─▶ security ─▶ merge-gate          │ pool: rust-hi         │
│   ✕ web    3  │                 │                     │                                                 │ q: 00:03 run: 04:12   │
│   ⚠ deploy 2  │                 ├─▶ test-macos ◌queued┤                                                 │ ETA: 02:33            │
│ ▸ isolated 11 │                 │                     └─▶ artifact/signature ◌blocked                  │ cache: 78% hit        │
│ ▸ infra     6 │                 └─▶ docs ↷VTI skip conf .98                                             │ VTI: selected         │
│               │                                                                                        │ Evidence: 4 refs      │
│ SAVED LENSES  │ veox-web MR!123                                                                       │ Actions:              │
│ * critical    │  plan ─▶ build ✓ ─▶ test ✕fail ─▶ capsule ◆ ─▶ bug #VE-882 ─▶ agent fix ▶running       │ [l] logs [e] proof    │
│ * agents      │                                                                                        │ [a] actions [B] why   │
├───────────────┴──────────────────────────────────────────────────────────┬───────────────────────────┴───────────────────────┤
│ EVENT RIBBON                                                              │ TOP ATTENTION                                             │
│ 12:01:02 job started veox-api/test-linux  12:01:04 cache miss storm ...   │ 1 ✕ veox-web test failed, retry blocked by selector miss  │
│                                                                           │ 2 ⚠ cache 91% full, crates +34GiB                         │
╰───────────────────────────────────────────────────────────────────────────┴───────────────────────────────────────────────────╯
```

### 7.5 Workflow graph semantics

Node state combines several dimensions:

| Dimension | Examples |
|---|---|
| Execution | planned, created, pending, queued, running, passed, failed, canceled, manual, skipped |
| Readiness | dependency-blocked, tag-blocked, trust-blocked, manual approval, resource pressure |
| Source | planned, webhook, GitLab REST, DB, inferred, artifact parser |
| Risk | normal, flaky, critical-path, release-gate, production, security |
| Reuse | VTI skipped, cache reused, artifact reused, stale reuse, tainted reuse |
| Evidence | capsule, test receipt, selector miss proof, signature, SBOM, gate JSON, audit record |

Edges show dependency type:

| Edge | Meaning |
|---|---|
| solid | hard dependency / `needs` |
| dashed | stage order or inferred dependency |
| dotted | evidence/proof relationship |
| double | release/production gate |
| dim | skipped/reused path |
| animated | live active flow |

---

## 8. Mission Control screen

Mission is the executive cockpit. It trades graph detail for immediate operational understanding.

### 8.1 Mission mock

```text
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ JERYU MISSION CONTROL  LIVE Δ42ms  GitLab✓ DB✓ Docker⚠stale Cache✓87% Vault✓ Broker⚠lag327                  │
├───────────────────────┬──────────────────────────────────────────────┬───────────────────────────────────────┤
│ REPO FAMILIES          │ LIVE FLEET QUEUE                             │ ATTENTION / NEXT ACTION                │
│ ▾ veox-*          18   │ Capacity: 142 usable slots                    │ TOP BLOCKER                            │
│   ✓ veox-core      4   │ Busy: 128  Idle: 7  Bad: 7                    │ ✕ veox-web pipeline #8172               │
│   ▶ veox-api       9   │ Theoretical: ███████████████░░ 90%            │ test-linux failed after 11m42s          │
│   ✕ veox-web       3   │ Queue pressure: 1.37x next 10m ⚠              │ Evidence: capsule #fc-882              │
│   ⚠ veox-deploy    2   │ Critical queue: rust-highmem 23/28 max         │ Suggested: inspect cache, retry tests   │
│ ▸ isolated        11   │ Scheduler efficiency: 84%                     │ [p] preview  [Enter] drill              │
│ ▸ infra            6   │ Wasted slot-seconds: 11.8k                    │                                       │
│                       │ Jobs: 77 run 41 queue 6 fail 3 block          │ OTHER ATTENTION                         │
│ SAVED LENSES          │ Agents: 9 active 2 blocked 4 review            │ ⚠ Broker lag 327 events                 │
│ * Critical only        │ Bugs: 31 ready 8 in progress 5 fix proposed    │ ⚠ VTI low confidence: payment paths     │
│ * Agent work           │ Releases: 1 canary, 1 prod approval waiting     │ ✕ 2 unsigned nightly artifacts          │
│ * Release train        │ Cache: 87% full 79% hit top growth crates       │ ⚠ Jankurai score down 4.2 veox-api      │
├───────────────────────┴──────────────────────┬───────────────────────┴───────────────────────────────────────┤
│ HOT REPOS                                      │ RELEASE / SECURITY / QUALITY SNAPSHOT                         │
│ repo          CI queue ETA   VTI   cache agent │ veox-deploy v1.8.4 canary 72% telemetry green approval pending  │
│ veox-web      ✕  3     --    ?     61%   2     │ security: 2 high 7 medium 0 known secret leaks                  │
│ veox-api      ▶  9     06m   .91   78%   3     │ jankurai fleet avg 88.1, 4 cap-limited repos                    │
│ veox-core     ✓  0     --    .98   89%   0     │ artifacts: 128 signed, 2 missing provenance                     │
╰────────────────────────────────────────────────┴───────────────────────────────────────────────────────────────╯
```

### 8.2 Mission ranking model

Attention items are ranked by:

```text
score = severity_weight
      + critical_path_weight
      + production_risk_weight
      + repo_priority_weight
      + age_weight
      + fanout_weight
      + confidence_weight
      - already_assigned_discount
```

Mission must include a one-line “why this is #1” explanation. Every attention item links to evidence and actions.

---

## 9. Repo family and repo drilldown

### 9.1 Repo families

Repo families are first-class because operators think in systems, not isolated repos. Families can be configured explicitly or inferred by naming patterns:

```toml
[[repo_families]]
id = "veox"
label = "veox-*"
patterns = ["veox-*", "deploy/veox-*", "enclave/veox-*"]
critical_repos = ["veox-core", "veox-deploy"]
release_repos = ["veox-deploy"]
owners = ["platform", "release"]
```

Family-level metrics:

- repos total/healthy/degraded/failing;
- live queue by repo and pool;
- critical path across repos;
- shared runner/cache pressure;
- VTI confidence by family;
- agent work by repo;
- bug readiness by component;
- release train state;
- security/Jankurai/artifact posture;
- Git sync/admission drift;
- top cross-repo blockers.

### 9.2 Family screen mock

```text
╭veox-* family────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ health ⚠ degraded  repos 18  running 31  queued 19  failing 3  agents 7  bugs ready 14  release canary-green   │
├───────────────┬──────────────────────────────────────────────────────────┬─────────────────────────────────────┤
│ REPOS          │ FAMILY CRITICAL PATH                                     │ FAMILY INSPECTOR                    │
│ ✓ veox-core    │ veox-api build ▶ -> tests ▶ -> jankurai ◌ -> security ◌  │ shared bottleneck: rust-hi runners  │
│ ▶ veox-api     │ veox-deploy waits on veox-api artifact signature         │ cache growth: crates +34GiB         │
│ ✕ veox-web     │ veox-web blocked by test failure + selector miss         │ VTI: 0.91 avg, 2 low-confidence     │
│ ⚠ veox-deploy  │                                                          │ release: v1.8.4 canary 72%          │
│ ✓ veox-enclave │                                                          │ actions: scale pool, open queue lab │
├───────────────┴──────────────────────────────────────────────────────────┴─────────────────────────────────────┤
│ repo          ci   queue eta   vti  cache  agents bugs  release   security jankurai  last event                 │
│ veox-api      ▶    9     06m   .91  78%    3      4     n/a       ✓        87.2      job test-linux progress     │
│ veox-web      ✕    3     --    .62  61%    2      8     n/a       !2       81.0      failed test auth_refresh     │
│ veox-deploy   ⚠    2     04m   .96  88%    1      1     canary    ✓        90.4      prod approval waiting        │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 9.3 Repo screen

Repo drilldown must combine the currently scattered worlds:

- active branch/MR/pipeline;
- workflow DAG;
- live jobs/traces;
- recent commits/churn;
- VTI plan and test receipts;
- cache keys/hits/misses/taints;
- agents and bug attempts;
- Jankurai/security/artifact state;
- release participation;
- Git sync/admission/protection state;
- evidence timeline.

Repo subtabs:

```text
Overview | Workflow | Pipelines | Tests/VTI | Cache | Agents | Bugs | Git | Quality | Security | Release | Evidence | Config
```

Repo header:

```text
veox-api  main@9fd2c4  MR!118  pipeline#8172 ▶ 63%  queue 9  VTI .91  cache 78%  sec✓  jankurai 87.2  LIVE
```

---

## 10. Pipeline graph and live trace viewer

### 10.1 Pipeline graph requirements

Pipeline view must show:

- parent and child pipelines;
- stage order;
- `needs` edges where available;
- artifact dependencies;
- manual jobs/gates;
- VTI skipped nodes;
- cached/reused successes;
- jobs blocked by failed dependencies;
- jobs blocked by runner tags/trust tier;
- queue duration vs runtime;
- runner/pool/node attribution;
- critical path and ETA;
- parsed artifact reports;
- downstream release/canary impact;
- cross-links to bugs/agents/evidence.

### 10.2 Job inspector

Selecting a job shows:

- job id, name, stage, status, allow-failure;
- project/ref/SHA/pipeline/root pipeline;
- runner description, pool, manager, node, trust tier;
- queued duration, runtime, ETA, historical p50/p95/max;
- live stdout/stderr trace, pause/follow/search;
- failure capsule, retry decision, classification;
- artifacts and parsed reports: JUnit, coverage, code quality, SAST, dependency scan, container scan, benchmark JSON, `nextest` archive;
- cache verdicts and hits/misses by key;
- VTI relation: selected, skipped, missed, receipt;
- actions: open URL, retry, cancel, play manual, fetch artifacts, create bug, assign agent, explain blockers.

### 10.3 Trace viewer

Trace viewer mock:

```text
╭job trace: veox-api/test-linux #119332 LIVE offset=884281 follow=on────────────────────────────────────────────╮
│ status ▶ RUN 63%  runtime 04:12  ETA 02:33  runner rust-hi/node-b  cache hit 78%  last chunk 120ms            │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ 04:09.120 cargo nextest run --profile ci                                                                      │
│ 04:10.882 PASS auth::jwt_refresh                                                                              │
│ 04:11.203 PASS api::rate_limit                                                                                │
│ 04:12.118 RUN  billing::invoice_reconcile                                                                     │
│                                                                                                               │
│ markers:  ✕ errors 0  ! warnings 7  cache misses 18  slow tests 3  selected test billing::invoice_reconcile  │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ keys: / search  f follow  p pause  e evidence  a actions  y copy  Esc back                                     │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Preferred transport:

```text
GET /inspect/jobs/{project_id}/{job_id}/trace/stream
GET /events/stream?kinds=job.log.chunk&entity=job:{job_id}
```

Fallback transport:

- Poll GitLab trace every ~650 ms only for selected/followed jobs.
- Deduplicate by byte offset.
- Store bounded ring buffer per visible job.
- Drop oldest hidden trace chunks first under memory pressure.

---

## 11. Queue and theoretical-limit model

### 11.1 Operator question

The UI must answer: **“How close am I running to the theoretical limit, and what prevents 100%?”**

### 11.2 Capacity definitions

```text
raw_slots = sum(configured runner concurrency)
healthy_slots = raw_slots - down/paused/unhealthy slots
trust_eligible_slots = healthy slots that satisfy job trust tier
resource_adjusted_slots = healthy_slots * node_pressure_weight
usable_slots_for_queue = slots matching queued job tags, trust, project/pool affinity
runnable_jobs = queued jobs whose dependencies are satisfied and whose tags/trust match at least one usable slot
running_runnable_jobs = running jobs that were runnable under current constraints
frontier_utilization = running_runnable_jobs / usable_slots_for_queue
queue_pressure = queued_runnable_seconds / (usable_slots_for_queue * horizon_seconds)
scheduler_efficiency = useful_running_slot_seconds / available_slot_seconds
lost_capacity = usable_slots_for_queue - running_runnable_jobs
critical_path_utilization = critical_path_running_time / wall_clock_elapsed
```

### 11.3 Loss decomposition

Every “why not 100%?” answer is decomposed into buckets:

| Loss bucket | Example | Action |
|---|---|---|
| dependency wait | test jobs waiting for build | none or split build |
| tag mismatch | idle runners cannot run `rust-hi` jobs | scale right pool / retag |
| trust mismatch | untrusted pool cannot run release/signing | add trusted capacity |
| resource pressure | node CPU/memory/disk throttling | drain/GC/migrate |
| queue policy | concurrency cap, protected ref limit | tune policy |
| GitLab delay | API/pipeline creation lag | inspect GitLab health |
| cache misses | compile storm, no reuse | inspect cache, prewarm |
| artifact wait | downstream needs artifact/signature | inspect artifact path |
| manual approval | release or merge gate waiting | review proof |
| flaky retries | jobs re-running and consuming slots | flake lab |
| broker lag | webhooks delayed, state stale | inspect broker |

### 11.4 Queue Lab mock

```text
╭Queue Lab──────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ usable slots 142  busy 128  lost 14  frontier 90%  queue pressure 1.37x  p95 wait 04:12  critical ETA 13:40 │
├──────────────┬───────────────────────────────────────────────┬──────────────────────────────────────────────┤
│ POOLS         │ LOSS DECOMPOSITION                            │ WHAT-IF / RECOMMENDATIONS                    │
│ rust-hi 28    │ tag mismatch        ███████  41%  5.7k sec     │ +4 rust-hi managers -> p95 wait -41%         │
│ rust-lo 48    │ dependency wait     █████    29%  4.1k sec     │ prewarm crates.io top 128 -> build -18%      │
│ trusted 12    │ cache miss storms   ██       11%  1.5k sec     │ split build:core -> tests unblock -3m        │
│ gpu      4    │ resource pressure   ██        9%  1.2k sec     │ node-b disk GC reclaims 42GiB                │
│ generic 50    │ manual gates        █         5%  .7k sec      │ prod gate awaiting approval                  │
├──────────────┴───────────────────────────────────────────────┴──────────────────────────────────────────────┤
│ queued jobs                                                                                                  │
│ repo        job             tags       q-age  runnable?   blocker               predicted start              │
│ veox-api    test-linux      rust-hi    04:12  yes         slot scarcity          01:42                       │
│ veox-web    test-linux      rust-hi    03:11  no          build failed           --                          │
│ veox-deploy sign-image      trusted    02:02  no          approval gate          --                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 11.5 Backend inputs required

- runner pools/managers/nodes/concurrency;
- manager/runner health and paused/draining state;
- per-node CPU/memory/disk/network pressure;
- runner tags, trust tiers, protected-ref eligibility;
- queued/running jobs with stage, dependencies, tags, project/ref/SHA;
- historical duration by job/stage/pool/ref;
- cache hit/miss and wait time;
- manual gate/approval state;
- broker/webhook lag;
- GitLab API latency;
- VTI skip/selection decisions.

---

## 12. Cache Observatory

### 12.1 Purpose

The cache screen answers:

- Are we full?
- What categories are consuming storage?
- Are cache hits actually improving CI time?
- Are Rust crates, Cargo sparse index, sccache, OCI layers, npm packages, Git mirrors, or action cache growing abnormally?
- Which objects are hot, stale, tainted, leased, or unsafe?
- What can be garbage-collected safely?

### 12.2 Cache categories

At minimum classify:

- Rust crates downloads;
- Cargo sparse index;
- Cargo registry metadata;
- sccache objects;
- build artifacts;
- OCI image layers;
- OCI registry mirror data;
- npm packages;
- Git mirrors / refs;
- action cache;
- material objects / aliases;
- toolchain fingerprints;
- hot cache entries;
- unknown/unclassified.

### 12.3 Cache screen mock

```text
╭SmartCache Observatory────────────────────────────────────────────────────────────────────────────────────────╮
│ status ✓ healthy  storage 871GiB/1.0TiB  87%  hit 79%  singleflight 1,234  taints 3  leases 18  GC safe 146GiB│
├───────────────┬─────────────────────────────────────────────────────┬───────────────────────────────────────┤
│ CATEGORIES     │ STORAGE / GROWTH                                    │ INSPECTOR                             │
│ crates   412G  │ crates      ████████████████ 412G +34G/24h           │ selected: crates.io/serde@1.0.203     │
│ sccache  221G  │ sccache     ████████         221G +12G/24h           │ digest: sha256:...                    │
│ OCI      118G  │ OCI layers   ████             118G +4G/24h           │ hits: 283  last: 2m ago               │
│ npm       44G  │ npm          ██                44G +1G/24h           │ trust: clean, epoch 12                │
│ git       38G  │ git mirrors  █                 38G stable            │ leases: none                          │
│ unknown   17G  │ unknown      ▌                 17G +9G/24h ⚠         │ actions: explain, evict, pin          │
├───────────────┴─────────────────────────────────────────────────────┴───────────────────────────────────────┤
│ hot keys / misses                                                                                           │
│ key                           category hit% bytes  p95 wait  verdict  taint  action                         │
│ crates.io/syn/2.0.79           crates   97  1.2G   12ms      clean    no     pin                            │
│ cargo/git/checkouts/foo        git      22  8.8G   2.1s      stale    no     refresh                         │
│ oci/layer/sha256:abc           OCI      3   14G    8.2s      tainted  yes    inspect                         │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 12.4 Existing cache data to expose

- cache requests: URL, method, hit/miss, reason code, bytes, timestamp;
- cache objects: key, digest, size, category, mutability, timestamps, hits;
- `bytes_served`, `total_requests`, `hit_count`, `miss_count`, `object_count`, `hit_ratio`, `singleflight_coalesced`;
- hot cache entries;
- build/image signatures;
- force refresh rules;
- taints, leases, verdicts, promotions;
- material objects/aliases;
- action cache;
- cache epochs;
- toolchain fingerprints;
- proxy health, registry health, CA mount, CAS/crate disk sizes.

### 12.5 Required cache endpoints

Current `/cache/summary` is too small. Add:

```text
GET /inspect/cache/summary
GET /inspect/cache/metrics
GET /inspect/cache/categories
GET /inspect/cache/hot
GET /inspect/cache/requests?after=&category=
GET /inspect/cache/taints
GET /inspect/cache/verdicts
GET /inspect/cache/gc-plan
GET /inspect/cache/object/{key}
POST /inspect/cache/action/preview
POST /inspect/cache/action/execute
```

Cache actions:

- preview GC;
- run safe GC;
- evict selected object;
- pin hot object;
- force refresh rule;
- clear taint after proof;
- prewarm category/repo/toolchain;
- show object evidence/provenance;
- export cache report.

---

## 13. VTI smart test skipper cockpit

### 13.1 Purpose

VTI is only trusted if the operator can see why tests were selected or skipped. The VTI screen must answer:

- What changed?
- Which tests were selected?
- Which tests were skipped?
- What confidence score justifies each skip?
- Have there been selector misses recently?
- Which paths/components are under-modeled?
- Did VTI actually save time without causing failures?

### 13.2 VTI mock

```text
╭VTI Smart Test Skipper────────────────────────────────────────────────────────────────────────────────────────╮
│ fleet confidence .92  saved 318 runner-min today  selector misses 2/30d  low-confidence repos 3              │
├───────────────┬──────────────────────────────────────────────────────┬──────────────────────────────────────┤
│ REPOS          │ CURRENT TEST PLAN                                    │ INSPECTOR                            │
│ veox-api .91   │ changed paths: 34  components: auth,billing          │ selected: auth::jwt_refresh          │
│ veox-web .62 ! │ selected tests: 188/2,431  skipped: 2,243            │ why: touched auth/jwt.rs             │
│ veox-core .98  │ estimated saved: 42m  risk: medium                   │ last failures: 0/180d                │
│ veox-deploy .96│                                                      │ evidence: test receipt #tr-338       │
├───────────────┴──────────────────────────────────────────────────────┴──────────────────────────────────────┤
│ skipped tests                                                                                                │
│ test                            confidence  reason                         last pass       miss history       │
│ ui::dark_mode_snapshot           .94        no impacted frontend paths      main@abc 2h     0/90d             │
│ billing::tax_edge_case           .71 ⚠      weak component mapping          main@def 6h     1/30d             │
│ deploy::rollback_canary          .98        release files unchanged         main@ghi 1h     0/180d            │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 13.3 Required VTI data

- impact decision from push/ref/SHA;
- changed paths and component mapping;
- test plan and test plan items;
- selected/skipped/required tests;
- test execution history;
- selector misses, especially last 30 days;
- per-test confidence;
- saved runner-minutes;
- false-skip suspicion;
- receipts/proof paths;
- plan validation result.

### 13.4 VTI guardrail

If selector misses exist for the entity/repo/ref/component in the last policy window, the UI must downgrade VTI confidence and prevent high-risk merge/release actions from treating VTI skips as sufficient proof without an override gate.

---

## 14. Agents and autonomous workflows

### 14.1 Purpose

Agents must feel visible and controllable, not mysterious. The operator should see:

- every agent/session/task;
- what it is trying to do;
- which grant/capability it has;
- which branch/MR/pipeline/bug it owns;
- step-by-step logs;
- diffs and artifacts;
- LLM budget/spend;
- blocked/failed reasons;
- whether multiple agents are colliding;
- how to pause, kill, reassign, or approve.

### 14.2 Agents mock

```text
╭Agents Control Center─────────────────────────────────────────────────────────────────────────────────────────╮
│ active 9  blocked 2  races 1  ready reviews 4  spend today $18.42  kill bell armed=no  autonomy live          │
├───────────────┬──────────────────────────────────────────────────────┬──────────────────────────────────────┤
│ AGENTS         │ TASKS / RACES                                        │ INSPECTOR                            │
│ ▶ fixbot-12    │ bug VE-882 auth refresh failure                      │ agent: fixbot-12                     │
│ ▶ fixbot-18    │   step 1 reproduce ✓                                 │ grant: agent_task g-883              │
│ ⚠ releasebot-3 │   step 2 patch ✓ branch fix/ve-882                   │ branch: fix/ve-882                   │
│ ◌ triagebot-7  │   step 3 pipeline ▶ 63%                               │ MR: !119                             │
│ ✕ auditbot-2   │   step 4 review ◌                                    │ logs: live                           │
│               │ race: billing hypothesis x3                           │ actions: pause, kill, view diff      │
├───────────────┴──────────────────────────────────────────────────────┴──────────────────────────────────────┤
│ recent agent events                                                                                         │
│ 12:01:02 fixbot-12 proposed patch 3 files +42 -8                                                           │
│ 12:01:04 fixbot-12 triggered pipeline #8172                                                                 │
│ 12:01:08 releasebot-3 blocked: production approval grant missing                                             │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 14.3 Dedicated agent lifecycle tables to add

Existing Agents screen is partly inferred from pipeline/audit state. Add durable lifecycle tables or equivalent event projections:

```sql
agent_sessions(id, agent_id, actor, provider, model, started_at, ended_at, status, kill_bell_state, budget_id)
agent_tasks(id, session_id, kind, target_entity, repo_id, bug_id, branch, mr_iid, status, priority, grant_id, created_at, updated_at)
agent_steps(id, task_id, seq, kind, status, summary, started_at, ended_at, evidence_ref, log_ref)
agent_messages(id, session_id, task_id, role, redacted_content, token_count, created_at)
agent_artifacts(id, task_id, kind, path, url, digest, redacted, created_at)
agent_races(id, repo_id, base_branch, status, winner_task_id, created_at, completed_at)
agent_race_hypotheses(id, race_id, task_id, branch, pipeline_id, score, status)
```

Or map the same concepts into `TuiEvent` + `EntityKind` if a separate schema is undesirable.

### 14.4 Autonomous workflow config editor

Config editing inside TUI must be schema-aware and guarded:

1. open config entity;
2. render structured form editor;
3. validate locally;
4. show diff;
5. dry-run policy;
6. preview blast radius;
7. apply through action registry;
8. show audit/evidence record.

Config screens:

- agent policy;
- grant policy;
- VTI thresholds;
- cache GC policy;
- runner scaling policy;
- release gates;
- kill bell / freeze windows;
- repo family patterns;
- saved lenses;
- theme/keymap.

---

## 15. Bugs and issues cockpit

### 15.1 Purpose

The bug board unifies local JeRyu bugs, GitLab/GitHub issues, CI failures, agent attempts, evidence, and readiness for automation.

### 15.2 Canonical bug fields

A canonical bug report includes:

- source project;
- target project;
- title;
- component;
- current behavior;
- expected behavior;
- environment;
- frequency;
- impact;
- security/privacy notes;
- no-secrets confirmation;
- reproduction steps;
- evidence;
- acceptance criteria;
- severity;
- priority;
- difficulty.

Bug records expose:

- id/title/source/target/component;
- status/severity/priority/difficulty;
- impact/security flag/owner/body;
- created/updated;
- attempt counts;
- external refs;
- events;
- attempts;
- evidence.

Statuses:

```text
needs_triage, needs_info, accepted, ready, in_progress, blocked,
fix_proposed, reviewing, verifying, done, duplicate, invalid,
cannot_reproduce, wont_do
```

Attempt statuses:

```text
pending, started, failed, fix_proposed, verified, abandoned
```

### 15.3 Bugs mock

```text
╭Bugs / Issues Cockpit─────────────────────────────────────────────────────────────────────────────────────────╮
│ total 312  ready 31  in-progress 8  fix-proposed 5  blocked 11  security 2  agentable 24                    │
├───────────────┬──────────────────────────────────────────────────────┬──────────────────────────────────────┤
│ LANES          │ BUGS                                                 │ INSPECTOR                            │
│ ready 31       │ VE-882 ✕ auth refresh failure   sev high pri p0      │ VE-882 auth refresh failure          │
│ in_prog 8      │ VE-901 ⚠ cache taint false pos  sev med              │ source: veox-web target: veox-api    │
│ blocked 11     │ VE-777 ! release gate hangs     sev high             │ attempts: 2 failed, 1 running        │
│ review 5       │                                                      │ evidence: capsule, trace, MR!119     │
│ done 201       │                                                      │ actions: assign agent, open MR       │
├───────────────┴──────────────────────────────────────────────────────┴──────────────────────────────────────┤
│ agent-ready queue                                                                                           │
│ bug     repo       component   difficulty  last failed reason        suggested agent                         │
│ VE-882  veox-api   auth        M           test now reproduced        fixbot                                 │
│ VE-901  veox-core  cache       H           needs cache object proof   cachebot                               │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 15.4 Agent bug workflow

```text
bug ready -> assign agent -> agent attempts reproduce -> patch -> pipeline -> MR -> review -> verify -> close
```

Every attempt is attached to logs, sandbox path, branch, base/head SHA, PR/MR URL, CI evidence, notes, and timestamps.

---

## 16. Git Sync and remote state

Git Sync must show where local, remote, GitLab/GitHub, mirrors, protected refs, hooks, and admission policy disagree.

### 16.1 Data shown

- tracked repositories;
- local HEAD, upstream HEAD, protected target SHA;
- ahead/behind/diverged;
- ref update events;
- pre-receive admission decisions;
- grant ids;
- mirror jobs and backup/shadow status;
- Git command events and artifacts;
- MR/PR state and checks;
- hook installation health;
- branch protection and policy SHA;
- docs/source/action registry drift.

### 16.2 Git Sync mock

```text
╭Git Sync / Remote State───────────────────────────────────────────────────────────────────────────────────────╮
│ repos 48  drift 5  hooks missing 2  denied pushes 3/24h  mirrors stale 1  protected refs clean               │
├───────────────┬──────────────────────────────────────────────────────┬──────────────────────────────────────┤
│ REPOS          │ REF STATE                                            │ INSPECTOR                            │
│ veox-api       │ main local 9fd2 remote 9fd2 mirror 9fd2 ✓            │ selected: veox-web main              │
│ veox-web !     │ main local a1b2 remote c3d4 diverged ⚠               │ ahead 2 behind 4                     │
│ veox-deploy    │ release/v1.8.4 protected, MR-only ✓                  │ last denial: no merge grant          │
│ infra-tools    │ hooks missing ✕                                      │ actions: sync, install hook, proof   │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

---

## 17. Jankurai Audit Center

### 17.1 Purpose

Jankurai should become a first-class quality/security/release audit dimension rather than a side report.

Show:

- latest score by repo;
- score trend;
- cap reason;
- blockers/warnings/info;
- duplicate clusters;
- rot findings;
- tool adoption/enforcement gaps;
- security/provenance/release anti-patterns;
- version of Jankurai used;
- evidence paths;
- autofix/issue creation actions.

### 17.2 Jankurai mock

```text
╭Jankurai Audit Center─────────────────────────────────────────────────────────────────────────────────────────╮
│ fleet avg 88.1  cap-limited 4  blockers 9  warnings 42  duplicate clusters 18  latest run 6m ago            │
├───────────────┬──────────────────────────────────────────────────────┬──────────────────────────────────────┤
│ REPOS          │ SCORE / FINDINGS                                     │ INSPECTOR                            │
│ veox-core 94   │ veox-api 87.2 ↓4.2  cap: duplicate-code              │ finding: duplicate auth parser       │
│ veox-api 87 !  │ blockers 3 warnings 11                               │ files: 4  loc: 612                   │
│ veox-web 81 ✕  │ veox-web 81.0 cap: release-safety                    │ action: create bug, assign agent     │
│ veox-deploy 90 │                                                      │ evidence: jankurai run #jk-883       │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 17.3 Jankurai data model

```rust
pub struct JankuraiRunSummary {
    pub run_id: String,
    pub repo_id: String,
    pub commit_sha: String,
    pub version: String,
    pub score: f32,
    pub prior_score: Option<f32>,
    pub cap_reason: Option<String>,
    pub blocker_count: u32,
    pub warning_count: u32,
    pub info_count: u32,
    pub duplicate_cluster_count: u32,
    pub security_finding_count: u32,
    pub provenance_finding_count: u32,
    pub release_finding_count: u32,
    pub evidence_path: Option<String>,
    pub created_at: DateTime<Utc>,
}
```

---

## 18. Runners, pools, nodes, and system utilization

### 18.1 Purpose

Runner/System view answers:

- What compute exists?
- What is healthy, paused, draining, unreachable, OOMing, or disk-full?
- Which pool/tag/trust tier is constrained?
- Are remote nodes doing useful work?
- What can be scaled, drained, or garbage-collected?

### 18.2 Runners mock

```text
╭Runners / System Utilization─────────────────────────────────────────────────────────────────────────────────╮
│ slots raw 160 usable 142 busy 128 idle 7 bad 7  nodes 8  oom 2/24h  disk critical 1  remote unreachable 0    │
├───────────────┬──────────────────────────────────────────────────────┬──────────────────────────────────────┤
│ POOLS          │ NODES / MANAGERS                                      │ INSPECTOR                            │
│ rust-hi 28 ⚠   │ node-a cpu 72 mem 61 disk 83  managers 12/16          │ selected: node-b                     │
│ rust-lo 48 ✓   │ node-b cpu 91 mem 86 disk 94 ⚠ managers 16/16         │ pressure: disk/cpu                   │
│ trusted 12 ✓   │ node-c cpu 43 mem 48 disk 51  managers 6/12           │ events: 1 OOM, 3 die                 │
│ gpu 4 ✓        │ remote-x cpu 55 mem 49 disk 78 managers 2/4           │ actions: drain, GC, logs, scale      │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 18.3 Required runtime metrics to plumb

- per-node CPU/memory/disk/network;
- Docker daemon health;
- manager count by node;
- runner system IDs and contacted_at;
- queue pressure per pool;
- GC actions and reclaimed bytes;
- node unreachable history;
- Docker `die`/`oom` events;
- pool affinity/capacity planning;
- remote runner logs;
- storage used/limit/90% warning/95% critical.

---

## 19. Release, production, rollback, and version control

### 19.1 Purpose

Release screen is a war-room. It must show:

- exact release version/ref/SHA;
- upstream/release/prod pipeline state;
- canary state and public URL;
- gate files and evidence paths;
- telemetry/e2e/identity gates;
- eligibility and blockers;
- production approval state;
- rollback target and readiness;
- foundry candidates;
- release critical/extended/research lane progress;
- pipeline doctor recommendation;
- signed artifact/SBOM/provenance/security dependencies.

### 19.2 Release mock

```text
╭Release Train─────────────────────────────────────────────────────────────────────────────────────────────────╮
│ veox-deploy v1.8.4 sha 9fd2c4  canary 72% ✓ telemetry green  prod approval waiting  rollback ready v1.8.3    │
├───────────────┬──────────────────────────────────────────────────────┬──────────────────────────────────────┤
│ TRAIN          │ GATES                                                │ INSPECTOR                            │
│ candidate ✓    │ remote canary       ✓ evidence remote-canary.json    │ selected: prod approval              │
│ canary ▶ 72%   │ canary e2e          ✓ evidence e2e.json              │ required: human grant                 │
│ prod ◌ wait    │ telemetry           ✓ evidence telemetry.json        │ blockers: unsigned artifact? no       │
│ rollback ✓     │ identity            ✓ evidence identity.json         │ rollback: v1.8.3 ready               │
│               │ signatures          ✓ 128/128                         │ actions: approve, rollback, doctor   │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 19.3 Release action safety

Production-risk actions require:

- exact version;
- exact source SHA;
- target environment;
- required gate list and pass/fail status;
- artifact signature and provenance status;
- security/Jankurai status;
- rollback target and test state;
- grant id and actor;
- typed confirmation token, e.g. `PROMOTE v1.8.4 9fd2c4`.

---

## 20. Security, signed artifacts, secrets, and supply chain

### 20.1 Security Center

Security screen aggregates:

- SAST;
- dependency scans;
- container scans;
- IaC scans;
- secret leak detection;
- Git/admission policy violations;
- protected ref violations;
- vulnerability severity by repo/family;
- security waiver/approval state;
- mapping to release blockers.

### 20.2 Artifacts / provenance screen

Show:

- build artifacts;
- image signatures;
- build signatures;
- SBOM status;
- provenance/attestation;
- source SHA;
- pipeline/job that produced artifact;
- verification status;
- downstream deployments;
- artifact evidence and digest;
- missing or stale signatures.

### 20.3 Secrets / Vault screen

Only show redacted metadata. Never render plaintext secrets.

Inspectable secret data:

- Vault address, initialized, sealed, healthy, token present;
- mount/prefix/bootstrap/env paths;
- secret authorities;
- release secret sets by repo/version/target;
- rendered deploy/runtime env paths;
- audit bundle/report path;
- runtime/recovery Vault paths;
- expiry and rotation/finalization timestamps;
- secret audit events: repo, version, target, action, status, detail, timestamp.

UI rules:

- Secrets panel uses redaction by default.
- Copy operations copy paths/fingerprints, not values.
- Any recovery/bootstrap action is production/security risk tier.
- Sealed Vault is a global header warning.

---

## 21. Evidence timeline / flight recorder

### 21.1 Purpose

Evidence is the truth spine of JeRyu. It should be a searchable proof timeline across jobs, pipelines, agents, bugs, release gates, cache verdicts, Git admissions, grants, secrets, LLM budgets, and actions.

### 21.2 Timeline item

```text
12:01:04.882  job#119332  ✕ failure capsule created
  repo=veox-api pipeline#8172 stage=test actor=runner/node-b
  evidence=capsule://fc-882 trace=gitlab://job/119332/trace
  related: bug VE-882, agent fixbot-12, cache verdict cv-440
```

### 21.3 Evidence graph

Every entity detail has a proof tab showing:

```text
selected entity
  -> events
  -> evidence capsules
  -> action previews/results
  -> grants/intents
  -> artifacts/signatures
  -> cache verdicts
  -> test receipts
  -> bugs/attempts
  -> release gates
  -> logs/traces
```

### 21.4 Required proof API

```text
GET /inspect/proof?entity=&kind=&since=&actor=&severity=&cursor=
GET /inspect/evidence/{evidence_id}
GET /inspect/entity/{kind}/{id}/evidence
MCP resource: jeryu://proof/timeline
MCP tool: jeryu.search_proof_timeline
```

Filters:

- entity kind/id;
- actor;
- event type;
- severity;
- request id;
- correlation id;
- repo/ref/SHA;
- branch/MR;
- job/pipeline;
- action id;
- grant id;
- security finding;
- evidence kind.

---

## 22. Command palette and search

### 22.1 Command palette

`:` opens command palette. It searches:

- top-level screens;
- current entity actions;
- global actions;
- saved lenses;
- repos/families;
- jobs/pipelines;
- bugs;
- agents;
- cache objects;
- evidence;
- settings;
- help.

Command result rows show risk badge and preview requirements:

```text
> scale rust-hi
R2  scale pool rust-hi +N       preview required
R0  open Queue Lab filtered rust-hi
R0  show rust-hi bottlenecks
```

### 22.2 Global search

`Ctrl-/` searches all indexed local state and recent streams:

```text
repo:veox-api status:failed kind:job
bug:ready priority:p0
cache category:crates growth:>10GiB
vti confidence:<.75 selector_miss:30d
agent status:blocked grant:missing
release version:1.8.4 gate:waiting
sha:9fd2c4
```

### 22.3 Filter grammar

Support simple filters first:

```text
key:value
key!=value
key>value
key<value
text words
"quoted phrase"
kind:job status:failed repo:veox-api
```

Then add boolean operators if needed:

```text
(repo:veox-api OR family:veox) AND status:failed AND age<24h
```

Saved views/lenses:

```toml
[[lenses]]
name = "Critical only"
filter = "severity:critical OR release:blocking OR security:high"
```

---

## 23. Backend inspection plane required for the dream TUI

### 23.1 Architectural principle

The TUI should consume a typed **inspection plane**, not rebuild truth by directly querying every backend in screen code.

```text
GitLab webhooks/API ┐
State DB            ├── collectors/projections ──> Inspection API / event stream ──> TUI store/render
Docker/remotes      │
Cache gateway       │
Vault/secrets       │
Agents/autonomy     │
Jankurai/artifacts  │
Git/admission       ┘
```

### 23.2 Minimum API

```text
GET  /inspect/read-model
GET  /inspect/events?after={cursor}&limit={n}&filter=...
GET  /inspect/entity/{kind}/{id}
GET  /inspect/entities?kind=&filter=&cursor=
GET  /inspect/repo-families
GET  /inspect/repo/{repo_id}/summary
GET  /inspect/pipeline/{project_id}/{pipeline_id}/graph
GET  /inspect/jobs/{project_id}/{job_id}/trace?offset=
GET  /inspect/jobs/{project_id}/{job_id}/trace/stream
GET  /inspect/proof?entity=&kind=&since=&actor=&cursor=
POST /inspect/action/preview
POST /inspect/action/execute
GET  /inspect/action/{action_id}/events
GET  /inspect/health/deep
GET  /events/stream
```

### 23.3 MCP resources/watch additions

Current MCP is tool-focused. Add resources/subscriptions:

```text
resources/list
resources/read
resources/subscribe
jeryu://read-model
jeryu://events?after=N
jeryu://entities/{kind}/{id}
jeryu://repos/{repo}/summary
jeryu://pipelines/{project}/{pipeline}/graph
jeryu://jobs/{project}/{job}/trace
jeryu://cache/summary
jeryu://proof/timeline
```

### 23.4 Read model skeleton

```rust
pub struct TuiReadModel {
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    pub event_cursor: u64,
    pub source_freshness: Vec<SourceFreshness>,
    pub system_health: SystemHealth,
    pub mission: MissionSnapshot,
    pub workflow: WorkflowAtlasSnapshot,
    pub repo_families: Vec<RepoFamilySummary>,
    pub repos: Vec<RepoSummary>,
    pub pipelines: Vec<PipelineSummary>,
    pub queue: CapacitySummary,
    pub runners: RunnerFleetSummary,
    pub cache: CacheSummary,
    pub vti: VtiFleetSummary,
    pub agents: AgentFleetSummary,
    pub bugs: BugBoardSummary,
    pub release: ReleaseSummary,
    pub security: SecuritySummary,
    pub artifacts: ArtifactSummary,
    pub evidence: EvidenceSummary,
    pub attention: Vec<AttentionItem>,
    pub next_actions: Vec<ActionDescriptor>,
}
```

### 23.5 Event model skeleton

```rust
pub struct TuiEvent {
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub kind: TuiEventKind,
    pub severity: Severity,
    pub entity: EntityRef,
    pub parent: Option<EntityRef>,
    pub summary: String,
    pub detail: Option<serde_json::Value>,
    pub correlation_id: Option<String>,
    pub evidence_refs: Vec<EvidenceRef>,
    pub action_refs: Vec<ActionRef>,
    pub source: DataSource,
    pub stale_after_ms: u64,
}
```

Event kinds should cover:

```text
JobCreated, JobQueued, JobStarted, JobProgress, JobFinished, JobFailed, JobCanceled, JobTraceChunk,
PipelineCreated, PipelineUpdated, PipelineFinished, PipelineBlocked,
RepoChanged, PushReceived, MrUpdated, GitAdmissionDecision,
RunnerStarted, RunnerStopped, RunnerOom, RunnerDraining, NodePressureChanged,
CacheHit, CacheMiss, CacheTainted, CacheVerdict, CacheGcPlanned, CacheGcFinished,
VtiPlanCreated, VtiTestSelected, VtiTestSkipped, VtiSelectorMiss,
AgentSessionCreated, AgentTaskStarted, AgentStepFinished, AgentPatchProposed, AgentRaceCreated, AgentRaceWinnerSelected,
BugSubmitted, BugUpdated, BugAttemptStarted, BugAttemptFinished,
ReleaseAttemptCreated, CanaryStarted, CanaryGatePassed, CanaryGateFailed, ProdApprovalRequested, ProdPromoted, RollbackStarted,
SecretRotated, VaultHealthChanged,
JankuraiRunFinished, SecurityFindingCreated, ArtifactSigned, ArtifactVerificationFailed,
ActionPreviewed, ActionStarted, ActionProgress, ActionFinished, ActionFailed,
SourceBecameStale, SourceRecovered
```

### 23.6 EntityRef taxonomy

```rust
pub enum EntityKind {
    Fleet,
    RepoFamily,
    Repo,
    Branch,
    Commit,
    MergeRequest,
    Pipeline,
    PipelineBridge,
    Job,
    TestPlan,
    Test,
    SelectorMiss,
    RunnerPool,
    RunnerManager,
    Runner,
    Node,
    CacheObject,
    CacheRequest,
    CacheVerdict,
    Agent,
    AgentSession,
    AgentTask,
    AgentRace,
    Bug,
    BugAttempt,
    ReleaseAttempt,
    ReleaseGate,
    Artifact,
    Signature,
    Sbom,
    SecurityFinding,
    JankuraiRun,
    JankuraiFinding,
    SecretAuthority,
    ReleaseSecretSet,
    EvidenceCapsule,
    AuditEvent,
    CapabilityIntent,
    CapabilityGrant,
    AdmissionDecision,
    Action,
    Source,
}

pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
    pub label: String,
    pub repo_id: Option<String>,
    pub project_id: Option<i64>,
}
```

---

## 24. Rust implementation architecture

### 24.1 Recommended stack

- **TUI rendering:** Ratatui-style immediate rendering with crossterm backend.
- **Async runtime:** Tokio.
- **HTTP client/server:** existing stack; use typed clients, not screen-local reqwest calls everywhere.
- **Serialization:** serde.
- **DB fallback:** existing SQLx Any / SQLite default / RedlineDB opt-in.
- **State management:** reducer + normalized stores + event ring buffers.
- **Testing:** golden terminal snapshots, fake backend, event replay, deterministic fixtures.

Do not hardcode specific crate versions in the spec. Resolve versions in the repository at implementation time.

### 24.2 Module layout

```text
src/tui/
  mod.rs
  app.rs                  App state, navigation stack, mode, reducer entry
  main_loop.rs            terminal init, input/event/render loop
  input/
    router.rs             key/mouse routing
    keymap.rs             configurable key definitions
    command_palette.rs
    help.rs
  model/
    entity.rs             EntityRef, EntityDetail, relationships
    events.rs             TuiEvent cache, cursors, filters
    actions.rs            action descriptors/previews/results
    freshness.rs          source TTLs and stale labels
    search.rs             fuzzy/global index
    nav.rs                Route, breadcrumbs, back stack
    graph.rs              workflow graph models
  data/
    client.rs             InspectionClient trait
    inspection_http.rs    new HTTP inspection API client
    mcp_resources.rs      MCP resource/tool fallback client
    local_db_fallback.rs  local developer mode
    event_stream.rs       SSE/WebSocket/event cursor
    trace_stream.rs       job log streaming/fallback polling
    fixtures.rs           demo/test fixtures
  store/
    entity_store.rs       normalized entities and relationships
    event_store.rs        ring buffer + cursor map
    table_store.rs        virtualized table state
    trace_store.rs        bounded trace buffers
    selection_store.rs    selected rows/cards/entities
    lens_store.rs         filters/saved views
  pages/
    workflow_atlas.rs
    mission.rs
    repo_family.rs
    repo.rs
    pipelines.rs
    trace.rs
    queue.rs
    runners.rs
    cache.rs
    vti.rs
    agents.rs
    bugs.rs
    git_sync.rs
    bottlenecks.rs
    jankurai.rs
    security.rs
    artifacts.rs
    release.rs
    secrets.rs
    evidence.rs
    config.rs
    source_doctor.rs
    llm_autonomy.rs
  widgets/
    status_header.rs
    tabs.rs
    attention_queue.rs
    entity_table.rs
    virtual_table.rs
    inspector.rs
    progress_bar.rs
    sparkline.rs
    heatmap.rs
    dag.rs
    log_viewer.rs
    diff_viewer.rs
    proof_modal.rs
    form_editor.rs
    timeline.rs
    capacity_meter.rs
    freshness_badge.rs
    mini_chart.rs
    event_ribbon.rs
    command_palette.rs
  theme/
    palette.rs
    symbols.rs
    terminal_capabilities.rs
  test_support/
    snapshots.rs
    fake_backend.rs
    event_replay.rs
    fixtures.rs
```

### 24.3 App state

```rust
pub struct App {
    pub route: Route,
    pub nav_stack: Vec<Route>,
    pub history: Vec<Route>,
    pub focus: FocusPath,
    pub selected: SelectionState,
    pub mode: AppMode,
    pub filters: FilterState,
    pub stores: Stores,
    pub data: Box<dyn InspectionClient>,
    pub keymap: KeyMap,
    pub theme: Theme,
    pub command_palette: CommandPaletteState,
    pub pending_action: Option<ActionFlow>,
    pub diagnostics: TuiDiagnostics,
    pub animation: AnimationState,
}

pub enum AppMode {
    Normal,
    Search { scope: SearchScope, query: String },
    CommandPalette,
    ActionPreview,
    ProofModal,
    FormEdit,
    MultiSelect,
    Help,
}
```

### 24.4 Inspection client trait

```rust
#[async_trait::async_trait]
pub trait InspectionClient: Send + Sync {
    async fn read_model(&self) -> anyhow::Result<TuiReadModel>;
    async fn events_after(&self, cursor: u64, filter: EventFilter) -> anyhow::Result<Vec<TuiEvent>>;
    async fn entity_detail(&self, entity: &EntityRef) -> anyhow::Result<EntityDetail>;
    async fn workflow_graph(&self, scope: WorkflowScope) -> anyhow::Result<WorkflowGraph>;
    async fn pipeline_graph(&self, project_id: i64, pipeline_id: i64) -> anyhow::Result<WorkflowGraph>;
    async fn job_trace(&self, project_id: i64, job_id: i64, offset: u64) -> anyhow::Result<TraceChunk>;
    async fn action_preview(&self, action: ActionRequest) -> anyhow::Result<ActionPreview>;
    async fn action_execute(&self, action: ActionRequest, proof: ProofAck) -> anyhow::Result<ActionResult>;
    fn subscribe_events(&self, filter: EventFilter) -> EventStream;
    fn subscribe_trace(&self, project_id: i64, job_id: i64) -> TraceStream;
}
```

Implementations:

- `HttpInspectionClient` for new endpoints.
- `McpInspectionClient` for MCP resources/tools.
- `LocalDbInspectionClient` for developer fallback.
- `FixtureInspectionClient` for demo/testing.

### 24.5 Event loop

```text
terminal input ─┐
resize events ──┤
stream events ──┤       ┌───────────────┐       ┌──────────────┐
timers ─────────┼──────▶│ app reducer   │──────▶│ dirty render │
action results ─┤       └───────────────┘       └──────────────┘
trace chunks ───┘
```

Rules:

- Never block render on network.
- Coalesce high-frequency events into frame batches.
- Render only on dirty state or heartbeat.
- Prioritize visible traces and selected entities.
- Apply backpressure to streams; drop hidden low-value trace chunks before event cursors.
- Keep a bounded memory window.
- Persist enough cursor state to recover without blanking.

### 24.6 Performance targets

| Target | Requirement |
|---|---|
| initial interactive paint | `< 500 ms` with cached snapshot; `< 2 s` cold network |
| input latency | p95 `< 50 ms` |
| render frame | p95 `< 16 ms`, p99 `< 33 ms` on common screens |
| event apply latency | p95 `< 100 ms` from stream receipt |
| trace display latency | p95 `< 250 ms` from chunk receipt |
| scale | 500 repos, 50k recent jobs, 10k events in memory window, 100 trace subscriptions with one/few visible prioritized |
| memory | bounded stores; default target `< 250 MB` |

Use virtualized tables for all large lists. Never render thousands of rows/widgets if only 40 are visible.

---

## 25. Action model and safety

### 25.1 Risk tiers

| Tier | Examples | UI requirement |
|---|---|---|
| `R0 Read` | open logs, show proof, inspect cache object | immediate |
| `R1 Local safe` | create local bug, rerun local audit, copy report | preview optional, undo if possible |
| `R2 CI mutation` | retry job, cancel job, run tests, assign agent | preview required |
| `R3 Repo mutation` | propose patch, create branch/MR, update config | preview + target confirmation |
| `R4 Release mutation` | release approval, promote canary, rotate release secrets | proof modal + grant |
| `R5 Production/security` | prod promotion, rollback, revoke/recover secrets, kill bell | strict proof + typed confirmation + audit |

### 25.2 Action descriptor

```rust
pub struct ActionDescriptor {
    pub id: String,
    pub label: String,
    pub target: EntityRef,
    pub risk: RiskTier,
    pub side_effect_class: SideEffectClass,
    pub required_grants: Vec<GrantRequirement>,
    pub dry_run_available: bool,
    pub undo_available: bool,
    pub confirmation: ConfirmationPolicy,
    pub expected_evidence: Vec<EvidenceKind>,
    pub surfaces: Vec<ActionSurface>,
}
```

### 25.3 Proof modal

```text
╭Action Preview──────────────────────────────────────────────────────────────╮
│ Action: scale pool rust-hi +4                                               │
│ Risk: R2 CI capacity mutation                                                │
│ Target: pool rust-hi, nodes local-a,node-b                                   │
│ Why: queue p95 estimated -41%, 19 runnable jobs waiting                      │
│ Preconditions: node-b disk <95% after GC, GitLab ready, Docker ready         │
│ Side effects: create 4 runner managers, register tokens, start containers    │
│ Evidence: bottleneck report ci-bot-883, node health node-212                 │
│ Idempotency: action/scale-rust-hi/2026-05-26T10:42:00Z                       │
│                                                                            │
│ [y] execute  [d] dry-run only  [Esc] cancel                                  │
╰────────────────────────────────────────────────────────────────────────────╯
```

Production proof modal additionally shows exact version/SHA/environment/rollback/grant and requires typed confirmation.

### 25.4 Mutating actions to route through registry/capability

- `run_tests` → targeted pipeline action.
- `propose_patch` → branch/MR action.
- `race_patches` → race action; add race status/winner/cleanup actions.
- `request_merge` → merge proof gate before GitLab accept.
- pool scale/pause/drain → action registry entries.
- release promote/rollback → production approval grant.
- cache GC/force refresh → cache trust policy.
- workflow config edits → schema/dry-run/proof.
- secrets rotation/recovery → production/security policy.
- agent pause/kill/reassign → autonomy policy.

---

## 26. Source Doctor and data freshness

### 26.1 Purpose

Source Doctor is the operator’s answer to “Can I trust this dashboard?”

It shows:

- component health;
- data source freshness;
- stream status;
- event cursor gaps;
- schema version;
- action registry hash;
- MCP manifest hash;
- DB backend/profile;
- docs/source drift;
- broker lag;
- webhook delivery ledger;
- GitLab API latency;
- Docker/Vault/cache health;
- fixture/demo mode flags.

### 26.2 Known drift to flag

| Drift / risk | UI treatment |
|---|---|
| docs describe RedlineDB-only but source uses SQLite default + RedlineDB opt-in | header/system shows actual DB backend; Source Doctor warning if docs mismatch |
| older MCP/action listings lack bug tools | consume generated action registry and MCP manifest; show hash mismatch |
| `/cache/summary` older docs imply unauthenticated but source requires `X-Jeryu-Token` | show auth posture in API doctor |
| `ListAllowedActions` may drift from action registry | warn if capability list differs |
| `request_merge` may be more direct than docs imply | force UI proof gate regardless of backend permissiveness |
| MR hooks accepted/logged but not acted on | mark MR data partial until plumbed |

### 26.3 Deep health endpoint

Add:

```text
GET /inspect/health/deep
```

Return:

- GitLab ready + latency;
- DB backend/path/latency;
- Docker ready + managed container counts;
- Vault health;
- cache proxy/registry health;
- broker backend/lag/DLQ;
- runner pool readiness;
- disk pressure;
- last reconciliation timestamp;
- stream subscriber counts;
- schema/action registry versions.

---

## 27. Backend plumbing backlog

### 27.1 Must-have for vNext MVP

1. Expose `TuiReadModel` and `TuiEvent` externally via HTTP and/or MCP resources.
2. Add event streaming for jobs/pipelines/pushes/actions/cache/release/source freshness.
3. Add selected job trace streaming or robust offset polling endpoint.
4. Add multi-pipeline graph API with stages, `needs`, bridges, child pipelines, artifact dependencies, manual gates, critical path.
5. Add repo-family projection.
6. Add queue/capacity summary and loss decomposition.
7. Expand cache read APIs: categories, hot objects, taints, verdicts, GC plan.
8. Add dedicated agent lifecycle projection/tables.
9. Add searchable proof timeline.
10. Make action registry the single generated source for CLI/TUI/MCP/capability action listings.

### 27.2 Should-have

1. Merge Request webhook ingestion into durable MR state.
2. Webhook delivery ledger with UUID, body hash, topic, offset, status, processing latency.
3. Broker observability: backend, producer health, consumer lag, DLQ/errors, per-topic throughput.
4. Docker/container/node resource metrics.
5. GitLab artifact parsing: JUnit, coverage, code quality, SAST, dependency/container scans, benchmark JSON, release gate JSON, `nextest` archives.
6. Vault lease/audit metadata without secret values.
7. LLM provider and key-pool health endpoint.
8. Runtime profile endpoint.
9. Prometheus/OpenTelemetry exporter.
10. GitHub/GitLab parity through `GitHost` resources.
11. Jankurai run persistence and trend model.
12. Artifact/SBOM/provenance summary model.

### 27.3 Nice-to-have / dream

1. Predictive time-to-green model.
2. What-if simulator for pool scaling/cache prewarm/stage split.
3. Agent race winner finalizer and losing-branch cleanup.
4. Flake intelligence and quarantine workflow.
5. Cost/resource economics by repo/family/pool.
6. Replayable incident/action/evidence chains.
7. Ownership map by component/path/repo.
8. Knowledge graph mode.
9. Demo “scream mode” with rich animations.
10. Terminal share/export mode: SVG/HTML/text snapshots.

---

## 28. Demo, capture, and deterministic fixtures

Preserve and expand existing deterministic capture modes:

```bash
jeryu tui --demo --tab workflow --width 180 --height 50 --screenshot out/workflow.svg
jeryu tui --capture fixture.json --tab cache --output out/cache.txt
jeryu repo capture-tui-screenshots --all-tabs
```

Fixtures:

- green fleet;
- queue saturation;
- failed pipeline;
- VTI degraded;
- cache almost full;
- cache taint incident;
- agent race;
- bug ready queue;
- release canary;
- production rollback;
- security finding;
- unsigned artifact;
- Jankurai regression;
- Git drift;
- stale Docker source;
- broker lag.

Golden snapshot tests should validate:

- screen structure;
- key labels;
- stale badges;
- responsive layout;
- graph node state;
- action proof modal;
- logs/traces;
- redaction rules;
- no panic on missing sources.

---

## 29. Testing strategy

### 29.1 Unit tests

- reducers for each event kind;
- freshness/staleness transitions;
- action risk classification;
- filter grammar;
- graph layout and spatial navigation;
- capacity model calculations;
- cache category classification;
- VTI confidence downgrade rules;
- attention ranking;
- proof modal requirements;
- redaction utilities.

### 29.2 Integration tests

- fake inspection server streams events;
- selected job trace stream updates visible trace;
- reconnect after stream drop without blank screen;
- action preview/execute lifecycle;
- MR hook projection when added;
- cache GC preview;
- agent task event projection;
- release gate proof;
- Source Doctor detects schema/action drift.

### 29.3 Golden UI tests

Use deterministic fixtures and terminal sizes:

```text
80x24, 100x30, 140x40, 180x50, 220x60
```

Each top-level tab gets at least:

- healthy fixture;
- degraded fixture;
- stale-source fixture;
- empty-but-valid fixture;
- high-volume fixture.

### 29.4 Safety tests

- no production action executes without proof modal;
- `request_merge` cannot be executed from UI without merge proof;
- secrets never render plaintext;
- cache taint cannot be cleared without evidence/grant;
- agent cannot exceed grant envelope;
- stale data cannot be used as production proof without explicit override;
- Ctrl-C during action shows safe quit/continue prompt.

---

## 30. Implementation phases

### Phase 0 — Foundation alignment

- Generate source-of-truth action registry manifest.
- Add schema/action registry version hashes.
- Create repo-family config/projection.
- Define `EntityRef`, `TuiReadModel`, `TuiEvent`, `ActionPreview`, `ActionResult` contracts.
- Add fake backend and fixtures.

### Phase 1 — Flight Deck shell

- Build global shell/header/tab bar/bottom bar.
- Implement route/nav stack, focus model, keymap, command palette, help overlay.
- Implement normalized stores and reducer.
- Implement Mission and Workflow Atlas using fixtures first.
- Add Source Doctor basic view.

### Phase 2 — Inspection client and realtime

- Implement HTTP/MCP/local inspection clients.
- Add read-model/event cursor polling fallback.
- Add SSE/WebSocket stream if backend available.
- Add anti-blanking and source freshness.
- Add live event ribbon.

### Phase 3 — Workflow, pipelines, traces

- Add multi-pipeline DAG renderer.
- Add spatial graph navigation.
- Add selected job inspector and trace viewer.
- Add critical path and ETA placeholders.
- Add parsed artifact summary hooks.

### Phase 4 — Fleet capacity, runners, cache, VTI

- Implement Queue Lab and theoretical limit model.
- Implement Runners/System view.
- Implement Cache Observatory with categories/taints/GC preview.
- Implement VTI cockpit with plan validation/selector miss proof.

### Phase 5 — Repo families, agents, bugs, Git sync

- Implement family drilldown and repo overview.
- Add dedicated Agents lifecycle view/projection.
- Add bug board/detail/agent attempt workflow.
- Add Git Sync/ref/admission/mirror view.

### Phase 6 — Release, evidence, security, artifacts, secrets

- Implement Release war-room with proof-gated actions.
- Implement Evidence timeline/search.
- Implement Security and Artifacts views.
- Implement Secrets/Vault redacted view.
- Wire all high-risk action proof modals.

### Phase 7 — Jankurai, LLM/autonomy, polish

- Add Jankurai audit center/trends.
- Add LLM/autonomy spend/provider/kill-bell views.
- Add animation modes and theme polish.
- Add screenshot/capture/demo modes.
- Run full acceptance suite.

---

## 31. Acceptance criteria

### 31.1 Navigation and control

- Any top-level screen reachable in one key or command palette.
- Any selected entity drillable with `Enter`.
- `Esc` always goes one scope up or closes the current modal.
- `Tab`/`Shift-Tab` always changes pane focus.
- Arrow keys work in all tables and spatially in DAGs.
- `/` filters current pane.
- `Ctrl-/` searches globally.
- `:` opens command palette.
- Contextual help always reflects current pane.

### 31.2 Global observability

- Global view shows all repo families and hot repos.
- Live queue shows running/queued/failed/blocked counts.
- Capacity panel shows theoretical frontier, queue pressure, lost capacity, and loss buckets.
- Attention queue ranks blockers with one-line explanations.
- Every displayed number shows freshness/source on inspection.

### 31.3 Repo and workflow drilldown

- Repo family -> repo -> pipeline -> job -> trace works entirely from keyboard.
- Workflow Atlas shows planned gates before GitLab jobs exist.
- Pipeline graph includes live job status, skipped/reused states, manual gates, child pipelines when available.
- Selected job trace updates live and supports search/pause/follow.
- Failed job exposes capsule, blocker explanation, create-bug action, retry action preview.

### 31.4 Cache/VTI/agents/bugs

- Cache screen answers fullness, category usage, hot objects, hit/miss, taint/trust, safe GC plan.
- VTI screen shows selected/skipped tests, confidence, selector misses, and proof for each skip.
- Agents screen shows sessions/tasks/steps/logs/grants/branches/MRs/spend and pause/kill/reassign actions.
- Bugs screen shows cross-repo lanes, ready queue, attempts, evidence, agent assignment.

### 31.5 Release/security/proof

- Release screen shows exact version/SHA, gates, canary/prod/rollback state.
- Production actions require proof modal and typed confirmation.
- Security and artifact screens show blocking findings and provenance/signature/SBOM status.
- Evidence timeline can find proof for any major entity.
- Secrets screen never renders plaintext values.

### 31.6 Performance/resilience

- No blank screens during transient source failure.
- Initial render meets target with cached snapshot.
- Input latency p95 `< 50 ms`.
- Render frame p95 `< 16 ms` on common screens.
- Event storms do not freeze UI.
- Large tables are virtualized.

---

## 32. Implementation agent checklist

Use this checklist when assigning build tasks.

### Shell and input

- [ ] `App`, `Route`, `FocusPath`, `SelectionState`, `AppMode` implemented.
- [ ] Universal keymap implemented and configurable.
- [ ] Breadcrumb/nav stack implemented.
- [ ] Command palette implemented.
- [ ] Help overlay generated from active pane keymap.
- [ ] Responsive shell implemented for 80/100/140/180/220 column widths.

### Data and stores

- [ ] `InspectionClient` trait implemented.
- [ ] Fixture backend implemented.
- [ ] HTTP/MCP/local clients behind feature flags or runtime profile.
- [ ] Entity store normalized by `EntityRef`.
- [ ] Event store with cursor and bounded ring buffer.
- [ ] Trace store with offset/dedup/backpressure.
- [ ] Freshness model and TTLs implemented.
- [ ] Source Doctor consumes freshness and component health.

### Screens

- [ ] Workflow Atlas.
- [ ] Mission Control.
- [ ] Repo Families.
- [ ] Repo Overview.
- [ ] Pipelines/DAG.
- [ ] Trace viewer.
- [ ] Queue Lab.
- [ ] Runners/System.
- [ ] Cache Observatory.
- [ ] VTI cockpit.
- [ ] Agents Control Center.
- [ ] Bugs board/detail.
- [ ] Git Sync.
- [ ] Jankurai.
- [ ] Security.
- [ ] Artifacts/Provenance.
- [ ] Release war-room.
- [ ] Secrets/Vault.
- [ ] Evidence timeline.
- [ ] Config editor.
- [ ] LLM/Autonomy.

### Safety

- [ ] All mutating actions go through action registry/capability.
- [ ] Risk tiers displayed everywhere.
- [ ] Action preview implemented.
- [ ] Proof modal implemented.
- [ ] Idempotency keys included.
- [ ] Action events stream into Evidence timeline.
- [ ] Production/secret actions require typed confirmation.
- [ ] Stale data cannot satisfy production proof silently.

### Polish

- [ ] Semantic color palette with truecolor/256/mono fallback.
- [ ] Animation modes: off/low/rich/scream.
- [ ] Reduced-motion setting.
- [ ] Copy/export actions.
- [ ] Demo fixtures.
- [ ] SVG/text screenshot capture.
- [ ] Golden snapshot tests.

---


## 33. Additional high-value screens and superpowers

### 33.1 CI Bottleneck Lab

The Queue Lab explains current capacity. **CI Bottleneck Lab** explains historical and structural slowness.

It should answer:

- Which jobs consume the most wall-clock time?
- Which jobs create the critical path most often?
- Which pools/tags are chronically constrained?
- Which repos regress build/test duration over time?
- Which failures are flaky vs deterministic?
- Which cache misses correlate with slowdowns?
- Which VTI decisions saved time or produced risk?

Bottleneck score:

```text
bottleneck_score = avg_duration_weight
                 + p95_duration_weight
                 + critical_path_frequency_weight
                 + queue_wait_weight
                 + retry_flake_weight
                 + cache_miss_weight
                 + failure_rate_weight
                 + repo_priority_weight
```

Mock:

```text
╭CI Bottleneck Lab─────────────────────────────────────────────────────────────────────────────────────────────╮
│ scope fleet last 14d  slowest critical path: veox-api/test-linux  p95 13m42s  queue p95 04m12s              │
├───────────────┬──────────────────────────────────────────────────────┬──────────────────────────────────────┤
│ FILTERS        │ BOTTLENECKS                                          │ INSPECTOR                            │
│ family veox-*  │ 1 veox-api/test-linux      score 98  p95 13m42s      │ selected: test-linux                 │
│ pool rust-hi   │ 2 veox-web/build-ui        score 91  p95 11m03s      │ avg 08m21 p95 13m42 max 21m10        │
│ ref main       │ 3 veox-core/cargo-clippy   score 79  p95 09m44s      │ queue p95 04m12 cache miss 22%       │
│ window 14d     │ 4 veox-deploy/sign-image   score 75  p95 06m12s      │ appears on critical path 62%         │
│               │                                                      │ actions: split, cache prewarm, VTI   │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Required data:

- historical `ci_job_runs` by repo/job/stage/ref/pool;
- latest/avg/p50/p95/max durations;
- queue duration vs runtime;
- critical path participation;
- retry/failure/flake rate;
- cache hit/miss and wait contribution;
- VTI selection/skip relation;
- runner pool/node attribution;
- trace size/tail and stuck suspicion;
- recommendations generated by pipeline doctor.

### 33.2 Code churn, velocity, and risk

This screen helps answer: **“Where is change velocity creating operational risk?”**

Metrics:

- commits by repo/family/time window;
- changed files and lines added/deleted;
- hot files/components;
- ownership concentration;
- dependency/config/workflow changes;
- release-critical paths touched;
- churn correlated with test failures;
- churn correlated with VTI selector misses;
- bug creation/closure velocity;
- agent-authored vs human-authored changes;
- review latency and merge latency;
- risky file clusters: secrets, release configs, CI configs, executor/cache code.

Mock:

```text
╭Code Churn / Risk─────────────────────────────────────────────────────────────────────────────────────────────╮
│ window 7d  commits 418  files 1,942  +84k -39k  risk high repos 4  agent-authored 17%                       │
├───────────────┬──────────────────────────────────────────────────────┬──────────────────────────────────────┤
│ REPOS          │ HOTSPOTS                                             │ INSPECTOR                            │
│ veox-api ⚠     │ auth/             +4.2k -1.1k  failures +3           │ selected: auth/                      │
│ veox-web ✕     │ ci/               +980 -120   VTI miss 1             │ owners: 2 primary, 1 new contributor │
│ veox-core ✓    │ cache/            +2.1k -800  cache misses +18%      │ tests impacted: 188 selected         │
│ veox-deploy ⚠  │ release/          +620 -40    prod gate touched      │ bugs opened: 3  agents: 1            │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Actions:

- open impacted workflow;
- request focused Jankurai audit;
- request targeted tests;
- create risk bug;
- assign review owner;
- compare before/after duration or failure rate.

### 33.3 Flake Radar

Flaky tests consume capacity and confuse agents. Add a Flake Radar lens that can live under VTI, Bottlenecks, or Tests.

Show:

- test name, repo, component;
- pass/fail/intermittent history;
- last failure and failure signature;
- affected branches;
- quarantine status;
- retry count and slot-seconds wasted;
- likely owner;
- whether VTI must always select it until stable.

Actions:

- mark flaky with evidence;
- quarantine with policy;
- create bug;
- assign agent;
- force selection in VTI;
- open trace/capsule.

### 33.4 LLM and autonomy screen

The LLM/Autonomy screen is for governance, safety, and cost.

Show:

- provider health;
- key source policy and redaction posture;
- spend by provider/model/repo/agent/task;
- token usage and failure rate;
- budget ledger entries;
- active autonomy launch ledger;
- Evidence Gate / VibeGate verdicts;
- kill bell status;
- freeze windows;
- blocked grants;
- agent authority envelope;
- recent agent tool calls and action outcomes.

Mock:

```text
╭LLMs / Autonomy───────────────────────────────────────────────────────────────────────────────────────────────╮
│ spend today $18.42  budget left $81.58  providers 2/3 healthy  kill bell armed=no  freeze window none        │
├───────────────┬──────────────────────────────────────────────────────┬──────────────────────────────────────┤
│ PROVIDERS      │ AUTONOMY LEDGER                                      │ INSPECTOR                            │
│ openai ✓       │ fixbot-12 bug VE-882   grant agent_task ✓            │ selected: fixbot-12                  │
│ local  ✓       │ releasebot-3 prod gate blocked: approval missing      │ budget $3.12 / $10                   │
│ fallback ✕     │ auditbot-2 killed by policy: stale evidence           │ provider openai key policy redacted  │
│               │                                                      │ actions: pause, budget, kill bell    │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 33.5 Config and runtime profile screen

Settings should be visible and editable through safe forms, with redaction and validation.

Known defaults that should be shown in the runtime profile:

```text
settings path:        ~/.jeryu/settings.json
GitLab HTTP default:  8929
GitLab SSH default:   2224
Vault default:        18200
Webhook/API default:  127.0.0.1:9777
MCP HTTP default:     127.0.0.1:9778
Cache proxy default:  19800
OCI registry mirror:  19801
```

Config sections:

- GitLab URL/token/webhook secret status;
- database backend/path;
- runner pools and scaling;
- remote nodes;
- cache paths/limits/GC policy;
- VTI thresholds;
- release gates;
- secrets/Vault;
- MCP/capability settings;
- action registry profile;
- repo families;
- themes/keymaps;
- demo/capture settings.

Edit flow:

```text
select setting -> edit form -> validate -> diff -> dry-run -> proof -> apply -> audit record -> source refresh
```

Secrets/tokens must display only presence, source, age, scope, and fingerprint.

### 33.6 Capability API contract to preserve

The Unix-socket capability API uses a versioned request envelope. TUI action previews/results should display these fields when relevant:

```text
protocol_version
request_id
actor
nonce
expires_at
project/ref/base/idempotency/budget/grant fields
intent
```

Validation rules:

- protocol version must be supported;
- nonce cannot replay;
- actor/request/nonce must be non-empty;
- expiry must be in the future;
- grants must match risk/side-effect class;
- idempotency key must be stable for retries.

Capability intents include patch proposal, patch racing, test runs, capsule fetch, merge request, blocker explanation, system snapshot, pipeline jobs, CI bottlenecks, allowed-actions listing, plan validation, and bug operations.

### 33.7 Custom executor, sandbox, honeypot, and admission visibility

Expose custom executor lifecycle as first-class events:

```text
exec config -> prepare -> run -> cleanup
```

Show:

- builds/cache dirs;
- driver name/version;
- sandbox path;
- job/project env redacted;
- BuildKit/Cargo/cache env injection;
- Cargo proxy config;
- honeypot tokens/tripwire status;
- stdout/stderr capture;
- failure/quarantine capsules;
- stage execution events;
- produced artifacts.

Admission view should show:

- raw ref update old/new SHA;
- ref name;
- actor kind;
- reasons;
- grant id;
- backup status;
- policy version;
- allowed/audit/denied decision;
- `JERYU_ADMISSION_ENFORCE` status.

### 33.8 CLI surfaces the TUI should mirror or link to

The TUI should not hide the CLI; it should make CLI operations discoverable and show equivalent command strings in action previews.

Top-level CLI groups to mirror:

| CLI group | Data/actions to surface |
|---|---|
| `pool` | list, scale, pause/resume, drain, delete/remove, rotate-token |
| `job` | list, trace, play, cancel, retry/requeue, explain, clear local record |
| `pipeline` | explain, doctor, jobs, ingest, cancel, bottlenecks |
| `cache` | enable, doctor, status, GC |
| `local` | Cargo wrapper/env for proxy/cache |
| `logs` | service/container/job logs |
| `agent` | spawn/list/merge/submit with evidence/draft PR support |
| `test` | run, plan, batch, results, requeue, failed, impact, select/choose, receipts, explain-plan, audit, learn, cache-status |
| `release` | status/watch/reconcile, promote-prod, preflight, doctor, ready, dry-run, submit, approve, rollback |
| `secrets` | provision, status, doctor, rotate, finalize, report, recover |
| `remote` | install/update/doctor/status/logs/restart/stop/start/ssh/run/tunnel/uninstall |
| `node` | add/list/remove/doctor remote Docker nodes |
| `repo` | agent index, surface audit, git hooks, init/adopt/mode/hooks, standard plan/apply/verify, fleet list/status/sync, shadow/backup, state proof, TUI screenshots |
| `bug` | project add/list/link; submit/list/show/triage/link/ready; attempt start/fail/complete |
| `policy` | policy audit |
| `host` | storage-audit, doctor, reclaim, GC timer/service install |
| hidden `exec` | custom executor config/prepare/run/cleanup |
| hidden `server-hook` | Git pre-receive admission |
| hidden `capability` | Unix socket capability server |
| `mcp` | stdio server, HTTP server, tool manifest |
| `action` | action registry list |

### 33.9 Incident mode and time-travel replay

Incident mode freezes the visual layout around the current problem and starts recording the evidence chain.

Incident mode should:

- pin top attention item;
- widen trace/evidence panes;
- record event cursor range;
- track actions taken;
- snapshot relevant config;
- export an incident bundle;
- support replay later.

Time-travel replay:

```text
jeryu tui replay --from-event 184000 --to-event 184293 --speed 4x
```

Replay should render the same screens using event history and fixtures, useful for debugging, demos, and postmortems.

### 33.10 Cost and efficiency

Add cost/efficiency overlays where data exists:

- runner-minutes by repo/family/pool;
- wasted slot-seconds by flake/retry/cache miss/dependency wait;
- cache bytes vs time saved;
- VTI runner-minutes saved vs selector miss risk;
- agent spend vs bugs resolved / time saved;
- release delay cost;
- queue pressure cost by priority.

This can start as an analytics tab/lens rather than a full top-level screen.

## 34. Final design stance

The best JeRyu TUI is not a collection of tabs. It is a **realtime, evidence-backed entity graph** where every repo, job, cache object, test plan, agent task, bug, release gate, artifact, and policy decision is one navigable object in the same universe.

The winning experience is:

```text
Open jeryu tui.
See everything moving.
Know the fleet posture instantly.
Arrow to the suspicious thing.
Enter down.
Enter down again.
Open logs or proof.
Preview a safe action.
Execute with evidence.
Esc back to the fleet.
```

Build for that loop relentlessly.
