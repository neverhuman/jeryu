# JeRyu Final Rust TUI Engineering Specification

**Artifact:** `jeryu_final_rust_tui_engineering_spec.md`  
**Working product names:** `jeryu tui`, **JeRyu Workflow Atlas**, **Flight Deck**, **Control Deck**, **Mission Control**  
**Primary command:** `jeryu tui`  
**Default lens:** `0 Workflow`  
**Audience:** Rust/TUI/backend implementation agents building the next JeRyu operator experience.  
**Goal:** Build the strongest possible terminal-native control plane for multi-repo CI/CD, agents, VTI, SmartCache, runners, release, evidence, security, and system utilization.

---

## 0. Executive summary

JeRyu already has the raw ingredients of an elite CI/CD control plane: a Rust single-binary architecture, CLI, TUI, typed read model, state DB, GitLab webhooks/REST wrapper, custom executor, runner pools, SmartCache, VTI smart test selection, local bug tracking, release/canary/rollback workflows, Vault/secrets, Git admission hooks, capability API, MCP tools, autonomy/Evidence Gate, LLM provider telemetry, Jankurai audits, and signed/provenance-aware artifacts.

The final TUI should not feel like a prettier CLI or a static dashboard. It should feel like **air traffic control for autonomous software delivery**:

```text
Global Workflow Atlas
  -> Repo Family
      -> Repo Cockpit
          -> Pipeline / Workflow DAG
              -> Job / Trace / Evidence
                  -> Action Preview / Proof / Receipt
```

The implementation center of gravity is a **terminal-native object browser over JeRyu’s live read model**:

1. **Entities:** repo families, repos, pipelines, jobs, runners, pools, nodes, agents, autonomous workflows, bugs, test plans, cache objects, cache taints, artifacts, signatures, releases, gates, secret authorities, admission decisions, evidence capsules, Jankurai findings, security findings, LLM providers, and system sources.
2. **Events:** append-only cursor-addressable updates from webhooks, GitLab REST reconciliation, DB writes, Docker, cache gateway, Vault, agents, Git hooks, release automation, Jankurai, LLM providers, and action execution.
3. **Actions:** previewable, dry-runnable, capability-gated, risk-tiered mutations with idempotency keys, source SHA binding, evidence, and audit receipts.

The operator must be able to answer these questions immediately:

- What is happening across all repos right now?
- Which repo family, repo, pipeline, job, cache namespace, runner pool, release gate, security finding, or agent needs attention first?
- Are we near the theoretical CI throughput limit, or are we wasting runner capacity?
- Are CPU, memory, disk, network, Docker, GitLab API, cache, or VTI limiting us?
- Should we increase runner count, scale managers, add remote nodes, run GC, rebalance tags, split jobs, or do nothing?
- What is the live queue across all jobs and all repos?
- Can I drill into a repo, pipeline DAG, job trace, VTI decision, cache object, agent branch, bug attempt, release proof, or artifact signature in a few keystrokes?
- Are autonomous agents helping, stuck, racing, blocked by grants, spending too much, or creating risk?
- Can I safely merge, release, promote, rotate secrets, or roll back?

**Important runner-count note:** the uploaded archive contains API/data inventories and design drafts, not a live host utilization snapshot. Therefore this spec does not claim “increase runners now” as a static answer. Instead it defines the exact live telemetry, formulas, thresholds, and UI decision rules that should make the TUI answer that question truthfully at runtime.

---

## 1. Source-derived baseline

The uploaded `.txt` inventories consistently describe the following current JeRyu surfaces and data families. Treat source-derived inventory as more authoritative than stale docs when there is drift.

### 1.1 Control surfaces available today or already implied

| Surface | Entrypoint / transport | TUI relevance |
|---|---|---|
| CLI | `jeryu <command>` | Install, serve, down, remote/node management, git/save/sync/undo, status, settings, pools, jobs, pipelines, cache, logs, agents, tests/VTI, release, secrets, progress, blockers, actions, repo/fleet, host, policy, bugs, hidden executor/hook/capability/MCP surfaces. |
| TUI internal API | `src/api/*` | Typed read model, entity taxonomy, event stream concepts, snapshots, action previews/results, mission snapshot, attention queue, source freshness. |
| MCP stdio | `jeryu mcp serve-stdio` / `jeryu mcp serve` | JSON-RPC tool calls over stdio. |
| MCP HTTP | default `127.0.0.1:9778`, `POST /mcp`, `DELETE /mcp`, GET disabled today | Tool-centric remote/local agent surface; loopback/origin/session guarded. Should grow resources/watch streams. |
| Webhook/API server | Axum, default `127.0.0.1:9777` | `/health`, `/hooks`, `/cache/summary`; consumes GitLab Job/Pipeline/Push, MR currently partial/logged. |
| Capability API | Unix domain socket | Agent intents, grants, nonces, expiry, idempotency, budget, project/ref/SHA, action responses. |
| GitLab REST wrapper | internal client | Projects, jobs, traces, artifacts, pipelines, bridges/downstream pipelines, variables, runners/managers, issues, MRs, branches, webhooks, play/cancel/retry. |
| GitLab webhooks | `/hooks` | Job, pipeline, push; MR should become first-class durable state. |
| Message log / broker | Kafka or Jansu feature gated | `jeryu.webhook.jobs`, `jeryu.webhook.pipelines`, `jeryu.webhook.pushes`. |
| Custom executor | hidden `jeryu exec config/prepare/run/cleanup` | Runner lifecycle, job environment, sandbox, logs, failure capsules, cache decisions. |
| Git server hook | hidden `jeryu server-hook pre-receive` | Admission decisions, actor kind, grants, policies, allow/audit/deny. |
| SmartCache / gateway | proxy `19800`, OCI mirror `19801` | Cargo sparse/crate downloads, CAS hits, singleflight, cache requests, taints, verdicts, hot objects, GC. |
| Docker / runner plane | Bollard + compose/remotes | Managed runner containers, Docker events, logs, lifecycle, OOM/restarts, node pressure. |
| Vault/secrets | Vault HTTP + DB | Secret authorities, release secret sets, audit, rotation, finalization, expiry, redacted fingerprints. |
| State DB | SQLite default, RedlineDB optional | Durable truth for pools, managers, jobs, releases, evidence, retry, cache, VTI, secrets, grants, bugs, autonomy, LLM budget, ledger/verdicts. |
| Autonomy binary | `autonomy` CLI/server | Evidence Gate/VibeGate, kill bell, freeze windows, verdicts, launch ledger, PR drift, `/metrics`, `/health`, `/events`. |
| GitHost abstraction | GitHub/GitLab-like | PR/MR state, diff, checks, comments, approvals, target policy SHA. |
| Jankurai | repo audit tooling/action | Audit score, tool adoption, duplicate code, release/security/TUI/web/repo-rot findings, enforcement modes. |

### 1.2 Current MCP tool manifest

The current source-derived MCP inventory says JeRyu exposes **16 tools**, all under the `jeryu.` prefix:

| Tool | Kind | Primary TUI use |
|---|---|---|
| `jeryu.fetch_capsule` | read | Fetch latest structured failure/evidence capsule for a job. |
| `jeryu.get_system_snapshot` | read | Seed global snapshot: GitLab readiness, pool count, recent events, latest release. |
| `jeryu.get_pipeline_jobs` | read | Pipeline/job drilldown, including downstream-expanded jobs. |
| `jeryu.get_ci_bottlenecks` | read | Historical timing bottleneck view. |
| `jeryu.explain_blockers` | read | Attention queue, blocker explain panels, proof modals. |
| `jeryu.plan_validation` | read | Validate VTI plan against selector-miss history. |
| `jeryu.run_tests` | mutate | Trigger targeted test pipeline. |
| `jeryu.propose_patch` | mutate | Agent/human patch branch + commit + optional MR. |
| `jeryu.race_patches` | mutate | Parallel patch hypotheses. |
| `jeryu.request_merge` | high-risk mutate | Merge request action; TUI must force proof gate. |
| `jeryu.bug_submit` | local mutate | Create canonical local bug. |
| `jeryu.bug_list` | read | Bug board. |
| `jeryu.bug_show` | read | Bug detail. |
| `jeryu.bug_ready` | read | Ready/unblocked bug queue. |
| `jeryu.bug_update` | local mutate | Triage/edit bug. |
| `jeryu.bug_record_attempt` | local mutate | Append attempt history. |

### 1.3 Durable data families available or implied

The TUI should treat these as first-class data families:

- **System/source health:** GitLab, DB, Docker, cache, Vault, MCP, broker, event cursor, freshness, settings profile, schema version.
- **CI/runners:** pools, managers, runner IDs/tokens/tags/executors, paused state, trust tier, runner managers, job events, CI job runs, tracked pipelines, runner assignment, queue duration, duration, status, web URL.
- **Pipelines/jobs/logs:** GitLab pipeline IDs/status/ref/SHA, bridges, child pipelines, job traces, artifacts, job names/stages/statuses, queue/run times, runner, failure capsules.
- **Capability/admission/audit:** capability intents, capability grants, admission decisions, git command events, ref updates, mirror jobs, risk approvals, command artifacts, action registry.
- **Evidence/retry/VTI:** evidence capsules, retry decisions, test executions, test plans, test plan items, selector misses, confidence, selected/skipped tests, affected subsystems, learning repairs.
- **Cache/provenance/materials:** cache objects, cache requests, hot entries, build/image signatures, force refresh rules, resolved refs, taints, leases, verdicts, promotions, material objects/aliases, action cache, epochs, toolchain fingerprints.
- **Secrets/Vault:** secret authorities, release secret sets, secret audit events, rotation/finalization/recovery status, redacted token fingerprints, Vault path metadata.
- **Bug tracker:** projects, project edges, bugs, bug events, attempts, links, external refs, evidence, sync status, owner/agent, status/severity/priority/difficulty.
- **Autonomy/Evidence Gate:** launch ledger, kill-bell state, verdicts, foundry candidates, LLM budget ledger, freeze checks, canary decisions, reviewer receipts.
- **Git/remote/repo fleet:** repo status, hooks, standards, fleet sync, mirrors, backups, remotes, remote nodes, SSH/tunnel/service status.
- **Security/artifacts/release:** GitLab scan artifacts, vulnerability summaries, Jankurai findings, secret scans, admission policy, artifact digests, signatures, SBOM/provenance, release attempts/gates/canary/prod/rollback.
- **LLM/provider telemetry:** provider health, model, token usage, latency, cost, failures, key pool state, data-use policy, redacted error reasons.

### 1.4 Defaults, ports, and paths worth showing in Source Doctor

| Setting / path | Value or purpose |
|---|---|
| GitLab HTTP / SSH | `8929` / `2224` |
| JeRyu webhook/API | `127.0.0.1:9777` |
| MCP HTTP | `127.0.0.1:9778` |
| Vault | `18200` |
| SmartCache proxy | `19800` |
| OCI registry mirror | `19801` |
| Settings | `~/.jeryu/settings.json` |
| Local env | `jeryu.env` |
| DB | `jeryu.sqlite` / `jeryu.db` / optional RedlineDB path by profile |
| Runner configs | `runners/` |
| Cache | `cache/` |
| Repo registry | `.jeryu/local/repos` |
| Autonomy config | `.jeryu/autonomy` |
| LLM provider chains | `.jeryu/autonomy/providers/llm.yml` |
| Important env | `GITLAB_PAT`, `JERYU_WEBHOOK_SECRET`, `GITLAB_ROOT_PASSWORD`, `JERYU_RELEASE_REPO_ROOT`, `JERYU_DATABASE_URL`, `JERYU_GITLAB_INSECURE_TLS`, Vault env, release vars. |
| Important headers | `X-Gitlab-Token`, `X-Gitlab-Event`, `X-Gitlab-Webhook-UUID`, `X-Jeryu-Token`. |

### 1.5 Known drift and risk to surface explicitly

| Drift / gap | TUI behavior |
|---|---|
| Some docs lag source for MCP/action tools. | Source Doctor shows docs/source/action-registry/MCP mismatch. |
| State docs may mention RedlineDB-only while current source indicates SQLite default and RedlineDB optional. | Header shows active backend/profile and DB latency. |
| MCP is tool-centric; no resources/prompts/watch stream today. | TUI uses native inspection API first; MCP resources are a plumbing target. |
| MCP HTTP GET is disabled today. | Show “MCP watch unavailable; polling” until streaming exists. |
| MR hooks are accepted/logged but not first-class state yet. | Label MR state partial until MR ingestion is complete. |
| Pipeline graph edges are undercomputed. | Display inferred edges as `INFERRED`; never fake exact DAG. |
| Live logs are polling-oriented. | Use trace polling fallback, but design for stream. |
| Evidence is not fully searchable proof timeline. | Build Evidence Ledger and backend query API. |
| Agents lack dedicated lifecycle table. | Add lifecycle table as P0/P1 backend work. |
| Docker/node metrics are underexposed. | Runner screen must say “resource unknown” until telemetry is plumbed. |
| `request_merge` is high-risk. | TUI must not call it directly without proof gate bound to exact SHA. |

---

## 2. Product doctrine

### 2.1 One mental model everywhere

Every screen is made of the same primitives:

```text
Object -> State -> Timeline -> Blockers -> Evidence -> Related Objects -> Actions
```

A job, release gate, cache object, runner, bug, agent, secret authority, Jankurai finding, and artifact all behave the same way:

