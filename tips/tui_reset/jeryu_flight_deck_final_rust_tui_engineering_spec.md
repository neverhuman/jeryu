# JeRyu Flight Deck — Final Rust TUI Engineering Specification

**Date:** 2026-05-26  
**Artifact:** final consolidated engineering spec from the supplied `tui2.tar(6).gz` archive  
**Product name:** **JeRyu Flight Deck** (`jeryu tui`, `jeryu cockpit`)  
**Audience:** Rust implementation engineers, CI/CD platform engineers, autonomous-agent engineers, and operators managing many repo families such as `veox-*`, `redline-*`, isolated repos, shared runners, SmartCache, VTI, Jankurai, releases, security, bugs, and agents.  
**North-star promise:** One terminal shows every repo, every job, every queue, every runner, every cache pressure point, every agent, every bug, every proof, every release, and every safe action — live, colorful, drillable, and fast.

---

## 0. Source review and final design stance

I reviewed the archive as a design corpus, not as a running production system. It contains API/realtime inventories plus multiple prior attempts at a dream CI TUI. The strongest shared conclusion across those files is:

> **Do not build a prettier pile of tabs. Build a realtime entity graph with a normalized read model, proof-backed drilldown, a capacity frontier, and a universal action preview system.**

The final design is **JeRyu Flight Deck**: a terminal-native command center with three constantly available primitives:

1. **Entities** — repo families, repos, refs, pipelines, jobs, runners, pools, nodes, cache objects, VTI plans, tests, agents, grants, bugs, MRs/PRs, artifacts, releases, Jankurai findings, security findings, secrets, evidence capsules, and events.
2. **Events** — monotonic, cursor-addressable updates from GitLab webhooks, GitLab REST reconciliation, Docker events, SmartCache, Vault, Git hooks, Jankurai, release automation, LLM provider activity, agent lifecycle, and the durable DB.
3. **Actions** — discoverable, previewable, risk-classified, capability-gated, dry-runnable where possible, evidence-producing, and auditable.

The winning UX is not one screen. It is a **navigation grammar**:

```text
Global Mission  →  Repo Family  →  Repo  →  Workflow/Pipeline  →  Job/Trace/Evidence  →  Action/Proof
      ↑                ↑             ↑             ↑                       ↑                  │
      └────────────────┴─────────────┴─────────────┴───────────────────────┴──────────────────┘
                                  Esc always goes up one level
```

Everything visible is focusable. `Enter` drills down. `Esc` goes up. `Tab` moves sideways. Arrow keys move within the current focus plane. `Ctrl-K` opens the command/action palette. `/` filters. `?` explains the local controls.

### 0.1 Input files studied

| File | Lines | Bytes |
|---|---:|---:|
| `jeryu_dream_rust_tui_engineering_spec(1).md` | 2,590 | 102,649 |
| `jeryu_dream_rust_tui_engineering_spec(2).md` | 2,640 | 112,690 |
| `jeryu_dream_rust_tui_engineering_spec.md` | 2,358 | 126,838 |
| `jeryu_dream_rust_tui_spec(1).md` | 2,363 | 108,702 |
| `jeryu_dream_rust_tui_spec.md` | 2,044 | 113,572 |
| `jeryu_dream_tui_engineering_spec(1).md` | 2,298 | 106,518 |
| `jeryu_dream_tui_engineering_spec(2).md` | 2,112 | 108,387 |
| `jeryu_dream_tui_engineering_spec.md` | 2,717 | 108,705 |
| `tip1.txt` | 448 | 46,966 |
| `tip2.txt` | 183 | 42,733 |
| `tip3.txt` | 691 | 50,738 |
| `tip4.txt` | 673 | 61,453 |
| `tip5.txt` | 695 | 50,014 |
| `tip6.txt` | 143 | 35,391 |
| `tip7.txt` | 462 | 31,816 |
| `tip8.txt` | 107 | 4,197 |
| `tip9.txt` | 89 | 6,609 |

---

## 1. Source-derived baseline

### 1.1 What JeRyu already exposes or implies

The uploaded inventories describe JeRyu as a Rust single-binary CI/CD control plane around Git, GitLab, runner orchestration, custom executor sandboxes, SmartCache, VTI smart testing, releases/canary/rollback, Vault/secrets, local bugs, agents, MCP/capability actions, Git admission, durable evidence, Jankurai, LLM provider accounting, and a Ratatui/crossterm terminal UI.

Use the **source-derived inventories over stale docs** where they conflict. The archive repeatedly calls out these important facts:

- SQLite is the current default state store; RedlineDB is optional/feature-gated in places where old docs imply RedlineDB-only.
- Current MCP has **16 tools**, including local bug tools; older docs undercount the MCP/action surface.
- `/cache/summary` is not just an unauthenticated toy endpoint; source-derived notes say it requires `X-Jeryu-Token` matching the webhook secret.
- MR webhooks are accepted/logged but not first-class state transitions yet.
- Current TUI has strong bones but important limitations: log polling instead of streaming, incomplete workflow graph edges, first-active-pipeline bias, heuristic ETA, non-searchable evidence timeline, and no dedicated agent lifecycle table.

### 1.2 Existing control-plane surfaces

| Surface | Existing entrypoint / transport | Realtime data or control available |
|---|---|---|
| CLI | `jeryu <command>` | Install/bootstrap, serve, remote, node, Git wrappers, repo/fleet, status, pools, jobs, pipelines, cache, logs, agents, settings, tests/VTI, releases, secrets, bugs, policy, host, MCP, blockers, actions. |
| TUI | `jeryu tui`, `--once`, `--capture` | Mission, Workflow/Delivery, Jobs/Flow, Release, Pools, Cache, Evidence, Tests, Agents, Secrets, LLMs, Git, screenshot/capture fixtures. |
| Internal TUI API/read model | `src/api/*` style typed model | `TuiReadModel`, `TuiEvent`, `EntityDetail`, `ActionPreview`, `ActionResult`, freshness, mission snapshot, system health, event taxonomy. |
| MCP stdio | `jeryu mcp serve` | JSON-RPC initialize/ping/tools/list/tools/call; action registry backed. |
| MCP Streamable HTTP | `jeryu mcp serve-http`, default `127.0.0.1:9778`, `/mcp` | POST JSON-RPC, DELETE session, GET currently disabled in source-derived notes, loopback/origin/session/method/name checks. |
| Capability Unix socket | `jeryu capability serve <socket_path>` | Agent action envelope with actor, nonce, expiry, idempotency, grant, budget, intent, response. |
| Webhook/API server | Axum, default `127.0.0.1:9777` | `GET /health`, `POST /hooks`, `GET /cache/summary`; GitLab Job/Pipeline/Push webhooks; MR accepted/logged only. |
| GitLab REST wrapper | internal client | Projects, files, commit actions, jobs, traces, artifacts, pipelines, bridges, downstream jobs, variables, runners/managers, issues, MRs, branches, protected branches, webhook install, retry/cancel/play. |
| Message log | Kafka or Jansu feature profile | Topics `jeryu.webhook.jobs`, `jeryu.webhook.pipelines`, `jeryu.webhook.pushes`. |
| Custom executor | hidden `jeryu exec config/prepare/run/cleanup` | Runner lifecycle, sandbox copy, honeypot, job env, logs, failure capsules, artifacts. |
| Git admission hook | `jeryu server-hook pre-receive` | Ref update policy, actor kind, grant id, allow/audit/deny, protected ref violations, evidence. |
| SmartCache/gateway | proxy `19800`, OCI mirror `19801` | Cargo sparse config/downloads, CAS hits, singleflight, request metrics, hot entries, taints, verdicts, leases, epochs, toolchain fingerprints. |
| Docker/runner plane | Bollard + compose + remote ops | Managed runner containers, labels, logs, Docker `die`/`oom`, manager lifecycle, remote nodes, storage GC. |
| Vault/secrets | Vault HTTP + DB audit | health/seal/init, authorities, release secret sets, rendered paths, expiries, rotation/finalization, audit events. |
| State DB | SQLite default; RedlineDB optional | Pools, managers, jobs, pipelines, events, releases, evidence, retry decisions, cache, tests, selector misses, secrets, grants, admission, Git events, launch ledger, verdicts, LLM budget, bugs. |
| Bug tracker | CLI + MCP/capability + DB | Canonical bug reports, project links, events, attempts, external refs, evidence, statuses. |
| LLM provider layer | provider abstraction | provider/model, latency, token/cost ledger, key source, data-use policy, failures, budget. |
| GitHost abstraction | GitHub/GitLab adapters | PR/MR state, diffs, comments, SHA-bound approvals, workflow/check runs, policy SHA, merge-passport check. |
| Jankurai | audit tooling/action | score/version/trend, caps, duplicate clusters, structured findings, proof artifacts once plumbed. |

### 1.3 Current MCP tools to preserve and expose in UI

| Tool | Kind | TUI integration |
|---|---:|---|
| `jeryu.fetch_capsule` | read | Job failure/evidence detail pane. |
| `jeryu.get_system_snapshot` | read | Global seed and degraded fallback. |
| `jeryu.get_pipeline_jobs` | read | Workflow DAG/job list. |
| `jeryu.get_ci_bottlenecks` | read | Bottleneck lab and capacity page. |
| `jeryu.explain_blockers` | read | Attention queue, `x` explain, blocker modal. |
| `jeryu.plan_validation` | read | VTI proof and selector-miss validation. |
| `jeryu.run_tests` | mutate | Targeted test action with preview. |
| `jeryu.propose_patch` | mutate | Agent/human patch proposal. |
| `jeryu.race_patches` | mutate | Hypothesis race arena. |
| `jeryu.request_merge` | high-risk mutate | Merge action; must be proof/SHA/evidence gated. |
| `jeryu.bug_submit` | local mutate | Create bug from failure/finding/log selection. |
| `jeryu.bug_list` | read | Bug board. |
| `jeryu.bug_show` | read | Bug detail pane. |
| `jeryu.bug_ready` | read | Agent-ready work queue. |
| `jeryu.bug_update` | local mutate | Triage/update. |
| `jeryu.bug_record_attempt` | local mutate | Append agent attempt history. |

### 1.4 Source/docs drift becomes a first-class operational risk

Build a **Data Source Doctor** because stale docs and stale action metadata are themselves unsafe in an agent-facing system.

| Drift / risk | Required TUI treatment |
|---|---|
| Docs undercount MCP/action tools | Header/system doctor shows action registry hash, MCP manifest hash, docs hash, mismatch warning. |
| SQLite default vs RedlineDB-only docs | Runtime profile always shows DB backend/path/profile. |
| `/cache/summary` auth drift | API doctor shows auth requirement and last auth failure, never token. |
| `ListAllowedActions` stale vs registry | Generate all UI action lists from the action registry; fail tests if mismatched. |
| `request_merge` may be too direct | Treat merge as high risk until proven gated; require exact SHA, evidence, approval, and typed confirmation. |
| MR hooks logged but not acted on | Mark MR realtime state as `PARTIAL` until MR ingestion is plumbed. |
| Agents lack lifecycle table | Agents page shows `INFERRED` badge until dedicated tables exist. |

---

## 2. Product philosophy and non-negotiable UX laws

### 2.1 The operator should never wonder

Every pane must answer at least one of these questions:

- What is happening right now?
- Why is it happening?
- Is it healthy, slow, blocked, risky, stale, or trusted?
- What changed recently?
- What is the proof?
- What should I do next?
- What happens if I take that action?
- Is this measured, historical, configured, or heuristic?

### 2.2 Every visible thing is addressable

Anything displayed must be backed by an `EntityRef` or `EventRef`:

```rust
pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
    pub label: String,
    pub repo_slug: Option<String>,
    pub family: Option<String>,
    pub source: SourceRef,
}
```

Focusable entities include repo families, repos, branches, refs, pipelines, jobs, runners, pools, nodes, cache categories, cache objects, taints, VTI plans, tests, agents, grants, bugs, bug attempts, MRs/PRs, artifacts, releases, gates, Jankurai audits, security findings, secret authorities, events, and evidence capsules.

### 2.3 Every warning explains itself

A red/yellow item must include:

- short warning label (`QUEUE SATURATED`, `CACHE TAINT`, `VTI MISS`, `UNSIGNED`, `AGENT BLOCKED`, `MR DRIFT`, `SECRET TTL`, `PROD GATE FAILED`),
- cause line,
- confidence label,
- data age,
- next action,
- proof/trace/config link.

### 2.4 Evidence over vibes

Never show green without a proof path. Green should drill into one or more of:

- commit SHA/ref,
- pipeline/job IDs,
- VTI plan receipt,
- test report,
- cache verdict,
- Jankurai audit run,
- security scan,
- artifact digest/signature/SBOM/provenance,
- release gate evidence,
- admission decision,
- capability grant,
- secret audit metadata,
- source freshness timestamp.

If proof is missing, display `OK?`, `HEUR`, `STALE`, `NO PROOF`, or `UNVERIFIED`.

### 2.5 No modal dead ends

Every overlay supports:

