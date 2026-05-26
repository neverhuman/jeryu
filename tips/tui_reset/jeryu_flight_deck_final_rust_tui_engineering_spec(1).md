# JeRyu Flight Deck — Final Rust TUI Engineering Specification

**Artifact:** final synthesis of the uploaded `*.txt` source inventories and prior `*.md` TUI design attempts.  
**Target command:** `jeryu tui`  
**Working product name:** **Flight Deck** / **Mission Control**  
**Design goal:** a terminal-native, realtime, keyboard-first Rust cockpit where a developer can see every repo, repo family, CI queue, runner, cache, test-skip decision, agent, bug, release, artifact, security signal, and proof trail from one place, then drill down or act safely in seconds.

This document is meant to be directly actionable by an implementation team. It intentionally includes UX doctrine, screen requirements, data contracts, Rust module design, backend plumbing, safety rules, testing strategy, and a milestone plan.

---

## 0. Executive mandate

JeRyu is not merely a CI dashboard. The uploaded inventories describe a Rust control plane around GitLab CI/CD, runner orchestration, SmartCache, VTI smart test selection, Vault/secrets, release/canary/rollback, local bug tracking, MCP, capability grants, Git/admission hooks, autonomous agents, LLM providers, host/remote node management, and a typed internal TUI read/event/action layer.

The final TUI must therefore be a **realtime operational graph**, not a set of unrelated dashboards. Its three canonical primitives are:

1. **Entities** — repo families, repos, projects, MRs/PRs, pipelines, jobs, stages, runners, pools, remote nodes, cache objects, VTI plans, tests, agents, agent sessions, bugs, bug attempts, releases, release gates, artifacts, signatures, Jankurai audits, security findings, secret authorities, grants, admission decisions, evidence capsules, LLM providers, and system components.
2. **Events** — monotonic, cursor-addressable updates from GitLab webhooks, GitLab REST polling, state DB writes, Docker events, runner reconciliation, SmartCache, Vault, Git hooks, MCP/capability calls, agents, release automation, Jankurai, LLM providers, and user actions.
3. **Actions** — previewable, dry-runnable, risk-tiered operations with side-effect classification, capability grants, idempotency keys, exact SHA binding where relevant, proof receipts, and rollback/undo semantics where possible.

The core operator loop is:

```text
see global truth -> focus the hottest object -> drill down -> inspect evidence -> preview action -> execute safely -> watch proof stream
```

The core navigation loop is:

```text
Fleet / Global -> Repo Family -> Repo -> Workflow / Pipeline -> Job / Trace / Evidence -> Action / Proof
       ^              ^          ^              ^                       ^                   |
       +--------------+----------+--------------+-----------------------+-------------------+
                       Esc always goes up one level without losing context
```

The UI must answer these questions immediately:

- What is happening across all repos and repo families right now?
- Which jobs are queued/running/failing, and which queue limit are we hitting?
- How close are we to the theoretical CI speed limit?
- Is the bottleneck runners, tags, Docker, disk, cache, VTI, serial DAG design, release policy, security, approval, or agents?
- Can I drill into a repo and see the live workflow graph, current progress, logs, failures, artifacts, and evidence?
- Is the cache full, trustworthy, hot, leased, tainted, or wasting space?
- Is VTI actually saving time, or quietly skipping the wrong tests?
- Which agents are working, what grants do they hold, what branches/MRs did they create, what logs/tool calls exist, and are they safe?
- Which bugs exist across repos, which are assigned, which have failed attempts, and which fixes have proof?
- Is Git/local/remote/MR state in sync?
- Are Jankurai, security, secrets, signatures, provenance, release gates, and rollback plans healthy?
- What should I do next, and why is that recommendation safe?

---

## 1. Ground truth from the uploaded archive

### 1.1 Source material studied

The archive contained eight prior TUI spec attempts and nine source-inventory notes:

- `jeryu_dream_rust_tui_engineering_spec.md`
- `jeryu_dream_rust_tui_engineering_spec(1).md`
- `jeryu_dream_rust_tui_engineering_spec(2).md`
- `jeryu_dream_rust_tui_spec.md`
- `jeryu_dream_rust_tui_spec(1).md`
- `jeryu_dream_tui_engineering_spec.md`
- `jeryu_dream_tui_engineering_spec(1).md`
- `jeryu_dream_tui_engineering_spec(2).md`
- `tip1.txt` through `tip9.txt`

The `._*` AppleDouble files in the tarball are metadata sidecars and should be ignored.

### 1.2 Existing JeRyu control surfaces

| Surface | Current entrypoint / transport | What the TUI should use it for |
|---|---|---|
| CLI | `jeryu <command>` | Full operator surface: init/install/serve/remote/node/git/save/sync/undo/system/status/settings/pools/jobs/pipelines/cache/logs/agents/tests/release/secrets/progress/repo/bug/policy/host/next/explain/action. Useful fallback and implementation reference. |
| Existing TUI | `jeryu tui` | Existing terminal control plane, snapshot builders, tabs, panels, live log polling, action registry integration. Preserve useful behavior but rebuild around unified graph. |
| Internal TUI API | `src/api/*` | Typed read model, entity model, event stream, action preview/result, component health, freshness, mission snapshot. This should become the primary backend contract. |
| MCP stdio | `jeryu mcp serve` / `serve-stdio` | JSON-RPC `initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`. Agent-facing tool transport. |
| MCP HTTP | `jeryu mcp serve-http`, default `127.0.0.1:9778` | POST `/mcp`, DELETE `/mcp`; GET currently disabled. Should eventually expose streaming/resources. |
| Capability API | Unix domain socket | Agent intent envelope with actor, nonce, expiry, grant, budget, project/ref/base SHA, idempotency, and intent. Mutating action policy engine. |
| Webhook/API daemon | Axum, default `127.0.0.1:9777` | `GET /health`, `POST /hooks`, `GET /cache/summary`; GitLab Job/Pipeline/Push hooks; MR hooks currently accepted/logged but not acted on. |
| GitLab REST wrapper | Internal `GitlabClient` | Projects, jobs, traces, artifacts, pipelines, bridges/downstream pipelines, variables, runners, runner managers, issues, MRs, branches, protected branches, retries, cancel, play. |
| Message log / broker | Kafka or Jansu feature-gated | Topics `jeryu.webhook.jobs`, `jeryu.webhook.pipelines`, `jeryu.webhook.pushes`. Needed for realtime event ingestion and replay. |
| Git server hook | `jeryu server-hook pre-receive` | Admission decisions, protected ref policy, actor kind, grants, old/new SHA, allow/audit/deny. |
| Custom executor | `jeryu exec config/prepare/run/cleanup` | Runner lifecycle, sandbox state, job env, stdout/stderr, failure/quarantine capsules, stage execution events. |
| SmartCache / gateway | Proxy default `19800`, OCI mirror default `19801` | Cargo sparse config, crate downloads, CAS hits, singleflight, cache objects/requests, taints, verdicts, leases, promotions, hot entries, disk pressure. |
| Docker / runner plane | Bollard, Docker Compose, remote nodes | Managed runner containers, labels, logs, OOM/death events, lifecycle, remote manager reconciliation, node storage reports. |
| Vault / secrets | Vault HTTP + state DB | Vault health, init/unseal, KV v2, policies, release secret sets, rotation/finalization/recovery reports, redacted audit metadata. |
| State DB | SQLite default, RedlineDB optional | Durable source for pools, managers, job events, tracked pipelines, releases, evidence, retry decisions, tests/VTI, cache, grants, admission, Git events, secrets, bugs, autonomy, LLM budget. |
| Autonomy binary | `autonomy` CLI/server | Evidence Gate/VibeGate, kill bell, freeze windows, launch ledger, verdicts, foundry candidates, canary/nightwatch, rollback drills, `/metrics`, `/health`, `/events`. |
| GitHost abstraction | GitHub/GitLab adapters | PR/MR state, checks, comments, approvals, diffs, target policy SHA, merge passport. |
| LLM provider plane | OpenAI-compatible providers | Provider health, key source, token usage, latency, cost estimate, model routing, secret scrub status, failures. |
| Jankurai | `repo jankurai-fast` and audit outputs | Code audit score, duplicate clusters, caps, security/provenance/release/TUI/testing/rot findings, score trends, evidence receipts. |

### 1.3 Current MCP tools

Current source-derived inventory says there are **16 MCP tools**, all under the `jeryu.` prefix.

| MCP tool | Kind | Required/important args | TUI purpose |
|---|---:|---|---|
| `jeryu.fetch_capsule` | read | `job_id` | Fetch latest failure/evidence capsule for a job. |
| `jeryu.get_system_snapshot` | read | none | Seed global mission/health snapshot. |
| `jeryu.get_pipeline_jobs` | read | `project_id`, `pipeline_id` | Pipeline/job list, including downstream-expanded jobs. |
| `jeryu.get_ci_bottlenecks` | read | `project_id`, optional `ref_name`, optional `limit` | Historical timing bottleneck analysis. |
| `jeryu.explain_blockers` | read | `entity_type`, `entity_id` | Explain job/release/merge blockers. |
| `jeryu.plan_validation` | read | `project_id`, `test_ids[]`, `ref_name` | Validate VTI plan against selector-miss history. |
| `jeryu.run_tests` | mutate | `project_id`, `target_ref`, `test_scope` | Create targeted test branch/pipeline. |
| `jeryu.propose_patch` | mutate | project, branch, base ref, message, modifications | Create branch, commit files, open MR, record grant. |
| `jeryu.race_patches` | mutate | project, base branch, message, hypotheses | Launch multiple hypothesis branches/pipelines. |
| `jeryu.request_merge` | high-risk mutate | project, MR IID, source/target branch | Merge request path; TUI must proof-gate this. |
| `jeryu.bug_submit` | local mutate | canonical bug report | Create local bug. |
| `jeryu.bug_list` | read | optional project/status/sort | Bug board. |
| `jeryu.bug_show` | read | `bug_id` | Bug detail, events, attempts. |
| `jeryu.bug_ready` | read | optional project | Ready/unblocked bug queue. |
| `jeryu.bug_update` | local mutate | bug id and triage fields | Update status/severity/priority/component/owner. |
| `jeryu.bug_record_attempt` | local mutate | bug id and attempt | Append agent/human attempt history. |

Capability intents are those 16 plus `ListAllowedActions`. `run_tests.test_scope` values include `unit`, `integration`, `lint`, and `full`.

### 1.4 Current HTTP routes and defaults

Current main daemon routes:

| Method | Path | Auth | Current behavior |
|---|---|---|---|
| `GET` | `/health` | none | Returns basic `ok`. Must be expanded to deep health. |
| `POST` | `/hooks` | `X-Gitlab-Token` | Consumes GitLab webhook body and dispatches Job/Pipeline/Push events through broker; MR accepted/logged only. |
| `GET` | `/cache/summary` | `X-Jeryu-Token` | Returns `bytes_served`, `hits`, `objects`, `status`; needs richer cache APIs. |

Current MCP HTTP route:

| Method | Path | Current behavior |
|---|---|---|
| `POST` | `/mcp` | JSON-RPC call, no batches. |
| `DELETE` | `/mcp` | Delete session. |
| `GET` | `/mcp` | Explicitly disabled with `405`; streaming/resources are missing. |

Important defaults surfaced by the inventory:

| Setting | Default / note |
|---|---|
| GitLab HTTP | `8929` |
| GitLab SSH | `2224` |
| Vault | `18200` |
| JeRyu webhook/API | `127.0.0.1:9777` |
| MCP HTTP | `127.0.0.1:9778` |
| SmartCache proxy | `19800` |
| OCI registry mirror | `19801` |
| Cache budget | about `400 GiB` in inventory examples |
| TUI sync interval | about `5000 ms` in inventory examples |
| Live trace polling | about `650 ms` where streaming is absent |
| State backend | SQLite default, RedlineDB opt-in behind feature/profile |

### 1.5 Durable state data families

The final TUI should treat the state store as the broadest local source of truth.

| Domain | Existing / expected tables and records |
|---|---|
| Runner / CI / release | `pools`, `managers`, `job_events`, `ci_job_runs`, `tracked_pipelines`, `tracked_repositories`, `release_attempts` |
| Capability / admission / Git audit | `capability_intents`, `capability_grants`, `admission_decisions`, `git_command_events`, `git_ref_updates`, `git_mirror_jobs`, `git_risk_approvals`, `git_command_artifacts`, `events` |
| Evidence / retry / VTI / tests | `evidence_capsules`, `retry_decisions`, `test_executions`, `test_plans`, `test_plan_items`, `selector_misses` |
| Cache / provenance | `cache_objects`, `cache_requests`, `hot_cache_entries`, `build_signatures`, `image_signatures`, `force_refresh_rules`, `resolved_refs`, `cache_taints`, `cache_leases`, `cache_verdicts`, `cache_promotions`, `material_objects`, `material_aliases`, `action_cache`, `cache_epochs`, `toolchain_fingerprints` |
| Secrets / Vault | `secret_authorities`, `release_secret_sets`, `secret_audit_events` |
| Bug tracker | `bug_projects`, `bug_project_edges`, `bugs`, `bug_events`, `bug_attempts`, `bug_links`, `bug_external_refs`, `bug_evidence` |
| Autonomy / Evidence Gate | `launch_ledger`, `kill_bell_state`, `verdicts`, `foundry_candidates`, `llm_budget_ledger` |

### 1.6 Known current limitations the TUI must fix or label

The uploaded docs converge on these weaknesses:

| Current limitation / drift | Final TUI treatment |
|---|---|
| No WebSocket/SSE transport for TUI event stream yet. | Implement streaming endpoint; until then, visibly mark poll mode and freshness. |
| Live logs are polling-based. | Add bounded log stream; keep polling fallback with staleness indicator. |
| Existing Flow Board renders only the first active pipeline. | Multi-pipeline, multi-repo flow board is required. |
| Pipeline graph edges are not fully computed. | Build graph from `needs`, bridges, child pipelines, artifacts, stage fallback; label inferred edges. |
| ETA is heuristic. | Show ETA confidence and source; distinguish historical/model/projection values. |
| Evidence is useful but not searchable proof timeline. | Build universal proof ledger query and timeline. |
| Agents tab lacks dedicated lifecycle table. | Add agent session/task/step/message/artifact data model. |
| MR hooks are accepted/logged but not acted on. | Label MR state as partial until ingestion exists; add MR ingestion as P0/P1. |
| Older docs undercount MCP tools. | Generate docs/manifests from source/action registry. |
| `/cache/summary` auth docs drift. | Show API auth posture and drift warning in Source Doctor. |
| RedlineDB-only docs are stale. | Show actual DB backend/feature profile. |
| `ListAllowedActions` appears stale relative to registry. | Generate it from the action registry; alert on mismatch. |
| `request_merge` appears more direct than the docs imply. | TUI must enforce merge proof gate before invocation. |