- `Enter` drills into the object.
- `Esc` goes up one level.
- `a` opens actions.
- `e` opens evidence.
- `l` opens logs/trace if available.
- `x` explains blockers.
- `/` filters/searches.
- `Space` pins/unpins to inspector.

### 2.2 Never lie about state

Every fact must have freshness and provenance:

| Label | Meaning |
|---|---|
| `LIVE` | Updated by stream or fresh API call within TTL. |
| `FRESH 0.8s` | Polled/reconciled recently. |
| `STALE 12s` | Last value is older than TTL but source may recover. |
| `LAST KNOWN` | Source is down/disconnected; value may be obsolete. |
| `INFERRED` | TUI/backend derived from partial data. |
| `UNKNOWN` | Better than fake certainty. |
| `CONFLICT` | Two sources disagree; Source Doctor must explain. |

A pane cannot show “running” or “safe” without source and last-update metadata available through inspector.

### 2.3 Plan-first, live-overlay workflow

The flagship `0 Workflow` screen shows the intended validation/release journey before all jobs exist, then overlays live GitLab/job state as it arrives:

- planned validation graph
- VTI plan
- CI jobs and stages
- Jankurai audit
- security scans
- artifact/signature/provenance gates
- merge witness / merge passport
- release candidate / canary / Nightwatch / production
- rollback readiness

This solves the “show all needed tests in execution order” requirement even when GitLab has not materialized the full job list yet.

### 2.4 Human trust beats animation

The TUI should look alive, but animation must never reduce trust or readability.

- Animate only data changes, not random decoration.
- Pulse changed cells for 1-2 frames.
- Move progress bars only when fresh events arrive or estimates update.
- Animate DAG edges to show live critical path, but support `--no-animation` and low-motion mode.
- Show exact event cursor and frame stats so users know whether the UI is alive.

### 2.5 Mutating actions are evidence-gated

Every mutation flows through:

```text
select entity -> action menu -> preview -> proof/dry-run -> confirm -> execute -> stream progress -> receipt/audit
```

Risk tiers drive friction:

| Tier | Examples | Confirmation |
|---|---|---|
| Read | open logs, inspect cache, explain blocker, fetch capsule | none |
| Low | retry a job, create local bug, add note, fetch artifacts | Enter confirmation or immediate with undo where safe |
| Medium | cache GC, run targeted tests, assign agent, pause pool, drain one runner | preview required |
| High | scale pool, update workflow config, force cache refresh, broad runner drain | proof modal + explicit confirmation |
| Production | merge, promote prod, rollback, rotate secrets, waive security gate | proof modal + grant/approval + typed confirmation + exact SHA/digest binding |

---

## 3. Final information architecture

### 3.1 Top-level lenses

The tabs should optimize for “what is happening, why, and what should I do?”

| Key | Lens | Purpose |
|---:|---|---|
| `0` | **Workflow** | Default atlas: end-to-end planned + live delivery graph across current scope. |
| `1` | **Mission** | Whole-fleet posture, attention queue, next action, safe-to-code/merge/release/rollback. |
| `2` | **Repos** | Repo families, isolated repos, health, queue/load/security/release summaries. |
| `3` | **Queue** | Live queue, theoretical limit, bottleneck decomposition, runner scaling answer. |
| `4` | **Pipelines** | Pipeline DAGs, jobs, stages, traces, child pipelines, critical path. |
| `5` | **Runners** | Pools, runner managers, nodes, Docker/host pressure, scale/pause/drain controls. |
| `6` | **Cache** | SmartCache storage/speed/trust/taint/provenance/GC. |
| `7` | **VTI** | Smart test skipper proof: selected/skipped, confidence, misses, savings, learning. |
| `8` | **Agents** | Agent sessions, grants, races, logs, patch workflows, config editor. |
| `9` | **Bugs** | Cross-repo bug board, attempts, ownership, agent progress, evidence. |
| `r` | **Release** | Release/canary/prod/rollback/version lineage/evidence gates. |
| `j` | **Jankurai** | Audit scores, caps/issues, duplication, trends, versions per repo. |
| `s` | **Security** | Security findings, secrets, admission, policy, provenance risk. |
| `a` | **Artifacts** | Signed artifacts, SBOMs, digests, provenance, rollback candidates. |
| `g` | **Git Sync** | Local/remote/mirror/branch/PR/MR/admission drift. |
| `e` | **Evidence** | Searchable proof timeline / flight recorder / time travel. |
| `l` | **LLMs** | Provider health, cost, token budget, agent model routing. |
| `d` | **Doctor** | Source freshness, config, API/MCP drift, docs drift, schema versions. |

For narrow terminals, show `0–9` and put the rest in `More ▸`; all remain accessible through `g` jump menu and `:` command palette.

### 3.2 Scope stack

Every lens works at global, family, repo, pipeline, job, or entity scope:

```text
GLOBAL
  -> Family: veox-*
      -> Repo: veox-api
          -> Pipeline: #8123 main@abc123
              -> Job: cargo-test-linux
                  -> Trace line / artifact / evidence capsule
```

The header breadcrumb must be always visible when possible:

```text
GLOBAL ▸ veox-* ▸ veox-api ▸ pipeline #8123 ▸ cargo-test-linux ▸ line 9041
```

### 3.3 Universal navigation keys

| Key | Action |
|---|---|
| `↑↓←→` | Move spatially inside focused pane; in DAG, move to graph neighbor. |
| `h/j/k/l` | Vim aliases for movement. |
| `Tab` / `Shift-Tab` | Cycle pane focus. |
| `Enter` | Drill into selected entity. |
| `Esc` | Pop route / close modal / go up one level. |
| `Backspace` | Parent object; useful where Esc has latency. |
| `[` / `]` | Previous/next sibling entity or event cursor in replay mode. |
| `{` / `}` | Previous/next repo family. |
| `/` | Search/filter current pane. |
| `Ctrl-/` | Global search. |
| `:` or `.` | Command palette, seeded with current entity. |
| `?` | Contextual help. |
| `a` | Action menu for selected entity. |
| `A` | Global actions. |
| `x` | Explain blocker / why waiting / why not green. |
| `e` | Evidence. |
| `l` | Logs/trace. |
| `d` | Diff/details. |
| `t` | Timeline. |
| `f` | Filter or follow mode depending context. |
| `r` | Refresh current scope. |
| `R` | Reconcile/retry with preview. |
| `Space` | Pin/unpin selected entity in inspector; in DAG focus subtree. |
| `z` | Zoom selected graph node/subgraph. |
| `m` | Toggle minimap or merge detail depending context. |
| `y` | Copy selected entity id/URL/SHA/path. |
| `q` | Quit, with confirmation if actions are running. |

### 3.4 Expert quick keys

| Key | Context | Action |
|---|---|---|
| `o` | URL/path/object | Open externally. |
| `L` | log | Follow/unfollow live tail. |
| `n/N` | log/evidence | Next/previous annotation or finding. |
| `E` | failure/log | Open/create evidence capsule. |
| `B` | job/finding/release | Create/link bug or rollback depending context. |
| `C` | cache | Cache provenance / GC preview. |
| `V` | VTI | Validate plan / show VTI proof. |
| `J` | repo | Open/run Jankurai audit. |
| `S` | pool/family | Scale simulation / scale preview. |
| `D` | runner/node/pool | Drain preview. |
| `M` | MR/release | Merge/promote proof modal. |
| `K` | agent/workflow | Kill/pause/resume preview. |

---

## 4. Visual system

### 4.1 Terminal capability targets

The TUI should support:

- truecolor / 256-color / 16-color fallback
- Unicode + ASCII fallback
- mouse capture optional
- alternate screen
- no-color mode
- low-motion mode
- narrow terminal mode
- headless capture/screenshot mode

### 4.2 Semantic palette

Use semantic styles only; never ad hoc colors.

| Semantic | Use | Suggested tone |
|---|---|---|
| `bg` | base background | deep navy/black |
| `panel` | pane background | dark slate |
| `border` | inactive border | slate gray |
| `border.focus` | focused pane | cyan/blue |
| `text` | normal | light gray |
| `muted` | stale/secondary | gray |
| `ok` | success/safe/verified | green |
| `info` | running/live/selected | cyan |
| `queued` | ready/queued | blue/white |
| `warn` | degraded/waiting | amber |
| `bad` | failed/unsafe | red |
| `critical` | production/security emergency | bright red/magenta |
| `agent` | autonomous actors | purple |
| `cache` | cache/storage/trust | teal/cyan |
| `release` | release/prod/canary | teal/gold |
| `security` | secrets/admission/findings | magenta/red |
| `proof` | evidence/signature/provenance | gold |
| `diff.add` | additions | green |
| `diff.del` | deletions | red |
| `unknown` | unknown/unavailable | gray |

### 4.3 Status glyphs

| Meaning | Unicode | ASCII fallback |
|---|---:|---|
| passed/healthy | `✓` | `OK` |
| running | `▶` / `⟳` | `RUN` |
| queued/runnable | `●` | `Q` |
| waiting/dependency | `…` | `WAIT` |
| skipped by VTI | `↷` | `SKIP` |
| cache hit/reused | `◆` | `HIT` |
| cache miss | `◇` | `MISS` |
| warning | `▲` | `!` |
| failed | `✕` | `FAIL` |
| blocked | `⛔` | `BLOCK` |
| security risk | `⚑` | `SEC` |
| stale | `◌` | `STALE` |
| agent-owned | `◆` / `🤖` | `AGENT` |
| signed | `✦` | `SIG` |
| release/prod | `⬢` | `REL` |
| rollback | `↶` | `RB` |
| tainted | `☣` | `TAINT` |
| evidence/proof | `§` | `EV` |

### 4.4 Progress bars

Measured progress:

```text
build-linux  ▶ ███████████████░░░░░ 74%  04:12/05:41  runner:r17  cache:hit  logs:2.1k/s
```

Estimated progress:

```text
test-linux   ▶ ███████░░░░░░░░░░░░ ~43% est  conf:0.62  ETA 07:12±02:20
```

Unknown progress:

```text
security-scan ▶ ???????????????????? unknown  live logs available
```

VTI/cache overlays:

- dim segment: skipped by VTI
- cyan marker: cache hit
- yellow marker: waiting/blocked
- red marker: failure point
- gold marker: signed/evidence-producing step

### 4.5 Motion language

Use motion sparingly:

- new event: one-frame pulse in event tape + entity row
- running job: spinner plus progress bar update
- stream connected: subtle arrow/cursor movement in header
- DAG edge: animated `·` flow only on active critical path
- bottleneck: heat shimmer/pulse in relevant capacity cell
- stale: fade/dim, not blink
- critical: blink only if accessibility setting allows; otherwise bold red/magenta + glyph

---

## 5. Global shell and responsive layout

### 5.1 Ideal wide layout

