# JeRyu Hyperdeck — Final Rust TUI Engineering Specification

**Working names:** `jeryu tui` vNext, **Flight Deck**, **Hyperdeck**, **Mission Control**  
**Audience:** Rust implementation agents, backend/control-plane engineers, and product operators building the definitive multi-repository CI/CD terminal cockpit.  
**Primary goal:** make one terminal show *everything* moving in real time across repo families, repos, pipelines, runners, caches, agents, bugs, release gates, security, artifacts, and evidence, with instant keyboard drill-down and safe action execution.

This specification supersedes the earlier design drafts in the archive by merging their best ideas into one build-ready plan. It is intentionally detailed: the TUI should feel like an observatory, debugger, release cockpit, agent console, and proof ledger combined.

---

## 0. Source corpus studied

The archive contained two kinds of source material:

1. **API/data inventory notes**: `tip1.txt` through `tip9.txt`. These describe the realtime data surfaces JeRyu already has or can plausibly expose: CLI, TUI, MCP, capability socket, webhook server, GitLab REST, message broker, custom executor, pre-receive hook, SmartCache, Docker/runners, Vault/secrets, durable state DB, local bug tracker, autonomy/evidence-gate state, GitHub/GitLab host abstractions, and Jankurai audit tooling.
2. **Prior dream TUI drafts**: eight large Markdown specs named `jeryu_dream_*`. They converge on the same core design: a typed read-model cockpit, fleet-first dashboard, repo-family drilldown, workflow DAG, action registry, proof/evidence timeline, cache/VTI/agents/bugs/release/security screens, strong keyboard navigation, color-rich motion, freshness markers, and strict safety gates.

Apple `._*` metadata files were ignored. The useful content was in the `.txt` and `.md` files.

---

## 1. North star

JeRyu Hyperdeck is a **terminal-native realtime operating room for software delivery**.

A developer should be able to run:

```bash
jeryu tui
```

and within five seconds know:

- which repo family is blocking the fleet;
- how close the CI system is to the theoretical throughput limit;
- which jobs are queued, running, failed, blocked, or starved by runner tags;
- whether VTI is safely skipping tests or hiding risk;
- whether caches are fast, full, stale, tainted, or causing misses;
- what autonomous agents are doing, what grants they have, and whether their work is trustworthy;
- which bugs/issues are pending, racing, proposed, verified, or blocked;
- whether Git remotes, PRs/MRs, branches, mirrors, and local working trees are in sync;
- what Jankurai, security, secret, artifact, and release evidence says;
- whether it is safe to merge, promote, or roll back;
- exactly what action to take next, and why.

The final experience should be summarized as:

```text
one terminal + total fleet awareness + zero mystery + safe high-speed autonomy
```

---

## 2. Hard product laws

### 2.1 Every visible object is addressable

Every row, glyph, graph node, alert, cache object, test-plan receipt, agent task, bug, gate, artifact, grant, and event must be selectable. When selected, it must support at least one of:

- `Enter` drill down;
- `Esc` go up;
- `a` actions;
- `e` evidence;
- `l` logs/traces;
- `x` explain;
- `o` open related URL/path;
- `y` copy stable ID/SHA/digest/path;
- `p` pin/watch;
- `/` filter related data;
- `?` contextual help.

A pane that shows facts but cannot be drilled into is considered incomplete.

### 2.2 Evidence over vibes

No important green status may be merely decorative. Healthy statuses need a proof path: commit SHA, ref, pipeline/job IDs, test-plan receipt, cache verdict, security scan, artifact signature, release gate, admission decision, capability grant, Jankurai run, or freshness timestamp.

If proof is missing, the UI must say so with explicit badges such as:

```text
OK?   HEUR   STALE   NO PROOF   UNVERIFIED   PARTIAL   SOURCE DOWN
```

### 2.3 Real time, but not theater

Motion is valuable only when it conveys changing state. Use animated spinners, sparklines, live event tapes, streaming progress bars, and pulse highlights for actual events. Do not animate stale or inferred data as if it were live.

### 2.4 Keyboard first, mouse friendly

The core navigation grammar is:

```text
arrows = move through space
Tab / Shift-Tab = switch sibling panes or tabs
Enter = drill down
Esc = drill up / back out
```

Mouse support is optional: click to focus, double-click to drill, wheel to scroll, right-click for action menu. No critical workflow may require a mouse.

### 2.5 No blank screens

The TUI may show fresh data, stale data, loading skeletons, degraded-source warnings, explicit empty states, cached snapshots, synthetic fallback from recent events, or demo fixtures. It must never silently render an empty pane because a source missed one refresh.

### 2.6 One action path

Mutating operations must flow through the action registry / capability model. The TUI must not create a second mutation path. Actions are previewed, risk-classified, grant-checked, idempotency-aware, and audited.

### 2.7 Staleness is a first-class visual state

Every screen must know the age, source, cursor, and last error of its data. Freshness badges should be visible in the header and drillable through the Source Doctor.

---

## 3. Source-derived JeRyu baseline

### 3.1 Existing control/data surfaces

| Surface | Current entrypoint / transport | What Hyperdeck should use it for |
|---|---|---|
| CLI | `jeryu <command>` | Install, serve, remote, node, git/save/sync/undo, status, pools, jobs, pipelines, cache, logs, agents, settings, tests/VTI, release, secrets, progress, bugs, policy, host, MCP, next action, blocker explanations, action registry. |
| Current TUI | `jeryu tui` | Ratatui/crossterm shell, tabs, live log tailing, screenshot/capture modes, existing mission/workflow/cache/release/evidence concepts. |
| MCP stdio | `jeryu mcp serve` | Agent-facing JSON-RPC tools over stdin/stdout. |
| MCP Streamable HTTP | `jeryu mcp serve-http`, default loopback `127.0.0.1:9778`, `POST /mcp`, `DELETE /mcp` | Agent tooling surface; currently tool-oriented. Hyperdeck should influence resources/streams added here. |
| Capability API | `jeryu capability serve <socket>` | Local supervised-agent intent API with envelopes, actor, nonce, expiry, grants, budgets, idempotency. |
| Webhook/API engine | Axum server, default `127.0.0.1:9777` | Current `/health`, `/hooks`, `/cache/summary`; future read-model/events/action/proof APIs. |
| GitLab REST wrapper | internal `GitlabClient` | Projects, pipelines, jobs, traces, artifacts, variables, runners, MRs, issues, branches, webhooks, downstream pipelines. |
| GitLab webhooks | `POST /hooks` | Job, Pipeline, Push; MR accepted/logged but not yet fully acted on. |
| Message log / broker | Kafka or Jansu feature gated | Typed topics for webhook jobs, pipelines, pushes; future event delivery observability. |
| Custom executor | `jeryu exec config/prepare/run/cleanup` | GitLab Runner lifecycle, sandbox state, job env, logs, failure/quarantine capsules. |
| Git server hook | `jeryu server-hook pre-receive` | Admission policy, actor kind, grant linkage, allow/deny verdicts, ref updates. |
| SmartCache gateway | cache proxy `19800`, OCI registry mirror `19801` | Cargo sparse config, crate downloads, CAS hits, cache requests/objects, taints, leases, verdicts, promotions, singleflight. |
| Docker/runner control | Bollard + compose + remotes | Managed runner containers, logs, lifecycle, Docker events, OOM/death, node reconciliation. |
| Vault/secrets | Vault HTTP + local DB | Redacted health, authorities, release secret sets, rotation/finalization, audit metadata. |
| State DB | SQLite default, RedlineDB optional | Durable truth for pools, managers, jobs, pipelines, releases, evidence, cache, VTI, grants, bugs, LLM budget, autonomy ledgers. |
| Bug tracker | CLI + MCP + DB | Canonical cross-project bugs, events, attempts, links, external refs, evidence. |
| Autonomy / Evidence Gate | separate CLI/server + DB/ledgers | Kill bell, freeze windows, verdicts, launch ledger, foundry queue, PR drift, LLM budget, provider health. |
| GitHost abstraction | GitHub/GitLab adapters | PR/MR state, diffs, comments, approvals, checks, workflow runs, merge-passport policy SHA. |
| Jankurai | repo audit tooling/action | Audit score, anti-patterns, duplicate/rot findings, version/tool enforcement, fix proposals. |

### 3.2 Existing MCP tool surface

The current source-derived inventory says JeRyu exposes 16 `jeryu.*` MCP tools. Hyperdeck should show these in the action registry/source doctor and should never hardcode a stale list.

| Tool | Kind | Hyperdeck usage |
|---|---:|---|
| `jeryu.fetch_capsule` | read | Latest structured failure capsule for a job. |
| `jeryu.get_system_snapshot` | read | Seed global status, GitLab readiness, pool count, recent job events, latest release. |
| `jeryu.get_pipeline_jobs` | read | Pipeline/job drilldown, including downstream-expanded jobs. |
| `jeryu.get_ci_bottlenecks` | read | Bottleneck lab and queue analytics. |
| `jeryu.explain_blockers` | read | Explain failures, releases, merge blockers, selector misses. |
| `jeryu.plan_validation` | read | Validate VTI/test plan against selector misses and expected coverage. |
| `jeryu.run_tests` | mutate | Trigger targeted tests via ephemeral CI branch. |
| `jeryu.propose_patch` | mutate | Agent patch proposal, branch/MR creation. |
| `jeryu.race_patches` | mutate | Multi-hypothesis patch racing. |
| `jeryu.request_merge` | high-risk mutate | Merge request acceptance; must be proof-gated and audited. |
| `jeryu.bug_submit` | local mutate | Create canonical local bug. |
| `jeryu.bug_list` | read | Bug board. |
| `jeryu.bug_show` | read | Bug detail with events/attempts. |
| `jeryu.bug_ready` | read | Agent-ready bugs, failed-attempt filters. |
| `jeryu.bug_update` | local mutate | Triage/edit bug fields. |
| `jeryu.bug_record_attempt` | local mutate | Append agent attempt history. |

### 3.3 Current HTTP/API surface

Current public HTTP surface is intentionally small:

| Method | Path | Auth | Current role |
|---|---|---|---|
| `GET` | `/health` | none | returns a minimal `ok`. |
| `POST` | `/hooks` | `X-Gitlab-Token` | consumes GitLab webhook bodies and routes Job/Pipeline/Push through broker when enabled. |
| `GET` | `/cache/summary` | `X-Jeryu-Token` per source-derived notes | returns cache bytes/hits/objects/status. |

Hyperdeck requires a richer inspection plane; see section 10.

### 3.4 Current TUI facts to preserve

The existing TUI already has valuable primitives that should be kept:

- Ratatui/crossterm raw-mode alternate-screen application.
- Deterministic `--once`, capture, screenshot, and demo-friendly behavior.
- Tabs for Mission, Release, Jobs, Agents, Tests, Pools, Cache, Evidence, Secrets, LLMs, Git, and related screens in drafts.
- Workflow/Delivery concepts: PR rail, canonical phases, DAG canvas, minimap, inspector, macro/micro focus.
- Live job list, log preview, maximized log view.
- Background workers for snapshot sync, flow collection, selected-job live log polling.
- Anti-blanking: preserve last meaningful state and mark stale instead of flashing empty.
- Command palette preview from action registry.
- Evidence pane with capsule and audit-ledger modes.
- Cache pane with storage/gateway/singleflight/trust/taint information.
- Tests pane with bottleneck and selected-test history concepts.

Known limitations to explicitly fix:

- Selected job logs are currently polling-based, roughly sub-second, rather than WebSocket/SSE stream based.
- Flow board renders only the first active pipeline in some prior implementation notes.
- Graph edges are not fully computed.
- ETAs are heuristic and must show confidence.
- Evidence is not yet a full searchable proof timeline.
- Agents do not yet have a dedicated lifecycle table.
- MR webhooks are accepted/logged but not acted on as first-class data.
- MCP is tool-heavy and lacks first-class resources/stream subscriptions.
- Documentation/action/MCP lists have drifted; generated registry/docs should replace hardcoded truth.

---

## 4. The operating model

Hyperdeck must present JeRyu as a live entity graph:

```text
Fleet
  ├── RepoFamily: veox-* / veox-deploy / veox-enclave / infra / isolated / external
  │     ├── Repo
  │     │     ├── Branch / PR / MR
  │     │     │     ├── Pipeline / Workflow / Patch Race / Release Train
  │     │     │     │     ├── Stage / Phase
  │     │     │     │     │     ├── Job / Agent Task / Release Gate / Test Plan
  │     │     │     │     │     │     ├── Logs / Traces / Artifacts / Evidence / Actions
  │     │     │     │     │     └── VTI Receipt / Selector Miss / Jankurai Finding
  │     │     │     ├── Bug / Issue / Attempt
  │     │     │     ├── Security Finding / Secret Event
  │     │     │     └── Artifact / SBOM / Provenance
  │     │     ├── Cache Namespace
  │     │     ├── Git Sync State
  │     │     └── Release / Rollback State
  │     └── Family Rollups
  └── Global Resources: runners, pools, nodes, cache, Vault, GitLab, brokers, MCP, LLM providers
```

Every screen is just a lens over this graph. The user moves through the graph with arrows, tabs, Enter, and Esc.

---

## 5. Top-level information architecture

### 5.1 Primary lenses

The wide-terminal top nav should expose these lenses. Compact layouts can collapse them into a command palette and left rail.