---

## 2. Product doctrine

### 2.1 One object model everywhere

Every visible thing is an addressable entity. Tables, charts, graphs, logs, event rows, and alerts all carry `EntityRef`. `Enter` drills in, `Esc` drills out, `a` opens actions, `e` opens evidence, `l` opens logs, `b` explains blockers, `t` opens timeline.

No panel should contain inert text when it could link to proof.

### 2.2 Stream-first; poll honestly as fallback

The dream experience is live. Jobs pulse, logs stream, queue pressure updates, cache hits tick, runner slots shift, agents advance steps, and release gates change without manual refresh.

When streaming is unavailable, the UI must say so:

```text
stream:poll 650ms  source:GitLab  freshness:1.7s  cursor:unknown  confidence:medium
```

### 2.3 Truth cockpit, not log wall

Logs are essential but not sufficient. The UI should summarize, classify, and link:

- first failure line;
- failure capsule;
- suspected root cause;
- retry/quarantine decision;
- artifact/test report references;
- cache verdicts;
- VTI receipt;
- release gate evidence;
- related bug or agent attempt.

### 2.4 Every warning explains itself

Warnings must answer:

1. What is wrong?
2. Which entity is affected?
3. What evidence proves it?
4. What are the next safe actions?
5. How stale is the data?

### 2.5 Keyboard-first, mouse-comfortable

Arrow keys and tabs should feel like driving a fast game UI. Mouse support is welcome but never required.

### 2.6 Safety before speed for mutations

Read paths should be instant. Mutating paths must be explicit. The TUI should never make production, merge, release, secret, destructive cache, or autonomous config changes without a preview/proof modal.

### 2.7 No blank screens

If a backend is missing, show what is missing, how to enable it, what fallback is being used, and what data is stale. Do not show empty panes without diagnosis.

### 2.8 Human trust beats animation

The UI can be colorful and alive, but motion must never hide uncertainty. Every animated indicator needs a text/glyph fallback and low-motion mode.

---

## 3. Operating model and information architecture

### 3.1 Five nested levels

| Level | Name | Purpose | Example route |
|---:|---|---|---|
| 1 | Fleet | All repo families, global queue, runners, agents, releases, risks. | `/global` |
| 2 | Repo family | Shared family rollup such as `veox-*` or isolated groups. | `/family/veox` |
| 3 | Repo | One repo’s current CI, Git, cache, VTI, issues, release state. | `/repo/veox-api` |
| 4 | Workflow / entity family | Pipeline DAG, bug board, cache objects, agent sessions, release train. | `/repo/veox-api/pipeline/123` |
| 5 | Entity detail | Job trace, evidence capsule, cache object, bug attempt, grant, artifact. | `/job/92341` |

### 3.2 Top-level tabs

The top-level tab order should optimize for the user’s stated priorities and the natural debug flow.

| Key | Tab | Primary question |
|---:|---|---|
| `0` | **Global** | What is happening everywhere right now? |
| `1` | **Queue** | How close are we to the theoretical CI limit and what is slowing us? |
| `2` | **Repos** | Which repo families/repos are healthy, hot, blocked, or isolated? |
| `3` | **Workflow** | What is running, what is next, what is blocked, and what is the critical path? |
| `4` | **Runners** | Are pools/nodes/slots healthy and efficiently used? |
| `5` | **Cache** | Is cache fast, full, trustworthy, and worth its cost? |
| `6` | **VTI/Tests** | Is smart test skipping saving time safely? |
| `7` | **Agents** | Which agents/workflows are active, blocked, expensive, or risky? |
| `8` | **Bugs** | What bugs exist, who/what is working them, and what evidence exists? |
| `9` | **Git** | Are local/remote/MR/PR/ref/admission states in sync? |
| `q` | **Jankurai** | What quality/audit findings and score caps exist? |
| `w` | **Security** | What vulnerabilities, secret risks, grants, policies, and scans matter? |
| `e` | **Artifacts** | Are artifacts signed, reproducible, SBOMed, and provenance-bound? |
| `r` | **Release** | Can we release/promote/rollback safely? |
| `t` | **Evidence** | What proof exists across the whole system? |
| `y` | **Doctor** | Are data sources, API/MCP, DB, workers, settings, and docs healthy? |

The tab bar should show short labels and status badges, not long names, on narrow terminals:

```text
0 Global  1 Queue  2 Repos  3 Flow  4 Run  5 Cache  6 VTI  7 Agents  8 Bugs  9 Git  q Quality  w Sec  e Art  r Rel  t Proof  y Doctor
```

### 3.3 Scope stack

Everything operates within a current scope:

```rust
pub enum Scope {
    Fleet,
    RepoFamily { family_id: RepoFamilyId },
    Repo { repo_id: RepoId },
    Project { project_id: i64 },
    Pipeline { project_id: i64, pipeline_id: i64 },
    Entity(EntityRef),
}
```

The scope stack drives breadcrumbs and filters:

```text
Fleet › veox-* › veox-api › pipeline #8842 › job test:integration
```

### 3.4 Route stack

Routes are reversible. `Esc` pops one route without losing selection state.

```rust
pub enum Route {
    Global,
    Queue { scope: Scope },
    Repos { selected_family: Option<RepoFamilyId> },
    Repo { repo_id: RepoId, tab: RepoTab },
    Workflow { repo_id: RepoId, pipeline_id: Option<i64>, mode: WorkflowMode },
    Job { project_id: i64, job_id: i64, tab: JobTab },
    Cache { scope: Scope, filter: CacheFilter },
    Vti { scope: Scope },
    Agents { scope: Scope, selected: Option<AgentId> },
    Bugs { scope: Scope, lane: BugLane },
    Git { scope: Scope },
    Jankurai { scope: Scope },
    Security { scope: Scope },
    Artifacts { scope: Scope },
    Release { scope: Scope, release_id: Option<String> },
    Evidence { query: ProofQuery },
    Doctor { tab: DoctorTab },
    EntityDetail { entity: EntityRef },
    ActionPreview { action: ActionDescriptor, target: EntityRef },
}
```

---

## 4. Global shell layout and visual language

### 4.1 Responsive breakpoints

| Width | Layout |
|---:|---|
| `<100` | One focused pane, compact header, hidden side rails, full keyboard support. |
| `100–139` | Header + tabs + main pane + bottom event/action strip. |
| `140–179` | Header + tabs + left attention rail + main pane + bottom detail/log strip. |
| `180+` | Header + tabs + left nav/attention + center workspace + right inspector + bottom event/log/action strip. |

Height behavior:

| Height | Behavior |
|---:|---|
| `<28` | Focus mode; hide secondary charts; keep header/breadcrumb/footer. |
| `28–39` | Compact charts, one detail rail, virtualized tables. |
| `40+` | Full dashboard density, bottom log/event tape. |

### 4.2 Persistent regions

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ HEADER: profile • DB • GitLab • event cursor • queue • cap • runners • cache • VTI • sec • rel • fresh │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ TABS: 0 Global 1 Queue 2 Repos 3 Flow 4 Run 5 Cache 6 VTI 7 Agents 8 Bugs 9 Git q Quality ...          │
├──────────────────┬──────────────────────────────────────────────────────────────┬──────────────────────┤
│ LEFT             │ CENTER                                                       │ RIGHT                │
│ attention queue  │ selected dashboard/table/DAG/log/diff/replay                 │ inspector/actions     │
│ scope nav        │                                                              │ evidence/blockers     │
├──────────────────┴──────────────────────────────────────────────────────────────┴──────────────────────┤
│ BOTTOM: hotkeys • breadcrumbs • stream status • event tape • command hint • frame/source diagnostics     │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 4.3 Header contract

Example wide header:

```text
JERYU FLIGHT DECK  prod  db:sqlite/wal  gitlab:ok  event:184923↑  queue:84/112  cap:91%  runners:28/32  cache:71%  vti:+4.2h  agents:12/2b  sec:2C/9H  rel:v2.8.1 canary  fresh:0.8s
```

Header fields are drillable:

| Field | Meaning | Drill route |
|---|---|---|
| profile | local/prod/staging/effective settings profile | Doctor › Runtime Profile |
| db | SQLite/RedlineDB backend, migration state, latency | Doctor › DB Inspector |
| gitlab | readiness, API latency, rate limits | Doctor › Sources |
| event | latest event cursor and stream direction | Evidence/Event Ledger |
| queue | runnable/total queued jobs | Queue |
| cap | speed-limit pressure / SCREAM index | Queue › Capacity Lab |
| runners | busy/usable/theoretical slots | Runners |
| cache | disk/object pressure and hit ratio | Cache |
| vti | time saved / misses / confidence | VTI |
| agents | active/blocked agents | Agents |
| sec | critical/high findings | Security |
| rel | latest release/canary/prod state | Release |
| fresh | worst source freshness | Doctor › Source Freshness |

### 4.4 Bottom strip

Normal mode:

```text
[↑↓] move [←→] panes [Tab] tabs [Enter] drill [Esc] back [a] actions [l] logs [e] evidence [/] search [:] command [?] help  stream:sse ok  frame:7ms
```

Action-select mode:

```text
Action: retry job #92341  risk:R2  dry-run:yes  grant:none  [p] preview [Enter] execute [Esc] cancel
```

Degraded mode:

```text
DEGRADED: GitLab stale 12.4s • MCP resources missing • log stream polling 650ms • MR hook partial  [?] why  [d] doctor
```

### 4.5 Semantic palette

Use 24-bit color when available, 256-color fallback, monochrome fallback. Status must not depend on color alone.

| Semantic | Color family | Glyph | Meaning |
|---|---|---|---|
| healthy/success | green | `✓` | Complete, safe, passing. |
| running/live | cyan/blue | `▶` / spinner | Active and progressing. |
| queued/waiting | yellow | `…` | Waiting but not failing. |
| warning/degraded | amber | `!` | Needs attention, not immediately fatal. |
| failed/blocked | red | `✗` / `⊘` | Failed, blocked, unsafe. |
| risky/security | magenta/red | `◆` / `⚠` | Security, policy, proof, or prod risk. |
| stale/unknown | gray | `?` / `~` | Missing, stale, inferred, or unknown. |
| agent/autonomy | violet | `🤖` fallback `A` | Autonomous action or agent session. |
| evidence/proof | teal | `◈` | Receipt/proof/evidence object. |
| cache/trust | green/amber/red by verdict | `◇` | Cache hit/miss/taint/verdict. |

### 4.6 Status glyphs

| Status | Glyph | Text fallback |
|---|---|---|
| success | `✓` | `OK` |
| running | `▶` | `RUN` |
| pending | `…` | `WAIT` |
| skipped | `↷` | `SKIP` |
| manual | `◌` | `MANUAL` |
| blocked | `⊘` | `BLOCK` |
| failed | `✗` | `FAIL` |
| canceled | `×` | `CANCEL` |
| retried | `↻` | `RETRY` |
| stale | `~` | `STALE` |
| inferred | `≈` | `INFER` |
| proof | `◈` | `PROOF` |
| grant | `⚿` | `GRANT` |
| secret redacted | `•••` | `REDACTED` |
| production | `⬢` | `PROD` |
| rollback | `↩` | `ROLLBACK` |

### 4.7 Progress bars

Use layered bars with text labels:

```text
pipeline  61%  ██████████████░░░░░░░░░  18/31 jobs  crit: test:integration  ETA 7m40s±2m
queue     91%  ███████████████████░░░   usable 142/164  loss: tag 5, disk 2, unhealthy 7
cache     71%  ███████████████░░░░░░░   284/400GiB  hot 62GiB  leased 33GiB  tainted 4GiB
vti       84%  ████████████████░░░░░    confidence 0.84  skip 1,204  select 182  miss 3
```

Never animate progress without also showing numeric values.

### 4.8 Motion and “moving activity” rules

The UI should feel alive:

- live event tape scrolling at bottom;
- runner slot mini-map pulsing on starts/finishes;
- pipeline DAG nodes ticking through queued/running/success/fail states;
- selected job trace streaming with highlighted new chunks;
- cache hit/miss counters incrementing;
- VTI selected/skipped counters updating;
- agent step timeline advancing;
- release gates lighting up as proofs arrive;
- sparklines for queue pressure, cache hit ratio, selector misses, runner saturation.

But motion must be controlled:

- coalesce updates to avoid flicker;
- default max render cadence 30 FPS, lower when idle;
- disable animations in `--low-motion`;
- pause background motion when an action proof modal is open;
- never move selected rows unexpectedly.

---

## 5. Keyboard, focus, search, and command model

### 5.1 Universal keys

| Key | Action |
|---|---|
| `Tab` / `Shift-Tab` | Switch top-level tab or pane focus depending mode. |
| `←` / `→` | Move pane focus; in DAG, move to graph neighbor. |
| `↑` / `↓` | Move row/node selection. |
| `Enter` | Drill into selected entity. |
| `Esc` | Pop route stack / close modal / go up one level. |
| `Backspace` | Alternate go-up for terminals with Esc delay. |
| `/` | Search/filter current pane. |
| `Ctrl-/` | Global search. |
| `:` | Command palette. |
| `?` | Contextual help. |
| `a` | Action menu for selected entity. |
| `p` | Preview selected action/proof. |
| `e` | Evidence for selected entity. |
| `l` | Logs/trace for selected entity. |
| `t` | Timeline. |
| `b` | Explain blockers. |
| `f` | Filter menu. |
| `s` | Sort menu or simulator depending screen. |
| `r` | Refresh now. |
| `R` | Reconcile/reload source; proof-gated if mutating. |
| `Space` | Pin/unpin selected entity in inspector. |
| `m` | Mark/bookmark. |
| `1`–`9` | Quick switch to visible tabs/actions. |
| `Ctrl-k` | Kill bell / autonomy pause overlay. |
| `Ctrl-c` | Quit or cancel active streaming action. |

### 5.2 Expert quick actions

| Key | Context | Action |
|---|---|---|
| `x` | job/pipeline | Cancel selected job/pipeline, preview if broad. |
| `y` | job | Retry/requeue job. |
| `o` | job/MR/artifact/path | Open URL/path externally. |
| `L` | log | Follow/unfollow live tail. |
| `E` | log/failure | Create/open evidence capsule. |
| `P` | pipeline/release | Pin critical path. |
| `S` | pool/runner/node | Scale runner pool preview. |
| `D` | pool/runner/node | Drain preview. |
| `M` | MR/release | Merge/promote proof modal. |
| `B` | release | Rollback proof modal. |
| `C` | cache | Cache GC preview. |
| `V` | tests | Validate VTI plan. |
| `J` | repo | Run/open Jankurai audit. |
| `A` | bug/agent | Assign/spawn agent proof modal. |
| `!` | global | Incident mode / emergency overlay. |