| Key | Required behavior |
|---|---|
| `Esc` | Cancel/up/close. |
| `Enter` | Accept/drill/default. |
| `/` | Filter list fields. |
| `?` | Local help. |
| `Ctrl-G` | Emergency close to Global. |
| `y/n` | Simple confirmation. |
| `d` | Dry-run where supported. |
| `e` | Evidence for the modal’s entity/action. |

### 2.6 Human trust beats animation

The UI should feel alive, but it may never fake certainty. Progress bars distinguish:

- `MEAS` measured actual progress,
- `STRUCT` structured job/test events,
- `ART` artifact/test report parsed,
- `HIST` historical duration estimate,
- `HEUR` heuristic fallback,
- `CONF` configured-only estimate,
- `MISS` missing source,
- `STALE` last-known truth.

Example:

```text
nextest-shard-7  ● running  43% MEAS  elapsed 11m  p95 18m  runner build-3  q=2m  CRIT
security-sast    ○ queued    0% CONF  wait 04m    tag security  cap 3/4
release-prod     ! blocked   gate artifact-signature-missing  evidence E-8812
```

### 2.7 No blank screens

A pane may render real data, stale last-known data, loading skeleton, degraded source marker, explicit empty state, or fixture/demo state. It must never silently go blank.

---

## 3. Final information architecture

### 3.1 Primary lenses

| Lens | Shortcut | Purpose | Operator question |
|---|---|---|---|
| Global | `g0` | Whole fleet mission control | “Is the engineering machine healthy?” |
| Queue / Capacity | `gq` | Live queue and theoretical limit | “Should I add runners, rebalance tags, or fix another bottleneck?” |
| Repos / Families | `gr` | Repo family map | “Which repo/family needs attention?” |
| Workflow Atlas | `gw` | Pipeline/PR/bug/release DAG | “What exactly is running and what is blocked?” |
| Runners / System | `gu` | Pools, nodes, managers, CPU/mem/disk | “Are we near core/memory/disk limits?” |
| Cache | `gc` | Storage, hit rate, taints, GC plan | “Are we full and what is taking space?” |
| VTI / Tests | `gv` | Test selection proof | “Is smart skipping working safely?” |
| Agents | `ga` | Agent sessions/logs/grants | “What are the agents doing?” |
| Autonomy | `go` | Kill bell, verdicts, workflows | “Can autonomous workflows act safely?” |
| Bugs | `gb` | Cross-repo bug board | “What is pending, assigned, failed, verifying, done?” |
| Git Sync | `gg` | Mirrors, MRs/PRs, drift | “Are repos synced and merge attempts healthy?” |
| Jankurai | `gj` | Audit score/version/findings | “Is code quality improving or blocking release?” |
| Churn | `gh` | Commit volume, hot files, risk | “Where is change risk building?” |
| Security | `gs` | Findings, secrets, policies | “What blocks merge/release from a security standpoint?” |
| Artifacts | `gi` | Signatures, SBOM, provenance | “Can I trust what we built?” |
| Release | `gR` | Canary/prod/rollback | “What is in prod and how do I rollback?” |
| Evidence | `ge` | Universal proof timeline | “Show me exactly what happened.” |
| Runtime/API Doctor | `gS` | Config, sources, MCP/API health | “What is enabled, stale, missing, or drifting?” |

### 3.2 Navigation hierarchy

```text
Universe
├─ Repo families: veox-*, redline-*, jeryu, jankurai, isolated/*
│  ├─ Repositories
│  │  ├─ Branches / refs / MRs / PRs
│  │  ├─ Workflows / pipelines / release attempts / bug attempts
│  │  │  ├─ Stages / phases
│  │  │  ├─ Jobs
│  │  │  │  ├─ live trace
│  │  │  │  ├─ artifacts
│  │  │  │  ├─ failure capsule
│  │  │  │  ├─ test report
│  │  │  │  └─ retry/cancel/play/explain
│  │  │  └─ downstream pipelines / child graphs
│  │  ├─ Agents / autonomous workflows
│  │  ├─ Bugs / attempts / evidence
│  │  ├─ VTI plans / selector misses / test executions
│  │  ├─ Cache namespace / taints / verdicts
│  │  ├─ Jankurai audits / findings
│  │  ├─ Security findings / secrets / policies
│  │  ├─ Artifacts / SBOM / signatures / provenance
│  │  ├─ Releases / canaries / prod / rollback
│  │  └─ Git sync / mirrors / admission
│  └─ Shared policies / runners / cache / release gates
└─ Global infrastructure
   ├─ Runner pools / managers / remote nodes / Docker
   ├─ SmartCache / CAS / crates / target / sccache / OCI / npm / git
   ├─ Vault / secret authorities
   ├─ MCP / capability / action registry
   ├─ LLM providers / key pools / budget
   ├─ Broker / event store / proof timeline
   └─ Host CPU / memory / disk / network
```

### 3.3 Route stack

```rust
pub struct RouteStack {
    pub stack: Vec<Route>,
    pub history: Vec<Route>,
    pub forward: Vec<Route>,
}

pub enum Route {
    Global,
    Queue { filter: QueueFilter },
    RepoFamily { family: String },
    Repo { slug: String, tab: RepoTab },
    Workflow { scope: WorkflowScope, selected: Option<EntityRef> },
    Entity { entity: EntityRef, tab: InspectorTab },
    Evidence { query: ProofQuery },
    RuntimeDoctor,
}
```

Breadcrumb example:

```text
Fleet > veox-* > veox-core > MR !1842 > Pipeline #530 > Job nextest-shard-7 > Logs
```

---

## 4. Input model and keymap

### 4.1 Input modes

| Mode | Border/title | Arrow behavior | Enter | Esc |
|---|---|---|---|---|
| Root/macro | focused pane border | move between panes/entities depending screen | drill into focused entity/pane | route up/back |
| Drill/micro | title includes `[esc]` | move inside current pane/table/graph/log | open selected child/detail | exit drill |
| Filter | title includes `filter:` | edit cursor/history | apply/select | close filter |
| Command palette | modal | select command/action | preview/execute | close |
| Confirm action | risk-colored modal | edit confirmation/select option | execute confirmed action | cancel |
| Text edit | editor border | edit field | save/preview | cancel |
| Help | overlay | scroll help | close or select topic | close |

### 4.2 Universal keys

| Key | Action |
|---|---|
| `q` | Quit with confirmation if actions/streams active. |
| `Esc` | Go up one level, exit drill, or close overlay. |
| `Enter` | Drill into focused object or accept default. |
| `Backspace` | Navigation history back. |
| `Ctrl-O` | Navigation history forward. |
| `Tab` / `Shift-Tab` | Next/previous pane, tab, or sibling group. |
| `↑↓←→` | Move focus/selection. |
| `h/j/k/l` | Vim aliases. |
| `Home/End` | First/last item. |
| `PgUp/PgDn` | Page scroll. |
| `Ctrl-K` or `:` | Command/action/entity palette. |
| `/` | Filter current scope. |
| `?` | Context help. |
| `g` then key | Go-to menu. |
| `b` | Jump to top blocker for current scope. |
| `c` | Jump to critical path; context-specific cancel only via palette/preview. |
| `x` | Explain selected warning/wait/blocker. |
| `e` | Evidence/proof for selected entity. |
| `l` | Logs/traces for selected job/agent/workflow. |
| `a` | Actions for selected entity. |
| `o` | Open source URL/path if safe. |
| `y` | Copy ID/SHA/path/digest. |
| `p` | Pin/unpin; never pause without preview. |
| `f` | Follow live event/log stream. |
| `r` | Context retry/rerun/rollback preview. |
| `s` | Context scale/sync/security action preview. |
| `Ctrl-R` | Refresh current dashboard. |
| `Ctrl-S` | Save/export current view snapshot. |
| `Ctrl-G` | Emergency home, close all overlays. |

### 4.3 Command palette

The command palette is generated from route registry, entity registry, and action registry.

```text
╭─ Command Palette ─────────────────────────────────────────────────────────────╮
│ query: scale build                                                           │
├──────────────────────────────────────┬───────────────────────────────────────┤
│ > Scale build pool +2                │ Action: pool.scale                    │
│   Queue: build bottleneck report     │ Risk: R2 capacity mutation            │
│   Open runners build pool            │ Side effects: create runner managers  │
│   Explain build queue                │ Dry-run: yes                          │
│                                      │ Required proof: queue p95 > SLO       │
│                                      │ Expected evidence: ActionExecuted     │
│                                      │ Current recommendation confidence .78 │
╰─ Enter preview  Shift-Enter dry-run  Esc close ───────────────────────────────╯
```

Palette entries include:

- navigation destinations,
- context actions,
- recent actions,
- disabled actions with reasons,
- entity deep links,
- saved filters/lenses,
- generated “why?” explainers.

---

## 5. Visual language: incredible but honest

### 5.1 Terminal capability tiers

Support three rendering tiers:

1. **Truecolor rich** — RGB palette, Unicode boxes/glyphs, subtle animation.
2. **256-color fallback** — nearest palette mapping, normal Unicode.
3. **ASCII-safe** — no special glyphs, no reliance on color alone.

Configuration overrides:

```toml
[tui]
color = "auto"          # auto | truecolor | 256 | 16 | none
glyphs = "auto"         # auto | unicode | ascii
motion = "auto"         # auto | rich | reduced | none
fps = 12                # default draw target; coalesce faster events
high_contrast = false
colorblind_safe = true
```

### 5.2 Semantic color palette

| Token | Truecolor | 256 fallback | 16-color fallback | Meaning |
|---|---:|---:|---|---|
| `bg` | `#070A0F` | 232 | black | Base background. |
| `panel` | `#0D1117` | 233 | black | Panel background. |
| `panel.hot` | `#160B0B` | 52 | red bg | Critical panel. |
| `border` | `#273241` | 238 | bright black | Normal border. |
| `border.focus` | `#82AAFF` | 111 | bright blue | Focused pane. |
| `text` | `#D8DEE9` | 252 | white | Normal text. |
| `muted` | `#7D8590` | 245 | bright black | Secondary text. |
| `success` | `#7EE787` | 120 | bright green | Passed/healthy/verified. |
| `warn` | `#F2CC60` | 221 | yellow | Warning/degraded. |
| `danger` | `#FF6B6B` | 203 | bright red | Failure/blocked. |
| `info` | `#58A6FF` | 75 | bright blue | Running/informational. |
| `cyan` | `#39D0D8` | 80 | cyan | Network/cache/data. |
| `purple` | `#C792EA` | 177 | magenta | Agents/autonomy. |
| `orange` | `#FFB86B` | 215 | yellow | Queue pressure/capacity. |
| `pink` | `#FF79C6` | 212 | magenta | Security/secrets. |
| `artifact` | `#B7E3A1` | 150 | green | Signed artifacts/provenance. |
| `stale` | `#6E7681` | 243 | bright black | Stale/degraded data. |

Never encode state by color alone. Pair color with glyph/text.

### 5.3 Glyphs and ASCII fallback

| State/domain | Rich glyph | ASCII fallback |
|---|---|---|
| Running | `●` | `RUN` |
| Queued/waiting | `○` / `⏳` | `WAIT` |
| Success | `✓` | `OK` |
| Failure | `✗` | `FAIL` |
| Warning | `⚠` / `!` | `WARN` |
| Paused/manual | `⏸` | `PAUSE` |
| Retrying/reconciling | `⟳` | `RETRY` |
| Hot/fast path | `⚡` | `HOT` |
| Release/artifact | `◆` | `ART` |
| Runner/pool | `⬢` | `RUNNER` |
| Cache | `⌁` | `CACHE` |
| Security/trust | `🛡` | `SEC` |
| Agent/autonomy | `A` | `AGENT` |
| VTI | `V` | `VTI` |
| Jankurai | `J` | `JANK` |
| Secret/locked | `🔒` | `LOCK` |
| Stale | `…` | `STALE` |

### 5.4 Motion design

The user asked for **incredible moving activity in realtime**. Implement motion as meaningful state, not decoration:

- Running jobs pulse every draw tick with a low-intensity moving dot.
- Queues animate with a conveyor strip only when queue length changes.
- Live logs show a streaming cursor and byte offset.
- Critical path edges shimmer subtly when the graph is focused.
- Event tail highlights new events for 1–2 seconds.
- Saturation meters have moving fill only if source freshness is live.
- Incident state uses strong static framing plus optional pulse.
- Reduced-motion mode disables all nonessential animation.

Animation never changes the displayed numeric value. It only communicates freshness/activity.

---

## 6. Responsive layout rules

### 6.1 Width breakpoints

| Width | Mode | Layout |
|---:|---|---|
| `<80` | emergency | one pane at a time; breadcrumbs and command palette remain usable. |
| `80–119` | compact | two panes max; inspector as modal. |
| `120–159` | normal | three-pane layouts; dense tables. |
| `160–219` | wide | left nav + center matrix/graph + right inspector + bottom event tail. |
| `>=220` | command center | all panes, minimap, timeline, and side inspectors. |

### 6.2 Height breakpoints

