# JeRyu Flight Deck — Final Rust TUI Engineering Specification

**Date:** 2026-05-26  
**Artifact:** `jeryu_final_rust_tui_engineering_spec.md`  
**Target:** A world-class Rust terminal control plane for JeRyu / Veox-style multi-repo CI, agents, runner fleets, SmartCache, VTI, releases, evidence, security, and autonomous workflows.  
**Primary user:** A developer/operator managing many repositories and repo families, including shared families such as `veox-*` and isolated repos, who needs to see everything moving in realtime and answer: _What is happening? Why is it blocked? Are we near the machine limit? Should I add runners? What can safely be done now?_

---

## 0. Source basis and synthesis stance

This document synthesizes every non-AppleDouble `.md` and `.txt` file in the uploaded archive:

- Prior design/spec documents:
  - `jeryu_dream_rust_tui_engineering_spec.md`
  - `jeryu_dream_rust_tui_engineering_spec(1).md`
  - `jeryu_dream_rust_tui_engineering_spec(2).md`
  - `jeryu_dream_rust_tui_spec.md`
  - `jeryu_dream_rust_tui_spec(1).md`
  - `jeryu_dream_tui_engineering_spec.md`
  - `jeryu_dream_tui_engineering_spec(1).md`
  - `jeryu_dream_tui_engineering_spec(2).md`
- API/realtime inventory notes:
  - `tip1.txt` through `tip9.txt`

The `.txt` files describe the discovered JeRyu API, MCP, CLI, HTTP, webhook, GitLab, Docker, SmartCache, Vault, bug, release, state DB, autonomy, and TUI read-model surfaces. The `.md` files propose overlapping versions of the dream Rust TUI. This final spec merges them into one build-ready design and resolves the major repeated themes:

1. The TUI must be a **fleet flight deck**, not a collection of static dashboard tabs.
2. The user must see **live cross-repo queue pressure**, **runner/node/core/memory saturation**, **cache pressure**, **VTI correctness**, **agent activity**, **bugs**, **Git sync**, **releases**, **Jankurai quality**, **security**, **artifact provenance**, and **evidence** in one coherent operating model.
3. Every displayed number needs provenance, freshness, and drilldown.
4. Every warning needs a reason, confidence, source, and next safe action.
5. The system must constantly answer whether more runners will help or whether the real bottleneck is serial DAG structure, tags, cache, VTI, policy, disk, memory, GitLab, agents, release gates, or security.
6. Realtime motion should be useful: animated activity must reveal flow, contention, and state changes without harming trust or readability.

---

## 1. Product north star

**JeRyu Flight Deck lets one developer operate a fleet of repositories, runners, agents, caches, tests, releases, and proofs at production-control-room speed from a keyboard-first Rust TUI.**

The default view should make a skilled operator feel like they are looking at an air-traffic-control screen for engineering work:

- every repo family visible;
- every hot repo pulsing with live state;
- every job flowing through lanes;
- every queue explained by constraint;
- every runner pool showing online/theoretical/effective capacity;
- every cache, VTI, agent, release, and security warning tied to evidence;
- every drilldown reachable with `Enter`, reversible with `Esc`, and explainable with `x` or `?`.

The goal is not simply “pretty terminal UI.” The goal is an operating console that reduces the time from **fleet confusion** to **correct action** to under ten seconds.

---

## 2. Source-derived backend reality

### 2.1 Existing control-plane surfaces

JeRyu is described by the inventories as a single Rust control plane with these relevant surfaces:

| Surface | Entrypoint / transport | Data or control exposed |
|---|---|---|
| CLI | `jeryu <command>` | Install, serve, remote, node, TUI, Git wrapper, repo/fleet, status, pools, jobs, pipelines, cache, logs, agents, settings, tests, release, secrets, progress, bugs, policy, host, MCP, next action, blocker explanations, action registry. |
| Existing TUI | `jeryu tui` | Mission, Workflow/Delivery, Jobs/Flow, Release, Pools, Cache, Evidence, Tests, Agents, Secrets, LLMs, Git, Bugs depending on source vintage. |
| MCP stdio | `jeryu mcp serve` | JSON-RPC MCP tools over stdin/stdout. |
| MCP loopback HTTP | `jeryu mcp serve-http`, default `127.0.0.1:9778`, `POST /mcp` | Same tool surface over local HTTP. GET is currently disabled; DELETE terminates sessions. |
| Webhook/API engine | `jeryu serve`, default `127.0.0.1:9777` | `/health`, `/hooks`, `/cache/summary`, GitLab Job/Pipeline/Push hooks, cache summary. |
| Capability API | Unix socket | Length-framed capability requests, agent intents, grants, envelopes, nonces, budgets, responses. |
| GitLab REST wrapper | Internal `GitlabClient` | Projects, jobs, traces, artifacts, pipelines, downstream pipelines, variables, runners, runner managers, MRs, issues, branches, webhooks. |
| GitLab webhooks | `/hooks` | Job, Pipeline, Push; MR hooks accepted/logged but not fully acted on in the inventories. |
| Broker/message log | Kafka or Jansu feature-gated | Topics such as `jeryu.webhook.jobs`, `jeryu.webhook.pipelines`, `jeryu.webhook.pushes`. |
| Custom executor | `jeryu exec config/prepare/run/cleanup` | GitLab Runner custom executor lifecycle, sandbox state, job env, logs, tripwires, failure capsules. |
| Git pre-receive hook | `jeryu server-hook pre-receive` | Ref update admission, actor kind, grants, policy verdicts, denials. |
| SmartCache / gateway | Proxy default `19800`, OCI registry mirror default `19801` | Cargo sparse config, crate downloads, CAS hits, singleflight, CONNECT proxy metrics, cache DB records. |
| Docker control plane | Bollard + compose | Managed runner containers, lifecycle, logs, Docker events, OOM/die detection. |
| Vault/secrets | Vault HTTP API | Vault health, init/unseal, KV v2 mount, policies, rotation outputs/audit. |
| State DB | SQLite default, RedlineDB optional | Durable source of pools, managers, job events, pipelines, releases, evidence, cache, grants, bugs, LLM budgets, VTI, etc. |
| Autonomy binary | `autonomy ...`; HTTP `/metrics`, `/health`, `/events` | Evidence Gate/VibeGate workflows, kill bell, freeze windows, foundry, canary, rollback drills, ledger replay, shadow, LLM providers. |

### 2.2 Current MCP tool set to preserve and extend

The inventories repeatedly identify the current source-of-truth MCP/capability tools as these 16 tools under `jeryu.`:

| Tool | Read/write | Purpose |
|---|---:|---|
| `jeryu.fetch_capsule` | Read | Latest structured failure/evidence capsule for a job. |
| `jeryu.get_system_snapshot` | Read | GitLab readiness, pool count, recent job events, latest release attempt. |
| `jeryu.get_pipeline_jobs` | Read | Downstream-expanded pipeline job list with status/stage/timing/runner details. |
| `jeryu.get_ci_bottlenecks` | Read | Historical bottleneck rows by job/stage/pool/duration. |
| `jeryu.explain_blockers` | Read | Blocker explanation for job/release/merge entities. |
| `jeryu.plan_validation` | Read | VTI/test-plan validation against selector misses. |
| `jeryu.run_tests` | Write | Creates/request test execution path via ephemeral branch/CI. |
| `jeryu.propose_patch` | Write | Creates branch, commits modifications, opens MR, records grant. |
| `jeryu.race_patches` | Write | Launches competing patch hypotheses/branches and pipelines. |
| `jeryu.request_merge` | Write / production-risk | Requests or accepts merge through GitLab/capability path. |
| `jeryu.bug_submit` | Write | Submits canonical local bug report. |
| `jeryu.bug_list` | Read | Lists local bug records. |
| `jeryu.bug_show` | Read | Shows bug with events and attempts. |
| `jeryu.bug_ready` | Read | Lists ready bugs with failed-attempt filtering. |
| `jeryu.bug_update` | Write | Updates triage fields. |
| `jeryu.bug_record_attempt` | Write | Appends bug attempt history. |

The TUI must use this as a trusted action foundation, but it should not be limited by the current tool list. The dream design requires read-only MCP resources, a unified read-model endpoint, realtime event streams, and additional safe read tools for pools, nodes, cache, releases, secrets, policies, settings, and host health.

### 2.3 Durable state families available to inspect

The inventories identify the state DB as the broadest local truth source. The TUI should assume the following durable data families either exist now or should be normalized into the read model:

| Family | Representative records / facts |
|---|---|
| Runner fleet | Pools, managers, GitLab runner IDs, tags, executor, min/max managers, concurrency, request concurrency, paused state, trust tier, backend type, remote cluster alias, manager state, container/pod ID, system ID, node alias. |
| CI/job timeline | Job events, CI job runs, tracked pipelines, pipeline/ref/SHA mapping, queue/duration/start/finish, runner/pool/system attribution, root/downstream mapping. |
| Evidence/recovery | Evidence capsules, retry decisions, append-only events, failure kind/classification/stage/exit code/ref/commit/payload. |
| Capability/admission | Capability intents, grants, nonces, expiry, exact SHA binding, admission decisions, pre-receive verdicts/reasons. |
| Git wrapper audit | Git command events, ref updates, mirror jobs, risk approvals, artifacts, argv hashes, command class, dirty state, before/after branch/head. |
| Repositories | Tracked repositories, aliases, provider, remote, local root, default branch, health profile, project graph. |
| Release | Release attempts, foundry candidates, verdicts, project/ref/SHA/version, release/prod pipeline IDs/status, canary state, gates, eligibility, evidence paths. |
| Secrets | Secret authorities, release secret sets, secret audit events, Vault metadata, token fingerprints, mount/prefix/path metadata, expiry, rotation/finalization. |
| Cache/provenance | Cache objects, requests, hot entries, build/image signatures, force-refresh rules, resolved refs, taints, leases, verdicts, promotions, material objects, aliases, action cache, cache epochs, toolchain fingerprints. |
| Test intelligence/VTI | Test executions, plans, plan items, selector misses, selected/skipped counts, mode, confidence, durations, subsystems, escalation reasons, per-test actions/reasons. |
| Bug tracker | Projects, graph edges, bugs, bug events, attempts, links, external refs, evidence, status/severity/priority/difficulty/repro/impact/security/owner. |
| Autonomy governance | Launch ledger, kill bell, verdicts, foundry queue, freeze windows, canary/rollback evidence, LLM budget ledger. |
| LLM budget/provider | Prompt/completion tokens, micro-USD, provider/model/latency/status/key source/user where safely available. |
| Host/remote/node | Node configs, SSH target, Docker socket, storage limits, runner data/cache dirs, max managers, pool affinity, enabled flag, storage reports, GC history. |

### 2.4 Source/docs drift to handle explicitly

The TUI must be honest about known drift reported by the inventories:

1. Some docs describe RedlineDB-only state, but the current source inventory says SQLite is the default and RedlineDB is optional.
2. Older docs list fewer MCP tools than the current action registry.
3. Existing TUI screens are useful but have limitations: no WebSocket transport, polling-based live logs, first-active-pipeline bias in the flow board, incomplete graph edges, heuristic ETA, incomplete searchable evidence timeline, and no dedicated agent lifecycle table.
4. Merge Request hooks are accepted/logged but not fully acted on.
5. `/cache/summary` is too small for the desired cache cockpit.
6. `race_patches` launches work, but winner selection / losing-branch cleanup needs first-class lifecycle support.
7. Main daemon health is shallow compared with what the flight deck requires.
8. Autonomy exists partly as a parallel universe and must be unified into the main TUI/MCP/read model.

---

## 3. Non-negotiable product laws

### 3.1 Every visible thing is addressable

Anything rendered on screen must have an `EntityRef` behind it. Pressing `Enter` on it drills down. Pressing `Esc` goes up. Pressing `e` opens evidence. Pressing `x` explains. Pressing `?` shows local actions.

Addressable entities include:

- repo family;
- repo;
- branch/ref;
- commit/SHA;
- MR/PR;
- pipeline;
- pipeline graph node;
- job;
- log line annotation;
- artifact;
- release attempt/gate;
- runner pool;
- runner manager;
- remote node;
- Docker container;
- cache object/category/taint/verdict;
- VTI plan/test/selector miss;
- agent/session/task/step/intent/grant;
- bug/attempt/evidence;
- Jankurai audit/finding/control;
- secret authority/secret set/audit event;
- admission decision;
- capability grant;
- LLM call/budget row;
- event/proof capsule.

### 3.2 Every warning explains itself

A red or yellow cell must include:

- short label, such as `QUEUE SATURATED`, `VTI MISS`, `CACHE TAINT`, `AGENT BLOCKED`, `SECRET TTL`, `UNSIGNED`, `MR DRIFT`, `OOM`, `DISK PRESSURE`;
- cause line;
- source and freshness;
- confidence;
- next safe action;
- linked evidence/log/config/table row.

The user should never have to ask “why is this red?” and then manually hunt across tabs.

### 3.3 Every number has provenance

Numbers must show where they came from and how fresh they are:

```text
Cache hit ratio 82.4%  source=cache_requests+cache_verdicts  window=1h  age=1.2s
Runner saturation 91%  source=DB managers + Docker stats + GitLab jobs  age=0.8s
Limit distance 1.34×  source=capacity simulator  model=p50_7d  confidence=0.81
```

When data is stale, show it as stale, not as current. When data is estimated, label it as estimated. When model confidence is low, display that clearly.

### 3.4 Stream first; poll only as fallback

The dream TUI should consume an event stream for changes and only poll for compatibility/fallback. Target order:

1. WebSocket or SSE for `TuiEvent` stream.
2. Bounded log streaming for job traces.
3. MCP resources/watch for agent-compatible observation.
4. HTTP read-model snapshots for initial state and resync.
5. CLI JSON/polling fallback for development, demo, or degraded mode.

### 3.5 No modal dead ends

Every detail view must have at least one of:

- parent breadcrumb;
- related entities;
- evidence/proof;
- local actions;
- source URL/path;
- copy/export command;
- explanation.

### 3.6 High motion, low deception

Animation is allowed and encouraged when it communicates change:

- flowing particles on active job lanes;
- pulse on newly changed rows;
- flicker-free progress bars;
- color intensity proportional to severity/freshness;
- moving queue conveyor when ready jobs are waiting;
- runner pool heat shimmer when saturated;
- agent race lanes with branch contenders moving toward gates.

Animation must not imply live freshness if the source is stale. Stale data freezes and gets a visible `STALE` badge.

---

## 4. Operating mental model

The entire app uses five nested levels:

```text
Fleet
  └── Repo family
        └── Repo
              └── Domain view / workflow / pipeline / board
                    └── Entity detail / evidence / action
```

The user should be able to traverse this spatially:

- `Tab` / `Shift+Tab`: move between panes.
- `←/→`: move laterally between sibling panes or graph nodes.
- `↑/↓`: move vertically within the focused list/graph.
- `Enter`: drill into selected entity.
- `Esc`: move up/back.
- `[` / `]`: previous/next scope sibling.
- `g` then key: jump to top-level screen.
- `/`: filter focused pane.
- `:`: command palette.
- `x`: explain selected object.
- `e`: evidence/proof for selected object.
- `a`: action palette for selected object.
- `Space`: expand/collapse or pin depending on context.

The app should feel like a fast graph browser, not a tabbed web dashboard squeezed into a terminal.

---

## 5. Information architecture

### 5.1 Primary screens

Use numeric tabs and mnemonic go-to shortcuts. The default order puts the most urgent cross-fleet questions first.

| # | Screen | Shortcut | Core question |
|---:|---|---|---|
| 0 | **Flight Deck** | `g f` | Is the whole engineering machine healthy, fast, and safe? |
| 1 | **Queue / Limit** | `g q` | How close are we to theoretical throughput? Should we add runners? |
| 2 | **Repos** | `g r` | Which repo families and repos need attention? |
| 3 | **Workflow Atlas** | `g w` | What pipelines/jobs are flowing, blocked, failed, or critical? |
| 4 | **Runners / System** | `g s` | Are cores, memory, disk, containers, and nodes saturated? |
| 5 | **Cache** | `g c` | Is cache full, healthy, trusted, and saving time? |
| 6 | **VTI Tests** | `g t` | Is smart test skipping safe and effective? |
| 7 | **Agents / Autonomy** | `g a` | What are agents doing, blocked on, spending, or changing? |
| 8 | **Bugs / Issues** | `g b` | What bugs exist across repos and who/what is working them? |
| 9 | **Git Sync / MR** | `g g` | Are branches, remotes, mirrors, MRs, and main in sync? |
| 10 | **Release / Rollback** | `g p` | What is safe to ship, promote, or roll back? |
| 11 | **Evidence** | `g e` | What proofs explain this system state? |
| 12 | **Security / Secrets / Artifacts** | `g z` | Are gates, secrets, signatures, SBOMs, and policies safe? |
| 13 | **Jankurai Quality** | `g j` | What code-audit scores, caps, repair queues, and proofs exist? |
| 14 | **Churn / Velocity** | `g v` | What code change volume correlates with failures or risk? |
| 15 | **Source Doctor / Settings** | `g d` | Are sources, configs, credentials, versions, and streams healthy? |

