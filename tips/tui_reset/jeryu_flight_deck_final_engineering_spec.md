# JeRyu Flight Deck — Final Rust TUI Engineering Specification

**Artifact:** `jeryu_flight_deck_final_engineering_spec.md`  
**Date:** 2026-05-26  
**Audience:** Rust backend engineers, TUI engineers, CI/release engineers, and autonomous build agents.  
**Target:** A world-class Rust terminal control plane for JeRyu and Veox-style multi-repo CI, release, cache, VTI, agent, evidence, security, and repository operations.  
**Design center:** One developer can open `jeryu tui`, understand the entire engineering machine in under ten seconds, then drill from fleet posture to repo family, repo, pipeline, job, live trace, evidence, config, bug, release gate, or action receipt in one or two keystrokes.

---

## 0. Final product thesis

JeRyu Flight Deck is not a dashboard. It is **air traffic control for autonomous software delivery**.

It must show every repo, repo family, pipeline, job, queue, runner, cache, VTI decision, agent, bug, release, artifact, secret, policy, security finding, Jankurai score, Git sync state, and proof event as one living system. The TUI should feel alive: queues pulse, DAGs advance, traces stream, cache traffic moves, agents leave visible footprints, and event tails roll in real time. But it must never fake confidence. Every animated fact must come from a timestamped event, stream, snapshot, or explicitly degraded/polled source.

The promise:

> **JeRyu Flight Deck lets a developer operate a fleet of repositories and agents at production-control-room speed, while preserving evidence, provenance, safety, and exact drilldown for every claim.**

The highest-level design stance is:

1. **Truth first:** every number has provenance, freshness, and an entity behind it.
2. **Motion second:** animation exists to reveal liveness, flow, and change; it never hides missing data.
3. **Keyboard speed always:** arrows, tabs, enter, escape, slash search, and colon command palette are the main operating surface.
4. **Everything is drillable:** every red/yellow item explains itself and points to proof.
5. **Every mutation is previewed:** actions are generated from the action registry, dry-run when possible, and logged to evidence.
6. **Fleet and family aware:** repo families such as `veox-*`, isolated repos, and shared infrastructure repos must be first-class scope units.
7. **Built on the typed read model:** no screen should invent private truth if a shared `TuiReadModel`, `TuiEvent`, `EntityDetail`, `ActionPreview`, and `ActionResult` can expose it.

---

## 1. Source-derived baseline

The uploaded archive contains prior `.md` TUI design attempts plus `.txt` inventories of JeRyu's API, MCP, realtime, and durable-state surfaces. The following baseline is treated as source-derived truth for this spec.

### 1.1 Current JeRyu control-plane surfaces

| Surface | Current transport / entrypoint | Realtime data exposed or consumed | Mutating |
|---|---|---|---|
| Main CLI | `jeryu <command>` | install, serve, remote, node, TUI, Git wrapper, repo/fleet, status, pools, jobs, pipelines, cache, logs, agents, settings, tests, release, secrets, progress, bugs, policy, host, MCP, next action, blocker explanations, action registry | Yes |
| TUI | `jeryu tui` | Mission, Release, Jobs/Flow, Agents, Tests/VTI, Pools, Cache, Evidence, Secrets, LLMs, Git-style views depending on vintage | Limited actions |
| Typed TUI API | Rust `src/api` layer | entity kinds, event kinds, read-model snapshots, component health, actions, snapshot builders | Yes through action dispatch |
| MCP stdio | `jeryu mcp serve` / `serve-stdio` | JSON-RPC MCP tools over stdin/stdout | Yes, via capability envelope |
| MCP HTTP | default `127.0.0.1:9778`, `/mcp` | POST tool calls, session/protocol headers, DELETE session; GET is currently disabled | Yes |
| HTTP webhook/API | default `127.0.0.1:9777` | `/health`, `/hooks`, `/cache/summary` | Yes, internal side effects |
| Capability API | Unix-domain socket | length-framed agent action requests, grants, nonces, budgets, capability proofs | Yes |
| GitLab REST wrapper | internal client | projects, jobs, traces, artifacts, pipelines, downstream pipelines, variables, runners, MRs, issues, branches, webhooks | Yes |
| GitLab webhooks | `/hooks` | Job, Pipeline, Push; MR hooks accepted/logged but not fully acted on | Consumes and triggers side effects |
| Broker/message log | Kafka or Jansu feature-gated | `jeryu.webhook.jobs`, `jeryu.webhook.pipelines`, `jeryu.webhook.pushes` | Internal |
| Custom executor | `jeryu exec config/prepare/run/cleanup` | GitLab Runner custom executor lifecycle, sandbox state, job env, logs, capsules | Yes |
| Git server hook | `jeryu server-hook pre-receive` | ref update admission, actor kind, grants, policy verdicts | Yes, can deny push |
| SmartCache/cache gateway | proxy default `19800`, registry mirror `19801` | cargo sparse config, crate downloads, CAS hits, singleflight, CONNECT proxy metrics | Yes, cache writes |
| Docker runner control | Bollard / compose | manager containers, logs, lifecycle, Docker events | Yes |
| Vault/secrets | Vault HTTP API | health/init/unseal/KV v2/policies/rotation/report/recovery metadata | Yes |
| State DB | SQLite default, RedlineDB optional | durable truth for pools, jobs, releases, evidence, cache, grants, bugs, LLM budget, autonomy, etc. | Yes |
| Autonomy binary | `autonomy` CLI/server | Evidence Gate/VibeGate, kill bell, verdicts, foundry candidates, LLM budget, `/metrics`, `/health`, `/events` | Yes |
| GitHost abstraction | GitHub/GitLab-like adapter | PR/MR state, diffs, checks, comments, approvals, policy SHA | Yes |

### 1.2 Current MCP source-of-truth tool list

The current MCP surface is tool-centric. These tools are under the `jeryu.` prefix:

| Tool | Args | Purpose |
|---|---|---|
| `jeryu.fetch_capsule` | `job_id` | Fetch latest structured failure/evidence capsule for a job. |
| `jeryu.get_system_snapshot` | none | GitLab readiness, pool count, recent job events, latest release attempt. |
| `jeryu.get_pipeline_jobs` | `project_id`, `pipeline_id` | Downstream-expanded pipeline job list. |
| `jeryu.get_ci_bottlenecks` | `project_id`, optional `ref_name`, optional `limit` | Historical CI timing bottlenecks. |
| `jeryu.explain_blockers` | `entity_type`, `entity_id` | Job/release/merge blockers from capsules, releases, selector misses. |
| `jeryu.plan_validation` | `project_id`, `ref_name`, `test_ids[]` | Validate proposed test plan against selector misses. |
| `jeryu.run_tests` | `project_id`, `target_ref`, `test_scope` | Create ephemeral CI branch, inject CI YAML, trigger pipeline. |
| `jeryu.propose_patch` | `project_id`, `branch_name`, `base_ref`, `commit_message`, `modifications[]`, optional `mr_title` | Create branch, commit files, open MR, record branch grant. |
| `jeryu.race_patches` | `project_id`, `base_branch`, `commit_message`, `hypotheses[]` | Create multiple hypothesis branches and trigger pipelines. |
| `jeryu.request_merge` | `project_id`, `mr_iid`, `source_branch`, `target_branch` | Accept/request GitLab MR through gate logic. |
| `jeryu.bug_submit` | `report`, optional `idempotency_key` | Submit canonical local bug report. |
| `jeryu.bug_list` | optional `project`, `status`, `sort` | List local bug records. |
| `jeryu.bug_show` | `bug_id` | Show bug with events and attempts. |
| `jeryu.bug_ready` | optional `project` | List ready bugs, including failed-attempt filters. |
| `jeryu.bug_update` | `bug_id`, optional status/severity/priority/component/owner | Triage/update bug. |
| `jeryu.bug_record_attempt` | `bug_id`, `attempt` | Append attempt history. |

### 1.3 Durable state families available to the TUI

The durable DB is the broadest local source of truth. The TUI should assume these families exist or can be added with migrations.

| Family | Current / expected tables and data |
|---|---|
| Runner / CI / release | `pools`, `managers`, `job_events`, `ci_job_runs`, `tracked_pipelines`, `tracked_repositories`, `release_attempts` |
| Capability / admission / Git audit | `capability_intents`, `capability_grants`, `admission_decisions`, `git_command_events`, `git_ref_updates`, `git_mirror_jobs`, `git_risk_approvals`, `git_command_artifacts`, `events` |
| Evidence / retry / VTI / tests | `evidence_capsules`, `retry_decisions`, `test_executions`, `test_plans`, `test_plan_items`, `selector_misses` |
| Cache / provenance / material | `cache_objects`, `cache_requests`, `hot_cache_entries`, `build_signatures`, `image_signatures`, `force_refresh_rules`, `resolved_refs`, `cache_taints`, `cache_leases`, `cache_verdicts`, `cache_promotions`, `material_objects`, `material_aliases`, `action_cache`, `cache_epochs`, `toolchain_fingerprints` |
| Secrets / Vault | `secret_authorities`, `release_secret_sets`, `secret_audit_events` |
| Bugs | `bug_projects`, `bug_project_edges`, `bugs`, `bug_events`, `bug_attempts`, `bug_links`, `bug_external_refs`, `bug_evidence` |
| Autonomy / Evidence Gate | `launch_ledger`, `kill_bell_state`, `verdicts`, `foundry_candidates`, `llm_budget_ledger` |
| Proposed agent lifecycle | `agent_sessions`, `agent_steps`, `agent_artifacts`, `agent_messages`, `agent_events` |
| Proposed observability | `webhook_deliveries`, `node_metrics`, `container_metrics`, `main_daemon_metrics`, `mcp_call_audit` |
| Proposed quality/supply chain | `jankurai_audits`, `jankurai_findings`, `artifact_attestations`, `code_churn_samples`, `scan_findings` |

### 1.4 Known gaps that this spec fixes

| Gap | Impact | Required fix |
|---|---|---|
| TUI currently polls for many live views | UI cannot feel truly live at scale | Add event, log, cache, release, and agent streaming; keep polling fallback. |
| MCP HTTP GET/SSE disabled | Agents can call tools but cannot browse/watch resources | Add MCP resources and subscriptions or mirror via HTTP inspection API. |
| Flow board biases first active pipeline | Multi-repo/multi-pipeline fleet cannot be understood | Global workflow atlas and scope-aware pipeline selection. |
| Pipeline graph edges incomplete | DAG view cannot show true critical path | Compute edges from GitLab `needs`, stage barriers, child pipelines, artifacts, release gates, cache/VTI dependencies. |
| ETA heuristic only | Misleads about theoretical limit | Physics/fleet/policy bounds with source confidence. |
| Evidence not a searchable proof timeline | Hard to trust, audit, or answer “why?” | Unified proof ledger endpoint/resource and TUI screen. |
| Agents lack dedicated lifecycle table | Agent work is reconstructed from side effects | Add explicit agent session/step/event tables. |
| MR hooks accepted/logged but not acted on | Workflow/MR state incomplete | Persist MR entity state: labels, approvals, discussions, mergeability, target policy SHA, diff risk. |
| `/cache/summary` sparse | Cannot answer what fills cache or what is tainted | Expand cache endpoints and category/provenance model. |
| Broker and webhook delivery metadata underexposed | Forensics weak | Record delivery UUID, event type, raw body SHA, bytes, parse status, broker offset, handler status, latency, correlation ID. |
| Docs/action/MCP drift | Agents call stale surfaces | Generate docs/schemas from Clap, action registry, MCP definitions, DB schema. |

---

## 2. Product doctrine

### 2.1 Operator questions the TUI must answer instantly

When `jeryu tui` opens, the operator should immediately know:

1. Can I code safely right now?
2. Can I merge safely right now?
3. Can I release or roll back safely right now?
4. What is running across all repos?
5. Which repo family is blocking global progress?
6. How close is CI running to the true theoretical limit?
7. Are runners, caches, test selection, policies, or agents slowing us down?
8. Is VTI saving time safely, or is it missing tests?
9. Are caches full, stale, tainted, or wasting space?
10. Which agents are active, blocked, racing, waiting for grants, or burning budget?
11. Which bugs are pending, worked, blocked, fixed, or awaiting review?
12. Is Git local/remote state synced?
13. Which repos are failing Jankurai minimums or security gates?
14. Are artifacts signed, provenanced, and releasable?
15. Why is something not green?
16. What should I do next?

### 2.2 Non-negotiable UX laws

#### Law 1: Every visible object is addressable

Every row, node, card, metric, sparkline, progress bar, warning, log annotation, event, and artifact must have an `EntityRef` or explicit non-entity metadata behind it.

Addressable objects include:

- repo family
- repo/project
- branch/ref/SHA
- MR/PR
- pipeline
- child pipeline
- job
- stage
- workflow edge
- runner pool
- runner manager
- remote node
- cache object/category/taint/verdict/lease
- VTI plan/test/miss/selector receipt
- agent/session/task/step/grant
- bug/attempt/evidence
- Jankurai audit/finding/cap/rule family
- security finding
- secret authority/secret set/audit event
- release attempt/gate/canary/rollback target
- signed artifact/SBOM/provenance/signature
- admission decision
- Git command/ref update/mirror job
- LLM call/provider/budget ledger row
- evidence capsule/proof timeline event

#### Law 2: Red and yellow must explain themselves

A warning must always expose:

```text
label: short human-readable status, e.g. QUEUE SATURATED
cause: structured cause line
confidence: high/medium/low or exact source confidence
freshness: last source update and staleness state
evidence: one or more EvidenceRef values
action: next recommended safe action
owner: human, agent, repo, or team if known
```

#### Law 3: Drilldown is spatial and reversible

- `Enter` or `Right` drills into focused object.
- `Esc`, `Left`, or `Backspace` returns to the previous scope.
- Breadcrumbs are always visible.
- The global alert strip stays visible even deep inside job logs.
- No modal dead ends: every overlay supports `Esc` cancel/up, `Enter` accept/default, `?` local help.

#### Law 4: Motion must be truthful

Animations are allowed and encouraged, but they must derive from real state:

- active job pulse uses actual job state and elapsed time;
- progress bars advance only from job progress, trace timestamps, or ETA confidence bands;
- cache flow animation uses observed hit/miss/request events;
- agent activity comet trails use actual steps or heartbeats;
- event ticker uses real `TuiEvent` entries;
- stale sources freeze animation and show a staleness badge.

#### Law 5: Action safety beats speed

Read-only actions can be instant. Mutations require risk-aware preview. Destructive/prod/secret/merge actions require typed confirmation and evidence. Dry-run must be offered whenever the backend can support it.

#### Law 6: Empty states are useful

No screen should be blank. Empty states should say why, where data would come from, whether the source is stale/disabled, and what action would enable or repair it.

---

## 3. Core mental model: universe → family → repo → workflow → entity → proof

The TUI navigation stack is a spatial hierarchy:

```text
Universe / Fleet
  ├─ Repo families
  │   ├─ veox-*
  │   │   ├─ veox-deploy
  │   │   │   ├─ branches / MRs
  │   │   │   ├─ pipelines / child pipelines
  │   │   │   │   ├─ stages
  │   │   │   │   ├─ jobs
  │   │   │   │   ├─ traces
  │   │   │   │   ├─ artifacts
  │   │   │   │   └─ evidence capsules
  │   │   │   ├─ VTI plans / selector misses
  │   │   │   ├─ cache objects / verdicts
  │   │   │   ├─ bugs / attempts
  │   │   │   ├─ agents / grants
  │   │   │   ├─ release gates / artifacts
  │   │   │   └─ proof timeline
  │   │   └─ veox-enclave
  │   ├─ redline-*
  │   ├─ jeryu
  │   └─ isolated repos
  ├─ Shared execution fabric
  │   ├─ pools / runners / managers / nodes
  │   ├─ cache / registry / CAS / material trust
  │   ├─ Vault / secrets / release secret sets
  │   ├─ broker / webhook delivery ledger
  │   └─ LLM providers / budget / key pools
  └─ Evidence fabric
      ├─ event ledger
      ├─ capsules
      ├─ admission decisions
      ├─ capability grants
      ├─ signed artifacts
      ├─ Jankurai proofs
      └─ release passports
```

The global page shows the Universe. Each drilldown narrows scope but keeps global posture, source freshness, and critical alerts visible.

---

## 4. Top-level navigation

### 4.1 Primary tabs / lenses

The tab row is fixed. Numeric keys jump to high-frequency pages. Letter keys jump to named lenses.

| Key | Lens | Purpose |
|---|---|---|
| `1` / `g` | Global | Fleet posture, live work, next actions, hot alerts. |
| `2` / `r` | Repos | Repo families, repo inventory, health, ownership, drilldown. |
| `3` / `w` | Workflows | Workflow Atlas: pipeline DAGs, live jobs, traces, critical path. |
| `4` / `q` | Queue | Queue pressure, theoretical limit, SCREAM index, scheduling. |
| `5` / `u` | Utilization | Runner pools, remote nodes, host/Docker, capacity controls. |
| `6` / `c` | Cache | SmartCache storage, hit/miss, taints, GC, provenance. |
| `7` / `t` | VTI / Tests | Smart test skipper proof, selected/skipped tests, misses, savings. |
| `8` / `a` | Agents | Agent sessions, races, grants, budgets, logs, config. |
| `9` / `b` | Bugs | Cross-repo bugs/issues, attempts, ownership, agent work. |
| `0` / `x` | Release | Release/canary/prod, rollback, versions, signed artifacts. |
| `j` | Jankurai | Audit scores, caps, duplicate code, proof lanes, repair queue. |
| `s` | Security | Secrets, policies, vulnerabilities, admission, supply chain. |
| `p` | Proof | Evidence ledger, capsules, action receipts, audit timeline. |
| `m` | Metrics | CI economics, churn, LLM budget, waste, trends. |
| `:` | Command | Command palette / action launcher. |
| `/` | Search | Global search / filter. |
| `?` | Help | Context-sensitive keymap and local affordances. |

### 4.2 Universal keymap

| Key | Behavior |
|---|---|
| `Up` / `Down` | Move selection within active pane/table/list. |
| `Left` | Move to parent pane or pop scope when no horizontal neighbor exists. |
| `Right` | Move to child pane or drill into focused object. |
| `Enter` | Drill into focused object or accept default action. |
| `Esc` / `Backspace` | Close overlay or pop navigation stack. |
| `Tab` / `Shift-Tab` | Cycle focus panes forward/back. |
| `[` / `]` | Previous/next sub-tab inside current scope. |
| `Home` / `End` | Start/end of active table/log. |
| `PageUp` / `PageDown` | Page active list/log. |
| `/` | Focus filter/search for current scope. |
| `Ctrl-/` | Global search across all entities. |
| `:` | Command palette. |
| `.` | Repeat last safe read-only command or refresh focused entity. |
| `r` | Retry/refresh/reconcile depending on context; mutation requires preview. |
| `R` | Retry dangerous/release action; always previewed. |
| `l` | Open logs/trace for focused job/agent/node. |
| `e` | Open evidence/proof for focused object. |
| `o` | Open external URL/path when safe. |
| `y` | Open “why?” explanation for focused warning/entity. |
| `n` | Open “next action” drawer. |
| `f` | Freeze/unfreeze current live selection while streams continue in background. |
| `Space` | Pin/unpin selected entity to watchlist. |
| `Ctrl-s` | Save screenshot/capture. |
| `Ctrl-r` | Hard reconnect stream / reload snapshot. |
| `Ctrl-c` | Quit with terminal cleanup. |

### 4.3 Pane movement grammar

To satisfy fast directional operation:

```text
Up/Down      = move within the current list, table, menu, or graph column
Left/Right   = move between sibling panes; at hierarchy edge, Left pops scope and Right drills scope
Tab          = cycle macro focus across panes
Enter        = descend into selected object
Esc          = ascend out of current object/overlay
```

The user should be able to traverse:

```text
Global → veox-* family → veox-deploy repo → pipeline #9182 → job integration/auth → live trace → evidence capsule
```

using only:

```text
Down, Enter, Down, Enter, Right, Enter, l, e, Esc, Esc, Esc
```

### 4.4 Command palette

`:` opens a fuzzy command palette. Commands are generated from the action registry and filtered by current focus.

Examples:

```text
:queue explain veox-*
:runner scale pool=rust +2 --dry-run
:cache gc --category cargo-registry --dry-run
:vti audit repo=veox-api window=7d
:agent spawn --bug BUG-183 --repo veox-api
:release rollback --target v1.1.7 --dry-run
:open evidence capsule job:53812
:copy artifact digest release:2026.05.26-4
:watch pipeline 9182
```

Palette rows must show:

```text
command | side-effect class | risk tier | required grant | dry-run? | estimated blast radius | last success/failure
```

---

## 5. Visual and motion language

### 5.1 Terminal capability tiers

| Tier | Capability | Behavior |
|---|---|---|
| Truecolor | 24-bit color, Unicode, mouse, alternate screen | Full theme, smooth gradients, glyph-rich DAGs. |
| 256-color | 256-color terminal, Unicode | Semantic colors with downgraded palette. |
| 16-color | basic terminal | Semantic status retained through glyphs and words. |
| No Unicode | ASCII fallback | Box drawings and symbols replaced with `+`, `-`, `|`, `OK`, `WARN`, `FAIL`. |
| Low-motion | user/environment preference | Disable nonessential pulsing; keep event updates and progress. |

### 5.2 Semantic palette

| Token | Truecolor | 256 fallback | Meaning |
|---|---:|---:|---|
| `ok` | `#50FA7B` | 48 | Healthy, green, passed. |
| `warn` | `#F1FA8C` | 228 | Warning, needs attention. |
| `critical` | `#FF5555` | 203 | Failed, blocked, unsafe. |
| `running` | `#8BE9FD` | 117 | Active work, live streams. |
| `queued` | `#FFB86C` | 215 | Waiting, queue pressure. |
| `stale` | `#6272A4` | 61 | Stale/degraded/unknown data. |
| `agent` | `#C792EA` | 177 | Agent/autonomy actions. |
| `cache` | `#7FDBCA` | 116 | Cache/storage/material trust. |
| `vti` | `#A3BE8C` | 150 | Test selection, saved time. |
| `jankurai` | `#BD93F9` | 141 | Audit/quality. |
| `security` | `#FF79C6` | 212 | Secrets/security/policy. |
| `artifact` | `#B7E3A1` | 150 | Signed artifacts/provenance. |
| `release` | `#FFD866` | 221 | Release/canary/prod. |
| `dim` | `#6B7280` | 244 | Secondary information. |
| `focus` | `#FFFFFF` | 15 | Active focus border/text. |

### 5.3 Status glyphs

| Glyph | Meaning |
|---|---|
| `●` | running/live |
| `○` | idle/waiting |
| `✓` | passed/healthy |
| `!` | warning/blocker |
| `✖` | failed/unsafe |
| `…` | pending/in progress |
| `⏸` | paused/approval/freeze |
| `↻` | retry/reconcile |
| `⇄` | sync/mirror |
| `◆` | release/artifact |
| `◇` | unsigned/unverified artifact |
| `⚿` | secret/Vault |
| `⌁` | cache flow |
| `V` | VTI/test selection |
| `J` | Jankurai/audit |
| `A` | agent/autonomy |
| `E` | evidence/proof |

### 5.4 Motion design

Motion should be legible, not noisy.

| Motion | Use | Rules |
|---|---|---|
| Pipeline node pulse | running jobs | Pulse speed tied to last log/event age; stops on stale. |
| Progress shimmer | active stage/job | Width derived from progress/ETA; shimmer means “active”, not “more complete”. |
| Queue conveyor | queued jobs | One cell per ready job bucket; color by family/pool. |
| Cache arterial flow | hit/miss streams | Left-to-right for downloads, right-to-left for writes; red sparks for taints. |
| Agent comet trail | agent steps | Trail length = recent steps within window; dim with age. |
| Event ticker | global event tail | Severity colors; never drop critical events without count. |
| Heatmap flicker | utilization/cost | Only on changed cells. |
| Scream meter | throughput posture | Smooths over 3–5 seconds; raw numeric remains exact. |

Animation must have a global low-motion toggle and must be disabled when source freshness is stale.

### 5.5 Progress bars

Use segmented bars that encode status:

```text
[████████░░░░] 67% running
[██████!!░░░] 61% warning/blocker inside progress
[███???????] low confidence ETA
[──────────] unknown/not started
```

Every progress bar supports `Enter` for details and `y` for explanation.

---

## 6. Global shell layout

### 6.1 Persistent regions

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Header: posture, scope, time, stream, freshness, SCREAM, next action         │
├ Tabs: Global Repos Workflows Queue Util Cache VTI Agents Bugs Release ... ───┤
│ Left scope rail │ Main workspace                                      │ Right │
│ families/repos │ tables, DAGs, heatmaps, flow panes                  │ insp. │
│ watchlist      │                                                      │       │
├──────────────────────────────────────────────────────────────────────────────┤
│ Event tail / active command / selected action hint / stream diagnostics      │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 6.2 Header fields

The header must be dense and always visible:

```text
JeRyu Flight Deck | scope=fleet | stream=live seq=183921 | stale=none | safe code✓ merge! release✖ | SCREAM 83 | next: fix artifact signature
```

Header segments:

- current route/breadcrumb
- scope: fleet/family/repo/pipeline/entity
- stream status: live, reconnecting, polling, offline, replay
- freshness minima by source: GitLab, DB, Docker, cache, Vault, broker, autonomy
- safe-to-code / safe-to-merge / safe-to-release booleans
- SCREAM index
- top blocker
- next action
- active watch pins

### 6.3 Bottom bar

The bottom bar is context-sensitive and shows local actions:

```text
↑↓ select  ← up  →/Enter drill  Tab pane  / filter  : command  y why  e evidence  ? help
```

For dangerous contexts:

```text
PREVIEW REQUIRED: R rollback  p promote  s sign  d drain  Esc cancel  ? safety
```

---

## 7. Global screen: Fleet Mission Control

### 7.1 Purpose

The global page answers:

- What is happening across all repos right now?
- Which repo family needs attention?
- Are we close to the theoretical CI limit?
- Is anything unsafe to merge/release?
- Which queue/pipeline/job/agent/release/security fact should I inspect next?

### 7.2 Wide mock

```text
┌ JeRyu Flight Deck ─ fleet live seq=183921 ─ SCREAM 86 ▲ ─ safe code✓ merge! release✖ ┐
│ Sources GitLab 1s✓ DB 0s✓ Docker 2s✓ Cache 4s✓ Vault 1s✓ Broker 0s✓ Autonomy 3s✓      │
├ Families ─────────────────┬ Live Critical Path ───────────────────┬ Attention ───────┤
│ veox-*        ● 31 run 7! │ veox-deploy #9182 deploy-canary ● 72% │ ✖ unsigned wasm   │
│  deploy       ● 9 run 2!  │  build ✓ test ✓ sign ✖ canary … prod □ │ ! queue rust p95  │
│  enclave      ● 4 run 1!  │  blocker: ART-392 unsigned            │ ! VTI miss auth   │
│ redline-*     ● 12 run 3! │ redline-db #774 btree stress ● 43%    │ ! cache 91% full  │
│ jeryu         ✓ green     │  critical path: test/btree/delete     │ ! MR hook stale   │
│ isolated      ○ idle      │ veox-api #552 authz e2e queued 6m     │ ✓ Vault healthy   │
├ Queue pressure ───────────┼ Runners / Cache / Agents ─────────────┼ Next Action ──────┤
│ ready 42 running 58 wait  │ rust pool 93% useful  gpu 41% idle    │ retry sign job    │
│ physics 18m fleet 27m     │ cache 366/400GiB hit 82% taints 2     │ job #774? dry-run │
│ policy 41m loss gate 14m  │ agents active 18 blocked 5 racing 3   │ Enter preview     │
├ Event tail ───────────────────────────────────────────────────────────────────────────┤
│ 03:41 job#774 sign failed: missing provenance  03:42 VTI miss auth_e2e  03:42 cache taint│
└ Enter drill  Tab pane  y why  e evidence  n next  : command  / search  ? help ─────────┘
```