```text
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ JeRyu Workflow Atlas  LIVE  prod  db:sqlite  GitLab ✓  Docker ✓  Cache ▲  Vault ✓  MCP ✓  seq=193847  fresh=0.8s  09:41 │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ 0 Workflow  1 Mission  2 Repos  3 Queue  4 Pipelines  5 Runners  6 Cache  7 VTI  8 Agents  9 Bugs  r Release  More ▸       │
├───────────────┬──────────────────────────────────────────────────────────────────────────────────────┬───────────────────────┤
│ SCOPE / NAV   │ MAIN CANVAS                                                                          │ INSPECTOR / ACTIONS    │
│ Families      │ Graph/table/heatmap/timeline/logs depending current lens                             │ selected entity        │
│ ▸ veox-*      │                                                                                      │ state                  │
│   isolated    │                                                                                      │ blockers               │
│   infra       │                                                                                      │ evidence               │
│ Saved lenses  │                                                                                      │ actions                │
│ ▸ Hot Queue   │                                                                                      │ related                │
│   Prod Risk   │                                                                                      │ live tail              │
├───────────────┴──────────────────────────────────────────────────────────────────────────────────────┴───────────────────────┤
│ ↑↓←→ move  Tab pane  Enter drill  Esc up  / search  : command  a actions  e evidence  l logs  x explain  ? help  frame 7ms │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 5.2 Responsive breakpoints

| Width | Layout behavior |
|---:|---|
| `<100` | Single focused pane; compact header; inspector as overlay. |
| `100–139` | Header + tabs + main pane + collapsible inspector. |
| `140–179` | Left scope + center main + right inspector. |
| `180+` | Full cockpit: scope rail, main canvas, inspector, event tape/minimap. |

Height behavior:

| Height | Behavior |
|---:|---|
| `<28` | Compact mode: one-line header, short tabs, hide event tape. |
| `28–44` | Standard dashboard; bottom detail panes collapsed. |
| `45+` | Full details: logs/events/annotations can remain visible. |

### 5.3 Header contract

The header must always answer:

```text
profile • backend • schema • event cursor • source freshness • safe posture • queue pressure • runner pressure • cache pressure • release/security posture
```

Example:

```text
JERYU  prod  db:sqlite  schema:v4  seq:184923↑  GitLab:0.8s  queue:84/112  cap:91%  runners:28/32  cache:81%  vti:+4.2h  sec:2C/9H  rel:v2.8.1 canary
```

Header fields are drillable:

| Field | Drilldown |
|---|---|
| profile | Settings/runtime profile |
| db | DB inspector/schema/migrations/latency |
| GitLab | API health/rate limit/webhook status |
| seq | Evidence/event ledger |
| queue/cap | Queue/theoretical-limit screen |
| runners | Runners/system utilization |
| cache | Cache observatory |
| VTI | VTI cockpit |
| sec | Security center |
| rel | Release control |
| fresh | Source Doctor |

---

## 6. Screen 0: Workflow Atlas

### 6.1 Purpose

The Workflow Atlas is the flagship. It shows the end-to-end validation and delivery graph across the selected scope, before and after jobs materialize.

It must show:

- local branch/worktree readiness
- VTI planning
- lint/build/test/security/Jankurai gates
- artifact/signature/provenance gates
- merge/mr/passport gate
- release candidate/foundry/canary/prod/rollback
- live job status and progress
- critical path
- bottleneck/lost-limit explanation
- agent ownership and evidence

### 6.2 Workflow layers

```text
L0 local/repo state
L1 plan/VTI/impact analysis
L2 fast CI
L3 certification/full CI
L4 audit/security/Jankurai
L5 package/artifact/sign/provenance
L6 merge/passport
L7 release candidate/foundry
L8 canary/Nightwatch
L9 production promotion
L10 rollback readiness
```

### 6.3 Node card design

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

VTI skipped/cached card:

```text
╭ui-smoke-tests────────────────╮
│ ↷ SKIP by VTI  conf 0.94     │
│ no impacted paths            │
│ last pass main@9fd2  2h ago  │
│ Enter: proof                 │
╰──────────────────────────────╯
```

### 6.4 Global Workflow Atlas mock

```text
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Workflow Atlas  LIVE  GLOBAL  frontier=86%  runnable=41/48 slots  queue=19 jobs  blockers=7  prod=canary-green              │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ 0 Workflow  1 Mission  2 Repos  3 Queue  4 Pipelines  5 Runners  6 Cache  7 VTI  8 Agents  9 Bugs  r Release  More ▸       │
├───────────────┬──────────────────────────────────────────────────────────────────────────────────────┬───────────────────────┤
│ Scope         │ GLOBAL DELIVERY GRAPH                                                               │ Selected               │
│ ▸ GLOBAL      │                                                                                      │ veox-api / rust-tests   │
│   veox-*      │  L0 PLAN        L1 FAST CI            L2 CERTIFY             L3 RELEASE             │ ▶ running 63%           │
│   isolated    │  ╭vti-plan────╮ ╭lint──────╮ ╭unit-linux────────╮ ╭jankurai──────╮ ╭canary──────╮   │ ETA 2m33s               │
│   infra       │  │✓ 38s       │ │✓ 01:02   │ │▶ 63% 04:12      │ │✕ cap dup    │ │✓ ring 10%   │   │ runner rust-hi-03       │
│               │  ╰───────────╯ ╰──────────╯ ╰─────────────────╯ ╰───────────────╯ ╰────────────╯   │ queue wait 00:03        │
│ Hot lenses    │        │              │          │       │                 │              │        │ cache hit 78%            │
│ ▸ Queue limit │        ▼              ▼          ▼       ▼                 ▼              ▼        │ VTI selected            │
│   Jankurai ↓  │  ╭graph-audit╮ ╭clippy────╮ ╭integration──────╮ ╭security──────╮ ╭prod-ready──╮   │ stdout tail             │
│   Cache full  │  │✓ no leak   │ │✓ 02:31   │ │● queued rust-hi │ │✓ no secrets  │ │… waits audit│   │  test auth::jwt...       │
│   Prod risk   │  ╰───────────╯ ╰──────────╯ ╰─────────────────╯ ╰──────────────╯ ╰────────────╯   │  ok api::login          │
│               │                                                                                      │ Actions                 │
│ Repo heat     │  CRITICAL PATH: veox-api unit-linux → jankurai-audit → prod-ready                    │ [Enter] trace           │
│ veox-api  ✕   │  LOST LIMIT: 4 idle tag mismatch, 2 cache IO, 1 approval, 3 serial deps              │ [r] retry after fail     │
│ veox-ui   ✓   │                                                                                      │ [b] bottleneck          │
│ veox-db   ▶   │                                                                                      │ [a] actions             │
├───────────────┴──────────────────────────────────────────────────────────────────────────────────────┴───────────────────────┤
│ ↑↓←→ graph nav  Tab pane  Enter drill  Esc up  Space subtree  z zoom  m minimap  / filter  : command  F5 refresh             │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 6.5 Graph navigation rules

- Arrow keys move to nearest graph neighbor in that direction.
- `Enter` drills into node detail.
- `l` opens logs when node maps to job/agent/action.
- `e` opens evidence/proof when node maps to gate/artifact/signature.
- `z` zooms selected subgraph.
- `Space` isolates subtree and dims unrelated nodes.
- `P` pins critical path.
- `m` toggles minimap.
- `[` and `]` switch among critical path nodes.

---

## 7. Screen 1: Mission Control

### 7.1 Purpose

Mission is the one-minute executive cockpit. It shows posture, attention queue, next action, capacity, agents, cache, VTI, release, Jankurai, security, and evidence.

### 7.2 Mission mock

```text
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Mission  GLOBAL  SAFE-CODE ✓  SAFE-MERGE ⚠  SAFE-RELEASE ✕  SAFE-ROLLBACK ✓  top: veox-api Jankurai duplicate-code cap      │
├───────────────────────────────┬───────────────────────────────────────────────┬──────────────────────────────────────────────┤
│ POSTURE                       │ ATTENTION QUEUE                               │ NEXT ACTION                                  │
│ Code      ✓ all repos writable│ 1 ✕ veox-api jankurai score 82 cap dup-code   │ Open veox-api Jankurai issue cluster #14     │
│ Merge     ⚠ 3 MRs need proof  │ 2 ⚠ rust-hi pool 86% frontier, 4 tag-idle     │ Why: prod-ready is waiting on audit cap       │
│ Release   ✕ canary green but  │ 3 ⚠ cache 91% full, crates 128GiB             │ Risk: read/triage                            │
│             audit blocking    │ 4 ⚑ sec: 1 secret denied event in veox-ui     │ Key: Enter drill, a actions, ! escalate       │
│ Agents    ✓ 12 active, 0 rogue│ 5 … isolated/db migration pipeline queued     │                                              │
├───────────────────────────────┴───────────────────────────────────────────────┴──────────────────────────────────────────────┤
│ CI FRONTIER: Running 41/48 effective slots  ████████████████████████████████░░░░ 86%                                        │
│ Loss: tag mismatch 4 slots | cache IO 2 slots | approval 1 slot | serial deps 3 jobs | remote node stale 1                    │
├───────────────────────────────┬───────────────────────────────┬───────────────────────────────┬──────────────────────────────┤
│ VTI                           │ CACHE                         │ JANKURAI                      │ RELEASE                      │
│ saved 11h today  conf .93     │ 91% full  hit 78%             │ fleet score 88.1 ↓1.7         │ rc 1.14.0 sha abc123          │
│ misses 2 / 7d  false-skip 0   │ crates 128G target 84G OCI 31G│ caps 3 dup 8 complexity 4     │ canary ✓ prod waits audit     │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 7.3 Attention ranking

```text
score = severity_weight
      + blast_radius(repo/family/release/prod/security)
      + critical_path_impact_minutes
      + queue_impact_minutes
      + human_action_required_boost
      + security_or_secret_boost
      + repeated_failure_boost
      + staleness_or_source_conflict_boost
      - agent_already_working_discount
      - low_confidence_discount
```

Every attention item has:

```rust
pub struct AttentionItem {
    pub id: String,
    pub severity: Severity,
    pub entity: EntityRef,
    pub summary: String,
    pub why_now: String,
    pub impact: ImpactSummary,
    pub recommended_action: Option<ActionDescriptor>,
    pub owner: Option<ActorRef>,
    pub stale: bool,
    pub confidence: f32,
}
```

---

## 8. Screen 3: Queue, theoretical limit, and runner-count decision engine

### 8.1 Purpose

This screen answers the user’s explicit operational question: **How close are we to the theoretical limit, and should we increase runner count?**

It must distinguish:

- runnable queue vs blocked queue
- runner slots vs effective slots
- configured theoretical capacity vs live online capacity
- CPU/memory/disk/network saturation vs scheduling saturation
- tag/trust-tier mismatch vs true lack of runners
- cache and Docker pressure vs runner count
- serial DAG critical path vs parallelism opportunity
- VTI too conservative vs VTI unsafe
- GitLab/API/rate-limit bottlenecks vs runner bottlenecks

### 8.2 Required definitions

| Term | Definition |
|---|---|
| Online slots | Currently available runner concurrency from live managers/runners. |
| Theoretical slots | Maximum possible concurrency from configured pool limits, remote node limits, GitLab runner limits, request concurrency, and global caps. |
| Effective slots | Online slots adjusted by observed speed factors and bottlenecks. |
| Busy slots | Slots currently running jobs. |
| Runnable queue | Queued jobs with dependencies satisfied and matching eligible tags/trust. |
| Blocked queue | Jobs blocked by DAG, manual approval, release/security gate, missing artifact, or no matching resource. |
| Queue pressure | Weighted runnable queued work divided by near-term service capacity. |
| Limit distance | Current projected wall time divided by ideal critical-path lower bound. |
| Headroom | Theoretical slots minus online slots, also constrained by host/node resources. |
| Waste | Work seconds lost to retries, flakes, cache misses, queue wait, cold starts, canceled jobs, and avoidable full suites. |

### 8.3 Capacity formulas

Pool theoretical slots:

```text
theoretical_slots(pool) = min(
  pool.max_managers * pool.runner_concurrency,
  pool.request_concurrency_limit,
  remote_node_available_slots(pool),
  gitlab_runner_limit(pool),
  optional_global_cap
)
```

Online slots:

```text
online_slots(pool) = sum(manager.online ? manager.configured_concurrency : 0)
```

Busy slots:

```text
busy_slots(pool) = count(running jobs assigned to pool/tag/trust tier)
```

Predicted job work:

```text
work_seconds(job) =
  if running: remaining_runtime_observed_or_estimated
  else historical_p50(repo, job_name, stage, runner_class, cache_state)
  else historical_p50(stage, runner_class)
  else conservative_default(stage)