### 5.3 Command palette

Command palette opens with `:` and is fuzzy-searchable. It should combine commands, entities, filters, and actions.

Examples:

```text
:repo veox-api
:family veox
:job 92341 logs
:why not green
:queue simulate +4 rust-large
:cache gc preview family:veox
:vti explain plan veox-core main
:agent pause agent-red-17
:bug ready family:veox
:jankurai run veox-api
:release rollback preview veox-api
:evidence sha:a19c88f
:doctor mcp
```

Palette result row:

```text
[Action] retry job #92341       risk:R2  dry-run:yes   target:veox-api/test:integration
[Entity] veox-api pipeline #8842 running 61% fresh:0.8s
[Proof] release gate canary-e2e failed sha:a19c88f
[Route] Cache Observatory scope:veox-* filter:tainted
```

### 5.4 Filter syntax

Filters should work everywhere:

```text
repo:veox-api status:failed stage:test branch:main age:<30m
family:veox kind:job status:queued pool:rust-large
cache:tainted category:cargo size:>1GiB hot:false
vti:low-confidence miss:true repo:veox-core
agent:blocked grant:expired risk:>=R3
bug:ready severity:high attempts:0
proof:release sha:a19c88f actor:agent-red-17
```

### 5.5 Saved lenses

A lens is a saved view/filter/sort/scope layout.

Examples:

- `Fleet Fire`: failed jobs, failed releases, critical/high security, blocked agents.
- `VTI Risk`: low-confidence plans, selector misses, escalations.
- `Cache Waste`: cold large objects, tainted entries, reclaimable bytes.
- `Agent Accountability`: active tasks, grants, token cost, failed attempts.
- `Release Proof`: release gates, artifacts, signatures, canary telemetry.

---

## 6. Core screens

## 6.1 Global Flight Deck

### Purpose

The default page is the entire fleet in motion. It must show repo family health, live queue, theoretical-limit pressure, hot workflows, top blockers, active agents, release/security/cache/VTI posture, and an event tape.

### Mock

```text
┌ JERYU FLIGHT DECK ─ prod ─ db:sqlite ✓ ─ gitlab:ok 212ms ─ event:184923↑ ─ fresh:0.8s ───────────────┐
│ queue 84/112  cap 91% SCREAM 87  runners 28/32  cache 71% hit 83%  VTI +4.2h miss 3  sec 2C/9H rel canary │
├ FAMILIES ─────────────┬ LIVE AIR TRAFFIC / CRITICAL PATH ───────────────────────────────┬ ATTENTION ───────┤
│ veox-*        ● 18 repos│ veox-api     pipeline #8842  ███████████░░ 61%  crit test:int │ 1 ✗ release gate │
│  run 42 q 61 fail 3    │   build ✓  lint ✓  unit ▶  int ▶  package …  canary ⊘         │   veox-api canary │
│  cap 94% cache 78%     │ veox-core    #8841  ████████░░░░ 45%  runner wait rust-large  │   e2e proof stale │
│ redlinedb     ● 4 repos│ redlinedb    #2219  ✗ test:sqlite-compat  first fail line 812  │ 2 ⚠ queue pool    │
│  run 8 q 2 fail 1      │ deploy       #771   ✓ green  release candidate ready           │   rust-large p95  │
│ isolated      ○ 9 repos├ RUNNER / CACHE / AGENT PULSE ──────────────────────────────────┤ 3 ⚠ VTI miss     │
│  run 3 q 0 fail 0      │ rust-large  ██████████ 100% q42  p95 wait 7m   node-3 disk 91%│   auth mapping    │
│ experiments   ~ stale  │ cache       ███████░░ 71%  hot 62G leased 33G tainted 4G       │ 4 🤖 agent stuck  │
│                        │ agents      12 active 2 blocked 1 awaiting grant  spend $4.82  │   grant expired   │
├ EVENT TAPE ──────────────────────────────────────────────────────────────────────────────────────────┤
│ 09:42:18 job veox-api/test:int started on runner rust-lg-14 • 09:42:19 cache miss crate serde@1.0... │
└ [Enter] drill  [1] queue  [/] filter  [:] command  [a] action  [e] evidence  [?] help ───────────────┘
```

### Required panels

1. **Header posture rail** — safe-to-code, safe-to-merge, safe-to-release, safe-to-rollback, autonomy state, source freshness, worst blocker.
2. **Repo family pane** — grouped families, totals, queue/failure counts, cache/VTI/release/security badges, family trends.
3. **Live air-traffic board** — hot pipelines across repos, critical path, progress, blocker, ETA confidence.
4. **Runner/cache/agent pulse** — live slots, disk/cache pressure, active/blocked agents, LLM spend.
5. **Attention rail** — ranked list of the most important actionable issues.
6. **Event tape** — latest cross-system events, cursor, source, severity.

### Ranking function for attention rail

```text
attention_score = severity_weight
                + production_weight
                + blocked_critical_path_weight
                + queue_impact_weight
                + security_weight
                + stale_data_weight
                + agent_unblocking_weight
                + user_pinned_weight
                - acknowledged_decay
```

Every attention item must carry:

- entity ref;
- severity;
- summary;
- source freshness;
- evidence links;
- recommended next action;
- “why ranked here” explanation.

### Global actions

- Open queue/capacity lab.
- Open top blocker proof.
- Cancel superseded pipelines preview.
- Scale recommended pool preview.
- Pause/resume autonomy through kill bell overlay.
- Export incident snapshot.
- Start replay from current event cursor.
- Switch scope to family/repo.

---

## 6.2 Queue and Theoretical Limit Lab

### Purpose

The Queue page answers: “How close am I to the theoretical limit, and what exactly prevents faster green?”

### Three-limit model

For each fleet/family/repo scope, compute three lower bounds:

1. **Physics limit** — lower bound from DAG critical path using best historical durations and zero queue delay.
2. **Fleet limit** — lower bound under actual pools, runners, nodes, tags, trust tiers, cold starts, remote affinity, disk pressure, and cache state.
3. **Policy limit** — lower bound after non-bypassable approvals, release gates, freeze windows, canary minimums, security checks, artifact signing, and secret rotation.

Definitions:

```text
D_best(j)    = p10 historical duration for same job/stage/ref class with hot cache
D_p50(j)     = median historical duration
D_current(j) = observed elapsed/completed duration
Deps(j)      = needs/stage/artifact/child-pipeline dependencies
Pool(j)      = eligible runner pools/tags/trust tiers
Risk(j)      = required gates/security/release policy
```

Physics bound:

```text
physics_eta = longest_path(D_best, DAG_deps)
physics_efficiency = physics_eta / max(actual_or_predicted_wall_clock, 1s)
```

Fleet bound:

```text
fleet_eta = simulate_schedule(
  jobs        = queued + ready + running + pending,
  durations   = D_p50 adjusted by cache state,
  resources   = runner_slots_by_pool_node_tag,
  cold_start  = p50 manager startup,
  constraints = deps + tags + trust_tier + remote_affinity + disk_pressure
)
fleet_efficiency = fleet_eta / max(actual_or_predicted_wall_clock, 1s)
```

Policy bound:

```text
policy_eta = fleet_eta
           + unavoidable_gate_waits
           + release_canary_min_duration
           + required_human_approval_sla_remaining
           + freeze_window_remaining_if_applicable
policy_efficiency = policy_eta / max(actual_or_predicted_wall_clock, 1s)
```

SCREAM index:

```text
scream = clamp(100 * weighted_mean([
  policy_efficiency,              .30,
  useful_runner_utilization,      .20,
  non_obsolete_work_ratio,        .15,
  cache_health_score,             .10,
  vti_confidence_score,           .10,
  source_freshness_score,         .10,
  blocker_resolution_score,       .05
]), 0, 100)
```

### Capacity calculation

```text
theoretical_slots(pool) = min(
  pool.max_managers * pool.runner_concurrency,
  pool.request_concurrency_limit,
  remote_node_available_slots(pool),
  gitlab_runner_limit(pool),
  optional_global_cap
)

usable_slots = theoretical_slots
             - paused_slots
             - unhealthy_slots
             - incompatible_tag_slots
             - reserved_release_slots
             - trust_tier_blocked_slots
             - disk_pressure_blocked_slots
             - image_pull_backoff_slots
```

### Mock

```text
┌ QUEUE / THEORETICAL LIMIT ─ scope:fleet ─ SCREAM 87 ───────────────────────────────────────────────────────┐
│ Lower bounds: physics 18m12s  fleet 27m44s  policy 41m10s  current projection 46m22s  gap +5m12s          │
├ SLOTS BY POOL ─────────────────────────────────────────────────────────────────────────────────────────────┤
│ Pool          usable/theory busy queued unsched p95 wait  loss reason                   action             │
│ rust-large    32/40         32   42     0       7m42s     unhealthy 4, disk 2, tag 2    +4 slots helps 5m │
│ docker-med    58/64         49   12     3       1m10s     trust mismatch 3              retag jobs        │
│ macos         4/4           4    9      0       22m       hard physical limit            add node         │
│ release       6/8           2    0      0       0s        reserved capacity              ok               │
├ LOSS DECOMPOSITION ────────────────────────────────────────────────────────────────────────────────────────┤
│ queue tax 9m32s ████████░  cold-start 2m10s ██░  cache miss 3m55s ███░  serial DAG 10m44s █████████░       │
│ obsolete work 1m42s ░  VTI fallback 4m20s ████░  policy gates 13m26s ███████████░                         │
├ RECOMMENDATIONS ───────────────────────────────────────────────────────────────────────────────────────────┤
│ 1. Split veox-core/test:integration; removes 7m from critical path; confidence .74; risk R3                │
│ 2. Scale rust-large +4 on node-2; saves 5m; avoid node-3 disk 91%; risk R2                                  │
│ 3. Cancel 18 superseded pipelines; saves 38 slot-min; risk R2                                               │
│ 4. Fix VTI auth selector miss; saves estimated 4m/run; risk R1                                              │
└ [s] simulate  [a] action  [Enter] drill  [e] proof  [x] cancel superseded preview ─────────────────────────┘
```

### Bottleneck taxonomy

| Bottleneck | Signals | Safe action suggestions |
|---|---|---|
| Runner scarcity | ready jobs > free eligible runners; high p95 queue wait | Scale pool, add node, prewarm managers. |
| Tag/trust fragmentation | idle runners but unschedulable jobs | Retag jobs/pools, adjust trust tiers, route isolated jobs. |
| Cold starts | many pending managers; image pull latency | Increase warm managers, pre-pull images. |
| Serial DAG | long critical path with low parallelism | Split job, add `needs`, shard tests. |
| Cache miss storm | hit ratio drop, upstream latency, high bytes served | Inspect misses, prewarm, fix force-refresh/taints. |
| VTI fallback | low confidence, full test escalation | Learn mappings, repair selector misses. |
| Obsolete work | superseded pipelines still running | Cancel superseded pipelines. |
| Release policy | canary/approval/freeze/security wait | Show proof; no runner scaling recommendation. |
| Remote node pressure | CPU/mem/disk/network/SSH degraded | Rebalance, GC, add node. |
| GitLab/API/broker lag | stale webhooks/events, rate limits | Doctor, retry, fall back to polling. |

### Simulator

Queue simulator should support interactive “what if”:

- add/remove slots by pool;
- pause/unpause pools;
- change warm manager count;
- cancel superseded jobs;
- force cache prewarm;
- split/shard a job;
- mark VTI mapping repaired;
- remove release gate waits.

Each simulation must show projected wall-clock, cost, risk, confidence, and the new limiting bottleneck.

---

## 6.3 Repo Families and Repo Atlas

### Purpose

The Repos screen manages many repos, including shared families like `veox-*` and isolated repos. It should make cross-repo state obvious without losing drilldown speed.

### Family grouping rules

Repo families come from:

1. explicit config;
2. prefix/glob rules like `veox-*`;
3. GitLab groups/namespaces;
4. runner pool sharing;
5. release group;
6. dependency graph;
7. custom tags.

Suggested config:

```toml
[[repo_families]]
id = "veox"
label = "veox-*"
match = ["veox-*"]
shared_runners = ["rust-large", "docker-medium"]
release_group = "veox"

[[repo_families]]
id = "isolated"
label = "isolated"
match = ["redlinedb", "jeryu", "experiments/*"]
shared_runners = []
```

### Mock

```text
┌ REPO ATLAS ─ families:5 repos:73 ─ selected:veox-* ───────────────────────────────────────────────────────┐
│ Family        Repos Run Que Fail Rel Sec VTI Cache Cap  Bugs Agents  Trend         Top blocker            │
│ veox-*        18    42  61  3    ⚠   2H  84  78%   94%  37   9/2b    ▅▆▇▇▆▇     release e2e stale       │
│ redlinedb     4     8   2   1    ✓   0   91  64%   52%  11   1/0     ▂▃▂▆       sqlite compat fail      │
│ deploy        6     3   0   0    ✓   1M  88  82%   31%  4    0/0     ▂▂▁        none                    │
│ isolated      9     2   0   0    —   0   74  23%   18%  8    2/0     ▁▁▂        Jankurai stale          │
├ REPOS IN veox-* ───────────────────────────────────────────────────────────────────────────────────────────┤
│ Repo        Branch SHA      Pipeline Progress  Queue  Crit path       Cache VTI  Bugs Agents Release Sec │
│ veox-api    main   a19c88f  #8842    61%       2m10s  test:int 7m     82%   87   8    2/1b   canary  1H  │
│ veox-core   main   77ab013  #8841    45%       7m42s  rust-large wait 76%   71   14   4/0    n/a     0   │
│ veox-web    main   3dd91aa  #8810    failed    0s     build failed    69%   79   5    1/0    blocked 1C  │
└ [Enter] repo  [f] family filter  [c] compare  [g] graph deps  [a] family actions ─────────────────────────┘
```

### Required rollups

Family rows must include:

- repo count;
- active/running/queued/failed job counts;
- capacity and queue pressure by shared pools;
- cache usage and hit ratio attributable to family;
- VTI selected/skipped/escalated/miss totals;
- active agents and blocked agents;
- bug status counts;
- release readiness;
- security critical/high findings;
- Jankurai average score and stale/missing audits;
- last event time and freshness.

### Repo row fields

Each repo row should include:

- repo id/name/family/provider/project id;
- branch/ref/head SHA;
- pipeline id/status/progress/ETA;
- queue wait / stage / critical path;
- latest failure capsule;
- cache hit ratio / miss storm / taints;
- VTI confidence / selector misses;
- agent sessions / blocked grants;
- bug counts by lane;
- Git sync drift;
- Jankurai score/staleness;
- security/artifact/release badges.

---

## 6.4 Repo Dashboard

### Purpose

The repo dashboard is the one-repo command center. It should summarize CI, queue, cache, VTI, agents, bugs, Git sync, Jankurai, security, artifacts, release, and evidence for the selected repo.

### Mock

```text
┌ REPO veox-api ─ family:veox-* ─ main a19c88f ─ pipeline #8842 running ─ fresh:0.7s ─────────────────────────┐
│ Ship score 72 ⚠  CI 61% ▶  Queue p95 2m10s  Cache 82%  VTI 87  Bugs 8  Agents 2/1b  Sec 1H  Rel canary⊘     │
├ CI NOW ───────────────────────────┬ CACHE/VTI ───────────────────────┬ ATTENTION / PROOF ──────────────────┤
│ build ✓ 1m12s                     │ cache hit 82% miss serde index    │ 1 release canary e2e stale           │
│ lint ✓ 44s                        │ cargo 92G  sccache 31G  oci 48G   │ 2 agent-red-17 grant expired         │
│ unit ▶ 3/8 shards                 │ VTI skip 412 select 63 miss 1      │ 3 test:int p95 regression +44%       │
│ integration ▶ 2/12 shards         │ low-conf: auth/session mapping     │                                      │
│ package … waiting needs:int       ├ BUGS/AGENTS ──────────────────────┤                                      │
│ canary ⊘ blocked until package    │ ready 3 in-progress 2 review 1     │ [e] release proof  [l] selected logs │
├ GIT / QUALITY / RELEASE ──────────┴───────────────────────────────────┴────────────────────────────────────┤
│ Git: local=remote main ✓ MR !117 head=a19c88f approvals 1/2  Jankurai 86.1 ↓2.4 dup cap  Artifact unsigned │
└ [3] workflow [5] cache [6] VTI [7] agents [8] bugs [9] git [q] Jankurai [r] release ───────────────────────┘
```

### Repo subtabs

Within a repo, `[` and `]` or `Shift-Tab` cycles:

- Overview
- Workflow
- Logs
- Tests/VTI
- Cache
- Agents
- Bugs
- Git/MR
- Quality/Jankurai
- Security
- Artifacts
- Release
- Evidence
- Settings

### Repo actions

- Refresh/reconcile repo state.
- Cancel superseded pipelines.
- Retry failed job.
- Run selected tests.
- Open MR/PR.
- Assign agent to bug/failure.
- Run Jankurai audit.
- Preview release/promote/rollback.
- Export repo proof bundle.

---

## 6.5 Workflow Atlas / Pipeline DAG

### Purpose

The Workflow page must visualize current CI flow as a graph, not a list. It should show multiple active pipelines across repos globally, and detailed DAGs in repo scope.

### Graph edge types

| Edge type | Source |
|---|---|
| `Needs` | GitLab `needs` / DAG dependencies |
| `StageBarrier` | stage order fallback when `needs` unavailable |
| `Artifact` | artifact dependency |
| `ChildPipeline` | bridge/downstream/child pipeline |
| `VtiSelection` | VTI selected test relationship |
| `VtiSkip` | VTI skipped test relationship |
| `ReleaseGate` | release/canary/security/artifact gate |
| `CacheDependency` | cache object/toolchain/material dependency |
| `Inferred` | fallback heuristic; visibly dashed/labeled |
| `BlockedBy` | failed/manual/approval/policy dependency |

### Graph behavior

1. Build `PipelineGraph` from GitLab jobs, bridges, child pipelines, `needs` where available, stage order fallback.
2. Collapse finished-green subgraphs by default on narrow terminals.
3. Highlight critical path using historical runtime plus current status.
4. Route edges with minimal crossings per stage lane.
5. `Space` expands/collapses selected stage/subgraph.
6. `c` centers selected object.
7. `C` toggles critical-path-only mode.
8. `l` opens live logs for selected job.
9. `e` opens evidence/capsule for selected node.
10. Stale or inferred graph sections must be marked.

### Mock

```text
┌ WORKFLOW ATLAS ─ veox-api pipeline #8842 ─ 61% ─ ETA 7m40s±2m ─ crit:test:integration ─────────────────────┐
│ build          lint             unit                        integration              package      canary     │
│  ✓ compile ──▶ ✓ fmt ─────┬──▶ ▶ unit:1 ████░  ───────┬──▶ ▶ int:auth ███░           … image      ⊘ e2e     │
│  ✓ deps                  │    ▶ unit:2 ███░           │    ▶ int:api ██░            waits:int    stale     │
│  ✓ cache warm            │    ✓ unit:3                │    … int:db waiting runner                            │
│                           └──▶ ↷ skipped:ui ◈VTI .93   └──▶ ✗ int:billing fail line 812                      │
├ JOB INSPECTOR ─ selected:int:billing #92341 ────────────────────────────────────────────────────────────────┤
│ status failed  runner rust-lg-14  queued 2m12s  duration 5m44s  p95 3m31s regression +63%  allow_fail:no    │
│ first failure: thread 'billing_roundtrip' panicked at tests/billing.rs:812                                   │
│ evidence: capsule#cap_92341  cache:hit cargo, miss sccache  VTI:selected reason:path billing/* conf .91     │
│ actions: [y] retry [b] explain [E] create bug [A] assign agent [o] GitLab [c] copy trace                    │
├ LIVE LOG TAIL ──────────────────────────────────────────────────────────────────────────────────────────────┤
│ 09:42:31 running cargo test --test billing ...                                                              │
│ 09:44:02 ERROR mismatch expected 100.00 got 99.99                                                           │
└ [←→↑↓] graph  [Space] expand  [C] critical path  [l] logs  [e] evidence  [a] actions ──────────────────────┘
```

### Job detail tabs

A selected job detail must show:

1. **Identity** — project, pipeline, job ID, name, ref, SHA, stage, status, allow-failure, web URL.
2. **Timing** — created/queued/started/finished, duration, queued duration, p50/p95, regression, critical-path rank.
3. **Runner** — pool, runner ID, manager, system ID, remote node, Docker container, contacted_at, CPU/mem/disk pressure.
4. **Logs** — stream/SSE/WS/poll fallback, search, ANSI support, error folding, jump to first failure.
5. **Artifacts** — files, size, expiry, digest, SBOM/test report/code quality/SAST/JUnit detection.
6. **Evidence** — failure capsule, retry decision, quarantine capsule, VTI receipt, cache verdict, Jankurai/security links.
7. **Actions** — play manual job, cancel, retry/requeue, explain, open in GitLab, copy trace, download artifact, attach evidence to bug.

---

## 6.6 Live Trace Viewer

### Requirements

- Bounded live streaming via SSE/WebSocket preferred; polling fallback.
- Cap in-memory tail by bytes and lines.
- Preserve ANSI color but offer sanitized mode.
- Timestamp each chunk when received and when emitted if available.
- Highlight errors, warnings, panics, test failures, cache misses, secret-redaction events, and artifact uploads.
- Search forward/backward.
- Fold noisy sections.
- Jump to first failure, last failure, next warning, artifact section, test summary.
- Link log annotations to evidence and bug creation.

### Mock

```text
┌ TRACE job #92341 veox-api/test:int:billing ─ follow:on ─ source:sse ─ 4.8MiB ─ redacted:0 ────────────────┐
│ [09:42:29.114] $ cargo test --test billing -- --nocapture                                                │
│ [09:42:31.771] cache: sccache MISS reason:toolchain_fingerprint_changed                                  │
│ [09:43:58.006] test billing_roundtrip ... FAILED                                                         │
│ [09:43:58.007] panic at tests/billing.rs:812: expected 100.00 got 99.99                                  │
│                                                                                                           │
├ ANNOTATIONS ─ first failure 812 ─ cache miss 1 ─ VTI receipt vti_552 ─ capsule cap_92341 ────────────────┤
│ [Enter] open annotation  [/] search  [n/N] next/prev  [f] follow  [E] create evidence  [A] assign agent │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 6.7 SmartCache Observatory

### Purpose

The Cache page must answer:

- Are we full?
- What is using storage?
- Which categories matter: Rust crates, Cargo sparse index, sccache, OCI layers, artifacts, action cache, material objects?
- Are objects hot, cold, leased, tainted, mutable, trusted, or safe to GC?
- Why did a job hit/miss?
- How much time/money did cache save or waste?

### Categories

Required category breakdown:

- Cargo crates
- Cargo sparse index
- sccache/compiler cache
- Rust target dirs
- OCI layers/images
- Git mirrors/shadows
- CI artifacts
- JUnit/test reports
- benchmark reports
- action cache
- material objects/aliases
- toolchain fingerprints
- unknown/unclassified

### Mock

```text
┌ SMARTCACHE OBSERVATORY ─ 284/400GiB 71% ─ hit 83% ─ singleflight 1,228 ─ tainted 4.1GiB ────────────────┐
│ Category          Size    Hot   Hit%  Miss reason top              Leased  Tainted  Reclaimable  Trend │
│ cargo crates      92GiB   41GiB 94%   none                         8GiB    0        12GiB        ▇▇▆  │
│ sccache           61GiB   12GiB 71%   toolchain fingerprint        6GiB    1.2GiB   19GiB        ▅▆▇  │
│ oci layers        48GiB   31GiB 88%   base image refresh           12GiB   0        4GiB         ▆▆▆  │
│ artifacts         39GiB   8GiB  62%   expired release evidence     7GiB    0        14GiB        ▃▃▂  │
│ material objects  31GiB   9GiB  78%   trust tier mismatch          0       2.9GiB   5GiB         ▂▃▄  │
├ SELECTED sccache/toolchain-a19c ────────────────────────────────────────────────────────────────────────┤
│ digest sha256:91cc...  trust:repo-local  lease:veox-api pipeline #8842 expires 42m  verdict:usable      │
│ misses: 37 today reason rustc minor version changed  suggestion: prewarm rust 1.82.1 image              │
├ GC PLAN ─ safe 54GiB ─ cautious 12GiB ─ blocked 33GiB leased ───────────────────────────────────────────┤
│ [g] preview GC  [h] hot entries  [m] miss reasons  [t] taints  [v] verdicts  [p] provenance             │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Cache object drilldown

Show:

- object key/digest/size/category/mutability;
- first/last seen;
- hit count and last hit;
- repo/family attribution;
- lease holders and expiry;
- taints and reasons;
- verdicts/promotions;
- material aliases/trust labels/auth scopes;
- toolchain fingerprint;
- related jobs/artifacts;
- safe GC eligibility.

### Required cache endpoints

Current `/cache/summary` is insufficient. Add:

```text
GET /api/v1/cache/summary
GET /api/v1/cache/metrics
GET /api/v1/cache/categories
GET /api/v1/cache/hot?scope=&limit=
GET /api/v1/cache/misses?scope=&since=&limit=
GET /api/v1/cache/taints?scope=&status=
GET /api/v1/cache/verdicts?object=&job=&scope=
GET /api/v1/cache/gc-plan?scope=&risk=
GET /api/v1/cache/object/{key}
GET /api/v1/cache/provenance/{key}
POST /api/v1/cache/gc/preview
POST /api/v1/cache/gc/execute
```

---

## 6.8 VTI Smart Test Skipper

### Purpose

VTI is only valuable if it is safe and explainable. This screen must prove what was selected, what was skipped, why, with what confidence, and whether misses occurred.

### Scorecard

| Metric | Meaning |
|---|---|
| selected tests | tests VTI chose to run |
| skipped tests | tests VTI skipped with reason/confidence |
| escalated tests | tests forced due to low confidence/risk |
| selector misses | failures that indicate VTI mapping missed impact |
| confidence | aggregate confidence by subsystem/change |
| time saved | estimated runner minutes saved |
| false-green risk | probability/heuristic risk of unsafe skip |
| learning health | mapping repair freshness |
| receipt coverage | every skip has durable receipt |

### Mock

```text
┌ VTI / SMART TEST SKIPPER ─ scope:veox-* ─ saved 4h12m today ─ confidence 0.84 ─ misses 3 ───────────────┐
│ Repo        Plan        Changed paths       Select Skip Escalate Conf  Miss  Time saved  State          │
│ veox-api    vti_552     billing/* auth/*    63     412  9        .91   0     38m         healthy        │
│ veox-core   vti_551     auth/session/*      122    811  44       .62   2     1h02m       degraded       │
│ veox-web    vti_548     ui/routes/*         34     220  3        .88   1     22m         needs learn    │
├ PLAN DETAIL vti_551 ────────────────────────────────────────────────────────────────────────────────────┤
│ Low confidence reason: auth/session path touched; historical selector miss 2 in 30d; external selector stale │
│ Selected: auth_int, session_roundtrip, api_contract, sqlite_compat                                       │
│ Skipped: ui_snapshot reason:path-unaffected conf .94 receipt ◈vti_551_88                                  │
│ Escalated: full auth suite reason:recent miss                                                            │
│ Actions: [v] validate [L] learn mapping [A] audit misses [r] rerun full [e] evidence                      │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Guardrails

- Every skipped test must have a reason and confidence.
- Low confidence must escalate or explain why it did not.
- VTI receipt is required before a skipped test counts as proof.
- Selector misses must remain visible until repaired.
- Release/prod gates must not accept VTI skip without receipt and sufficient confidence.

---

## 6.9 Agents and Autonomous Workflows

### Purpose

Agents are powerful and risky. The Agents screen must show lifecycle, authority, tasks, branches, MRs, logs, tool calls, CI evidence, bug attempts, and cost.

### Agent lifecycle states

```text
created -> waiting_for_grant -> planning -> editing -> tests_running -> ci_waiting -> fix_proposed -> review -> merged -> verified -> done
                                      ↘ blocked / failed / abandoned / revoked / paused