### 7.3 Panels

#### Family pulse pane

Rows grouped by family, then repo. Each row shows:

```text
family/repo | posture glyph | running | queued | failed | blocked | release | cache | VTI | agents | last event age
```

Family rollup is not a simple average. It is a severity-weighted summary:

```text
critical if any child has production/release/security/data-loss blocker
warning if any child has CI/cache/VTI/agent blocker
ok if all active children are green or only low-risk work remains
```

#### Live critical path pane

Shows top 3–5 active critical paths by impact, not just latest pipelines. Includes:

- repo/family
- pipeline/job/release gate
- progress
- ETA and confidence
- current blocker
- physics/fleet/policy bound loss
- next action

#### Attention queue

Severity-ranked actionable items. Ranking inputs:

```text
severity
x blast_radius
x user_blocking_weight
x stale_source_penalty
x release_or_prod_weight
x security_weight
x confidence
- already_assigned_discount
```

#### Event tail

Shows last events but must preserve critical event visibility. If 1,000 events arrive in a burst, coalesce low-severity events but keep counts:

```text
+312 job.log.chunk  +18 cache.hit  +1 CRITICAL release.gate.failed ART-392
```

---

## 8. Queue and theoretical-limit model

### 8.1 Goal

The user explicitly wants to know whether the system is running near its theoretical limit. A simple CPU utilization or runner utilization number is misleading. The TUI must separate **physics limit**, **fleet limit**, and **policy limit**.

### 8.2 Definitions

For each scope `S` = fleet, family, repo, pipeline, or release train:

```text
D_best(j)        = p10 historical duration for same job/stage/ref class with hot cache
D_p50(j)         = median historical duration
D_p95(j)         = p95 duration
D_current(j)     = elapsed or completed duration for current run
Deps(j)          = stage barrier, needs, artifacts, child pipeline, release gate dependencies
Pool(j)          = eligible runner pools/tags/trust tiers/nodes
Cache(j)         = cache verdict, hit probability, taints, toolchain fingerprint
Risk(j)          = required gates/security/release/approval policy
Freshness(j)     = source freshness confidence
```

### 8.3 Physics bound

The physics bound ignores queue and current fleet capacity. It asks: “If all possible work had perfect execution resources, what is the critical-path lower bound?”

```text
physics_eta = longest_path(D_best, DAG_deps)
physics_efficiency = physics_eta / max(actual_or_predicted_wall_clock, 1s)
```

### 8.4 Fleet bound

The fleet bound includes current runner slots, tags, remote nodes, warm/cold manager state, cache state, and resource constraints.

```text
fleet_eta = simulate_schedule(
  jobs = ready + running + pending,
  durations = D_p50 adjusted by cache and flake state,
  resources = runner_slots_by_pool_node_tag,
  cold_start = p50 manager startup by pool/node,
  constraints = deps + tags + trust tier + node affinity + cache/material trust
)
fleet_efficiency = fleet_eta / max(actual_or_predicted_wall_clock, 1s)
```

### 8.5 Policy bound

The policy bound adds non-bypassable gates:

```text
policy_eta = fleet_eta
           + unavoidable_gate_waits
           + release_canary_min_duration
           + required_human_approval_sla_remaining
           + freeze_window_remaining_if_applicable
           + secret_rotation_or_signature_wait
policy_efficiency = policy_eta / max(actual_or_predicted_wall_clock, 1s)
```

### 8.6 SCREAM index

The **SCREAM index** is the headline fleet efficiency score from 0–100.

```text
scream = clamp(100 * weighted_mean([
  policy_efficiency,             weight .30,
  useful_runner_utilization,     weight .20,
  non_obsolete_work_ratio,       weight .15,
  cache_health_score,            weight .10,
  vti_confidence_score,          weight .10,
  source_freshness_score,        weight .10,
  blocker_resolution_score       weight .05
]), 0, 100)
```

Where:

```text
useful_runner_utilization = busy_runner_seconds_on_non_superseded_jobs / total_runner_capacity_seconds
non_obsolete_work_ratio   = active_non_superseded_jobs / max(active_jobs, 1)
cache_health_score        = hit_ratio * not_tainted_factor * not_full_factor
vti_confidence_score      = 1 - weighted_recent_selector_miss_rate
source_freshness_score    = min freshness across GitLab/DB/Docker/cache/Vault/broker/autonomy
blocker_resolution_score  = fraction of high-priority blockers with owner/action/evidence
```

### 8.7 Queue screen mock

```text
┌ Queue Physics ─ scope fleet ─ SCREAM 86 ─ loss 23m vs policy bound ─────────────┐
│ Current wall 41m  physics 18m  fleet 27m  policy 41m  useful runner util 88%    │
├ Limits ───────────────────────┬ Loss attribution ──────────────────────────────┤
│ Physics lower bound   18m ✓   │ 14m release policy: canary minimum + approval  │
│ Fleet lower bound     27m !   │  6m queue: rust runners saturated              │
│ Policy lower bound    41m !   │  3m cache: cold crates / taint quarantine      │
│ Current predicted     43m !   │  2m VTI fallback full auth suite               │
├ Ready jobs by pool ───────────┼ What-if simulator ─────────────────────────────┤
│ rust       ready 28 slots 30  │ +2 rust managers: -4m10, cost +0.7 runner-hr   │
│ gpu        ready  2 slots  5  │ cancel superseded: -2m40, safe yes            │
│ secure     ready  8 slots  4  │ prewarm image rust-ci:latest: -1m50           │
│ release    ready  1 slots  1  │ no action clears artifact signature gate      │
└ Enter pool  w what-if  c cancel obsolete  s scale preview  v VTI  e evidence ───┘
```

### 8.8 Bottleneck classes

| Class | Signal | Suggested action |
|---|---|---|
| Queue saturation | ready jobs > free eligible runners | Scale pool, adjust tags, unpause pool, add node. |
| Cold starts | high wait from manager startup | Increase warm managers, pre-pull images. |
| Cache miss storm | hit ratio drop, upstream latency, taints | Inspect top misses, taints, force-refresh, hot entries. |
| Serial DAG | long critical path with low parallelism | Split jobs, add `needs`, shard tests. |
| VTI low confidence | full-test fallbacks, selector misses | Teach mappings, audit misses, adjust guardrails. |
| Obsolete work | superseded pipelines still consuming runners | Auto-cancel superseded safe jobs. |
| Release policy wait | canary/approval/freeze/signature gate | Approve, sign, rollback, or wait; runners do not help. |
| Security gate | SAST/secret/dependency/signature failure | Drill into scan/artifact proof. |
| Remote node bottleneck | CPU/mem/disk/SSH high, Docker unhealthy | Rebalance, add node, GC disk, restart manager. |
| Agent contention | grants/budgets/worktrees exhausted | Increase budget/grants or pause low-priority agents. |

---

## 9. Repo families and repo atlas

### 9.1 Family grouping

Repo families are first-class objects. Default grouping:

```text
family = explicit .jeryu/family.toml if present
      || configured regex map in ~/.jeryu/settings.json
      || prefix before second hyphen for known groups, e.g. veox-deploy → veox-*
      || provider namespace/group
      || isolated
```

Family metadata:

```rust
pub struct RepoFamily {
    pub id: String,
    pub display_name: String,
    pub match_rules: Vec<RepoMatchRule>,
    pub repos: Vec<RepoId>,
    pub owner: Option<String>,
    pub policy: Option<FamilyPolicyRef>,
    pub aggregate_posture: Posture,
    pub risk_score: u8,
    pub last_event_at: DateTime<Utc>,
}
```

### 9.2 Repo atlas mock

```text
┌ Repo Atlas ─ families 6 ─ repos 42 ─ active 21 ─ blocked 7 ───────────────────┐
│ filter: family=all status!=archived  sort=attention  generated excluded ✓      │
├ Family / Repo       Posture CI        VTI       Cache     Agents Bugs Release ┤
│ veox-*              !       31r 7!    91% 2miss 82% 2!   12/3   38   hold    │
│   veox-deploy       ✖       #9182 ✖   88% 1miss 77% 1!    3/1    7   blocked │
│   veox-enclave      !       #552 ●    94%       81%       4/0    5   none    │
│ redline-*           !       12r 3!    76% 4miss 90% 1!    5/2   19   none    │
│ jeryu               ✓       green     96%       74%       1/0    3   ready   │
│ isolated            ○       idle      -         62%       0/0    2   none    │
├ Selected veox-deploy ────────────────────────────────────────────────────────┤
│ blocker: unsigned wasm artifact ART-392; queue loss 6m; last green main 42m   │
│ next: retry sign job #774, then canary telemetry gate                         │
└ Enter repo  → drill  f family details  w workflows  q queue  b bugs  e proof ┘
```

### 9.3 Repo row fields

Each repo row must show:

- repo ID/slug/provider project ID
- family
- default branch and current tracked branch
- last local head and remote head
- dirty state
- last green SHA
- active pipelines and top pipeline status
- queued/running/failed jobs
- VTI confidence and selector misses
- cache hit ratio and taints
- active/blocked agents
- bug counts by lane
- MR/PR counts and mergeability
- Jankurai score/trend/minimum
- security posture
- signed artifact readiness
- release state
- freshness

### 9.4 Family detail panels

Family detail includes:

1. **Family topology:** repos, shared libraries, dependency edges, release trains.
2. **Family work:** active pipelines/MRs/bugs/agents.
3. **Family bottlenecks:** queue, cache, VTI, release gates, security.
4. **Family quality:** Jankurai, churn, flake, security trend.
5. **Family actions:** sync, standardize, run audit, assign agents, pause low-priority work.

---

## 10. Repo dashboard

### 10.1 Purpose

A repo dashboard answers:

- Is this repo green, risky, blocked, stale, or releasable?
- What is the active workflow and critical path?
- What changed recently?
- What bugs/agents/MRs/releases/security findings are attached?
- What should happen next?

### 10.2 Mock

```text
┌ Repo veox-deploy ─ family veox-* ─ main a40f911 ─ local a91f2bc ─ drift +4 ┐
│ CI #9182 ✖ sign failed  Release hold  VTI 88%  Cache 77%  Bugs 7  Agents 3 │
├ Overview cards ────────────────────────────────────────────────────────────┤
│ Workflow    build ✓ test ✓ sign ✖ canary … prod □  ETA policy 18m          │
│ Git/MR      MR !184 draft, approvals 1/2, unresolved 3, policy SHA stale   │
│ Cache       312GiB family, crates 141GiB, target 98GiB, taints 1           │
│ VTI         142 selected, 319 skipped, miss auth_e2e, saved 38m            │
│ Agent       release-1 waiting grant, fixer-7 running, racer-2 comparing    │
│ Security    wasm unsigned, secret denied 1, SAST green, deps green         │
├ Hot timeline ───────────────────────────────────────────────────────────────┤
│ 03:31 push agent/a17  03:34 pipeline 9182  03:39 sign failed  03:42 VTI miss│
└ w workflow  l logs  m MR  b bugs  a agents  c cache  v VTI  e evidence ───┘
```

### 10.3 Repo sub-tabs

Within a repo, `[` and `]` cycle:

```text
Overview | Workflow | Jobs | Logs | MR/PR | Bugs | Agents | VTI | Cache | Security | Artifacts | Release | Git | Evidence | Settings
```

### 10.4 Repo dashboard widget contract

Every widget exposes:

```rust
pub struct WidgetCard {
    pub id: WidgetId,
    pub title: String,
    pub entity: Option<EntityRef>,
    pub severity: Severity,
    pub summary: String,
    pub metrics: Vec<Metric>,
    pub freshness: SourceFreshnessSet,
    pub local_actions: Vec<ActionRef>,
    pub drill_route: Option<Route>,
}
```

---

## 11. Workflow Atlas and pipeline DAG

### 11.1 Goal

The Workflow Atlas is the TUI's most cinematic screen. It must show many pipelines across many repos, then drill into a single repo/pipeline DAG, then job details and live traces.

It must support:

- fleet-wide active pipeline board;
- family/repo scoped pipeline DAGs;
- child/downstream pipelines;
- stage barriers and `needs` edges;
- critical path highlighting;
- active log annotations;
- release gates as nodes;
- cache and VTI overlays;
- ETA confidence and stale source markers.

### 11.2 DAG edge types

| Edge | Meaning | Data source |
|---|---|---|
| `needs` | explicit GitLab job dependency | GitLab CI config/pipeline jobs |
| `stage_barrier` | stage-order dependency | job stages |
| `artifact` | artifact consumed by downstream job | artifact metadata/config |
| `child_pipeline` | parent → child pipeline | GitLab downstream pipeline API |
| `cache_material` | cache/material object dependency | SmartCache/material DB |
| `vti_plan` | selected tests derive from plan | test plan DB |
| `release_gate` | release/canary/prod gate dependency | release attempt state |
| `approval_gate` | human/policy approval | action/admission/release policy |
| `security_gate` | SAST/secret/dependency/artifact signature | scan/artifact/security data |

### 11.3 Workflow Atlas mock

```text
┌ Workflow Atlas ─ live pipelines 28 ─ failed 4 ─ blocked 6 ─ critical sorted ┐
│ family filter all  view swimlanes  overlay critical-path+cache+VTI          │
├ veox-* ─────────────────────────────────────────────────────────────────────┤
│ veox-deploy #9182  build ✓──test ✓──sign ✖──canary …──prod □   blocker ART │
│ veox-api    #552   plan ✓──unit ✓──auth ●──package □──deploy □ ETA 12m     │
│ veox-ui     #817   lint ✓──test ✓──bundle ✓──sign ✓──release ● ETA 4m     │
├ redline-* ──────────────────────────────────────────────────────────────────┤
│ redline-db  #774   fmt ✓──unit ✓──btree ●━━━━━━━stress □  p95 high        │
│ redline-ui  #211   lint ✓──build ✓──test ✖  capsule CAP-238               │
├ Inspector: selected veox-deploy sign job #774 ─────────────────────────────┤
│ status failed  reason missing provenance  runner rust-12  cache hit 71%    │
│ trace tail: `cosign attest: no predicate file`  evidence ART-392 CAP-9912  │
└ ↑↓ select  →/Enter pipeline  l logs  e evidence  c cache  v VTI  y why ────┘
```