| Lens | Primary question | Required drilldown |
|---|---|---|
| **Global** | Is the whole fleet healthy, fast, and safe? | family → repo → pipeline/job/agent/bug/release/security |
| **Queue** | How close are we to theoretical limit? | tag/pool bottleneck → runner/node/job |
| **Repos** | Which repo/family needs attention? | family → repo dashboard → workflow/subtabs |
| **Workflow** | What is running, blocked, or failed? | DAG node → job → trace/evidence/action |
| **Runners** | Can we add/use more runners? | pool → runner manager → node → logs/actions |
| **Cache** | Are we full, slow, tainted, or missing? | category → object → request/verdict/GC action |
| **VTI** | Is smart test skipping safe and valuable? | plan → test → selector miss → repair action |
| **Agents** | What are agents doing and should I trust them? | session → task → step/log/grant/MR/evidence |
| **Autonomy** | What automation is allowed right now? | workflow → policy → kill bell/freeze/verdict |
| **Bugs** | What is pending, being worked, fixed, or blocked? | bug → attempts → branch/MR/CI/evidence |
| **Git Sync** | Are local, remote, PR/MR, mirror states aligned? | repo/branch → commit/ref update → action |
| **Bottlenecks** | Why is CI slow? | stage/job/pool/cache/VTI dimension → proof |
| **Jankurai** | Is code quality improving? | finding → file/dupe/rot → agent/fix path |
| **Security** | Is build/merge/release safe? | finding/secret/grant/admission → proof/action |
| **Artifacts** | What did we build and can we trust it? | artifact → digest/signature/SBOM/provenance |
| **Release** | What version is where, can we promote or roll back? | gate → evidence → action modal |
| **Evidence** | Show me the receipts. | event/capsule/grant/decision → raw details |
| **Settings** | What is enabled, stale, misconfigured, or drifting? | source doctor/runtime profile/action registry |
| **LLMs** | Are model calls healthy and cost-bounded? | provider → call → token/cost/tool fanout/evidence |

### 5.2 Default route

Default route should be:

```text
Global Mission Control
```

Alternative power-user default can be configured:

```text
Workflow Atlas
```

The default screen must include a clear status header, live queue/capacity, repo-family rollup, active work, attention queue, event tape, and context inspector.

---

## 6. Universal shell layout

### 6.1 Wide layout, 160+ columns

```text
┌ JERYU HYPERDECK ─ prod ─ db:sqlite ─ gitlab:ok  docker:ok  cache:84%  vti:73%  event:184923↑ fresh:0.8s ──────────────┐
│ Tabs: [Global] Queue Repos Workflow Runners Cache VTI Agents Autonomy Bugs Git Bottlenecks Jankurai Security Artifacts Release Evidence │
├───────────────┬───────────────────────────────────────────────────────────────────────────────────────┬───────────────────┤
│ NAV / FILTERS │ MAIN LIVE CANVAS                                                                       │ INSPECTOR / PROOF  │
│ family tree   │ tables, DAGs, graphs, sparklines, activity lanes, event streams                       │ selected entity    │
│ quick lenses  │                                                                                       │ actions/evidence   │
├───────────────┴───────────────────────────────────────────────────────────────────────────────────────┴───────────────────┤
│ EVENT TAPE / STATUS STRIP: latest facts, hotkeys, stream state, action previews, frame timings, source warnings             │
└─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 6.2 Ultra-wide layout, 220+ columns

Use five zones:

1. left repo-family rail;
2. global attention queue;
3. central canvas;
4. right inspector;
5. bottom event tape + pinned watch objects.

This mode should feel like a cockpit wallboard without sacrificing keyboard drilldown.

### 6.3 Medium layout, 110–159 columns

Use three zones:

1. top posture/header;
2. central main canvas;
3. bottom stacked inspector/event tape.

Left nav becomes a compact single-column family list or a tab bar.

### 6.4 Compact layout, 80–109 columns

Use stacked panes:

```text
HEADER
TABS
ATTENTION / SUMMARY
MAIN SELECTED TABLE OR DAG
INSPECTOR COLLAPSED
STATUS STRIP
```

Compact mode should preserve all functionality through `g` shortcuts and the command palette.

### 6.5 Tiny/CI capture layout, under 80 columns

This mode is for screenshots, SSH sessions, CI logs, or small terminals. Use ASCII fallback, no heavy unicode, minimal borders, and explicit line wrapping.

---

## 7. Visual language and moving activity

### 7.1 Terminal capability tiers

| Tier | Capability | Behavior |
|---|---|---|
| Truecolor + Unicode | modern terminal | Full palette, braille sparklines, box drawing, smooth glyphs. |
| 256-color + Unicode | common terminal | Similar semantics, simpler gradients. |
| 16-color | conservative terminal | Strong status colors, no subtle gradients. |
| Monochrome / no Unicode | logs/CI/limited SSH | ASCII glyphs, explicit text labels. |

### 7.2 Semantic colors

| Semantic state | Color family | Glyph | Text fallback |
|---|---|---|---|
| healthy/success | green | `✓` | `OK` |
| running/live | cyan/blue | `▶`, spinner | `RUN` |
| queued/waiting | gray/white | `…` | `QUEUE` |
| blocked | yellow/amber | `⛔`, `!` | `BLOCK` |
| warning/degraded | amber | `▲` | `WARN` |
| failed | red | `✗` | `FAIL` |
| critical/security | red + bold/magenta | `‼` | `CRIT` |
| stale | dim gray | `◌` | `STALE` |
| agent/autonomy | purple | `◆` | `AGENT` |
| cache | cyan | `◇` | `CACHE` |
| release/prod | blue/green/yellow/red by gate | `⬢` | `REL` |
| evidence/proof | gold/yellow | `§` | `PROOF` |
| secret/redacted | magenta/dim | `◼` | `REDACTED` |

### 7.3 Motion rules

Use animation only for real changes:

- spinners for running jobs, agents, active gates, live streams;
- pulse highlight on rows updated in the last 2 seconds;
- live sparkline for queue depth, cache hits, runner utilization, event rate;
- mini waterfall for pipeline stages;
- moving event tape at the bottom with coalescing under high volume;
- progress bars with ETA confidence bands;
- DAG edges that brighten when upstream/downstream state changes;
- source freshness heartbeat in the header.

Never animate stale data as if it is live. Stale objects should dim and show age.

### 7.4 Progress bars

Canonical job row:

```text
test-linux ▶ ███████████████░░░░░ 74%  04:12/05:41  runner:r17  cache:hit  logs:2.1k/s  conf:med
```

Semantics:

- completed segment = green;
- running segment = cyan/blue;
- waiting/blocked segment = amber overlay;
- failure point = red marker;
- cached/skipped segment = dim/cyan;
- low-confidence ETA = `?` or dotted bar.

### 7.5 Density modes

- `d` cycles density: compact → normal → verbose.
- Verbose mode shows more proof/freshness text.
- Compact mode favors single-line rows, sparklines, and inspector drilldown.

---

## 8. Keyboard, focus, and drilldown model

### 8.1 Universal keymap

| Key | Action |
|---|---|
| `q` | Quit after confirmation if actions/streams are active. |
| `Esc` | Go up one level, exit drill mode, close modal, or clear search. |
| `Enter` | Drill into focused pane/entity/action. |
| `Backspace` | Navigate back in route history. |
| `Tab` | Next pane/tab/subtab depending context. |
| `Shift-Tab` | Previous pane/tab/subtab. |
| `↑↓←→` | Move focus/selection; in DAG mode, move graph-neighbor aware. |
| `h j k l` | Vim aliases for arrows. |
| `Ctrl-K` or `:` | Command palette. |
| `/` | Search/filter current scope. |
| `?` | Contextual help overlay. |
| `g` then key | Go-to shortcut menu. |
| `b` | Jump to top blocker. |
| `c` | Jump to critical path. |
| `e` | Evidence for selected entity. |
| `l` | Logs/traces for selected entity. |
| `t` | Timeline for selected entity. |
| `a` | Actions for selected entity. |
| `x` | Explain selected entity or screen. |
| `o` | Open URL/path externally. |
| `y` | Copy ID/SHA/path/digest. |
| `p` | Pin/unpin selected entity to watch panel. |
| `f` | Follow/unfollow live entity/event tail. |
| `r` | Contextual refresh/retry/rerun preview. |
| `s` | Contextual scale/sync/security/settings action; always preview. |
| `Ctrl-R` | Refresh current dashboard. |
| `Ctrl-S` | Save/export current view snapshot. |
| `!` | Attention-only mode: show only what needs human attention. |

### 8.2 Go-to shortcuts

```text
g0  Global
gq  Queue
gr  Repos
gw  Workflow
gu  Runners
gc  Cache
gv  VTI
ga  Agents
go  Autonomy
gb  Bugs
gg  Git Sync
gx  Bottlenecks
gj  Jankurai
gs  Security
gi  Artifacts
gR  Release
ge  Evidence
gS  Settings / Source Doctor
gl  LLMs
```

### 8.3 Macro/micro modes

Hyperdeck has two focus modes everywhere:

```text
Macro mode:
  arrows move between panes/major objects
  Enter drills into selected pane/object
  Tab moves to next pane or top-level tab

Micro mode:
  focused pane is locked
  arrows scroll/select within pane
  Enter drills into selected row/node
  Esc returns to macro mode or pops route
```

Pane title must show mode:

```text
╭─ Live Queue [macro] ───────────────────╮
╭─ Live Queue [drill: Esc up] ────────────╮
```

### 8.4 Breadcrumbs

Every drill pushes a breadcrumb:

```text
Global > veox-* > veox-api > MR !221 > Pipeline #581 > test-linux #99122 > Trace
```

Numbered crumbs can be jumped to with `Alt-1` through `Alt-9` or selected from the route overlay.

### 8.5 Related-object pivot

Press `o` or `Enter` on a relation to pivot quickly:

```text
job → pipeline → repo → bug → agent → branch → MR → artifact → release gate → evidence
```

This is the core of fast drilldown. The TUI should feel spatial: every object has neighbors.

---

## 9. Global Mission Control

### 9.1 Purpose

The Global screen is the default “everything now” surface. It must answer:

- Are we safe to code, merge, release, and roll back?
- What needs attention first?
- What is running across all repos/families?
- How close are we to the theoretical CI limit?
- Which repo family, runner tag, cache category, or gate is bottlenecking us?
- What changed in the last few seconds?

### 9.2 Wide mock

```text
┌ JERYU HYPERDECK ─ prod ─ db:sqlite ─ gitlab:ok  docker:ok  vault:ok  broker:ok  event:184923↑ fresh:0.8s ─────────────┐
│ Safe: code ✓  merge ▲  release ✗  rollback ✓ │ Queue 84/112 │ Limit 91% ██████████████████░░ │ Cache 84% │ VTI saved 4.2h │
├ Tabs: [Global] Queue Repos Workflow Runners Cache VTI Agents Autonomy Bugs Git Bottlenecks Jankurai Security Artifacts Release Evidence ┤
│ ATTENTION QUEUE                         │ GLOBAL LIVE QUEUE / CAPACITY                                           │ INSPECTOR        │
│ 1 ✗ veox-core #94812 test-linux          │ Family       Q Run Fail Block ETA   Limit  Bottleneck      Trend         │ selected: veox-* │
│   first error: borrow checker            │ veox-*       48 17  3    5   31m   96%    linux runners   ▂▃▅▇█        │ repos: 14        │
│ 2 ▲ cache disk 88% target/               │ veox-deploy  12  5  0    2   11m   72%    release gate    ▂▃▃▄▄        │ q:48 run:17      │
│ 3 ⛔ rel v2.8.1 canary telemetry         │ veox-enclave  8  2  1    1   19m   81%    security scan   ▆▅▃▃▂        │ fail:3           │
│ 4 ◆ agent-7 blocked by grant             │ isolated     12  6  0    1   09m   63%    none            ▂▂▃▃▄        │ bottleneck:      │
│ 5 ‼ secret scan veox-api P0              │ infra         4  2  0    0   04m   41%    idle            ▁▁▂▁▁        │ linux runners    │
│                                          │ Critical path now: veox-core MR!221 → test-linux → release gate, 18m      │ actions:         │
│ QUICK FILTERS                            │ ┌ Repo / Pipeline                     Stage      Progress    Runner  ETA │ Enter drill      │
│ [A] all [V] veox [F] failing [S] sec     │ │ veox-core !221                       test       74% ████░   r17     6m │ a actions        │
│ [G] agents [R] release [C] cache [!] me  │ │ veox-api  main                       build      21% ██░░░   r03    11m │ e evidence       │
│                                          │ │ veox-web  !118                       vti skip   92% ████░   r22     2m │ l logs           │
│ RECENT EVENTS                            │ │ redlinedb feat/wal                   queue      00% ░░░░░   tag:gpu  ? │ x explain        │
│ 09:41 job.failed veox-core#94812         │ │ jeryu     main                       release    55% ███░░   r09    14m │                  │
│ 09:40 cache.miss storm crates.io         │ └───────────────────────────────────────────────────────────────────────┘ │                  │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ [Enter] drill [Esc] back [a] actions [l] logs [e] evidence [/] filter [:] command [?] help  ws:ok frame:6ms drops:0 coalesce:off       │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 9.3 Required panels

**Header posture bar**

- environment/profile;
- DB backend and feature profile;
- GitLab/Docker/Vault/cache/broker health;
- stream cursor and freshness;
- safe-to-code, safe-to-merge, safe-to-release, safe-to-rollback;
- active freeze/kill-bell state;
- current user/actor authority envelope.

**Attention queue**

- ranked by severity × urgency × blast radius × confidence × human-required action;
- each item must have reason, entity, proof path, and suggested next action;
- `Enter` drills to item; `a` actions; `x` explain.

**Fleet live queue**