```

### Dedicated tables to add

- `agent_sessions`
- `agent_tasks`
- `agent_steps`
- `agent_messages`
- `agent_tool_calls`
- `agent_artifacts`
- `agent_grant_links`
- `agent_races`
- `agent_race_branches`
- `agent_scorecards`

### Mock

```text
┌ AGENT CONTROL CENTER ─ active 12 ─ blocked 2 ─ awaiting grant 1 ─ spend $4.82/$20 ─ kill-bell armed ─────┐
│ Agent        Task/Bug      Repo       State          Branch/MR       Grant        CI       Cost  ETA     │
│ red-17       BUG-1842      veox-api   blocked        agent/fix-1842  expired R3   failed   $0.82 —       │
│ blue-03      JANK-dup-9    veox-core  tests_running  race/h2         valid R3     #8841 ▶  $1.41 8m      │
│ green-11     VTI-miss      veox-web   planning       —               valid R2     —        $0.18 3m      │
├ SELECTED red-17 ────────────────────────────────────────────────────────────────────────────────────────┤
│ Goal: fix billing_roundtrip panic BUG-1842. Actor agent-red-17. Grant expired 09:41. No self-approval.   │
│ Steps: plan ✓ edit ✓ unit ✓ integration ✗ propose MR pending. Last tool: GitLab trace fetch.              │
│ Evidence: bug attempt #3, pipeline #8840, capsule cap_92341, branch head 881abc                          │
│ Actions: [g] renew grant preview  [l] logs  [d] diff  [r] rerun tests  [k] kill task  [e] evidence       │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Agent config editor

The TUI may edit agent/autonomy configs, but only safely:

- open config as structured form plus raw diff;
- validate schema and policy;
- show risk tier and affected paths;
- for protected repos, save as branch/MR, not direct write;
- require CODEOWNER/human approval for high-risk config;
- agents cannot approve their own config changes;
- every change creates proof event.

### Autonomous workflows panel

Show:

- autonomy profile;
- kill-bell status;
- freeze windows;
- Evidence Gate/VibeGate verdicts;
- merge passports;
- release passports;
- Foundry candidates;
- Nightwatch canary evaluations;
- launch ledger replay;
- escalation dispatch results;
- daemon poller state;
- PR/MR drift;
- LLM provider health and budget.

---

## 6.10 Bugs and Issues Cockpit

### Purpose

The Bugs screen must give cross-repo accountability: what is known, what is ready, what is being worked, what failed, what is reviewing, and what is done with proof.

### Bug lanes

```text
needs_triage -> needs_info -> accepted -> ready -> in_progress -> blocked -> fix_proposed -> reviewing -> verifying -> done
                                        ↘ duplicate / invalid / cannot_reproduce / wont_do
```

Attempt states:

```text
pending -> started -> failed | fix_proposed | verified | abandoned
```

### Mock

```text
┌ BUGS / ISSUES ─ scope:fleet ─ open 91 ─ ready 18 ─ in_progress 11 ─ blocked 7 ─ done 144 ────────────────┐
│ NEEDS TRIAGE      READY                 IN PROGRESS             REVIEWING             DONE              │
│ BUG-1901 sec leak  BUG-1842 billing      BUG-1777 VTI miss       BUG-1711 dup refactor BUG-1620 cache GC │
│ BUG-1899 flaky int BUG-1812 Jankurai cap BUG-1750 release e2e    BUG-1704 sqlite compat BUG-1599 docs    │
├ SELECTED BUG-1842 ─ veox-api ─ severity high priority p0 difficulty medium ─────────────────────────────┤
│ Title: billing_roundtrip returns 99.99 under integration. Acceptance: exact decimal roundtrip passes.    │
│ Evidence: capsule cap_92341, log job #92341, failed pipeline #8840, customer repro path docs/billing.md │
│ Attempts: #1 agent-red failed branch a/fix-1; #2 human note; #3 agent-red blocked grant expired           │
│ Links: MR !117, issue #52, release blocker rel_2026_05_26                                               │
│ Actions: [A] assign agent [u] update [p] proof [m] open MR [c] commits [e] evidence                      │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Bug detail must show

- canonical report fields;
- source/target project;
- component;
- current/expected behavior;
- environment/frequency/impact;
- severity/priority/difficulty;
- security/privacy flags;
- no-secrets confirmation;
- reproduction steps;
- evidence paths/URLs/digests;
- acceptance criteria;
- owner;
- events;
- attempts with agent, sandbox path, branch, base/head SHA, PR/MR URL, CI evidence, notes;
- final commits and release links when done.

---

## 6.11 Git Sync / MR / PR / Admission

### Purpose

The Git screen answers whether local repos, shadow remotes, backup mirrors, protected branches, MRs/PRs, and admission decisions are aligned.

### Mock

```text
┌ GIT SYNC / REMOTE STATE ─ scope:veox-* ─ MR hook:partial ⚠ ─ admission enforce:on ──────────────────────┐
│ Repo       Local main Remote main Diverge Dirty Last MR/PR  Mergeable Approvals Admission Mirror  Drift │
│ veox-api   a19c88f    a19c88f    0       no    !117        maybe     1/2       allow     ok      none  │
│ veox-core  77ab013    71cc903    +4/-1   no    !112        conflict  0/2       audit     lag     5m    │
│ veox-web   3dd91aa    3dd91aa    0       yes   !109        blocked   2/2       deny      ok      dirty │
├ SELECTED veox-core ─────────────────────────────────────────────────────────────────────────────────────┤
│ local ahead 4 behind 1; last sync failed due conflict in crates/auth/session.rs                          │
│ last admission: audit reason missing grant for protected main push actor agent-blue-03                   │
│ MR !112 head 77ab013 target main policy_sha 991a approvals 0/2 discussions unresolved 3                 │
│ Actions: [s] sync preview [m] open MR [a] admission proof [d] diff [r] reconcile                         │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Data required

- local head SHA, branch, dirty state;
- remote head SHA;
- divergence counts;
- last successful sync;
- last failed sync and reason;
- mirror/backup status;
- MR/PR state: IID/number, source/target, head/base SHA, mergeability, approvals, discussions, labels, draft status, changed files, pipeline;
- protected branch policy;
- admission decisions and grant matches;
- Git command events and artifacts;
- sidecar/shadow status.

### MR hook ingestion to add

Capture MR IID, source/target, title, labels, author, merge status, detailed merge status, approval state, reviewers, discussions, changed files, diff stats, pipeline head SHA, draft state, and linked bugs/release blockers.

---

## 6.12 CI Bottleneck Lab

### Purpose

A deeper analytics view for CI performance. It should rank bottlenecks by real impact, not raw duration.

### Dimensions

- job name/stage;
- repo/family;
- pool/tag/trust tier;
- p50/p95/latest/max duration;
- queue wait p50/p95;
- critical-path hits;
- regression vs baseline;
- cache miss correlation;
- VTI fallback correlation;
- flake rate;
- failure rate;
- agent-introduced regressions;
- release blocker frequency.

### Mock

```text
┌ CI BOTTLENECK LAB ─ scope:fleet ─ window:14d ─ sorted:critical-path impact ─────────────────────────────┐
│ Rank Job/Stage              Repo/family   p50   p95   Latest  Queue p95 Crit hits  Cause       Action  │
│ 1    test:integration       veox-core     9m10  18m40 22m11   7m42      81%        serial DAG  split   │
│ 2    docker:build           veox-api      6m02  12m31 8m44    1m10      44%        cache miss  prewarm │
│ 3    security:sast          veox-web      4m55  9m20  15m01   0m20      22%        regression  inspect │
├ EXPLAIN rank 1 ────────────────────────────────────────────────────────────────────────────────────────┤
│ Integration is the critical path in 81% of veox-core runs. Adding runners saves <1m because stage is serial. │
│ Best action: shard by module or add GitLab needs edges. Estimated save 7m, confidence .74, risk R3.       │
└ [s] simulator  [x] export report  [A] create optimization bug  [e] evidence ────────────────────────────┘
```

---

## 6.13 Jankurai Audit Center

### Purpose

Jankurai must be first-class. It should show audit score, trend, installed version, caps, findings, duplicate clusters, stale/missing runs, and whether quality gates affect CI/release.

### Data model

```rust
pub struct JankuraiRun {
    pub repo_id: RepoId,
    pub run_id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub version: Option<String>,
    pub profile: String,
    pub commit_sha: Sha,
    pub status: AuditStatus,
    pub score: Option<f32>,
    pub previous_score: Option<f32>,
    pub caps: Vec<JankuraiCap>,
    pub findings: Vec<JankuraiFinding>,
    pub duplicate_clusters: Vec<DuplicateCluster>,
    pub tool_adoption: Vec<JankuraiToolAdoption>,
    pub evidence_refs: Vec<EvidenceRef>,
    pub artifact_digest: Option<String>,
}
```

Required finding categories:

- duplicate code / semantic clones;
- release bad behavior: mutable tags/assets, missing proof, missing provenance, no rollback;
- Docker anti-patterns;
- type/contract drift;
- web security;
- dependency/security/provenance findings;
- test integrity / proof routing gaps;
- TUI black-box testing failures;
- repo rot and generated-zone violations;
- UX/accessibility evidence gaps;
- migration safety issues.

### Mock

```text
┌ JANKURAI AUDIT CENTER ─ fleet score 88.1 ↓1.7 ─ blocking caps 3 ─ duplicate clusters 14 ────────────────┐
│ Repo        Version  Score Trend Gate  Blocking cap          Dup clusters Issues Last run  Agent caused │
│ veox-api    0.14.2   82.0  ↓6.4  warn  duplicate-code        8            31     4m        blue-03      │
│ veox-ui     0.14.2   91.2  ↑1.1  pass  —                     2            9      17m       —            │
│ veox-db     0.14.1   88.4  →     warn  complexity-budget     3            18     1h        —            │
│ redlinedb   missing  —     —     fail  jankurai not installed —           —      never     —            │
├ FINDING detail ─ duplicate-code cap ─────────────────────────────────────────────────────────────────────┤
│ observed 14.2%, limit 8%; files crates/api/auth.rs and crates/core/tokens.rs; similarity .91; 184 lines │
│ Actions: [b] create bug [A] assign agent [d] duplicated hunks [g] gate policy [r] rerun audit           │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 6.14 Runners, Pools, Nodes, and System Utilization

### Purpose

This page separates “we need more machines” from “machines are idle but unusable.”

### Required metrics

- theoretical slots, online slots, usable slots, busy/idle/unhealthy slots;
- pool max managers, min warm, concurrency, request concurrency;
- pool tags and trust tiers;
- queue by eligible pool;
- manager/container state;
- runner ID, system ID, contacted_at;
- remote node CPU/memory/disk/network/SSH latency;
- Docker daemon health;
- disk thresholds 90% warning / 95% critical;
- image pull latency;
- OOM/death events;
- reconcile loop status;
- GC actions and reclaimed bytes;
- version/config hash.

### Mock

```text
┌ RUNNERS / SYSTEM UTILIZATION ─ usable 142/164 ─ busy 131 ─ unhealthy 7 ─ disk pressure 2 ───────────────┐
│ Pool        Slots busy/usable/theory  Queue p95  Managers  Nodes          Trust  Loss                  │
│ rust-large  32/32/40                  7m42s      16/20     local,node-2   high   unhealthy4 disk2 tag2 │
│ docker-med  49/58/64                  1m10s      25/32     node-1,node-3  med    trust mismatch3       │
│ macos       4/4/4                     22m        4/4       mac-mini       high   physical max          │
├ NODE DETAIL node-3 ────────────────────────────────────────────────────────────────────────────────────┤
│ CPU 89% mem 74% disk 91% ⚠ net ok docker ok managers 8 max 10 cache 122GiB GC reclaimable 18GiB        │
│ Events: OOM runner docker-med-7 09:21, GC skipped lease active, SSH latency p95 180ms                  │
│ Actions: [S] scale [D] drain [g] GC node [l] logs [r] reconcile [p] proof                              │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 6.15 Code Change Volume and Risk

### Purpose

This screen correlates churn with risk, CI cost, VTI misses, Jankurai regressions, bugs, and agent activity.

### Metrics

- additions/removals over time;
- churn heatmap by repo/path/author/agent;
- MR size, files touched, generated vs human code;
- touched proof lanes;
- test impact and VTI confidence;
- Jankurai score correlation;
- bug/revert correlation;
- release risk;
- owner/reviewer routing.

### Mock

```text
┌ CODE CHURN / RISK ─ 14d ─ fleet +41k/-18k ─ generated 22% ─ agent-authored 31% ────────────────────────┐
│ Repo       ΔLOC    Files  MRs  Agents  Risk  VTI miss  Jankurai Δ  Bugs opened  Hot paths              │
│ veox-core  +12k/-4k 188   31   42%     high  2         -2.4        14           auth/session, cache     │
│ veox-api   +8k/-3k  121   24   29%     med   0         +0.9        8            billing, release        │
│ veox-web   +5k/-6k  144   19   18%     high  1         -5.1        9            routes, auth            │
└ [Enter] path heatmap  [v] VTI correlation  [j] Jankurai correlation  [b] create risk bug ───────────────┘
```

---

## 6.16 Security, Policy, Secrets, and Grants

### Purpose

This screen must make the system safe to operate without leaking secrets. It combines scan reports, Vault metadata, active grants, admission decisions, policy violations, and secret audit events.

### Domains

- SAST/code-quality/dependency/container scans;
- secret scanning and Vault status;
- active capability grants;
- admission decisions;
- Git risk approvals;
- MCP/capability nonce replay or denied requests;
- policy version and drift;
- redacted settings/config;
- Jankurai security findings;
- artifact signature/provenance failures;
- no-self-approval violations.

### Mock

```text
┌ SECURITY / POLICY ─ critical 2 high 9 ─ grants active 17 ─ Vault sealed:no ─ policy prod-safe-v4 ───────┐
│ Repo       Findings       Grants  Admission     Secrets             Artifact      Policy               │
│ veox-web   1C 4H          2       deny 1        no plaintext ✓      unsigned      CORS wildcard        │
│ veox-api   0C 1H          4       allow 12      rotation due 2d     signed        ok                   │
│ veox-core  1C 2H          7       audit 3       token fp only ✓     SBOM stale    self-approval block  │
├ SELECTED veox-web critical ────────────────────────────────────────────────────────────────────────────┤
│ Finding: browser token storage in src/auth/session.ts. Evidence scan artifact sast_991. SHA 3dd91aa.    │
│ Actions: [b] create bug [A] assign agent [p] proof [m] block merge [o] artifact                         │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Redaction rules

- Never render plaintext secrets.
- Show fingerprints, paths, status, mount, prefix, expiry, policy, and redacted env var presence only.
- Screenshots/export must redact tokens, Authorization headers, PATs, unseal keys, root tokens, webhook secrets, and raw request bodies.
- Logs must run through redaction before display and before snapshot tests.

---

## 6.17 Signed Artifacts and Provenance

### Purpose

This screen shows whether build artifacts can be trusted, reproduced, promoted, or rolled back.

### Required fields

- artifact id/path/name/type;
- project/repo/pipeline/job/ref/SHA;
- digest and size;
- signature status and signer;
- SBOM status;
- provenance/SLSA-style statement status;
- scan reports;
- cache/material dependencies;
- release passport link;
- rollback eligibility;
- expiry;
- exact digest deployed to canary/prod.

### Mock

```text
┌ ARTIFACTS / PROVENANCE ─ scope:veox-api ─ unsigned 1 ─ SBOM stale 1 ───────────────────────────────────┐
│ Artifact        Digest        Job        Signature  SBOM  Provenance  Scans  Release       Rollback    │
│ api:2.8.1-rc1   sha256:118a   package    signed     ok    verified    ok     canary        no          │
│ web:2.8.1-rc1   sha256:22bc   package    unsigned   ok    missing     warn   blocked       no          │
│ api:2.8.0       sha256:91cc   package    signed     ok    verified    ok     prod current  yes         │
├ SELECTED api:2.8.1-rc1 ────────────────────────────────────────────────────────────────────────────────┤
│ Built from a19c88f pipeline #8842 job #92380. Cache verdicts clean. VTI receipt vti_552. Jankurai 86.1. │
│ Actions: [p] provenance [s] signature detail [r] release proof [B] rollback target compare              │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 6.18 Release, Production, Rollback, and Version Control