```

Weighted service rate:

```text
weighted_capacity_sec_per_sec(pool) = sum(slot_speed_factor(slot))
effective_service_rate(pool) = weighted_capacity_sec_per_sec(pool) * pressure_factor(pool)
```

Pressure factor should penalize:

```text
pressure_factor = cpu_factor * memory_factor * disk_factor * network_factor * cache_factor * gitlab_api_factor * docker_factor
```

Critical path lower bound:

```text
critical_path_min = longest_path_sum(stage/job historical p50) over workflow DAG assuming infinite runners
current_projection = simulated schedule over online/effective slots and dependency constraints
limit_distance = current_projection / critical_path_min
```

Interpretation:

| Limit distance | Meaning |
|---:|---|
| `1.00–1.15×` | Near physical/DAG lower bound. Adding runners probably has little effect. |
| `1.15–1.50×` | Good but improvable. Investigate constrained pool, cache, or one serial stage. |
| `1.50–2.00×` | Meaningful waste. Scaling or bottleneck fix likely helps. |
| `>2.00×` | Severe underutilization/bottleneck. Run Bottleneck Lab and fix highest loss. |

### 8.4 Node/core/memory safety formulas

To avoid over-scaling into CPU/memory exhaustion, compute node-level safe slots:

```text
cpu_safe_slots(node, runner_class) = floor((cpu_cores * target_cpu_util - reserved_cores) / p95_cpu_cores_per_job)
mem_safe_slots(node, runner_class) = floor((available_mem_bytes * target_mem_util - reserved_mem_bytes) / p95_rss_bytes_per_job)
disk_safe_slots(node, runner_class) = floor(available_iops_or_bytes_per_sec / p95_disk_need_per_job)
net_safe_slots(node, runner_class) = floor(available_net_bytes_per_sec / p95_net_need_per_job)
node_safe_slots = min(cpu_safe_slots, mem_safe_slots, disk_safe_slots, net_safe_slots, docker_safe_slots, cache_mount_safe_slots)
```

Recommended defaults:

| Resource | Green | Warning | Critical | Action |
|---|---:|---:|---:|---|
| CPU p95 utilization | `<70%` | `70–85%` | `>85%` | Add managers only if queue-bound and jobs are IO-waiting; otherwise add nodes. |
| Load/core | `<1.0` | `1.0–1.5` | `>1.5` | High load/core blocks more local runners. |
| Memory available | `>30%` | `15–30%` | `<15%` | Do not add local runners near critical; add nodes or reduce concurrency. |
| Swap activity | `0` | low | any sustained | Treat as critical for CI runners. |
| Disk used | `<75%` | `75–85%` | `>85%` | Run GC before adding runners if builds/cache write heavily. |
| Inodes used | `<70%` | `70–85%` | `>85%` | GC before scale. |
| IO wait | `<5%` | `5–15%` | `>15%` | More runners likely worsens jobs. |
| Network | `<60%` | `60–80%` | `>80%` | Pre-pull/cache or add nodes near data. |
| Docker restarts/OOM | `0` | occasional | repeated | Fix stability before scale. |
| Cache pressure | `<75%` | `75–85%` | `>85%` | GC/expand cache first. |

### 8.5 Runner-count decision matrix

The TUI should produce a recommendation with evidence and confidence:

| Condition | Recommendation |
|---|---|
| Busy slots `>85%` sustained, runnable queue high, blocked queue low, CPU/mem/disk/network green, theoretical headroom exists | **Increase runner managers** for constrained pool. |
| Busy slots high, runnable queue high, local CPU/memory/disk critical | **Do not increase local runner count**. Add remote nodes/hardware or reduce concurrency. |
| Queue high but idle slots exist with wrong tags/trust/protected settings | **Rebalance tags/trust tiers**; adding generic runners may not help. |
| Queue high because jobs are blocked by DAG/manual/release/security gates | **Do not add runners**; fix gate/approval/DAG. |
| Runners idle but GitLab API/rate limit/source stale | **Fix GitLab/API/reconciliation**; runner count is not the first bottleneck. |
| Queue high and cache miss/cold image dominates | **Warm cache/pre-pull/GC/expand cache** before scale. |
| VTI is too conservative and full suites explode | **Tune VTI / validate selector misses** before scale. |
| Serial critical path dominates limit distance | **Split/shard long serial job**; runners have limited effect. |
| Theoretical slots far above online slots but remote nodes down | **Repair remote nodes/tunnels/services** before increasing pool config. |
| Effective slots much lower than online slots | **Diagnose pressure factor**; adding runners may worsen saturation. |

### 8.6 Queue screen mock

```text
╭─ Queue / Theoretical Limit ───────────────────────────────────────────────────────────────────────────────────────╮
│ Fleet: all repos  Window: live + 24h history  Model: p50/p90 by repo+job+stage  Confidence: 0.81                  │
├─ Summary ─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Online slots 87 / Configured theoretical 160 / Effective 104 │ Busy 79 │ Queued 42 │ Drain ETA 18m p50 / 31m p90 │
│ Limit-distance 1.34×  Critical-path floor 13m27s  Projected 18m02s  Waste 4m35s  Main cause: tag bottleneck TEST  │
├─ Pools ──────────────────────────────────────────────────────┬─ Queue by constraint ─────────────────────────────╮
│ Pool              on/th/eff  busy q  util p95 wait  diagnosis│ Constraint             jobs work    fix            │
│ ▶ rust-fast        24/48/31   24 18 100%  12m04s  saturated  │ tag=rust-fast           18  9h12m   scale +9       │
│   rust-default     33/60/41   29  8  88%   3m11s  ok         │ needs docker socket      7  2h01m   add remote     │
│   gpu-audit         2/ 4/ 2    2  3 100%  21m40s  scarce     │ serial release gate      4  1h10m   no runner fix  │
│   sec-scan          4/ 8/ 4    3  5  75%   8m32s  image pull │ image cold-start         9  2h20m   pre-pull/cache │
│   remote-nyc       18/32/21   16  6  89%   4m50s  disk warn  │ disk pressure            6  1h44m   GC/buildkit    │
├─ Critical path ───────────────────────────────────────────────┴───────────────────────────────────────────────────┤
│ veox-api#581  build ✓ 2m11s ─► integ ✕ running 9m/14m ─► audit ◌ 4m ─► package ◌ 2m ─► release blocked by integ │
│ Slowest deltas vs 7d: integ +42%, cargo-deny +31%, image-build +28%, queue wait rust-fast +214%                  │
╰─ Keys: Enter pool drill  s scale preview  d diagnose  h history  / filter  x explain limit-distance ───────────╯
```

### 8.7 Scale preview modal

```text
╭─ Action Preview: scale pool rust-fast +9 ──────────────────────────────────────────────────────────────────────────╮
│ Risk: medium │ Dry-run: available ✓ │ Actor: human:ben │ Idempotency: tui-183772-scale-rust-fast                 │
├─ Effects ──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ + start up to 9 runner managers on remote nodes nyc-1,sfo-2                                                       │
│ + expected p95 queue wait rust-fast 12m04s → 4m10s                                                                │
│ ▲ remote-nyc disk 91%; effective gain only +7 until GC                                                            │
│ ✓ no running jobs killed                                                                                          │
├─ Evidence / checks ────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ✓ pool not paused  ✓ token valid  ✓ remote nodes reachable  ▲ disk pressure  ✓ GitLab request concurrency ok      │
├─ Recommendation ───────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Best path: run GC remote-nyc buildkit 41GiB, then scale rust-fast +9. Confidence 0.78.                             │
╰─ Enter execute  d dry-run  g run recommended GC first  x explain  Esc cancel ─────────────────────────────────────╯
```

---

## 9. Screen 5: Runners, nodes, and system utilization

### 9.1 Purpose

This screen answers “How close are we to core/memory/disk/network issues?” and “Can we safely run more runners?”

### 9.2 Required telemetry

| Domain | Required fields |
|---|---|
| Pool | min/max managers, online managers, desired managers, concurrency, request concurrency, paused, trust tier, backend, tags. |
| Runner | runner ID, tags, executor, protected flag, contact time, version, config hash, token state. |
| Manager | manager/container/pod ID, system ID, node alias, state, job ID, image digest, restart count, OOM events. |
| Node | alias, CPU cores, load, p50/p95 CPU, memory used/free, swap, disk bytes/inodes, IO wait, network, Docker health. |
| Remote | heartbeat age, SSH latency, tunnel state, service version, disk/cache pressure, node affinity. |
| Queue | jobs by pool/tag/trust, runnable/blocked, p50/p95 wait, oldest queued, work seconds. |
| Cache | cache mount usage by node/pool, BuildKit size, target dirs, crates, OCI layers, sccache. |
| Docker | events, image pulls, pull latency, build cache, daemon errors, container stats. |

### 9.3 System mock

```text
╭─ System / Runners / Nodes ─────────────────────────────────────────────────────────────────────────────────────────╮
│ local host CPU 68% mem 74% disk 82% inode 51% │ remote nodes 6/7 ok │ docker events 2 OOM today │ fresh 1.0s       │
├─ Pools ───────────────────────────────────────────────┬─ Nodes ─────────────────────────────────────────────────╮
│ Pool          managers slots busy util q  pressure    │ Node          CPU mem disk cache runners state          │
│ ▶ rust-fast   12/24    24/48 24 100%18 saturated ▲    │ local         68  74  82   337G  31      disk warn       │
│   rust-def    17/30    33/60 29  88% 8 ok             │ remote-nyc-1  81  62  91   188G  12      cache critical  │
│   gpu-audit    1/ 2     2/ 4  2 100% 3 scarce         │ remote-sfo-2  55  49  63    99G  10      ok             │
├─ Selected rust-fast ──────────────────────────────────┴───────────────────────────────────────────────────────────┤
│ Can add +24 theoretical slots, but remote-nyc disk pressure limits effective to +7 until GC. Startup p95 2m42s.   │
│ Recommended: GC remote-nyc buildkit 41GiB, then scale rust-fast +9. Estimated queue p95 12m → 4m.                 │
╰─ Keys: s scale  g GC  d drain  p pause  n node detail  l logs  x explain recommendation ─────────────────────────╯
```

### 9.4 Runner actions

- scale pool preview
- pause/resume pool
- drain runner/pool/node
- rotate token
- restart manager
- remote node doctor
- install/update remote runner
- reclaim host storage
- pre-pull runner images
- rebalance tags
- change request concurrency

All actions must show blast radius, running-job impact, rollback action, and audit destination.

---

## 10. Screen 2: Repo families and repo drilldown

### 10.1 Family grouping rules

Repo families are first-class objects. A repo can belong to multiple views:

- prefix/glob: `veox-*`
- explicit config group
- GitLab namespace/group
- product/release train
- team/owner
- trust tier
- agent ownership
- isolated/shared runner/cache policy
- bug project graph edges

Example config:

```toml
[[repo_groups]]
id = "veox"
label = "veox-*"
match = ["veox-*", "apps/veox-*"]
release_train = "foundry-main"
shared_cache = "veox-cargo"
shared_runner_tags = ["linux-large", "rust-fast"]