- repo families with queued/running/failed/blocked counts;
- current ETA/drain estimates;
- theoretical-limit pressure;
- top bottleneck;
- trend sparkline;
- hot jobs/pipelines.

**Inspector**

- selected object summary;
- freshness/provenance;
- related entities;
- available actions;
- evidence snippets.

**Event tape**

- latest normalized events;
- event kind, severity, entity, correlation id;
- coalesced under high volume;
- `f` follow/unfollow.

---

## 10. Backend inspection plane

### 10.1 Golden architecture rule

Only adapters touch raw systems. Renderers consume typed view models.

Bad:

```rust
fn draw_cache_tab(app: &App) {
    let rows = sqlx::query("SELECT ..."); // no: render path must not query DB
}
```

Good:

```rust
fn draw_cache_tab(f: &mut Frame, area: Rect, view: &CacheDashboardView, focus: FocusState) {
    // pure rendering over immutable view data
}
```

### 10.2 Target architecture

```text
             ┌──────────────────────────┐
             │ GitLab REST + Webhooks   │
             └────────────┬─────────────┘
                          │
┌────────────┐   ┌─────────▼─────────┐   ┌──────────────┐
│ Docker     │   │ Event normalizer  │   │ Vault        │
│ Runners    ├──▶│ + source adapters │◀──┤ Secrets      │
└────────────┘   └─────────┬─────────┘   └──────────────┘
                          │
┌────────────┐   ┌─────────▼─────────┐   ┌──────────────┐
│ Cache      ├──▶│ Durable state DB  │◀──┤ Git/Repos    │
│ Gateway    │   │ + event store     │   │ Jankurai     │
└────────────┘   └─────────┬─────────┘   └──────────────┘
                          │
                 ┌────────▼──────────┐
                 │ TUI Read Model     │
                 │ snapshot + deltas  │
                 └────────┬──────────┘
                          │
       ┌──────────────────┼──────────────────┐
       │                  │                  │
┌──────▼──────┐   ┌───────▼───────┐   ┌──────▼──────┐
│ Ratatui TUI │   │ MCP resources │   │ HTTP/SSE/WS │
│             │   │ + tools       │   │ API         │
└─────────────┘   └───────────────┘   └─────────────┘
```

### 10.3 Required HTTP read APIs

Add these endpoints to the main daemon or an internal loopback API. They should be versioned and schema-described.

```http
GET  /api/read-model
GET  /api/events?cursor=N&limit=500&kinds=&entity_kind=&entity_id=
GET  /api/entity/{kind}/{id}
GET  /api/proof?entity=&kind=&since=&actor=&cursor=&limit=
GET  /api/runtime/profile
GET  /api/action-registry
POST /api/action/preview
POST /api/action/execute

GET  /api/repos
GET  /api/repos/{repo_slug}/overview
GET  /api/families
GET  /api/families/{family}/overview
GET  /api/queue
GET  /api/runners/capacity
GET  /api/cache/dashboard
GET  /api/vti/dashboard
GET  /api/agents/dashboard
GET  /api/autonomy/dashboard
GET  /api/bugs/dashboard
GET  /api/git-sync/dashboard
GET  /api/bottlenecks/dashboard
GET  /api/jankurai/dashboard
GET  /api/security/dashboard
GET  /api/artifacts/dashboard
GET  /api/release/dashboard
GET  /api/llms/dashboard
```

### 10.4 Required streaming APIs

```http
GET /api/events/stream?cursor=N                 # SSE
GET /api/ws/events                              # WebSocket normalized events
GET /api/ws/logs?project_id=&job_id=&cursor=    # WebSocket log chunks
GET /api/ws/entity/{kind}/{id}                  # entity-scoped updates
GET /api/ws/action/{action_run_id}              # action execution stream
```

Fallbacks:

- If WebSocket unavailable, use SSE.
- If SSE unavailable, use cursor polling.
- If API unavailable, use existing local DB/GitLab snapshot paths with explicit degraded source markers.

### 10.5 Required MCP resources

MCP tools should remain action-oriented. MCP resources should be read-oriented and safe.

```text
jeryu://system/snapshot
jeryu://runtime/profile
jeryu://events?cursor=N
jeryu://proof?entity=repo:veox-core
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

Add subscription support where possible:

```text
resources/subscribe jeryu://events
resources/subscribe jeryu://jobs/{project_id}/{job_id}/trace
```

### 10.6 Runtime profile

Every TUI session needs a runtime profile:

```rust
struct RuntimeProfile {
    version: String,
    build_sha: String,
    build_time: Option<DateTime<Utc>>,
    feature_flags: BTreeMap<String, bool>,
    db_backend: DbBackend,
    db_path_redacted: Option<String>,
    api_bind: Option<String>,
    mcp_bind: Option<String>,
    webhook_bind: Option<String>,
    cache_proxy_port: Option<u16>,
    registry_mirror_port: Option<u16>,
    gitlab_url_redacted: Option<String>,
    vault_addr_redacted: Option<String>,
    action_registry_sha: String,
    mcp_manifest_sha: String,
    schema_version: u32,
}
```

The Settings/Source Doctor screen should compare runtime profile, docs metadata, action registry, MCP manifest, and DB schema version.

---

## 11. Core data model

### 11.1 Entity identity

```rust
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
struct EntityRef {
    kind: EntityKind,
    id: String,
    label: String,
    repo: Option<String>,
    family: Option<String>,
    project_id: Option<i64>,
    url: Option<String>,
}

enum EntityKind {
    Fleet,
    RepoFamily,
    Repo,
    Branch,
    MergeRequest,
    Pipeline,
    Stage,
    Job,
    RunnerPool,
    Runner,
    Node,
    CacheObject,
    CacheTaint,
    TestPlan,
    TestCase,
    SelectorMiss,
    Agent,
    AgentSession,
    AgentTask,
    Bug,
    BugAttempt,
    GitRefUpdate,
    AdmissionDecision,
    CapabilityGrant,
    JankuraiRun,
    JankuraiFinding,
    SecurityFinding,
    SecretSet,
    Artifact,
    ReleaseAttempt,
    ReleaseGate,
    Evidence,
    LlmCall,
    Source,
    System,
}
```

### 11.2 Freshness

```rust
struct SourceFreshness {
    source: SourceKind,
    state: FreshnessState,
    last_success_at: Option<DateTime<Utc>>,
    last_attempt_at: Option<DateTime<Utc>>,
    age_ms: Option<u64>,
    ttl_ms: u64,
    cursor: Option<u64>,
    error: Option<String>,
    degraded_reason: Option<String>,
}

enum FreshnessState {
    Fresh,
    Stale,
    Loading,
    Degraded,
    Down,
    Disabled,
    Partial,
}
```

### 11.3 Unified read model

```rust
struct TuiReadModel {
    schema_version: u32,
    generated_at: DateTime<Utc>,
    event_cursor: u64,
    runtime: RuntimeProfile,
    freshness: Vec<SourceFreshness>,
    mission: MissionSummary,
    fleet: FleetSummary,
    repo_families: Vec<RepoFamilySummary>,
    repos: Vec<RepoSummary>,
    queue: QueueSummary,
    attention: Vec<AttentionItem>,
    next_action: Option<ActionRecommendation>,
    runners: RunnerFleetSummary,
    cache: CacheDashboardView,
    vti: VtiDashboardView,
    agents: AgentDashboardView,
    autonomy: AutonomyDashboardView,
    bugs: BugDashboardView,
    git_sync: GitSyncDashboardView,
    bottlenecks: BottleneckDashboardView,
    jankurai: JankuraiDashboardView,
    security: SecurityDashboardView,
    artifacts: ArtifactDashboardView,
    release: ReleaseDashboardView,
    evidence: EvidenceSummary,
    llms: LlmDashboardView,
    system: SystemSummary,
}
```

### 11.4 Event model

```rust
struct TuiEvent {
    seq: u64,
    timestamp: DateTime<Utc>,
    kind: EventKind,
    severity: Severity,
    entity: Option<EntityRef>,
    parent: Option<EntityRef>,
    summary: String,
    details: serde_json::Value,
    correlation_id: Option<String>,
    evidence_refs: Vec<EvidenceRef>,
    suggested_actions: Vec<ActionRecommendation>,
    stale_after_ms: Option<u64>,
}
```

Event kinds must cover at least:

- `system.health.updated`;
- pipeline/job lifecycle;
- job log chunks and annotations;
- failure-capsule creation;
- test plan, VTI acceleration, selector miss;
- agent session/task/step/patch/race;
- capability grant/intent/admission decision;
- cache hit/miss/taint/lease/verdict/promotion/GC;
- release gate/promotion/rollback;
- secret audit/denied access;
- bug lifecycle/attempt;
- Jankurai finding/score change;
- security finding/artifact signature/provenance;
- action preview/execute/result;
- source freshness/snapshot refresh.

### 11.5 Action model

```rust
struct ActionPreview {
    action_id: String,
    title: String,
    entity: Option<EntityRef>,
    risk: RiskTier,
    side_effect: SideEffectClass,
    dry_run_available: bool,
    undo_available: bool,
    idempotency_key: Option<String>,
    required_grants: Vec<GrantRequirement>,
    required_confirmations: Vec<ConfirmationRequirement>,
    expected_changes: Vec<String>,
    expected_evidence: Vec<EvidenceRef>,
    blockers: Vec<Blocker>,
    warnings: Vec<String>,
}