| Height | Behavior |
|---:|---|
| `<28` | hide decorative rows, collapse event tail to one line. |
| `28–39` | compact panels, single-line detail summaries. |
| `40–59` | default. |
| `>=60` | extra logs/timeline/history visible. |

### 6.3 Pane chrome

Every pane title includes:

```text
[*] Pane title  filter=...  sort=...  source=db+gitlab+docker  age=1.2s  cursor=1849912
```

`[*]` marks focus. `[esc]` marks drill mode. `STALE 4m` replaces `age` if outside freshness budget.

---

## 7. Global screen: Fleet Mission Control

### 7.1 Purpose

The Global screen answers in under five seconds:

- Are all repos and repo families healthy?
- What is running, queued, failing, blocked, stale, or unsafe?
- What is the top blocker?
- How close are we to runner/core/memory/cache limits?
- What should I do next?
- Is it safe to code, merge, and release?

### 7.2 Wide mock

```text
╭─ JeRyu Flight Deck ─ runtime sqlite+kafka ─ gitlab 31ms ─ db 4ms ─ docker ✓ ─ vault ✓ ─ cache 92% ─ age 0.8s ╮
│ SAFE: code ✓  merge ?  release ✗     queue 31/48 slots  safe sat 65%  burst 82%  headroom 17  stale none     │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Tabs: g0 Global  gq Queue  gr Repos  gw Workflow  gu Runners  gc Cache  gv VTI  ga Agents  gb Bugs  gR Release│
├──────────────────────────────┬──────────────────────────────────────────────────────┬─────────────────────────┤
│ [*] Fleet pulse              │ Live work across all repos                           │ Attention / next action │
│                              │                                                      │                         │
│ OPEN REPOS        41         │ veox-core       ●●●●○○  4 run 2 q  71%  ETA 9m      │ 1 ✗ veox-db release     │
│ FAMILIES           7         │ veox-ui         ●●○     2 run 1 q  43%  ETA 4m      │   prod gate failed      │
│ RUNNING JOBS      31         │ redline-db      ✗●○○    1 fail 1 run 2 q            │   evidence gate#882     │
│ QUEUED JOBS       17         │ jankurai        ✓✓●     1 run      82%  ETA 2m      │   action explain        │
│ BLOCKED            4         │ isolated/foo    ○○      2 q       wait 5m           │                         │
│ FAILED             3         │                                                      │ 2 ⚠ cache 84% full      │
│ AGENTS          12/16        │ Critical path: redline-db > nextest > e2e            │   reclaim 121 GiB       │
│ CANARY             1         │ Bottleneck: pool=build queue p95 6m                  │   action gc preview     │
│ PROD               8         │ Wasted compute: 42m superseded today                 │                         │
│                              │ VTI saved: 11.3h / miss rate 0.7%                    │ NEXT: rerun gate#882    │
├──────────────────────────────┴──────────────────────────────────────────────────────┴─────────────────────────┤
│ Runner capacity: trusted 18/24 build 9/16 untrusted 4/8 remote-west 5/8  | Cache: crates 212G target 88G │
├───────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Event tail: [12:01:03] job.failed redline-db#530 nextest-shard-7 [capsule ready] [enter drill]               │
╰─ ↑↓←→ focus  Enter drill  Esc up  / filter  Ctrl-K commands  b blocker  x explain  ? help ───────────────────╯
```

### 7.3 Panes

**Header posture bar** shows runtime profile, GitLab/DB/Docker/Vault/broker/MCP/cache health, event freshness, queue saturation, and safe-to-code/merge/release flags.

**Fleet pulse** shows repo count, family count, running/queued/blocked/failed jobs, active/blocked agents, open bugs by severity, release/canary/prod posture, active security findings, artifact signing failures, VTI savings, cache pressure, and queue headroom.

**Live work** is a severity-ranked cross-repo list sorted by:

1. production/security incident,
2. release blocker,
3. failed required job,
4. blocked agent,
5. stale running job,
6. queued job beyond SLO,
7. low-confidence VTI skip,
8. Jankurai/security regression,
9. normal running work.

**Attention/next action** shows `AttentionItem` plus `NextActionRecommendation`. Every item is drillable and has evidence.

**Runner/cache bottom strip** is always visible on Global, Queue, Workflow, and Release because the user explicitly cares about core/memory/cache limits.

---

## 8. Queue / Capacity screen: theoretical limit and runner decisions

### 8.1 Purpose

This is the screen for: **“How close are we to the theoretical limit, and should we increase runner count?”**

It must not show “96 runners” as a fake capacity number. It must show the **capacity frontier**: the minimum of runner slots, GitLab dispatch limits, tag eligibility, CPU, memory, disk I/O, cache bandwidth, network, locks, and policy gates.

### 8.2 Capacity mock

```text
╭─ Queue / Capacity Frontier ───────────────────────────────────────────────────────────────────────────────────╮
│ Fleet saturation safe 65%  burst 82%  headroom 17 weighted slots  queue p50 44s p95 6m12s  dispatch lag 1.2s │
├────────────────────────────┬───────────────────────────────┬────────────────────────────────────────────────┤
│ [*] Pools                  │ Theoretical limit              │ Queue by repo/stage                            │
│ trusted     18/24 slots    │ configured slots        48     │ veox-core     test       8 q  p95 3m           │
│ build        9/16 slots ⚠  │ healthy slots           45     │ veox-db       build      4 q  p95 8m ⚠         │
│ untrusted    4/8 slots     │ GitLab dispatch         43     │ redline-db    nextest    3 q  p95 11m ⚠        │
│ remote-a     5/8 slots     │ CPU-bound slots         41     │ jeryu         security   1 q  p95 1m           │
│ paused       0/4 slots     │ memory-bound slots      52     │                                                │
│ draining     2 managers    │ disk-IO-bound slots     37 ⚠   │ Bottleneck: build pool disk IO                │
│ unhealthy    1 manager     │ cache-bound slots       44     │ Suggest: +2 build on remote-a OR GC node-b     │
│                             │ SAFE LIMIT              37     │                                                │
│                             │ current weighted        24     │                                                │
├────────────────────────────┴───────────────────────────────┴────────────────────────────────────────────────┤
│ Critical waiting: redline-db#884 nextest shard 7 [11m]  veox-db#530 image-build [8m]                         │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Timeline 30m: running ████████████████░░░ queued ░░░░░░ wait ███ failures ✗✗ superseded ≋≋≋                 │
╰─ Enter drill  s scale preview  d drain  p pause  w what-if  x why waiting  / filter ─────────────────────────╯
```

### 8.3 Required metrics

Collect per pool, node, tag bucket, job kind, and repo family:

| Metric | Source | Confidence |
|---|---|---|
| configured runner slots | pool/manager config | `CONF` |
| healthy slots | manager health, runner contacted age, Docker events | `MEAS` |
| GitLab dispatch/request limits | GitLab runner config/API if available | `CONF/MEAS` |
| queued/running by tag | webhooks + GitLab reconciliation | `MEAS` |
| arrival rate | event history | `HIST` |
| service rate | completed jobs per interval | `HIST` |
| p50/p95 queue wait | `job_events`, GitLab queued duration | `HIST/MEAS` |
| p50/p95 duration by job kind | `ci_job_runs`, GitLab job durations | `HIST` |
| per-job CPU/mem/disk/network | cgroup/Docker stats; fallback job-kind hints | `MEAS/HEUR` |
| node CPU/mem/disk/network | host/remote telemetry | `MEAS` |
| cache throughput/hit/miss | SmartCache metrics | `MEAS` |
| cache storage category/reclaimable | cache object/category scans | `MEAS` |
| stale runners/managers | runner manager contacted age | `MEAS` |
| tag mismatch rejects | scheduling diagnosis | `MEAS/HEUR` |
| lock/policy gates | release/admission/evidence state | `MEAS` |

### 8.4 Formula: safe capacity frontier

Compute by constraint bucket. A bucket is usually a normalized tag set or pool kind: `build`, `trusted`, `untrusted`, `postgres`, `security`, `release`, `gpu`, etc.

```text
configured_slots(bucket) = Σ(manager.runner_concurrency)
healthy_slots(bucket) = slots where manager healthy && runner contacted_recently && !paused && !draining
eligible_slots(bucket) = healthy slots matching job tags/trust/security/privilege requirements
gitlab_slots(bucket) = min(eligible_slots, GitLab runner concurrency/request_concurrency constraints)

cpu_slots(node, job_kind) = floor((usable_cpu_cores - reserved_cpu_cores) / p95_cpu_cores_per_job_kind)
mem_slots(node, job_kind) = floor((usable_mem_gib - reserved_mem_gib) / p95_mem_gib_per_job_kind)
disk_io_slots(node, job_kind) = floor(observed_disk_iops_capacity / p95_iops_per_job_kind)
cache_slots(bucket, job_kind) = floor(cache_gateway_sustained_mbps / p95_cache_mbps_per_job_kind)
network_slots(node, job_kind) = floor(network_sustained_mbps / p95_network_mbps_per_job_kind)

safe_limit(bucket) = min(gitlab_slots, cpu_slots, mem_slots, disk_io_slots, cache_slots, network_slots)
burst_limit(bucket) = min(configured_slots, cpu_slots * burst_factor, mem_slots * burst_factor)
weighted_running(bucket) = Σ(job.weight_by_kind)
headroom(bucket) = safe_limit - weighted_running
saturation(bucket) = weighted_running / max(safe_limit, 1)

arrival_rate(bucket) = jobs_created_per_minute over rolling window
service_rate(bucket) = jobs_completed_per_minute over rolling window
traffic_rho(bucket) = arrival_rate / max(service_rate, epsilon)
drain_eta(bucket) = queued_jobs / max(service_rate, epsilon)
queue_pressure = weighted(saturation, traffic_rho, oldest_wait, p95_wait, critical_jobs_waiting, blocked_main_jobs)
```

Each number displays confidence:

| Label | Meaning |
|---|---|
| `MEAS` | Direct measured telemetry. |
| `HIST` | Historical inference from previous jobs. |
| `CONF` | Configured value only. |
| `HEUR` | Heuristic fallback. |
| `MISS` | Missing data; do not automate scaling from it. |

### 8.5 Should we increase runner count?

The archive does **not** include live CPU/memory/runner telemetry from a running instance, so Flight Deck must not make a global yes/no decision from static docs. It should make a per-pool recommendation with proof.

**Default operational stance:** do **not** globally increase runner count just because jobs are queued. Increase only the constrained pool when the bottleneck is proven to be runner slots and host resources have headroom.

Recommend **scale up** only when all are true:

1. `saturation >= 0.85` for the affected pool for at least the configured window, e.g. 10–15 minutes.
2. Queue p95 exceeds SLO or oldest critical job wait exceeds SLO.
3. Dominant bottleneck is `RunnerSlots` or `WarmManagers`, not CPU, memory, disk I/O, cache bandwidth, GitLab dispatch, tags, locks, approvals, or release gates.
4. Candidate node has CPU/memory/disk/cache/network headroom after adding projected jobs.
5. Recent OOM/container death rate is below threshold.
6. Tag eligibility improves for the queued jobs; adding generic runners does not help if the jobs require missing tags.
7. Scale-up cost/budget policy allows it.
8. Simulation predicts meaningful p95 queue reduction.

Recommend **do not add runners yet** when:

- CPU > 85–90% or load average is already above available cores.
- Memory > 80–85%, swap is active, or Docker OOM occurred recently.
- Disk > 85–90% or IO wait is the safe-limit bottleneck.
- Cache is near full or miss storms dominate job time.
- GitLab dispatch lag is high while slots are idle.
- Tag mismatch prevents jobs from using available slots.
- Jobs are blocked on locks, manual release gates, signatures, security findings, or approvals.
- Superseded pipelines are consuming slots.
- VTI is unnecessarily escalating full suites due to low confidence.

Scale recommendation pseudocode:

```rust
fn recommend_scale(pool: &PoolCapacityView) -> ScaleRecommendation {
    if pool.queue.p95_wait <= pool.slo.p95_wait && pool.oldest_critical_wait <= pool.slo.critical_wait {
        return ScaleRecommendation::NoAction("queue within SLO");
    }
    if pool.safe_limit.limiting_factor != LimitingFactor::RunnerSlots &&
       pool.safe_limit.limiting_factor != LimitingFactor::WarmManagers {
        return ScaleRecommendation::FixBottleneck(pool.safe_limit.limiting_factor);
    }
    if !pool.candidate_nodes.iter().any(|n| n.has_projected_headroom(pool.projected_job_mix())) {
        return ScaleRecommendation::NoScale("no node has CPU/memory/disk/cache headroom");
    }
    if pool.tag_mismatch_rate > 0.15 {
        return ScaleRecommendation::FixTags("queued jobs cannot use generic added runners");
    }
    if pool.recent_ooms > 0 || pool.memory_pressure.is_red() {
        return ScaleRecommendation::NoScale("memory/OOM risk");
    }
    pool.simulate_add_slots(vec![1, 2, 4, 8]).best_by_queue_reduction_per_cost()
}
```