[[repo_groups]]
id = "isolated"
label = "isolated repos"
match = ["!veox-*", "tag:isolated"]
```

### 10.2 Repo family mock

```text
╭─ Repos ▸ Family veox-* ─ 18 repos ─ queue=72 ─ running=41 ─ frontier=86% ─ bugs=39 ─ release blockers=2 ─ cache=91% ─╮
│ Families      │ REPO FAMILY: veox-*                                                                  │ Family inspector      │
│ ▸ veox-*      │ Repo          State  Queue  Front  CI       VTI        Cache     Jankurai   Bugs Git │ Theoretical limit     │
│   isolated    │ veox-api      ✕      8/14   92%    fail     .94 4h sv  78% ⚠     82↓ cap    7/2  ✓   │ 48 effective slots    │
│   infra       │ veox-ui       ✓      0/3    51%    pass     .97 1h sv  84%       91↑        2/0  ✓   │ 41 active             │
│ Lenses        │ veox-db       ▶      6/9    88%    run      .89 miss1  61%       88→        3/1  ⚠   │ 4 tag-idle            │
│ ▸ Hot queue   │ veox-deploy   ⚠      1/2    34%    wait     n/a        73%       93→        1/0  ✓   │ 2 cache-IO            │
│   Bugs ready  │ veox-agent    ◆      2/4    74%    run      .92        80%       86↓        5/3  ✓   │ 1 approval            │
│   Score drops │                                                                                      │ Add runners? +4 helps │
│               │ Trends: CI p95 ↓8%  cache hit ↑3%  VTI saves ↑14%  Jankurai ↓1.7  churn ↑22%       │ queue p95 -41%        │
╰─ Enter repo  / filter  s sort  f family graph  c compare  j Jankurai  v VTI  b bottlenecks  Esc global ───────────────╯
```

### 10.3 Repo row columns

| Column | Meaning |
|---|---|
| Repo | name/path/project ID |
| State | health summary: pass/running/fail/blocked/stale |
| Queue | queued/running/blocked jobs |
| Front | frontier/effective utilization for repo’s eligible slots |
| CI | latest pipeline state and critical path |
| VTI | confidence, saved time, selector misses |
| Cache | hit ratio, bytes, taints, pressure |
| Jankurai | score, trend, cap/fail/warn, version |
| Bugs | ready/in-progress/blocked |
| Git | sync/divergence/admission/mirror state |
| Release | candidate/canary/prod/rollback status |
| Security | critical/high/secret/provenance blockers |
| Churn | recent added/removed lines and risky file changes |

### 10.4 Repo cockpit mock

```text
╭─ Repo veox-core ─ project:42 ─ family:veox-* ─ main:a83f91c ─ remote:in-sync ─ last main merge:09:12 ─────────────────╮
│ Safe to code: ✓   Safe to merge: ✗ selector miss + failing job   Safe to release: ⛔ canary gate   Agents:3 active       │
├ CURRENT WORKFLOWS ────────────────────────────────────────────────┬ REPO INSPECTOR ──────────────────────────────────────┤
│ Pipeline #8912  MR !221  branch:fix/cache-key  head:b91c2e       │ Top blocker: test-linux failed                         │
│                                                                    │ Critical path: test-linux -> package -> sign -> canary  │
│  prepare ✓ ─ build-linux ▶74% ─ test-linux ✗ ─ package … ─ sign … │ VTI: selected 213 / skipped 1,842  conf 93%  misses 2  │
│            └ build-macos  ✓  ─ test-macos  ✓ ──────────────────── │ Cache: hit 81%, target/ 88%, crates 64%, taints 1      │
│            └ lint         ✓  ─ jankurai    ▲ score 86 -2.4        │ Jankurai: dup cap, BAD_DOCKER warning                  │
│            └ security     ‼ secret? high                          │ Security: 1 critical secret finding                    │
│                                                                    │ Release: v2.8.1 canary waiting                         │
│ RECENT COMMITS                                                     │ Bugs: 4 ready, 1 in progress by agent-7                 │
│ b91c2e ben     +184 -29  cache key normalization                  │ Signed artifacts: pending                              │
│ a83f91c agent7 +030 -12  retry wrapper                             │                                                     │
╰─ Enter node  l logs  d diff  v VTI  j audit  a actions  m MR  r release  Esc family ────────────────────────────────────╯
```

---

## 11. Pipeline DAG and live trace cockpit

### 11.1 DAG edge types

| Edge | Style | Meaning |
|---|---|---|
| stage order | normal line | GitLab stage progression |
| `needs` | solid arrow | explicit dependency |
| artifact | dotted arrow | artifact consumed |
| child pipeline | double arrow | bridge/downstream pipeline |
| VTI skip | dim dashed | skipped by smart selection |
| cache dependency | cyan dotted | cache object/provenance involved |
| manual/blocked | yellow/red | human/gate required |
| failed critical path | red bold | failure blocks completion |
| cross-repo | long dashed | dependent repo/release train edge |

### 11.2 Pipeline mock

```text
╭─ Pipeline #8912 ─ veox-core !221 ─ head:b91c2e ─ elapsed 12:41 ─ predicted remaining 18:20 ─────────────────────────────╮
│ Critical path: build-linux ▶ test-linux ✗ package … sign … canary …       Runner pressure: linux-large 98%              │
├ DAG ───────────────────────────────────────────────────────────────────────┬ NODE DETAIL ───────────────────────────────┤
│   [prepare ✓ 00:31]                                                        │ test-linux #94812 ✗ failed                  │
│          │                                                                 │ stage:test  runner:r17/linux-large          │
│          ├──▶ [build-linux ▶ 74% 04:12/05:41] ───▶ [test-linux ✗ 04:12]    │ queued:1m20s  ran:4m12s                    │
│          │             │                           │                      │ first error: E0597 borrowed value...        │
│          │             │ artifact:target.tar        ├──▶ [package …]      │ cache: hit crates, miss target              │
│          │             └ cache:target-key:a93f      └──▶ [coverage …]     │ VTI: selected by files src/cache.rs         │
│          ├──▶ [build-macos ✓ 06:11] ───────────────▶ [test-macos ✓]        │ evidence: capsule#c91                       │
│          ├──▶ [lint ✓ 01:02]                                               │ actions: retry, fetch logs, create bug      │
│          └──▶ [security ‼ secret high]                                      │ related: bug#812, agent-7, MR!221           │
╰─ arrows graph  Enter drill  l logs  e evidence  c cache  v VTI  b blocker  p critical path  Esc repo ──────────────────╯
```

### 11.3 Trace viewer mock

```text
╭─ Trace job #94812 test-linux ─ offset 188k ─ follow:on ─ regex:E0597 ──────────────────────────────────────────────────╮
│ LOG                                                                            │ ANNOTATIONS                             │
│ 09:41:08.122  cargo test --workspace --all-features                            │ ✗ line 9041 E0597 borrow checker        │
│ 09:41:08.912  compiling veox-core v0.8.2                                       │ ▲ line 8130 cache miss target-key:a93f  │
│ 09:41:17.441  test cache::key_normalizes_paths ... ok                          │ § capsule c91 created                   │
│ 09:41:19.002  test cache::remote_taint_is_rejected ... FAILED                  │ related bug#812                         │
│ 09:41:19.006  thread panicked at src/cache/trust.rs:188                        │ suggested: retry after cache purge      │
│ 09:41:19.012  error[E0597]: borrowed value does not live long enough           │ CONTEXT                                 │
│ 09:41:19.013    --> src/cache/trust.rs:211:19                                  │ runner:r17 cpu:91% mem:62% io:high      │
╰─ L follow  n/N annotations  E capsule  B create bug  R retry  C cache provenance  / regex  Esc job ───────────────────╯
```

### 11.4 Trace requirements

- Bounded ring buffer; never load unbounded logs into memory.
- Byte-offset resume.
- GitLab trace range/poll fallback.
- WebSocket/SSE stream preferred.
- ANSI-aware rendering with strip/toggle.
- Timestamp delta/performance mode.
- Error folding and repeated-warning folding.
- Jump to first error, next warning, test failure, cache miss, artifact upload, security finding.
- Inline links to file:line, artifact, capsule, VTI decision, cache object, bug, agent.
- Redaction before display and before evidence export.

---

## 12. Cache Observatory

### 12.1 Purpose

Answer:

- Are we full?
- What types of files are taking storage?
- Why did this job miss?
- Which misses are expensive?
- What can be safely GC’d?
- Which cache objects are tainted or untrusted?
- How much time/bandwidth did cache save?

### 12.2 Categories

- `cargo_registry_index`
- `cargo_registry_crates`
- `cargo_git_checkouts`
- `cargo_target_debug`
- `cargo_target_release`
- `cargo_incremental`
- `sccache_objects`
- `oci_layers`
- `oci_manifests`
- `buildkit_cache`
- `job_artifacts`
- `material_cas`
- `action_cache`
- `logs_evidence`
- `unknown`

### 12.3 Cache mock

```text
╭─ Cache Observatory ─ disk 812GiB/1.0TiB 81% ─ hit 78% ─ miss storm: crates.io sparse ─ taints:3 ───────────────────────╮
│ Pressure: target/ 88% █████████████████░░  crates 64% ███████████░░░░  sccache 51% ████████░░░░  OCI 42% ██████░░░░    │
├ BY CATEGORY ─────────────────────────────┬ HOT OBJECTS / MISSES ───────────────────────┬ INSPECTOR ─────────────────────┤
│ Category        Size   Objects Hit%  Age │ Key                         Size Hits Verdict │ selected: target-key:a93f       │
│ Rust target     512G   18,402  61%   3d  │ target:veox-core:a93f        9.2G  41  tainted │ repo:veox-core                  │
│ Cargo registry  143G   91,110  94%   8d  │ crate:serde-1.0.203          1.2M 890  trusted │ mutability:derived              │
│ Cargo git        58G    2,021  82%   6d  │ target:veox-api:771b         7.8G  32  trusted │ taint: remote branch mismatch   │
│ sccache          48G  121,991  72%   2d  │ oci:rust-builder:sha256...   2.1G  28  trusted │ last hit: 09:32                 │
│ OCI layers       41G      982  88%   9d  │ miss: sparse_index_config    0.0G 312  n/a     │ GC eligible: no lease 2h        │
├ GC PLAN ─────────────────────────────────┴──────────────────────────────────────────────┴────────────────────────────────┤
│ reclaimable now: 83G  safe: 42G  risky: 41G  biggest win: stale target dirs from failed branches                         │
╰─ Enter object  g GC preview  p provenance  t taints  m misses  r force refresh preview  Esc global ──────────────────────╯
```

### 12.4 Cache actions

| Action | Risk | Preview requirements |
|---|---:|---|
| GC safe objects | medium | bytes reclaimed, leases respected, repos affected |
| Force refresh key | medium | jobs/repos depending on key and upstream source |
| Quarantine tainted object | low/medium | taint reason, current users, cache verdict history |
| Expand cache budget | high | disk availability, node impact, config diff |
| Promote cache entry | medium | provenance, digest, signature/build trust |

---

## 13. VTI smart test skipper cockpit

### 13.1 Purpose

Prove whether VTI is saving time safely.

The screen must answer:

- selected vs skipped tests
- confidence
- time saved
- net saved after miss penalty
- selector misses
- false-skip risk
- high-risk skipped tests
- learning status
- why each test was selected/skipped

### 13.2 VTI mock

```text
╭─ VTI Cockpit ─ repo:veox-core ─ plan #vti-7781 ─ confidence 93% ─ net saved 42m ─ misses 2 ─────────────────────────────╮
│ Changed files: src/cache/trust.rs src/cache/key.rs tests/cache_taint.rs                                                     │
├ PLAN SUMMARY ─────────────────────────────────────┬ SELECTED TESTS ─────────────────────────┬ SKIPPED RISK ─────────────┤
│ Baseline full suite:     2,055 tests / 68m         │ Test                         Why        │ skipped: 1,842 tests       │
│ Selected:                  213 tests / 21m         │ cache::remote_taint_rejected direct     │ high-risk skipped: 4       │
│ Skipped:                 1,842 tests / 47m         │ cache::key_normalizes_paths  direct     │ selector misses: 2/30d     │
│ Net saved after penalty:             42m           │ release::cache_bundle        dependency │ flake overlap: 7 tests     │
│ Confidence:                         93% █████░     │ api::smoke_cache             historical │ invalid edges: 0           │
│ Last miss: 2d ago, tests/api/auth_cache.rs         │ ...                                  │                            │
├ SELECTOR MISS TIMELINE ───────────────────────────┴─────────────────────────────────────────┴────────────────────────────┤
│ 05/24 miss: api auth cache path not mapped -> learned edge src/cache/key.rs -> tests/api/auth_cache.rs                    │
│ 05/20 miss: generated schema drift -> escalated to full integration on schema changes                                      │
╰─ Enter test detail  v validate plan  f force full  l learn from failure  a audit  / search  Esc repo ───────────────────╯
```

### 13.3 VTI metrics

| Metric | Formula / meaning |
|---|---|
| selection ratio | selected_tests / total_tests |
| time saved | baseline_full_duration - selected_duration |
| miss penalty | missed_failure_count × estimated_escape_cost |
| net saved | time_saved - miss_penalty |
| confidence | backend score from changed files, mappings, misses, flake score, dependency coverage |
| conservatism | selected tests / expected minimum tests |
| safety trend | confidence trend, miss trend, false-skip trend |
| learning velocity | selector misses resolved per week |

### 13.4 Guardrail rule

The UI must flag VTI as unsafe if any are true:

- recent selector miss is unresolved
- high-risk files changed and full suite was skipped
- confidence below configured threshold
- generated schema/migration/public API changed without integration expansion
- release/security-critical path skipped without proof
- VTI data is stale or only inferred

---

## 14. Agents and autonomous workflows

### 14.1 Purpose

Show what every agent is doing, whether it has authority, whether it is blocked, how much it is costing, and what proof it has produced.

### 14.2 Agents mock

```text
╭─ Agents Tower ─ active:8 blocked:2 idle:4 races:1 grants:12 ─ LLM budget today:$3.42/$20 ───────────────────────────────╮
│ AGENTS                                      │ TASK / CI STATE                                      │ INSPECTOR          │
│ ◆ agent-7  veox-core bug#812  running       │ branch agent/bug-812-cache-trust  MR !221            │ agent-7            │
│ ◆ agent-9  veox-web  bug#801  blocked grant │ CI test-linux ✗, retry proposed                       │ actor:agent        │
│ ◆ agent-3  jeryu     race#55  racing        │ hypotheses 3, winner none, pipelines 2/3 running      │ grant: patch repo  │
│ ◇ agent-4  redlinedb idle                   │ ready bugs: 2                                          │ expires: 38m       │
│ ◆ agent-5  veox-api  security fix           │ secret finding P0, needs human review                  │ last intent:       │
│                                             │                                                       │ propose_patch      │
│ RECENT AGENT EVENTS                         │ LIVE LOG / PATCH SUMMARY                              │ actions: logs diff │
│ 09:40 agent-7 grant issued                   │ + src/cache/trust.rs                                   │ pause revoke grant │
│ 09:39 agent-7 patch proposed                 │ + tests/cache_taint.rs                                 │                    │
│ 09:38 agent-9 denied merge grant             │ - old unsafe cache trust                               │                    │
╰─ Enter agent  l logs  d diff  g grants  p patch  k kill/pause  A assign bug  c config  Esc global ─────────────────────╯
```

### 14.3 Agent detail fields

- actor id, session id, agent version
- task source: bug, MR, release, incident, manual command
- branch, MR, project, base ref/SHA, head SHA
- capability intents and grants
- grant expiry, proof, nonce, idempotency
- last action preview/result
- CI pipeline/jobs linked to agent work
- logs and LLM calls, redacted
- token/cost budget
- patch diff summary
- evidence capsules and bug attempt history
- blockers: missing grant, failing CI, stale base, merge conflict, security gate, VTI risk

### 14.4 Autonomous workflow controls

Workflow states:

```text
disabled -> paused -> dry-run -> proposal-only -> supervised -> autonomous
```

Must-have workflow rows:

- bug fixer
- flake triager
- VTI learner
- cache healer / GC planner
- runner autoscaler
- Jankurai enforcer
- security fixer
- dependency updater
- release shepherd
- rollback guardian
- secret rotation checker
- Git sync/mirror healer

### 14.5 Config editor safety

```text
load redacted config -> validate schema -> show diff -> dry-run against recent events -> proof modal -> apply -> audit digest
```

Never allow autonomous mutations to be enabled from stale data.

---

## 15. Bugs and issues board

### 15.1 Purpose

Cross-repo accountability for bugs, attempts, owners, agent progress, evidence, MRs, commits, CI proof, and external refs.

### 15.2 Bug lanes

```text
ready -> in_progress -> fix_proposed -> reviewing -> verifying -> done
      -> blocked
      -> duplicate / invalid / cannot_reproduce / won't_do
```

### 15.3 Bugs mock

```text
╭─ Bugs / Issues ─ all repos ─ ready:18 in-progress:7 blocked:5 done:122 ─ sort:rank ───────────────────────────────────╮
│ STATUS LANES                                                                                                             │
│ READY                         IN PROGRESS                    FIX PROPOSED                    VERIFYING / DONE             │
│ S0 veox-api secret leak       bug#812 veox-core agent-7      bug#790 redlinedb MR!44        bug#755 jeryu ✓ v0.7.2        │
│ S1 veox-core cache taint      bug#801 veox-web agent-9       bug#788 veox-api MR!102        bug#749 veox-core ✓          │
│ S2 jeryu release doctor       bug#799 infra human            bug#781 veox-agent MR!18       ...                          │
├ BUG DETAIL ───────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ bug#812 cache taint incorrectly trusted  severity:S1 priority:P0 difficulty:3 owner:agent-7 status:in_progress           │
│ attempts: 3  failed:2  current branch:agent/bug-812-cache-trust  MR:!221  CI:test-linux failed                            │
│ acceptance: tainted remote cache must be rejected; regression test; jankurai score >=85; no security findings             │
│ evidence: capsule#c91, trace#94812, cache verdict target-key:a93f                                                         │
╰─ Enter bug  A assign agent  u update triage  l linked logs  m MR  c commits  e evidence  Esc global ───────────────────╯
```

---

## 16. Git Sync and remote state

### 16.1 Purpose

Show whether local repos, GitLab, GitHub, mirrors, shadow refs, protected branches, and release trains are in sync.

### 16.2 Git Sync mock

```text
╭─ Git Sync ─ all repos ─ drift:3 ─ mirror failures:1 ─ last fleet sync:09:18 ───────────────────────────────────────────╮
│ Repo          Local main  Remote main  Drift  Last merge main  Last PR/MR attempt  Admission  Mirror     Action          │
│ veox-core     a83f91c     a83f91c      0      09:12 MR!220     09:41 MR!221 fail   audit      ok         inspect         │
│ veox-api      7b2d111     4a9c882      +3     yesterday        09:30 MR!102 open   allow      lag 6m     sync preview    │
│ veox-web      c912eee     c912eee      0      08:44 MR!118     08:44 pass          allow      ok         none            │
│ redlinedb     5580aaa     5580aaa      0      05/25            09:22 PR#44 fail     deny       failed     repair          │
├ SELECTED ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ veox-api drift +3 commits: local ahead remote. Mirror lag caused by SSH tunnel flake. Required check pending.             │
│ suggested: run sync preview; wait for MR!102 checks; do not force push.                                                    │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 16.3 Git sync data