struct ActionResult {
    action_run_id: String,
    status: ActionStatus,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    entity_changes: Vec<EntityRef>,
    evidence_created: Vec<EvidenceRef>,
    logs: Vec<ActionLogLine>,
    error: Option<String>,
}
```

Risk tiers:

```text
read_only
local_write
ci_write
repo_write
merge
release
secret
production
destructive
```

High-risk actions require typed confirmation and proof display.

---

## 12. Queue and theoretical-limit screen

### 12.1 Purpose

The user explicitly asked: “I want a single incredible view that shows the live queue across all jobs across all repos, how close am I running to the theoretical limit?”

This screen is the answer. It must distinguish:

- actual utilization;
- effective capacity;
- theoretical capacity;
- tag-constrained capacity;
- blocked queue;
- runnable queue;
- critical path floor;
- queue drain projection;
- bottlenecks that adding runners will not fix.

### 12.2 Capacity model

Definitions:

```text
slot_capacity(tag) = sum(healthy_runner.concurrency where runner supports tag)
weighted_capacity(tag) = slot_capacity(tag) * observed_runner_speed_factor(tag)
ready_work_seconds(tag) = sum(predicted_duration(job) for runnable queued jobs requiring tag)
running_work_seconds_per_second(tag) = sum(speed_factor(active_runner) for active jobs requiring tag)
queue_efficiency = running_work_seconds_per_second / weighted_capacity_seconds_per_second
limit_pressure = min(100%, runnable_demand_seconds / next_window_capacity_seconds)
critical_path_floor = longest remaining dependency path duration, ignoring queue wait
current_projection = queue_wait + critical_path_floor + gate_wait
limit_distance = current_projection / critical_path_floor
```

A pipeline at 95% runner utilization can still be far from theoretical limit if:

- wrong tags are saturated;
- jobs are blocked by DAG/manual gates;
- cache misses inflate durations;
- runners are heterogeneous and slow runners dominate;
- GitLab API/broker latency delays scheduling;
- VTI selector misses force extra tests;
- release/security gates are serial.

### 12.3 Queue mock

```text
╭─ Queue Physics ─────────────────────────────────────────────────────────────────────────────────────────────╮
│ total work 112 jobs │ runnable 84 │ blocked 18 │ running 28 │ healthy slots 32 │ theoretical slots 40 │ eff 91% │
├─ Capacity by tag ───────────────────────────────────────────────────────────────────────────────────────────┤
│ tag          run/q/block  slots busy  pressure  wait p50/p95  speed  bottleneck reason                       │
│ rust-fast    12/31/3      12    12    99%       08m/22m       1.42x  saturated; sccache fingerprint misses   │
│ linux        11/44/8      18    16    88%       04m/13m       1.00x  OK, but 2 offline runners                │
│ gpu           1/4/0        1     1    96%       19m/44m       0.90x  single slot bottleneck                    │
│ security      2/2/4        4     2    41%       01m/03m       0.76x  blocked by manual gates                   │
├─ Loss decomposition ────────────────────────────────────────────────────────────────────────────────────────┤
│ theoretical floor 12m │ current projection 38m │ lost: tag saturation 11m, cache misses 7m, gates 5m, stale 3m │
├─ Recommendations ───────────────────────────────────────────────────────────────────────────────────────────┤
│ 1 add 4 rust-fast slots: projection 38m → 24m, confidence high, cost medium                                  │
│ 2 fix sccache toolchain key drift in veox-core: saves ~7m, confidence medium                                 │
│ 3 do not add security runners: queue is gate-blocked, not capacity-blocked                                   │
╰─ Enter drill tag  a actions  s simulate  ? help ────────────────────────────────────────────────────────────╯
```

### 12.4 What-if simulator

The Queue screen should support `s` simulation:

```text
what if +4 rust-fast runners?
what if cache hit rate improves from 61% to 85%?
what if VTI confidence threshold lowers from 0.80 to 0.72?
what if security gate becomes parallel?
```

Simulation results must show confidence and assumptions.

---

## 13. Repo families and repo drilldown

### 13.1 Family model

Repo families are first-class objects. Examples:

```text
veox-*
veox-deploy
veox-enclave
infra
isolated
archived
external
```

A repo can belong to multiple families. Family matching should support:

- glob by repo slug;
- explicit config membership;
- tag-based membership;
- path/owner/provider grouping;
- release-train grouping;
- “isolated” fallback for repos with no family.

### 13.2 Family screen mock

```text
╭─ Family: veox-* ─────────────────────────────────────────────────────────────────────────────────────────╮
│ repos 14 │ run 17 │ queue 48 │ fail 3 │ blocked 5 │ cache 84% │ vti 73% │ jankurai 82↓ │ release blocked │
├─ Repos ───────────────────────────────┬─ Critical Path ───────────────────┬─ Family Signals ─────────────┤
│ repo          branch/MR    CI     rel │ veox-core !221 test-linux 18m     │ bottleneck rust-fast          │
│ ▶ veox-core   !221         ✗ 3    blk │ veox-api main build-linux 11m     │ cache target/ 88%             │
│   veox-api    main         ▶ 21%  --  │ veox-deploy canary telemetry 9m   │ VTI selector misses 4         │
│   veox-web    !118         ▶ 92%  --  │                                      │ agents blocked 2              │
│   veox-auth   main         ✓      ok  │                                      │ security P0 1                 │
├─ Timeline ───────────────────────────────────────────────────────────────────────────────────────────────┤
│ 09:41 fail veox-core test-linux  09:40 cache miss storm  09:39 agent race started JRY-412               │
╰─ Enter repo  Tab panes  / filter  a family actions  e evidence ─────────────────────────────────────────╯
```

### 13.3 Repo dashboard

A repo dashboard must show:

- identity: slug, provider, project ID, default branch, head SHA, remote main SHA;
- Git sync: dirty state, ahead/behind, branch, mirror, hooks;
- active pipelines/MRs/releases;
- queue/runners/tags;
- cache namespace and hit rate;
- VTI health and last plan;
- agents active on repo;
- bugs/issues linked to repo;
- Jankurai/security/artifact/release posture;
- source freshness.

Mock:

```text
╭─ Repo: veox-api ─ main ab91c2e ─ GitLab #184 ─ family veox-* ─ fresh 1.1s ─────────────────────────────╮
│ CI: ▶ build 21% │ queue 7 │ fail 0 │ VTI saved 19m safe? ▲ │ cache 78% │ Jankurai 84↑ │ sec 0C/2H │ rel -- │
├─ Subtabs: [Overview] Workflow MRs Jobs Logs Cache VTI Agents Bugs Git Jankurai Security Artifacts Release Evidence ─────┤
│ Active Work                              │ Health / Risk                          │ Recommended Next Action                    │
│ MR !221 test-linux running               │ cache miss storm: crates.io             │ Wait 6m for test-linux; do not merge yet    │
│ main build-linux 21%                     │ VTI confidence low on auth tests         │ Run auth smoke plan? [a] preview            │
│ agent race JRY-412 h1/h2/h3              │ 2 high security warnings, no critical    │                                            │
├─ Recent entities ───────────────────────────────────────────────────────────────────────────────────────┤
│ job #99122 test-linux failed  capsule ready   bug JRY-412 ready   artifact api:ab91 signed? pending     │
╰─ Enter selected  ←→ subtabs  Esc family  a actions  e evidence  l logs ────────────────────────────────╯
```

---

## 14. Workflow / pipeline DAG

### 14.1 Purpose

The workflow view is the core drilldown for a repo/branch/MR/pipeline. It should show what is running, what is finished, what was skipped, what failed, what is blocked, and what will happen next.

### 14.2 DAG visual encoding

Node states:

```text
✓ success
▶ running
… queued
⛔ blocked
✗ failed
◌ skipped/stale/manual
◆ agent task
⬢ release gate
§ evidence/proof
```

Edges:

- solid = explicit dependency;
- dotted = inferred stage order;
- double = critical path;
- dim = skipped/cached/VTI accelerated;
- red = failed dependency;
- amber = blocked/gate.

### 14.3 Wide DAG mock

```text
╭─ Workflow: veox-core MR !221 pipeline #581 sha ab91c2e ─────────────────────────────────────────────────╮
│ progress 64% │ ETA 18m±6 │ critical path test-linux → integ-db → package │ source gitlab fresh 0.7s       │
├─ DAG ───────────────────────────────────────────────────┬─ Inspector: job test-linux #99122 ───────────┤
│ prepare ✓                                               │ status ▶ running 74%                         │
│    │                                                     │ runner r17 rust-fast                         │
│ build-linux ✓ ── build-mac ✓ ── build-image ✓            │ started 04:12 ago, eta 01:29                 │
│    │              │                  │                  │ queue wait 08:31, p95 normal 02:10 ▲        │
│ test-linux ▶████████░ 74% ───────┐   │                  │ cache sccache miss: toolchain fp drift      │
│ test-mac ✓                       │   │                  │ first warning: flaky auth_timeout           │
│ vti-skip ◌ 128 skipped safe? ▲    │   │                  │ evidence: plan#778, logs live, capsule none │
│ integ-db … queued tag:db         ◀───┘                  │ actions: cancel retry trace explain         │
│ audit-jankurai ◌ waiting                                │                                             │
│ package ◌ blocked by integ-db                           │ Logs preview                                │
│ release-gate ◌ blocked                                  │ 09:41 running test auth::expires            │
├─────────────────────────────────────────────────────────┴─────────────────────────────────────────────┤
│ [←→↑↓] graph nav  [Enter] drill  [l] logs  [e] evidence  [c] critical path  [a] actions  [Esc] repo     │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 14.4 Graph layout algorithm

Start simple, then improve:

1. **Stage fallback:** group nodes by GitLab stage, stable sort by name.
2. **Needs edges:** use explicit `needs`/dependencies when available.
3. **Downstream expansion:** recursively include bridges/downstream pipelines.
4. **Critical path:** compute longest remaining path using historical/observed durations.
5. **Lane assignment:** minimize edge crossings; keep same job names in stable vertical positions across refreshes.
6. **State-preserving layout:** do not jump nodes around every refresh; animate state changes, not coordinates.
7. **Minimap:** for large DAGs, show compressed stage count and viewport.

### 14.5 Job detail

Job detail tabs:

```text
Overview | Logs | Failure Capsule | Artifacts | Tests | Cache | Runner | Evidence | Actions | Raw
```

Required fields:

- job ID/name/stage/ref/SHA/status;
- queued duration, start/finish, runtime, ETA/confidence;
- runner ID/name/pool/tags/system ID/node;
- allow-failure/manual status;
- web URL;
- trace cursor and source;
- failure capsule if present;
- artifact links and parsed reports;
- VTI/test-plan relation;
- cache hits/misses/taints;
- evidence refs and correlation id;
- available actions.

---

## 15. Live traces and logs

### 15.1 Requirements

The log viewer must be extremely fast and usable under heavy output.

Features:

- follow/unfollow live tail;
- bounded memory with backfill pagination;
- search within logs;
- jump to first error/warning/failure annotation;
- fold noisy sections;
- syntax highlighting for timestamps, levels, paths, test names, panic traces, cargo errors;
- evidence/capsule annotations inline;
- source freshness marker;
- copy selected range;
- export redacted log bundle;
- low-bandwidth mode.

### 15.2 Log mock

```text
╭─ Trace: veox-core test-linux #99122 ─ live ws cursor 881284 ─ follow:on ─ errors:1 warnings:12 ─────────╮
│ 09:41:12.103 INFO nextest run auth::token_expiry                                                       │
│ 09:41:12.891 WARN retrying db fixture setup after timeout                                               │
│ 09:41:14.221 ERR  src/auth/token.rs:184: assertion failed                                               │
│             left: Expired(30s)                                                                          │
│            right: Valid                                                                                 │
│ annotation: failure.kind=assertion evidence=capsule pending                                             │
├─ Minimap ───────────────────────────────────────────────────────────────────────────────────────────────┤
│ ░░░░░░░░▒▒▒▒▓▓▓▓▓▓▓▓▓▓▓▓▓▓█err▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ live                                            │
╰─ / search  n/N next  f follow  E capsule  y copy  Esc job ─────────────────────────────────────────────╯
```

### 15.3 Trace transport

Preferred order:

1. WebSocket log chunks with cursor resume.
2. SSE log stream.
3. GitLab trace polling with backoff and range support.
4. Last-known trace snapshot with stale badge.

---

## 16. Cache Observatory

### 16.1 Purpose

The Cache screen must answer:

- Are we full?
- What is taking space?
- Are Rust crates/sccache/build artifacts/OCI layers causing pressure?
- Are misses avoidable?
- Are taints/verdicts preventing reuse?
- What can be safely GC’d?
- Is cache trust improving or degrading?

### 16.2 Cache categories

Minimum categories:

- Cargo registry sparse index;
- Cargo crate downloads;
- Cargo git checkouts;
- sccache objects;
- build artifacts;
- OCI layers/images;
- npm/yarn/pnpm where present;
- generic CAS objects;
- material objects/aliases;
- action-cache entries;
- hot cache entries;
- tainted/quarantined objects.

### 16.3 Cache mock