### 8.6 Core/memory pressure thresholds

Make thresholds configurable, but use these defaults:

| Resource | Yellow | Red | Action |
|---|---:|---:|---|
| CPU utilization | `>75%` sustained | `>90%` sustained | Add runners only if jobs are not CPU-bound or new node has headroom. |
| Load per core | `>0.8` | `>1.2` | Treat as CPU-bound. |
| Memory utilization | `>75%` | `>85%` | Avoid scale-up on same node; inspect p95 job memory. |
| Swap in/out | any sustained | high sustained | Stop scaling; investigate memory. |
| Docker OOMs | `>=1/day` | `>=2/day` | Drain/restart/bisect before scaling. |
| Disk usage | `>80%` | `>90%` | GC/cache reclaim before adding. |
| Disk IO wait | `>10%` | `>20%` | Treat as disk-bound. |
| Cache storage | `>80%` | `>90%` | Run GC preview and category cleanup. |
| Cache hit ratio | `<80%` | `<60%` | Investigate miss storms before adding runners. |
| GitLab dispatch lag | `>30s` | `>120s` | Adding runners may not help. |
| Runner contacted age | `>2m` | `>5m` | Mark manager stale/unhealthy. |

### 8.7 What-if simulator

The Queue page must include a what-if panel:

```text
WHAT-IF candidates
1. +2 build runners on remote-a         p95 wait 8m → 4m   cost +$0.21/h  risk low  confidence .78
2. GC node-b cache 121GiB               safe disk slots 37 → 44  cost 0   risk med  confidence .71
3. Cancel superseded pipelines          free 6 slots now   cost 0         risk low  confidence .92
4. Fix tag postgres on remote-a         eligible slots 5 → 9              risk low  confidence .83
5. Raise VTI confidence docs-only path  skip 9 jobs/run                 risk med  confidence .66
```

Every candidate drills into evidence and preconditions.

---

## 9. Repos / Families screen

### 9.1 Purpose

You have many repos, some grouped by prefix/family (`veox-*`, `redline-*`) and some isolated. This screen is the map of the empire.

### 9.2 Family matrix mock

```text
╭─ Repos / Families ─────────────────────────────────────────────────────────────────────────────────────────────╮
│ grouping prefix  filter all  sort attention  families 7  repos 41  stale 2                                    │
├────────────────────────┬──────────────────────────────────────────────────────────────────────────────────────┤
│ [*] Families           │ Repo matrix                                                                          │
│ ▶ veox-*       18  ⚠3  │ repo          CI        VTI       Jankurai      Git       Bugs   Sec  Rel     Ship   │
│   redline-*     5  ✗1  │ veox-core     ● 71%     83% saved  91 ↑ +2      sync ✓    12/3   ✓    canary  82     │
│   jeryu         1  ✓   │ veox-ui       ○ queued  61% saved  86 ↓ -4 ⚠    drift 2   4/1    ⚠    none    64     │
│   jankurai      1  ✓   │ veox-db       ✗ gate    22% saved  78 cap ✗     PR fail   9/2    ✗    block   31     │
│   isolated     16  ⚠2  │ veox-deploy   ✓         n/a        89 →         sync ✓    2/0    ✓    prod    94     │
├────────────────────────┴──────────────────────────────────────────────────────────────────────────────────────┤
│ Family rollup: veox-* jobs 23 run / 11 q / 2 fail | critical path veox-db#530 | release blocked               │
╰─ Enter family/repo  Space expand  / filter  j jankurai  g git-sync  b bugs  s security  x explain ───────────╯
```

### 9.3 Repo row fields

Each row includes alias/slug/path, family, provider/project ID, default branch, CI status, current pipeline progress/ETA, last successful main, last merge attempt, Git drift, VTI savings/misses, cache usage/hit rate, Jankurai score/trend/version/caps, bug counts, agent sessions, security findings, artifact signature status, release/canary/prod version, churn, ship score, and freshness.

### 9.4 Ship score

A repo-level ship score is acceptable only if fully explainable:

```text
ship_score = weighted(
  ci_green, required_jobs_done, vti_confidence, cache_trust,
  jankurai_score, security_posture, artifact_signature,
  release_gate, git_sync, open_critical_bugs, freshness
)
```

The score is a drill target, not a black box.

---

## 10. Workflow Atlas: repo/PR/pipeline/bug/release DAG

### 10.1 Purpose

This is the “what exactly is running?” screen. It must support scopes:

- repo current pipeline,
- repo family queue,
- branch/ref,
- MR/PR,
- pipeline/child pipeline,
- bug attempt,
- agent patch race,
- release candidate/canary/prod,
- autonomous workflow.

### 10.2 Canonical phase DAG

```text
Admission → Impact/VTI → Build → Unit → Integration → Security → Package → Artifact Sign → Release Gate → Canary → Prod → Monitor/Rollback
```

For MRs/PRs:

```text
Pre-merge CI → Agent review pre → Merge gate → Auto-merge → Post-merge CI → Agent review post
```

For bug attempts:

```text
Bug accepted → Agent assigned → Branch created → Patch proposed → CI → Review → Verified → Done
```

For patch races, show one lane per hypothesis branch.

### 10.3 Workflow mock

```text
╭─ Workflow veox-core / MR !1842 / pipeline #530 ────────────────────────────────────────────────────────────────╮
│ status RUNNING  progress 71% MEAS  ETA 9m HIST  blocker nextest shard 7  critical path test→security→gate     │
├─ PR rail ──────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ [✗ !1842 cache race] [● !1841 auth refactor] [○ !1839 UI] [✓ !1837 deps]                                      │
├──────────────┬───────────────────────────────────────────────────────────────┬────────────────────────────────┤
│ Phase rail   │ [*] DAG canvas                                                │ Inspector                      │
│ Admission ✓  │  Admission       Build          Tests            Security     │ Job: nextest-shard-7           │
│ Impact    ✓  │  ┌─────────┐     ┌─────────┐    ┌──────────┐     ┌────────┐  │ status: RUNNING               │
│ Build     ✓  │  │ hook ✓  │───▶ │ image ✓ │──▶ │ unit ✓    │──▶ │ sast ○ │  │ progress: 43% MEAS            │
│ Unit      ✓  │  └─────────┘     └─────────┘    ├──────────┤     └────────┘  │ elapsed: 11m                  │
│ Integration● │                                  │ nextest ● │◀[SEL][CRIT]    │ runner: build-3                │
│ Security  ○  │                                  │ 43% 11m  │                 │ pool: build                   │
│ Package   ·  │                                  ├──────────┤                 │ cache: miss storm ⚠            │
│ Sign      ·  │                                  │ e2e ○    │                 │ logs: 14k lines live           │
│ Canary    ·  │                                  └──────────┘                 │ actions: retry/cancel/explain  │
├──────────────┴───────────────────────────────────────────────────────────────┴────────────────────────────────┤
│ Activity/logs [live follow] cargo nextest run --profile ci ...                                                 │
│   test cache::race::reuses_singleflight ... ok                                                                 │
│   test pool::drain::keeps_grant ... running t=681s                                                             │
╰─ arrows node  Enter drill  l logs  e evidence  x explain  b blocker  c critical  r retry ─────────────────────╯
```

### 10.4 Node card requirements

A node card shows, when space permits:

- glyph/status,
- label,
- progress percent and confidence,
- elapsed and ETA,
- queue wait,
- required/optional/manual marker,
- critical path marker,
- blocker count,
- VTI/cache/security/signed indicators,
- runner/pool/tag,
- stale age,
- action availability marker.

Dense mode:

```text
[● nextest 43% 11m CRIT]
```

### 10.5 Inspector tabs

Every selected object has these tabs:

1. **Overview** — status, command, duration, queue wait, runner, pool, retry count, expected artifacts.
2. **Logs** — live trace, annotations, search, follow/manual scroll.
3. **Evidence** — capsule, VTI plan, cache verdict, scan, signature, admission, grant.
4. **Deps** — upstream/downstream DAG, blockers, produced/consumed artifacts.
5. **Actions** — context actions with risk previews.
6. **Raw** — raw JSON/source fields.

### 10.6 Graph edge computation

Edges come from:

- GitLab `needs`/DAG where available,
- stage order fallback,
- bridges/child pipelines,
- artifact dependencies,
- release-gate dependencies,
- VTI decisions,
- cache dependencies,
- agent task relationships,
- bug attempt relationships,
- heuristic stage/name classifier fallback.

```rust
pub enum WorkflowEdgeKind {
    Needs,
    StageOrder,
    Bridge,
    Artifact,
    ReleaseGate,
    SecurityGate,
    VtiDecision,
    CacheDependency,
    AgentIntent,
    BugAttempt,
    Heuristic,
}
```

Heuristic edges are dashed/lighter and labelled `HEUR`.

### 10.7 Live logs/traces

Target:

```text
TUI ──subscribe──▶ /api/ws/logs?project_id=48&job_id=530&cursor=last
TUI ◀─chunks────── JobLogChunk {{ seq, offset, bytes, annotations, eof, stale_after_ms }}
```

Fallback:

- current GitLab trace polling around the existing cadence when stream unavailable,
- title shows `[poll]`,
- bounded ring buffer/rope,
- offset resume when stream comes back,
- redaction before render.

---

## 11. Runners / System screen

### 11.1 Purpose

This screen answers the system utilization part of the user’s request:

- Are runners healthy?
- Are we using all available runners?
- Can we add more without hitting core/memory/disk limits?
- Which remote nodes have spare capacity?
- Which managers are stale, OOMing, draining, or not contacted?
- Are we wasting idle capacity?

### 11.2 Mock

```text
╭─ Runners / System Utilization ─────────────────────────────────────────────────────────────────────────────────╮
│ nodes 5  managers 32  slots 48  used 31  unhealthy 1  OOM 2 today  scale recommendation +2 build             │
├──────────────────────────────┬─────────────────────────────────────────────┬────────────────────────────────┤
│ [*] Pools                    │ Nodes                                       │ Manager logs/events            │
│ trusted    18/24  p95 wait 1m│ local       cpu 72% mem 64% disk 81%        │ mgr-build-3 OOM 09:11          │
│ build       9/16  p95 wait 8m│ remote-a    cpu 41% mem 38% disk 52%        │ mgr-build-7 restarted          │
│ untrusted   4/8   p95 wait 2m│ remote-b    cpu 89% mem 75% disk 93% ⚠      │ mgr-trust-2 contact old        │
│ privileged  0/4   paused     │ remote-c    unreachable 4m ✗               │                                │
╰─ Enter pool/node/manager  s scale preview  d drain  p pause  l logs  h host doctor  x why ───────────────────╯
```

### 11.3 Data to add

- Per-node CPU/memory/disk/network samples.
- Per-container CPU/memory/network/block IO.
- Docker daemon health.
- Image pull latency.
- Runner version/config hash.
- Runner contacted age.
- Remote node heartbeat/SSH latency.
- Node unreachable history.
- GC actions and reclaimed bytes.
- Scheduling rejects by tag mismatch.
- Per-job resource hints or observed p95 resource consumption.

---

## 12. Cache Observatory

### 12.1 Purpose

The Cache screen answers:

- Are we full?
- What categories are taking space?
- What is safe to reclaim?
- Is cache helping or hurting?
- Are there taints/trust problems?
- Is singleflight working?
- Are Rust crates, Cargo target dirs, sccache, nextest extracts, OCI layers, npm, git, logs, and artifacts under control?

### 12.2 Mock

```text
╭─ Cache / SmartCache ───────────────────────────────────────────────────────────────────────────────────────────╮
│ total 407G / 500G 81%  reclaimable 121G  hit 92.4%  miss 1,203  taints 2  full in ~3.4d                       │
├──────────────────────────────┬──────────────────────────────┬────────────────────────────────────────────────┤
│ [*] Storage by category      │ Gateway / singleflight        │ Trust / taint / GC plan                         │
│ Rust crates        212G 52%  │ proxy ONLINE  p95 41ms        │ active taints: 2                                │
│ Cargo target        88G 22%  │ registry ONLINE               │ denied cache verdicts: 7                        │
│ sccache             31G  8%  │ CA mounted yes                │ cold downgrades: 3                              │
│ OCI layers          26G  6%  │ coalesced 14,882              │ detonation breaches: 0                          │
│ nextest extracts    18G  4%  │ saved est 72.1G               │ protected leases: 312                           │
│ artifacts/logs      12G  3%  │ upstream errors 2             │ GC preview: reclaim 121G safe                   │
│ npm/git/other       20G  5%  │ miss storm redline-db ⚠       │ action: run gc --dry-run                        │
├──────────────────────────────┴──────────────────────────────┴────────────────────────────────────────────────┤
│ Hot objects: crate tokio 982 hits · nextest archive 221 hits · OCI rust image 77 hits                         │
╰─ Enter category/object  g GC preview  t taints  h hot  m miss storm  e evidence  / filter ───────────────────╯
```

### 12.3 Cache category model