- local branch SHA
- remote branch SHA
- target branch SHA
- mirror branch SHA
- shadow remote state
- last successful merge into main
- last PR/MR attempt
- admission allow/audit/deny
- protected branch policy
- required check/passport
- mirror job status
- drift commits and risky files

---

## 17. CI Bottleneck Lab and simulator

### 17.1 Purpose

Turn historical/live data into specific improvement actions.

### 17.2 Bottleneck taxonomy

- queue duration by tag/pool/trust/repo/stage
- job duration regressions
- critical path frequency
- p50/p95/p99 duration and wait
- retry/waste/flake rate
- cache hit/miss influence
- runner class performance
- artifact upload/download time
- Docker pull/build time
- GitLab API latency/rate limits
- VTI misses/full-suite fallbacks
- resource saturation CPU/mem/disk/network
- approval/release/security gate latency

### 17.3 Simulator scenarios

```text
Scenario                    ETA delta   Cost delta   Risk          Notes
+2 linux-large runners      -11m        +$0.42/h     low           only if disk GC first
+2 small runners             0m         +$0.21/h     no benefit    tag mismatch
VTI threshold 93 -> 88      -18m        $0           medium        miss risk +0.7%
GC stale target now          future -4m $0           low           reclaim 42G safe
Split test-linux shard       -9m        eng work     medium        serial critical path
Pre-pull rust-builder        -3m/job    storage +2G  low           fixes cold-start
```

---

## 18. Jankurai Audit Center

### 18.1 Purpose

Show audit score, trend, version, caps/issues, duplicate code, release/security/TUI/web/repo-rot findings, tool adoption, and enforcement mode per repo.

### 18.2 Jankurai mock

```text
╭─ Jankurai Audit Center ─ repos:28 ─ avg score 87.4 ↑1.2 ─ caps:6 ─ critical findings:2 ───────────────────────────────╮
│ Repo          Version  Score Trend  Gate  Caps/Issues                Dup%  Security  Release  TUI  Rot   Last audit      │
│ veox-core     0.8.10   86.1  ↓2.4   warn  duplicate-code cap         7.2%  ok        warn     n/a  ok    09:33           │
│ veox-api      0.8.10   91.8  ↑0.9   pass  none                       2.1%  ok        ok       n/a  ok    09:30           │
│ veox-web      0.8.8    78.0  ↓5.1   fail  web-security, ux-qa        5.9%  high      ok       fail warn  08:58           │
│ redlinedb     missing  --    --     fail  jankurai not installed     --    unknown   unknown  n/a  warn  never           │
├ FINDINGS ────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ veox-core: duplicate code cap hit: src/cache/trust.rs and src/cache/provenance.rs share 184-line semantic clone.          │
│ evidence: jankurai://audit/veox-core/2026-05-26T09:33  suggested: create refactor bug; block release? advisory only.     │
╰─ Enter finding  B create bug  A assign agent  r run audit  g gate mode  / filter  Esc global ───────────────────────────╯
```

### 18.3 Finding categories

- duplicate code / semantic clones
- release bad behavior
- Docker anti-patterns
- type/contract drift
- web security
- dependency/security/provenance
- test integrity / proof routing gaps
- TUI black-box testing failures
- repo rot
- UX/accessibility evidence gaps
- migration safety

---

## 19. Code churn, velocity, and risk

### 19.1 Purpose

Show additions/removals over time by repo/family/author/agent/component and correlate with CI, bugs, Jankurai, and security.

### 19.2 Churn mock

```text
╭─ Code Volume ─ window:14d ─ repos:all ─ additions 184k ─ removals 91k ─ agent-authored 38% ───────────────────────────╮
│ Family       +Lines  -Lines  Commits  Agent%  Files  Hotspot                         CI fail Δ  Jank Δ  Sec Δ             │
│ veox-*       92k     41k     381      44%     1,204  src/cache, web/auth             +12%       -1.8    +2H              │
│ isolated     21k     11k      88      17%       301  db/migrations                   -3%        +0.4    0                │
│ redlinedb    55k     32k     144      51%       447  wal, pager, parser              +22%       n/a     0                │
├ TIMELINE ────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ +Lines ▁▂▃▆█▃▂▂   -Lines ▁▁▂▅▇▂▂▁   Failures ▁▁▂▃▆▅▃▂   Jank score ▅▆▆▅▃▃▄▅                       │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

---

## 20. Security Center

### 20.1 Purpose

Unified posture across GitLab scan artifacts, Jankurai security, secret scans, dependency vulnerabilities, Vault audit, Git admission, artifact provenance, runner trust, and cache taints.

### 20.2 Security mock

```text
╭─ Security Center ─ critical:2 high:9 medium:31 ─ secrets:1 ─ vulnerable deps:6 ─ provenance gaps:3 ───────────────────╮
│ FINDINGS                                  │ POSTURE BY REPO                                  │ INSPECTOR             │
│ ‼ veox-api secret in web bundle           │ Repo        C H M  Secrets  SAST  Deps  Prov Gate │ secret finding        │
│ ‼ veox-web credentialed wildcard CORS     │ veox-api    1 2 4     1      ok    warn  ok   fail │ file:dist/config.js   │
│ ▲ redlinedb dependency RUSTSEC-...        │ veox-web    1 4 9     0      high  ok    gap  fail │ source:jankurai+scan  │
│ ▲ jeryu Vault rotation due                │ jeryu       0 1 3     0      ok    ok    ok   warn │ action:create bug     │
│ ▲ runner remote-1 untrusted image digest  │ redlinedb   0 2 8     0      ok    high  n/a  warn │ block release:yes     │
╰─ Enter finding  B create bug  A assign agent  p policy proof  r rerun scan  v Vault  Esc global ───────────────────────╯
```

### 20.3 Security invariants

- Never render plaintext secrets.
- Show fingerprints, paths, TTLs, digests, status, and audit counts only.
- All release/merge proof modals include security posture.
- Critical security blocks release unless explicitly waived through production-risk proof.
- Secret finding rows link to redacted evidence only.

---

## 21. Signed artifacts and provenance

### 21.1 Purpose

Know which artifacts exist, whether they are signed, what source produced them, whether SBOM/provenance exists, and whether release/rollback is safe.

### 21.2 Artifacts mock

```text
╭─ Artifacts / Provenance ─ repo:veox-core ─ release:v2.8.1 ─ signed:7/8 ─ provenance:1 gap ───────────────────────────╮
│ Artifact                         Build Job     Digest      Signed  SBOM  Provenance  Release  Expires  Status             │
│ veox-core-linux-amd64.tar.zst     #94820        sha256:abc  ✓       ✓     ✓           v2.8.1   never    ready              │
│ veox-core-macos-arm64.tar.zst     #94821        sha256:def  ✓       ✓     ✓           v2.8.1   never    ready              │
│ veox-core-web-assets.zip          #94822        sha256:123  ✗       ✓     gap         v2.8.1   7d       block              │
├ PROVENANCE DETAIL ───────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ source commit:b91c2e  pipeline:#8912  runner:r09 trusted  cache verdict:trusted  jankurai:86 warn  security:1C           │
│ signer:fingerprint:9F...  certificate:ok  SBOM:path artifacts/sbom.json  release evidence:pending                       │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 21.3 Artifact fields

- artifact id/name/path/type
- project/repo/pipeline/job
- commit SHA/ref/tag
- digest and size
- expiration/keep status
- signature status/type/fingerprint/certificate
- SBOM path/digest
- provenance path/digest
- source cache verdicts
- runner trust tier
- Jankurai/security/release gate status
- release association
- rollback eligibility

---

## 22. Release, canary, production, and rollback

### 22.1 Purpose

One screen to answer: “Can I ship?”, “What is live?”, “Can I roll back?”, “What changed since last release?”, and “What proof do we have?”

### 22.2 State machine

```text
draft -> candidate -> preflight -> submitted -> canary -> canary_review -> prod_approved -> promoted -> monitoring -> done
                                           \-> failed -> rollback_ready -> rollback_started -> rollback_done
```

### 22.3 Release mock

```text
╭─ Release Control ─ latest:v2.8.1 ─ canary:25% ─ prod:v2.8.0 ─ rollback:v2.8.0 ready ─────────────────────────────────╮
│ RELEASE TRAIN                                                                                                             │
│ candidate v2.8.1  commit:b91c2e  pipeline:#8912  gates: 6/8  canary:running  prod:blocked                                │
│ previous  v2.8.0  commit:a83f91c  healthy       rollback artifact:signed ✓                                                │
├ GATES ────────────────────────────────────────────┬ CANARY / PROD ─────────────────────────┬ INSPECTOR ─────────────────┤
│ ✓ CI pipeline green except non-blocking docs       │ canary ring 25%  duration 18m          │ blocker: web asset unsigned │
│ ✗ security critical finding veox-api               │ metrics: err +0.2% latency +1.1%       │ artifact:web-assets.zip     │
│ ▲ Jankurai score veox-core 86 warn                 │ Nightwatch: pending                    │ suggested: sign artifact    │
│ ✓ VTI plan valid 93%                               │ prod promotion: blocked                │ rollback: ready             │
│ ✗ artifact web-assets unsigned                     │ rollback target:v2.8.0 signed          │ actions: sign rerun rollback│
│ ✓ Vault secret set finalized                       │ release evidence: pending              │                             │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 22.4 Production proof modal

```text
╭─ PROOF REQUIRED: Promote v2.8.1 to production ─ risk:production ───────────────────────────────────────────────────────╮
│ Source SHA: b91c2e    Target: production    Current prod: v2.8.0/a83f91c                                                   │
│ Gates: CI ✓  VTI ✓  Jankurai ▲ warn accepted? no  Security ✗ critical  Artifacts ✗ unsigned  Secrets ✓  Rollback ✓        │
│ Decision: DENY                                                                                                             │
│ Reasons: 1 critical security finding; 1 unsigned artifact.                                                                  │
│ Actions: [s] security finding  [a] artifact signer  [b] create blocker bug  [Esc] cancel                                    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

---

## 23. Secrets and Vault

### 23.1 Purpose

Safe visibility into secret lifecycle without leaks.

### 23.2 Secrets mock

```text
╭─ Secrets / Vault ─ vault:unsealed ok ─ authorities:2 ─ rotation due:1 ─ denied accesses:3 ─────────────────────────────╮
│ Authority      Mount      Prefix        Token FP  Health  Last rotate  Due       Denied  Notes                            │
│ prod-vault     kv-v2      jeryu/prod    9F:22..   ok      12d ago      2d        0       release secrets finalized       │
│ ci-vault       kv-v2      jeryu/ci      AA:19..   warn    41d ago      overdue   3       token TTL low                   │
├ RELEASE SECRET SETS ─────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ v2.8.1 deploy.env rendered ✓ runtime.env ✓ bundle ✓ report ✓ finalized ✓ expires 2026-06-26 paths redacted               │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

Never render values. Only paths, versions, fingerprints, TTLs, statuses, audit summaries, and digests.

---

## 24. Evidence ledger and time travel

### 24.1 Purpose

One searchable proof timeline across jobs, actions, grants, admission, release, cache, signatures, bug attempts, agents, and evidence capsules.

### 24.2 Evidence mock

```text
╭─ Evidence Ledger ─ cursor:184923 ─ filters:repo=veox-core ────────────────────────────────────────────────────────────╮
│ Seq     Time      Kind                    Entity             Actor     Severity  Summary                                  │
│ 184923  09:41:19  job.failed              job#94812          runner    error     test-linux failed E0597                 │
│ 184917  09:40:55  cache.verdict           target-key:a93f    cache     warn      tainted remote branch mismatch          │
│ 184901  09:39:04  agent.intent.requested  agent-7            agent     info      propose_patch bug#812                   │
│ 184890  09:38:12  grant.issued            grant#991          jeryu     info      patch repo veox-core expires 38m         │
│ 184880  09:36:01  admission.audit         ref agent/...      hook      warn      agent ref allowed with audit             │
├ DETAIL ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ event 184923 correlation:pipe#8912/job#94812 evidence:capsule#c91 related:bug#812 agent-7 cache:a93f                      │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 24.3 Time travel mode

`[` and `]` move cursor backward/forward. The entire TUI can render as-of cursor N:

```text
TIME TRAVEL: rendering as of event 184880, 09:38:12. Press ] to move forward, Esc to live.
```

This should be backed by event replay fixtures and used for incident diagnosis.

---

## 25. Source Doctor and API/MCP explorer

### 25.1 Purpose

Show data freshness, source drift, schema versions, current ports, auth posture, runtime configuration, MCP/API availability, docs/source drift, and incomplete plumbing.

### 25.2 Source Doctor mock