### 5.2 Persistent shell regions

All screens share the same high-level shell:

```text
╭─ JeRyu Flight Deck ─ fleet:all ─ profile:prod ─ cursor:18,442 ─ fresh:0.9s ─────────────╮
│ POSTURE RAIL: safe-to-code ✓  safe-to-merge ⚠  safe-to-release ✗  kill-bell armed ✓   │
├─ tabs / breadcrumbs / scope stack ──────────────────────────────────────────────────────┤
│ left navigator │ main workspace / graph / table / board │ right inspector / next action│
├─ event tape / alert ticker / command hints / source freshness ──────────────────────────┤
╰─ keys: Tab pane  Enter drill  Esc up  / filter  : command  x explain  a actions  ? help╯
```

Persistent elements:

- **Header posture rail:** health, speed, trust, safety, kill bell, active incident, freshness.
- **Breadcrumb/scope stack:** `Fleet › veox-* › veox-enclave › pipeline #1821 › job test:linux`.
- **Left navigator:** scope-aware repo families, repos, queues, or domain objects.
- **Main workspace:** current selected screen.
- **Right inspector:** selected entity summary, proof, next action, local graph, action buttons.
- **Bottom event tape:** recent high-signal events, new failures, grants, cache taints, selector misses, runner deaths.

### 5.3 Responsive breakpoints

| Width | Layout |
|---:|---|
| `>= 180` cols | Three-pane cockpit: navigator + wide workspace + inspector. Rich graphs. |
| `140-179` cols | Three-pane compact; smaller inspector and fewer inline sparkbars. |
| `110-139` cols | Two-pane layout; inspector toggles with `i`. |
| `80-109` cols | Single focused pane; tabs become stacked compact labels. |
| `<80` cols | Emergency compact mode: posture, attention list, selected detail only. |

Height rules:

- `<28` rows: hide noncritical charts and compress bottom tape.
- `28-45` rows: default layout.
- `>45` rows: add secondary timelines, logs, and historical sparklines.

### 5.4 Density modes

`D` cycles density:

- **Calm:** fewer colors, more whitespace, summaries.
- **Ops:** default high-information cockpit.
- **Scream:** maximum realtime detail, compressed rows, event tape, high-intensity motion.
- **Capture:** no animation, stable deterministic render for screenshots/tests.

---

## 6. Visual system

### 6.1 Semantic color palette

Use semantic colors, not arbitrary decoration. Provide truecolor theme plus 256-color fallback and monochrome glyph fallback.

| Meaning | Visual treatment |
|---|---|
| Healthy/success | Green / emerald; steady. |
| Running/active | Cyan/blue; animated pulse or moving bar. |
| Queued/waiting | Amber; queue conveyor. |
| Blocked | Orange/red; hard border, reason badge. |
| Failed | Red/magenta; high contrast, no subtlety. |
| Risk/security | Purple/magenta; shield/lock glyphs. |
| Evidence/proof | Teal/blue; receipt glyph. |
| Stale/unknown | Gray; frozen, dimmed, `STALE` label. |
| Mutating action | Yellow/orange; confirmation affordance. |
| Production action | Red border + typed confirmation. |

### 6.2 Glyphs

| Glyph | Meaning |
|---|---|
| `●` running / active |
| `○` queued / pending |
| `✓` passed / healthy |
| `✗` failed |
| `!` warning |
| `⛔` denied / blocked |
| `⚡` high activity / optimization |
| `⧗` waiting / ETA |
| `◆` proof/evidence |
| `◇` estimated/model-derived |
| `⬢` cache object |
| `⬡` cache taint/trust item |
| `♟` agent |
| `⛨` security/policy |
| `🔒` secret/signature, with ASCII fallback `LOCK` |
| `↻` retry/requeue |
| `⇡` scale up |
| `⇣` drain/scale down |

### 6.3 Progress bars

Progress bars encode two dimensions:

```text
██████░░░░  61%         normal progress
████▒▒▒░░░  61%         estimated / confidence band
████!!░░░░  61%         blocked region
██████▸░░░  live        moving cursor for active job
```

Job progress should be computed from stage/job historical timing if no native progress exists. Always label estimated progress with `◇` or confidence.

### 6.4 Motion grammar

| Motion | Meaning |
|---|---|
| Slow pulse | Active but normal. |
| Fast pulse | Urgent active event or recent status change. |
| Moving lane particles | Work flowing through DAG/queue. |
| Flicker-free bar advance | Estimated progress. |
| Frozen dim state | Data stale or disconnected. |
| Expanding ring | New event focus; disappears after a few seconds. |
| Shaking border | Mutating action failed or blocked by policy. |

Motion must be disabled in capture mode and reduced when terminal performance is poor.

---

## 7. Keyboard and interaction model

### 7.1 Universal keys

| Key | Action |
|---|---|
| `Tab` / `Shift+Tab` | Next/previous pane. |
| Arrow keys | Move selection within focused pane or graph. |
| `Enter` | Drill into focused entity. |
| `Esc` | Back/up. Never quits unless already at root and confirmed. |
| `Backspace` | Back one navigation stack entry. |
| `/` | Filter focused pane. |
| `:` | Command palette. |
| `?` | Context help / available actions. |
| `x` | Explain selected entity/screen. |
| `e` | Evidence/proof for selected entity. |
| `a` | Action palette for selected entity. |
| `o` | Open source URL/path if safe. |
| `p` | Pin/unpin selected entity to watch panel. |
| `Space` | Expand/collapse row, graph group, or stage. |
| `c` | Center selected graph object. |
| `C` | Critical-path-only toggle in graph views. |
| `f` | Focus mode for selected pane. |
| `D` | Cycle density. |
| `r` | Refresh/resync current scope. |
| `R` | Replay/time-travel current scope. |
| `y` | Copy ID/URL/summary/evidence hash. |
| `q` | Open quit confirmation only from root. |

### 7.2 Numeric tabs

`0`-`9` open the first ten primary screens. `Alt+number` opens extended screens. On terminals that intercept Alt, use `g` prefixes.

### 7.3 Graph-specific keys

| Key | Graph action |
|---|---|
| `←/→` | Move to upstream/downstream node. |
| `↑/↓` | Move within stage/lane. |
| `[` / `]` | Previous/next pipeline. |
| `{` / `}` | Previous/next repo in family. |
| `Space` | Expand/collapse stage/subgraph. |
| `C` | Critical path only. |
| `L` | Toggle live logs split for selected job. |
| `E` | Toggle evidence overlay. |
| `M` | Toggle merge/release gates overlay. |
| `T` | Toggle VTI/test overlay. |

### 7.4 Command palette

Command palette syntax examples:

```text
:go veox-enclave
:repo veox-deploy
:job 981273 trace
:why not green
:explain queue limit
:scale rust-fast +4 dry-run
:cache gc plan --repo veox-api
:vti misses --7d
:agent pause all --reason "release freeze"
:bug ready veox-enclave
:release rollback prod --dry-run
:evidence sha b13c9a1
:filter repo:veox-* status:failed age:<1h
```

Every palette result row includes:

```text
[label] [scope] [risk] [freshness] [preview available?] [shortcut]
```

No mutating palette command executes immediately. It opens an action preview.

---

## 8. Core data architecture

### 8.1 Golden rule

The TUI must not rebuild business truth differently from CLI/MCP/agents. It should consume a unified typed inspection plane.

Current backend state exists in DB/GitLab/Docker/Vault/cache/filesystem/autonomy. The dream architecture exposes a single normalized read and event model:

```text
GitLab webhooks/API ─┐
State DB ────────────┤
Docker/remote nodes ─┤
SmartCache ──────────┤
Vault/secrets ───────┤──► Inspection Aggregator ─► HTTP read model
Autonomy ledger ─────┤                         ├─► SSE/WS events
Git/admission ───────┤                         ├─► MCP resources/tools
Jankurai artifacts ──┤                         └─► CLI JSON
Host sys metrics ────┘
```

### 8.2 Required HTTP endpoints

Add these to the main daemon or an inspection sidecar:

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/read-model` | Complete `TuiReadModel` snapshot for current scope. |
| `GET` | `/api/events?cursor=N&scope=&kinds=` | Incremental event stream catch-up. |
| `GET` | `/api/entity/{kind}/{id}` | Full entity detail with timeline, blockers, evidence, related entities, actions. |
| `GET` | `/api/logs/job/{project_id}/{job_id}?cursor=&limit=` | Bounded log chunks with annotations. |
| `GET` | `/api/pipeline/{project_id}/{pipeline_id}/graph` | Multi-pipeline DAG including bridges/downstream and computed edges. |
| `GET` | `/api/capacity?scope=&window=` | Capacity/theoretical-limit snapshot and recommendation model. |
| `GET` | `/api/runners?scope=` | Pools, managers, nodes, telemetry, constraints. |
| `GET` | `/api/cache/metrics` | Cache categories, hit/miss, bytes, taints, hot objects, GC plan. |
| `GET` | `/api/vti/summary?scope=&window=` | VTI efficacy, misses, saved time, confidence. |
| `GET` | `/api/agents?scope=` | Agent lifecycle table, sessions, tasks, grants, logs, races. |
| `GET` | `/api/bugs?scope=&status=` | Cross-repo bug board. |
| `GET` | `/api/git-sync?scope=` | Local/remote/mirror/MR/ref/admission state. |
| `GET` | `/api/jankurai?scope=` | Audit scores, caps, findings, repair queue, ownership, proof lanes. |
| `GET` | `/api/security?scope=` | Policies, secret metadata, grants, denials, scan findings. |
| `GET` | `/api/artifacts?scope=` | Signatures, SBOM, provenance, artifact trust state. |
| `GET` | `/api/release?scope=` | Release attempts, gates, canary, prod, rollback paths. |
| `GET` | `/api/evidence?entity=&kind=&actor=&since=&sha=` | Searchable proof ledger. |
| `GET` | `/api/source-doctor` | Component health, freshness, config, versions, dependencies. |
| `POST` | `/api/action/preview` | Mutating action preview with side effects and required grant. |
| `POST` | `/api/action/execute` | Execute approved action with idempotency key. |

### 8.3 Streaming endpoints

Add one or more of:

```text
GET /api/stream/events                # SSE stream of TuiEvent
GET /api/ws                           # WebSocket multiplexed events/logs/actions
GET /api/stream/job/{project}/{job}   # bounded job log stream
GET /api/stream/action/{action_id}    # action execution progress
GET /api/stream/replay?since=...      # deterministic replay stream
```

Event stream must support:

- cursor-based resume;
- backpressure;
- heartbeat;
- schema version;
- source timestamps and ingest timestamps;
- entity references;
- redaction of sensitive fields;
- deduplication keys;
- ordered catch-up after reconnect.

### 8.4 MCP resources to add

Current MCP is tool-centric. Add read-only resources so agents can observe safely:

| Resource URI | Data |
|---|---|
| `jeryu://system/snapshot` | Current fleet read model summary. |
| `jeryu://events?cursor=N` | TUI event stream. |
| `jeryu://entity/{kind}/{id}` | Entity detail. |
| `jeryu://repos` | Repo families and repo summaries. |
| `jeryu://queue/capacity` | Theoretical/effective capacity. |
| `jeryu://runners` | Pools, managers, nodes, telemetry. |
| `jeryu://pipelines/{project_id}/{pipeline_id}` | Pipeline graph and jobs. |
| `jeryu://jobs/{project_id}/{job_id}/trace` | Bounded redacted trace chunks. |
| `jeryu://cache/metrics` | Cache summary, categories, taints, verdicts. |
| `jeryu://vti/summary` | VTI plan/miss summary. |
| `jeryu://agents` | Agent sessions/tasks/grants/races. |
| `jeryu://bugs/{bug_id}` | Bug detail, attempts, evidence. |
| `jeryu://release/latest` | Release attempt and gates. |
| `jeryu://evidence/search?...` | Proof timeline search. |
| `jeryu://settings/effective` | Redacted effective settings. |
| `jeryu://autonomy/kill-bell` | Kill-bell state. |

### 8.5 Event kinds

Use a stable event taxonomy. Example top-level event kinds:

```text
system.health.updated
source.freshness.updated
repo.family.updated
repo.updated
pipeline.created
pipeline.updated
pipeline.completed
pipeline.superseded
job.created
job.queued
job.started
job.log.chunk
job.log.annotation
job.completed
job.failed
job.retried
runner.pool.updated
runner.manager.started
runner.manager.stopped
runner.manager.oom
runner.node.telemetry
queue.capacity.updated
cache.object.updated
cache.request.observed
cache.taint.added
cache.verdict.recorded
cache.gc.planned
cache.gc.completed
vti.plan.created
vti.plan.validated
vti.selector.miss
vti.fullrun.escalated
agent.session.created
agent.task.started
agent.intent.started
agent.intent.finished
agent.patch.proposed
agent.race.created
agent.race.winner.selected
capability.intent.requested
capability.grant.created
admission.decision.recorded
bug.created
bug.updated
bug.attempt.started
bug.attempt.completed
release.attempt.created
release.gate.updated
release.promoted
release.rollback.started
secret.audit.recorded
artifact.provenance.recorded
security.finding.updated
jankurai.audit.ingested
jankurai.repair.queued
action.previewed
action.started
action.progress
action.completed
action.failed
```

### 8.6 Core view-model structs