```rust
pub struct CacheCategoryUsage {
    pub category: CacheCategory,
    pub bytes: u64,
    pub object_count: u64,
    pub hit_count_24h: u64,
    pub miss_count_24h: u64,
    pub reclaimable_bytes: u64,
    pub protected_bytes: u64,
    pub oldest_access_age_secs: u64,
    pub newest_access_age_secs: u64,
}

pub enum CacheCategory {
    RustCrates,
    CargoSparseIndex,
    CargoTarget,
    Sccache,
    NextestExtract,
    CasBlob,
    OciLayer,
    NpmPackage,
    GitObject,
    Artifact,
    Log,
    Other,
}
```

### 12.4 Cache actions

- GC preview.
- GC execute with confirmation.
- Protect/unprotect namespace.
- Clear taint after evidence.
- Add/remove force refresh rule.
- Open cache object inspector.
- Explain miss storm.
- Show top wasted bytes.
- Show top saved bytes.
- Drill to repo/job causing cache pressure.

---

## 13. VTI / Smart Test Selection

### 13.1 Purpose

This screen proves whether VTI is working:

- selected vs skipped tests,
- selector misses,
- confidence distribution,
- saved wall-clock and runner minutes,
- false skip incidents,
- forced full-suite escalations,
- cache/test speed impact,
- stale or missing mappings.

### 13.2 Mock

```text
╭─ VTI / Smart Test Selection ───────────────────────────────────────────────────────────────────────────────────╮
│ saved today 11.3h  selected 2,144  skipped 18,920  miss rate 0.7%  low-confidence plans 3                     │
├────────────────────────────┬──────────────────────────────┬──────────────────────────────────────────────────┤
│ [*] Repo VTI health        │ Selected plan                 │ Selector misses / audits                          │
│ veox-core   83% saved  ✓   │ repo veox-core                │ 2026-05-26 cache::race missed by impact           │
│ veox-ui     61% saved  ⚠   │ base fa51a52 head b19d03      │ severity medium  repaired yes                     │
│ veox-db     22% saved  ⚠   │ confidence 0.91 high          │                                                    │
│ redline-db  74% saved  ✓   │ changed src/cache.rs          │ false skip risk LOW                               │
│ jeryu       55% saved  ✓   │ selected 42 skipped 311       │ audit command: jeryu test audit ...               │
│                            │ escalated: security touched   │                                                    │
├────────────────────────────┴──────────────────────────────┴──────────────────────────────────────────────────┤
│ Impact graph: src/cache.rs → cache unit/integration/security · skipped docs/ui-only tests                      │
╰─ Enter plan/test  m misses  a audit  l learn  x explain plan  / filter ───────────────────────────────────────╯
```

### 13.3 Guardrail

If VTI confidence is low, security/critical files changed, selector misses rose, or mapping is stale, the UI must show escalation to fuller test runs as a safety-positive event, not a failure.

---

## 14. Agents and Autonomy

### 14.1 Agents screen purpose

Answer:

- Which agents are working?
- What are they assigned to?
- Are they healthy or looping?
- What branches/MRs/bugs are they touching?
- What grants/budgets do they have?
- What logs/messages/artifacts are they producing?
- Can configs be edited safely?

### 14.2 Agents mock

```text
╭─ Agents ───────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ active 12  blocked 2  grants 9  budget today $3.82  kill-bell armed yes  autonomy paused no                  │
├──────────────────────────────┬─────────────────────────────────────────────┬────────────────────────────────┤
│ [*] Agent sessions           │ Current task / branch                       │ Logs / messages                 │
│ bot-cache-17 RUN  veox-core  │ bug BUG-219 fix cache race                  │ 12:03 propose patch             │
│ bot-ui-02    WAIT veox-ui    │ branch agent/cache-race-b19d                │ 12:04 run tests unit            │
│ bot-db-05    BLK  veox-db    │ MR !1842  pipeline #530 progress 71%        │ 12:05 selector miss?            │
│ bot-sec-01   RUN  redline    │ grant agent_task gnt_abc expires 18m        │ 12:06 waiting nextest           │
│                              │ risk high, no prod grant                    │                                │
├──────────────────────────────┴─────────────────────────────────────────────┴────────────────────────────────┤
│ Actions: view diff · view prompt hash · edit config · revoke grant · pause agent · merge preview · rerun      │
╰─ Enter agent  e evidence  g grants  c config  p pause preview  k kill-bell  / filter ─────────────────────────╯
```

### 14.3 Dedicated lifecycle tables to add

```text
agent_sessions
agent_tasks
agent_steps
agent_messages
agent_artifacts
agent_config_versions
agent_budget_events
```

```rust
pub struct AgentSessionView {
    pub id: String,
    pub agent_name: String,
    pub repo_slug: String,
    pub task_id: Option<String>,
    pub bug_id: Option<String>,
    pub branch: Option<String>,
    pub mr_iid: Option<i64>,
    pub pipeline_id: Option<i64>,
    pub status: AgentStatus,
    pub phase: AgentPhase,
    pub progress_pct: u16,
    pub current_step: Option<String>,
    pub grant_ids: Vec<String>,
    pub budget_used_micro_usd: i64,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub last_message_at: DateTime<Utc>,
    pub logs_cursor: u64,
    pub evidence_refs: Vec<String>,
}
```

### 14.4 Config editor rules

- Opens as safe modal or external `$EDITOR` only after preview.
- Shows config diff and risk.
- Saves versioned config.
- Active agents do not silently pick up changes unless config declares live reload.
- Editable fields include model/provider, budgets, allowed repos/families, allowed actions/risk tiers, concurrency, test policy, branch naming, secrets access policy, prompt template hash.

### 14.5 Autonomy governance

Show kill bell, freeze windows, active workflows, verdicts, launch ledger, foundry queue, release/passport flow, escalation results, Nightwatch/canary reviews, LLM reviewer/provider health.

```text
╭─ Autonomy / Governance ────────────────────────────────────────────────────────────────────────────────────────╮
│ kill-bell ARMED  paused no  freeze none  active verdicts 6  foundry queue 2  escalations 0 failed             │
├──────────────────────────────┬──────────────────────────────────────┬────────────────────────────────────────┤
│ [*] Workflows                │ Verdicts / passports                 │ Launch ledger                           │
│ auto-merge veox-core RUN     │ MR !1842 decision WAIT security      │ 12:01 WebhookReceived signed            │
│ release foundry WAIT         │ risk high head b19d03 policy fa55    │ 12:02 ReviewerStarted                   │
│ nightly audit RUN            │ expires 17m superseded no            │ 12:04 VerdictWritten                    │
│ rollback drill OK            │ passport missing artifact sig         │ 12:05 EscalationSkipped                 │
╰─ Enter workflow  k pause/resume  f freeze  v verdict  p passport  e evidence ─────────────────────────────────╯
```

---

## 15. Bugs / Issues cockpit

### 15.1 Purpose

A cross-repo accountability board:

- pending/ready/in progress/blocked/fix proposed/reviewing/verifying/done,
- owner/agent,
- attempts and failed attempts,
- branch/MR/pipeline/commit/evidence per attempt,
- external refs/sync status,
- reviewable historical completion.

### 15.2 Mock

```text
╭─ Bugs / Issues ────────────────────────────────────────────────────────────────────────────────────────────────╮
│ open 84  ready 21  in-progress 9  blocked 6  fix-proposed 7  done 112  security 3                            │
├──────────────────────────────┬─────────────────────────────────────────────┬────────────────────────────────┤
│ [*] Bug board                │ Selected bug                                │ Attempts / evidence             │
│ READY                        │ BUG-219 cache race in pool drain            │ #1 bot-cache failed             │
│  BUG-219 veox-core P1        │ repo veox-core component cache              │    CI #528 fail capsule         │
│  BUG-221 redline-db P0 SEC   │ severity high priority P1 difficulty med    │ #2 bot-cache running            │
│ IN PROGRESS                  │ owner bot-cache-17                          │    branch agent/...             │
│  BUG-208 veox-ui P2          │ acceptance no stale lease reuse             │    pipeline #530 71%            │
│ FIX PROPOSED                 │ current MR !1842                            │                                │
│  BUG-201 jeryu P1            │ status in_progress                           │ commits b19d03                  │
├──────────────────────────────┴─────────────────────────────────────────────┴────────────────────────────────┤
│ Filters: project=all status!=done severity>=medium agent=all sort=priority                                    │
╰─ Enter bug  n new  t triage  a assign agent  e evidence  c commits  p PR/MR  / filter ────────────────────────╯
```

### 15.3 Canonical statuses

```text
needs_triage, needs_info, accepted, ready, in_progress, blocked,
fix_proposed, reviewing, verifying, done, duplicate, invalid,
cannot_reproduce, wont_do
```

Attempt statuses:

```text
pending, started, failed, fix_proposed, verified, abandoned
```

---

## 16. Git Sync / Remote State

### 16.1 Purpose

Answer:

- Are local mirrors/shadows in sync with remote origin/main?
- Last successful merge to remote main?
- Last PR/MR attempt?
- Which repos have drift, push failure, rebase failure, branch protection denial, stale bot token, or missing webhook?
- Are signatures required and present?

### 16.2 Mock

```text
╭─ Git Sync / Remotes ───────────────────────────────────────────────────────────────────────────────────────────╮
│ repos 41  synced 34  drift 5  push failures 1  PR failures 2  unsigned commits 3                              │
├──────────────────────────────┬─────────────────────────────────────────────┬────────────────────────────────┤
│ [*] Repo sync table          │ Selected repo veox-core                     │ Recent git events              │
│ veox-core   sync ✓           │ origin git@gitlab:veox/core.git             │ 12:01 push main OK             │
│ veox-ui     behind 2 ⚠       │ local main fa51a52 remote main fa51a52      │ 12:03 branch create            │
│ veox-db     PR fail ✗        │ last merge 2026-05-26 11:52 by agent        │ 12:04 MR !1842 opened          │
│ redline-db  diverged ⚠       │ last PR attempt !1842 status running        │ 12:06 admission allow          │
│ isolated/a  unsigned ✗       │ mirror job healthy                          │                                │
╰─ Enter repo  p push/sync preview  m merge history  r rebase status  e evidence ───────────────────────────────╯
```

### 16.3 Add materialized views

- `remote_refs_snapshot` per repo/ref.
- `merge_attempts` normalized across GitLab/GitHub.
- `signature_status` per commit/tag.
- `branch_protection_snapshot`.
- `webhook_installation_status`.

---

## 17. Jankurai Audit Command Center

### 17.1 Purpose

Answer:

- Score per repo and trend.
- Installed/expected version.
- Stale/missing audits.
- Caps/hard gates.
- Duplicate code clusters.
- Hotspot files/functions.
- Merge/release blockers.
- Findings assignable to agents/bugs.

### 17.2 Mock

```text
╭─ Jankurai Audits ──────────────────────────────────────────────────────────────────────────────────────────────╮
│ fleet score avg 87.2  min 73  improving 21 repos  regressing 6  pinned version 1.5.1  stale audits 4          │
├──────────────────────────────┬─────────────────────────────────────────────┬────────────────────────────────┤
│ [*] Repo scores              │ Selected repo veox-db                       │ Findings                       │
│ jeryu        89 → v1.5.1     │ score 78 ↓ -6  threshold 85  FAIL           │ CAP complexity 14 files        │
│ veox-core    91 ↑ v1.5.1     │ last run 8m ago CI job #522                 │ DUP src/cache.rs x3            │
│ veox-ui      86 ↓ v1.5.1     │ hard caps duplicate code, complexity        │ SEC unsafe unwrap?             │
│ veox-db      78 ✗ v1.4.9 ⚠   │ duplicate blocks 7                          │ DOC stale generated            │
│ redline-db   84 ⚠ missing    │ version drift repo 1.4.9 host 1.5.1         │                                │
├──────────────────────────────┴─────────────────────────────────────────────┴────────────────────────────────┤
│ Trend 30d: 72 ▁▂▃▅▆▆▇ 78   Top duplicate cluster: src/cache.rs ↔ src/gateway/cache.rs                         │
╰─ Enter finding  u update  r rerun audit  d duplicates  c caps  a assign agent  / filter ─────────────────────╯
```

### 17.3 Tables to add

```text
jankurai_runs
jankurai_findings
jankurai_duplicate_clusters
jankurai_caps
jankurai_versions
```

---

## 18. Code Churn / Velocity Risk

Show commit volume, additions/deletions, hot files, agent-authored percentage, generated-code detection, dependency lockfile churn, churn-to-test ratio, correlations to CI failures, Jankurai regressions, bugs, selector misses, security findings.

```text
╭─ Code Churn ───────────────────────────────────────────────────────────────────────────────────────────────────╮
│ window 7d  commits 182  additions +128k  deletions -74k  net +54k  agent-authored 68%                         │
├──────────────────────────────┬─────────────────────────────────────────────┬────────────────────────────────┤
│ [*] Repo churn               │ Hot files/components                         │ Risk correlations              │
│ veox-core +12k/-5k           │ src/cache.rs       +1.9k/-822  fail x3       │ churn↑ jankurai↓ veox-db       │
│ veox-db   +31k/-19k ⚠        │ src/pool.rs        +1.1k/-402  bug x2        │ churn↑ selector misses         │
│ redline   +44k/-28k ⚠        │ crates/sql/src/*   +9.2k/-6k   sec x1        │ agent churn p95 high           │
╰─ Enter repo/file  a agents  j jankurai  b bugs  s security  / filter ────────────────────────────────────────╯
```