### Purpose

The Release screen must make ship/no-ship and rollback decisions exact, fast, and safe.

### Release doctrine

- A release is bound to exact source SHA.
- A release has artifact digests, SBOM/provenance/signatures, VTI/Jankurai/security receipts, canary telemetry, and rollback target.
- Production promotion requires proof gates and approval.
- Rollback preview is one key away; rollback execution is never accidental.

### Mock

```text
┌ RELEASE CONTROL ─ veox-api ─ current prod 2.8.0 sha256:91cc ─ candidate 2.8.1 sha256:118a ─────────────┐
│ Stage          State      Pipeline  SHA      Artifact      Gate proof                  Action          │
│ source exact   ✓          #8842     a19c88f  —             commit verified             —               │
│ CI full        ▶ 61%      #8842     a19c88f  —             test:int running            wait            │
│ VTI receipt    ✓          vti_552   a19c88f  —             conf .91 no misses          open            │
│ Jankurai       ⚠          jank_92   a19c88f  —             dup cap advisory            waive?          │
│ artifact       ✓          job 92380 a19c88f  sha256:118a   signed SBOM ok              open            │
│ canary e2e     ⊘ stale    #8849     a19c88f  sha256:118a   e2e proof older than SHA    rerun           │
│ telemetry      … waiting  —         —        —             min 30m canary required     wait            │
│ prod promote   blocked    —         —        —             canary e2e stale            disabled        │
├ ROLLBACK PLAN ─────────────────────────────────────────────────────────────────────────────────────────┤
│ target 2.8.0 sha256:91cc last known good, prod verified 2026-05-25 22:11, rollback drill passed 2d ago │
│ Actions: [p] release proof [c] canary logs [B] rollback preview [M] promote disabled [r] reconcile      │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Release actions

| Action | Risk | Required preview/proof |
|---|---:|---|
| release status/watch/reconcile | R0/R1 | state diff, source freshness |
| preflight/ready/dry-run | R1 | exact SHA, gate checklist |
| submit release | R4 | version, SHA, artifacts, gates, branch/ref, idempotency |
| approve/promote prod | R5 | human approval, exact SHA, artifact digest, rollback target, all gates green/waived |
| rollback | R5 | current artifact, target artifact, last-known-good proof, impact, typed confirmation |
| waive advisory gate | R4/R5 | policy, reason, approver, expiry, evidence |

### Automatic release/version control

Autonomous release flows may suggest actions, but high-risk steps require proof and approval. The UI must show Foundry candidates, ReleasePassports, Nightwatch verdicts, freeze windows, kill-bell status, and merge passports.

---

## 6.19 Evidence / Proof Ledger

### Purpose

The evidence system is the trust backbone. Every important decision must be replayable.

### Query dimensions

- entity kind/id;
- repo/family/project;
- branch/ref/SHA;
- actor/agent/human;
- request id / correlation id;
- event type;
- severity;
- action id;
- grant id;
- MR/PR id;
- job/pipeline id;
- release id;
- artifact digest;
- time range;
- source system;
- redaction status.

### Proof timeline object

```rust
pub struct ProofEvent {
    pub id: String,
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub kind: ProofKind,
    pub severity: Severity,
    pub entity: EntityRef,
    pub actor: Option<String>,
    pub correlation_id: Option<String>,
    pub summary: String,
    pub refs: Vec<EvidenceRef>,
    pub payload_redacted: serde_json::Value,
    pub source: DataSourceId,
    pub digest: Option<String>,
}
```

### Mock

```text
┌ EVIDENCE / FLIGHT RECORDER ─ query:repo=veox-api sha=a19c88f ─ 88 events ───────────────────────────────┐
│ Time      Kind                 Entity             Actor        Summary                         Proof      │
│ 09:12:01 webhook.pipeline       pipeline #8842     gitlab       running main a19c88f             sha:aa12  │
│ 09:14:18 vti.plan.created       vti_552            system       select 63 skip 412 conf .91      ◈         │
│ 09:21:44 job.failed             job #92341         runner       billing_roundtrip panic          cap_92341 │
│ 09:22:10 bug.attempt.started    BUG-1842           agent-red-17 branch agent/fix-1842            grant_41  │
│ 09:31:53 artifact.signed        api:2.8.1-rc1      release      sha256:118a signed               sig_77    │
│ 09:39:04 release.gate.blocked   canary-e2e         system       proof stale for candidate SHA    gate_12   │
├ SELECTED release.gate.blocked ──────────────────────────────────────────────────────────────────────────┤
│ Gate canary-e2e requires proof newer than a19c88f. Existing proof built from 71cc903.                    │
│ Actions: [o] open file [r] rerun e2e [B] rollback preview [x] export proof bundle                        │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Evidence sources

- `events` table;
- evidence capsules;
- admission decisions;
- capability intents and grants;
- git command events/ref updates/mirror jobs/risk approvals/artifacts;
- secret audit events;
- release attempts/gates;
- retry decisions;
- VTI plans and selector misses;
- cache verdicts/taints/promotions;
- bug events/attempts/evidence;
- agent sessions/tool calls;
- autonomy launch ledger/verdicts;
- webhook delivery ledger;
- artifact signatures/provenance;
- Jankurai audit receipts;
- scan reports.

---

## 6.20 Doctor: Source, Runtime, API/MCP, DB, Workers, Config

### Purpose

The Doctor screen makes system truth inspectable. It should expose freshness, docs/source drift, runtime build metadata, API catalogs, worker loops, DB health, settings, secrets, and synthetic self-tests.

### Subtabs

1. **Source Freshness** — GitLab, DB, Docker, cache, Vault, broker, MCP, events, logs.
2. **Runtime Profile** — commit SHA, build time, feature flags, profile, ports, binds, uptime, memory/CPU/open handles.
3. **API Catalog** — HTTP/OpenAPI-style live routes with auth, schemas, p50/p95 latency, errors.
4. **MCP Registry** — tools/resources/prompts, schemas, capability flags, source version, call audit.
5. **DB Inspector** — backend, migrations, row counts, indexes, slow queries, recent queries, query plans.
6. **Workers/Pollers** — last tick, next tick, lag, backoff, counts, high-water marks, last error.
7. **Outbound API Tap** — GitLab/Vault/LLM/GitHub calls, method, URL template, status, latency, retries, redacted samples.
8. **Config/Secrets Health** — required/optional env vars, missing credentials, fingerprints only.
9. **Synthetic Self-Test** — exercises API/MCP/DB/dependencies and reports actionable failures.
10. **Docs/Source Drift** — RedlineDB docs, MCP count, cache auth, `ListAllowedActions`, `request_merge` gate.

### Mock

```text
┌ DOCTOR / RUNTIME PROFILE ─ build a19c88f ─ profile sqlite+kafka ─ uptime 4h12m ─────────────────────────┐
│ Source       State   Fresh  Cursor/Version       Latency   Notes                                      │
│ DB           ✓       0.3s   sqlite WAL mig 42     2ms       rows events=184923 jobs=12011              │
│ GitLab       ✓       1.1s   rate 4921/5000        212ms     hooks ok                                  │
│ Broker       ⚠       4.8s   kafka lag jobs=83     19ms      pipeline consumer lag high                 │
│ Docker       ✓       0.9s   events live           8ms       last OOM 21m                               │
│ MCP          ⚠       —      tools=16 resources=0  —         resources missing GET /mcp 405             │
│ Cache        ✓       2.2s   284GiB/400GiB         31ms      token auth required                         │
│ Vault        ✓       3.0s   sealed:no             45ms      rotation due 2d                            │
├ DRIFT / WARNINGS ──────────────────────────────────────────────────────────────────────────────────────┤
│ ⚠ docs/API undercounts MCP tools; source has 16.                                                       │
│ ⚠ ListAllowedActions static/stale vs action registry.                                                  │
│ ⚠ request_merge must be routed through merge proof gate before production use.                          │
│ ✓ SQLite default; RedlineDB optional.                                                                  │
├ ACTIONS ────────────────────────────────────────────────────────────────────────────────────────────────┤
│ [s] synthetic self-test  [o] OpenAPI catalog  [m] MCP registry  [d] DB inspector  [j] redacted JSON     │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 6.21 Incident Mode, Replay Mode, Demo/Capture

### Incident Mode

`!` or `:incident start` opens a focused room:

- freezes current scope and event cursor;
- pins top blockers;
- starts incident event bundle;
- shows release/rollback/security/queue/agent panels;
- disables risky actions unless explicitly confirmed;
- exports proof bundle and screenshots.

### Replay Mode

Replay any event stream:

```bash
jeryu tui replay --incident release-2026-05-26 --speed 4x
jeryu tui replay --from-event 180000 --to-event 184923 --scope veox-api
```

UI should support time-travel diff:

```text
Now vs 30m ago: queue pressure +0.31, cache +22GiB, VTI confidence -0.18, Jankurai -2.4, runner disk node-3 +12%.
```

### Demo and capture

Preserve/expand existing capture/screenshot support:

```bash
jeryu tui --demo dream --tab global --width 180 --height 50
jeryu tui --capture fixture.json --tab workflow --output out/workflow.txt
jeryu tui --demo --tab cache --screenshot out/cache.svg
jeryu repo capture-tui-screenshots --all-tabs
```

Deterministic demo fixtures:

- green fleet;
- queue saturation;
- failed pipeline;
- VTI degraded;
- cache near full;
- agent race;
- release canary blocked;
- security critical;
- Jankurai regression;
- Git drift;
- stale data source;
- production rollback.

---

## 7. Backend inspection plane

### 7.1 Golden architecture

```text
GitLab webhooks/API   Docker events   Cache proxy   Vault   Git hooks   Agents/MCP   Autonomy   Jankurai
        │                │              │           │        │          │            │          │
        └──────────────┬─┴──────────────┴──────┬────┴────────┴──────────┴────────────┴──────────┘
                       │                        │
                 Event ingestion          State repositories
                       │                        │
                       └────────────┬───────────┘
                                    │
                         Unified Read Model
                 Entity graph + Event stream + Action registry
                                    │
             ┌──────────────────────┼──────────────────────┐
             │                      │                      │
         Rust TUI             HTTP/SSE API              MCP resources/tools
```

Golden rule: **TUI, CLI, MCP, agents, and docs consume the same typed read/action contract.** No screen should reimplement truth parsing from ad hoc CLI text if a typed source exists.

### 7.2 Required HTTP API

Minimum endpoints:

```text
GET  /api/v1/tui/snapshot?scope=&since_cursor=
GET  /api/v1/tui/events?cursor=&limit=&kinds=&entity_kind=&entity_id=&scope=
GET  /api/v1/tui/events/stream?cursor=&scope=&kinds=          # SSE
GET  /api/v1/tui/entity/{kind}/{id}
GET  /api/v1/tui/search?q=&scope=&limit=
POST /api/v1/tui/action/preview
POST /api/v1/tui/action/execute
GET  /api/v1/tui/actions?entity_kind=&risk=&surface=
GET  /api/v1/tui/freshness
GET  /api/v1/tui/runtime-profile/redacted
GET  /api/v1/tui/doctor/deep
```

Domain endpoints:

```text
GET /api/v1/repos
GET /api/v1/repo-families
GET /api/v1/repo/{repo_id}
GET /api/v1/pipeline/{project_id}/{pipeline_id}/graph
GET /api/v1/job/{project_id}/{job_id}/trace/stream
GET /api/v1/queue/global
GET /api/v1/runners/pools
GET /api/v1/runners/nodes
GET /api/v1/cache/summary|metrics|categories|hot|taints|verdicts|gc-plan
GET /api/v1/vti/summary
GET /api/v1/vti/plan/{plan_id}
GET /api/v1/agents
GET /api/v1/agent/{agent_id}
GET /api/v1/bugs
GET /api/v1/bug/{bug_id}
GET /api/v1/git-sync
GET /api/v1/jankurai/summary
GET /api/v1/security/summary
GET /api/v1/artifacts
GET /api/v1/release/latest
GET /api/v1/release/{release_id}
GET /api/v1/evidence/search
GET /api/v1/settings/effective-redacted
GET /api/v1/openapi/live
GET /api/v1/mcp/registry
GET /api/v1/workers
GET /api/v1/db/inspect
GET /api/v1/outbound-api/tap
GET /api/v1/self-test
```

### 7.3 MCP resources to mirror

MCP should be more than tools. Add read resources:

```text
jeryu://tui/read-model
jeryu://events?after=N
jeryu://system/snapshot
jeryu://system/freshness
jeryu://repos
jeryu://repo/{repo_id}
jeryu://repo-family/{family_id}
jeryu://queue/global
jeryu://pipeline/{project_id}/{pipeline_id}
jeryu://pipeline/{project_id}/{pipeline_id}/graph
jeryu://job/{project_id}/{job_id}
jeryu://job/{project_id}/{job_id}/trace
jeryu://job/{project_id}/{job_id}/capsule
jeryu://runners/pools
jeryu://runners/nodes
jeryu://cache/summary
jeryu://cache/object/{key}
jeryu://cache/provenance/{key}
jeryu://vti/summary
jeryu://vti/plan/{plan_id}
jeryu://agents
jeryu://agent/{agent_id}
jeryu://bugs/ready
jeryu://bug/{bug_id}
jeryu://git-sync
jeryu://admission/recent
jeryu://capability/grants
jeryu://jankurai/repo/{repo_id}
jeryu://security/repo/{repo_id}
jeryu://artifact/{artifact_id}/provenance
jeryu://release/latest
jeryu://release/{release_id}
jeryu://secrets/status
jeryu://autonomy/kill-bell
jeryu://autonomy/verdicts
jeryu://settings/effective-redacted
jeryu://doctor/self-test
```

Add MCP watch semantics:

```text
jeryu.watch_events({ cursor, kinds?, entity_kind?, entity_id?, scope? })
```

or MCP-native `resources/subscribe` for `jeryu://events`.