```text
╭─ Source Doctor ─ freshness worst:GitLab runners 12s ─ drift:3 ─────────────────────────────────────────────────────────╮
│ Source            Status  Freshness  Cursor/Version         Drift / Issue                                                  │
│ TuiReadModel      ok      0.8s       schema 4 event 184923  none                                                           │
│ GitLab API        ok      1.2s       v18.x?                 rate limit 12% used                                             │
│ Webhooks          ok      live       last UUID ...          MR hooks logged only, not stateful                              │
│ MCP tools         warn    n/a        16 tools               resources/watch unavailable                                     │
│ Action registry   ok      build sha  a11f...                none                                                           │
│ Docs/API          warn    n/a        unknown                cache auth, MCP count, DB backend stale                         │
│ Docker            warn    5.4s       event stream ok        remote-2 down                                                   │
│ Cache             ok      0.9s       summary                none                                                           │
│ Vault             ok      2.1s       unsealed               ci-vault rotation due                                           │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 25.3 API explorer contents

- CLI command tree and JSON availability
- MCP tool manifest and schemas
- future MCP resource manifest
- HTTP endpoints and auth requirements
- action registry IDs, key hints, risk tiers
- state table allowlist and schema hashes
- stream availability and last message
- docs/generated API drift

---

## 26. LLM and agent ROI ledger

### 26.1 Purpose

Show whether LLM/agent work is productive and within budget.

### 26.2 Metrics

- provider health by model/endpoint
- token use and estimated cost
- latency/error/rate limit
- key pool fairness
- role chain health
- agent spend by repo/bug/MR
- bugs closed
- CI failures introduced/fixed
- Jankurai score delta
- human interventions
- refusals/failures/redaction hits

---

## 27. Backend inspection plane

### 27.1 Golden rule

The TUI must not scrape ad hoc CLI text for normal operation. It should consume a unified typed inspection plane and use CLI fallback only in degraded/local developer mode.

### 27.2 Core contracts

```text
TuiReadModel       initial/current global snapshot
TuiEvent           append-only event/delta stream
EntityDetail       uniform object drilldown
WorkflowGraph      planned + live graph
ActionDescriptor   contextual action metadata
ActionPreview      dry-run/proof/blast-radius
ActionResult       execution outcome/audit receipt
SourceFreshness    per-source truth metadata
```

### 27.3 Minimum HTTP endpoints to add

```http
GET  /inspect/read-model
GET  /inspect/events?after=<cursor>&kinds=&entity_kind=&entity_id=&limit=
GET  /inspect/entity/{kind}/{id}
GET  /inspect/entity/{kind}/{id}/timeline
GET  /inspect/entity/{kind}/{id}/related
GET  /inspect/search?q=&scope=
POST /inspect/action/preview
POST /inspect/action/execute
GET  /inspect/action/{action_id}/events

GET  /inspect/repos
GET  /inspect/repo-families
GET  /inspect/repo/{repo_id}/dashboard
GET  /inspect/repo/{repo_id}/workflow-graph?pipeline_id=&ref=
GET  /inspect/pipeline/{project_id}/{pipeline_id}
GET  /inspect/pipeline/{project_id}/{pipeline_id}/jobs
GET  /inspect/job/{project_id}/{job_id}
GET  /inspect/job/{project_id}/{job_id}/trace?offset=&limit=
GET  /inspect/job/{project_id}/{job_id}/capsule

GET  /inspect/queue/global
GET  /inspect/queue/theoretical-limit
GET  /inspect/bottlenecks?repo=&family=&window=&ref=
GET  /inspect/runners
GET  /inspect/runner/{runner_id}
GET  /inspect/nodes
GET  /inspect/node/{node_id}/pressure

GET  /inspect/cache/summary
GET  /inspect/cache/objects?category=&repo=&hot=&tainted=
GET  /inspect/cache/provenance/{key}
GET  /inspect/cache/gc-plan

GET  /inspect/vti/repo/{repo_id}
GET  /inspect/vti/plan/{plan_id}
GET  /inspect/vti/misses?repo=&window=

GET  /inspect/agents
GET  /inspect/agent/{agent_id}
GET  /inspect/autonomy/workflows
GET  /inspect/autonomy/workflow/{id}

GET  /inspect/bugs?repo=&status=&owner=&agent=&sort=
GET  /inspect/bug/{bug_id}
GET  /inspect/git-sync
GET  /inspect/jankurai/summary
GET  /inspect/jankurai/repo/{repo_id}
GET  /inspect/security/summary
GET  /inspect/security/repo/{repo_id}
GET  /inspect/artifacts/repo/{repo_id}
GET  /inspect/artifact/{artifact_id}/provenance
GET  /inspect/releases
GET  /inspect/release/{release_id}
GET  /inspect/secrets/status
GET  /inspect/llm/providers
GET  /inspect/settings/effective-redacted
GET  /inspect/source-doctor
```

### 27.4 Streaming endpoints

```http
GET /events/stream                  # SSE for TuiEvent
GET /ws/events                      # WebSocket alternative
GET /jobs/{job_id}/trace/stream     # bounded stdout/stderr stream
GET /agents/{agent_id}/logs/stream  # agent log stream
GET /release/{id}/watch             # release/canary/prod stream
GET /cache/events/stream            # cache taints/promotions/GC/misses
GET /system/metrics/stream          # runner/node/Docker/host metrics
```

### 27.5 MCP resources to mirror

```text
jeryu://tui/read-model
jeryu://events?after=N
jeryu://system/snapshot
jeryu://repos
jeryu://repo/{repo_id}
jeryu://pipeline/{project_id}/{pipeline_id}
jeryu://job/{project_id}/{job_id}
jeryu://job/{project_id}/{job_id}/trace
jeryu://job/{project_id}/{job_id}/capsule
jeryu://queue/global
jeryu://runners
jeryu://nodes
jeryu://cache/summary
jeryu://cache/object/{key}
jeryu://vti/repo/{repo_id}
jeryu://agents
jeryu://agent/{agent_id}
jeryu://bugs/ready
jeryu://bug/{bug_id}
jeryu://git-sync
jeryu://jankurai/repo/{repo_id}
jeryu://security/repo/{repo_id}
jeryu://artifact/{artifact_id}/provenance
jeryu://release/latest
jeryu://release/{release_id}
jeryu://secrets/status
jeryu://llm/providers
jeryu://admission/recent
jeryu://capability/grants
jeryu://settings/effective-redacted
```

---

## 28. Core Rust data model

### 28.1 Entity references

```rust
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
    pub label: String,
    pub repo_id: Option<String>,
    pub project_id: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum EntityKind {
    System,
    Source,
    RepoFamily,
    Repo,
    Project,
    MergeRequest,
    Pipeline,
    Job,
    WorkflowNode,
    Runner,
    Pool,
    RemoteNode,
    Agent,
    AgentTask,
    AutonomousWorkflow,
    Bug,
    BugAttempt,
    TestPlan,
    TestCase,
    VtiDecision,
    CacheObject,
    CacheTaint,
    CacheVerdict,
    JankuraiAudit,
    JankuraiFinding,
    SecurityFinding,
    Artifact,
    Signature,
    ReleaseAttempt,
    ReleaseGate,
    SecretAuthority,
    SecretAccess,
    Grant,
    AdmissionDecision,
    EvidenceCapsule,
    LlmProvider,
}
```

### 28.2 Entity detail

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityDetail {
    pub entity: EntityRef,
    pub state: RuntimeStatus,
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
```

### 28.3 Event model

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TuiEvent {
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub kind: String,
    pub severity: Severity,
    pub entity: EntityRef,
    pub parent: Option<EntityRef>,
    pub repo_id: Option<String>,
    pub correlation_id: Option<String>,
    pub summary: String,
    pub fields: serde_json::Value,
    pub evidence_refs: Vec<EvidenceRef>,
    pub next_actions: Vec<ActionDescriptor>,
    pub source: DataSourceId,
}
```

Event kinds must cover system, repo, pipeline, job, runner, cache, VTI, agent, bug, git/admission, Jankurai, security, artifacts, release, secrets, LLM, and action lifecycle.

### 28.4 Capacity summary

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CapacitySummary {
    pub online_slots: u32,
    pub theoretical_slots: u32,
    pub effective_slots: f32,
    pub busy_slots: u32,
    pub runnable_queue_jobs: u32,
    pub blocked_queue_jobs: u32,
    pub queued_work_seconds: f64,
    pub drain_eta_p50_seconds: f64,
    pub drain_eta_p90_seconds: f64,
    pub critical_path_floor_seconds: f64,
    pub current_projection_seconds: f64,
    pub limit_distance: f64,
    pub bottlenecks: Vec<BottleneckSummary>,
    pub runner_recommendations: Vec<RunnerRecommendation>,
    pub source_freshness: Vec<SourceFreshness>,
}
```

### 28.5 Runtime status

```rust
pub enum RuntimeStatus {
    Planned,
    WaitingDeps,
    Runnable,
    Queued,
    Running { pct: Option<f32> },
    Passed,
    Failed,
    Canceled,
    SkippedByVti { reason: String, confidence: f32 },
    ReusedFromCache { verdict_id: String, digest: String },
    Blocked { blocker: BlockerKind },
    NeedsHuman { reason: String },
    Stale { age_ms: u64 },
    Unknown,
}
```

---

## 29. Rust implementation architecture

### 29.1 Recommended stack

Use the existing Rust codebase and typed APIs. The TUI layer should be a pure Rust app.

| Layer | Recommendation | Notes |
|---|---|---|
| Terminal UI | `ratatui` | Layout, widgets, custom DAGs, tables, sparklines, heatmaps. |
| Terminal backend/input | `crossterm` | Raw mode, key/mouse events, resize, cross-platform terminal IO. |
| Async runtime | `tokio` | Event fan-in, HTTP/SSE/WS clients, timers, background tasks. |
| HTTP/stream client | `reqwest`, `tokio-tungstenite` or SSE client | Inspection API, event streams, trace streams. |
| Docker metrics | existing Bollard usage / `bollard` | Container stats/events where backend exposes them. |
| System metrics | backend plumbed via sysinfo/cgroups/procfs | TUI consumes backend metrics; avoid privileged probing in UI. |
| Serialization | `serde`, `serde_json` | Shared schemas. |
| DB fallback | existing repo layer / `sqlx` if appropriate | Fallback only; prefer inspection API. |
| Telemetry | `tracing`, `tracing-subscriber` | TUI perf, dropped events, source lag. |
| Errors | `color-eyre`, `thiserror` | Human-readable diagnostics. |
| Testing | Ratatui test backend, `insta`, event replay fixtures | Golden screenshots and interaction tests. |

Do not pin exact versions in this spec; use workspace dependency policy.

### 29.2 Module layout

```text
crates/jeryu-tui/src/
  main.rs
  app.rs
  args.rs
  runtime.rs
  config.rs
  theme.rs
  keymap.rs
  router.rs
  focus.rs
  store.rs
  selection.rs
  data/
    client.rs
    models.rs
    inspection_http.rs
    mcp_resources.rs
    local_db_fallback.rs
    subscription.rs
    event_stream.rs
    trace_stream.rs
    demo.rs
    fixtures.rs
  actions/
    registry.rs
    preview.rs
    execute.rs
    modal.rs
  panes/
    workflow.rs
    mission.rs
    repos.rs
    repo_family.rs
    repo.rs
    queue.rs
    pipeline.rs
    job.rs
    logs.rs
    runners.rs
    cache.rs
    vti.rs
    agents.rs
    workflows.rs
    bugs.rs
    git_sync.rs
    bottlenecks.rs
    jankurai.rs
    churn.rs
    security.rs
    artifacts.rs
    release.rs
    secrets.rs
    evidence.rs
    llms.rs
    source_doctor.rs
    settings.rs
    command_palette.rs
    help.rs
  widgets/
    frame.rs
    status_header.rs
    tabs.rs
    breadcrumb.rs
    attention_queue.rs
    entity_table.rs
    virtual_table.rs
    progress.rs
    sparkline.rs
    heatmap.rs
    dag.rs
    minimap.rs
    timeline.rs
    log_viewer.rs
    diff_viewer.rs
    tree.rs
    gauge.rs
    badge.rs
    inspector.rs
    mini_chart.rs
    proof_modal.rs
    form.rs
    editor.rs
  graph/
    layout.rs
    route_edges.rs
    critical_path.rs
    simulator.rs
  test_support/
    snapshots.rs
    fake_backend.rs
    event_replay.rs
```

### 29.3 App state

```rust
pub struct App {
    pub route: Route,
    pub nav_stack: Vec<Route>,
    pub history: Vec<Route>,
    pub focus: FocusPath,
    pub selected: Selection,
    pub mode: AppMode,
    pub filters: FilterState,
    pub stores: Stores,
    pub data: Box<dyn InspectionClient>,
    pub keymap: KeyMap,
    pub theme: Theme,
    pub command_palette: CommandPaletteState,
    pub pending_action: Option<ActionFlow>,
    pub diagnostics: TuiDiagnostics,
}
```

### 29.4 Event loop

```text
terminal input ─┐
resize events ──┤
stream events ──┤       ┌───────────────┐       ┌──────────────┐
timers ─────────┼──────▶│ app reducer   │──────▶│ dirty render │
action results ─┤       └───────────────┘       └──────────────┘
trace chunks ───┘
```

Rules:

- Render loop never blocks on network, disk, DB, GitLab, Docker, or action execution.
- Network tasks send compact deltas through bounded channels.
- Reducer owns canonical UI state.
- Render functions are pure over `&AppState`.
- Frames are dirty-rendered and capped, e.g. 30 FPS max.
- High-frequency events are coalesced by entity/kind.
- Logs are ring-buffered by job with byte offsets and search index.
- Visible trace stream is prioritized; hidden streams are summarized or sampled.

### 29.5 Inspection client trait

```rust
#[async_trait]
pub trait InspectionClient: Send + Sync {
    async fn read_model(&self) -> Result<TuiReadModel>;
    async fn events_after(&self, cursor: u64, filter: EventFilter) -> Result<Vec<TuiEvent>>;
    async fn entity_detail(&self, entity: &EntityRef) -> Result<EntityDetail>;
    async fn workflow_graph(&self, scope: ScopeRef) -> Result<WorkflowGraph>;
    async fn pipeline_graph(&self, project_id: i64, pipeline_id: i64) -> Result<WorkflowGraph>;
    async fn job_trace(&self, project_id: i64, job_id: i64, offset: u64) -> Result<TraceChunk>;
    async fn action_preview(&self, action: ActionRequest) -> Result<ActionPreview>;
    async fn action_execute(&self, action: ActionRequest, proof: ProofAck) -> Result<ActionResult>;
    fn subscribe_events(&self, filter: EventFilter) -> EventStream;
    fn subscribe_trace(&self, project_id: i64, job_id: i64) -> TraceStream;
}
```

Implementations:

- `HttpInspectionClient`
- `McpInspectionClient`
- `LocalDbInspectionClient`
- `FixtureInspectionClient`

---

## 30. Search, filtering, and saved lenses

### 30.1 Command palette examples

```text
> repo veox-core
> family veox-*
> job 99122 trace
> explain why queue saturated
> cache gc dry-run
> pool rust-fast scale 8
> bug ready veox-core
> agent race bug JRY-412
> release rollback prod 1.7.4
> jankurai veox-core duplicates
> security unsigned artifacts
```

### 30.2 Filter language

Examples:

```text
repo:veox-* status:failed
family:veox-* tag:rust-fast queued>5m
kind:job stage:test status:running
cache:tainted repo:veox-core
vti:miss window:7d
agent:active grant:expired
security:critical OR secret:true
release:v2.8.1 gate:blocked
jankurai:score<85 cap:duplicate-code
```

### 30.3 Saved lenses

Users can save operational views:

- Hot Queue
- Prod Risk
- Cache Full
- Agent Blocked
- Release Train
- VTI Misses
- Jankurai Drops
- Security Critical
- Remote Nodes Down

Saved lenses store filters, scope, sort, visible columns, pinned panes, and alert thresholds.

---

## 31. Performance targets

| Target | Requirement |
|---|---|
| initial interactive paint | `<500ms` with cached snapshot; `<2s` cold network |
| render frame | p95 `<16ms`; p99 `<33ms` |
| input latency | p95 `<50ms` |
| event apply latency | p95 `<100ms` stream receipt to visible state |
| trace display latency | p95 `<250ms` backend chunk to screen |
| scale | 500 repos, 50k recent jobs, 10k event memory window, 100 trace subscriptions with one visible prioritized |
| memory | default `<250MB`; bounded trace/event stores |
| search/filter | 10k visible entities filtered in `<20ms` |
| terminal resize | stable layout recompute `<50ms` |

Use virtualization for all large tables. Precompute graph layouts where possible. Coalesce event storms. Drop non-visible trace chunks before input responsiveness suffers.

---

## 32. Safety and security requirements

- Production actions bind to exact source SHA/digest/version.
- Stale proofs cannot execute.
- Action registry/capability path is the only mutation path.
- Merge/release/rollback/secret actions require proof modal.
- Secrets redacted at backend and UI layers.
- No plaintext secret in panic/error/debug output.
- TUI’s own mutating requests are logged to evidence ledger.
- Source freshness is visible before risky actions.
- High-risk actions show blast radius and rollback/undo availability.
- Cache GC respects leases and trust policy.
- Runner scale/drain shows running-job impact.
- Agent workflow config edits require dry-run and audit digest.

---

## 33. Backend plumbing roadmap

### P0 — make the TUI truthful and connected

- expose unified read model
- expose events after cursor
- expose source freshness
- expose action preview/execute uniformly
- add fixture/demo backend
- add trace polling fallback
- build entity detail endpoint

### P1 — make it live

- SSE/WebSocket event stream
- live trace stream
- agent log stream
- cache event stream
- system metrics stream
- event backpressure and coalescing

### P2 — make capacity/runners accurate

- Docker stats and events
- node CPU/mem/disk/network/inode/iowait
- remote node heartbeat and SSH latency
- per-tag queue and effective slots
- startup lag/pull latency
- runner manager config hash/version
- simulator endpoint

### P3 — make workflows graph-native

- pipeline DAG `needs`
- child pipelines/bridges
- artifact dependencies
- test reports/JUnit
- coverage/code quality/security artifacts
- failure annotations
- critical path and ETA

### P4 — make autonomy first-class

- agent lifecycle table
- race lifecycle/status/cleanup/winner
- autonomous workflows and config editor backend
- LLM provider/resource telemetry
- kill bell/freeze/verdicts integrated into main read model

### P5 — make trust/release complete

- Jankurai structured output
- security normalization
- artifact signing/provenance/SBOM
- release evidence and rollback readiness
- Vault lease/audit metadata
- admission/policy proof views

### P6 — make APIs self-describing

- generated API docs from Clap/action/MCP/AgentIntent/DB schemas
- MCP resources/list/read/subscribe
- OpenAPI/JSON Schema export
- state table allowlist inspector
- docs/source drift CI check

---

## 34. Testing strategy

### 34.1 Unit tests

- capacity formulas
- pressure factor calculations
- critical path calculation
- attention ranking
- VTI metrics
- cache GC plan safety
- action risk tier handling
- filter language parser
- source freshness conflict resolution

### 34.2 Golden TUI tests

Use deterministic rendering for:

- global healthy
- global overloaded
- queue saturated by tag
- CPU/memory critical runner node
- failing repo
- stale source
- cache nearly full
- VTI unsafe miss
- agent blocked by grant
- release denied by security
- narrow terminal layout
- no-color/ascii mode

### 34.3 Event replay tests

Replay fixtures:

- normal pipeline success
- failed job -> capsule -> bug -> agent patch -> retry -> pass
- cache taint storm
- VTI selector miss and learning
- runner outage and autoscale
- release canary fail and rollback
- security secret finding blocking release
- Git sync drift repaired

### 34.4 Property tests

- `Enter` then `Esc` returns to previous scope.
- Critical blocker cannot be hidden by filters without explicit warning.
- Events remain monotonic or conflict-marked.
- Stale proof cannot execute.
- High/production actions cannot execute without proof.
- Entity cache handles out-of-order updates.
- Search/filter never panics on weird unicode/regex input.

### 34.5 Load tests

- 500 repos
- 100 repo families
- 50k jobs
- 1M historical backend events, 10k UI memory window
- 100 runner pools
- 1,000 runner managers/nodes
- 10k cache objects
- 10k bugs/findings
- 100 trace streams, one visible

### 34.6 Security tests

- secret redaction in logs/traces/entity details/evidence exports
- no secret in panic/debug logs
- proof required for production actions
- origin/bind/auth posture displayed accurately
- action audit IDs emitted and searchable

---

## 35. Implementation phases

### Phase 0 — contracts and fixtures

- Define shared schemas.
- Build fixture backend.
- Render all major screens from fixture data.
- Snapshot tests for wide/narrow/ascii modes.

### Phase 1 — shell and navigation

- Header, tabs, scope stack, focus model, keymap, command palette.
- Entity inspector and action menu skeleton.
- Universal `Enter`/`Esc` behavior.

### Phase 2 — global/mission/queue MVP

- Mission screen.
- Workflow Atlas first pass.
- Queue/theoretical-limit screen with fixture + existing snapshot data.
- Attention queue.

### Phase 3 — repo/pipeline/log drilldown

- Repo family and repo cockpits.
- Pipeline DAG renderer.
- Job trace viewer with polling fallback.
- Failure capsule integration.

### Phase 4 — runners/cache/VTI

- Runner fleet and node pressure.
- Cache Observatory and GC preview.
- VTI cockpit and plan proof.
- Capacity simulator.

### Phase 5 — agents/bugs/git/autonomy

- Agents Tower.
- Bug board/detail/attempts.
- Git Sync screen.
- Autonomous workflows list and safe config editor.

### Phase 6 — trust/release/security/artifacts

- Jankurai Audit Center.
- Security Center.
- Artifacts/provenance.
- Release control and rollback proof.
- Secrets/Vault.
- Evidence ledger/time travel.

### Phase 7 — streaming and production polish

- Event/trace/cache/agent/system streams.
- Backpressure and coalescing.
- Source Doctor.
- MCP resource mirror.
- Headless capture/screenshot.
- Performance/load hardening.

---

## 36. Acceptance criteria

### 36.1 UX

- From global screen, drill to a failing job’s first error in ≤4 keystrokes.
- From global screen, answer “why are jobs queued?” in ≤5 seconds.
- From queue screen, answer “should we increase runners?” with evidence in ≤10 seconds.
- From repo screen, answer “safe to merge?” with exact blockers in ≤5 seconds.
- From cache screen, identify top storage category and safe GC bytes in ≤5 seconds.
- From VTI screen, determine whether VTI is saving time safely in ≤10 seconds.
- From agent screen, identify task, branch/MR, grant, logs, and blocker in ≤10 seconds.
- `Enter` drills and `Esc` goes back everywhere.
- Every visible number has freshness or inherited pane freshness.

### 36.2 Data

- Every entity detail includes source freshness and last updated.
- Every mutating action has preview/proof/result events.
- The global queue uses webhook/local state and GitLab reconciliation.
- Theoretical limit accounts for tags/trust tiers/blocked jobs, not just runner count.
- Pipeline graph includes child pipelines and artifact/needs edges when available.
- Security/release proof modals include artifact signature/provenance status.
- Jankurai screen shows installed version per repo and stale/missing versions.
- Runner screen distinguishes CPU/memory/disk/network saturation from missing runner slots.

### 36.3 Performance

- p95 render `<16ms` on 180x50 dashboard with 500 repos aggregated.
- p95 input-to-render `<50ms`.
- trace viewer handles 10MB logs without blocking input.
- event filters over 10k in-memory events under 20ms.
- backend stream drop leaves UI useful with stale indicators and reconnect state.

### 36.4 Safety

- Secrets never render in plaintext.
- Production actions cannot execute from stale proof.
- Merge/release actions bind to exact source SHA.
- Capability grants and action registry are shown before high-risk actions.
- TUI logs mutating action requests to evidence/audit ledger.

---

## 37. Final runner-count policy

Until the live telemetry exists, no static document can honestly say “increase runner count now.” The runtime policy should be:

1. **Increase runner managers** only when runnable queue pressure is sustained, busy slots are high, blocked queue is low, node resources are green/yellow but not critical, tags/trust match, and theoretical headroom exists.
2. **Add remote nodes or hardware** when queue pressure is sustained but local CPU/memory/disk/network is already critical.
3. **Run cache/host GC first** when disk/cache pressure limits effective slots or cache misses dominate waste.
4. **Rebalance tags/trust tiers** when jobs wait while eligible-looking runners are idle.
5. **Do not add runners** when the bottleneck is serial DAG, manual approval, release/security gate, missing artifact, GitLab API, DB latency, stale source, or VTI misconfiguration.
6. **Split/shard jobs** when critical-path serial runtime dominates limit distance.
7. **Tune VTI** when full-suite fallbacks or selector misses dominate CI work.
8. **Pre-pull/warm images** when cold-start/image pull latency dominates.

The UI should always show a recommendation like:

```text
Recommendation: do not add generic runners.
Reason: 9 rust-fast jobs are queued, but 4 generic slots are idle due tag mismatch; remote-nyc disk is 91%; effective gain from scaling now is only +2 slots.
Best next: GC remote-nyc buildkit 41GiB, then scale rust-fast +7. Expected p95 queue wait 12m -> 4m. Confidence 0.78.
```

or:

```text
Recommendation: add +6 linux-large managers now.
Evidence: runnable queue 11h work, busy 97% sustained 18m, blocked queue 8%, CPU p95 62%, mem available 38%, disk 71%, tags match, theoretical headroom +14.
Expected: drain ETA 31m -> 13m, limit-distance 1.82x -> 1.24x. Confidence 0.84.
```

---

## 38. Final design stance

The final JeRyu TUI is not “one giant dashboard.” It is a **fast, colorful, evidence-driven, keyboard-native, realtime object graph** for agent-era CI/CD.

Its greatness comes from five commitments:

1. **Everything visible is drillable.**
2. **Every number has provenance and freshness.**
3. **Every action is previewed, proof-gated, and audited.**
4. **Every bottleneck is explained in operational terms.**
5. **Every repo family, repo, pipeline, job, agent, cache object, release gate, and artifact participates in the same navigation grammar.**

Build the typed inspection plane first. Then build the shell, Workflow Atlas, Mission, Queue/Runner decision engine, repo/pipeline/trace drilldowns, Cache Observatory, VTI proof, Agents Tower, Bugs board, Jankurai, Security, Artifacts, Release, Secrets, Evidence Ledger, and Source Doctor. Add streaming and replay until the terminal feels alive without sacrificing truth.