---

## 19. Security, Secrets, Artifacts, and Release

### 19.1 Security center

Normalize Vault health, secret authorities, secret audit, admission decisions, grants, policy violations, release gates, artifact signatures, cache taints/verdicts, Jankurai findings, and GitLab report artifacts.

```text
╭─ Security ─────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ posture WARN  critical 0 high 3 medium 12  secret leaks 0  policy violations 2  unsigned artifacts 1          │
├──────────────────────────────┬─────────────────────────────────────────────┬────────────────────────────────┤
│ [*] Findings                 │ Selected finding                            │ Proof / remediation            │
│ HIGH veox-db dep vuln        │ crate openssl 0.x advisory RUSTSEC-...      │ cargo deny report              │
│ HIGH redline unsafe cap      │ blocks release yes                          │ fix update openssl             │
│ MED veox-ui CSP missing      │ detected by scan job #884                   │ assigned bot-sec-01            │
│ POL agent grant denied       │ actor bot-db requested prod                 │ admission deny proof           │
╰─ Enter finding  e evidence  a assign  p policy  v vault  d deps  s scans ─────────────────────────────────────╯
```

Secret handling rules:

- Never render plaintext secret values.
- Show Vault address/mount/prefix, token fingerprint, expiry, rotation age, finalization status, audit event action/status/detail, release/version/target.
- Redact at backend and UI layers.
- Copy action refuses secret values.

### 19.2 Artifacts / Provenance

```text
╭─ Artifacts / Provenance ───────────────────────────────────────────────────────────────────────────────────────╮
│ artifacts 28  signed 27  unsigned 1  SBOM 26  provenance 25  release passports 8                             │
├──────────────────────────────┬─────────────────────────────────────────────┬────────────────────────────────┤
│ [*] Artifact list            │ Artifact detail                              │ Evidence chain                 │
│ ✓ veox-core:ci-fa51 image    │ digest sha256:abc...                         │ commit fa51a52 signed          │
│ ✗ veox-db:ci-b19d image      │ signature missing BLOCKS PROD                │ pipeline #530 71%              │
│ ✓ redline-db binary          │ SBOM path artifacts/sbom.json                │ tests pass 980/980             │
│ ✓ deploy bundle 1.2.0        │ provenance in-toto attestation ok            │ scans pass                     │
╰─ Enter artifact  v verify  s SBOM  p provenance  r release  e evidence ───────────────────────────────────────╯
```

### 19.3 Release / rollback

```text
╭─ Release / Production ─────────────────────────────────────────────────────────────────────────────────────────╮
│ train veox  current prod 1.2.3  canary 1.2.4  candidate 1.2.5  prod gate BLOCKED artifact signature           │
├──────────────────────────────┬─────────────────────────────────────────────┬────────────────────────────────┤
│ [*] Version rail             │ Gates                                       │ Rollback                       │
│ prod    1.2.3 ✓              │ upstream CI ✓                               │ target 1.2.3 verified          │
│ canary  1.2.4 ● 10%          │ e2e ✓                                       │ drill last passed 2d           │
│ cand    1.2.5 ✗ sig          │ telemetry wait                              │ rollback plan signed ✓         │
│ prev    1.2.2 ✓              │ artifact signature ✗                        │ action rollback preview        │
│                              │ security ✓                                  │                                │
├──────────────────────────────┴─────────────────────────────────────────────┴────────────────────────────────┤
│ Change volume +1.2k/-420 · bugs fixed 7 · security 1 · Jankurai +2 · VTI full run required                    │
╰─ Enter version/gate  p promote preview  r rollback preview  d dry-run  e evidence  n notes ───────────────────╯
```

All prod actions require typed confirmation, risk preview, evidence refs, identity/SHA binding, rollback target preview, grant/approval proof, and event log record.

---

## 20. Evidence / Proof Timeline

### 20.1 Purpose

Evidence is the spine. Every status and action must drill into proof.

```text
╭─ Evidence / Proof Timeline ────────────────────────────────────────────────────────────────────────────────────╮
│ filter entity=veox-core pipeline#530  events 184  capsules 3  grants 2  artifacts 4  signed ledgers 6         │
├──────────────────────────────┬─────────────────────────────────────────────┬────────────────────────────────┤
│ [*] Timeline                 │ Selected proof                               │ Related entities               │
│ 12:01 pipeline.created       │ job.failed capsule cap_882                   │ job #530                       │
│ 12:03 job.started            │ failure kind test_timeout                    │ pipeline #530                  │
│ 12:06 vti.plan.created       │ stage integration                            │ bug BUG-219                    │
│ 12:11 job.failed ✗           │ log tail hash sha256:...                     │ MR !1842                       │
│ 12:12 capsule.created        │ retry advice rerun shard                     │ agent bot-cache-17             │
│ 12:13 action.previewed       │ evidence path /.../cap_882.json              │                                │
╰─ Enter proof  / filter  y copy digest  r replay  raw raw JSON  a actions ─────────────────────────────────────╯
```

### 20.2 Timeline sources

Normalize:

- `events`,
- `evidence_capsules`,
- `retry_decisions`,
- `test_plans`, `test_plan_items`, `selector_misses`,
- `capability_intents`, `capability_grants`,
- `admission_decisions`,
- `git_command_events`, `git_ref_updates`, `git_mirror_jobs`, `git_risk_approvals`,
- `secret_audit_events`,
- `bug_events`, `bug_attempts`, `bug_evidence`,
- `release_attempts`, `foundry_candidates`, `verdicts`,
- `cache_taints`, `cache_verdicts`, `cache_promotions`,
- `llm_budget_ledger`,
- `launch_ledger`, `kill_bell_state`,
- Jankurai audit runs/findings,
- artifact signature/SBOM/provenance records,
- action preview/execution receipts.

### 20.3 Time travel / replay

Use event cursors to support:

- “show me what happened during the failure,”
- “compare current state to two hours ago,”
- “replay agent attempt,”
- “what changed before release went red?”

---

## 21. Runtime / API / MCP Doctor

### 21.1 Purpose

Operators and agents need to know what is actually enabled. Show:

- binary version, commit SHA, build time, dirty state, feature flags,
- DB backend/path/version,
- broker backend/lag,
- GitLab URL/readiness/latency,
- Docker health/managed containers,
- Vault address/health,
- MCP bind/protocol/session count,
- webhook bind/auth status,
- cache proxy/registry ports,
- release defaults,
- TUI refresh intervals,
- Jankurai version/expected version,
- redacted env/config health,
- missing credentials or disabled features,
- docs/source/action registry drift.

### 21.2 Deep health endpoint

Add:

```http
GET /api/health/deep
GET /api/runtime/profile
GET /api/settings/effective
```

Rules:

- Redact secrets by default.
- Include source of each setting: default, file, env, CLI.
- Include config drift warnings.
- Include generated-docs hash and source/action-registry hash.

---

## 22. Backend read model and API contract

### 22.1 Golden rule

Only adapters touch raw external systems. Renderers consume typed view models.

Bad:

```rust
fn draw_cache_tab(app: &App) {
    let rows = sqlx::query("SELECT ..."); // never inside renderer
}
```

Good:

```rust
fn draw_cache_tab(f: &mut Frame, area: Rect, view: &CacheDashboardView, focus: FocusState) {
    // pure rendering
}
```

### 22.2 Data plane architecture

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

### 22.3 HTTP endpoints to add

Use `/api` as the final contract. Existing or prior `/inspect` names can be thin aliases during migration.

```http
GET  /api/read-model
GET  /api/events?cursor=N&limit=500&kinds=&entity_kind=&entity_id=
GET  /api/entity/{kind}/{id}
POST /api/action/preview
POST /api/action/execute
GET  /api/proof?entity_kind=&entity_id=&since=&actor=&severity=&kind=&cursor=&limit=
GET  /api/proof/{proof_id}
GET  /api/runtime/profile
GET  /api/settings/effective
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
GET  /api/jankurai/dashboard
GET  /api/churn/dashboard
GET  /api/security/dashboard
GET  /api/artifacts/dashboard
GET  /api/release/dashboard
GET  /api/health/deep
```

Streaming:

```http
GET /api/events/stream?cursor=N                 # SSE
GET /api/ws/events                              # websocket
GET /api/ws/logs?project_id=&job_id=&cursor=    # websocket log chunks
GET /api/ws/entity/{kind}/{id}                  # entity-scoped updates
```

### 22.4 MCP resources to add

MCP tools are for actions. MCP resources are for inspection.

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
jeryu://jankurai/dashboard
jeryu://security/dashboard
jeryu://artifacts/dashboard
jeryu://release/latest
jeryu://jobs/{project_id}/{job_id}/trace
jeryu://pipelines/{project_id}/{pipeline_id}/jobs
jeryu://settings/effective
```

### 22.5 High-value plumbing backlog

1. Expose `TuiReadModel` and `TuiEvent` over HTTP/SSE/WebSocket and MCP resources.
2. Turn MR webhooks into first-class state.
3. Add MCP resources, not just tools.
4. Plumb GitLab test reports, coverage, code-quality, security artifacts, job `needs`, child pipelines.
5. Add bounded log/trace streams with cursor/offset resume.
6. Plumb Docker/container stats.
7. Deepen remote node telemetry.
8. Promote cache analytics from summary to full category/provenance/GC observability.
9. Add broker observability.
10. Add dedicated agent lifecycle tables.
11. Sync bug tracker external refs with GitHub/GitLab issues.
12. Add Vault/secret health without values.
13. Add Jankurai structured output.
14. Add OpenAPI/JSON Schema export.
15. Add Prometheus/OpenTelemetry exporter.
16. Normalize docs/manifests/action registry.

---

## 23. Rust implementation architecture

### 23.1 Recommended stack

- Rust application integrated with existing JeRyu binary.
- Ratatui renderer with Crossterm backend.
- Tokio tasks for data subscriptions, timers, streams, and action execution.
- `tracing` for app diagnostics.
- `serde`/`serde_json` for models and raw inspector payloads.
- Existing `sqlx`/state repo only inside data adapters, not renderers.
- TestBackend/golden snapshots for deterministic screen tests.

### 23.2 Source layout

```text
src/tui/
  mod.rs
  app.rs                         # App state, render entry, input dispatch
  focus.rs                       # macro/micro focus state
  theme.rs                       # palette, glyphs, terminal capability
  keymap.rs                      # key definitions and help
  command_palette.rs             # Ctrl-K, action preview
  routes.rs                      # navigation stack and deep links
  runtime/
    mod.rs
    event_bus.rs                 # EventBus, subscriptions, coalescing
    subscriptions.rs             # HTTP/SSE/WS/direct DB subscriptions
    reducer.rs                   # applies TuiEvent deltas to view cache
    action_client.rs             # preview/execute action API
    log_stream.rs                # websocket/poll fallback log chunks
    freshness.rs                 # stale/degraded source handling
    backpressure.rs              # event/drop/coalesce policy
  model/
    mod.rs
    fleet.rs
    repo.rs
    queue.rs
    workflow.rs
    runners.rs
    cache.rs
    vti.rs
    agents.rs
    autonomy.rs
    bugs.rs
    git_sync.rs
    jankurai.rs
    churn.rs
    security.rs
    artifacts.rs
    release.rs
    evidence.rs
    settings.rs
  screens/
    mod.rs
    global.rs
    queue.rs
    repos.rs
    workflow.rs
    runners.rs
    cache.rs
    vti.rs
    agents.rs
    autonomy.rs
    bugs.rs
    git_sync.rs
    jankurai.rs
    churn.rs
    security.rs
    artifacts.rs
    release.rs
    evidence.rs
    settings.rs
  widgets/
    mod.rs
    table.rs                     # virtualized selectable table
    sparkline.rs
    progress.rs
    status_strip.rs
    dag.rs
    minimap.rs
    inspector.rs
    event_tail.rs
    log_view.rs
    heatmap.rs
    breadcrumbs.rs
    modal.rs
    tabs.rs
    help.rs
  data/
    mod.rs
    client.rs                    # trait TuiDataClient
    local.rs                     # direct DB/GitLab/Docker fallback
    http.rs                      # /api client
    demo.rs                      # deterministic fixtures
    recording.rs                 # capture/replay sessions
  tests/
    fixtures.rs
    snapshots.rs
```

### 23.3 App state

```rust
pub struct App {
    pub route: RouteStack,
    pub focus: FocusState,
    pub theme: Theme,
    pub keymap: Keymap,
    pub views: ViewCache,
    pub subscriptions: SubscriptionState,
    pub command_palette: CommandPaletteState,
    pub modals: ModalStack,
    pub event_tail: EventTailState,
    pub log_state: LogPaneState,
    pub input_mode: InputMode,
    pub last_action: Option<ActionFeedback>,
    pub now: DateTime<Utc>,
}