### 7.4 Entity model

```rust
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
    pub label: String,
    pub repo_id: Option<RepoId>,
    pub family_id: Option<RepoFamilyId>,
    pub project_id: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum EntityKind {
    System,
    Component,
    RepoFamily,
    Repo,
    Project,
    MergeRequest,
    PullRequest,
    Pipeline,
    WorkflowNode,
    Job,
    Stage,
    Runner,
    Pool,
    RemoteNode,
    CacheObject,
    CacheTaint,
    CacheVerdict,
    TestPlan,
    TestCase,
    VtiDecision,
    Agent,
    AgentSession,
    AgentTask,
    AgentStep,
    AgentRace,
    AutonomousWorkflow,
    Bug,
    BugAttempt,
    GitCommand,
    AdmissionDecision,
    CapabilityGrant,
    CapabilityIntent,
    JankuraiAudit,
    JankuraiFinding,
    SecurityFinding,
    SecretAuthority,
    SecretAccess,
    Artifact,
    Signature,
    Provenance,
    ReleaseAttempt,
    ReleaseGate,
    EvidenceCapsule,
    ProofEvent,
    LlmProvider,
}
```

### 7.5 Entity detail contract

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityDetail {
    pub entity: EntityRef,
    pub state: String,
    pub severity: Severity,
    pub summary: String,
    pub metrics: Vec<Metric>,
    pub timeline: Vec<TimelineEvent>,
    pub blockers: Vec<Blocker>,
    pub evidence: Vec<EvidenceRef>,
    pub related: Vec<EntityRef>,
    pub available_actions: Vec<ActionDescriptor>,
    pub risk: Option<RiskTier>,
    pub source_freshness: Vec<SourceFreshness>,
    pub last_updated: DateTime<Utc>,
    pub stale_after_ms: u64,
    pub raw_redacted: serde_json::Value,
}
```

### 7.6 Event model

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TuiEvent {
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub kind: String,
    pub severity: Severity,
    pub entity: EntityRef,
    pub parent: Option<EntityRef>,
    pub repo_id: Option<RepoId>,
    pub family_id: Option<RepoFamilyId>,
    pub correlation_id: Option<String>,
    pub summary: String,
    pub fields: serde_json::Value,
    pub evidence_refs: Vec<EvidenceRef>,
    pub next_actions: Vec<ActionDescriptor>,
    pub source: DataSourceId,
}
```

Required event kind families:

- `system.health.updated`, `source.freshness.changed`, `snapshot.refreshed`
- `repo.discovered`, `repo.sync.updated`, `repo.family.updated`
- `pipeline.created/running/succeeded/failed/canceled/blocked`
- `job.queued/started/progress/log.chunk/annotation/failed/succeeded/retried/canceled`
- `runner.online/offline/busy/idle/degraded/oom/scale.requested`
- `cache.hit/miss/taint.created/verdict/gc.plan/gc.completed`
- `vti.plan.created/test.selected/test.skipped/selector.miss/learning.updated`
- `agent.session.started/intent.requested/grant.issued/patch.proposed/race.started/race.winner/failed/completed`
- `bug.created/triaged/attempt.started/attempt.failed/fix.proposed/done`
- `git.sync.updated/admission.allow/admission.audit/admission.deny/mirror.failed`
- `jankurai.audit.started/completed/score.changed/finding.created/cap.hit`
- `security.finding.created/secret.detected/vulnerability.changed/policy.violation`
- `artifact.built/signed/signature.failed/provenance.verified`
- `release.submitted/canary.started/canary.failed/promoted/rollback.started/rollback.completed/gate.blocked`
- `secret.audit.denied/rotation.due/finalized`
- `llm.provider.degraded/budget.exhausted/call.completed`
- `action.previewed/started/completed/denied/failed`

### 7.7 Read model snapshot

```rust
pub struct TuiReadModel {
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub event_cursor: u64,
    pub sources: Vec<SourceFreshness>,
    pub mission: MissionSnapshot,
    pub repo_families: Vec<RepoFamilySummary>,
    pub repos: Vec<RepoSummary>,
    pub queue: QueueSnapshot,
    pub runners: RunnerFleetSnapshot,
    pub cache: CacheSnapshot,
    pub vti: VtiFleetSnapshot,
    pub agents: AgentFleetSnapshot,
    pub bugs: BugFleetSnapshot,
    pub git: GitSyncSnapshot,
    pub jankurai: JankuraiFleetSnapshot,
    pub security: SecuritySnapshot,
    pub artifacts: ArtifactFleetSnapshot,
    pub release: ReleaseFleetSnapshot,
    pub evidence: EvidenceSummary,
    pub attention: Vec<AttentionItem>,
    pub next_action: Option<ActionDescriptor>,
}
```

### 7.8 Action model

```rust
pub struct ActionDescriptor {
    pub id: String,
    pub label: String,
    pub target: EntityRef,
    pub risk: RiskTier,
    pub side_effect: SideEffectClass,
    pub dry_run_supported: bool,
    pub required_grants: Vec<GrantRequirement>,
    pub required_proofs: Vec<ProofRequirement>,
    pub idempotency_key: Option<String>,
    pub disabled_reason: Option<String>,
}

pub struct ActionPreviewRequest {
    pub action_id: String,
    pub target: EntityRef,
    pub args: serde_json::Value,
    pub actor: String,
    pub idempotency_key: String,
}

pub struct ActionPreview {
    pub action: ActionDescriptor,
    pub allowed: bool,
    pub blockers: Vec<Blocker>,
    pub planned_writes: Vec<PlannedWrite>,
    pub external_calls: Vec<ExternalCallPreview>,
    pub required_confirm_text: Option<String>,
    pub expected_events: Vec<String>,
    pub rollback_plan: Option<RollbackPlan>,
    pub proof_refs: Vec<EvidenceRef>,
}

pub struct ActionResult {
    pub action_id: String,
    pub target: EntityRef,
    pub status: ActionStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub events: Vec<TuiEvent>,
    pub evidence_refs: Vec<EvidenceRef>,
    pub output_redacted: serde_json::Value,
}
```

Risk tiers:

| Tier | Meaning | Examples | Confirmation |
|---|---|---|---|
| R0 | Read-only | open logs, show proof, filter | none |
| R1 | Local non-destructive | refresh, reconcile read, clear stale local view | simple |
| R2 | CI mutation | retry job, cancel pipeline, run tests | preview |
| R3 | Repo mutation | propose patch, create branch, update bug | preview + grant if agent |
| R4 | Merge/release-sensitive | request merge, approve release, edit autonomy config | exact SHA + grant + approval |
| R5 | Production/destructive | promote prod, rollback, delete runner, rotate/finalize secrets, destructive GC | typed confirmation + required approver |

Absolute action rules:

- Never expose raw secrets.
- Never allow agents to approve their own work.
- Never merge without exact SHA binding.
- Never promote to production without rollback target.
- Never treat skipped tests as proof without VTI receipt.
- Never treat cache reuse as safe without trust verdict/taint check.
- Never hide stale data.
- Never execute high-risk action from stale read model.

---

## 8. Rust implementation architecture

### 8.1 Recommended stack

- `ratatui` for terminal UI.
- `crossterm` backend first; optional `termion` only if needed.
- `tokio` runtime for async tasks.
- `reqwest` or existing internal HTTP client for backend API.
- `tokio-tungstenite` only if WebSocket is chosen; otherwise SSE via streaming HTTP.
- `serde`, `serde_json`, `serde_with` for typed contracts.
- `indexmap` for stable render order.
- `lru` or custom bounded caches for traces/entities.
- `unicode-width`, `unicode-segmentation` for correct rendering.
- `tui-input` or custom input editor for command palette/filter.
- `insta` for golden snapshots.
- `proptest` for reducer/navigation invariants.

### 8.2 Crate/module layout

```text
src/
  tui/
    mod.rs
    app.rs                  # App root, route stack, focus, reducer
    event_loop.rs           # tokio + terminal event multiplex
    terminal.rs             # setup/restore/panic hooks
    config.rs               # TUI config, theme, keymap, demo options
    model/
      mod.rs
      ids.rs
      route.rs
      entity.rs
      event.rs
      action.rs
      snapshot.rs
      freshness.rs
      metrics.rs
      proof.rs
      workflow.rs
      cache.rs
      vti.rs
      agents.rs
      bugs.rs
      release.rs
      security.rs
    client/
      mod.rs
      trait.rs              # InspectionClient
      http.rs
      mcp.rs
      cli_fallback.rs
      fake.rs
      replay.rs
      stream.rs
      logs.rs
    store/
      mod.rs
      entity_store.rs
      indexes.rs
      event_ring.rs
      trace_store.rs
      selection.rs
      lens_store.rs
    views/
      mod.rs
      global.rs
      queue.rs
      repos.rs
      repo.rs
      workflow.rs
      job.rs
      cache.rs
      vti.rs
      agents.rs
      bugs.rs
      git.rs
      bottlenecks.rs
      jankurai.rs
      runners.rs
      churn.rs
      security.rs
      artifacts.rs
      release.rs
      evidence.rs
      doctor.rs
      incident.rs
      replay.rs
    widgets/
      mod.rs
      table.rs
      tree.rs
      graph.rs
      sparkline.rs
      progress.rs
      log_view.rs
      inspector.rs
      action_modal.rs
      command_palette.rs
      filter_bar.rs
      event_tape.rs
      status_bar.rs
      tabs.rs
      breadcrumbs.rs
      forms.rs
      diff.rs
      heatmap.rs
    input/
      mod.rs
      keymap.rs
      commands.rs
      search.rs
      focus.rs
      mouse.rs
    render/
      mod.rs
      theme.rs
      layout.rs
      responsive.rs
      frame_budget.rs
      redaction.rs
      snapshots.rs
    graph/
      mod.rs
      dag.rs
      layout.rs
      routing.rs
      critical_path.rs
      schedule_sim.rs
    safety/
      mod.rs
      risk.rs
      preview.rs
      confirmations.rs
      redaction.rs
    tests/
      fixtures.rs
      harness.rs
```

### 8.3 Inspection client trait

```rust
#[async_trait::async_trait]
pub trait InspectionClient: Send + Sync {
    async fn snapshot(&self, scope: Scope) -> anyhow::Result<TuiReadModel>;
    async fn entity_detail(&self, entity: &EntityRef) -> anyhow::Result<EntityDetail>;
    async fn search(&self, query: &str, scope: Scope) -> anyhow::Result<Vec<SearchResult>>;
    async fn action_preview(&self, req: ActionPreviewRequest) -> anyhow::Result<ActionPreview>;
    async fn action_execute(&self, req: ActionExecuteRequest) -> anyhow::Result<ActionResult>;

    async fn pipeline_graph(&self, project_id: i64, pipeline_id: i64) -> anyhow::Result<PipelineGraph>;
    async fn job_trace_snapshot(&self, project_id: i64, job_id: i64, max_bytes: usize) -> anyhow::Result<TraceSnapshot>;
    async fn cache_object(&self, key: &str) -> anyhow::Result<CacheObjectDetail>;
    async fn release_detail(&self, release_id: &str) -> anyhow::Result<ReleaseDetail>;

    fn event_stream(&self, cursor: Option<u64>, filter: EventFilter) -> EventStream;
    fn job_trace_stream(&self, project_id: i64, job_id: i64, cursor: Option<String>) -> TraceStream;
}
```

Client implementations:

1. `HttpInspectionClient` — preferred.
2. `McpInspectionClient` — agent-compatible path.
3. `CliFallbackClient` — runs JSON CLI commands when API missing.
4. `FakeInspectionClient` — deterministic demo/development.
5. `ReplayInspectionClient` — event replay/incident mode.

### 8.4 App state

```rust
pub struct App {
    pub route_stack: Vec<Route>,
    pub focus: FocusState,
    pub selections: SelectionStore,
    pub filters: FilterStore,
    pub lenses: LensStore,
    pub store: EntityStore,
    pub event_ring: EventRing,
    pub traces: TraceStore,
    pub actions: ActionState,
    pub command_palette: CommandPaletteState,
    pub theme: Theme,
    pub config: TuiConfig,
    pub diagnostics: DiagnosticsState,
}
```

### 8.5 Event loop

Use a reducer architecture:

```text
Terminal input ─┐
Mouse input ────┼──> AppMsg -> reducer -> App state -> render
Backend events ─┤
Trace chunks ───┤
Timers ─────────┘
```

`AppMsg` families:

- `Input(KeyEvent)`
- `Mouse(MouseEvent)`
- `Tick(Instant)`
- `SnapshotLoaded(TuiReadModel)`
- `BackendEvent(TuiEvent)`
- `TraceChunk(TraceChunk)`
- `EntityDetailLoaded(EntityDetail)`
- `ActionPreviewLoaded(ActionPreview)`
- `ActionResultLoaded(ActionResult)`
- `SourceDegraded(DataSourceId, ErrorSummary)`
- `RenderBudgetExceeded`

### 8.6 Background tasks

- snapshot refresh task;
- event stream task with cursor resume and heartbeat;
- selected job trace stream task;
- entity detail prefetch task;
- source freshness monitor;
- action execution stream monitor;
- fake/demo event generator;
- replay clock;
- screenshot/capture renderer.

### 8.7 Rendering performance targets

| Target | Requirement |
|---|---:|
| Initial cached render | `<300 ms` |
| First live snapshot | `<2 s` local daemon |
| Static frame p95 | `<16 ms` |
| Log streaming frame p95 | `<33 ms` |
| Entities loaded | `10,000+` without UI freeze |
| Events | `1,000/min` without dropped interaction |
| Log throughput | `500 lines/sec` with coalescing |
| Command palette search | `<50 ms` for 10k objects |
| Memory trace cap | bounded per job and global |

Render rules:

- virtualize tables;
- diff render state where possible;
- coalesce backend bursts;
- avoid allocating per cell every frame;
- precompute widths and visible rows;
- keep log search index incremental;
- never block terminal input on network calls.

### 8.8 Graph layout algorithm

1. Normalize nodes by stage/lane.
2. Build edges from `needs`, bridge/child pipeline, artifacts, fallback stage order.
3. Annotate edge confidence: exact, inferred, stale.
4. Compute critical path using historical durations plus current remaining estimates.
5. Collapse completed-green subgraphs if narrow.
6. Route edges lane-to-lane with minimal crossings.
7. Preserve stable node positions across updates.
8. Center selected node on drilldown.

---

## 9. Backend plumbing roadmap

### P0 — make the existing TUI truthful