The actual code can refine these, but implementation should start with structs shaped like:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TuiReadModel {
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    pub event_cursor: u64,
    pub profile: RuntimeProfile,
    pub freshness: SourceFreshnessMap,
    pub posture: FleetPosture,
    pub capacity: CapacitySnapshot,
    pub repo_families: Vec<RepoFamilySummary>,
    pub attention: Vec<AttentionItem>,
    pub next_actions: Vec<NextAction>,
    pub pinned: Vec<EntityRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
    pub label: String,
    pub scope: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityDetail {
    pub entity: EntityRef,
    pub state: String,
    pub summary: String,
    pub freshness: Freshness,
    pub timeline: Vec<TimelineEvent>,
    pub blockers: Vec<Blocker>,
    pub evidence: Vec<EvidenceRef>,
    pub related: Vec<EntityRef>,
    pub metrics: BTreeMap<String, MetricValue>,
    pub available_actions: Vec<ActionDescriptor>,
}
```

---

## 9. Flight Deck screen

### 9.1 Purpose

The Flight Deck is the default view. It answers:

- Are we safe to code, merge, and release?
- Which repo families are hot?
- What is moving right now?
- What is red/yellow and why?
- Are we close to theoretical throughput?
- Are core/memory/disk/runner limits near saturation?
- What should I do next?

### 9.2 Wide mock

```text
╭─ JeRyu Flight Deck ─ all repos ─ prod profile ─ fresh 0.8s ─ cursor 188442 ───────────────╮
│ SAFE CODE ✓  MERGE ⚠ queue+audit  RELEASE ✗ unsigned rc.17  KILL BELL armed ✓  DEMAND ↑ │
├─ Fleet Speed ───────────────────────┬─ Repo Families ───────────────┬─ Next Actions ──────┤
│ Slots 92/118 online, 84 busy        │ ▶ veox-*       21 repos  ⚠    │ 1 scale rust-fast +6 │
│ Theoretical 176  Effective 123      │   veox-enclave   red 2 jobs   │   saves 41m/d conf .83│
│ Limit distance 1.38×  waste 22%     │   veox-deploy    rel blocked  │ 2 GC remote-3 cache  │
│ Queue 47 jobs  Drain 18m p50        │   redlinedb      perf reg ✗   │ 3 retry unsigned job │
│ CPU 74%  Mem 81%  Disk 88% ⚠        │   jeryu          audit ⚠      │ 4 teach VTI mapping  │
│ Cache 337/400GiB  Hit 82%  taints 3 │   isolated       9 repos ✓     │ 5 review agent grant │
├─ Live Workflow Atlas ─────────────────────────────────────────────────────────────────────┤
│ veox-api       build ✓ ─ test ●●● 68% ─ audit ○ ─ package ○ ─ release ⛔ unsigned        │
│ veox-enclave   build ✓ ─ integ ✗ db-timeout ─ audit ○       agent ♟ patch-race 2/4       │
│ veox-deploy    plan ✓ ─ canary ● 42% ─ telemetry ○ ─ prod ⧗ approval                     │
│ redlinedb      compile ✓ ─ sqllogic ● 91% slow +38% ─ perf ✗ 3.1x baseline               │
├─ Hot Signals ────────────────────────┬─ Resource Pressure ──────────┬─ Event Tape ────────┤
│ ✗ 3 failing critical-path jobs       │ CPU cores: local 74% remote  │ 12:04 job fail ...   │
│ ⚠ rust-fast p95 wait 12m04s          │ Mem: remote-3 91% OOM 2      │ 12:03 cache taint... │
│ ⚠ cache Cargo target 92GiB + rising  │ Disk: cache 84%, node 95% ✗  │ 12:02 grant created  │
│ ⚠ VTI selector misses 7 in 24h       │ GitLab API p95 214ms ok      │ 12:01 runner oom...  │
╰─ Enter drill  Esc up  / filter  x explain  e evidence  a actions  : command  g? jumps ─╯
```

### 9.3 Panels

**Header posture rail**

- Safe-to-code: based on local repo health, branch state, source freshness, kill bell, system health.
- Safe-to-merge: based on CI, tests, VTI, Jankurai, MR drift, policy, artifact signing.
- Safe-to-release: based on release gates, canary, signed artifacts, secrets, telemetry, rollback readiness.
- Kill bell: autonomy paused/armed and TTL.
- Demand: queue inflow/outflow trend.

**Fleet Speed strip**

- online / theoretical / effective slots;
- busy/idle/unhealthy slots;
- queued jobs and weighted work seconds;
- p50/p90 drain ETA;
- limit distance;
- SCREAM index;
- core/memory/disk pressure;
- top bottleneck cause.

**Repo Families**

Group by explicit config and heuristics:

1. explicit `repo_family` config;
2. common prefix before second hyphen, e.g. `veox-deploy`, `veox-enclave` under `veox-*`;
3. provider group/namespace;
4. dependency graph;
5. isolated bucket.

Each family row shows:

- repo count;
- active pipelines;
- failing/blocked counts;
- queue work seconds;
- runner constraints;
- cache/VTI/security/release badges;
- trend sparkline.

**Live Workflow Atlas**

Shows top active pipelines and critical paths across families. It does not attempt to show every job; it shows the most important moving/blocked flows. Use `Enter` to drill into the selected repo/pipeline.

**Hot Signals**

Severity-ranked facts that require attention:

```text
score = severity_weight
      + critical_path_impact
      + production_or_release_impact
      + blocked_age_weight
      + confidence_weight
      - acknowledged_or_muted_weight
```

**Resource Pressure**

A compact view of host/node/pool pressure:

- CPU utilization and runnable load per core;
- memory utilization and OOM events;
- disk/cache pressure;
- Docker daemon/container health;
- queue pressure by runner tag;
- GitLab API latency/rate limits;
- DB latency;
- broker lag.

---

## 10. Queue / Theoretical Limit screen

### 10.1 Purpose

This is the most important new screen because the user explicitly cares about whether the system is close to CPU/memory/core limits and whether runner count should increase.

It must answer:

1. How close are we to the theoretical lower bound?
2. Are runner slots saturated, fragmented, unhealthy, or idle?
3. Are cores/memory/disk the real bottleneck?
4. Which jobs are unschedulable and why?
5. Would adding runners help?
6. If yes, where should runners be added, how many, and what is the expected benefit?
7. If no, what is the non-runner bottleneck?

### 10.2 Three-limit model

A single CPU or runner utilization number is misleading. Use three limits:

| Limit | Meaning | What it reveals |
|---|---|---|
| Physics limit | Best possible pipeline wall time from DAG critical path with infinite runners and hot cache. | Code/test DAG structure, serial stages, inherent test duration. |
| Fleet limit | Best possible wall time under current configured/online runners, tags, trust tiers, nodes, cold starts, cache state, core/memory/disk limits. | Whether more/better runners help. |
| Policy limit | Fleet limit plus required approvals, signing, security, freeze windows, canary minima, human gates. | Whether machines cannot solve the wait. |

Display all three:

```text
Physics floor: 13m27s
Fleet floor:   17m12s  (+3m45s due rust-fast queue + cold image pulls)
Policy floor:  25m00s  (+7m48s due canary min + approval SLA)
Current projection: 18m02s
Limit distance vs physics: 1.34×
Limit distance vs fleet:   1.05×
```

A pipeline can be far from physics but close to fleet; that means adding runners or changing topology might help. A pipeline can be close to fleet but far from policy; runner changes will not help.

### 10.3 Capacity definitions

```rust
pub struct CapacitySnapshot {
    pub scope: ScopeRef,
    pub generated_at: DateTime<Utc>,
    pub confidence: f64,
    pub theoretical_slots: u32,
    pub configured_slots: u32,
    pub online_slots: u32,
    pub effective_slots: u32,
    pub busy_slots: u32,
    pub idle_slots: u32,
    pub unhealthy_slots: u32,
    pub blocked_slots: u32,
    pub queued_jobs: u32,
    pub queued_work_seconds_p50: f64,
    pub running_jobs: u32,
    pub unschedulable_jobs: u32,
    pub p50_queue_wait_secs: f64,
    pub p95_queue_wait_secs: f64,
    pub queue_pressure_10m: f64,
    pub runner_saturation: f64,
    pub scheduler_efficiency: f64,
    pub useful_utilization: f64,
    pub wasted_slot_seconds_1h: f64,
    pub physics_floor_secs: f64,
    pub fleet_floor_secs: f64,
    pub policy_floor_secs: f64,
    pub projected_wall_secs: f64,
    pub limit_distance_physics: f64,
    pub limit_distance_fleet: f64,
    pub top_constraints: Vec<QueueConstraint>,
    pub recommendations: Vec<CapacityRecommendation>,
}
```

### 10.4 Per-pool slot math

For each pool:

```text
theoretical_slots(pool) = min(
  pool.max_managers * pool.runner_concurrency,
  pool.request_concurrency_limit,
  remote_node_available_slots(pool),
  gitlab_runner_limit(pool),
  optional_global_cap
)

online_slots(pool) = Σ manager.configured_concurrency where manager.state in {online, running}

busy_slots(pool) = count(running jobs assigned to pool/tag)

usable_slots(pool) = online_slots
                   - paused_slots
                   - unhealthy_slots
                   - trust_tier_blocked_slots
                   - incompatible_tag_slots
                   - disk_pressure_blocked_slots
                   - memory_pressure_blocked_slots
                   - reserved_release_slots

effective_slots(pool) = usable_slots * efficiency_factor(pool)
```

Efficiency factor should consider:

- image pull overhead;
- cache miss rate;
- runner cold start p50/p95;
- OOM/restart rate;
- Docker daemon health;
- node CPU steal or load pressure;
- memory pressure and swap;
- disk pressure and IO wait;
- GitLab API latency/rate limits;
- broker lag;
- jobs stuck without log progress.

### 10.5 Core and memory pressure model

Runner count decisions must be tied to actual core/memory headroom.

For each node:

```text
cpu_headroom_cores = physical_or_vcpu_cores * target_cpu_utilization - used_cpu_cores_p95
memory_headroom_bytes = total_mem * target_mem_utilization - used_mem_p95 - safety_reserve
io_headroom = target_io_wait - observed_io_wait_p95
disk_headroom = disk_limit - disk_used - gc_reserved_space
container_headroom = docker_limit - running_managers - system_reserved_containers
```

Recommended target thresholds:

| Signal | Green | Amber | Red |
|---|---:|---:|---:|
| CPU p95 utilization | `<75%` | `75-88%` | `>88%` with queue |
| Load per core | `<1.0` | `1.0-1.5` | `>1.5` sustained |
| Memory p95 | `<75%` | `75-88%` | `>88%` or OOM/swap |
| Swap activity | none | low | sustained |
| Disk used | `<80%` | `80-90%` | `>90%` / critical at `95%` |
| IO wait | `<10%` | `10-25%` | `>25%` sustained |
| Container OOM | 0 | 1 recent | multiple/repeating |
| Queue wait p95 | near zero | rising | SLA breach |

### 10.6 Useful utilization, not raw utilization

Raw utilization can be bad. The TUI should distinguish:

| State | Meaning | Decision |
|---|---|---|
| High CPU + low queue wait + jobs progressing | Healthy saturation. | Do not add runners unless future demand predicts queue. |
| High CPU + high queue wait | Capacity constrained. | Add runners/nodes only if memory/disk headroom exists. |
| Low CPU + high queue wait | Fragmentation/tag/trust/config bottleneck. | Fix tags, paused pools, grants, or unschedulable constraints. |
| High runners busy + obsolete pipelines | Wasted capacity. | Cancel superseded jobs before scaling. |
| High queue + critical path serial | Runner scaling has low impact. | Split/shard/parallelize jobs; add `needs`; fix DAG. |
| High queue + policy gates | Machines cannot help. | Approve/resolve gate or wait. |
| High queue + cache miss storm | Runners may amplify pain. | Warm/fix cache first. |
| High queue + memory OOM | More runner processes on same nodes worsens state. | Add memory/node capacity or reduce concurrency. |
| Disk red + cache full | More runners may fail. | GC/rebalance cache before scaling. |

### 10.7 Queue drain estimate

For every queued job:

```text
work_seconds(job) =
  if known_historical(repo, job_name, stage, ref_class, cache_state) then p50/p90
  else if known_stage_default(stage) then stage_p50/p90
  else global_default

eligible_pools(job) = pools matching tags + trust tier + executor + protected branch rules

queued_work_by_constraint = group jobs by most restrictive eligible pool/tag/node/policy
```

Drain ETA:

```text
drain_eta(pool) = queued_work_seconds_matching_pool / max(1, effective_freeing_rate(pool))

effective_freeing_rate(pool) = effective_slots(pool) / mean_runtime_remaining(pool)
```

For accuracy, run a small discrete-event scheduler simulation over the DAG and pool constraints rather than using division alone.

### 10.8 Limit-distance formula

```text
critical_path_min = longest_path_sum(best_or_p10_durations, DAG_deps)
current_projection = simulated_schedule(online/effective slots, deps, tags, cache, nodes)
policy_projection = current_projection + required_gate_waits + release_minima
limit_distance = current_projection / max(critical_path_min, 1s)
```

Interpretation:

| Limit distance | Meaning |
|---:|---|
| `1.00-1.15×` | Near ideal; mostly physics-limited. |
| `1.15-1.50×` | Good but some avoidable queue/cache/topology loss. |
| `1.50-2.50×` | Major waste; investigate runner/cache/DAG. |
| `>2.50×` | Screaming; immediate operator action likely needed. |

### 10.9 SCREAM index

A single headline can be useful if it is decomposable:

```text
SCREAM = clamp(100 * weighted_mean([
  inverse_limit_distance,        weight .22,
  useful_runner_utilization,     weight .18,
  low_queue_wait_score,          weight .15,
  non_obsolete_work_ratio,       weight .12,
  resource_headroom_score,       weight .10,
  cache_health_score,            weight .08,
  vti_confidence_score,          weight .06,
  source_freshness_score,        weight .05,
  blocker_resolution_score       weight .04
]), 0, 100)
```

Name can be changed, but the concept is: **how close is the engineering machine to screaming at optimal speed without waste or hidden danger?**

### 10.10 Runner increase decision tree

The TUI should not blindly recommend more runners. It should classify first.

```text
IF queue_pressure high AND p95_queue_wait high:
  IF eligible pools saturated AND node CPU/mem/disk headroom exists:
    recommend scale pool or add managers
  ELSE IF eligible pools saturated BUT node resource headroom low:
    recommend add remote node / larger node / reduce per-node concurrency
  ELSE IF many jobs unschedulable:
    recommend fix tags/trust/protected-runner config
  ELSE IF queue dominated by serial critical path:
    recommend split/shard/rewrite DAG, not runner scale
  ELSE IF cache miss storm or image pulls dominate:
    recommend cache/image warmup first
  ELSE IF policy gate dominates:
    recommend approval/gate action, not runner scale
ELSE IF low queue and low utilization:
  recommend no scale; optionally drain/scale down warm managers
ELSE IF high utilization but low queue:
  show healthy saturation; no immediate scale
```

### 10.11 Scale recommendation payload

```rust
pub struct CapacityRecommendation {
    pub id: String,
    pub title: String,
    pub scope: ScopeRef,
    pub action: Option<ActionRef>,
    pub risk: RiskTier,
    pub reason: String,
    pub evidence: Vec<EvidenceRef>,
    pub expected_p50_saved_secs_per_day: f64,
    pub expected_p95_queue_reduction_secs: f64,
    pub cost_delta_per_day: Option<Money>,
    pub resource_impact: ResourceImpact,
    pub confidence: f64,
    pub prerequisites: Vec<String>,
    pub alternatives: Vec<String>,
}
```

Example:

```text
Recommendation: scale rust-fast +6 managers
Why: tag=rust-fast has 18 queued jobs / 9h12m work; pool 24/24 busy; nodes remote-1/2 have CPU 63%, mem 58%, disk 72%; no policy gate.
Impact: p95 queue wait 12m04s → 2m10s; saves 41m/day on critical path; confidence .83.
Risk: medium. Dry run available. Required grant: runner_admin.
Prereq: pre-pull image rust-ci:2026-05 or first jobs may cold-start.
```

### 10.12 Queue screen mock

```text
╭─ Queue / Theoretical Limit ───────────────────────────────────────────────────────────────────────────────────╮
│ Scope all repos  Window live+24h  Model p50/p90 repo+job+stage  Confidence .81  Fresh 0.7s                   │
├─ Summary ─────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Theoretical 176 slots │ Configured 160 │ Online 118 │ Effective 123 │ Busy 84 │ Queued 47 │ Unhealthy 5       │
│ CPU 74% p95  Mem 81% p95  Disk 88% ⚠  OOM 2  Queue pressure 1.42  Drain 18m p50 / 31m p90                    │
│ Physics floor 13m27s │ Fleet floor 17m12s │ Projected 18m02s │ Limit distance 1.34× │ Main cause rust-fast tag │
├─ Pools ───────────────────────────────────────┬─ Queue by Constraint ─────────────────┬─ Scale Advice ───────╮
│ Pool             on/th/eff busy q util wait   │ Constraint       jobs work  diagnosis  │ Best action           │
│ ▶ rust-fast       24/48/31  24 18 100% 12m04s │ tag=rust-fast     18 9h12m saturated  │ +6 managers saves 41m │
│   rust-default    33/60/41  29  8  88%  3m11s │ docker socket      7 2h01m tag/config │ fix pool affinity     │
│   gpu-audit        2/ 4/ 2   2  3 100% 21m40s │ serial release     4 1h10m policy     │ no runner fix         │
│   sec-scan         4/ 8/ 4   3  5  75%  8m32s │ image cold-start   9 2h20m cache      │ pre-pull/cache        │
│   remote-nyc      18/32/21  16  6  89%  4m50s │ disk pressure      6 1h44m node       │ GC/buildkit first     │
├─ Critical Path Deltas ────────────────────────┴────────────────────────────────────────┴──────────────────────╯
│ veox-api#581: build ✓ 2m11s ─► integ ● 9m/14m ─► audit ○ 4m ─► package ○ 2m ─► release blocked by integ      │
│ Deltas vs 7d: integ +42%, cargo-deny +31%, image-build +28%, queue wait rust-fast +214%                       │
╰─ Enter drill  s scale preview  d diagnose  h history  x explain model  / filter  Esc back ──────────────────╯
```

---

## 11. Runners / System Utilization screen

### 11.1 Purpose

This screen is the deep answer to system utilization:

- Are we close to core limits?
- Are we close to memory limits?
- Is disk/cache pressure going to break CI?
- Which node/pool/manager is the bottleneck?
- Which runners are unhealthy, OOMing, stale, underused, or misconfigured?
- Can we safely increase runner count?
- Should we scale down/drain/rebalance instead?

### 11.2 Data required

Add or expose telemetry sampling for:

**Host/node metrics**

- CPU core count, utilization p50/p95/p99;
- load average normalized by core count;
- memory total/used/available/p95;
- swap used and swap-in/out;
- disk bytes used/free by mount;
- disk IO wait/read/write throughput;
- network throughput/errors;
- process count/open file descriptors if useful;
- clock skew;
- uptime;
- kernel/container runtime versions.

**Docker/container metrics**

- manager container CPU, memory, network, block IO;
- container state, restart count, exit reason;
- OOM/die events;
- image pull duration and failures;
- Docker daemon health and latency;
- config hash and runner version.

**Runner/pool metrics**

- pool min/max managers;
- configured concurrency/request concurrency;
- online/busy/idle/unhealthy counts;
- queued jobs matching pool tags;
- p50/p95 queue wait;
- p50/p95 job duration;
- useful slot seconds;
- wasted slot seconds;
- obsolete/superseded work;
- warm/cold start time;
- trust tier and protected branch restrictions;
- remote node affinity.

**Cache/storage metrics**

- per-node cache usage;
- namespace/category sizes;
- GC history;
- inode pressure;
- BuildKit/Docker layer usage;
- Cargo/sccache/registry mirror sizes;
- eviction forecast.

### 11.3 Screen mock

```text
╭─ Runners / System Utilization ─ all repos ─ fresh 0.9s ─────────────────────────────────────────────────────╮
│ Nodes 7  Pools 5  Managers 164 max / 118 online / 123 effective / 84 busy  OOM 2  Dead 1  Disk pressure 1    │
├─ Nodes ─────────────────────────────┬─ Pools ───────────────────────────────┬─ Selected Node: remote-3 ─────╮
│ ▶ local-docker CPU 71% Mem 69 Disk84│ rust-fast    24/48 busy 100% wait12m │ SSH 12ms  Docker ✓  age 0.5s  │
│   remote-1     CPU 63% Mem 58 Disk72│ rust-default 33/60 busy  88% wait3m  │ CPU 82% p95  load/core 1.3 ⚠  │
│   remote-2     CPU 38% Mem 64 Disk69│ sec-scan      4/ 8 busy  75% wait8m  │ Mem 91% p95  OOM 2 ✗          │
│   remote-3     CPU 82% Mem 91 Disk91│ gpu-audit     2/ 4 busy 100% wait21m │ Disk 91%  cache 221/240GiB ⚠  │
│   remote-4     offline 16m ✗        │ macos         0/12 offline wait∞     │ Managers 24/30  restarts 3    │
├─ Pressure Timeline ─────────────────┴───────────────────────────────────────┴──────────────────────────────╯
│ CPU ▁▃▅▇▇▆  Mem ▃▅▆████  Disk ▆▆▇██  Queue rust-fast ▁▁▄████  OOM events ✗ ✗                             │
├─ Recommendations ────────────────────────────────────────────────────────────────────────────────────────────┤
│ 1. Do NOT add more managers on remote-3 until GC or memory limit fixed; OOM risk high.                        │
│ 2. Scale rust-fast +6 on remote-1/2; enough CPU/mem/disk headroom; saves ~41m/day, confidence .83.            │
│ 3. remote-4 unreachable makes macos jobs unschedulable; repair node or reroute tag.                           │
│ 4. GC remote-3 cache can recover ~88GiB; then allow +3 sec-scan managers.                                     │
╰─ Enter node/pool drill  s scale preview  d drain  g GC plan  l logs  h history  x explain ─────────────────╯
```

### 11.4 Node detail tabs

Press `Enter` on a node:

1. **Overview:** CPU/mem/disk/network, health, SSH/Docker status, managers, queue contribution.
2. **Managers:** containers, runner IDs, versions, config hashes, states, restarts.
3. **Jobs:** running jobs, assigned pools, resource consumption, log progress.
4. **Storage:** cache categories, Docker layers, build dirs, GC candidates.
5. **Events:** Docker `die`/`oom`, reconcilers, SSH failures, GC, scale actions.
6. **Config:** node max managers, storage limit, pool affinity, GitLab URL override, enabled flag.
7. **Actions:** drain, pause, resume, GC, restart Docker, reload config, remove, scale.

### 11.5 Pool detail tabs

Press `Enter` on a pool:

1. **Capacity:** theoretical/online/effective slots, headroom, losses.
2. **Queue:** jobs waiting by tag/ref/repo/stage, p50/p95 wait.
3. **Managers:** individual manager state and health.
4. **Eligibility:** tags, trust tier, protected branches, executor constraints.
5. **History:** queue wait, utilization, OOMs, cold starts, image pulls.
6. **Recommendations:** scale/drain/rebalance with projected impact.
7. **Actions:** scale, pause, resume, drain, delete, rotate token, edit config.

### 11.6 Runner count action preview

When pressing `s` on a pool:

```text
╭─ Scale Preview: rust-fast +6 managers ─────────────────────────────────────────────────╮
│ Risk: medium  Required grant: runner_admin  Dry run: available  Idempotency: scale-... │
├─ Why ──────────────────────────────────────────────────────────────────────────────────┤
│ tag=rust-fast has 18 queued jobs, 9h12m p50 work, p95 wait 12m04s, 24/24 busy slots.    │
│ Eligible nodes remote-1/2 have CPU <65%, Mem <65%, Disk <75%, no OOMs.                  │
├─ Expected impact ──────────────────────────────────────────────────────────────────────┤
│ p95 wait 12m04s → 2m10s  Drain 18m → 7m  Critical-path saved 41m/day  Confidence .83    │
├─ Resource impact ──────────────────────────────────────────────────────────────────────┤
│ remote-1 +3 managers: CPU +12%, Mem +9GiB, Disk +18GiB                                  │
│ remote-2 +3 managers: CPU +10%, Mem +8GiB, Disk +18GiB                                  │
│ New p95 CPU 75%, new p95 Mem 73%, Disk 78%; below amber threshold.                      │
├─ Alternatives ─────────────────────────────────────────────────────────────────────────┤
│ +3 managers saves 24m/day; split integ-db shard saves 17m/day; warm image saves 9m/day. │
╰─ y execute  n cancel  d dry-run  e evidence  x explain  Esc back ─────────────────────╯
```

---

## 12. Repos and repo-family drilldown

### 12.1 Repo family screen

Purpose:

- group shared repos like `veox-*`;
- compare isolated vs shared family health;
- see cross-repo bottlenecks and shared resource pressure;
- drill into family-level flow, bugs, agents, cache, releases.

Mock:

```text
╭─ Repo Family: veox-* ─ 21 repos ─ fresh 0.8s ───────────────────────────────────────────╮
│ Health ⚠  Active pipelines 37  Failing 4  Queued work 18h22m  Shared pools rust-fast ⚠ │
├─ Repos ────────────────────────────────┬─ Family Critical Path ────────────────────────┤
│ ▶ veox-enclave  ✗ integ-db  agent race │ veox-enclave #912 integ-db ✗ blocks release   │
│   veox-deploy   ⚠ canary wait          │ veox-api #581 test ● 68% queue rust-fast      │
│   veox-api      ● test 68%             │ veox-deploy #77 canary ● telemetry pending    │
│   veox-auth     ✓                      │ red shared cache miss storm: cargo target     │
│   veox-web      ⚠ unsigned artifact    │                                               │
├─ Shared Signals ───────────────────────┴─ Family Actions ──────────────────────────────┤
│ rust-fast wait 12m04s; cache Cargo target 92GiB; VTI misses in api/enclave; 3 bugs ready│
│ Actions: scale family pool, cancel superseded, family cache GC, family VTI audit, export │
╰─ Enter repo  ] next family  x explain family  e evidence  / filter ───────────────────╯
```

### 12.2 Repo row fields

Each repo row should carry:

- repo slug/alias;
- family;
- provider/project ID;
- default branch;
- local root;
- active pipelines;
- latest main status;
- open MRs/PRs;
- bugs by status;
- agents active/blocked;
- queue work seconds;
- runner pool pressure;
- cache hit/miss/taint badges;
- VTI confidence/miss badges;
- Jankurai score/caps;
- security/artifact/release badges;
- source freshness;
- one-line blocker.

### 12.3 Repo dashboard

Mock:

```text
╭─ Repo: veox-enclave ─ family veox-* ─ branch main ─ fresh 0.7s ─────────────────────────╮
│ Status ✗  Latest main b13c9a1  Pipeline #912 failing  Release blocked  Agents 2 active │
├─ Overview Cards ────────────────────────────────────────────────────────────────────────┤
│ CI ✗ integ-db timeout  Queue 4m12s  VTI ⚠ 3 misses  Cache ✓ 84%  Jankurai 87 cap dupes │
│ Bugs 7 open / 2 ready  Security ✓  Artifacts ⚠ unsigned rc.17  Git sync ✓ mirror age 7s│
├─ Current Pipeline ─────────────────────┬─ Right Inspector ─────────────────────────────╮
│ build ✓ 2m11s ─► unit ✓ 3m04s          │ Selected: integ-db job #981273                │
│            └► integ-db ✗ timeout       │ Cause: postgres service health timeout         │
│            └► audit ○ queued           │ Runner: rust-fast remote-2 CPU 74 Mem 62       │
│ package ○ release ○                    │ Evidence: capsule cap_981273 ◆                │
├─ Repo Timeline ────────────────────────┴───────────────────────────────────────────────╯
│ 12:04 job failed  12:02 agent patch proposed  11:58 VTI selector miss  11:51 cache hit │
╰─ Enter pipeline/job  [/] prev/next repo  a actions  e evidence  x explain  L logs ────╯
```

### 12.4 Repo sub-tabs

Within a repo:

- Overview
- Workflow
- Jobs
- Logs
- MRs/PRs
- Bugs
- Agents
- Tests/VTI
- Cache
- Jankurai
- Security
- Artifacts
- Releases
- Evidence
- Settings

Use `[` and `]` to move across repos; use `Ctrl+[` / `Ctrl+]` or `H/L` to move across repo sub-tabs if configured.

---

## 13. Workflow Atlas and pipeline DAG

### 13.1 Purpose

The Workflow Atlas is the visual heart of the TUI. It must show running CI as a live DAG, not as a flat list.

It must answer:

- What is running now?
- Which job is on the critical path?
- What is queued vs blocked vs failed?
- Which runner/pool/node owns each job?
- How much of the pipeline is complete?
- What is the predicted green time?
- What evidence/logs/artifacts explain the state?

### 13.2 Pipeline graph construction

Graph inputs:

- GitLab jobs;
- stages;
- bridges/downstream pipelines;
- `needs` edges when available;
- artifact dependencies when available;
- stage-order fallback;
- release gates;
- VTI-generated dynamic tests;
- policy/security/artifact gates;
- child pipelines.

Edge kinds:

```rust
pub enum EdgeKind {
    Needs,
    StageBarrier,
    Artifact,
    Bridge,
    ChildPipeline,
    ManualGate,
    ReleaseGate,
    PolicyGate,
    VtiSelection,
    CacheDependency,
    Inferred,
}
```

Node kinds:

```rust
pub enum WorkflowNodeKind {
    Commit,
    MergeRequest,
    Pipeline,
    Stage,
    Job,
    Bridge,
    TestPlan,
    ReleaseGate,
    Artifact,
    SecurityGate,
    ManualApproval,
}
```

### 13.3 Graph layout rules

1. Use stage lanes left-to-right by default.
2. Place critical-path nodes on the central horizontal rail.
3. Collapse finished-green subgraphs when terminal is narrow.
4. Expand failed/blocked/current nodes by default.
5. Route edges with minimal crossings.
6. Use color/glyph for status, not layout alone.
7. Show `◇` on inferred edges.
8. Use animated flow only on active edges.
9. Support `C` critical-path-only mode.
10. Support `Space` expand/collapse stage/subgraph.

### 13.4 Workflow mock

```text
╭─ Workflow Atlas: veox-enclave #912 ─ predicted green 18m02s ─ critical path highlighted ─╮
│ commit b13c9a1 → MR !42 → pipeline #912  status failing  freshness 0.7s                 │
├─ DAG ────────────────────────────────────────────────────────────────────────────────────┤
│        ┌─ lint ✓ 1m12s ────────────────┐                                                 │
│ build ✓┼─ unit ✓ 3m04s ────────────────┼─ audit ○ queued ─ package ○ ─ release ⛔        │
│ 2m11s  └─ integ-db ✗ 14m22s timeout ───┘                                                 │
│             ▲ runner rust-fast/remote-2  queue 4m12s  p95 +42%  capsule ◆                │
├─ Selected job: integ-db #981273 ─────────────────────────────────────────────────────────┤
│ status failed  exit 1  stage test  runner remote-2  pool rust-fast  queue 4m12s duration 14m22s│
│ first failure: postgres service health timeout at line 1842  retries 0  flake score .22   │
╰─ L logs  e evidence  r retry preview  b attach bug  x explain  C critical-only ─────────╯
```

### 13.5 Job detail

Job detail has tabs:

1. **Summary:** identity, project, pipeline, stage, status, ref, allow-failure, web URL.
2. **Timing:** created/queued/started/finished/duration, p50/p95, bottleneck rank.
3. **Runner:** pool, runner ID, manager, system ID, node, Docker container, CPU/mem/disk pressure.
4. **Logs:** live trace, search, annotations, failure folding.
5. **Artifacts:** size, expiry, digest, signed status, parsed reports.
6. **Evidence:** failure capsule, retry decision, VTI receipt, cache verdict, Jankurai/security receipts.
7. **Related:** MR, bug, agent task, release gate, cache objects.
8. **Actions:** play manual, cancel, retry, requeue, explain, open GitLab, copy trace, attach to bug.

---

## 14. Live logs and traces

### 14.1 Requirements

The log viewer must support:

- streaming when available;
- polling fallback;
- bounded memory ring buffer;
- ANSI rendering with safe sanitization;
- search and filter;
- jump to first failure;
- annotations from parsers;
- fold noisy sections;
- follow/unfollow mode;
- copy/export redacted snippet;
- source line anchors;
- evidence linking.

### 14.2 Log viewer keys

| Key | Action |
|---|---|
| `f` | follow/unfollow tail |
| `/` | search logs |
| `n` / `N` | next/previous match |
| `F` | jump to first failure annotation |
| `E` | show evidence/capsule |
| `A` | show annotations only |
| `w` | toggle wrap |
| `u` | upload/attach snippet to bug/evidence if permitted |
| `y` | copy redacted snippet |

### 14.3 Trace annotation model

```rust
pub struct LogAnnotation {
    pub line_start: u64,
    pub line_end: u64,
    pub severity: Severity,
    pub kind: LogAnnotationKind,
    pub message: String,
    pub confidence: f64,
    pub evidence: Vec<EvidenceRef>,
}