pub enum InputMode {
    Root,
    Drill { pane_id: PaneId },
    Filter { query: String },
    CommandPalette,
    ConfirmAction { action_id: String },
    TextEdit { field_id: String, buffer: String },
    Help,
}
```

### 23.4 Data client trait

```rust
#[async_trait]
pub trait TuiDataClient: Send + Sync {
    async fn read_model(&self) -> anyhow::Result<TuiReadModel>;
    async fn entity(&self, kind: EntityKind, id: &str) -> anyhow::Result<EntityDetail>;
    async fn proof(&self, query: ProofQuery) -> anyhow::Result<ProofPage>;
    async fn preview_action(&self, req: ActionPreviewRequest) -> anyhow::Result<ActionPreview>;
    async fn execute_action(&self, req: ActionExecuteRequest) -> anyhow::Result<ActionResult>;
    async fn dashboard(&self, lens: DashboardLens, scope: Scope) -> anyhow::Result<DashboardPayload>;
    fn subscribe_events(&self, cursor: u64, filter: EventFilter) -> EventStream;
    fn subscribe_logs(&self, target: LogTarget) -> LogStream;
}
```

Implementations:

- `HttpDataClient` for `/api` + SSE/WS.
- `LocalDataClient` direct DB/GitLab/Docker fallback during migration.
- `DemoDataClient` deterministic rich fixtures.
- `RecordingDataClient` for replay tests and time travel.

### 23.5 View cache

```rust
pub struct ViewCache {
    pub global: Versioned<FleetDashboardView>,
    pub queue: Versioned<QueueDashboardView>,
    pub repos: Versioned<RepoBrowserView>,
    pub workflow: HashMap<WorkflowScope, Versioned<WorkflowDashboardView>>,
    pub runners: Versioned<RunnersDashboardView>,
    pub cache: Versioned<CacheDashboardView>,
    pub vti: Versioned<VtiDashboardView>,
    pub agents: Versioned<AgentsDashboardView>,
    pub autonomy: Versioned<AutonomyDashboardView>,
    pub bugs: Versioned<BugsDashboardView>,
    pub git_sync: Versioned<GitSyncDashboardView>,
    pub jankurai: Versioned<JankuraiDashboardView>,
    pub churn: Versioned<ChurnDashboardView>,
    pub security: Versioned<SecurityDashboardView>,
    pub artifacts: Versioned<ArtifactsDashboardView>,
    pub release: Versioned<ReleaseDashboardView>,
    pub evidence: Versioned<EvidenceDashboardView>,
}

pub struct Versioned<T> {
    pub value: T,
    pub generated_at: DateTime<Utc>,
    pub cursor: u64,
    pub stale: bool,
    pub source_health: SourceHealth,
}
```

### 23.6 Event loop

Draw is coalesced; network/DB never blocks rendering.

```rust
let tick_rate = Duration::from_millis(100);
loop {
    app.drain_events();
    app.advance_animations();

    if app.is_dirty() || app.should_heartbeat_draw() {
        terminal.draw(|f| screens::draw(f, &mut app))?;
        app.mark_clean();
    }

    if crossterm::event::poll(tick_rate)? {
        let ev = crossterm::event::read()?;
        input::handle(&mut app, ev).await?;
    }
}
```

Target draw cadence: 10–20 fps during active animation/log streams, lower when idle.

### 23.7 Reducer pattern

```rust
pub fn reduce_event(cache: &mut ViewCache, event: &TuiEvent) -> Vec<Invalidation> {
    match event.kind {
        TuiEventKind::JobStarted | TuiEventKind::JobUpdated | TuiEventKind::JobCompleted => {
            invalidate_queue(cache, event);
            invalidate_repo(cache, event);
            invalidate_workflow(cache, event);
        }
        TuiEventKind::CacheTaintCreated | TuiEventKind::CacheTaintCleared => {
            invalidate_cache(cache, event);
            invalidate_global_attention(cache, event);
        }
        TuiEventKind::ReleaseGateUpdated => {
            invalidate_release(cache, event);
            invalidate_global(cache, event);
        }
        TuiEventKind::AgentStepUpdated | TuiEventKind::BugAttemptUpdated => {
            invalidate_agents(cache, event);
            invalidate_bugs(cache, event);
            invalidate_workflow(cache, event);
        }
        _ => {}
    }
}
```

---

## 24. Core view model contracts

### 24.1 Fleet dashboard

```rust
pub struct FleetDashboardView {
    pub generated_at: DateTime<Utc>,
    pub event_cursor: u64,
    pub freshness: DataFreshness,
    pub runtime: RuntimeProfileSummary,
    pub mission: MissionSnapshot,
    pub families: Vec<RepoFamilySummary>,
    pub repos_attention: Vec<RepoAttentionSummary>,
    pub queue: QueueSummary,
    pub capacity: FleetCapacitySummary,
    pub cache: CacheSummary,
    pub vti: VtiFleetSummary,
    pub agents: AgentFleetSummary,
    pub bugs: BugFleetSummary,
    pub git_sync: GitSyncFleetSummary,
    pub jankurai: JankuraiFleetSummary,
    pub security: SecurityFleetSummary,
    pub artifacts: ArtifactFleetSummary,
    pub release: ReleaseFleetSummary,
    pub attention: Vec<AttentionItem>,
    pub next_action: Option<NextActionRecommendation>,
    pub recent_events: Vec<TuiEvent>,
}
```

### 24.2 Queue dashboard

```rust
pub struct QueueDashboardView {
    pub summary: QueueSummary,
    pub capacity: FleetCapacitySummary,
    pub pools: Vec<PoolCapacityView>,
    pub nodes: Vec<NodeCapacityView>,
    pub queued_by_repo_stage: Vec<QueueBucketView>,
    pub critical_waiting_jobs: Vec<JobQueueView>,
    pub bottlenecks: Vec<CapacityBottleneck>,
    pub timeline: Vec<QueueTimeslice>,
    pub recommendations: Vec<ScaleRecommendation>,
}
```

### 24.3 Repo overview

```rust
pub struct RepositoryOverviewView {
    pub repo: RepoIdentity,
    pub family: Option<String>,
    pub current_ref: String,
    pub head_sha: String,
    pub remote: GitRemoteState,
    pub ci: RepoCiSummary,
    pub workflows: Vec<WorkflowSummary>,
    pub vti: RepoVtiSummary,
    pub cache: RepoCacheSummary,
    pub agents: Vec<AgentSessionSummary>,
    pub bugs: RepoBugSummary,
    pub git_sync: RepoGitSyncSummary,
    pub jankurai: RepoJankuraiSummary,
    pub churn: RepoChurnSummary,
    pub security: RepoSecuritySummary,
    pub artifacts: RepoArtifactSummary,
    pub release: RepoReleaseSummary,
    pub attention: Vec<AttentionItem>,
    pub evidence_refs: Vec<String>,
}
```

### 24.4 Workflow dashboard

```rust
pub struct WorkflowDashboardView {
    pub scope: WorkflowScope,
    pub identity: WorkflowIdentity,
    pub status: WorkflowStatus,
    pub progress_pct: u16,
    pub progress_confidence: ProgressConfidence,
    pub eta: Option<EtaEstimate>,
    pub current_blocker: Option<EntityRef>,
    pub critical_path: Vec<EntityRef>,
    pub phases: Vec<WorkflowPhaseView>,
    pub graph: WorkflowGraphView,
    pub selected_entity: Option<EntityRef>,
    pub inspector: Option<EntityDetail>,
    pub live_log: Option<LiveLogView>,
    pub evidence_refs: Vec<String>,
    pub available_actions: Vec<ActionRef>,
}
```

### 24.5 Entity detail

```rust
pub struct EntityDetail {
    pub entity: EntityRef,
    pub title: String,
    pub state: String,
    pub summary: Vec<KeyValue>,
    pub timeline: Vec<TuiEvent>,
    pub blockers: Vec<BlockerSummary>,
    pub evidence: Vec<EvidenceRef>,
    pub related: Vec<EntityRef>,
    pub actions: Vec<ContextualAction>,
    pub risk: Option<RiskTier>,
    pub last_updated: DateTime<Utc>,
    pub stale: bool,
    pub raw: serde_json::Value,
}
```

### 24.6 Action model

```rust
pub struct ActionDescriptor {
    pub id: String,
    pub label: String,
    pub key_hint: Option<String>,
    pub surfaces: Vec<ActionSurface>,
    pub risk: RiskTier,
    pub side_effect: SideEffectClass,
    pub dry_run: DryRunSupport,
    pub required_grant: Option<GrantKind>,
    pub undo: UndoSupport,
    pub description: String,
}

pub struct ActionPreview {
    pub action: ActionDescriptor,
    pub target: EntityRef,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
    pub blast_radius: Vec<EntityRef>,
    pub preconditions: Vec<PreconditionStatus>,
    pub expected_evidence: Vec<EvidenceExpectation>,
    pub dry_run_result: Option<serde_json::Value>,
    pub confirmation: ConfirmationRequirement,
}
```

---

## 25. Action safety and mutation rules

### 25.1 Risk tiers

| Tier | Examples | UX |
|---|---|---|
| R0 read-only | open logs, inspect proof, list bugs | immediate. |
| R1 local reversible | pin, filter, save view, local bug triage | preview optional; event logged. |
| R2 CI capacity mutation | retry job, cancel superseded, scale pool, drain manager | preview required, dry-run where possible. |
| R3 code/repo mutation | propose patch, branch, commit, MR | preview + grant/evidence. |
| R4 merge/release/security | request merge, promote, rollback, clear taint, secrets | explicit typed confirmation + SHA/proof binding. |
| R5 production destructive | prod rollback execution, force delete, policy override | dual confirmation, proof not stale, audit receipt. |

### 25.2 Confirmation modal

```text
╭─ ACTION PREVIEW: rollback production ───────────────────────────────────────╮
│ Risk: R5 production mutation                                                │
│ Target: release train veox prod                                             │
│ Current prod: 1.2.4 sha b19d03                                              │
│ Rollback target: 1.2.3 sha fa51a52 verified 2d ago                          │
│ Preconditions: artifact sig ✓  rollback drill ✓  telemetry gate ✗ wait      │
│ Blast radius: veox-api, veox-ui, veox-deploy                                │
│ Evidence to create: action.previewed, rollback.requested, prod.state.changed │
│ Required confirmation: type ROLLBACK veox prod 1.2.3                         │
╰─ d dry-run  e evidence  Enter execute after phrase  Esc cancel ─────────────╯
```

### 25.3 Required hardening before broad actions

- Generate action metadata from one registry.
- Unit test that every mutating action is not marked read-only.
- Make `request_merge` evidence/SHA gated.
- Make allowed actions generated from registry.
- Show drift warnings in Runtime Doctor.
- Emit `ActionPreviewed`, `ActionExecuted`, `ActionFailed` events for every mutation.

---

## 26. Freshness, source health, and degraded behavior

### 26.1 Freshness model

```rust
pub struct DataFreshness {
    pub generated_at: DateTime<Utc>,
    pub cursor: u64,
    pub sources: Vec<SourceFreshness>,
}