- Show source freshness everywhere.
- Mark inferred graph edges and heuristic ETA.
- Add doc/source drift warnings.
- Show DB backend and feature profile.
- Show stream vs poll mode.
- Add entity refs to all rows.
- Disable/guard risky merge paths in TUI until proof gate is enforced.

### P1 — unified read model API

- Expose `TuiReadModel`, `TuiEvent`, `EntityDetail`, `ActionPreview`, and `ActionResult` externally.
- Add schema versioning.
- Add fake/demo snapshot generator.
- Add `/api/v1/tui/snapshot`, `/events`, `/entity`, `/action/preview`, `/action/execute`.

### P2 — realtime streaming

- SSE/WebSocket event stream with cursor resume.
- Job trace stream.
- Action execution stream.
- Heartbeats and staleness.
- Coalescing and backpressure.
- MCP event watch/resources.

### P3 — workflow graph and capacity physics

- Real pipeline graph API with nodes/edges/child pipelines/artifacts.
- Critical path calculation.
- Multi-pipeline Flow Board.
- Queue/capacity model with theoretical vs usable slots.
- Runner/node telemetry.
- Schedule simulator.

### P4 — cache, VTI, and proof detail

- Rich cache endpoints: categories, hot, taints, verdicts, GC plan, provenance.
- VTI receipts, mapping health, selector-miss repair status.
- Artifact report parsing: JUnit, coverage, code quality, SAST, dependency/container scans, benchmark JSON.
- Searchable proof timeline.

### P5 — agents, bugs, races, autonomy

- Dedicated agent lifecycle tables.
- Agent tool call/log/message/artifact records.
- Race lifecycle APIs: status, poll result, winner selection, cleanup losers, promote winner.
- Bug project management MCP/API parity.
- Autonomy integration: kill bell, freeze, verdicts, launch ledger, Foundry, Nightwatch, LLM budget.

### P6 — Git/MR/PR and webhook ledger

- Active MR hook ingestion.
- Webhook delivery ledger with UUID, body hash, event type, topic/offset, handler outcome.
- GitHost parity resources for GitHub/GitLab PR state, diffs, checks, policy SHA, merge passport.
- Admission/capability/grant read APIs.

### P7 — security, artifacts, releases

- Signed artifacts and provenance read model.
- Release passport browser.
- Rollback target verification.
- Vault audit metadata safely redacted.
- Security normalization across scan types.
- Release/canary/production event stream.

### P8 — Doctor, metrics, replay, optimizer

- Deep health endpoint.
- Prometheus/OpenTelemetry metrics for main daemon.
- HTTP/OpenAPI live catalog.
- MCP registry and call audit stream.
- Outbound API tap.
- DB inspector.
- Worker/poller dashboard.
- Synthetic realtime self-test.
- CI flight recorder, replay, time-travel diff.
- Optimization recommendations and cost/trust lenses.

---

## 10. Safety overlays

### 10.1 Action preview modal

```text
╭─ PREVIEW ACTION: scale rust-large +4 ─────────────────────────────────────────────────────────────╮
│ Risk: R2 CI mutation                                                                              │
│ Target: pool rust-large, nodes node-2/node-3                                                       │
│ Why: p95 queue 7m42s, 42 queued, saturation 100%, projected p95 after scale 1m15s                 │
│ Planned writes: create_runner_manager x4, update desired count, emit capacity_plan event           │
│ External calls: GitLab runner registration x4, Docker create/start x4                              │
│ Safety: node-3 disk 91%; plan avoids node-3 unless GC succeeds                                     │
│ Dry run: supported                                                                                 │
│ Confirm: [Enter] execute  [d] dry-run JSON  [Esc] cancel                                           │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 10.2 Production rollback modal

```text
╭─ ACTION PREVIEW: rollback production ─────────────────────────────────────────────────────────────╮
│ Risk: R5 production/destructive                                                                   │
│ Repo: veox-api                                                                                    │
│ Current prod: 2.8.1 sha256:118a built from a19c88f, canary/prod telemetry failing                 │
│ Rollback target: 2.8.0 sha256:91cc built from 71cc903, last known good, prod verified 2026-05-25  │
│ Writes: deployment rollback, release_attempts row, launch ledger, evidence event                  │
│ Required grants: production_approval, no_self_approval, exact_sha_binding                         │
│ Blockers: none for rollback; human approval required                                              │
│ Type exactly: rollback veox-api to 91cc                                                           │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 10.3 Kill bell overlay

```text
╭─ KILL BELL / AUTONOMY PAUSE ──────────────────────────────────────────────────────────────────────╮
│ Current: armed, not paused                                                                        │
│ Pause stops autonomous merges/promotions/agent writes. Read-only monitoring continues.            │
│ Reason: __________________________  TTL: 30m / 2h / custom                                        │
│ [p] pause autonomy  [r] resume  [s] status/evidence  [Esc] cancel                                 │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

---

## 11. Testing strategy

### 11.1 Unit tests

- route stack push/pop;
- focus transitions;
- command parsing;
- filter syntax;
- table virtualization;
- entity indexing;
- event reducer idempotency;
- source freshness calculations;
- risk tier classification;
- redaction.

### 11.2 Golden render tests

Golden snapshots at `80x24`, `120x36`, `160x48`, and `220x60` for:

- Global healthy fleet;
- Global degraded sources;
- Queue saturation;
- Repo family `veox-*` hot;
- Repo dashboard;
- Workflow DAG;
- Live trace failure;
- Cache near full;
- VTI low confidence;
- Agent race;
- Bug detail;
- Git drift;
- Jankurai regression;
- Security critical;
- Artifact unsigned;
- Release blocked;
- Evidence timeline;
- Doctor warnings;
- Incident mode;
- Monochrome mode;
- Low-motion mode.

### 11.3 Interaction tests

Use Tuiwright or equivalent black-box terminal driving for:

- `Esc` always goes up one level;
- from Global to selected job log in ≤2 keypresses when in attention rail;
- from Global to release blocker proof in ≤3 keypresses;
- command palette route search;
- filter and sort persistence;
- action preview cancellation does not mutate;
- high-risk confirmation requires exact typed text;
- selected row does not jump during event bursts.

### 11.4 Event replay tests

Record/replay streams for:

- job starts/runs/fails/retries/succeeds;
- runner OOM and reconciliation;
- cache threshold crossing;
- VTI selector miss;
- agent patch race;
- release canary block and rollback;
- stale source and stream reconnect;
- MR hook ingestion;
- webhook duplicate delivery.

### 11.5 Backend contract tests

- snapshot schema version compatibility;
- event cursor monotonicity;
- entity detail for every entity kind;
- action registry risk/side-effect/grant correctness;
- MCP resources mirror HTTP read model;
- CLI fallback JSON parity;
- redacted settings contains no secrets.

### 11.6 Performance/load tests

- 100 repo families, 1,000 repos, 10,000 jobs, 100,000 events.
- 500 log lines/sec to selected trace.
- 1,000 events/minute event stream.
- 10k search results indexed.
- Re-render under budget.
- Memory stable under long-running session.

### 11.7 Safety tests

- Secret values never appear in snapshots, logs, exports, screenshots, traces, or golden files.
- R4/R5 actions require proof/grants/typed confirmations.
- Stale read model blocks high-risk actions.
- Agents cannot approve their own work.
- Merge requires exact SHA and gate proof.
- Production promote requires rollback target.
- Cache GC preview distinguishes leased/tainted/hot objects.

---

## 12. Acceptance criteria

The TUI is done only when all of these are true.

### UX acceptance

- Operator can understand global state in under five seconds.
- Any top-level screen reachable in one key.
- Any visible entity drillable with `Enter`.
- `Esc` always returns one level up without losing selection.
- From Global to hot job log: ≤2 keypresses if in attention rail; ≤4 through search.
- From Global to release blocker proof: ≤3 keypresses.
- Every warning explains itself and links to evidence.
- Every screen useful at `120x36`; core operations still work at `80x24`.
- Monochrome/colorblind mode preserves status through glyph/text.

### Data acceptance

- Every panel shows source freshness.
- Event stream disconnect shows degraded state within 5 seconds.
- Inferred graph edges and heuristic ETAs are labeled.
- The Flow page supports multiple active pipelines.
- Cache categories include Rust crates/Cargo/sccache/OCI/artifacts/action/material objects.
- VTI skipped tests always have reason/confidence/receipt.
- Agents have lifecycle history, not just current guesses.
- Evidence is searchable by entity, actor, SHA, time, event type, and correlation id.

### Performance acceptance

- Initial cached render `<300ms`.
- First live snapshot `<2s` local daemon.
- Static p95 frame `<16ms`; live log p95 frame `<33ms`.
- 10k entities and 1k events/minute without input freeze.
- Table filtering and command palette feel instant for 10k objects.

### Safety acceptance

- No R4/R5 action executes without preview, proof, grants, and explicit confirmation.
- `request_merge` is proof-gated in the UI even if backend path is still being corrected.
- Production release/promote/rollback shows exact SHA, artifact digest, gate state, and rollback target.
- Secrets are redacted everywhere.
- Action execution writes audit/proof events.

---

## 13. Implementation phases

### Phase 0 — Contract and demo foundation

- Define TUI model types.
- Build fake/demo backend.
- Implement route stack, focus, keymap, command palette skeleton.
- Render shell, header, tabs, footer, breadcrumbs.
- Build redaction library and tests.

### Phase 1 — Global, Queue, Repos

- Global Flight Deck with fake data.
- Queue theoretical-limit model and simulator.
- Repo family/repo atlas.
- Attention rail and event tape.

### Phase 2 — Workflow and logs

- Pipeline graph data model.
- DAG widget.
- Job inspector.
- Live trace viewer with fake and polling fallback.
- Failure capsule/evidence links.

### Phase 3 — Cache, VTI, Runners

- Cache Observatory.
- VTI cockpit.
- Runner/system utilization.
- Capacity formulas wired to real/fake data.

### Phase 4 — Agents, Bugs, Git

- Agent cockpit and lifecycle model.
- Bug board and detail.
- Git sync/MR/admission view.
- Agent config editor preview flow.

### Phase 5 — Quality, Security, Artifacts, Release

- Jankurai Audit Center.
- Security/policy/secrets screen.
- Signed artifacts/provenance.
- Release/rollback control.

### Phase 6 — Evidence, Doctor, Replay

- Proof ledger search.
- Source Doctor/runtime/API/MCP/DB/worker views.
- Incident mode.
- Replay/time-travel mode.
- Demo/capture/golden fixtures.

### Phase 7 — True realtime and backend unification

- HTTP/SSE read model.
- Event/log streaming.
- MCP resources/watch.
- Action preview/execute API.
- Source freshness across all screens.

### Phase 8 — Optimizer and superpowers

- CI optimizer recommendations.
- Cost lens.
- Trust lens.
- Flake intelligence.
- Agent leaderboard.
- Policy simulator.
- Merge train radar.
- Natural-language “explain screen” backed strictly by proof ledger.

---

## 14. Extra superpowers worth building

### CI flight recorder

Record every event, action, log cursor, and snapshot delta so incidents can be replayed.

### Time-travel state diff

Compare now vs a past cursor and explain changes in queue, cache, VTI, Jankurai, runners, agents, and release gates.

### Optimizer recommendations

Rank candidate improvements by estimated time saved, confidence, cost, and risk:

- split job;
- add runner;
- prewarm cache;
- quarantine flake;
- repair VTI mapping;
- move job off critical path;
- cancel obsolete pipelines;
- downscale idle pools;
- adjust tags/trust tiers.

### Flake command center

Track flaky tests/jobs with flake rate, owner, quarantine state, last real failure, and VTI skip policy.

### Agent quality leaderboard

Track bugs completed, failed attempts, reverted commits, proof completeness, VTI misses caused, Jankurai regressions, token cost, and cycle time. Use it as a coaching/quality tool, not vanity.

### Policy simulator

Before changing policy/autonomy config, show historical impact:

```text
This policy would have blocked 7 merges, allowed 2 currently-blocked merges, and required 3 extra approvals in the last 30d.
```

### Merge train radar

Show branch/MR/proof/merge queue as a live railroad diagram with exact SHA validity and conflict risk.

### Cost lens

Optional overlay for runner dollars, LLM spend, storage cost, wasted compute, cost per release, cost per bug fix.

### Trust lens

For every release/artifact/cache/test decision, show:

```text
commit -> VTI receipt -> CI jobs -> Jankurai receipt -> SBOM -> provenance -> signature -> release passport -> canary telemetry -> prod
```

### Terminal pair mode

Read-only shared session for incident response, cursor sharing, copied route IDs, proof bundle sharing.

### Natural-language explain screen

Allow questions like:

```text
Explain why we are not at theoretical limit.
Explain why VTI escalated veox-core.
Explain whether it is safe to merge MR !117.
```

The answer must be generated only from the proof/read model and must cite internal entity/proof references, never inventing missing data.

---

## 15. Concrete build checklist

1. Define `EntityRef`, `EntityKind`, `TuiEvent`, `TuiReadModel`, `EntityDetail`, `ActionDescriptor`, `ActionPreview`, and `ActionResult`.
2. Build `FakeInspectionClient` and deterministic dream fixture set.
3. Implement terminal setup/restore and panic-safe teardown.
4. Implement app reducer, route stack, focus model, selection store, and filter store.
5. Render global shell: header, tab bar, left rail, center workspace, right inspector, bottom strip.
6. Implement command palette and fuzzy search over fake entities/actions/routes.
7. Implement generic table, tree, progress, sparkline, event tape, inspector, log view, and action modal widgets.
8. Implement Global Flight Deck.
9. Implement Queue Lab formulas and simulator.
10. Implement Repo Atlas and Repo Dashboard.
11. Implement Workflow DAG widget and graph layout.
12. Implement Job Trace viewer.
13. Implement Cache, VTI, Runners.
14. Implement Agents, Bugs, Git.
15. Implement Jankurai, Security, Artifacts, Release.
16. Implement Evidence and Doctor screens.
17. Add screenshot/capture/demo modes.
18. Add golden render tests across terminal sizes.
19. Add event replay tests.
20. Add redaction and safety tests.
21. Wire `HttpInspectionClient` to real unified API.
22. Wire SSE/log streaming.
23. Wire action preview/execute.
24. Add MCP resource fallback.
25. Enable mutating actions only after preview/safety test suite passes.

---

## 16. Final north star

**JeRyu Flight Deck should make a large human-plus-agent engineering system feel like one fast, safe, explainable machine: every repo, job, test skip, cache hit, runner slot, agent action, bug, merge, artifact, release, rollback, security finding, and proof receipt visible, drillable, replayable, and safely actionable from a single Rust TUI.**