pub enum LogAnnotationKind {
    FirstFailure,
    TestFailure,
    Panic,
    Timeout,
    Oom,
    DependencyDownload,
    CacheMiss,
    CompilerError,
    SecurityFinding,
    FlakeSignature,
    SecretRedaction,
}
```

---

## 15. Cache Observatory

### 15.1 Purpose

The Cache screen must answer:

- Are we full or near full?
- What categories are taking space?
- Is cache helping or hurting?
- Which repos/jobs are causing misses?
- Are taints/verdicts blocking reuse?
- What GC is safe?
- Are Rust crates, Cargo targets, sccache, registry blobs, Docker layers, and materialized artifacts behaving correctly?

### 15.2 Cache categories

At minimum:

- Cargo registry index;
- Cargo crate downloads;
- Cargo git checkouts;
- Cargo target directories;
- sccache;
- Docker/OCI registry layers;
- BuildKit layers;
- test artifacts;
- release artifacts;
- toolchains;
- material objects;
- action cache;
- unknown/unclassified.

### 15.3 Cache screen mock

```text
╭─ Cache Observatory ─ all repos ─ budget 400GiB ─ used 337GiB 84% ─ fresh 1.1s ──────────╮
│ Hit 82.4%  Miss 17.6%  Served 1.8TiB/24h  Taints 3  Leases 12  GC reclaimable 74GiB     │
├─ Categories ──────────────────────┬─ Hot Objects / Miss Storms ─────────────────────────╮
│ Cargo target       92GiB  27% ⚠   │ ▶ veox-enclave target/debug 18GiB hits 421 taint no │
│ Docker layers      71GiB  21%     │   rust:nightly layer 12GiB misses 88 image cold      │
│ sccache            54GiB  16%     │   crate serde 2.1MiB hits 982 ok                    │
│ Cargo crates       39GiB  12%     │   sec-scan db 9GiB stale verdict                    │
│ Registry mirror    31GiB   9%     │                                                       │
├─ Trust / Verdicts ─────────────────┴─ GC Plan ──────────────────────────────────────────╯
│ Taints: 3 active  Denied reuse: 14  Force refresh: 6  Toolchain epochs: 4                │
│ Safe reclaim: 74GiB now; Risky reclaim: 38GiB needs lease expiry; Critical threshold 95% │
╰─ Enter object  g GC preview  t taints  v verdicts  h hot  x explain  / filter ─────────╯
```

### 15.4 Cache object detail

Show:

- key/digest;
- category;
- size;
- repo/job ownership;
- last hit/write;
- hit count;
- source URL/template if applicable;
- trust/verdict;
- taints;
- active leases;
- promotion state;
- material aliases;
- toolchain fingerprint;
- GC eligibility;
- related jobs/pipelines;
- actions: pin, evict, force refresh, inspect leases, show provenance.

### 15.5 Required backend expansion

Expand `/cache/summary` into:

- `/api/cache/metrics`
- `/api/cache/categories`
- `/api/cache/hot`
- `/api/cache/taints`
- `/api/cache/verdicts`
- `/api/cache/gc-plan`
- `/api/cache/object/{key}`

---

## 16. VTI Smart Test Skipper cockpit

### 16.1 Purpose

The VTI screen must prove whether smart test skipping is working safely.

It answers:

- How much time did VTI save?
- How many tests were selected/skipped?
- What confidence did each plan have?
- Did any skipped test later fail?
- Where are selector misses happening?
- Which mappings need learning?
- Which repos should fall back to full runs?

### 16.2 VTI metrics

| Metric | Meaning |
|---|---|
| Selected tests | Tests VTI chose to run. |
| Skipped tests | Tests VTI skipped. |
| Time saved | Estimated p50 runtime of skipped tests minus overhead. |
| Confidence | Plan confidence from impact model/history. |
| Selector misses | Missed tests/subsystems that should have been selected. |
| Escalation rate | Fraction of plans forced to full run. |
| False skip risk | Weighted recent selector miss severity. |
| Learning debt | Number of misses/unmapped files/subsystems needing repair. |

### 16.3 VTI screen mock

```text
╭─ VTI Smart Test Skipper ─ window 7d ─ all repos ─ fresh 2.0s ───────────────────────────╮
│ Saved 41h32m  Plans 1,284  Selected 18,442  Skipped 92,118  Confidence .91  Misses 7 ⚠ │
├─ Repo Scoreboard ─────────────────────┬─ Selector Misses ──────────────────────────────╮
│ ▶ veox-api      saved 9h12m conf .88 ⚠│ 12:01 veox-api src/api/events.rs missed integ  │
│   veox-enclave  saved 7h44m conf .81 ⚠│ 10:44 enclave crypto.rs missed slow property    │
│   redlinedb     saved 3h01m conf .96 ✓│ 09:18 deploy helm chart missed e2e              │
│   jeryu         saved 5h33m conf .93 ✓│                                                       │
├─ Plan Detail ─────────────────────────┴────────────────────────────────────────────────╯
│ Plan #tp_8821 ref b13c9a1 mode selective  selected 42 skipped 318  reason src/api touch │
│ Guardrail: 3 misses in 24h for subsystem api/events → recommended full-run until learned│
╰─ Enter plan  m misses  l learn mapping  f force full next  e evidence  x explain ──────╯
```

### 16.4 Guardrail rules

VTI can save time but must never silently undermine trust.

Rules:

1. Recent high-severity selector miss forces full-run mode for that subsystem until repaired.
2. Low confidence plans must be visually marked and escalated.
3. VTI skip decisions must link to evidence: changed files, mapping rules, historical tests, confidence, misses.
4. Release/prod gates must show whether they accepted VTI or demanded full run.
5. User can run `plan_validation` and force full test scope through action preview.

---

## 17. Agents and Autonomy cockpit

### 17.1 Purpose

The Agents screen makes autonomous work visible and controllable.

It answers:

- Which agents are active?
- What are they trying to do?
- Which grants do they hold?
- Which branches/MRs/pipelines/logs/evidence belong to them?
- Are they blocked, racing, looping, spending too much, or colliding?
- Can I pause/resume/kill/edit configs safely?

### 17.2 Agent lifecycle model to add

Inventories repeatedly call out that a dedicated lifecycle table is missing. Add:

```rust
pub struct AgentSession {
    pub id: String,
    pub actor: String,
    pub kind: AgentKind,
    pub repo: Option<String>,
    pub bug_id: Option<String>,
    pub status: AgentStatus,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub budget: BudgetRef,
    pub grants: Vec<GrantRef>,
    pub current_task: Option<EntityRef>,
}