pub struct SourceFreshness {
    pub source: SourceKind,
    pub age_ms: u64,
    pub status: SourceStatus,
    pub last_error: Option<String>,
}
```

### 26.2 Staleness behavior

| Situation | UI behavior |
|---|---|
| Event stream down | Show reconnect banner, keep last snapshot, mark `STREAM STALE`. |
| GitLab down | Show DB last-known truth, mark GitLab cells stale. |
| DB down | Show live GitLab/cache/runner probes where possible, disable mutations. |
| Docker down | Mark runner/container metrics stale, keep job state. |
| Vault down | Disable secret/release actions requiring Vault. |
| Cache API down | Show last-known cache usage and disable GC execute. |
| MCP unavailable | TUI direct HTTP/local can remain read-only; agent action surfaces degraded. |

### 26.3 Diagnostics page

Every degraded banner drills into Runtime/API Doctor with source, last success, last error, retry/backoff, stale impact, and suggested remediation.

---

## 27. Performance and memory requirements

### 27.1 UI performance

- Keystroke-to-render p95 under 50 ms; target under 30 ms locally.
- Render under 16 ms for standard 160x48/180x50 dashboards where feasible; under 40 ms for massive graphs.
- No network/DB call on focus movement.
- Large lists virtualized.
- Log tails ring-buffered or rope-buffered.
- Tables support incremental filter/sort.
- Background refresh never blocks input.
- Draw only when dirty or on heartbeat; coalesce bursts.

### 27.2 Backend efficiency

- Use event cursors, not full refresh loops.
- Batch entity detail requests.
- Cache immutable records by ID/SHA/digest.
- TTL hot/fresh data by panel focus.
- Back off failing sources.
- Avoid fetching full traces unless selected.
- Prefer tail/cursor APIs for logs.
- Compress or page large proof/history results.

### 27.3 Default memory caps

| Buffer | Default |
|---|---:|
| Event ring | 10,000 events |
| Per selected job log tail | 2,000 lines or 10 MB, whichever first |
| Background job log summaries | 200 lines/job, max 20 jobs |
| Entity cache | 100,000 lightweight entities |
| Artifact/finding history | lazy loaded |
| Proof timeline page | 500 events/page |
| Table materialization | viewport slice + overscan, not all rows every frame |

### 27.4 Scale test fixtures

Synthetic performance tests must include:

- 1,000 repos,
- 10,000 recent jobs,
- 100,000 events,
- 10 MB selected log,
- 10,000 cache objects,
- 5,000 bugs,
- 5,000 Jankurai findings,
- 500 active agents in fixture mode,
- mixed stale/degraded sources.

---

## 28. Testing strategy

### 28.1 Unit tests

- capacity formulas,
- bottleneck classification,
- tag eligibility,
- progress confidence selection,
- source freshness/staleness,
- action risk classification,
- command palette filtering,
- route stack behavior,
- graph edge generation,
- redaction.

### 28.2 Golden render tests

Use Ratatui test backend to render deterministic screens for:

- empty state,
- healthy state,
- overloaded queue,
- failed job,
- degraded/stale source,
- narrow width,
- drill mode,
- action modal,
- command palette,
- ASCII mode,
- high-contrast mode,
- incident mode.

### 28.3 Black-box TUI tests

Scenarios:

1. Launch demo TUI.
2. Global renders key metrics.
3. Arrow keys move macro focus.
4. `Enter` drills and title shows `[esc]`.
5. `Esc` exits drill.
6. `/` filters repo table.
7. `Ctrl-K` opens command palette and preview.
8. `b` jumps to blocker.
9. `x` explains selected wait/blocker.
10. Workflow DAG node selection opens inspector.
11. Log follow/manual scroll works.
12. Action preview blocks production action without confirmation.
13. Stale data does not blank existing screen.
14. Secret values never render.

### 28.4 Safety tests

- No plaintext secrets in any renderer/snapshot.
- Mutating actions require preview.
- Production actions require typed confirmation.
- Agent actions require capability envelope, nonce/idempotency, grant, and budget.
- Stale proof disables merge/release.
- Action side-effect classes match allowlist tests.
- Copy/open actions do not expose secret values.

---

## 29. Implementation phases

### Phase 0 — truth cleanup and contracts

- Reconcile docs/source drift.
- Generate action registry manifest.
- Fix side-effect classifications.
- Ensure merge/release actions are evidence/SHA gated.
- Define shared `TuiReadModel`, `TuiEvent`, `EntityDetail`, `ActionDescriptor`, `WorkflowGraph`, `CapacityFrontier` schemas.
- Build demo/fixture/read-model backend.

### Phase 1 — TUI shell and navigation

- Route stack, breadcrumbs, focus model.
- Theme, glyph fallback, terminal capability detection.
- Command palette.
- Help overlay.
- Pure renderer framework.
- Golden snapshot harness.

### Phase 2 — unified read model and streaming

- `/api/read-model`.
- `/api/events` cursor page.
- SSE/WebSocket event stream.
- `/api/entity/{kind}/{id}`.
- Data client with local fallback.
- Source freshness/degraded behavior.

### Phase 3 — Global, Queue, Repos

- Fleet dashboard.
- Capacity frontier formulas.
- Runner increase/what-if simulator.
- Repo family browser.
- Attention queue.

### Phase 4 — Workflow Atlas and logs

- Multi-pipeline/multi-repo workflow selection.
- Graph edges and DAG layout.
- Inspector tabs.
- Log stream with polling fallback.
- Critical path/blocker/ETA improvements.

### Phase 5 — Deep domain screens

- Cache vNext.
- VTI vNext.
- Runners/System.
- Bugs.
- Git Sync.
- Churn.

### Phase 6 — Trust/compliance screens

- Jankurai.
- Security/secrets.
- Artifacts/provenance.
- Release/rollback.
- Evidence timeline.
- Runtime/API/MCP Doctor.

### Phase 7 — Agents/autonomy full cockpit

- Agent lifecycle tables.
- Agent logs/messages/artifacts.
- Agent config editing.
- Autonomy governance.
- Kill bell/freezes/verdict/passport integration.
- Patch race visualization.

### Phase 8 — polish, scale, and demo

- Tuiwright/black-box coverage.
- Performance tuning/virtualization.
- Recording/replay/time travel.
- Demo fixtures.
- Capture PNG/GIF generation.
- Accessibility/high-contrast/ASCII/reduced-motion.

---

## 30. Acceptance criteria

### 30.1 UX acceptance

- From Global, drill to a failing job’s first error in ≤ 4 keystrokes.
- From Global, answer “why are jobs queued?” in ≤ 5 seconds.
- From Queue, answer “should we add runners?” with proof, limiting factor, and what-if result in ≤ 10 seconds.
- From Runners, answer “are we near core/memory issues?” in ≤ 5 seconds.
- From Repo, answer “safe to merge?” with exact blockers in ≤ 5 seconds.
- From Cache, identify top storage category and safe GC bytes in ≤ 5 seconds.
- From VTI, tell whether smart skipping is safe in ≤ 10 seconds.
- From Agents, identify task, branch/MR, grant, logs, and blocker in ≤ 10 seconds.
- `Enter` drills and `Esc` goes back everywhere.
- Every visible number has freshness or inherits pane freshness.

### 30.2 Data acceptance

- Every entity detail includes source freshness and last updated.
- Every mutating action has preview/proof/result events.
- Global queue uses webhook/local state plus GitLab reconciliation.
- Capacity frontier accounts for tags, trust tiers, health, CPU, memory, disk, cache, and blocked jobs.
- Pipeline graph includes child pipelines and artifact/needs edges when available.
- Security/release proof modals include artifact signature/provenance status.
- Jankurai screen shows installed version per repo and stale/missing versions.
- MR realtime state is either fully ingested or visibly marked partial.

### 30.3 Performance acceptance

- p95 input-to-render < 50 ms.
- p95 render < 16 ms on normal dashboard where feasible.
- p95 render < 40 ms under large graph viewport.
- Trace viewer handles 10 MB logs without blocking input.
- Event filters over 10k in-memory events under 20 ms.
- App remains useful when backend stream drops.

### 30.4 Safety acceptance

- Secrets redacted at backend and UI layers.
- Production actions cannot execute from stale proof.
- Merge/release actions bind to exact source SHA.
- Capability grants and action registry shown before high-risk actions.
- TUI logs its own mutating action requests to evidence/audit ledger.
- Reduced-motion and ASCII modes remain fully usable.

---

## 31. Dream superpowers after MVP

### 31.1 “Why is it not green?”

One key opens a concise chain:

```text
Not green because:
1. Required job nextest-shard-7 failed at 12:11.
2. Failure capsule cap_882 classified test_timeout.
3. Retry recommended; similar flake seen 3 times on remote-b with disk IO >20%.
4. Release gate waits on pipeline #530.
5. Artifact signature not created because package stage did not run.
Next safest action: retry shard on remote-a, then rerun package/sign if pass.
```

### 31.2 “Why is this waiting?”

```text
Waiting because:
1. Needs tags build,dind; only build pool matches.
2. build pool has 9/16 slots running but disk IO makes safe limit 10.
3. One manager is draining; one is OOM-looping.
4. Queue p95 for this stage is 8m; ETA start 4m.
Suggested: GC node-b or scale +2 build on remote-a; cancel superseded pipeline #529 first.
```

### 31.3 Predictive green time

Estimate time to green using current critical path, p50/p95 job history, queue pressure, required gates, cache/VTI state, and confidence. Show `ETA green 18m HIST .72`, never as certain fact.

### 31.4 Agent ROI dashboard

Show agent saved time, cost, failed loops, accepted patches, reverted patches, CI waste, bug closure rate, and authority denials by repo/family.

### 31.5 Flake intelligence

Dedicated flake score per test/job correlated with runner/node/cache state, recent log signatures, owner/component, quarantine, and recommended fix/skip/escalate.

### 31.6 Incident mode

On prod/canary/security incident:

- incident banner,
- freeze non-essential automation,
- pin release/security/evidence/log panes,
- show rollback target and proof,
- start incident timeline,
- export report after resolution.

### 31.7 Natural-language hints without hidden magic

Optional local assistant panel can translate questions into safe read-only filters/actions, but must show exact query/action before mutation.

---

## 32. Appendix A — durable state surfaces to index

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
| Cache | `cache_objects`, `cache_requests`, `hot_cache_entries`, `build_signatures`, `image_signatures`, `force_refresh_rules` |
| Cache trust/taint | `resolved_refs`, `cache_taints`, `cache_leases`, `cache_verdicts`, `cache_promotions`, `material_objects`, `material_aliases`, `action_cache`, `cache_epochs`, `toolchain_fingerprints` |
| Test intelligence | `test_executions`, `test_plans`, `test_plan_items`, `selector_misses` |
| Autonomy/safety | `launch_ledger`, `kill_bell_state`, `verdicts` |
| LLM budgets | `llm_budget_ledger` |
| Bug tracker | `bug_projects`, `bug_project_edges`, `bugs`, `bug_events`, `bug_attempts`, `bug_links`, `bug_external_refs`, `bug_evidence` |

---

## 33. Appendix B — GitLab/webhook fields to normalize

### Job Hook

- `build_id`
- `project_id`
- `pipeline_id`
- `build_status`
- `build_name`
- `build_queued_duration`
- `tag`
- `ref`
- `runner.id`
- `runner.description`

### Pipeline Hook

- `project.id`
- `object_attributes.id`
- `object_attributes.status`
- `object_attributes.sha`
- `object_attributes.ref`

### Push Hook

- `project_id`
- `before`
- `after`
- `ref`
- `project.path_with_namespace`

### Merge Request Hook to plumb

- MR IID/id/title/author
- source/target branch
- head/base SHA
- labels/draft state
- approval state/rules
- mergeability/detailed status
- discussions/unresolved count
- reviewers
- changed files/diff stats
- linked pipeline/checks
- target policy SHA/merge passport

---

## 34. Final build checklist for an implementation agent

1. Read existing `src/tui`, `src/api`, `src/tui/workflow`, `src/tui/action_registry`, and docs.
2. Fix action metadata drift and safety gates before exposing broad mutations.
3. Define the shared read-model/event/action/entity contracts.
4. Build rich demo fixtures for all screens.
5. Implement route stack, focus stack, breadcrumbs, command palette, and help overlay.
6. Implement pure renderers for Global, Queue, Repos, Workflow.
7. Add `LocalDataClient` mapping current DB/GitLab/Docker/cache state into new models.
8. Add `HttpDataClient` when `/api` endpoints exist.
9. Add streaming client with polling fallback.
10. Implement virtualized tables and bounded log buffers before scale screens.
11. Add capacity frontier and runner what-if simulator.
12. Add domain screens one by one; never put raw SQL/network calls in renderers.
13. Wire every context action through preview/execute and evidence logging.
14. Add golden screenshots and black-box interaction tests.
15. Add performance fixtures and memory caps.
16. Add accessibility modes.
17. Keep every mutation proof-gated.

Definition of done for any screen:

- renders empty state,
- renders demo rich state,
- renders degraded stale state,
- supports macro/micro focus,
- supports `/` filter if it has a list/table,
- `Enter` drills into selected entity,
- `Esc` goes back,
- `?` shows contextual help,
- selected entity has inspector,
- evidence/action links work or are visibly disabled,
- tests cover 80/120/180/220-column widths,
- no secrets printed,
- source freshness is visible,
- action availability is explainable.

---

## 35. Final UX target

The finished TUI should feel like this:

- Open it and immediately know whether the fleet is healthy.
- Press `b` and land on the top blocker.
- Press `Enter` and see the exact workflow node.
- Press `l` and watch live trace chunks stream.
- Press `e` and see the proof chain.
- Press `x` and get a concrete explanation.
- Press `a` and see safe actions with blast radius.
- Press `Esc` twice and return to the global map.
- Press `gq` and know whether adding runners helps.
- Press `gu` and know whether CPU/memory/disk make more runners dangerous.
- Press `gc` and know whether cache is full and what to reclaim.
- Press `gv` and know whether VTI is saving time safely.
- Press `gj` and know if code quality is improving.
- Press `gR` and know exactly what version is in prod and how to roll back.

That is the bar: **one terminal, every repo, every job, every agent, every proof, every release, every action — fast enough to feel alive and honest enough to trust.**

---

## 36. External implementation references

These are not product requirements, but they align the implementation choices with maintained ecosystem contracts:

- Ratatui docs: https://docs.rs/ratatui/latest/ratatui/
- Ratatui backends: https://ratatui.rs/concepts/backends/
- Crossterm docs: https://docs.rs/crossterm/
- Tokio `mpsc`: https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html
- Tokio `watch`: https://docs.rs/tokio/latest/tokio/sync/watch/index.html
- MCP Streamable HTTP transport spec: https://modelcontextprotocol.io/specification/2025-11-25/basic/transports