```text
╭─ SmartCache Observatory ────────────────────────────────────────────────────────────────────────────────╮
│ used 842GiB / 1.0TiB 84% ▲ │ hit 78% ↓ │ singleflight 311 saved │ taints 12 │ GC reclaimable 176GiB      │
├─ By category ───────────────────────┬─ Hot / Problem Objects ───────────────┬─ Trust / Taints ───────────┤
│ Rust crates      312GiB ████████░   │ ▶ crate serde-1.0.203  hits 18k       │ taint toolchain mismatch 7  │
│ sccache          221GiB ██████░░░   │   sccache fp:rustc-1.78 old 44GiB     │ expired lease 3             │
│ target dirs      144GiB ████░░░░░   │   oci layer sha256:ab... 22GiB        │ untrusted material 2        │
│ OCI layers        91GiB ███░░░░░░   │   target/veox-core/debug 19GiB        │                              │
│ artifacts         54GiB ██░░░░░░░   │                                      │                              │
├─ Miss storm ────────────────────────────────────────────────────────────────────────────────────────────┤
│ rust-fast jobs missing sccache due to toolchain fingerprint drift: rustc hash changed on 8 runners       │
├─ Actions ───────────────────────────────────────────────────────────────────────────────────────────────┤
│ [g] preview GC 176GiB  [t] inspect taints  [f] force refresh  [e] evidence  [x] explain misses           │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 16.4 Cache data model

```rust
struct CacheDashboardView {
    total_bytes: u64,
    limit_bytes: Option<u64>,
    hit_ratio: f32,
    miss_ratio: f32,
    singleflight_coalesced: u64,
    categories: Vec<CacheCategorySummary>,
    hot_objects: Vec<CacheObjectSummary>,
    taints: Vec<CacheTaintSummary>,
    verdicts: Vec<CacheVerdictSummary>,
    gc_plan: Option<CacheGcPlan>,
    miss_storms: Vec<CacheMissStorm>,
    freshness: Vec<SourceFreshness>,
}
```

### 16.5 Cache actions

All mutating cache actions require preview:

- preview GC plan;
- execute safe GC;
- quarantine/untaint object;
- force-refresh namespace;
- inspect object provenance;
- compare toolchain fingerprints;
- open cache request history;
- generate cache incident bundle.

---

## 17. VTI smart test skipper cockpit

### 17.1 Purpose

VTI is powerful only if the user can trust it. The screen must answer:

- How much time did VTI save?
- Which tests were selected/skipped and why?
- What confidence did the selector have?
- Did selector misses occur?
- What was detected by later runs?
- Are mappings stale?
- Should thresholds change?

### 17.2 VTI mock

```text
╭─ VTI Smart Test Skipper ────────────────────────────────────────────────────────────────────────────────╮
│ saved today 4.2h │ skipped 12,841 │ selected 2,104 │ selector misses 4 ▲ │ confidence p50 .87 p10 .61 │
├─ Repo health ────────────────────────┬─ Recent plans ───────────────────────┬─ Misses / Repairs ───────┤
│ veox-core   saved 91m  miss 2 ▲      │ ▶ plan #778 MR!221 conf .74          │ auth_timeout missed high   │
│ veox-api    saved 42m  miss 0 ✓      │   selected 184 skipped 128           │ detected by nightly        │
│ veox-web    saved 77m  miss 1 ▲      │   reasons: src/auth/**, Cargo.lock   │ repair pending agent-3     │
│ infra       saved 13m  miss 0 ✓      │   risky skips: auth/e2e/payment      │                            │
├─ Plan explanation ──────────────────────────────────────────────────────────────────────────────────────┤
│ Selected auth::expires because src/auth/token.rs changed and prior miss JRY-412. Skipped 128 low-risk tests. │
│ Warning: confidence below release threshold; recommend targeted smoke expansion.                         │
╰─ Enter plan  v validate  a actions  x explain  e evidence ─────────────────────────────────────────────╯
```

### 17.3 Guardrail

Any VTI-driven skip that is used to justify merge/release must have:

- test-plan ID;
- base/head SHA;
- selected/skipped counts;
- selected test IDs and reasons;
- confidence;
- selector miss history;
- threshold policy;
- fallback/escalation path;
- evidence ref.

Low-confidence VTI must be visually distinct and block risky release actions unless explicitly overridden by policy.

---

## 18. Agents and autonomous workflows

### 18.1 Agent dashboard purpose

The Agents screen shows not just “agents exist” but whether they are helping:

- current task and objective;
- repo/bug/MR/pipeline relation;
- grants and authority;
- branch/MR/patch status;
- CI state;
- logs/messages;
- LLM budget and model/provider health;
- evidence produced;
- errors, blocked reasons, and next actions.

### 18.2 Agent lifecycle tables to add

The prior notes repeatedly call this out as missing. Add or expose:

```text
agent_sessions
agent_tasks
agent_steps
agent_messages
agent_artifacts
agent_patch_races
agent_hypotheses
agent_grants
agent_config_revisions
```

### 18.3 Agents mock

```text
╭─ Agents Control Center ────────────────────────────────────────────────────────────────────────────────╮
│ active 9 │ blocked 2 │ racing 3 │ grants 14 │ LLM spend today $18.42 │ kill bell disarmed │ freeze none │
├─ Sessions ─────────────────────────────┬─ Selected: agent-7 fix-auth-race ─────────────────────────────┤
│ agent      repo       task       state  │ bug JRY-412 │ branch agent/jry-412-h1 │ MR !221 │ CI 43%      │
│ ▶ agent-7  veox-core  JRY-412    wait   │ grant g-812 paths src/auth/** tests/auth/** expires 18m       │
│   agent-3  veox-web   JRY-388    run    │ blocked: needs broader test path for tests/e2e/auth/**        │
│   agent-9  veox-api   sec P1     run    │ last log: proposed patch, waiting for rust-fast runner        │
│   agent-2  infra      cache      done   │ evidence: attempt#44, diff digest, plan_validation#778        │
├─ Patch Race JRY-412 ──────────────────────────────────────────────────────────────────────────────────┤
│ h1 adjust expiry calc      CI ▶43% diff +82 -31  risk med  Jankurai +0.4  likely                         │
│ h2 update fixture only     CI ✗fail diff +12 -4   risk low  Jankurai 0.0   rejected                        │
│ h3 refactor token store    CI …q    diff +221-90  risk high Jankurai ?     broad                           │
╰─ Enter session  k kill task  g grant  c config  m merge winner if gated  e evidence ──────────────────╯
```

### 18.4 Agent config editor

The TUI can edit configs only through safe paths:

- open current config in embedded editor or `$EDITOR`;
- validate schema;
- diff against previous revision;
- preview affected agents/workflows;
- require confirmation for authority expansion;
- write revision with audit event;
- support rollback.

### 18.5 Autonomy dashboard

Autonomy should show:

- kill bell: armed/disarmed/paused, actor, reason, expiry;
- freeze window: active risk tier restrictions;
- verdicts: active/superseded, risk, decision, policy SHA, head SHA, expiry;
- foundry candidate queue;
- launch ledger replay;
- escalation dispatch outcomes;
- LLM budget ledger;
- provider health;
- GitHub/GitLab PR drift;
- autonomous workflows and config.

Mock:

```text
╭─ Autonomy Governance ──────────────────────────────────────────────────────────────────────────────────╮
│ kill bell: disarmed ✓ │ freeze: none │ active verdicts 6 │ superseded 3 │ budget 42% │ provider ok │
├─ Workflows ───────────────────────────┬─ Policy / Gates ─────────────────────┬─ Ledger ────────────────┤
│ nightly-fixer        enabled  risk med │ merge requires head SHA passport      │ 09:41 launch signed     │
│ security-triage      enabled  risk high│ release requires artifact provenance   │ 09:39 verdict supersede │
│ cache-gc-autopilot   paused   storage  │ P0 security blocks all automation      │ 09:36 escalation sent   │
╰─ Space pause  k kill bell  c config  e evidence  x explain ───────────────────────────────────────────╯
```

---

## 19. Bugs and issues cockpit

### 19.1 Bug statuses

Use the source-derived local bug lifecycle:

```text
needs_triage
needs_info
accepted
ready
in_progress
blocked
fix_proposed
reviewing
verifying
done
duplicate
invalid
cannot_reproduce
wont_do
```

Attempt statuses:

```text
pending
started
failed
fix_proposed
verified
abandoned
```

### 19.2 Bug board mock

```text
╭─ Bugs / Issues Across Repos ───────────────────────────────────────────────────────────────────────────╮
│ open 184 │ ready 31 │ in_progress 19 │ fix_proposed 12 │ blocked 8 │ P0 1 │ agent attempts today 44 │
├─ needs_triage ───────┬─ ready ───────────────┬─ in_progress ───────────┬─ review/verify ──────────────┤
│ JRY-501 cache panic   │ JRY-412 auth expiry   │ JRY-388 flaky e2e       │ JRY-399 signed artifact       │
│ JRY-498 docker oom    │ JRY-407 VTI miss      │ JRY-344 docs drift      │ JRY-377 release gate           │
│ JRY-490 secret scan   │ JRY-401 Jankurai dup  │ JRY-320 cache GC        │ JRY-300 runner drain           │
├─ Selected JRY-412 ────────────────────────────────────────────────────────────────────────────────────┤
│ target veox-core │ severity high │ priority P1 │ owner agent-7 │ attempts 3 │ branch agent/jry-412-h1 │
│ current: token expires early under clock drift. expected: stable auth grace window.                    │
│ evidence: failing job #99122, log range, test auth::expires, MR !221, patch race h1/h2/h3             │
╰─ Enter bug  a actions  A assign agent  e evidence  l logs  g Git/MR ─────────────────────────────────╯
```

### 19.3 Bug detail requirements

- canonical report fields;
- source/target projects;
- severity/priority/difficulty;
- component;
- current/expected behavior;
- environment/frequency/impact;
- security/privacy notes and no-secrets confirmation;
- reproduction steps;
- acceptance criteria;
- evidence list;
- attempt history;
- agent sessions;
- branch/MR/CI linkage;
- external refs/labels;
- action history.

---

## 20. Git sync and remote state

### 20.1 Purpose

The Git Sync screen answers:

- Are local and remote heads aligned?
- Which branches are dirty, ahead, behind, mirrored, or blocked?
- Are PRs/MRs open, draft, mergeable, approved, or stale?
- Are hooks installed and active?
- Are pre-receive admission decisions allowing/denying expected refs?
- Are sidecar/mirror jobs healthy?

### 20.2 Mock

```text
╭─ Git Sync / Remote State ──────────────────────────────────────────────────────────────────────────────╮
│ repos 58 │ dirty 4 │ ahead 7 │ behind 2 │ open MR/PR 31 │ admission deny today 3 │ hooks missing 1 │
├─ Repo table ───────────────────────────────────────────────────────────────────────────────────────────┤
│ repo          branch       local       remote      dirty  ahead/behind  MR/PR  hooks  admission  action │
│ veox-core     agent/h1     8fa2c91     8fa2c91     no     0/0           !221   ok     allow      open   │
│ veox-api      main         ab91c2e     aa18110     no     1/0           --     ok     allow      sync   │
│ veox-web      feat/nav     991afe2     771cc10     yes    3/2           !118   ok     warn       stash  │
│ redlinedb     main         11da001     11da001     no     0/0           --     miss   partial    install│
╰─ Enter repo  s sync preview  h hooks  e evidence  x explain drift ────────────────────────────────────╯
```

### 20.3 Data fields

- repo slug/path/provider/project ID;
- local head/branch/dirty state;
- remote main/head SHA;
- ahead/behind;
- PR/MR title/state/draft/mergeability/labels/reviewers;
- checks/workflow runs;
- admission decisions;
- capability grants;
- git command events;
- mirror/sidecar status;
- hook install status;
- backup status.

---

## 21. Bottleneck lab

### 21.1 Bottleneck taxonomy

Classify CI slowness into:

- runner slot saturation;
- tag-constrained saturation;
- runner speed heterogeneity;
- queue scheduler delay;
- cache miss storm;
- Docker/host resource pressure;
- GitLab API latency/rate limit;
- broker lag;
- DAG critical path serialization;
- manual/release/security gates;
- flaky retries;
- VTI misses or low-confidence expansion;
- artifact upload/download slowness;
- agent contention/race duplication.

### 21.2 Scoring

```text
score = impact_minutes * confidence * recurrence_factor * unblockability_factor * blast_radius
```

Render “fix this first” ranked recommendations, not just raw timings.

### 21.3 Mock

```text
╭─ CI Bottleneck Lab ───────────────────────────────────────────────────────────────────────────────────╮
│ lost today 18.4h │ top class cache misses │ p95 queue 12m │ p95 runtime 31m │ flaky retry loss 2.1h │
├─ Ranked bottlenecks ──────────────────────────────────────────────────────────────────────────────────┤
│ 1 rust-fast tag saturation        impact 6.8h  conf high  fix +4 runners or retag 12 jobs             │
│ 2 sccache fingerprint drift       impact 4.1h  conf med   fix toolchain pin on 8 managers             │
│ 3 integ-db serialized DAG         impact 2.9h  conf high  split db fixture seed                       │
│ 4 VTI low-confidence expansion    impact 1.7h  conf med   repair mappings auth/payment                │
│ 5 GitLab trace polling overhead   impact 0.8h  conf low   add WS log transport                        │
╰─ Enter detail  s simulate  a action  e evidence ──────────────────────────────────────────────────────╯
```

---

## 22. Jankurai audit center

### 22.1 Purpose

The Jankurai screen makes code-audit health visible:

- current version/tooling;
- repo/family scores;
- score trends;
- caps and enforcement;
- duplicate/rot findings;
- security/provenance/UX/release anti-patterns;
- what findings block merge/release;
- which findings agents are fixing;
- before/after evidence.

### 22.2 Mock

```text
╭─ Jankurai Audit Center ────────────────────────────────────────────────────────────────────────────────╮
│ version 0.9.4 │ fleet score 82.1 ↓1.4 │ caps enforced 11/14 │ critical findings 2 │ duplicate groups 7 │
├─ Family scores ───────────────────────┬─ Findings ──────────────────────────┬─ Selected finding ───────┤
│ veox-*       82 ↓   cap release:85 ✗   │ ‼ veox-api unsafe secret logging     │ kind security/provenance  │
│ veox-deploy  91 ↑   cap release:85 ✓   │ ▲ veox-core duplicate auth code x4   │ file src/auth/log.rs      │
│ enclave      77 ↓   cap release:90 ✗   │ ▲ redlinedb release rollback gap     │ blocks release: yes       │
│ infra        88 →   cap release:80 ✓   │ ◌ docs drift generated API stale     │ suggested agent fix: yes  │
╰─ Enter finding  r run audit  f fix with agent  e evidence  x explain cap ─────────────────────────────╯
```

### 22.3 Required model

```rust
struct JankuraiDashboardView {
    version: Option<String>,
    fleet_score: Option<f32>,
    score_trend: Trend,
    family_scores: Vec<JankuraiFamilyScore>,
    repo_scores: Vec<JankuraiRepoScore>,
    caps: Vec<JankuraiCap>,
    findings: Vec<JankuraiFindingSummary>,
    duplicate_groups: Vec<DuplicateGroup>,
    agent_fix_attempts: Vec<AgentTaskRef>,
    freshness: Vec<SourceFreshness>,
}
```

---

## 23. Runners, pools, nodes, and system utilization

### 23.1 Purpose

The Runners screen answers:

- How many slots exist, are healthy, busy, paused, draining, or offline?
- Which tags/pools are saturated?
- Which nodes are full, slow, unreachable, OOMing, or GC-heavy?
- Can we safely scale up or down?
- Are runners underused due to blocked jobs?

### 23.2 Mock

```text
╭─ Runner Fleet / System Utilization ───────────────────────────────────────────────────────────────────╮
│ pools 12 │ managers 31 │ slots 40 theoretical / 32 healthy / 28 busy │ nodes 6 │ offline 2 │ disk warn 1 │
├─ Pools ────────────────────────────────────────┬─ Nodes ─────────────────────────────────────────────┤
│ pool       tags        slots busy q pressure    │ node       cpu  mem  disk cache  managers state       │
│ rust-fast  rust,linux  12    12 31 99%          │ n1-local   88%  71%  83%  441G   8        ok          │
│ linux      linux       18    16 44 88%          │ n2-remote  92%  82%  91%  612G   6        disk warn   │
│ gpu        gpu          1     1  4 96%          │ n3-gpu     77%  61%  74%  120G   1        ok          │
│ security   sec          4     2  2 41%          │ n4-remote  --   --   --   --     0        unreachable │
╰─ Enter pool/node  s scale  d drain  g GC  l logs  e evidence ────────────────────────────────────────╯
```

### 23.3 Required data

- pool names, tags, trust tier, backend type;
- min/max warm managers, concurrency, request concurrency;
- paused/draining state;
- manager container/pod ID, GitLab runner ID/system ID;
- remote node alias/SSH target/Docker socket/storage limit;
- CPU/mem/disk/network metrics;
- Docker event OOM/die history;
- runner logs;
- queue pressure per tag/pool;
- scale recommendations and safety gates.

---

## 24. Security, secrets, policy, and grants

### 24.1 Security screen purpose

Security is not one tab of scans; it is release safety. Show:

- critical/high findings;
- dependency/container/SAST/secret results;
- Vault health and secret-set lifecycle;
- active capability grants;
- broad or expiring grants;
- admission denials;
- policy violations;
- unsigned or unproven artifacts;
- freeze windows;
- evidence links.

### 24.2 Mock

```text
╭─ Security / Policy / Secrets ─────────────────────────────────────────────────────────────────────────╮
│ critical 1 │ high 9 │ grants active 14 │ broad grants 1 ▲ │ Vault ok │ secret sets expiring 2 │ denies 3 │
├─ Findings ────────────────────────────┬─ Capability Grants ──────────────────┬─ Secrets / Vault ───────┤
│ ‼ veox-api secret logging P0           │ ▶ g-812 agent fix-auth expires 18m   │ authority prod ok        │
│ ▲ veox-core dependency high            │   g-801 BROAD release-nightwatch ▲   │ veox-api v2.8.1 exp 2d   │
│ ▲ enclave container scan high          │   g-799 patch race h1 expires 9m     │ rotation due 1           │
├─ Admission / Policy ──────────────────────────────────────────────────────────────────────────────────┤
│ 09:33 deny push main by agent-3: missing merge passport, grant expired, branch protected              │
╰─ Enter item  r revoke grant  v Vault detail  e evidence  x explain ──────────────────────────────────╯
```

### 24.3 Redaction rules

Never show plaintext:

- tokens;
- Vault root/unseal material;
- secrets;
- webhook secrets;
- private keys;
- env var values.

Show only:

- redacted path;
- fingerprint/hash prefix;
- presence/absence;
- expiry;
- owner/authority;
- audit event;
- policy metadata.

---

## 25. Artifacts, SBOM, provenance, and signed supply chain

### 25.1 Purpose

The Artifacts screen answers:

- What did we build?
- Which commit/ref/job produced it?
- Is it signed?
- Does it have SBOM/provenance?
- Which release/canary/prod environment uses it?
- Can we trace it to test/security/Jankurai evidence?

### 25.2 Mock

```text
╭─ Artifacts / Provenance ───────────────────────────────────────────────────────────────────────────────╮
│ artifacts 92 │ unsigned 3 ▲ │ SBOM missing 2 │ prod current v2.8.0 │ candidate v2.8.1 blocked │
├─ Artifact table ──────────────────────────────────────────────────────────────────────────────────────┤
│ name              version  digest        repo       job       signed  SBOM  provenance  release state │
│ veox-api          2.8.1    sha256:ab...  veox-api   #99122    yes     yes   yes         candidate     │
│ veox-worker       2.8.1    sha256:bc...  veox-api   #99128    no ▲    yes   partial     blocked       │
│ veox-enclave      1.4.2    sha256:ee...  enclave    #88210    yes     no ▲  yes         staging       │
╰─ Enter artifact  e evidence  s sign/verify preview  r release relation ──────────────────────────────╯
```

### 25.3 Artifact model

Fields:

- name/version/type;
- digest/size;
- repo/project/ref/SHA;
- producing pipeline/job;
- signature status;
- SBOM status/path/digest;
- provenance/SLSA-like attestation status;
- Jankurai/security/test evidence refs;
- release/canary/prod deployment refs;
- rollback compatibility.

---

## 26. Release, production, and rollback control

### 26.1 Purpose

The Release screen must make high-risk operations boring and provable:

- release attempt state;
- upstream/release/prod pipeline status;
- canary state;
- gates and evidence;
- eligibility;
- blocking/non-blocking failures;
- secret sets;
- signed artifacts;
- rollback target and blast radius.

### 26.2 Mock

```text
╭─ Release Control ──────────────────────────────────────────────────────────────────────────────────────╮
│ current prod v2.8.0 ab12cc ✓ │ candidate v2.8.1 ab91c2e blocked │ canary 20% telemetry fail │ rollback ready ✓ │
├─ Gates ─────────────────────────────────────┬─ Candidate Evidence ───────────────────────────────────┤
│ source SHA exact          ✓                 │ pipeline release #7721 success                         │
│ tests critical            ✓                 │ VTI plan #778 low-confidence auth ▲                     │
│ canary telemetry          ✗                 │ artifact veox-worker unsigned ▲                         │
│ e2e smoke                 ✓                 │ Jankurai score 82 below cap 85 ✗                         │
│ security critical         ✓                 │ secret set prod rendered, expires 2d                     │
│ artifact signatures       ▲                 │ rollback target v2.8.0 signed/SBOM/provenance ✓          │
├─ Recommended decision ────────────────────────────────────────────────────────────────────────────────┤
│ Do not promote. Fix unsigned worker artifact and canary telemetry gate. Rollback remains available.    │
╰─ p promote preview  B rollback preview  d doctor  e evidence  x explain ─────────────────────────────╯
```

### 26.3 Rollback modal

```text
╭─ ROLLBACK CONFIRMATION ────────────────────────────────────────────────────────────────────────────────╮
│ Current prod: v2.8.1 commit ab91c2e  status degraded telemetry error                                  │
│ Rollback to:   v2.8.0 commit ab12cc0  last-known-good ✓ signed ✓ SBOM ✓ provenance ✓                  │
│ Impact: shift 100% traffic to v2.8.0, estimated 4m30s                                                  │
│ Evidence: rollback drill passed, Vault env ready, artifact verified, release passport valid            │
│ Required confirmation: type ROLLBACK veox-api prod v2.8.0                                              │
├───────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ >                                                                                                     │
╰─ Esc cancel ──────────────────────────────────────────────────────────────────────────────────────────╯
```

---

## 27. Evidence and proof timeline

### 27.1 Purpose

Evidence is the universal receipt layer. Every green, red, warning, action, policy, gate, agent claim, and release decision should link here.

### 27.2 Searchable proof API

```http
GET /api/proof?entity=&kind=&since=&actor=&severity=&event_type=&sha=&branch=&mr=&cursor=&limit=
```

MCP:

```text
jeryu.search_proof_timeline
jeryu://proof?entity=job:99122
```

### 27.3 Timeline mock

```text
╭─ Evidence Timeline: veox-core MR !221 ────────────────────────────────────────────────────────────────╮
│ filters entity=MR!221 since=24h │ 184 events │ cursor 184923 │ freshness DB 0.3s GitLab 0.8s           │
├─ Timeline ────────────────────────────────────────────────────────────────────────────────────────────┤
│ 09:41 job.failed          job #99122 test-linux   capsule pending  corr c-91a                         │
│ 09:40 vti.plan.created    plan #778 selected 184 skipped 128 conf .74                                │
│ 09:39 agent.race.started  JRY-412 h1/h2/h3 branches created grants g-812/g-813/g-814                  │
│ 09:35 cache.miss.storm    sccache toolchain fingerprint drift                                        │
│ 09:30 admission.allow     agent/h1 push allowed grant g-812                                           │
│ 09:25 bug.ready           JRY-412 accepted and agent-ready                                            │
╰─ Enter event  y copy corr/id  raw toggle  / filter  Esc back ─────────────────────────────────────────╯
```

### 27.4 Evidence graph

When possible, show graph relationships:

```text
Bug JRY-412
  ├─ Agent race h1/h2/h3
  │   ├─ Branch agent/jry-412-h1
  │   │   ├─ MR !221
  │   │   │   ├─ Pipeline #581
  │   │   │   │   ├─ Job #99122
  │   │   │   │   ├─ VTI plan #778
  │   │   │   │   └─ Artifact veox-api:ab91
  │   │   │   └─ Release gate auth-smoke
  │   │   └─ Capability grant g-812
  └─ Evidence capsule cap-884
```

---

## 28. Settings and Source Doctor

### 28.1 Purpose

The Source Doctor prevents “beautiful UI over shaky truth.” It reveals data-source freshness, doc/source drift, schema mismatches, disabled features, missing credentials, stale action registries, and partial integrations.

### 28.2 Mock

```text
╭─ Source Doctor / Runtime Profile ──────────────────────────────────────────────────────────────────────╮
│ version 0.12.3-dev │ build ab91c2e │ db sqlite │ schema 42 │ action registry sha a81c │ mcp manifest sha 19f │
├─ Sources ──────────────────────────────────────────────────────────────────────────────────────────────┤
│ GitLab REST       fresh 0.8s   p95 120ms   errors 0    ok                                            │
│ Webhooks          fresh 0.1s   broker ok    lag 4      ok                                            │
│ DB                fresh 0.0s   sqlite       path ~/.jeryu/state.db redacted                           │
│ Docker            stale 31s    last error timeout listing n4-remote                                   │
│ Cache             fresh 1.3s   summary ok, detail endpoint missing                                    │
│ Vault             fresh 5.0s   sealed no, token present fingerprint 92af                              │
│ MCP               partial      tools 16, resources 0, streaming disabled                              │
│ Docs              drift ▲      README MCP count differs from action registry                           │
├─ Required fixes ───────────────────────────────────────────────────────────────────────────────────────┤
│ P0 generate API docs from action registry; P0 add read-model endpoint; P1 act on MR webhooks            │
╰─ Enter source  r refresh  g generate docs  e evidence  x explain drift ───────────────────────────────╯
```

### 28.3 Drift checks

- DB backend docs vs runtime backend;
- action registry vs hardcoded list;
- MCP `tools/list` vs docs;
- CLI command tree vs generated docs;
- capability intents vs MCP tools;
- read-model schema version vs TUI client schema;
- API endpoint availability vs spec;
- feature flags vs UI enabled tabs;
- cache summary auth behavior;
- MR webhook support state;
- merge action safety gate state.

---

## 29. LLM/provider inspector

### 29.1 Purpose

Agents and autonomy often depend on model calls. The LLM screen should make cost, latency, reliability, and tool-call fanout visible without leaking secrets.

### 29.2 Data

- provider/model;
- key source, redacted;
- request count;
- token counts;
- cost estimate;
- latency p50/p95;
- error/refusal rate;
- tool-call count;
- linked agent task/action;
- budget ledger row;
- policy/verdict relation;
- evidence digest.

### 29.3 Mock

```text
╭─ LLM / Provider Health ────────────────────────────────────────────────────────────────────────────────╮
│ spend today $18.42 / $50 │ calls 411 │ p95 4.2s │ errors 3 │ refusals 1 │ budget alerts 0 │
├─ Providers ───────────────────────────┬─ Recent Calls ─────────────────────────────────────────────────┤
│ openai:gpt-x    ok   p95 3.8s $12.10  │ agent-7 JRY-412 patch plan  tokens 8.2k  tools 4  ok           │
│ local:coder     slow p95 9.1s $0.00   │ autonomy verdict review     tokens 2.1k  tools 1  ok           │
│ backup:model    err  p95 --   $0.00   │ agent-3 e2e triage          tokens 9.8k  tools 7  err timeout  │
╰─ Enter call  e evidence  b budget  x explain ─────────────────────────────────────────────────────────╯
```

---

## 30. Command palette

### 30.1 Behavior

`Ctrl-K` or `:` opens a fuzzy command palette. It must search:

- screens/lenses;
- repos/families;
- pipelines/jobs;
- bugs/issues;
- agents/tasks;
- actions;
- evidence;
- settings;
- external URLs/paths.

### 30.2 Command row format

```text
retry job #99122              action  risk:low   dry-run:y   entity job#99122
open veox-core MR !221        nav     repo       gitlab url available
explain top blocker           query   attention  evidence 4 refs
scale rust-fast +4            action  risk:med   preview required
rollback veox-api prod        action  risk:prod  typed confirmation required
```

### 30.3 Action preview from palette

Selecting an action opens preview first, never executes immediately.

---

## 31. Search, filters, saved lenses, and pins

### 31.1 Filter language

Support typed filters:

```text
repo:veox-* status:failed kind:job
family:veox-* queue>10
security:critical signed:false
bug:ready owner:agent
jankurai:dup severity:high
cache:type=sccache miss_reason:toolchain
agent:blocked grant:expiring
release:blocked gate:telemetry
```

### 31.2 Saved lenses

Persist locally:

- `all`;
- `veox-*`;
- `prod train`;
- `my attention`;
- `agents only`;
- `security release blockers`;
- `cache pressure`;
- `Jankurai regressions`;
- `release candidates`;
- `flaky tests`.

### 31.3 Pins/watch panel

Users can pin:

- release attempt;
- job log;
- agent race;
- queue capacity;
- cache pressure;
- bug;
- security finding;
- artifact;
- node/pool.

Pinned objects update live even while browsing elsewhere.

---

## 32. Attention ranking and “what should I do next?”

### 32.1 Attention item model

```rust
struct AttentionItem {
    id: String,
    severity: Severity,
    title: String,
    summary: String,
    entity: Option<EntityRef>,
    reason: String,
    confidence: Confidence,
    blast_radius: BlastRadius,
    human_required: bool,
    suggested_actions: Vec<ActionRecommendation>,
    evidence_refs: Vec<EvidenceRef>,
    created_at: DateTime<Utc>,
    freshness: SourceFreshness,
}
```

### 32.2 Ranking formula

```text
rank = severity_weight
     * urgency_weight
     * blast_radius_weight
     * confidence_weight
     * human_required_boost
     * recency_decay
```

Human-required production/security/release blockers should beat noisy low-risk failures.

### 32.3 Explanation

Press `x` on any attention item:

```text
Why is this #1?
- It blocks release candidate v2.8.1.
- It affects veox-api production path.
- It has high-confidence evidence from canary telemetry.
- No autonomous workflow has authority to fix it.
- Rollback remains available, but promote is blocked.
```

---

## 33. Action safety model

### 33.1 Risk tiers and confirmations

| Tier | Examples | Confirmation |
|---|---|---|
| `read_only` | open logs, explain, fetch capsule | none |
| `local_write` | update local bug, save view | preview optional |
| `ci_write` | retry/cancel/play job, run tests | preview required |
| `repo_write` | propose patch, create branch | preview + grant/path scope |
| `merge` | request merge | proof gate + typed confirmation |
| `release` | approve/promote release | proof gate + typed confirmation |
| `secret` | rotate/finalize/recover secrets | proof gate + redaction + typed confirmation |
| `production` | rollback/promote prod | full modal + typed phrase + evidence |
| `destructive` | delete pool/cache object/broad cleanup | full modal + typed phrase + undo statement |

### 33.2 Preview modal requirements

Every preview must show:

- action name and target entity;
- risk tier and side-effect class;
- exact backend tool/endpoint invoked;
- grants required and current grant status;
- idempotency key;
- dry-run availability;
- expected changes;
- expected evidence created;
- blockers/warnings;
- rollback/undo story;
- confirmation requirement.

### 33.3 Execution stream

Action execution should render live:

```text
preview accepted → grant checked → backend call started → event cursor advanced → evidence written → result
```

Failures should produce actionable errors, not raw stack dumps.

---

## 34. Backend plumbing roadmap

### 34.1 P0: unify truth

- Serve `TuiReadModel` externally through `/api/read-model`.
- Serve cursor-based `/api/events`.
- Serve `/api/entity/{kind}/{id}`.
- Serve `/api/action-registry`.
- Route all TUI mutations through action preview/execute.
- Add source freshness metadata everywhere.
- Generate docs from action registry, Clap tree, MCP tools, capability intents, and DB schema.
- Audit `request_merge` risk gate.
- Fix stale action/MCP/docs drift.

### 34.2 P0: event/log streaming

- Add SSE/WebSocket normalized event stream.
- Add log streaming with cursor resume.
- Keep GitLab trace polling fallback.
- Coalesce high-volume events by entity.
- Persist all events in durable ledger even when UI drops frames.

### 34.3 P1: repo-family and MR truth

- Add first-class repo-family projection.
- Fully ingest MR webhooks.
- Capture changed files, labels, draft state, mergeability, reviewers, approvals, linked pipelines.
- Link MR/PR state to Workflow, Agents, Bugs, Release, Evidence.

### 34.4 P1: cache/dashboard expansion

Add:

```http
GET /cache/metrics
GET /cache/hot
GET /cache/taints
GET /cache/verdicts
GET /cache/gc-plan
GET /cache/object/{key}
```

Or expose equivalent under `/api/cache/*`.

### 34.5 P1: agent lifecycle and patch races

- Add `agent_sessions`, `agent_tasks`, `agent_steps`, `agent_messages`, `agent_artifacts`.
- Add race lifecycle table.
- Add `get_race_status`, `select_race_winner`, `cleanup_losing_branches` action surfaces.
- Link branches/pipelines/MRs/bugs/grants/evidence.

### 34.6 P1: deep health and metrics

- `/health/deep` with GitLab, DB, Docker, Vault, cache, broker, runner, disk, reconciliation state.
- `/metrics` for main daemon.
- Broker observability: backend, producer health, consumer lag, offsets, dead-letter/errors, throughput.
- GitLab API latency/rate-limit/error counts.

### 34.7 P1: artifact/report parsing

Parse and ingest:

- JUnit/xUnit XML;
- coverage reports;
- code-quality reports;
- SAST/dependency/container scans;
- benchmark JSON;
- release gate JSON;
- nextest archives;
- SBOM/provenance/signature metadata.

### 34.8 P2: what-if simulator and predictive intelligence

- Queue what-if simulation.
- Predictive green time.
- Agent ROI dashboard.
- Flake intelligence.
- Configuration drift.
- Time-travel replay.
- Natural-language local query over read-model, with citations to entities/evidence.

---

## 35. Rust implementation architecture

### 35.1 Recommended stack

- **Ratatui** for widgets/layout/rendering.
- **crossterm** for terminal events/backends.
- **Tokio** for async tasks and channels.
- **serde/serde_json** for data contracts.
- **schemars** or equivalent for JSON schema generation.
- **tracing** for instrumentation.
- **color-eyre/anyhow/thiserror** for error handling as appropriate.
- Existing JeRyu clients/repos/action registry wherever available.

### 35.2 Module layout

```text
src/tui/
  mod.rs
  app.rs                     # app object, boot/shutdown, terminal restore
  event_loop.rs              # input/data tick orchestration
  config.rs                  # theme, keymap, saved lenses
  route.rs                   # navigation stack, breadcrumbs
  focus.rs                   # macro/micro focus graph
  store/
    mod.rs
    state.rs                 # AppState
    reducer.rs               # event reducer
    selectors.rs             # derived view selectors
    cache.rs                 # bounded view cache
  data/
    mod.rs
    client.rs                # DataClient trait
    http_client.rs           # read-model/events/actions
    local_client.rs          # fallback direct local source
    fake_client.rs           # demo fixtures/tests
    stream.rs                # SSE/WS/cursor polling
    models.rs                # generated/shared DTOs
  actions/
    mod.rs
    registry.rs
    preview.rs
    execute.rs
    modal.rs
  screens/
    global.rs
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
    table.rs
    graph.rs
    sparkline.rs
    progress.rs
    inspector.rs
    event_tape.rs
    command_palette.rs
    help.rs
    modal.rs
    freshness.rs
    mini_map.rs
    log_viewer.rs
  theme/
    mod.rs
    palette.rs
    glyphs.rs
    terminal_caps.rs
  testing/
    fixtures.rs
    golden.rs
    interaction.rs
```

### 35.3 Core traits

```rust
#[async_trait]
trait DataClient: Send + Sync {
    async fn read_model(&self) -> Result<TuiReadModel>;
    async fn events(&self, cursor: u64, filter: EventFilter) -> Result<EventPage>;
    async fn entity(&self, entity: &EntityRef) -> Result<EntityDetail>;
    async fn proof(&self, query: ProofQuery) -> Result<ProofPage>;
    async fn action_registry(&self) -> Result<ActionRegistryView>;
    async fn preview_action(&self, req: ActionPreviewRequest) -> Result<ActionPreview>;
    async fn execute_action(&self, req: ActionExecuteRequest) -> Result<ActionResult>;
    async fn subscribe(&self, cursor: u64) -> Result<EventStream>;
}
```

Rendering contract:

```rust
trait Screen {
    fn id(&self) -> ScreenId;
    fn title(&self, state: &AppState) -> String;
    fn handle_key(&mut self, key: KeyEvent, ctx: &mut ScreenCtx) -> Handled;
    fn draw(&self, f: &mut Frame, area: Rect, state: &AppState, focus: &FocusState);
}
```

### 35.4 Event loop

```text
input task ───────┐
stream task ──────┼──▶ app event channel ─▶ reducer ─▶ render scheduler ─▶ draw
poll task ────────┤
action task ──────┤
tick task ────────┘
```

Rules:

- no network calls during draw;
- coalesce updates by entity when overloaded;
- always process input promptly;
- render at capped cadence, e.g. 30–60 FPS depending terminal and load;
- under overload, drop visual frames, not data;
- preserve terminal restore on panic.

### 35.5 State store

`AppState` should keep:

- current read model;
- event cursor;
- entity details cache;
- route stack;
- focus state;
- filters/search;
- pinned objects;
- action modal state;
- streams and source freshness;
- local UI config;
- diagnostics/performance counters.

Use reducers to keep changes deterministic and testable.

---

## 36. Responsive and accessibility requirements

### 36.1 Width breakpoints

| Width | Mode |
|---:|---|
| `<80` | tiny, ASCII/simple, stacked. |
| `80–109` | compact stacked with command-palette navigation. |
| `110–159` | medium, central canvas + bottom inspector. |
| `160–219` | wide, nav + canvas + inspector. |
| `220+` | cockpit, multi-pane wallboard. |

### 36.2 Accessibility

- No color-only meaning.
- Glyphs have text fallback.
- `?` contextual help everywhere.
- Searchable command palette for every action.
- Stable focus order.
- Configurable keymap.
- Reduced motion option.
- ASCII fallback.
- High-contrast theme.
- Screen capture/export for sharing.

---

## 37. Capture, demo, and evidence bundle mode

### 37.1 Capture keys

- `Ctrl-S`: save current view snapshot.
- `S`: screenshot current screen where supported.
- `Shift-S`: capture diagnostic bundle.

### 37.2 Bundle contents

- redacted ANSI/text screenshot;
- JSON read-model slice;
- entity details for selected object;
- source freshness map;
- event cursor and relevant events;
- evidence refs;
- action registry version;
- runtime profile;
- terminal dimensions/theme;
- redaction report.

### 37.3 Redaction

Automatic redaction must remove:

- tokens;
- secrets;
- env var values;
- private keys;
- credentials in URLs;
- raw request bodies likely containing sensitive fields.

---

## 38. Performance requirements

### 38.1 UI responsiveness

- Input latency p95 under 30 ms in normal state.
- Common render frame p95 under 16 ms on modern laptop; large-list p95 under 33 ms.
- No network or blocking DB calls on render path.
- Smooth scrolling through 100k log lines with virtualization.
- 500 repos and 10k recent jobs remain usable.
- 1k active jobs visible with coalesced updates.
- 100 runner pools and 1k runner managers/nodes supported.
- 100k bug/evidence records searchable through backend pagination.

### 38.2 Backpressure

If event rate exceeds UI handling capacity:

- coalesce status updates by entity;
- preserve terminal input and action results;
- preserve all data in backend event ledger;
- show `coalescing high-volume stream` badge;
- event tape may skip display frames but cursor must remain correct.

### 38.3 Memory

- Bounded live log memory per job.
- Paginated details for huge evidence/bug/history datasets.
- Entity detail LRU cache with explicit invalidation on event cursor.
- Configurable max memory footprint for SSH/small environments.

---

## 39. Testing strategy

### 39.1 Unit tests

- reducers;
- route stack;
- focus traversal;
- keymap dispatch;
- filter parser;
- capacity calculations;
- DAG layout;
- ETA/progress confidence;
- attention ranking;
- action risk/confirmation logic;
- redaction;
- source freshness transitions;
- responsive layout selection.

### 39.2 Golden render tests

Use deterministic fixtures and terminal sizes:

```text
80x24
100x30
120x36
160x48
220x60
```

Golden screens:

- global healthy;
- global degraded source;
- queue saturated;
- repo family with failures;
- repo dashboard;
- pipeline DAG running;
- failed job trace;
- cache pressure;
- VTI selector miss;
- agent race;
- bug board;
- Jankurai regression;
- security grant review;
- release rollback modal;
- evidence timeline;
- settings/source doctor.

### 39.3 Integration tests

- mock HTTP read-model server;
- mock SSE/WebSocket streams;
- disconnect/reconnect with cursor resume;
- fallback from WebSocket to SSE to polling;
- action preview/execute lifecycle;
- stale data warnings;
- terminal panic restore;
- screenshot/capture redaction;
- local fallback client.

### 39.4 Performance tests

- 500 repos fixture;
- 10k jobs fixture;
- 1k live event/sec burst;
- 100 jobs streaming logs concurrently;
- large DAG with downstream pipelines;
- 100k bug/evidence records paginated;
- long-running terminal session leak test.

### 39.5 Safety tests

- high-risk actions cannot execute without preview;
- merge/release/rollback require typed confirmation;
- stale proof blocks risky actions unless policy allows override;
- secret values never render or export;
- action registry mismatch blocks mutation;
- grants are displayed and enforced;
- idempotency prevents double-submit on key repeat.

---

## 40. Implementation phases

### Phase 0 — truth cleanup and contracts

- Verify current DB backend/runtime profile.
- Generate action/MCP/CLI docs from source.
- Add or stabilize shared DTOs for `TuiReadModel`, `TuiEvent`, `EntityDetail`, `ActionPreview`, `ActionResult`.
- Add `/api/read-model`, `/api/events`, `/api/entity`, `/api/action-registry`.
- Add source freshness metadata.
- Audit high-risk action registry classifications.

### Phase 1 — TUI shell foundation

- App boot/shutdown and terminal restore.
- Router/breadcrumb stack.
- Macro/micro focus engine.
- Theme/glyph/capability detection.
- Header/tabs/status strip.
- Command palette shell.
- Fake/demo data client.
- Golden test harness.

### Phase 2 — Global + Queue + Repos

- Global Mission Control.
- Attention queue.
- Repo-family rail and drilldown.
- Queue theoretical-limit screen.
- Pinned watch panel.
- Source Doctor minimal panel.

### Phase 3 — Workflow and logs

- Repo dashboard.
- Pipeline DAG with stage fallback.
- Job detail inspector.
- Log viewer with polling fallback.
- Failure capsule/evidence linking.
- Critical path calculation.

### Phase 4 — Realtime streams

- SSE/WebSocket event stream.
- Cursor resume.
- Log stream.
- Entity-scoped subscriptions.
- Backpressure/coalescing.
- Stream diagnostics.

### Phase 5 — Domain cockpits

- Runners/System.
- Cache.
- VTI.
- Agents.
- Autonomy.
- Bugs.
- Git Sync.
- Bottlenecks.
- Jankurai.
- Security/Secrets.
- Artifacts.
- Release.
- Evidence.
- LLMs.

### Phase 6 — Safe actions

- Action registry integration.
- Action menus per entity.
- Preview modals.
- Execution streams.
- Risk-tier confirmation.
- Grant display and checks.
- Evidence receipts.

### Phase 7 — polish and scale

- Responsive layouts.
- Themes and reduced motion.
- Saved lenses.
- Capture/evidence bundles.
- Performance tuning.
- Accessibility pass.
- Documentation and embedded help.

### Phase 8 — dream extras

- What-if queue simulator.
- Predictive green time.
- Agent ROI dashboard.
- Time-travel replay.
- Flake intelligence.
- Natural-language local query over read-model with proof citations.
- Demo mode for presentations.

---

## 41. Acceptance criteria

Hyperdeck is successful when a developer can do the following entirely from the TUI:

1. See whether the whole fleet is safe to code/merge/release within five seconds.
2. See live queue across all jobs/repos/families and understand actual vs theoretical capacity.
3. Drill from repo family → repo → pipeline → job → live trace → failure capsule.
4. Explain why a pipeline is slow and whether adding runners helps.
5. Distinguish runner saturation from blocked DAGs, cache misses, VTI expansion, and release/security gates.
6. See cache fullness, categories, hot objects, taints, misses, and safe GC plan.
7. Verify VTI saved time safely and inspect selector misses.
8. Inspect active agents, patch races, grants, logs, configs, evidence, and LLM cost.
9. Edit agent/autonomy config with validation, diff, audit, and rollback.
10. View all bugs/issues across repos and drill into attempts, branches, MRs, CI evidence, and status.
11. Confirm local/remote/MR/mirror/hook/admission state.
12. View Jankurai version, score trend, caps, findings, duplicate groups, and run/fix actions.
13. Determine whether runners/nodes can be scaled and whether current runners are underused or saturated.
14. See code churn over time by repo/agent/PR and its risk relation to failures.
15. Review security posture, secret metadata, active grants, admission denials, and unsigned artifacts.
16. Verify artifact signatures, SBOM, provenance, and release candidate evidence.
17. Promote, block, or roll back production only through proof-gated confirmation.
18. Export a redacted screenshot/evidence bundle for an issue or release decision.
19. Identify stale sources and doc/source drift before trusting risky actions.
20. Navigate the whole system with arrows, Tab, Enter, Esc, and command palette without a mouse.

---

## 42. Non-goals and guardrails

- Do not display secret values.
- Do not create a second mutation path outside the action registry/capability model.
- Do not block rendering on backend calls.
- Do not show high-precision ETA without confidence.
- Do not hide stale/partial data state.
- Do not mark the fleet green if selector misses, security blockers, or release gates are rising.
- Do not require mouse.
- Do not rely on Unicode/color only.
- Do not let global screen become a dumping ground; use progressive disclosure.
- Do not hardcode MCP/action lists; fetch/generate from source.
- Do not execute risky actions from keyboard repeats or stale previews.

---


## 43. Code change volume and development velocity

### 43.1 Purpose

The Churn/Velocity screen explains whether failures, slowdowns, or release risk are correlated with the amount and shape of recent code change. It should not shame velocity; it should identify risk concentration.

It must answer:

- Which repos are changing fastest?
- Which PR/MR or agent has the largest blast radius?
- Are generated files, vendored files, migrations, tests, or production paths dominating the diff?
- Did recent change volume correlate with CI time, cache misses, VTI misses, Jankurai regressions, security findings, or release blockers?
- Which changes need more review, tests, or agent attention?

### 43.2 Mock

```text
╭─ Code Change Volume / Risk ────────────────────────────────────────────────────────────────────────────╮
│ last 24h +18,421 -7,204 │ repos 22 │ agents 9 │ largest MR !221 │ risk trend ↑ │ generated 31% │ tests 18% │
├─ Repo churn ───────────────────────┬─ Risk concentration ─────────────────────┬─ Selected change ────────┤
│ repo          +lines -lines files   │ MR !221 veox-core +2,184 -731 risk high  │ paths src/auth/**        │
│ veox-core     5182   2110   144     │ agent-7 h1 +82 -31 risk med             │ tests auth/e2e impacted  │
│ veox-api      3211   1802    87     │ veox-api main +1104 -200 sec high       │ VTI confidence .74 ▲     │
│ veox-web      2180    992    61     │ generated protobuf +3200 low            │ Jankurai delta -1.4      │
├─ Correlations ────────────────────────────────────────────────────────────────────────────────────────┤
│ cache miss spikes follow toolchain/Cargo.lock changes; VTI misses cluster around auth/payment changes. │
╰─ Enter change  t tests  j Jankurai  s security  e evidence  x explain risk ──────────────────────────╯
```

### 43.3 Required metrics

- lines/files changed by repo/family/branch/MR/agent;
- production/test/docs/generated/vendor/migration split;
- changed subsystems and ownership;
- churn trend over time;
- risk-weighted churn;
- correlation with failed jobs, slow jobs, cache misses, VTI misses, Jankurai score, security findings, release gates;
- review coverage and approval state;
- agent-generated vs human-generated changes;
- revert/rollback proximity.

### 43.4 Risk scoring

```text
change_risk = path_criticality
            * lines_changed_weight
            * file_count_weight
            * ownership_gap_weight
            * test_confidence_inverse
            * recent_failure_correlation
            * security_surface_weight
            * release_proximity_weight
```

Render this as an explanation, not a mysterious number.

---

## 44. Advanced screens and operator superpowers

### 44.1 Incident mode / war room

Incident mode is a filtered, high-contrast, low-noise view for production or release emergencies.

Features:

- freezes the route to a selected incident entity;
- pins release, rollback, critical logs, telemetry gate, responsible agents, and evidence;
- suppresses non-incident event noise;
- shows elapsed incident time and decision ledger;
- makes rollback and mitigation paths explicit;
- exports an incident evidence bundle.

Mock:

```text
╭─ INCIDENT MODE: veox-api prod telemetry degradation ─ elapsed 12m ─ rollback ready ✓ ─────────────────╮
│ current prod v2.8.1 degraded │ canary telemetry fail │ top suspect artifact veox-worker unsigned ▲     │
├─ Live signals ─────────────────────────┬─ Mitigation paths ───────────────────┬─ Decision ledger ───────┤
│ telemetry error rate 7.2% ↑            │ 1 rollback to v2.8.0 safe ✓           │ 09:42 incident opened    │
│ release gate telemetry ✗               │ 2 disable worker feature flag ?       │ 09:44 agent assigned     │
│ job #99122 auth failure                 │ 3 hold promote, wait for h1 CI        │ 09:47 rollback preview   │
╰─ B rollback preview  e evidence  p pin  Esc leave incident mode ──────────────────────────────────────╯
```

### 44.2 Time-travel replay

The Evidence/Event ledger should allow replaying what the TUI would have shown at an earlier cursor or timestamp.

Use cases:

- debug why an agent made a decision;
- reconstruct release approval;
- compare before/after a cache miss storm;
- replay a flaky pipeline;
- create demo/training material.

Controls:

```text
[Space] play/pause  [←/→] step event  [Shift←/Shift→] jump 1m  [r] return live
```

### 44.3 Flake intelligence

A dedicated flake lens should aggregate:

- flaky test name;
- repo/component;
- failure signatures;
- retry outcomes;
- recent owners;
- relation to cache, runners, VTI, time of day, and changed files;
- confidence that it is a true flake vs real regression;
- recommended quarantine/repair action.

### 44.4 CI economics

Show cost and waste without making cost the only optimization target:

- runner-hours by repo/family;
- cache savings;
- VTI saved time;
- retry waste;
- agent compute/LLM spend;
- cost per merged PR/MR;
- cost per release candidate;
- “cost of not fixing this bottleneck.”

### 44.5 Configuration drift and governance posture

Governance view should show:

- repos missing hooks;
- drift from standard CI template;
- stale runner configs;
- inconsistent cache settings;
- missing branch protection;
- policy SHA mismatch;
- action registry/schema drift;
- docs generated from stale source;
- repo standardization plan/apply/verify state.

### 44.6 Review queue

A review lens should unify human and agent review obligations:

- PRs/MRs needing review;
- agent patch races needing winner selection;
- security exceptions needing approval;
- release gates needing human action;
- broad grants needing revocation;
- bugs needing triage;
- stale approvals bound to old SHA.

### 44.7 Predictive green time

For active repo/family/release, show:

```text
predicted green: 18m ± 6m
critical path: test-linux → integ-db → package
main risks: rust-fast queue, VTI low confidence, unsigned worker artifact
```

Prediction must always include confidence and reason.

### 44.8 Ask the system, locally and with proof

A natural-language query box can be added after the typed read model is stable. It must not be magic. It should answer only from structured local/read-model/evidence data and cite entity/evidence IDs.

Examples:

```text
Why is veox-* not green?
What should I do before promoting v2.8.1?
Which agents are blocked by grants?
What cache category is growing fastest?
Which bugs are ready for an agent but have failed twice?
```

The answer should be a ranked list of facts with drillable proof refs.

### 44.9 Scream mode

Scream mode is an optional visual intensity mode for wallboards or demos. It may use brighter colors and more motion, but the same evidence/freshness rules apply. It must never hide details or turn warnings into mere decoration.

---

## 45. Appendix A — current inspectable data catalog

This catalog condenses the source-derived `tip*.txt` inventory into build guidance.

### 45.1 GitLab live data

- projects;
- pipelines and downstream pipelines;
- jobs and job traces;
- artifacts and artifact files;
- variables;
- runners;
- branches and protected branches;
- merge requests;
- issues;
- webhook deliveries for Job/Pipeline/Push and partial MR handling;
- queued duration, duration, stage, status, runner, web URL;
- cancel/retry/play mutation surfaces.

### 45.2 Durable DB data

Runner/CI/release:

```text
pools, managers, job_events, ci_job_runs, tracked_pipelines, tracked_repositories, release_attempts
```

Capability/admission/git audit:

```text
capability_intents, capability_grants, admission_decisions, git_command_events,
git_ref_updates, git_mirror_jobs, git_risk_approvals, git_command_artifacts, events
```

Evidence/retry/VTI/test intelligence:

```text
evidence_capsules, retry_decisions, test_executions, test_plans, test_plan_items, selector_misses
```

Cache/provenance/material:

```text
cache_objects, cache_requests, hot_cache_entries, build_signatures, image_signatures,
force_refresh_rules, resolved_refs, cache_taints, cache_leases, cache_verdicts,
cache_promotions, material_objects, material_aliases, action_cache, cache_epochs,
toolchain_fingerprints
```

Secrets/Vault:

```text
secret_authorities, release_secret_sets, secret_audit_events
```

Bug tracker:

```text
bug_projects, bug_project_edges, bugs, bug_events, bug_attempts,
bug_links, bug_external_refs, bug_evidence
```

Autonomy/Evidence Gate:

```text
launch_ledger, kill_bell_state, verdicts, foundry_candidates, llm_budget_ledger
```

### 45.3 CLI command families to mirror in UI/actions

| Command family | UI relevance |
|---|---|
| `init`, `bootstrap`, `serve`, `down` | runtime/system lifecycle, source doctor. |
| `install`, `remote` | host/remote management, logs, tunnel, restart. |
| `tui` | capture/demo/profile behavior. |
| `git`, `save`, `sync`, `undo` | Git Sync screen and repo actions. |
| `system`, `status`, `settings` | System health and Settings screen. |
| `pool`, `job`, `pipeline`, `node` | Runners, Workflow, Queue, Bottlenecks. |
| `cache`, `local` | SmartCache screen. |
| `logs`, `agent` | Live trace and Agents screens. |
| `test` | VTI/test intelligence. |
| `release` | Release/rollback screen. |
| `secrets` | Security/Secrets screen. |
| `progress`, `next`, `explain-blocker`, `action` | Attention queue and command palette. |
| `repo`, `host`, `policy` | Repo families, governance, Source Doctor. |
| `bug` | Bugs screen. |
| hidden `exec`, `server-hook`, `capability`, `mcp` | Executor, admission, agent, MCP/source doctor. |

### 45.4 Ports and default settings worth surfacing

Show these in Settings/Source Doctor, redacted where needed:

- GitLab HTTP default `8929`;
- GitLab SSH default `2224`;
- Vault default `18200`;
- webhook default `127.0.0.1:9777`;
- MCP HTTP default `127.0.0.1:9778`;
- cache proxy default `19800`;
- OCI registry mirror default `19801`;
- settings path `~/.jeryu/settings.json`;
- state DB backend/path;
- cache directories and size limits;
- runner/cache/sccache/release/TUI settings.

---

## 46. Implementation agent checklist

Use this order:

1. Read current `src/api` and identify existing `TuiReadModel`, `TuiEvent`, `EntityDetail`, `ActionPreview`, `ActionResult` equivalents.
2. Read action registry and capability/MCP tool generation; do not duplicate hardcoded action metadata.
3. Add missing backend inspection endpoints behind feature flags if necessary.
4. Create deterministic demo fixtures for every screen before live wiring.
5. Build TUI shell with reducer/router/focus/theme/status strip.
6. Implement Global, Queue, Repos with fake data and golden tests.
7. Wire `/api/read-model` and `/api/events` fallback to polling.
8. Implement entity drilldown generically from `EntityRef`.
9. Implement Pipeline DAG with stage fallback, then explicit graph edges.
10. Implement log viewer with polling fallback; upgrade to stream when ready.
11. Implement action preview/execution through the action registry.
12. Add domain screens one by one, keeping every row drillable.
13. Add streaming/cursor resume/backpressure.
14. Add source doctor, drift checks, and schema mismatch blocking.
15. Add screenshot/capture and redaction.
16. Add performance tests and long-session leak tests.
17. Only after correctness and trust are solid, add motion polish, themes, and ultra-dense layouts.

---

## 47. Final desired feel

The best version feels like this:

1. You open `jeryu tui`.
2. The header immediately tells you: code safe, merge risky, release blocked, rollback ready.
3. The fleet panel shows `veox-*` is the hot family and `rust-fast` is the current limit.
4. The queue screen proves the system is at 91% effective capacity but only 65% of theoretical because of tag saturation and cache misses.
5. `Enter` drills into `veox-core`, then MR `!221`, then pipeline `#581`, then `test-linux`.
6. Logs stream live, the failure line is annotated, and the capsule is one key away.
7. The bug board shows `JRY-412` is already being raced by three agents.
8. The agent race view compares hypotheses, CI state, risk, Jankurai delta, and grants.
9. The release screen refuses promotion because telemetry, artifact signature, and Jankurai caps are not satisfied.
10. Rollback remains ready, signed, evidenced, and typed-confirmation gated.
11. A redacted evidence bundle can be exported for humans or agents.

That is the bar: **incredible colors, incredible motion, incredible control, fast drilldown, safe actions, and proof for every claim.**