pub struct AgentTask {
    pub id: String,
    pub session_id: String,
    pub kind: AgentTaskKind,
    pub status: AgentTaskStatus,
    pub repo: String,
    pub branch: Option<String>,
    pub mr_url: Option<String>,
    pub pipeline_id: Option<i64>,
    pub bug_id: Option<String>,
    pub evidence: Vec<EvidenceRef>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

pub struct AgentStep {
    pub id: String,
    pub task_id: String,
    pub sequence: u64,
    pub label: String,
    pub status: StepStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub summary: String,
    pub evidence: Vec<EvidenceRef>,
}
```

Also add `agent_artifacts`, `agent_messages`, and `agent_races` or represent them in the same event stream.

### 17.3 Agents screen mock

```text
╭─ Agents / Autonomy ─ active 12 ─ blocked 3 ─ spend $4.82/24h ─ kill bell armed ✓ ───────╮
│ Autonomy: enabled  Freeze: none  Budget: 18% used  Evidence gate: strict  Drift: 2 PRs ⚠│
├─ Agents ──────────────────────────────┬─ Selected Agent: enclave-fixer-2 ──────────────╮
│ ▶ ♟ enclave-fixer-2  patch race 2/4   │ Repo veox-enclave  Bug BUG-184  Status running │
│   ♟ deploy-watcher   canary review    │ Branch agent/bug-184-h2  MR !42  Pipeline #912 │
│   ♟ vti-learner      blocked grant ⚠  │ Grant branch_write expires 22m  Budget $0.62   │
│   ♟ sec-reviewer     waiting scans    │ Current step: inspect failing integ-db logs    │
├─ Race Arena ───────────────────────────┴─ Agent Logs / Evidence ───────────────────────╯
│ H1 fix timeout ✓ unit ● integ 68%      │ 12:04 read capsule cap_981273                 │
│ H2 increase health wait ✗ lint failed  │ 12:03 proposed patch branch h1                │
│ H3 mock db service ○ queued            │ 12:01 requested grant branch_write ✓          │
╰─ Enter detail  p pause  k kill  g grants  l logs  c config  e evidence  x explain ────╯
```

### 17.4 Agent config editor

The TUI may edit autonomous workflow configs only with:

1. schema validation;
2. diff preview;
3. dry-run validation;
4. risk tier;
5. required grant;
6. evidence/audit receipt;
7. rollback path;
8. redaction of secrets.

Config editor layout:

```text
╭─ Edit Agent Workflow: vti-learner ─ risk medium ────────────────────────────────────────╮
│ Left: current config                         │ Right: proposed config                   │
│ budget.max_usd_per_day: 5                    │ budget.max_usd_per_day: 8                 │
│ allowed_repos: [jeryu, veox-api]             │ allowed_repos: [jeryu, veox-api, enclave] │
├─ Validation ────────────────────────────────────────────────────────────────────────────┤
│ ✓ schema valid  ✓ no secrets  ⚠ budget increase  ✗ missing approval grant autonomy_cfg │
╰─ y apply after grant  d dry-run  r revert  e evidence  Esc cancel ─────────────────────╯
```

### 17.5 Autonomy panel

Integrate autonomy binary state:

- kill-bell status;
- freeze windows;
- active verdicts;
- superseded verdicts;
- foundry candidates;
- canary state;
- rollback drills;
- launch ledger replay;
- LLM provider health;
- LLM budget ledger;
- escalation dispatch results;
- GitHub/GitLab PR drift.

---

## 18. Bugs / Issues cockpit

### 18.1 Purpose

The Bugs screen must make cross-repo bug work accountable.

It answers:

- What bugs exist across all repos?
- Which are ready for agents?
- Which are being worked?
- Which are blocked?
- Which attempts failed and why?
- What branch/MR/CI/evidence is attached?

### 18.2 Status lanes

Use lanes:

```text
needs_triage → accepted/ready → in_progress → fix_proposed/reviewing → verifying → done
                 ↘ needs_info / blocked / duplicate / invalid / cannot_reproduce / wont_do
```

### 18.3 Bug board mock

```text
╭─ Bugs / Issues ─ all repos ─ open 128 ─ ready 23 ─ agent active 9 ──────────────────────╮
│ Filters: family=veox-* status!=done severity>=medium  Sort: critical-path impact        │
├─ Ready ───────────────┬─ In Progress ──────────────┬─ Review/Verify ───────────────────╮
│ ▶ BUG-184 enclave db  │ BUG-177 deploy canary       │ BUG-163 api flaky auth ✓ MR !18   │
│   BUG-192 cache taint │ BUG-181 redlinedb perf      │ BUG-170 jeryu docs drift          │
│   BUG-201 VTI miss    │ BUG-188 agent grant blocked │                                      │
├─ Detail: BUG-184 ─────┴────────────────────────────┴───────────────────────────────────╯
│ Repo veox-enclave  Severity high  Impact release-blocking  Owner agent/enclave-fixer-2 │
│ Attempts: h1 running pipeline #912; h2 failed lint; h3 queued. Evidence: cap_981273 ◆    │
╰─ Enter bug  A assign agent  n new  u update  l link  e evidence  x explain ───────────╯
```

### 18.4 Bug detail tabs

- Summary;
- Repro steps;
- Acceptance criteria;
- Timeline/events;
- Attempts;
- Branches/MRs;
- CI evidence;
- Logs/capsules;
- Related bugs;
- Security/privacy notes;
- Actions.

---

## 19. Git Sync / MR / Remote State screen

### 19.1 Purpose

This screen answers:

- Are local, remote, mirror, and main states in sync?
- Which pushes/refs were admitted or denied?
- Which MRs are mergeable?
- Which branches are stale, protected, or agent-created?
- Which mirror jobs failed?
- Which Git commands/audits created risk?

### 19.2 Mock

```text
╭─ Git Sync / MR State ─ all repos ─ fresh 1.4s ──────────────────────────────────────────╮
│ Main mirrors 96% healthy  Ref drift 2  Admission denies 3/24h  Open MRs 42  Agent MRs 9 │
├─ Repos ────────────────────────────────┬─ MR / Ref Detail ─────────────────────────────╮
│ ▶ veox-enclave  main ✓ mirror ✓ MR !42 │ MR !42 agent/bug-184 → main                    │
│   veox-deploy   main ✓ mirror lag 7s   │ CI failing integ-db  Jankurai pending          │
│   redlinedb      drift local ahead 2    │ Mergeability blocked: failed pipeline          │
│   jeryu          denied push 1          │ Admission: grant branch_write ok, merge no     │
├─ Recent Git/Audit Events ──────────────┴───────────────────────────────────────────────╯
│ 12:04 denied push main no grant  12:02 mirror veox-deploy ok  11:59 agent branch create │
╰─ Enter MR/ref  m mirror  p protect  d denials  e evidence  x explain  / filter ───────╯
```

### 19.3 Data to include

- tracked repository metadata;
- local branch/head/dirty state;
- remote tracking state;
- main/mirror refs;
- MR/PR state, labels, draft, approvals, discussions, mergeability;
- changed files;
- linked pipelines;
- admission decisions;
- grants;
- Git command events;
- mirror jobs;
- risk approvals;
- artifacts;
- web URLs.

### 19.4 Required backend improvement

MR hooks must be ingested into a real local read model:

- MR ID/IID;
- source/target branch;
- draft status;
- labels;
- reviewers/approvals;
- discussions/unresolved count;
- mergeability;
- changed files;
- linked pipelines;
- author/actor;
- webhook delivery metadata;
- raw payload hash.

---

## 20. CI Bottleneck Lab

### 20.1 Purpose

The Bottleneck Lab explains why CI is slow and distinguishes runtime, queue, cache, DAG, flaky retries, and policy waits.

### 20.2 Bottleneck taxonomy

| Class | Signals | Typical action |
|---|---|---|
| Queue saturation | ready jobs > eligible free runners, high p95 wait | scale/rebalance pool. |
| Tag fragmentation | low global util + high queue for tag | fix tags/trust/runner mapping. |
| Cold start | high manager/image startup | warm managers, pre-pull images. |
| Cache miss storm | hit ratio drops, download time spikes | inspect cache/hot misses/taints. |
| Serial DAG | long critical path with idle slots | split job, add `needs`, shard tests. |
| Flake retry storm | retry count high, same signatures | quarantine/fix flake. |
| VTI fallback | low confidence/full-run escalation | learn mappings/fix selector. |
| Policy gate | approvals/canary/freeze/signing | approve/fix gate/no runner action. |
| Node resource pressure | CPU/mem/disk/OOM/IO red | add node, lower concurrency, GC. |
| External dependency | GitLab/API/registry/Vault latency | fix dependency, cache, retry/backoff. |

### 20.3 Mock

```text
╭─ CI Bottleneck Lab ─ scope veox-* ─ window 7d ──────────────────────────────────────────╮
│ Top cause: rust-fast queue saturation  Impact 41m/day  Confidence .83                  │
├─ Ranked Bottlenecks ───────────────────────────┬─ Explanation ────────────────────────╮
│ ▶ rust-fast queue wait +214% 41m/day           │ 18 jobs constrained to tag rust-fast  │
│   integ-db runtime +42%      28m/day           │ Pool 24/24 busy, nodes 1/2 headroom   │
│   cargo image cold pull      19m/day           │ +6 managers clears 71% of wait         │
│   redlinedb shard imbalance  13m/day           │ Evidence: capacity sim capsim_882      │
│   VTI selector misses        10m/day           │                                         │
╰─ Enter drill  s simulate  a action  e evidence  x explain ────────────────────────────╯
```

### 20.4 Simulator

The simulator powers recommendations:

- add N managers;
- add remote node;
- change tags;
- pre-pull image;
- warm cache;
- split job into shards;
- add `needs` edges;
- cancel superseded jobs;
- force full VTI run;
- pause/drain unhealthy node.

Each simulation returns expected p50/p90 impact, confidence, cost, and risk.

---

## 21. Jankurai Quality cockpit

### 21.1 Purpose

Jankurai is treated as a code audit/quality/proof plane. The TUI should show:

- scores and trends;
- score caps and reasons;
- duplicate code groups;
- generated zones;
- ownership maps;
- proof lanes;
- security boundaries;
- repair queues;
- merge witnesses;
- auditable controls;
- historical score deltas per repo/family.

### 21.2 Data model to add

```rust
pub struct JankuraiAuditSummary {
    pub repo: String,
    pub run_id: String,
    pub version: String,
    pub commit_sha: String,
    pub score: f64,
    pub previous_score: Option<f64>,
    pub score_cap: Option<ScoreCap>,
    pub findings: Vec<JankuraiFinding>,
    pub duplicate_groups: Vec<DuplicateGroup>,
    pub generated_zones: Vec<GeneratedZone>,
    pub ownership: Vec<OwnershipEntry>,
    pub repair_queue: Vec<RepairItem>,
    pub proof_refs: Vec<EvidenceRef>,
    pub generated_at: DateTime<Utc>,
}
```

### 21.3 Mock

```text
╭─ Jankurai Quality ─ family veox-* ─ fresh 3.0s ─────────────────────────────────────────╮
│ Avg score 84.2 ▲1.8  Caps 4  Repairs 17  Duplicate groups 9  Security boundaries 2 ⚠  │
├─ Repos ───────────────────────────┬─ Findings / Repair Queue ─────────────────────────╮
│ ▶ veox-enclave 87 cap duplicate   │ CAP: duplicate crypto adapter blocks score >90    │
│   veox-api     91 ✓               │ Repair: extract shared interface owner @platform  │
│   veox-deploy  78 cap generated   │ Boundary: generated zone missing proof             │
│   redlinedb    94 ✓               │ Merge witness: MR !42 pending audit                │
╰─ Enter finding  r run audit  f fix queue  e proof  x explain cap ─────────────────────╯
```

### 21.4 Backend integration

Add:

- `jankurai_runs` table;
- `jankurai_findings` table;
- `jankurai_score_history` table;
- artifact ingestion for Jankurai output;
- event kind `jankurai.audit.ingested`;
- MCP resource `jeryu://jankurai/{repo}`;
- actions: run audit, open finding, create bug, assign repair agent, mark generated zone, export proof.

---

## 22. Code Churn / Velocity screen

### 22.1 Purpose

The Churn screen correlates code volume with risk, failures, VTI misses, Jankurai score movement, cache churn, and agent work.

### 22.2 Metrics

- commits per repo/family/time window;
- changed files/lines;
- generated vs human-authored zones;
- agent-authored vs human-authored changes;
- churn by subsystem/component/owner;
- churn linked to failures;
- churn linked to VTI misses;
- churn linked to Jankurai score caps;
- churn linked to bugs;
- review/merge latency;
- revert frequency.

### 22.3 Mock

```text
╭─ Churn / Velocity ─ window 14d ─ family veox-* ────────────────────────────────────────╮
│ Commits 482  Lines +88k/-41k  Agent-authored 18%  Failure correlation high in enclave  │
├─ Heatmap ─────────────────────────┬─ Risk Correlations ────────────────────────────────╮
│ api/events.rs        ████████     │ src/api/events.rs → VTI misses 3, failures 2       │
│ crypto/adapter.rs    ██████       │ deploy/helm → e2e miss, rollback drill pending     │
│ ci/templates.yml     █████        │ generated/proto → Jankurai cap generated proof     │
╰─ Enter file/subsystem  b bugs  t VTI  j Jankurai  e evidence ─────────────────────────╯
```

---

## 23. Security / Secrets / Artifacts cockpit

### 23.1 Purpose

The security and supply-chain area answers:

- Are there security findings?
- Are secrets healthy and redacted?
- Are grants and admission decisions safe?
- Are artifacts signed?
- Do releases have SBOM/provenance?
- Are policies enforced and auditable?

This can be one screen with sub-tabs or separate screens depending on terminal width.

### 23.2 Security domains

- capability grants/intents;
- admission decisions;
- Git risk approvals;
- secret authorities/sets/audit events;
- Vault health/mount/policy metadata;
- policy audit results;
- SAST/dependency/container scan artifacts;
- artifact signatures;
- SBOM and provenance;
- release passports;
- autonomy kill bell/freeze windows;
- LLM/tool-call governance;
- Jankurai security boundaries.

### 23.3 Secret handling rules

Absolute rules:

1. Never render plaintext secrets.
2. Render only fingerprints, paths, TTLs, mount/prefix metadata, status, and audit events.
3. Redact environment variables and tokens by default.
4. Copy actions for secret paths require confirmation and audit.
5. Secret access denials and grants appear in evidence timeline.

### 23.4 Security mock

```text
╭─ Security / Secrets ─ all repos ─ fresh 1.2s ───────────────────────────────────────────╮
│ Policy ✓  Grants 12 active  Denials 3/24h  Vault ✓ sealed=false  Secrets expiring 2 ⚠   │
├─ Domains ────────────────────────────┬─ Findings / Decisions ─────────────────────────╮
│ ▶ Grants        12 active 2 risky    │ grant g_882 branch_write agent/enclave-fixer ✓  │
│   Admission     3 denies             │ deny main push no merge grant at 12:04          │
│   Secrets       2 expiring           │ release secret veox-deploy rc.17 expires 3h     │
│   Policies      ✓                    │ policy audit clean                              │
│   Scans         1 medium             │ container scan medium CVE in sec-scan image     │
╰─ Enter decision  revoke grant  rotate  audit  e evidence  x explain ──────────────────╯
```

### 23.5 Artifacts mock

```text
╭─ Artifacts / Provenance ─ release rc.17 ────────────────────────────────────────────────╮
│ Artifacts 14  Signed 13/14 ⚠  SBOM 14/14 ✓  Provenance 14/14 ✓  Repro checks 12/14      │
├─ Artifact ────────────────────────────┬─ Detail ───────────────────────────────────────╮
│ ▶ veox-web.tar.zst unsigned ✗         │ digest sha256:...  size 182MiB                 │
│   veox-api.tar.zst signed ✓           │ signer none  expected release-bot              │
│   enclave.img signed ✓                │ blocks release gate artifact-signing            │
╰─ sign preview  verify  open SBOM  e evidence  x explain ──────────────────────────────╯
```

---

## 24. Release / Rollback / Version Control screen

### 24.1 Purpose

The Release screen must show production state, candidates, gates, canaries, approvals, evidence, and rollback safety.

It answers:

- What version is in prod?
- What candidate is next?
- Which gates are blocking?
- Is canary healthy?
- Are artifacts signed/provenanced?
- Are secrets finalized?
- Is rollback ready and drilled?
- Can automatic release/rollback safely proceed?

### 24.2 Release mock

```text
╭─ Release / Production ─ veox-deploy ─ prod v2.8.16 ─ candidate rc.17 ───────────────────╮
│ Safe-to-release ✗  Blockers: unsigned artifact, canary telemetry pending  Rollback ✓    │
├─ Release Train ───────────────────────┬─ Gates ────────────────────────────────────────╮
│ prod v2.8.16 ✓ deployed 2026-05-25    │ CI critical ✓                                  │
│ rc.17       ● canary 42%              │ VTI accepted ✓ full-run not required           │
│ rc.18       ○ foundry queued          │ Jankurai score 88 ✓                            │
│ rollback v2.8.15 ready ✓              │ Artifact signing ✗ veox-web.tar.zst            │
│                                       │ Canary telemetry ○ 18m remaining               │
├─ Evidence ────────────────────────────┴────────────────────────────────────────────────╯
│ release attempt rel_991  canary URL ...  telemetry diag path ...  rollback drill pass ◆│
╰─ promote  rollback  approve  block  dry-run  e evidence  x explain gates ─────────────╯
```

### 24.3 Release action safety

Production actions require:

- action preview;
- dry-run when available;
- current prod/candidate/rollback display;
- artifact digest display;
- gate list;
- expected side effects;
- required grant;
- typed confirmation;
- idempotency key;
- progress stream;
- signed/auditable receipt.

### 24.4 Automatic release/rollback policy

The TUI should not hide autonomous behavior. Show:

- enabled/disabled state;
- policy profile;
- gates allowed to auto-approve;
- gates requiring human approval;
- rollback triggers;
- canary thresholds;
- freeze windows;
- kill bell state;
- last automatic decision;
- evidence for decisions;
- simulation/dry-run for policy changes.

---

## 25. Evidence / Audit Ledger

### 25.1 Purpose

Evidence is the universal proof timeline across all domains. It must replace scattered proof hunting.

Search dimensions:

- entity kind/id;
- repo/family;
- actor/agent;
- event kind;
- severity;
- SHA/ref/branch;
- MR/PR;
- job/pipeline;
- release version;
- bug ID;
- grant ID;
- policy decision;
- cache key;
- artifact digest;
- time window.

### 25.2 Timeline object

```rust
pub struct EvidenceTimelineItem {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub source: EvidenceSource,
    pub entity: EntityRef,
    pub actor: Option<String>,
    pub kind: String,
    pub severity: Severity,
    pub summary: String,
    pub payload_preview: serde_json::Value,
    pub digest: Option<String>,
    pub path_or_url: Option<String>,
    pub related: Vec<EntityRef>,
    pub redacted: bool,
}
```

### 25.3 Evidence screen mock

```text
╭─ Evidence Ledger ─ query repo:veox-enclave sha:b13c9a1 ─ 48 results ───────────────────╮
│ Filters: kind=all actor=all since=24h redacted=true                                     │
├─ Timeline ────────────────────────────┬─ Detail ───────────────────────────────────────╮
│ ▶ 12:04 job.failed integ-db cap_981273│ Evidence capsule cap_981273                   │
│   12:03 agent.patch.proposed h1       │ Failure kind timeout  stage test  exit 1       │
│   12:01 grant.created branch_write    │ Related: job #981273, bug BUG-184, MR !42      │
│   11:58 vti.selector.miss             │ Digest sha256:...  Path evidence/...           │
│   11:52 cache.verdict.recorded        │ Actions: open, copy digest, attach to bug       │
╰─ Enter detail  / search  y copy digest  o open path  r replay  x explain ─────────────╯
```

### 25.4 Required endpoint

```text
GET /api/evidence?entity=&kind=&repo=&actor=&sha=&branch=&mr=&bug=&since=&limit=&cursor=
```

Return redacted, typed, paginated timeline items. Never return secret payloads.

---

## 26. Source Doctor / Settings screen

### 26.1 Purpose

This screen explains whether the TUI itself can be trusted.

It shows:

- source freshness per backend;
- schema versions;
- daemon build version/commit/profile;
- settings path and redacted effective config;
- GitLab/Vault/Docker/cache/broker/DB/MCP health;
- stream status and cursor lag;
- API latency/error rates;
- missing credentials/config;
- feature flags;
- doc/source drift warnings.

### 26.2 Mock

```text
╭─ Source Doctor / Settings ──────────────────────────────────────────────────────────────╮
│ TUI build abc123  Daemon abc123  Schema 7 ✓  Event cursor lag 0  Stream connected ✓     │
├─ Sources ─────────────────────────────┬─ Effective Profile ────────────────────────────╮
│ GitLab ✓  latency 214ms  age 0.9s     │ DB sqlite path ~/.jeryu/jeryu.sqlite            │
│ DB ✓      latency 4ms    age 0.4s     │ GitLab 127.0.0.1:8929  SSH 2224                │
│ Docker ✓  events live   age 0.5s     │ Webhook 127.0.0.1:9777  MCP 127.0.0.1:9778     │
│ Cache ⚠   summary stale age 9.4s     │ Cache 19800/19801 budget 400GiB                │
│ Vault ✓   sealed false  age 1.1s     │ Secrets redacted ✓                             │
│ Broker ✓  lag 0         age 0.8s     │                                               │
├─ Drift / Warnings ────────────────────┴────────────────────────────────────────────────╯
│ Docs mention RedlineDB-only; runtime profile says SQLite default. MCP docs list fewer tools than registry.│
╰─ r resync  v validate config  m MCP tools  h deep health  y copy redacted profile ─────╯
```

---

## 27. Action safety model

### 27.1 Action lifecycle

Every mutating action follows:

1. Select entity.
2. Open action palette.
3. Build preview.
4. Show risk tier, grants, side effects, dry-run availability.
5. Execute dry-run when available.
6. Confirm.
7. Execute with idempotency key.
8. Stream progress.
9. Persist result event/evidence.
10. Update UI via event stream.

### 27.2 Risk tiers

| Tier | Examples | Confirmation |
|---|---|---|
| Read | explain, open evidence, list jobs | none |
| Local safe | change view, pin, export redacted bundle | none/light |
| CI mutation | retry job, cancel obsolete pipeline, run tests | confirmation |
| Infra mutation | scale pool, drain node, GC cache | preview + confirmation |
| Code mutation | propose patch, race patches, update bug | preview + grant |
| Merge mutation | request/accept MR | evidence gate + grant + typed confirm |
| Production mutation | promote, rollback, secret finalize | strict preview + typed confirm + audit |
| Security mutation | revoke grant, rotate secret, policy change | strict preview + evidence + typed confirm |

### 27.3 Action descriptor

```rust
pub struct ActionDescriptor {
    pub id: String,
    pub label: String,
    pub entity: EntityRef,
    pub risk: RiskTier,
    pub side_effects: Vec<SideEffectClass>,
    pub required_grant: Option<String>,
    pub dry_run_available: bool,
    pub estimated_duration_secs: Option<f64>,
    pub description: String,
}

pub struct ActionPreview {
    pub action: ActionDescriptor,
    pub allowed: bool,
    pub blockers: Vec<Blocker>,
    pub side_effects: Vec<String>,
    pub expected_result: String,
    pub evidence_required: Vec<EvidenceRef>,
    pub idempotency_key: String,
    pub confirmation: ConfirmationRequirement,
}
```

### 27.4 Action preview modal

```text
╭─ Action Preview: cancel superseded pipelines ───────────────────────────────────────────╮
│ Risk: CI mutation  Dry-run: complete  Grant: not required  Scope: veox-*                │
├─ Will cancel ───────────────────────────────────────────────────────────────────────────┤
│ 7 pipelines superseded by newer SHA; none are release/prod pipelines; 19 runner slots freed.│
├─ Expected impact ───────────────────────────────────────────────────────────────────────┤
│ frees 2h14m obsolete work; reduces rust-fast queue p95 from 12m04s to 8m33s.             │
├─ Evidence ──────────────────────────────────────────────────────────────────────────────┤
│ supersedence proof sup_771, pipeline list snapshot snap_882                             │
╰─ y execute  d dry-run again  e evidence  n cancel  Esc back ───────────────────────────╯
```

---

## 28. Rust implementation architecture

### 28.1 Recommended stack

- **Ratatui** for rendering widgets/layouts.
- **Crossterm** for terminal backend/input/alternate screen.
- **Tokio** for async tasks, timers, network streams, and channels.
- **serde / serde_json** for read-model/event/action contracts.
- **reqwest** for HTTP fallback client.
- **tokio-tungstenite** or SSE client for streaming.
- **sysinfo** for local CPU/memory/disk/process metrics in local mode.
- **tracing** for diagnostics.
- **unicode-width / textwrap** for robust terminal layout.
- **insta** or similar for golden snapshot tests.
- **proptest** for reducer/navigation/action safety invariants.

### 28.2 Module layout

```text
src/tui/
  mod.rs
  app.rs                     # top-level App, reducer, navigation stack
  main_loop.rs               # terminal event loop, render loop, async bridge
  config.rs                  # TUI config, theme, density, keymap
  theme.rs                   # semantic palette and glyph fallback
  keymap.rs                  # key bindings, command dispatch
  routes.rs                  # Route/Scope/entity navigation
  store/
    mod.rs
    entity_store.rs          # normalized entity cache
    view_cache.rs            # derived screen data
    ring_buffer.rs           # event/log buffers
    freshness.rs
  client/
    mod.rs
    inspection_client.rs     # trait
    http_client.rs
    ws_client.rs
    sse_client.rs
    mcp_client.rs
    cli_fallback.rs
    fake_client.rs
  model/
    entity.rs
    events.rs
    read_model.rs
    capacity.rs
    workflow.rs
    cache.rs
    vti.rs
    agents.rs
    bugs.rs
    release.rs
    evidence.rs
    security.rs
    jankurai.rs
  screens/
    flight_deck.rs
    queue_limit.rs
    repos.rs
    repo_detail.rs
    workflow.rs
    log_viewer.rs
    runners_system.rs
    cache.rs
    vti.rs
    agents.rs
    bugs.rs
    git_sync.rs
    bottlenecks.rs
    jankurai.rs
    churn.rs
    security.rs
    artifacts.rs
    release.rs
    evidence.rs
    source_doctor.rs
  widgets/
    table.rs
    virtual_list.rs
    sparkline.rs
    heatmap.rs
    progress.rs
    graph.rs
    timeline.rs
    event_tape.rs
    inspector.rs
    modal.rs
    command_palette.rs
    action_preview.rs
  layout/
    shell.rs
    breakpoints.rs
    panes.rs
  actions/
    registry.rs
    preview.rs
    execute.rs
  test_support/
    fixtures.rs
    fake_backend.rs
    golden.rs
```

### 28.3 Core traits

```rust
#[async_trait::async_trait]
pub trait InspectionClient: Send + Sync {
    async fn read_model(&self, scope: ScopeRef) -> Result<TuiReadModel>;
    async fn events_since(&self, cursor: u64, filter: EventFilter) -> Result<Vec<TuiEvent>>;
    async fn entity_detail(&self, entity: &EntityRef) -> Result<EntityDetail>;
    async fn pipeline_graph(&self, project_id: i64, pipeline_id: i64) -> Result<PipelineGraph>;
    async fn job_log_chunk(&self, job: &JobRef, cursor: LogCursor) -> Result<LogChunk>;
    async fn action_preview(&self, req: ActionRequest) -> Result<ActionPreview>;
    async fn action_execute(&self, req: ExecuteActionRequest) -> Result<ActionReceipt>;
}

pub trait Screen {
    fn id(&self) -> ScreenId;
    fn title(&self, app: &AppState) -> String;
    fn handle_key(&mut self, key: KeyEvent, app: &mut AppState) -> Command;
    fn render(&mut self, frame: &mut Frame, area: Rect, app: &AppState);
}

pub trait Component {
    fn focus_id(&self) -> FocusId;
    fn handle_input(&mut self, input: InputEvent, app: &mut AppState) -> Command;
    fn render(&mut self, frame: &mut Frame, area: Rect, app: &AppState, focused: bool);
}
```

### 28.4 App state

```rust
pub struct AppState {
    pub route_stack: Vec<Route>,
    pub focus: FocusState,
    pub selected: Option<EntityRef>,
    pub pinned: Vec<EntityRef>,
    pub read_model: Option<TuiReadModel>,
    pub entities: EntityStore,
    pub events: RingBuffer<TuiEvent>,
    pub logs: LogStore,
    pub freshness: SourceFreshnessMap,
    pub actions: ActionState,
    pub command_palette: CommandPaletteState,
    pub filters: FilterState,
    pub theme: Theme,
    pub density: DensityMode,
    pub terminal: TerminalCapabilities,
    pub diagnostics: DiagnosticsState,
}
```

### 28.5 Event loop

```text
startup:
  load config/theme/keymap
  enter alternate screen
  connect InspectionClient
  fetch initial read model
  start event stream task
  start log stream tasks on demand
  start local sysinfo sampler if local mode
  render first frame

loop:
  select!
    terminal input -> reducer
    tick 16-250ms depending animation/data changes -> render if dirty
    backend event -> apply event, mark dirty
    log chunk -> append ring buffer, mark dirty if visible
    action progress -> update modal/entity, mark dirty
    source heartbeat -> update freshness
    resize -> recompute layout, mark dirty
```

### 28.6 Render cadence

- Input latency target: `<50ms`.
- Render target: `30-60fps` only when animation/data changes; otherwise idle.
- Snapshot refresh: event-driven; full resync every 30-120s or on cursor gap.
- Log render: throttle to `10-20fps` while active to avoid terminal overload.
- Tables/lists: virtualize; render visible rows only.
- Graphs: cache layout; recompute only on graph topology changes or width changes.
- Avoid allocating heavily per frame; precompute styled spans where practical.

### 28.7 Backpressure

If event volume is high:

1. coalesce repeated updates by entity;
2. keep high-priority events immediately visible;
3. summarize low-priority events in event tape;
4. trigger read-model resync if cursor gap detected;
5. show `EVENT LAG` if client falls behind.

---

## 29. Backend plumbing roadmap

### P0 — Truth foundation

1. Expose `/api/read-model` using existing `src/api` types.
2. Expose `/api/entity/{kind}/{id}`.
3. Expose `/api/action/preview` and `/api/action/execute` using action registry metadata.
4. Expose source freshness and runtime profile.
5. Make demo/fake backend produce realistic fleet data.

### P0 — Realtime event/log stream

1. Add SSE or WebSocket `TuiEvent` stream.
2. Add bounded job trace stream.
3. Add cursor resume and gap detection.
4. Add action progress stream.
5. Keep polling fallback.

### P0 — Capacity and system utilization

1. Add capacity snapshot endpoint.
2. Add host/node CPU/memory/disk telemetry.
3. Add container stats/OOM/restart tracking.
4. Add queue constraint grouping.
5. Add runner scale simulator.
6. Add “should we increase runners?” recommendation model.

### P1 — Workflow graph

1. Multi-pipeline graph edges.
2. Downstream/bridge support.
3. Stage fallback edges.
4. Critical path computation.
5. Job progress/ETA model.

### P1 — Cache expansion

1. Cache categories.
2. Hot objects/miss storms.
3. Taints/verdicts/promotions.
4. Safe GC plan.
5. Per-node cache storage.

### P1 — VTI excellence

1. Summary endpoint.
2. Selector-miss drilldown.
3. Plan evidence.
4. Guardrail/fallback status.
5. Learning actions.

### P1 — Agent lifecycle

1. Add `agent_sessions`, `agent_tasks`, `agent_steps`, `agent_artifacts`, `agent_messages` or equivalent event-backed model.
2. Add race lifecycle status.
3. Add cleanup/winner APIs.
4. Add config validation endpoint.

### P1 — MR hook behavior

1. Ingest MR payloads into local state.
2. Track mergeability, approvals, discussions, changed files.
3. Link MR to pipelines, agents, bugs, evidence.

### P1 — Evidence timeline

1. Unified proof query endpoint.
2. Normalize evidence from DB families.
3. Redaction layer.
4. Timeline replay.

### P2 — Jankurai, security, artifacts

1. Jankurai structured ingestion.
2. Security finding normalization.
3. Artifact parsing: JUnit, coverage, code-quality, SAST, dependency, container scan, benchmark, nextest, release gates.
4. Signature/SBOM/provenance model.

### P2 — Autonomy unification

1. Bring kill bell, freeze, foundry, verdicts, launch ledger, LLM budget into main read model.
2. Add MCP resources for autonomy state.
3. Add replay and shadow result views.

### P3 — Predictive and economics

1. Time-to-green predictor.
2. CI cost/slot-hour dashboard.
3. Flake command center.
4. Dependency/toolchain drift.
5. Cross-repo dependency impact.
6. Policy simulator.

---

## 30. Testing strategy

### 30.1 Unit tests

- capacity calculations;
- limit-distance math;
- queue constraint grouping;
- scale recommendation decision tree;
- source freshness/staleness;
- event reducer;
- navigation stack;
- action risk classification;
- redaction;
- filter parser;
- graph layout ordering;
- VTI guardrails.

### 30.2 Golden render tests

Render deterministic screenshots for:

- Flight Deck wide/medium/narrow;
- Queue/Limit with saturation;
- Queue/Limit with low util + high queue fragmentation;
- Runners node memory pressure;
- Workflow graph failed job;
- Cache near full;
- VTI selector miss;
- Agent patch race;
- Bugs board;
- Release blocked by unsigned artifact;
- Evidence search;
- Source Doctor stale cache source;
- action preview modal.

### 30.3 Event replay tests

Use recorded/fake event streams to verify:

- event ordering;
- cursor resume;
- gap detection;
- stale source behavior;
- animation disable in capture;
- log chunk streaming;
- action progress;
- route drilldown remains valid after entity updates.

### 30.4 Integration tests

- fake backend HTTP/SSE/WS;
- CLI fallback;
- MCP resource read;
- action preview/execute dry-run;
- pipeline graph endpoint;
- capacity endpoint;
- evidence endpoint;
- cache endpoint;
- VTI endpoint;
- source doctor.

### 30.5 Performance tests

Test at these scales:

| Scale | Requirement |
|---|---|
| 100 repos | Global screen first useful render under 2s. |
| 1,000 active jobs | Table/graph navigation remains under 50ms input latency. |
| 10,000 recent events | Event tape/timeline virtualized. |
| 100 active logs | Only visible/pinned logs stream at high frequency. |
| 500 runner managers | Runners screen virtualized and grouped. |
| 1,000 cache objects hot list | Top-N and paginated detail. |

### 30.6 Safety tests

- no plaintext secrets in render snapshots;
- production actions always require typed confirmation;
- merge actions require evidence gate/grant;
- action execute cannot bypass preview;
- stale data blocks dangerous actions unless explicitly overridden with reason;
- redaction on copy/export;
- grants expiry displayed and enforced.

---

## 31. Implementation phases

### Phase 0 — Contract and fake backend

- Define final view-model structs.
- Implement fake backend with realistic event stream.
- Implement terminal shell, theme, keymap, navigation stack.
- Build golden screenshot harness.

### Phase 1 — Flight Deck and read model

- Consume `/api/read-model` or fake equivalent.
- Render Flight Deck with posture, families, hot signals, event tape.
- Implement entity drilldown shell and right inspector.
- Implement command palette read-only actions.

### Phase 2 — Queue/Limit and Runners/System

- Implement capacity endpoint/client.
- Render Queue/Limit and Runners/System screens.
- Add core/memory/disk telemetry and per-node detail.
- Add scale preview modal using action preview.

### Phase 3 — Workflow and logs

- Implement pipeline graph model/layout.
- Render Workflow Atlas and repo pipeline drilldown.
- Add live log viewer with annotations.
- Add critical path and ETA.

### Phase 4 — Cache, VTI, repos

- Implement repo family/repo dashboards.
- Implement Cache Observatory.
- Implement VTI cockpit and guardrails.

### Phase 5 — Agents, bugs, Git sync

- Add agent lifecycle tables/read model.
- Implement Agents/Autonomy cockpit.
- Implement Bugs board and detail.
- Implement Git Sync/MR state.

### Phase 6 — Release, security, artifacts, Jankurai, evidence

- Implement Release/Rollback screen.
- Implement Security/Secrets/Artifacts.
- Implement Jankurai screen.
- Implement Evidence Ledger search/replay.

### Phase 7 — Realtime polish and scale

- SSE/WS streaming with cursor resume.
- Motion grammar, density modes, capture mode.
- Backpressure, virtualization, profiler pass.
- Full demo mode and acceptance screenshots.

### Phase 8 — Predictive cockpit

- CI optimizer what-if simulations.
- Time-to-green predictor.
- Cost/slot-hour dashboard.
- Flake command center.
- Policy simulator and replay.

---

## 32. Acceptance criteria

### 32.1 User-level acceptance

1. From cold start, Flight Deck shows useful fleet posture in under 2 seconds using cached/fresh data.
2. The user can tell whether all repos are safe to code/merge/release within 5 seconds.
3. The user can see live queue across all repos and how close the fleet is to theoretical capacity.
4. The user can determine whether runner count should increase, decrease, or remain unchanged with a reason.
5. The user can drill from repo family → repo → pipeline → job → live trace → evidence capsule using `Enter` and return with `Esc`.
6. The user can see core, memory, disk, Docker, node, and runner pressure in one screen.
7. Cache fullness and category breakdown are visible in one tab.
8. VTI time saved, skips, confidence, and selector misses are visible in one tab.
9. Agents show sessions, tasks, grants, branches, MRs, logs, races, and evidence.
10. Bugs show statuses, attempts, branches, CI evidence, and agent ownership.
11. Git sync shows local/remote/mirror/MR/admission state.
12. Jankurai score, caps, trends, repair queue, and proof lanes are visible.
13. Security and secrets are visible without leaking plaintext.
14. Signed artifacts, SBOMs, and provenance are visible before release.
15. Release view shows gates, canary, prod, rollback, approvals, and evidence.
16. Evidence ledger can explain every warning and action.

### 32.2 System-level acceptance

1. Event stream supports cursor resume and gap detection.
2. Data freshness is visible for every source.
3. Stale data is clearly rendered and dangerous actions are blocked or escalated.
4. The TUI remains responsive during backend failure.
5. Tables and timelines are virtualized.
6. Graph layout is stable and cached.
7. Log viewer memory is bounded.
8. Rendering can be captured deterministically.
9. Demo mode can render compelling examples without live credentials.

### 32.3 Safety acceptance

1. No plaintext secrets are rendered, copied, exported, or stored in screenshots.
2. All mutating actions use preview → confirmation → execute → receipt.
3. Production actions require typed confirmation and evidence.
4. Merge actions require gate/grant evidence.
5. Scale/GC/drain actions show expected impact and risks.
6. Every action receives an idempotency key.
7. Every action result appears in evidence timeline.

---

## 33. The final desired feel

The finished TUI should feel like this:

- You open it and immediately see the whole engineering machine breathing.
- Repo families pulse with activity; red/yellow states explain themselves.
- Jobs flow through DAG lanes; critical paths glow; queues move like conveyor belts.
- Runner pools show not just utilization, but useful utilization and headroom.
- The TUI tells you whether adding runners helps, where to add them, and where it would make things worse.
- Cache, VTI, agents, bugs, Git sync, Jankurai, security, artifacts, and releases are not separate mysteries; they are integrated facets of the same graph.
- Every number has provenance.
- Every action is safe, previewed, and auditable.
- Every drilldown is fast and reversible.
- Every proof is one key away.

The final mantra:

> **Show the whole fleet. Explain every warning. Simulate every scale decision. Prove every action. Move at terminal speed.**

---

## 34. External implementation references

These are implementation-stack references only; the product design above is source/archive-derived.

- Ratatui official docs: https://ratatui.rs/
- Ratatui crate docs: https://docs.rs/ratatui/
- Crossterm crate docs: https://docs.rs/crossterm/
- Tokio official docs: https://tokio.rs/
- Tokio crate docs: https://docs.rs/tokio/
- sysinfo crate: https://crates.io/crates/sysinfo

---

## 35. Exhaustive current surface inventory appendix

This appendix keeps the implementation team grounded in the current source-derived surface area so the TUI does not invent data that JeRyu already has elsewhere.

### 35.1 Main CLI families

The current inventories identify these top-level command families:

| CLI group | Commands / purpose |
|---|---|
| Lifecycle | `init`, hidden `bootstrap`, `serve`, `down`, `system`, `status`. |
| Install | `install`, `install guided`, `install doctor`, `install smoke`, `install server`, `install uninstall`, `install render-demo`. |
| Remote host | `remote install`, `remote update`, `remote doctor`, `remote status`, `remote logs`, `remote restart`, `remote stop`, `remote start`, `remote ssh`, `remote run`, `remote tunnel`, `remote uninstall`. |
| TUI | `tui --once`, `--demo`, `--capture`, `--screenshot`, `--tab`, `--output`, `--width`, `--height`, `--screenshot-hold-ms`. |
| Git compatibility | `git <args...>`, `save <message>`, `sync`, `undo`. |
| Pools | `pool list`, `scale`, `pause`, `resume`, `drain`, `delete/remove`, `rotate-token`. |
| Jobs | `job list`, `trace`, `play`, `cancel`, `retry`, `explain`, `clear`. |
| Pipelines | `pipeline explain`, `doctor`, `jobs`, `ingest`, `cancel`, `bottlenecks`. |
| Cache | `cache enable`, `doctor`, `status`, `gc`. |
| Local cargo | `local cargo`, `local cargo-env`. |
| Logs | `logs <manager_id> --lines`. |
| Agents | `agent spawn`, `list`, `merge`, `submit`. |
| Settings | `settings repair`, `settings reset`. |
| Tests / VTI | `test run`, `plan`, `batch`, `results`, `requeue`, `failed`, `impact`, `select/choose`, `explain-plan`, `select-external`, `audit`, `learn`, `cache-status`. |
| Release | `release status`, `watch`, `reconcile`, `promote-prod`, `preflight`, `doctor`, `ready`, `dry-run`, `submit`, `approve`, `rollback`. |
| Secrets | `secrets provision/init`, `status`, `doctor`, `rotate`, `finalize`, `report`, `recover`. |
| Progress / actions | `progress`, `next`, `explain-blocker`, `action list`. |
| Repo control | `repo render-agent-index`, `audit-agent-surface`, `install-git-hooks`, `init`, `adopt`, `mode`, `hooks`, `standard`, `fleet`, `shadow`, `backup`, `jankurai-fast`, `redline-state-proof`, `capture-tui-screenshots`. |
| Bug tracker | `bug project add/list/show/link`, `bug submit`, `list`, `show`, `triage`, `link`, `ready`, `attempt start/fail/complete`, `sync`. |
| Policy | `policy audit`. |
| Host | `host storage-audit`, `doctor`, `reclaim`, `install-gc-timer`, `install-gcd-service`. |
| Node | `node add`, `list`, `remove`, `doctor`. |
| Hidden executor | `exec config`, `exec prepare`, `exec run`, `exec cleanup`. |
| Hidden Git hook | `server-hook pre-receive`. |
| Capability / MCP | `capability serve`, `mcp serve`, `mcp serve-http`, `mcp tools`. |

### 35.2 Current HTTP/API endpoints

| Endpoint | Current behavior | Desired TUI expansion |
|---|---|---|
| `GET /health` | Shallow health. | Add deep health or `?deep=1` with GitLab, DB, Docker, Vault, cache, broker, runner, disk, stream, config status. |
| `POST /hooks` | GitLab webhook ingestion. | Persist delivery metadata, raw body hash, parse status, broker offset, handler outcome, correlation IDs. |
| `GET /cache/summary` | Basic bytes/hits/objects/status. | Replace/extend with cache categories, taints, verdicts, hot objects, leases, GC plan, per-node storage. |
| `POST /mcp` | MCP tool calls over loopback HTTP. | Keep; add read resources and watch/stream support. |
| `DELETE /mcp` | Terminates session. | Keep. |
| `GET /mcp` | Currently disabled. | Enable MCP Streamable HTTP GET or keep disabled and add SSE/WS `/api/stream/events`. |
| Autonomy `GET /metrics` | Prometheus metrics for autonomy binary. | Mirror key metrics into main TUI read model. |
| Autonomy `GET /health` | Autonomy readiness/degraded state. | Surface in Source Doctor and Autonomy cockpit. |
| Autonomy `POST /events` | GitHub-shaped webhook receiver. | Surface event delivery, verdict, and ledger state in Evidence/Autonomy. |

### 35.3 GitLab REST data to exploit

The GitLab wrapper can already inspect or mutate many useful objects:

- project listing and project detail;
- project creation;
- project bot tokens;
- CI jobs by scope;
- job trace/log;
- artifact files;
- pipeline trigger/list/get/cancel;
- pipeline variables;
- pipeline jobs;
- bridges/downstream pipelines;
- recursive downstream job listing;
- runners;
- runner managers;
- runner pause/unpause;
- runner creation/deletion/token reset;
- issues create/list/update/comment;
- merge requests create/get/accept;
- branches create/delete/protect;
- files create/update/commit action batches;
- group webhooks for job, pipeline, push, and MR events.

### 35.4 GitLab webhook payloads to normalize

| Webhook | Payload data to preserve | TUI use |
|---|---|---|
| Job Hook | build/job ID, status, name, stage, queued duration, project ID, pipeline ID, tag flag, ref, runner info. | Live job lifecycle, queue wait, runner attribution, failure detection, scale decisions. |
| Pipeline Hook | pipeline ID, status, ref, SHA, project. | Tracked pipelines, main success/failure, release/canary trigger, prod status. |
| Push Hook | project ID, before/after SHA, ref, namespace/path. | Shadow main, cancel superseded pipelines, compute impact/VTI plan. |
| MR Hook | MR event type currently accepted/logged. | Must become MR local model: mergeability, approvals, changed files, discussions, labels, linked pipelines. |

### 35.5 Settings and default ports

The inventories repeatedly identify these defaults:

| Setting | Default / note |
|---|---|
| GitLab image | `gitlab/gitlab-ce:17.9.2-ce.0` |
| GitLab Runner image | `gitlab/gitlab-runner:v17.9.2` |
| GitLab hostname | `gitlab.local` |
| GitLab HTTP / SSH ports | `8929` / `2224` |
| Vault image | `hashicorp/vault:1.17.5` |
| Vault host port | `18200` |
| Webhook/API bind | `127.0.0.1:9777` |
| MCP bind | `127.0.0.1:9778` |
| Cache proxy / registry ports | `19800` / `19801` |
| Cache manager budget | around `400 GiB` |
| sccache | enabled, `10G`, binary `v0.9.1` in inventory |
| Release repo root | `/home/ubuntu/veox-repos/veox-deploy` in inventory example |
| Default release project ID | `48` in inventory example |
| TUI sync interval | around `5000 ms` in current settings |
| Live trace polling fallback | around `650 ms` in current TUI notes |

The TUI must render these as redacted effective runtime profile data, not hard-code them as universal truth.

---

## 36. Realtime-inspectable data catalog by domain

### 36.1 System and infrastructure

Data to show:

- daemon uptime and build metadata;
- DB backend/path/latency/migrations;
- GitLab readiness and latency;
- Docker daemon readiness and event stream status;
- Vault health/sealed/initialized/token-present metadata;
- cache proxy/registry health;
- broker backend/producer/consumer lag;
- source freshness;
- config profile and feature flags;
- stream cursor and lag;
- host/node CPU/memory/disk/network.

### 36.2 CI jobs and pipelines

Data to show:

- job ID, name, stage, status, allow-failure, ref, web URL;
- project/pipeline/root/downstream IDs;
- created/queued/started/finished timestamps;
- queued duration, run duration, historical p50/p95;
- runner ID, runner manager, system ID, pool, node;
- logs/traces and annotations;
- artifacts and parsed reports;
- failure capsules and retry decisions;
- supersedence and obsolete work;
- DAG edges and critical path.

### 36.3 Runners, pools, nodes

Data to show:

- pools, tags, trust tier, executor, paused state;
- min/max managers, concurrency, request concurrency;
- managers by state;
- Docker containers, config dirs, runner system IDs;
- local vs remote node placement;
- node SSH/Docker/storage health;
- CPU/memory/disk/IO/network telemetry;
- OOM/die/restart events;
- reconciliation status;
- scale/drain/GC history.

### 36.4 Cache and materialization

Data to show:

- bytes served;
- hits/misses;
- objects;
- categories;
- cache requests by URL template/method/status;
- hot entries;
- taints;
- leases;
- verdicts;
- promotions;
- force refresh rules;
- resolved refs;
- material objects/aliases;
- action cache;
- toolchain fingerprints;
- epochs;
- GC candidates and reclaimed bytes.

### 36.5 VTI / tests

Data to show:

- test plans;
- test plan items;
- selected/skipped/escalated tests;
- plan mode and confidence;
- test execution history and durations;
- selector misses;
- repaired/unrepaired miss state;
- subsystem mappings;
- failed tests;
- external selector input;
- learning events;
- validation results from `plan_validation`.

### 36.6 Release and production

Data to show:

- release attempts;
- project/ref/SHA/version;
- upstream/release/prod pipeline IDs/statuses;
- canary state/timestamps/notes;
- eligibility, phase, detail, state_status;
- gate files/paths;
- telemetry diagnostics;
- foundry candidates;
- rollback plan/drill state;
- VibeGate/Evidence Gate verdicts;
- approvals and blockers.

### 36.7 Secrets and Vault

Data to show, redacted:

- Vault address/status/initialized/sealed/healthy;
- mount and prefix;
- authority metadata;
- token fingerprint only;
- release secret set paths/status/expiry;
- rendered deploy/runtime env paths, not contents;
- audit paths and report/bundle paths;
- rotation/finalization/recovery timestamps;
- secret audit events.

### 36.8 Evidence, admission, capability

Data to show:

- evidence capsules;
- retry decisions;
- append-only events;
- capability intents/grants;
- grant expiry and bound SHA;
- admission decisions and reasons;
- Git command events;
- risk approvals;
- command artifacts;
- signed launch ledger records;
- action receipts.

### 36.9 Agents, bugs, LLMs

Data to show:

- agent sessions/tasks/steps/artifacts/messages;
- capability grants and budgets;
- patch races and hypotheses;
- branches/MRs/pipelines per task;
- bug records/events/attempts/links/external refs/evidence;
- LLM provider health, model, token counts, latency, cost estimate, refusal/failure counts, prompt/template IDs where safely available.

---

## 37. Detailed backend contracts

### 37.1 `GET /api/read-model` response sketch

```json
{
  "schema_version": 7,
  "generated_at": "2026-05-26T10:00:00Z",
  "event_cursor": 188442,
  "profile": {
    "name": "prod",
    "daemon_commit": "abc123",
    "db_backend": "sqlite",
    "redacted_settings_hash": "sha256:..."
  },
  "freshness": {
    "gitlab": { "age_ms": 900, "status": "fresh" },
    "db": { "age_ms": 400, "status": "fresh" },
    "docker": { "age_ms": 500, "status": "fresh" },
    "cache": { "age_ms": 9400, "status": "stale" }
  },
  "posture": {
    "safe_to_code": "ok",
    "safe_to_merge": "warn",
    "safe_to_release": "blocked",
    "top_blocker": "unsigned artifact rc.17"
  },
  "capacity": { "limit_distance_physics": 1.34, "queued_jobs": 47 },
  "repo_families": [],
  "attention": [],
  "next_actions": []
}
```

### 37.2 `GET /api/entity/{kind}/{id}` response sketch

```json
{
  "entity": { "kind": "Job", "id": "981273", "label": "integ-db" },
  "state": "failed",
  "summary": "postgres service health timeout",
  "freshness": { "age_ms": 700, "status": "fresh" },
  "timeline": [
    { "ts": "2026-05-26T12:04:00Z", "kind": "job.failed", "summary": "exit 1" }
  ],
  "blockers": [
    { "label": "failed job", "reason": "service health timeout", "confidence": 0.91 }
  ],
  "evidence": [
    { "id": "cap_981273", "kind": "failure_capsule", "digest": "sha256:..." }
  ],
  "related": [
    { "kind": "Pipeline", "id": "912", "label": "#912" },
    { "kind": "Bug", "id": "BUG-184", "label": "enclave db timeout" }
  ],
  "available_actions": [
    { "id": "job.retry", "risk": "ci_mutation", "dry_run_available": true }
  ]
}
```

### 37.3 `GET /api/capacity` response additions

Include:

- raw pool slots;
- adjusted/effective slots;
- loss decomposition;
- queue constraints;
- DAG physics bound;
- schedule simulation result;
- resource headroom;
- recommendations;
- confidence and data quality notes.

```rust
pub struct QueueConstraint {
    pub label: String,
    pub kind: QueueConstraintKind,
    pub matching_jobs: u32,
    pub queued_work_secs_p50: f64,
    pub p95_wait_secs: f64,
    pub bottleneck_entity: Option<EntityRef>,
    pub suggested_fix: String,
    pub no_runner_fix: bool,
}

pub enum QueueConstraintKind {
    RunnerTag,
    TrustTier,
    ProtectedBranch,
    PoolPaused,
    PoolSaturated,
    NodeOffline,
    CpuPressure,
    MemoryPressure,
    DiskPressure,
    DockerUnhealthy,
    CacheCold,
    ImagePull,
    SerialDag,
    PolicyGate,
    ApprovalGate,
    ReleaseGate,
    Unschedulable,
}
```

---

## 38. Search, filters, lenses, and saved views

### 38.1 Global search

`/` filters the current pane. `Ctrl+/` or `:search` opens global search.

Global search scopes:

```text
repo:veox-*        family:veox-*       status:failed
kind:job           kind:bug            severity:high
pool:rust-fast     node:remote-3       tag:gpu
age:<1h            since:24h           stale:true
sha:b13c9a1        branch:main         mr:42
agent:enclave      grant:active        vti:miss
cache:tainted      artifact:unsigned   release:blocked
```

### 38.2 Filter grammar

Minimal grammar:

```text
query      = expr { space expr }
expr       = field_op | bare_text | negation | group
field_op   = field (":" | "=" | "!=" | ">" | "<" | ">=" | "<=") value
negation   = "!" expr
bare_text  = quoted | word
group      = "(" query ")"
```

Examples:

```text
family:veox-* status:failed
kind:job pool:rust-fast age:<1h
status!=done severity>=medium repo:veox-enclave
cache:tainted OR artifact:unsigned
agent:* grant:expired
```

### 38.3 Saved lenses

Allow saving views:

- `My release blockers`
- `veox-* critical path`
- `runner saturation`
- `VTI misses 7d`
- `agent work needing review`
- `unsigned artifacts`
- `stale source doctor`
- `bugs ready for agents`

Saved lenses are just named filters + screen + scope + columns.

---

## 39. Critical user flows

### 39.1 “Should we increase runner count?”

1. Open Flight Deck.
2. See `Limit distance 1.34×`, `rust-fast tag bottleneck`, `CPU/Mem/Disk headroom`.
3. Press `g q` for Queue/Limit.
4. Select `rust-fast`.
5. Press `x` to explain limit model.
6. Press `s` for scale preview.
7. Review resource impact and expected p95 wait reduction.
8. Execute dry-run.
9. Execute scale if safe.
10. Evidence receipt appears in event tape and Evidence Ledger.

### 39.2 “Why is release blocked?”

1. Press `g p` Release.
2. Release page highlights blocking gates.
3. Select unsigned artifact.
4. Press `Enter` to Artifacts detail.
5. Press `e` for provenance proof.
6. Press `a` for sign/rerun signing action preview.
7. Execute safe action or attach issue.

### 39.3 “Is VTI working?”

1. Press `g t`.
2. See saved time/confidence/misses.
3. Select repo with misses.
4. Press `Enter` plan detail.
5. Inspect changed files, selected/skipped tests, miss evidence.
6. Press `l` to learn mapping or `f` to force full run.

### 39.4 “What is an agent doing?”

1. Press `g a`.
2. Select agent.
3. Inspect current task, grants, branch, MR, pipeline, budget.
4. Press `L` logs, `e` evidence, `g` grants, or `p` pause.
5. Drill into associated bug or MR.

### 39.5 “Is cache full and what is taking space?”

1. Press `g c`.
2. Category bars show top storage categories.
3. Select Cargo target or Docker layers.
4. Press `Enter` for hot objects.
5. Press `g` for GC preview.
6. Review safe/risky reclaim and active leases.

### 39.6 “Which bug should agents work next?”

1. Press `g b`.
2. Filter `status:ready severity>=medium family:veox-*`.
3. Sort by critical-path/release impact.
4. Select bug and inspect attempts/evidence.
5. Press `A` assign/spawn agent.
6. Preview grant/budget/branch/pipeline side effects.

### 39.7 “Is Git remote/main/mirror state clean?”

1. Press `g g`.
2. Look for ref drift/mirror lag/admission denies.
3. Select repo/MR/ref.
4. Drill into MR state, linked pipeline, approvals, changed files.
5. Use evidence for denied push or drift.

---

## 40. Dream features beyond MVP

### 40.1 Incident room mode

A focused mode for active incidents:

- pins affected repos/pipelines/releases;
- freezes nonessential animations;
- opens evidence timeline and event tape;
- tracks owner/action items;
- records decisions;
- exports redacted incident bundle.

### 40.2 Time-travel replay

Replay state at time `T` or between events:

```text
:replay since 11:30 repo:veox-enclave
:replay event cap_981273
```

Use durable event log plus snapshots. This is invaluable for “how did this release become blocked?”

### 40.3 CI optimizer report

One-key report from Flight Deck:

```text
1. Scale rust-fast +6 during workday: saves 41m/day, confidence .83.
2. Split integ-db into 3 shards: saves 28m/day, confidence .71.
3. Pre-pull sec-scan image: saves 19m/day, confidence .78.
4. Teach VTI mapping for src/api/events.rs: reduces miss rate .42% → .18%.
5. Cancel superseded pipelines automatically after 3m grace: saves 2h14m obsolete work/day.
```

### 40.4 Flake command center

Show:

- flaky tests by repo/subsystem;
- retry storm impact;
- flake signatures;
- quarantine status;
- owner;
- linked bugs;
- trend;
- recommended quarantine/fix.

### 40.5 Agent collision detector

Detect agents editing same files, branches, bugs, or subsystems. Show collision risk and merge conflict forecast.

### 40.6 Dependency/toolchain drift

Show drift in:

- Rust toolchains;
- crate lockfiles;
- Docker base images;
- GitLab runner versions;
- Vault image versions;
- Node/npm/pnpm toolchains;
- CI templates;
- generated code.

### 40.7 Policy simulator

Before changing a policy, simulate:

- which MRs would newly fail;
- which releases would block;
- which agents would lose grants;
- which workflows become stricter/weaker.

### 40.8 Pair/operator mode

A read-only share mode with stable key hints and redacted output for pairing or screen sharing.

### 40.9 Soundless alert intensity

For a terminal, severity can be shown without sound:

- border intensity;
- event tape position;
- pulse speed;
- red/yellow count in header;
- focus stealing only for production-critical events.

---

## 41. Implementation checklist for agents/builders

1. Reuse existing `src/api` types where possible rather than creating parallel truth.
2. Define final `EntityKind`, `TuiEvent`, `TuiReadModel`, `ActionPreview`, and `ActionResult` schemas.
3. Implement fake backend first; no screen should require live GitLab to develop.
4. Build terminal shell, layout breakpoints, theme, keymap, focus model.
5. Implement navigation stack and universal drilldown before domain screens.
6. Implement source freshness and stale rendering early.
7. Implement Flight Deck with fake data.
8. Implement Queue/Limit and capacity model with fake and then real endpoint.
9. Add system telemetry sampler and remote node metrics.
10. Implement action preview modal before any mutating action.
11. Implement Evidence Ledger redaction before export/copy actions.
12. Implement Workflow graph with cached layout.
13. Implement log viewer with bounded memory.
14. Implement Cache/VTI/Agents/Bugs as domain screens with shared list/detail widgets.
15. Add demo/capture mode and golden tests.
16. Add performance instrumentation.
17. Add streaming client and reconnect/gap handling.
18. Add final polish: animation, density, saved lenses, replay.

---

## 42. “Should we add runners?” scenario matrix

| Observed state | TUI diagnosis | Recommended action |
|---|---|---|
| `busy≈usable`, high p95 queue, nodes have CPU/mem/disk headroom | True capacity bottleneck. | Increase managers/runners in constrained pool. |
| `busy≈usable`, high p95 queue, nodes memory red/OOM | Node memory bottleneck. | Add larger/extra nodes or reduce concurrency; do not add managers on same node. |
| High queue for one tag, low global utilization | Tag fragmentation or missing eligible runners. | Fix tags/pool affinity/trust/protected runner config. |
| High queue, many jobs unschedulable | No matching runner or offline node. | Add matching runner, repair node, or reroute tags. |
| High queue, cache miss storm/image cold pulls | Cache/image bottleneck. | Warm cache/pre-pull/fix taints before scaling. |
| High queue, critical path serial with idle runners | DAG/test structure bottleneck. | Split/shard/add `needs`; runner count has low impact. |
| High runner utilization but many superseded pipelines | Wasted obsolete work. | Cancel superseded pipelines; then reassess. |
| Release blocked by approval/canary/signature | Policy/release gate. | Resolve gate; runners do not help. |
| Low queue, high utilization | Healthy saturation. | No immediate scale; monitor forecast. |
| Low queue, low utilization, high warm managers | Overprovisioned. | Scale down or drain warm managers if cost matters. |

The UI should show this matrix implicitly through diagnosis labels, not require the user to memorize it.

---

## 43. Final build target checklist

A build is not “done” until a developer can perform these keystroke paths:

```text
g f              # open Flight Deck
g q x            # explain theoretical limit
g q s d          # scale dry-run preview
g s Enter        # node/pool utilization detail
g r Enter        # repo family drilldown
g w Enter L      # pipeline/job/log drilldown
g c g            # cache GC preview
g t m            # VTI misses
g a Enter e      # agent detail evidence
g b A            # assign bug to agent preview
g g Enter        # MR/git sync detail
g p e            # release evidence
g z              # security/secrets/artifacts
g j              # Jankurai quality
g e / sha:...    # proof search
:why not green   # synthesized blocker answer
```

The final `:why not green` answer should look like:

```text
Not green because:
1. veox-enclave pipeline #912 failed integ-db at 12:04; capsule cap_981273; release-blocking.
2. veox-web rc.17 artifact is unsigned; signing gate blocks release.
3. rust-fast pool p95 queue wait is 12m04s; +6 managers on remote-1/2 likely saves 41m/day.
4. VTI has 3 selector misses in api/events; next release should full-run that subsystem.
5. remote-3 memory is 91% p95 with 2 OOMs; do not schedule more managers there until GC/memory fix.
```

That answer is the soul of the product: concise, evidence-linked, operationally useful, and immediately actionable.