### 11.4 Single pipeline DAG mock

```text
┌ Pipeline #9182 veox-deploy main a91f2bc ─ status blocked ─ ETA policy 18m ┐
│ physics 7m fleet 11m policy 18m | critical path: sign → canary → telemetry │
├ DAG ───────────────────────────────────────────────────────────────────────┤
│  [plan ✓]──┬──[build-api ✓]────┬──[test-api ✓]────┬──[sign-wasm ✖]──[canary …] │
│            │                   │                  │                          │
│            ├──[build-ui ✓]─────┴──[test-ui ✓]─────┘                          │
│            └──[jankurai ✓]──────[security ✓]──────[artifact-provenance ✖]     │
├ Node detail ─────────────────────────────┬ Live trace / annotations ───────┤
│ sign-wasm job #774 failed                │ 03:39 cosign: missing predicate │
│ runner rust-12 pool release              │ 03:39 expected provenance.json  │
│ cache material wasm-bundle ✓             │ annotation: artifact gate fail  │
│ evidence ART-392 CAP-9912                │ suggested: retry provenance job │
└ Enter node  l logs  a artifacts  r retry preview  e evidence  Esc repo ────┘
```

### 11.5 Job detail

Job detail has sub-tabs:

```text
Summary | Live Trace | Annotations | Artifacts | Cache | VTI | Runner | Capsule | Actions | Raw JSON
```

Job detail fields:

- project ID, pipeline ID, job ID
- job name, stage, status, allow failure
- queued duration, run duration, start/finish
- runner ID/system ID/description/tags/pool/node
- ref/SHA/MR link
- trace cursor and byte count
- log annotations and failure signature
- artifacts and report files
- cache verdicts and material objects
- VTI selected tests and skipped tests relevant to job
- evidence capsules
- retry/cancel/play actions

### 11.6 Live trace viewer

Requirements:

- stream via WebSocket/SSE if available;
- fallback to polling with byte-range cursor;
- highlight errors, warnings, file paths, test names, artifact paths;
- show annotations in side rail;
- support follow mode and frozen mode;
- preserve scroll position while appending;
- support search, regex, severity filter, copy selected lines;
- link log lines to evidence/capsule when generated.

Keys:

| Key | Log behavior |
|---|---|
| `f` | follow/unfollow tail |
| `/` | search in log |
| `n` / `N` | next/previous search hit |
| `a` | toggle annotations |
| `e` | evidence for selected annotation |
| `c` | copy selected line/span |
| `[` / `]` | previous/next annotation |
| `Esc` | back to job/pipeline |

---

## 12. SmartCache observatory

### 12.1 Purpose

The cache page answers:

- Are we full?
- What categories consume storage?
- Which objects are hottest?
- Which misses cost the most time?
- Are any objects tainted or denied?
- Are Rust crates, build targets, OCI layers, sccache, artifacts, or material objects dominating usage?
- What can be safely garbage-collected?

### 12.2 Cache categories

Minimum categories:

- Cargo registry index
- Cargo crate downloads
- Cargo git checkouts
- Cargo target/build outputs
- sccache objects
- Docker/OCI layers
- GitLab job artifacts
- GitLab release artifacts
- nextest/test result archives
- VTI/test selector caches
- Jankurai audit artifacts
- material/CAS objects
- action cache objects
- temporary downloads
- unknown/unclassified

### 12.3 Cache mock

```text
┌ SmartCache Observatory ─ 366/400GiB 91% ! ─ hit 82% ─ taints 2 ─ saved 71h ┐
│ proxy 19800 ✓ registry 19801 ✓ upstream crates.io p95 182ms  singleflight 41│
├ Usage by category ──────────────┬ Hot / risky objects ─────────────────────┤
│ cargo crates     141GiB ███████ │ crate serde-1.0.203 hits 918 trust ✓     │
│ target dirs       98GiB █████   │ target veox-api/debug 41GiB stale 6d     │
│ OCI layers        62GiB ███     │ layer sha256:ab12 tainted: base drift    │
│ artifacts         44GiB ██      │ artifact wasm v1.2 unsigned quarantine   │
│ sccache           17GiB █       │ toolchain fp mismatch rust-1.87 vs 1.88  │
├ Misses / verdicts ──────────────┴ GC plan ─────────────────────────────────┤
│ miss storm: redline-db crates 8m lost       reclaimable 72GiB safe 39GiB    │
│ denied: material object tainted by force-refresh rule                       │
└ Enter object  g GC preview  t taints  h hot  p provenance  r refresh ──────┘
```

### 12.4 Cache object drilldown

Cache object detail must show:

- key/digest/namespace/category
- size and last access
- hit count, miss count, recent request samples
- mutability/trust tier
- material aliases
- toolchain fingerprint
- leases and active protection
- taints and verdict history
- promotions
- source URL/template, redacted if necessary
- linked jobs/pipelines/repos
- GC eligibility and safe reclaim preview

### 12.5 Cache actions

| Action | Safety |
|---|---|
| Refresh summary | read-only |
| Inspect object provenance | read-only |
| Preview GC | read-only/dry-run |
| Execute GC safe set | low mutation, confirmation |
| Quarantine object | medium mutation, evidence required |
| Clear taint | medium/high, requires proof and policy |
| Force refresh namespace | high, preview blast radius |
| Change cache budget/config | config edit branch/MR by default |

---

## 13. VTI smart test skipper cockpit

### 13.1 Purpose

The VTI page must prove whether smart test skipping is working safely.

It answers:

- How many tests did VTI select vs skip?
- How much time did it save?
- What is confidence by repo/subsystem?
- Which selector misses occurred?
- Which skipped tests later failed?
- What mappings need learning?
- When did VTI correctly fall back to full tests?

### 13.2 VTI mock

```text
┌ VTI / Tests ─ last 24h ─ saved 83h ─ confidence 91% ─ misses 3 ! ┐
│ selected 12,441 skipped 88,902 accelerated 74% fallback 9%        │
├ Repo scorecard ─────────────┬ Selector misses ───────────────────┤
│ veox-api     88% ! saved 31h│ auth_e2e missed src/auth/token.rs  │
│ veox-ui      96% ✓ saved 22h│ enclave_boundary missed ffi.rs    │
│ redline-db   76% ! saved 9h │ btree_delete missed planner.rs     │
│ jeryu        94% ✓ saved 8h │                                  │
├ Selected plan detail ────────────────────────────────────────────┤
│ plan TP-882 repo veox-api base a40f911 head a91f2bc               │
│ selected 142 skipped 319 confidence 0.88 reason changed auth/api  │
│ guardrail: full auth suite forced due recent miss                 │
└ Enter plan  m misses  l learn  a audit  f force-full preview ─────┘
```

### 13.3 VTI metrics

| Metric | Meaning |
|---|---|
| selected tests | tests chosen for run |
| skipped tests | tests skipped by confidence |
| acceleration ratio | skipped / total eligible |
| saved wall time | estimated wall-clock saved against full plan |
| saved runner time | runner-minutes saved |
| selector miss rate | misses / plans over window, severity-weighted |
| false skip count | skipped test that later should have run |
| fallback rate | plans forced to full by guardrails |
| confidence | model confidence per repo/subsystem |
| stale mapping count | mappings older than threshold or changed subsystem |

### 13.4 VTI guardrails

The TUI must never encourage blind skipping. Guardrail states:

| State | UI label | Behavior |
|---|---|---|
| high confidence | `VTI ✓` | normal selected/skipped display |
| moderate confidence | `VTI !` | show reason and audit option |
| recent selector miss | `VTI MISS` | require explanation and likely force broader tests |
| critical subsystem | `VTI GUARDED` | full or expanded suite unless policy allows |
| stale mappings | `VTI STALE` | learning/audit action recommended |
| source stale | `VTI UNKNOWN` | do not claim saved time as trusted |

### 13.5 VTI drilldown

Plan detail should show:

- base/head SHA
- changed files and affected subsystems
- selected test IDs, reasons, confidence
- skipped test IDs, reasons, confidence
- external selector input if used
- selector miss history
- related failures and bugs
- cache status for test artifacts
- guardrail decisions
- learning updates
- evidence receipt

---

## 14. Agents and autonomous workflows

### 14.1 Purpose

The Agents cockpit answers:

- Which agents are running?
- What are they doing?
- Which grants do they have?
- Which branches/MRs/pipelines/logs/evidence are attached?
- Which agents are blocked on approval, budget, stale policy, failed CI, missing config, or safety gates?
- Which patch races are running and who is winning?
- Can I safely edit autonomous workflow configs?

### 14.2 Agents mock

```text
┌ Agents ─ active 18 ─ blocked 5 ─ racing 3 ─ budget $42.31 today ─ kill bell armed ✓ ┐
│ filter: all repos  sort=attention  grants expiring 2  provider OpenAI✓ Anthropic✓    │
├ Agents ───────────────────────────────┬ Selected agent release-1 ───────────────────┤
│ id          repo        task        state      age  budget  grant  branch            │
│ release-1   veox-deploy sign fix    ⏸ grant   12m  $1.92   prod?  agent/sign-fix    │
│ fixer-7     redline-db  BUG-219     ● test     38m  $6.10   task   agent/bug-219    │
│ racer-2     veox-api    BUG-183     ● race     9m   $3.40   task   race/auth-a      │
│ auditor-3   veox-*      jankurai    ! blocked  4m   $0.80   none   -               │
├ Timeline / steps ─────────────────────┼ Grants / evidence / actions ────────────────┤
│ 03:31 started release workflow         │ grant needed: artifact signing path          │
│ 03:34 opened MR !184                   │ evidence E-9912 CAP-774 ART-392              │
│ 03:39 sign job failed                  │ actions: grant, deny, pause, open config     │
└ Enter detail  l logs  g grants  k pause/kill  c config  r race  m merge  e proof ───┘
```

### 14.3 Agent lifecycle data to add

```sql
agent_sessions(
  session_id text primary key,
  actor text not null,
  repo text,
  family text,
  task_kind text,
  task_id text,
  status text,
  branch text,
  base_sha text,
  head_sha text,
  started_at timestamp,
  updated_at timestamp,
  heartbeat_at timestamp,
  budget_json text,
  provider_chain text,
  sandbox_path text,
  correlation_id text,
  evidence_id text
);

agent_steps(
  step_id text primary key,
  session_id text not null,
  seq integer,
  kind text,
  status text,
  summary text,
  started_at timestamp,
  finished_at timestamp,
  tool_call_id text,
  pipeline_id integer,
  evidence_json text,
  log_cursor integer
);

agent_events(
  id text primary key,
  session_id text,
  ts timestamp,
  kind text,
  severity text,
  summary text,
  payload_json text,
  evidence_refs_json text
);
```

### 14.4 Autonomous workflow config editor

Editable surfaces:

- `.agents/*` profiles
- `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, or equivalent repo guidance
- `.jeryu/autonomy/*`
- provider chains and LLM budget configs
- proof lane config
- release policy config
- repo VTI config
- runner pool config
- cache budget/config
- security/admission policy files

Flow:

```text
select config → read-only preview → edit buffer → schema validate → policy lint
→ render diff → dry-run → create branch/MR by default → optional direct apply if low-risk and allowed
```

Rules:

1. Secret values are never displayed; names/fingerprints/paths only.
2. High-risk config changes require human approval.
3. Default apply creates a branch/MR, not direct main write.
4. The preview must list impacted workflows, agents, repos, risk tier, and rollback path.
5. Every accepted edit creates an evidence event.

### 14.5 Patch racing visualization

Patch races should show competing hypotheses as lanes:

```text
┌ Race BUG-183 auth timeout ─ base main a40f911 ─ hypotheses 4 ─ leading h2 ┐
│ h1 token-refresh        CI ● 63%  tests 48/142  jankurai ✓  risk med      │
│ h2 connection-pool      CI ● 81%  tests 120/142 jankurai ✓  risk low ★    │
│ h3 retry-backoff        CI ✖ CAP-883 failure integration/auth             │
│ h4 config-timeout       queued 4m due rust pool                            │
├ Winner criteria: CI green + VTI guarded tests + Jankurai >= min + no sec   │
└ Enter lane  l logs  e evidence  w select winner preview  c cleanup losers ┘
```

---

## 15. Bugs/issues cockpit

### 15.1 Purpose

The bug cockpit is a cross-repo accountability board.

It must answer:

- What bugs exist across all repos?
- Which are ready for agents?
- Which are in progress?
- Which are blocked and why?
- Which attempts have failed?
- Which fixes are awaiting review?
- Which bugs were fixed by which commits/MRs/releases?

### 15.2 Bug lanes

Default lanes:

```text
needs_triage | needs_info | accepted | ready | in_progress | blocked | fix_proposed | reviewing | verifying | done
```

Also supported:

```text
duplicate | invalid | cannot_reproduce | wont_do
```

Attempt statuses:

```text
pending | started | failed | fix_proposed | verified | abandoned
```

### 15.3 Bugs mock

```text
┌ Bugs / Issues Fleet ─ open 184 ─ ready 31 ─ in_progress 18 ─ review 9 ─ done 422 ┐
│ sort: rank  filter: status!=done  agents: all  external sync GitLab✓ GitHub!     │
├ Queue ───────────────────────────────────────────────────────────────────────────┤
│ id       sev prio repo        status       owner      attempts age  title         │
│ BUG-293  S1  P0   veox-api    in_progress  agent-a17  2        3h   auth timeout  │
│ BUG-301  S2  P1   redline-db  ready        -          1        1d   index plan    │
│ BUG-188  S0  P0   deploy      blocked      agent-a09  4        2d   rollback bug  │
│ BUG-275  S3  P2   veox-ui     reviewing    agent-a31  1        5h   nav flicker   │
├ Selected BUG-293 ────────────────────────────────────────────────────────────────┤
│ current: integration auth handshake times out under load                          │
│ expected: token refresh before 1500ms; acceptance: auth_integ passes              │
│ attempts: #1 failed CI #882, #2 running branch agent/a17/bug-293                  │
│ evidence: CAP-9917, reproduction, linked MR !184                                  │
└ Enter detail  A assign agent  R retry attempt  L logs  E evidence  O open issue ─┘
```

### 15.4 Bug detail

Bug detail must show:

- canonical report: title, component, current/expected behavior, environment, frequency, impact, security/privacy, no-secrets confirmation, reproduction steps, evidence, acceptance criteria, severity, priority, difficulty;
- source and target projects;
- status, owner, labels, external refs;
- events timeline;
- attempts: agent, status, sandbox path, branch, base/head SHA, PR/MR URL, CI evidence, notes, timestamps;
- linked bugs and project edges;
- evidence paths/URLs/digests/redaction flags;
- linked commits and release versions;
- rollback/revert status.

### 15.5 Bug actions

| Action | Risk |
|---|---|
| Filter/sort/search | read-only |
| Mark status/severity/priority | low mutation, preview compact |
| Assign/spawn agent | medium, grant/budget preview |
| Retry failed attempt | medium, CI cost preview |
| Open MR/PR | branch/MR mutation, preview diff |
| Request merge | high, typed confirmation if merge gate |
| Mark done | low/medium, requires linked evidence |

---

## 16. Git sync and remote state

### 16.1 Purpose

The Git sync screen answers:

- Are local repos, sidecar mirrors, and remote hosts in sync?
- What is the last successful merge to main?
- What was the last PR/MR attempt?
- Are hooks installed and enforcing policy?
- Are there rejected pushes/admission denials?
- Are risk approvals, command artifacts, and backups present?

### 16.2 Git sync mock

```text
┌ Git Sync Fleet ─ tracked repos 42 ─ synced 37 ─ drift 4 ─ broken 1 ───────────┐
│ admission allow 184 audit 12 deny 3  mirror lag p95 18s  signed git artifacts 98%│
├ repo          local head remote main last green merge  last PR/MR     mirror │
│ veox-api      a91f2bc    a40f911     a40f911 42m ago   !184 failed   ⚠ lag  │
│ veox-ui       f39d902    f39d902     f39d902 12m ago   !181 merged   ✓      │
│ redline-db    b1a81d0    b1a81d0     b1a81d0 1h ago    !77 running   ✓      │
│ jeryu         c2a1e19    c2a1e19     c2a1e19 8m ago    none         ✓      │
├ Selected veox-api ───────────────────────────────────────────────────────────┤
│ dirty no, branch agent/a17/bug-293, last denied ref main non-fast-forward    │
│ grant mismatch actor agent-a09; mirror job MIR-221 retrying network timeout  │
└ Enter repo  p PR/MR  a admission  m mirrors  h hooks  E evidence  / filter ┘
```

### 16.3 Data sources

- `tracked_repositories`
- `git_command_events`
- `git_ref_updates`
- `git_mirror_jobs`
- `git_risk_approvals`
- `git_command_artifacts`
- `admission_decisions`
- GitHost PR/MR adapter
- local repo filesystem status
- remote node state

Add a `last_successful_main_merge` materialized view for quick rendering.

---

## 17. CI bottleneck lab

### 17.1 Purpose

The bottleneck lab is an optimization screen, not a log viewer. It answers:

- Which jobs dominate wall clock?
- Which jobs dominate runner time and cost?
- Which jobs are flaky?
- Which jobs queue longest due to pool/tag constraints?
- Which caches/artifacts slow jobs?
- Which jobs should be split, parallelized, cached, or VTI-skipped?

### 17.2 Mock

```text
┌ CI Bottlenecks ─ scope fleet ─ last 7d ─ ranked by critical-path impact ─────┐
│ total runner time 913h  wasted obsolete 71h  queue wait 144h  cache misses 38h│
├ rank job/stage              repo       p50    p95    queue  fail impact action│
│ 1    integration/auth       veox-api   8m10   21m04  2m41   9%   18h   split │
│ 2    btree/delete-stress    redline-db 14m02  39m08  0m44   3%   14h   cache │
│ 3    jankurai/full-audit    veox-*     5m12   16m00  4m02   1%   11h   shard │
│ 4    docker-build-api       veox-api   6m31   18m07  5m13   2%   10h   prepull│
├ Selected integration/auth ──────────────────────────────────────────────────┤
│ critical in 37% pipelines; queue due tag=gpu-runner; suggestions: add 2 default│
│ runners, split auth_e2e, cache postgres fixture, teach VTI src/auth.rs mapping│
└ Enter job history  s suggestions  p examples  a create issue  w what-if ────┘
```

### 17.3 Suggestion rules

| Pattern | Suggestion |
|---|---|
| High p95/p50 ratio | Flake or unstable environment; inspect logs/runner/node. |
| High queue, low CPU | Add eligible runners or adjust tags. |
| High queue, saturated CPU | Add nodes/runners. |
| High cache miss cost | Pin/prefetch cache, remove force refresh, inspect taints. |
| Serial stage barrier | Add `needs`, split workflow, or shard job. |
| Long test job, low VTI confidence | Improve test mapping or split tests. |
| Obsolete work | Cancel superseded pipelines automatically. |
| Repeated failure capsule | Create/assign bug; prevent retry storms. |

---

## 18. Jankurai audit center

### 18.1 Purpose

Jankurai is the quality/audit/score plane across repos. The TUI must answer:

- What is the current score per repo/family?
- Is the score above required minimum?
- What caps the score?
- Which rule families are failing?
- Which duplicate code or architectural jank hotspots exist?
- Which commits introduced regressions?
- Which repairs are queued or assigned to agents?
- Which Jankurai version is installed per repo?

### 18.2 Data model

```rust
pub struct JankuraiRepoStatus {
    pub repo: RepoId,
    pub score: Option<u8>,
    pub min_required: Option<u8>,
    pub trend_delta: Option<i32>,
    pub auditor_version: Option<String>,
    pub expected_version: Option<String>,
    pub stale_version: bool,
    pub cap: Option<JankuraiCap>,
    pub proof_artifacts: Vec<ProofArtifactRef>,
    pub rule_family_counts: BTreeMap<String, FindingCounts>,
    pub duplicate_hotspots: Vec<DuplicateHotspot>,
    pub generated_zones: Vec<GeneratedZone>,
    pub security_boundaries: Vec<SecurityBoundaryFinding>,
    pub repair_queue: Vec<JankuraiRepair>,
    pub last_audit_at: Option<DateTime<Utc>>,
}
```

Proposed tables:

```sql
jankurai_audits(audit_id, repo, commit_sha, score, min_required, level,
                auditor_version, generated_at, artifact_path, digest,
                trend_delta, cap_json);

jankurai_findings(finding_id, audit_id, severity, category, rule_id,
                  path, line, summary, detail, duplicate_group_id,
                  suggested_action, owner, status);
```

### 18.3 Mock

```text
┌ Jankurai Audit Center ─ avg 88.7 ▲ +1.8 ─ below min 5 ─ stale version 3 ┐
│ policy min 85  high-level HL3  dominant version 1.5.1                    │
├ repo          score trend version min cap        dupes issues last audit │
│ jeryu         89    ▲+4  1.5.1   85  docs drift  12    18     8m ago     │
│ veox-api      83    ▼-7  1.5.1   85  duplicate   41    77     13m ago    │
│ veox-deploy   91    ▲+1  1.5.1   85  none        8     12     4m ago     │
│ redline-db    76    ▼-9  1.4.9!  85  complexity  22    81     1h ago     │
├ Selected veox-api ───────────────────────────────────────────────────────┤
│ cap duplicate-code: auth middleware copied in 4 modules; repair agent ready│
│ proof artifacts: audit.json, duplicate_map.svg, score_history.csv         │
└ Enter finding  r repair  A assign agent  h history  v version matrix ────┘
```

### 18.4 Required event kinds

- `jankurai.audit.started`
- `jankurai.audit.completed`
- `jankurai.score.changed`
- `jankurai.finding.opened`
- `jankurai.finding.closed`
- `jankurai.cap.changed`
- `jankurai.version.drift`
- `jankurai.repair.queued`
- `jankurai.repair.completed`

---

## 19. Runners, pools, nodes, and system utilization

### 19.1 Purpose

The utilization screen answers:

- Are runner pools healthy?
- Are we CPU/memory/disk/network constrained?
- Are remote nodes healthy?
- Are managers stuck, OOMing, draining, or cold-starting?
- Which pool/tag/trust tier gates jobs?
- Which scale action would help?

### 19.2 Mock

```text
┌ Utilization ─ useful 88% ─ queued 42 ─ managers 31/40 ─ nodes 6 ─────────────┐
│ pool        state  slots busy ready q_p95 warm cold oom disk node-affinity   │
│ rust        ● hot  30    28   28    6m10  4    2    0   61%  any             │
│ gpu         ○ cool 5     2    2     1m02  1    0    0   44%  gpu-a           │
│ secure      ! sat  4     4    8     9m41  1    3    1   83%  enclave-only    │
│ release     ● hot  1     1    1     0m33  1    0    0   51%  release         │
├ Node detail ───────────────────────┬ Manager events ────────────────────────┤
│ node rust-12 CPU 94% MEM 71% DISK 61│ 03:42 manager m-17 die exit=137 OOM    │
│ docker ✓ ssh 21ms managers 8        │ 03:43 reconcile created manager m-22   │
└ Enter pool/node  s scale preview  p pause  d drain  r restart  l logs ──────┘
```

### 19.3 Metrics to plumb

- per-node CPU/memory/disk/network
- per-container CPU/memory/network/block IO
- Docker daemon health
- manager restart count
- OOM kills
- image pull latency
- warm/cold manager counts
- queue depth by eligible pool/tag/trust tier
- remote node heartbeat age
- SSH latency
- runner version/config hash
- disk usage by cache namespace

---

## 20. Security, secrets, policy, and supply chain

### 20.1 Purpose

Security is not a hidden tab. It gates merge and release posture.

The security screen covers:

- Vault and secret lifecycle
- secret access denials and audit events
- SAST/dependency/container/secret scans
- admission/pre-receive decisions
- policy violations and drift
- signed artifacts and SBOM/provenance
- cache taints and material trust
- Jankurai security boundaries
- capability grants and high-risk actions

### 20.2 Security mock

```text
┌ Security Center ─ safe merge ! ─ safe release ✖ ─ Vault healthy ✓ ──────────┐
│ policy v4  admission enforce on  secrets redacted  high findings 1          │
├ Findings ──────────────────────────┬ Gates ────────────────────────────────┤
│ ✖ veox-deploy wasm unsigned ART-392 │ release gate artifact ✖               │
│ ! veox-api secret denied agent-2    │ merge gate MR !184 approvals 1/2      │
│ ! redline-db dependency high CVE-x  │ cache material trust taint 2          │
│ ✓ jeryu SAST clean                 │ Vault sealed=no token=present         │
├ Secret audit tail ─────────────────────────────────────────────────────────┤
│ 03:31 denied repo=veox-web actor=agent-2 path=prod/api reason=scope          │
└ Enter finding  V vault  G grants  A admission  J Jankurai  P policy  E proof┘
```

### 20.3 Redaction rules

- Never display plaintext secrets.
- Display secret path, mount, authority, status, TTL, expiry, fingerprint, and audit stats only.
- Redact tokens in URLs/logs.
- Copy-to-clipboard is disabled for secret-adjacent values unless explicitly safe.
- Screenshots/captures must apply the same redaction path.

### 20.4 Secret lifecycle fields

- Vault address/status/initialized/sealed/healthy/token-present
- KV mount and prefix
- bootstrap metadata path
- release secret set repo/version/target/status
- rendered deploy/runtime env path
- audit/report/bundle path
- Vault secret paths
- expiry
- rotation/finalization timestamp
- audit events: action/status/detail/actor/timestamp

---

## 21. Signed artifacts and provenance

### 21.1 Purpose

The artifact screen answers:

- What artifacts were built?
- Which are signed?
- Which have SBOM and provenance?
- Which release version contains them?
- Which pipeline/job produced them?
- Which cache/material object backed them?
- Are any unsigned/expired/tainted/quarantined?

### 21.2 Artifact mock

```text
┌ Artifacts / Provenance ─ repos 42 ─ signed 97% ─ unsigned 3 ─ quarantined 2 ┐
│ artifact              repo         version    digest      sig sbom prov gate │
│ veox-api-linux-amd64   veox-api     v1.2.0     sha256:a1   ✓   ✓    ✓    ✓   │
│ veox-ui.wasm           veox-deploy  v1.2.0     sha256:b2   ✖   ✓    ✖    ✖   │
│ redline-db             redline-db   nightly    sha256:c3   ✓   ✓    ✓    !   │
├ Selected veox-ui.wasm ──────────────────────────────────────────────────────┤
│ produced by pipeline #9182 job #774 runner rust-12; provenance missing       │
│ release blocked; rollback target v1.1.7 verified; evidence ART-392           │
└ Enter artifact  s sign preview  p provenance  b SBOM  r rebuild  E evidence ┘
```

### 21.3 Artifact attestation schema

```rust
pub struct ArtifactAttestation {
    pub artifact_id: String,
    pub repo: RepoId,
    pub family: Option<String>,
    pub version: Option<String>,
    pub kind: ArtifactKind,
    pub uri: String,
    pub digest: String,
    pub source_sha: Sha,
    pub pipeline_id: Option<i64>,
    pub job_id: Option<i64>,
    pub runner_id: Option<String>,
    pub sbom_uri: Option<String>,
    pub provenance_uri: Option<String>,
    pub signature_uri: Option<String>,
    pub verification_status: VerificationStatus,
    pub evidence: Vec<EvidenceRef>,
    pub created_at: DateTime<Utc>,
}
```

---

## 22. Release, canary, production, rollback, and version control

### 22.1 Purpose

The release screen answers:

- What is the current release candidate?
- What is in canary/prod?
- Which gates are blocking promotion?
- Is rollback ready?
- Which artifacts/secrets/policies/evidence support release?
- Why did automation act or not act?

### 22.2 Release mock

```text
┌ Release Train veox v1.2.0 ─ canary 25% ─ prod hold unsigned wasm ───────────┐
│ latest stable v1.1.7  last prod deploy 2h ago  rollback target v1.1.7 ready ✓│
├ Release flow ───────────────────────────────────────────────────────────────┤
│ candidate ✓ → dry-run ✓ → sign ! → canary ● 25% → telemetry … → prod □      │
│ gates: CI ✓ VTI ✓ security ! artifact ✖ secrets ✓ rollback ✓ approval □     │
├ Gate detail ──────────────────────────┬ Rollback ───────────────────────────┤
│ artifact gate failed: wasm unsigned   │ target v1.1.7 verified ✓            │
│ SBOM present, provenance missing      │ rollback drill 18h ago ✓            │
│ action: retry sign job #774           │ data migration reversible ✓         │
│ evidence ART-392 JOB-774 trace        │ command requires prod approval      │
└ Enter gate  s retry sign  p promote  R rollback  d doctor  w watch  E proof ┘
```

### 22.3 Release automation rules

Autopilot is allowed only when policy permits and proof is complete.

Auto-promote only if:

- release policy allows automatic promotion;
- all gates green;
- no freeze window blocks the risk tier;
- artifacts signed and provenance/SBOM verified;
- canary telemetry green for required duration;
- secrets finalized/valid;
- required approvals present;
- no active kill bell pause;
- source freshness acceptable.

Auto-rollback only if:

- policy allows automatic rollback;
- health regression crosses threshold;
- rollback target verified;
- migration safety clear;
- rollback evidence/passport exists;
- kill bell not paused;
- required approval present if risk tier demands.

The TUI must always show why automation did or did not act.

---

## 23. Evidence and audit ledger

### 23.1 Purpose

The proof ledger is the trust backbone. It must answer:

- Why did this happen?
- Who/what acted?
- Which source facts were used?
- Which action was previewed, approved, executed, denied, or rolled back?
- Which evidence supports a merge/release/VTI/cache/security decision?
- Was any source stale?

### 23.2 Query dimensions

```text
entity kind/id
repo/family
actor/agent/human
severity
event kind
action id
request id / correlation id
SHA/ref/branch/MR/pipeline/job
release version/artifact digest
since/until timestamp
source freshness / stale flag
```

### 23.3 Proof timeline object

```rust
pub struct ProofTimelineItem {
    pub id: String,
    pub ts: DateTime<Utc>,
    pub seq: u64,
    pub kind: ProofKind,
    pub severity: Severity,
    pub entity: EntityRef,
    pub actor: Option<ActorRef>,
    pub summary: String,
    pub detail: String,
    pub evidence_refs: Vec<EvidenceRef>,
    pub source_refs: Vec<SourceRef>,
    pub action_ref: Option<ActionRef>,
    pub correlation_id: Option<String>,
    pub redacted: bool,
}
```

### 23.4 Evidence mock

```text
┌ Proof Ledger ─ query entity=release:v1.2.0 ─ items 42 ─ cursor 183921 ┐
│ 03:31 pipeline #9182 created sha a91f2bc source GitLab✓               │
│ 03:34 VTI plan TP-882 selected 142 skipped 319 confidence .88 E-881   │
│ 03:39 artifact gate failed wasm unsigned ART-392 CAP-9912             │
│ 03:40 agent release-1 requested signing grant GR-771 risk production  │
│ 03:41 grant denied reason path scope mismatch admission AD-221        │
├ Selected ART-392 ─────────────────────────────────────────────────────┤
│ artifact veox-ui.wasm sha256:b2; SBOM present; provenance missing      │
│ source job #774 trace line 391; release gate artifact blocked          │
└ / search  Enter item  o open path  c copy digest  r raw  Esc release ┘
```

### 23.5 Required endpoint/resource

```http
GET /api/proof?entity=&kind=&since=&actor=&cursor=&limit=
```

```text
jeryu://proof?entity=repo:veox-deploy&since=24h
jeryu.search_proof_timeline
```

---

## 24. Code churn, CI economics, and cost/waste

### 24.1 Purpose

The metrics/churn lens answers:

- How much code changed by repo/family/agent/human/time?
- Which hot paths changed?
- Which churn correlates with CI failures, VTI misses, Jankurai regressions, or security findings?
- How much runner time, cache time, and LLM spend were saved or wasted?
- Which obsolete pipelines or retries burned capacity?

### 24.2 Churn mock

```text
┌ Code Churn ─ last 7d ─ +184k -92k ─ generated excluded 41% ─ risk rising ! ┐
│ repo/family       commits   +lines   -lines   files  fail corr  jankurai Δ │
│ veox-*              184     62,311   22,901   812    0.31       +2         │
│ redline-*            77     91,442   55,201   391    0.58       -6 !       │
│ jeryu                33     11,024    6,911   128    0.12       +4         │
├ Hot paths ─────────────────────────────────────────────────────────────────┤
│ redline-db/src/planner +18k -9k; veox-api/src/auth +4k -1k                 │
│ biggest risk: redline planner churn correlates with btree_delete failures   │
└ Enter path  t trend  a by agent  h by human  c commits  / filter ──────────┘
```

### 24.3 Code churn schema

```rust
pub struct CodeChurnSample {
    pub repo: RepoId,
    pub commit_sha: Sha,
    pub parent_sha: Sha,
    pub actor: String,
    pub actor_kind: ActorKind,
    pub branch: String,
    pub timestamp: DateTime<Utc>,
    pub additions: u64,
    pub deletions: u64,
    pub files_changed: u64,
    pub generated_additions: u64,
    pub generated_deletions: u64,
    pub paths: Vec<PathChurn>,
    pub linked_bug_ids: Vec<String>,
    pub linked_pipeline_ids: Vec<i64>,
    pub linked_jankurai_audits: Vec<String>,
}
```

### 24.4 CI economics metrics

- runner-hours by repo/family/job/agent;
- obsolete pipeline runner-hours;
- queue wait cost;
- cache miss cost;
- VTI time saved;
- cache time saved;
- retry storm cost;
- LLM tokens/cost by repo/agent/bug;
- remote node cost estimate if configured;
- release delay cost if policy data exists.

---

## 25. Runtime profile, settings, and diagnostics

### 25.1 Runtime profile screen

The runtime/settings screen shows:

- JeRyu version, commit SHA, build time, features;
- DB backend/path/profile: SQLite default, RedlineDB optional;
- effective redacted settings;
- configured ports;
- enabled integrations: GitLab, Vault, cache, broker, autonomy, LLMs;
- source freshness map;
- deep health diagnostics;
- docs/schema drift status;
- terminal capability and theme;
- stream/reconnect diagnostics.

### 25.2 Default ports/settings to surface

| Port | Component |
|---:|---|
| `8929` | GitLab HTTP/API |
| `2224` | GitLab SSH |
| `9777` | JeRyu webhook/API health server |
| `9778` | MCP HTTP |
| `18200` | Vault |
| `19800` | SmartCache proxy |
| `19801` | OCI registry mirror |

Important paths/env/headers:

- `~/.jeryu/settings.json`
- `jeryu.env`
- `jeryu.db`
- `runners/`
- `cache/`
- `.jeryu/local/repos`
- `.jeryu/autonomy`
- `GITLAB_PAT`
- `JERYU_WEBHOOK_SECRET`
- `GITLAB_ROOT_PASSWORD`
- `JERYU_RELEASE_REPO_ROOT`
- `JERYU_DATABASE_URL`
- `JERYU_GITLAB_INSECURE_TLS`
- `X-Gitlab-Token`
- `X-Gitlab-Event`
- `X-Gitlab-Webhook-UUID`
- `X-Jeryu-Token`
- custom executor env vars like `CUSTOM_ENV_CI_JOB_ID`

Secret values must never be displayed.

---

## 26. Backend inspection plane

### 26.1 Most important architectural decision

Build a read-only **Inspection Plane** beside the mutating capability plane.

```text
HTTP GET + SSE/WebSocket + MCP Resources + CLI --json + TUI read model
               all backed by the same typed schemas
```

The TUI must consume this plane first. During migration, a `LocalDataClient` may read direct DB/GitLab/Docker state, but the target is one typed contract.

### 26.2 Core contracts

```rust
pub struct TuiReadModel {
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    pub event_cursor: u64,
    pub freshness: SourceFreshnessSet,
    pub mission: MissionSnapshot,
    pub attention: Vec<AttentionItem>,
    pub next_action: Option<ActionDescriptor>,
    pub health: SystemHealth,
    pub dashboards: DashboardIndex,
}

pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
    pub label: String,
    pub repo_id: Option<String>,
    pub family_id: Option<String>,
    pub project_id: Option<i64>,
}

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
}

pub struct TuiEvent {
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub kind: TuiEventKind,
    pub severity: Severity,
    pub entity: EntityRef,
    pub parent: Option<EntityRef>,
    pub repo_id: Option<String>,
    pub family_id: Option<String>,
    pub correlation_id: Option<String>,
    pub summary: String,
    pub fields: serde_json::Value,
    pub evidence_refs: Vec<EvidenceRef>,
    pub next_actions: Vec<ActionDescriptor>,
    pub source: DataSourceId,
}
```

### 26.3 Entity kinds

```rust
pub enum EntityKind {
    System,
    RepoFamily,
    Repo,
    Project,
    Branch,
    Commit,
    MergeRequest,
    Workflow,
    Pipeline,
    ChildPipeline,
    WorkflowNode,
    Job,
    JobTrace,
    Runner,
    Pool,
    RunnerManager,
    RemoteNode,
    CacheObject,
    CacheTaint,
    CacheVerdict,
    CacheLease,
    MaterialObject,
    TestPlan,
    TestCase,
    VtiDecision,
    VtiMiss,
    Agent,
    AgentSession,
    AgentStep,
    AgentRace,
    Grant,
    CapabilityIntent,
    AdmissionDecision,
    Bug,
    BugAttempt,
    JankuraiAudit,
    JankuraiFinding,
    SecurityFinding,
    SecretAuthority,
    SecretSet,
    SecretAccess,
    Artifact,
    Signature,
    Sbom,
    Provenance,
    ReleaseAttempt,
    ReleaseGate,
    Canary,
    RollbackPlan,
    EvidenceCapsule,
    ProofTimelineItem,
    LlmProvider,
    LlmCall,
    WebhookDelivery,
    BrokerTopic,
    RuntimeProfile,
}
```

### 26.4 Source freshness

```rust
pub struct SourceFreshness {
    pub source: SourceKind,
    pub status: FreshnessStatus,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub latency_ms: Option<u64>,
    pub age_ms: Option<u64>,
    pub error: Option<String>,
}

pub enum SourceKind {
    GitLab,
    StateDb,
    Docker,
    CacheGateway,
    Vault,
    Broker,
    GitHost,
    Autonomy,
    LlmProvider,
    Jankurai,
    Filesystem,
    WebhookReceiver,
    Mcp,
}
```

Staleness behavior:

- stale source dims all dependent facts;
- unknown values render as `unknown`, not zero;
- actions that require stale source are blocked or require explicit override;
- event stream gaps trigger snapshot refresh;
- stale animations freeze.

### 26.5 HTTP endpoints to add

```http
GET  /api/read-model
GET  /api/events?cursor=N&limit=500&kinds=&entity_kind=&entity_id=&repo=&family=
GET  /api/events/stream?cursor=N                         # SSE
GET  /api/ws/events                                      # WebSocket alternative
GET  /api/entity/{kind}/{id}
GET  /api/proof?entity=&kind=&since=&actor=&cursor=&limit=
POST /api/action/preview
POST /api/action/execute
GET  /api/runtime/profile
GET  /api/health/deep
GET  /api/source-doctor
GET  /api/repos
GET  /api/repos/{repo_slug}/overview
GET  /api/families
GET  /api/families/{family}/overview
GET  /api/queue
GET  /api/queue/theoretical-limit
GET  /api/workflows/atlas?scope=&family=&repo=
GET  /api/repos/{repo_slug}/workflow-graph?pipeline_id=&ref=
GET  /api/pipelines/{project_id}/{pipeline_id}/jobs
GET  /api/jobs/{project_id}/{job_id}
GET  /api/jobs/{project_id}/{job_id}/trace?cursor=&limit=
GET  /api/jobs/{project_id}/{job_id}/trace/stream
GET  /api/jobs/{project_id}/{job_id}/capsule
GET  /api/bottlenecks?repo=&family=&window=&ref=
GET  /api/runners/capacity
GET  /api/nodes
GET  /api/cache/dashboard
GET  /api/cache/objects?category=&repo=&hot=&tainted=
GET  /api/cache/provenance/{key}
GET  /api/cache/gc-plan
GET  /api/cache/events/stream
GET  /api/vti/dashboard
GET  /api/vti/plan/{plan_id}
GET  /api/vti/misses?repo=&window=
GET  /api/agents/dashboard
GET  /api/agents/{agent_id}
GET  /api/agents/{agent_id}/logs/stream
GET  /api/autonomy/workflows
GET  /api/autonomy/workflows/{id}
GET  /api/bugs/dashboard
GET  /api/bugs/{bug_id}
GET  /api/git-sync/dashboard
GET  /api/jankurai/dashboard
GET  /api/security/dashboard
GET  /api/artifacts/dashboard
GET  /api/artifacts/{artifact_id}/provenance
GET  /api/release/dashboard
GET  /api/releases/{release_id}
GET  /api/release/{release_id}/watch
GET  /api/secrets/status
GET  /api/settings/effective-redacted
```

### 26.6 MCP resources to mirror

MCP tools remain for actions. MCP resources are read-only inspection.

```text
jeryu://tui/read-model
jeryu://events?cursor=N
jeryu://system/snapshot
jeryu://runtime/profile
jeryu://health/deep
jeryu://repos
jeryu://repos/{slug}
jeryu://families
jeryu://families/{family}
jeryu://queue
jeryu://workflows/atlas
jeryu://pipeline/{project_id}/{pipeline_id}
jeryu://jobs/{project_id}/{job_id}/trace
jeryu://jobs/{project_id}/{job_id}/capsule
jeryu://runners/capacity
jeryu://nodes
jeryu://cache/dashboard
jeryu://cache/object/{key}
jeryu://vti/dashboard
jeryu://vti/plan/{plan_id}
jeryu://agents/dashboard
jeryu://agent/{agent_id}
jeryu://bugs/dashboard
jeryu://bug/{bug_id}
jeryu://git-sync/dashboard
jeryu://jankurai/dashboard
jeryu://security/dashboard
jeryu://artifacts/dashboard
jeryu://artifact/{artifact_id}/provenance
jeryu://release/latest
jeryu://release/{release_id}
jeryu://proof?entity=&since=
jeryu://admission/recent
jeryu://capability/grants
jeryu://llm/providers
jeryu://settings/effective-redacted
```

### 26.7 Event kinds

Minimum event kinds:

```text
system.health.updated
source.freshness.updated
repo.discovered
repo.sync.updated
repo.family.updated
mr.opened
mr.updated
mr.approved
mr.blocked
mr.merged
pipeline.created
pipeline.running
pipeline.succeeded
pipeline.failed
pipeline.canceled
pipeline.blocked
job.queued
job.started
job.progress
job.log.chunk
job.annotation
job.failed
job.succeeded
job.retried
job.canceled
runner.online
runner.offline
runner.busy
runner.idle
runner.degraded
runner.oom
runner.scale.requested
cache.hit
cache.miss
cache.taint.created
cache.taint.cleared
cache.verdict
cache.gc.plan
cache.gc.completed
vti.plan.created
vti.test.selected
vti.test.skipped
vti.selector.miss
vti.learning.updated
agent.session.started
agent.session.heartbeat
agent.session.blocked
agent.session.finished
agent.step.started
agent.step.finished
agent.patch.proposed
agent.race.created
agent.race.winner.selected
agent.grant.requested
grant.created
grant.expired
grant.denied
admission.allowed
admission.denied
bug.created
bug.updated
bug.attempt.started
bug.attempt.failed
bug.fix.proposed
bug.verified
jankurai.audit.completed
jankurai.score.changed
jankurai.finding.opened
jankurai.finding.closed
security.finding.opened
security.finding.closed
secret.audit
secret.access.denied
artifact.created
artifact.signed
artifact.verification.failed
release.gate.updated
release.promoted
release.rollback.started
release.rollback.completed
policy.violation
action.previewed
action.started
action.progress
action.completed
action.failed
proof.item.created
llm.call.started
llm.call.completed
llm.budget.warning
webhook.received
webhook.parsed
webhook.dispatch.failed
broker.lag.updated
snapshot.refreshed
```

---

## 27. Action model and safety

### 27.1 Action descriptor

```rust
pub struct ActionDescriptor {
    pub id: String,
    pub label: String,
    pub description: String,
    pub entity: EntityRef,
    pub risk_tier: RiskTier,
    pub side_effect_class: SideEffectClass,
    pub dry_run_available: bool,
    pub required_grants: Vec<GrantRequirement>,
    pub required_fresh_sources: Vec<SourceKind>,
    pub disabled_reason: Option<String>,
    pub estimated_blast_radius: BlastRadius,
}
```

### 27.2 Risk tiers

| Tier | Examples | Confirmation |
|---|---|---|
| Read-only | refresh, open proof, inspect logs | immediate |
| Low local mutation | mark bug status, pin watchlist | preview + `y` |
| CI mutation | retry/cancel/play job, run tests | preview cost/grants + `y` |
| Branch/MR mutation | propose patch, open MR, edit config branch | preview diff/branch + `y` |
| Infrastructure mutation | scale/drain/restart pool/node/cache GC | preview blast radius + typed phrase if high |
| Merge/release/prod/secret/destructive | merge MR, promote prod, rollback, rotate secrets, clear taint | typed phrase + evidence bundle + optional second approval |

Typed phrase examples:

```text
MERGE veox-api !184 a91f2bc
ROLLBACK production veox 1.1.7
ROTATE prod secrets v1.2.0
DRAIN pool secure-runner
CLEAR CACHE TAINT sha256:ab12
```

### 27.3 Action flow

```text
focus object
  → choose action from local keys or command palette
  → build ActionRequest
  → POST /api/action/preview
  → render proof/risk/diff/dry-run/blast radius/freshness
  → confirm or cancel
  → POST /api/action/execute
  → stream ActionProgress events
  → update entity and proof timeline
  → show ActionResult receipt
```

### 27.4 Preview modal fields

```text
action label
entity and scope
risk tier
side-effect class
required grants
source freshness
exact backend calls planned
estimated runtime/cost
blast radius
policy gates
dry-run output
expected evidence receipt
rollback/undo path if any
confirmation requirement
```

---

## 28. Rust implementation architecture

### 28.1 Recommended stack

Use Rust with:

- `ratatui` for rendering widgets/layouts;
- `crossterm` for terminal backend, raw mode, keyboard/mouse/resize;
- `tokio` for async event fan-in, streams, timers, network, subscriptions;
- `serde` / `serde_json` for shared schemas;
- existing HTTP/GitLab clients and/or `reqwest` for inspection API;
- WebSocket/SSE client layer for live streams;
- `tracing` / `tracing-subscriber` for TUI diagnostics and performance telemetry;
- `thiserror` and `color-eyre`/`eyre` for errors;
- `insta` and Ratatui test backend for golden render tests.

Do not make rendering itself perform network or blocking IO.

### 28.2 Module layout

```text
src/tui/
  mod.rs
  app.rs                         # App state, render entry, input dispatch
  focus.rs                       # macro/micro focus state
  theme.rs                       # palette, glyphs, terminal capability
  keymap.rs                      # key definitions and contextual help
  command_palette.rs             # fuzzy commands and action launcher
  routes.rs                      # navigation stack and deep links
  motion.rs                      # animation ticks, low-motion, liveness pulses
  runtime/
    mod.rs
    event_bus.rs                 # event ingestion, subscriptions, coalescing
    subscriptions.rs             # HTTP/SSE/WS/direct DB subscriptions
    reducer.rs                   # applies TuiEvent deltas to view cache
    action_client.rs             # preview/execute action API
    log_stream.rs                # websocket/poll fallback log chunks
    freshness.rs                 # stale/degraded source handling
    backpressure.rs              # event/drop/coalesce policy
    diagnostics.rs               # frame time, dropped events, reconnects
  model/
    mod.rs
    entity.rs
    events.rs
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
    security.rs
    artifacts.rs
    release.rs
    evidence.rs
    churn.rs
    settings.rs
  screens/
    mod.rs
    global.rs
    repos.rs
    workflow.rs
    queue.rs
    runners.rs
    cache.rs
    vti.rs
    agents.rs
    bugs.rs
    git_sync.rs
    jankurai.rs
    security.rs
    artifacts.rs
    release.rs
    evidence.rs
    metrics.rs
    settings.rs
  widgets/
    mod.rs
    table.rs                     # virtualized selectable table
    dag.rs                       # workflow graph widget
    log_view.rs                  # streaming trace viewer
    sparkline.rs
    heatmap.rs
    progress.rs
    status_strip.rs
    minimap.rs
    inspector.rs
    event_tail.rs
    breadcrumbs.rs
    modal.rs
    tabs.rs
    help.rs
    diff.rs
    evidence_chip.rs
  data/
    mod.rs
    client.rs                    # trait TuiDataClient
    http.rs                      # /api + SSE/WS client
    local.rs                     # direct DB/GitLab/Docker fallback
    mcp.rs                       # optional MCP resource client
    demo.rs                      # deterministic fixtures
    recording.rs                 # capture/replay
  tests/
    fixtures.rs
    snapshots.rs
    interactions.rs
```

### 28.3 App state

```rust
pub struct App {
    pub route: RouteStack,
    pub focus: FocusState,
    pub theme: Theme,
    pub terminal_caps: TerminalCaps,
    pub keymap: Keymap,
    pub views: ViewCache,
    pub subscriptions: SubscriptionState,
    pub command_palette: CommandPaletteState,
    pub modals: ModalStack,
    pub event_tail: EventTailState,
    pub log_state: LogPaneState,
    pub input_mode: InputMode,
    pub watchlist: Watchlist,
    pub motion: MotionState,
    pub diagnostics: TuiDiagnostics,
    pub last_action: Option<ActionFeedback>,
    pub now: DateTime<Utc>,
}

pub enum InputMode {
    Root,
    Drill { pane_id: PaneId },
    Filter { query: String },
    GlobalSearch { query: String },
    CommandPalette,
    ConfirmAction { action_id: String },
    TextEdit { field_id: String, buffer: String },
    LogSearch { query: String },
    Help,
}
```

### 28.4 Data client trait

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

- `HttpDataClient`: target production client using `/api` + SSE/WS.
- `LocalDataClient`: transition fallback using state DB, GitLab, Docker, cache APIs.
- `McpDataClient`: optional read-only MCP resource path.
- `DemoDataClient`: deterministic fake data for demos/tests.
- `RecordingDataClient`: captures/replays real sessions.

### 28.5 View cache

```rust
pub struct ViewCache {
    pub global: Versioned<FleetDashboardView>,
    pub repos: Versioned<RepoBrowserView>,
    pub queue: Versioned<QueueDashboardView>,
    pub workflows: HashMap<WorkflowScope, Versioned<WorkflowDashboardView>>,
    pub runners: Versioned<RunnersDashboardView>,
    pub cache: Versioned<CacheDashboardView>,
    pub vti: Versioned<VtiDashboardView>,
    pub agents: Versioned<AgentsDashboardView>,
    pub bugs: Versioned<BugsDashboardView>,
    pub git_sync: Versioned<GitSyncDashboardView>,
    pub jankurai: Versioned<JankuraiDashboardView>,
    pub security: Versioned<SecurityDashboardView>,
    pub artifacts: Versioned<ArtifactsDashboardView>,
    pub release: Versioned<ReleaseDashboardView>,
    pub evidence: Versioned<EvidenceDashboardView>,
    pub metrics: Versioned<MetricsDashboardView>,
    pub entities: LruCache<EntityRef, EntityDetail>,
}

pub struct Versioned<T> {
    pub value: T,
    pub generated_at: DateTime<Utc>,
    pub cursor: u64,
    pub stale: bool,
    pub source_health: SourceHealth,
}
```

### 28.6 Event reducer

All incoming deltas go through a reducer.

```rust
pub fn reduce_event(cache: &mut ViewCache, event: &TuiEvent) -> Vec<Invalidation> {
    match event.kind {
        TuiEventKind::JobStarted | TuiEventKind::JobProgress | TuiEventKind::JobSucceeded | TuiEventKind::JobFailed => {
            invalidate_queue(cache, event);
            invalidate_repo(cache, event);
            invalidate_workflow(cache, event);
            invalidate_global_attention(cache, event);
        }
        TuiEventKind::CacheTaintCreated | TuiEventKind::CacheTaintCleared | TuiEventKind::CacheVerdict => {
            invalidate_cache(cache, event);
            invalidate_global_attention(cache, event);
        }
        TuiEventKind::VtiSelectorMiss => {
            invalidate_vti(cache, event);
            invalidate_repo(cache, event);
            invalidate_global_attention(cache, event);
        }
        TuiEventKind::AgentStepFinished | TuiEventKind::GrantRequested => {
            invalidate_agents(cache, event);
            invalidate_bugs_if_linked(cache, event);
        }
        TuiEventKind::ReleaseGateUpdated => {
            invalidate_release(cache, event);
            invalidate_artifacts_if_linked(cache, event);
            invalidate_global(cache, event);
        }
        _ => {}
    }
}
```

Reducers may update lightweight fields immediately and schedule full refresh for affected dashboards.

### 28.7 Render loop

Target render loop:

```rust
let input_tick = Duration::from_millis(20);
let render_tick = Duration::from_millis(100); // 10 fps default, adaptive to 20 fps for intense screens
loop {
    app.drain_backend_events();
    app.coalesce_invalidations();
    app.advance_motion();

    if app.should_render() {
        terminal.draw(|f| screens::draw(f, &mut app))?;
        app.diagnostics.record_frame();
    }

    tokio::select! {
        Some(input) = input_rx.recv() => input::handle(&mut app, input).await?,
        Some(event) = backend_event_rx.recv() => app.enqueue_event(event),
        _ = tokio::time::sleep(input_tick) => {}
    }
}
```

Rules:

- Do not block UI thread on network, DB, Docker, GitLab, Vault, filesystem, or compression.
- Coalesce high-frequency log chunks and cache hits.
- Keep selection stable across updates.
- Avoid full table re-sort unless sort key changed or refresh interval elapsed.
- Virtualize tables over 500 rows.
- Retain only bounded log rings unless user pins a trace.

### 28.8 DAG layout

Implement deterministic layout:

1. Build graph nodes from pipeline/job/release gate objects.
2. Add edges from `needs`, stage barriers, artifacts, child pipelines, release/security gates.
3. Collapse simple linear chains in fleet view; expand in pipeline view.
4. Rank nodes by dependency depth and stage.
5. Place critical path on primary horizontal lane.
6. Use swimlanes by stage or repo family in atlas mode.
7. Preserve node positions across updates to avoid flicker.
8. Recompute layout only when topology changes, not every progress tick.

---

## 29. Responsive layout

### 29.1 Width classes

| Width | Mode |
|---:|---|
| `< 100` | compact: single main pane + collapsible inspector |
| `100–139` | standard: left scope + main + bottom detail |
| `140–199` | wide: left scope + main + right inspector |
| `>= 200` | war room: multi-column, heatmaps, event tail, inspector |

### 29.2 Height classes

| Height | Behavior |
|---:|---|
| `< 28` | compact rows, hide secondary sparklines |
| `28–44` | standard |
| `45+` | include history/trend panes and richer trace tail |

### 29.3 Density modes

- `comfortable`: more labels and spacing.
- `dense`: default for expert operator.
- `war-room`: maximum information for large terminals.
- `focus`: one selected workflow/log/table with global header preserved.

---

## 30. Search, filters, and saved lenses

### 30.1 Global search

`Ctrl-/` searches:

- repos/families
- pipelines/jobs
- bugs
- agents
- commits/branches/MRs
- artifacts/digests
- evidence IDs
- release versions
- cache keys
- Jankurai findings
- security findings

Search result rows show entity kind, severity, freshness, and route.

### 30.2 Filter syntax

Support simple typed filters:

```text
repo:veox-api status:failed since:24h
family:veox-* kind:job status:running
bug:ready severity:S0,S1 agent:none
cache category:cargo size:>1GiB tainted:true
vti miss:true repo:redline-db
release gate:artifact status:blocked
```

### 30.3 Saved lenses

A lens stores:

- current screen
- scope
- filters
- sort order
- pinned panes
- visible overlays
- watchlist entities

Examples:

```text
lens:release-war-room
lens:agent-races
lens:cache-pressure
lens:veox-family
lens:security-gates
```

---

## 31. Incident mode, replay mode, and demo/capture

### 31.1 Incident mode

Incident mode increases signal density for production/release/security issues:

- header turns incident posture;
- release, rollback, prod health, gates, and escalation pinned;
- unrelated low-severity animations reduced;
- event tail filters to incident entities;
- actions emphasize rollback, freeze, pause agents, lock merge, notify.

### 31.2 Time-travel replay

Given an event cursor or timestamp, reconstruct:

- what the TUI would have shown;
- which facts were stale;
- which grants existed;
- which action preview was shown;
- which evidence supported a decision;
- which agents acted;
- which artifacts were signed.

Replay controls:

```text
Space play/pause | ←/→ step event | PgUp/PgDn jump 100 | t timestamp | e evidence | Esc live
```

### 31.3 Demo/capture mode

- deterministic fixtures;
- fake but plausible multi-repo activity;
- scripted failure/recovery/release story;
- screenshot/capture for docs;
- redaction always enabled;
- no mutating actions unless explicitly connected to demo backend.

---

## 32. Backend plumbing backlog

### 32.1 P0 — Make the TUI truthful and live

1. Expose `TuiReadModel` over HTTP.
2. Expose `TuiEvent` stream over SSE/WebSocket.
3. Add bounded job trace streaming with polling fallback.
4. Add source freshness/deep health endpoint.
5. Add entity detail endpoint for all existing entity kinds.
6. Expose action preview/execute endpoints generated from action registry.
7. Preserve stream cursor and replay gaps.

### 32.2 P0 — Fix workflow visibility

1. Remove first-active-pipeline bias.
2. Compute multi-pipeline atlas by fleet/family/repo scope.
3. Build graph edges from `needs`, stages, artifacts, child pipelines, and release gates.
4. Add critical-path calculation.
5. Add ETA confidence and staleness markings.

### 32.3 P0 — Agent and evidence foundations

1. Add agent lifecycle tables.
2. Persist agent steps/heartbeats/grants/evidence refs.
3. Make evidence a searchable proof timeline.
4. Add bug/agent/MR/pipeline linkages.

### 32.4 P1 — Cache and VTI depth

1. Expand `/cache/summary` into dashboard/object/taint/verdict/GC/provenance endpoints.
2. Categorize cache bytes by Rust crates, targets, OCI, artifacts, sccache, VTI, Jankurai, material objects.
3. Expose VTI plan detail, selector misses, learning state, guardrail decisions.
4. Add saved-time and false-skip metrics.

### 32.5 P1 — Git/MR/release/security

1. Act on MR webhooks and persist MR state.
2. Persist webhook delivery metadata and raw body SHA.
3. Parse GitLab artifacts: JUnit/xUnit, coverage, code quality, SAST, dependency, container, benchmark, nextest, release gate JSON.
4. Add artifact attestation ledger.
5. Add release/canary/prod event stream.
6. Add redacted Vault/secret lease/audit metadata.

### 32.6 P1 — Utilization and metrics

1. Add Docker/container stats sampling.
2. Add remote node metrics/heartbeat.
3. Add broker lag and throughput status.
4. Add main-daemon Prometheus/OpenTelemetry metrics.
5. Add CI economics and cost/waste samples.

### 32.7 P1 — Jankurai and quality

1. Add Jankurai audit ingestion table.
2. Add findings/caps/score/version drift models.
3. Add score history and repair queue.
4. Correlate Jankurai regressions with commits, agents, bugs, and release gates.

### 32.8 P2 — Superpowers

1. What-if scheduler simulator.
2. Natural-language “why?” generated from structured facts.
3. Automatic optimization reports.
4. Ownership/reviewer routing.
5. Dependency/toolchain drift cockpit.
6. Time-travel replay and trust replay.
7. Cross-repo dependency impact propagation.

---

## 33. Testing strategy

### 33.1 Unit tests

- reducers
- routing stack
- keymap dispatch
- focus transitions
- source freshness transitions
- action preview rendering decisions
- SCREAM index formulas
- queue simulation fixtures
- DAG layout stability
- cache categorization
- VTI guardrail states

### 33.2 Golden render tests

Use deterministic fixtures and Ratatui test backend for:

- global screen at 80x24, 120x40, 180x50, 240x60;
- repo atlas with many families;
- workflow DAG with failures, child pipelines, and gates;
- queue theoretical-limit screen;
- cache full/tainted state;
- VTI miss state;
- agent race state;
- bug board;
- security/artifact gate failure;
- release rollback screen;
- evidence ledger;
- stale/degraded sources;
- empty states;
- no-Unicode/16-color fallback.

### 33.3 Interaction tests

Use a black-box TUI test harness to verify:

```text
Global → family → repo → pipeline → job → log → evidence → Esc chain
```

Also test:

- `Tab` pane cycling;
- arrow movement;
- filters and search;
- command palette;
- action preview cancel/confirm;
- dangerous typed confirmation;
- config edit validation failure;
- streaming updates preserving selection;
- reconnect/gap snapshot refresh;
- time-travel replay controls;
- screenshot redaction.

### 33.4 Backend contract tests

- validate JSON schemas for read model, events, entity details, action previews/results;
- generate OpenAPI/JSON Schema from source types;
- ensure MCP resource schemas match HTTP schemas;
- ensure action registry docs match actual actions;
- fixtures for GitLab/Vault/Docker/cache unavailable states;
- event ordering and cursor gap tests.

### 33.5 Performance tests

Scenarios:

- 500 repos;
- 5,000 active jobs;
- 10,000 historical jobs in tables;
- 1,000 events/sec burst;
- 100 simultaneous log streams with only selected visible;
- 10 MB trace loaded incrementally;
- 1,000 cache hot objects;
- 500 agent steps/min;
- SSH terminal with low refresh and small window.

Targets:

- normal input latency < 30 ms;
- standard render frame < 16 ms when no heavy topology change;
- no network/DB request on render path;
- memory bounded by configured log/event rings;
- scroll/select stable under high-frequency updates;
- stream reconnect under 2 seconds local loopback;
- graceful polling fallback.

### 33.6 Safety tests

- secret values never render in screens, logs, captures, errors, or raw JSON unless explicitly permitted in a secure debug mode that should be disabled by default;
- dangerous actions cannot execute without preview and confirmation;
- stale source blocks unsafe actions;
- dry-run output cannot be mistaken for execution;
- action receipts are persisted;
- screenshots redact secret-adjacent fields;
- terminal state is restored after panic.

---

## 34. Implementation phases

### Phase 0 — Truth cleanup and fixtures

- Consolidate current API docs vs source drift.
- Generate fixtures from existing DB/GitLab/cache examples.
- Add demo backend with multi-repo/family data.
- Define schemas for read model/events/entity/action.

### Phase 1 — TUI shell and navigation

- Implement global shell, route stack, breadcrumbs, focus model.
- Implement keymap, command palette skeleton, help overlay.
- Implement theme, glyphs, low-motion, terminal capability fallback.
- Implement global/family/repo skeleton screens from demo data.

### Phase 2 — Unified read model client

- Add `TuiDataClient` trait.
- Implement `DemoDataClient` and `LocalDataClient`.
- Implement HTTP read-model endpoint if backend work is in scope.
- Add source freshness map and staleness UI.

### Phase 3 — Streaming and reducer

- Add event bus, reducers, view cache, invalidation model.
- Add SSE/WebSocket events with reconnect and cursor replay.
- Add trace streaming with polling fallback.
- Add event tail and live motion.

### Phase 4 — Workflow, queue, and logs

- Implement Workflow Atlas and pipeline DAG.
- Compute graph edges and critical path.
- Add job detail and live trace viewer.
- Implement queue physics, theoretical-limit model, SCREAM index.

### Phase 5 — Core domain screens

- Repo atlas/family dashboard.
- Runners/utilization.
- Cache observatory.
- VTI cockpit.
- CI bottleneck lab.

### Phase 6 — Agents, bugs, Git sync

- Agent cockpit from existing inferred data, then lifecycle tables.
- Bug board and detail.
- Patch racing lanes.
- Git sync/admission/mirror screen.
- Safe config editor MVP.

### Phase 7 — Trust/compliance screens

- Evidence/proof ledger.
- Security/secrets/policy screen.
- Artifacts/provenance ledger.
- Release/canary/rollback cockpit.
- Jankurai audit center.

### Phase 8 — Polish and superpowers

- What-if simulator.
- Time-travel replay.
- Incident mode.
- CI economics/churn.
- Natural-language “why?” backed by structured facts.
- Saved lenses/watchlists.
- Demo/capture mode.
- Performance hardening.

---

## 35. Build acceptance criteria

The TUI is ready when a developer can:

1. Open `jeryu tui` and see all active work across all repos and repo families.
2. Know safe-to-code, safe-to-merge, and safe-to-release in the header.
3. Know how close the fleet is to physics/fleet/policy theoretical limits.
4. Drill fleet → family → repo → pipeline → job → live trace → evidence using `Enter`/`Right` and return with `Esc`/`Left`.
5. See cache fullness by category and preview safe GC.
6. Prove whether VTI is saving time safely and inspect selector misses.
7. Inspect all active agents, grants, races, budgets, branches, MRs, logs, and evidence.
8. See cross-repo bug queues and agent attempts.
9. Inspect Git sync, MR state, hooks, admission decisions, and mirrors.
10. See Jankurai score/caps/trends and assign repair work.
11. See security findings, Vault status, secret audits, policy violations, and signed artifact gates.
12. Understand release/canary/prod/rollback state and why automation did or did not act.
13. Open a proof timeline for any entity.
14. Run any mutating action only through preview/confirmation/evidence.
15. Operate in a degraded environment without false confidence.
16. Use the UI on 80x24, standard, and war-room terminals.
17. Capture screenshots without leaking secrets.
18. Replay a past incident from event cursor and explain decisions.

---

## 36. Final experience target

The finished Flight Deck should feel like this:

- The top strip tells the truth immediately: safe-to-code, merge, release, SCREAM, stale sources, and next action.
- The global page is alive: repo families pulse, live queues move, critical paths animate, event tail scrolls, agents leave traces.
- Every visual element is actionable: focus it, press `Enter`, and the system opens the next level of detail.
- Every warning has a cause, confidence, proof, owner, and recommended action.
- Every mutation is auditable and previewed.
- The operator can switch from fleet-level chaos to one log line or one artifact signature in seconds.
- Agents feel powerful but contained: visible state, visible grants, visible budgets, visible evidence.
- The system never hides uncertainty. Stale data looks stale. Unknown data says unknown. Degraded backends explain what is missing.

The final design mantra:

> **Make the whole engineering machine visible, moving, colorful, drillable, provable, and safe.**

---

## 37. Appendix: high-value future MCP/API additions

Add read-only MCP tools/resources or HTTP endpoints for:

- `jeryu.pool_list`
- `jeryu.job_list`
- `jeryu.job_trace`
- `jeryu.pipeline_explain`
- `jeryu.pipeline_doctor`
- `jeryu.cache_status`
- `jeryu.cache_doctor`
- `jeryu.release_status`
- `jeryu.release_ready`
- `jeryu.secrets_status`
- `jeryu.host_doctor`
- `jeryu.node_list`
- `jeryu.node_doctor`
- `jeryu.policy_audit`
- `jeryu.settings_effective`
- `jeryu.action_list`
- `jeryu.git_host_ping`
- `jeryu.pr_list_open`
- `jeryu.pr_state`
- `jeryu.pr_diff`
- `jeryu.pr_policy_sha`
- `jeryu.post_check_preview`
- `jeryu.merge_passport_status`
- `jeryu.search_proof_timeline`
- `jeryu.get_runtime_profile`
- `jeryu.get_race_status`
- `jeryu.select_race_winner`
- `jeryu.cleanup_losing_branches`

---

## 38. Appendix: artifact parsing backlog

Parse and ingest these from GitLab artifacts or equivalent CI outputs:

- JUnit/xUnit XML;
- nextest archives;
- coverage reports;
- code-quality reports;
- SAST reports;
- dependency scan reports;
- container scan reports;
- secret detection reports;
- benchmark JSON;
- Jankurai audit JSON/CSV/proof artifacts;
- release gate JSON;
- SBOM files;
- provenance attestations;
- signature verification output.

Use parsed artifacts to feed:

- pipeline doctor;
- blocker explanations;
- VTI learning;
- flake intelligence;
- security screen;
- artifact/release gates;
- Jankurai screen;
- proof ledger.

---

## 39. Appendix: final build checklist for implementation agents

Before opening a PR, verify:

- [ ] Terminal raw mode always restores on panic/error.
- [ ] Header always shows scope, freshness, and safety posture.
- [ ] `Enter` drills and `Esc` goes back everywhere.
- [ ] Every red/yellow element supports `y` why and `e` evidence when evidence exists.
- [ ] No network or DB IO occurs in render functions.
- [ ] Tables are virtualized.
- [ ] Log buffers are bounded.
- [ ] Stream reconnect handles cursor gaps.
- [ ] Snapshot refresh repairs event gaps.
- [ ] Low-motion mode works.
- [ ] 16-color and no-Unicode fallbacks are legible.
- [ ] Secret redaction applies to screen, logs, error reports, screenshots, raw JSON panels.
- [ ] Mutating actions require preview.
- [ ] Dangerous actions require typed confirmation.
- [ ] Action results create evidence receipts.
- [ ] Stale source blocks unsafe actions.
- [ ] Golden snapshots cover compact/standard/war-room layouts.
- [ ] Demo mode exercises repo families, failures, agents, cache, VTI, release, Jankurai, and evidence.
- [ ] Generated docs/schemas are up to date.
