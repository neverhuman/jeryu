# JeRyu Flight Deck — Final Rust TUI Engineering Specification

**Artifact:** `jeryu_flight_deck_final_rust_tui_engineering_spec.md`  
**Product names used in this spec:** `JeRyu Flight Deck`, `Mission Control`, `Workflow Atlas`, `Cache MRI`, `Agent Tower`, `Evidence Flight Recorder`  
**Audience:** Rust engineers, backend/control-plane engineers, TUI engineers, automation/agent engineers, and reviewers implementing the next-generation `jeryu tui`.

---

## 0. Executive summary

JeRyu already has the raw ingredients for a developer’s dream CI/CD control room: a Rust control plane, GitLab webhooks and REST data, a typed TUI read model, durable state DB, MCP tools, a Unix-socket capability API, SmartCache, VTI smart-test selection, runner pools, remote nodes, Docker events, Vault secrets, release/canary/rollback state, bug tracking, Git admission controls, LLM/autonomy data, Jankurai audits, and proof/evidence ledgers.

The final TUI should not be a prettier CLI. It should be a **terminal-native realtime operating system for software delivery**. The operator sees the entire fleet, understands queue pressure and theoretical CI limits, drills from repo family to repo to workflow DAG to job trace in seconds, edits autonomy configs safely, watches agents work, sees cache/storage/trust posture, and acts through proof-gated commands.

The entire design reduces to three primitives:

1. **Entities** — repo families, repos, branches, MRs, pipelines, stages, jobs, traces, runners, pools, nodes, cache objects, VTI plans, test cases, agents, grants, bugs, releases, artifacts, secrets, security findings, Jankurai findings, evidence items, and actions.
2. **Events** — monotonic, cursor-addressable, replayable facts from webhooks, DB writes, GitLab REST polling, Docker, cache proxy, Vault, MCP/capability, Git hooks, agents, releases, Jankurai, LLM providers, and autonomy.
3. **Actions** — previewable, dry-runnable where possible, risk-classified, capability-gated operations that produce receipts and proof.

The visual experience should feel alive: streaming job lanes, queue-pressure meters, pulsing event cursors, sparklines, heatmaps, blinking-but-not-annoying status glyphs, live traces, and animated DAG edges. But the UX law is strict: **motion may communicate freshness; it must never fabricate certainty**. Every green status has a proof path, every warning explains itself, and stale/inferred data is visibly marked.

The primary navigation path is:

```text
Fleet Mission Control
  -> Repo Family
    -> Repo Cockpit
      -> Workflow / Pipeline DAG
        -> Job / Trace / Artifact / Evidence
          -> Action Preview / Proof / Receipt
```

`Enter` drills down. `Esc` goes up. Arrow keys move. `Tab` changes panes. The command palette reaches everything. The attention queue tells the operator what matters next.

---

## 1. Source-derived baseline

The uploaded archive contained multiple API/MCP/realtime inventories and several attempted TUI design specifications. This final spec consolidates the recurring facts and strongest design ideas. Where the archive reported doc/source drift, this spec treats the source-derived inventory as authoritative and requires the TUI to expose drift as operational risk.

### 1.1 Current control surfaces available

| Surface | Current entrypoint / transport | TUI use |
|---|---|---|
| CLI | `jeryu <command>` | Full operator surface: install, serve, remote, node, TUI, Git wrapper, repo/fleet, status, pools, jobs, pipelines, cache, logs, agents, settings, tests, release, secrets, progress, bugs, policy, host, MCP, next action, blockers, actions. |
| Existing TUI | `jeryu tui` | Current Ratatui/crossterm UI with mission/workflow/jobs/release/pools/cache/evidence/tests/agents/secrets/LLMs/Git surfaces and background workers. |
| Typed TUI API | `src/api/*` | Entity model, event model, read model, snapshots, action previews/results, component health, VTI status, graph edge kinds, cache verdicts, test-plan views. This should become the hard contract. |
| MCP stdio | `jeryu mcp serve` / `serve-stdio` | JSON-RPC tool calls over stdin/stdout. |
| MCP HTTP | `jeryu mcp serve-http`, default `127.0.0.1:9778`, `/mcp` | POST JSON-RPC, DELETE sessions. GET/SSE currently disabled. Good future transport for resources/watch. |
| Capability API | Unix socket | Agent intent envelope with protocol version, actor, nonce, expiry, grants, project/ref/SHA, budget, idempotency, and intent. |
| Main HTTP/webhook API | Axum, default `127.0.0.1:9777` | `/health`, `/hooks`, `/cache/summary`; GitLab Job/Pipeline/Push; MR hooks accepted/logged but not yet acted on. |
| GitLab REST wrapper | internal `GitlabClient` | Projects, jobs, traces, artifacts, pipelines, variables, bridges/downstream pipelines, runners, runner managers, issues, merge requests, branches, webhooks. |
| Message log / broker | Kafka or Jansu feature-gated | Topics: `jeryu.webhook.jobs`, `jeryu.webhook.pipelines`, `jeryu.webhook.pushes`. |
| Custom executor | `jeryu exec config/prepare/run/cleanup` | Runner phase, sandbox, job env, cache decisions, logs, cleanup outcomes, failure capsules. |
| Git admission hook | `jeryu server-hook pre-receive` | Ref updates, actor kind, grant match, policy verdict, allow/audit/deny reasons. |
| SmartCache/gateway | cache proxy `19800`, OCI registry mirror `19801` | Cargo sparse config, crate downloads, CAS hits, singleflight, requests, taints, leases, verdicts, hot entries, storage. |
| Docker/runner plane | Bollard + compose/remotes | Runner manager containers, managed labels, Docker events, logs, lifecycle, OOM/death events, reconciliation. |
| Vault/secrets | Vault HTTP + DB | Secret authorities, release secret sets, expiry, rotation/finalization, audit metadata, redacted token fingerprints. |
| State DB | SQLite default, RedlineDB opt-in | Durable source for pools, managers, jobs, pipelines, events, capability/admission, releases, evidence, cache, VTI, tests, secrets, autonomy, bugs, LLM budget. |
| Autonomy binary | `autonomy` CLI/server | Evidence Gate/VibeGate, kill bell, freeze windows, verdicts, launch ledger, foundry candidates, LLM telemetry, PR drift, `/metrics`, `/health`, `/events`. |
| GitHost abstraction | GitHub/GitLab-style adapters | PR/MR state, diffs, comments, approvals, checks, target policy SHA, merge-passport status. |
| Jankurai tooling | repo audit / GitHub Action / score artifacts | Quality/security/provenance/ownership/duplication/generated-zone/score history findings. |

### 1.2 Current MCP tools to support and display

The current source-derived inventory reports these 16 `jeryu.*` MCP tools:

| Tool | Type | TUI treatment |
|---|---:|---|
| `jeryu.fetch_capsule` | Read | Job failure capsule detail, Evidence tab, Trace inspector. |
| `jeryu.get_system_snapshot` | Read | Bootstrap seed for global state. |
| `jeryu.get_pipeline_jobs` | Read | Pipeline DAG/job table fallback until full graph endpoint exists. |
| `jeryu.get_ci_bottlenecks` | Read | Bottleneck Lab, Queue screen history. |
| `jeryu.explain_blockers` | Read | Attention queue, “why not green?”, entity blockers. |
| `jeryu.plan_validation` | Read | VTI plan validation, selector miss risk, test plan proof. |
| `jeryu.run_tests` | Mutating | Targeted test action; preview branch/pipeline/yaml before execution. |
| `jeryu.propose_patch` | Mutating | Agent patch or human quick-fix workflow; show branch/MR/grant receipt. |
| `jeryu.race_patches` | Mutating | Hypothesis race arena; requires race lifecycle completion work. |
| `jeryu.request_merge` | High-risk mutating | Never call directly from TUI without merge-proof gate; mark as guarded until implementation is audited. |
| `jeryu.bug_submit` | Mutating local | Create bug from failure/finding/log selection. |
| `jeryu.bug_list` | Read | Bugs board. |
| `jeryu.bug_show` | Read | Bug detail timeline. |
| `jeryu.bug_ready` | Read | Agent-ready bug queue. |
| `jeryu.bug_update` | Mutating local | Triage/edit bug metadata. |
| `jeryu.bug_record_attempt` | Mutating local | Append agent/human attempt history. |

MCP should remain action-oriented. The dream TUI additionally needs MCP **resources** and **subscriptions** for inspection, so agents and sidecar tools can browse state without invoking tools.

### 1.3 Known drift and risks that the TUI must expose

| Drift / risk | Required UI behavior |
|---|---|
| Older docs undercount MCP tools. | Source Doctor shows generated tool registry vs docs vs runtime. |
| Older docs describe RedlineDB-only, while source uses SQLite default with RedlineDB opt-in. | Header/system panel shows DB backend, file/path/URL, feature profile, migration state. |
| `/cache/summary` auth differs across docs/source in the archive notes. | API Doctor shows auth requirement and recent auth failures. |
| `ListAllowedActions` appears stale relative to action registry. | TUI never trusts hardcoded action lists; it consumes generated registry and warns on mismatch. |
| Some side-effect classes may mark mutating actions read-only. | Build-time and runtime validation fail closed; mutating actions must be non-read-only. |
| `request_merge` may directly call accept MR in some paths. | TUI treats merge as production-grade risk until gate is proven; requires fresh MR state, CI, VTI, security, Jankurai, artifact, approval proof. |
| MR hooks are accepted/logged but not first-class state yet. | MR panes show `PARTIAL MR REALTIME` badge until durable MR ingestion exists. |
| MCP HTTP GET/SSE is disabled today. | Use local HTTP/read-model/WebSocket if present; fall back to polling/MCP tools; mark stream source unavailable. |
| Existing Flow Board may render only first active pipeline and incomplete graph edges. | Workflow Atlas must be multi-pipeline and show edge confidence (`explicit`, `stage-inferred`, `name-inferred`, `unknown`). |
| Evidence is not yet a fully searchable proof timeline. | Build Flight Recorder as first-class endpoint/query, even if initially backed by multiple DB tables. |
| Agents lack dedicated lifecycle tables. | Add tables/models; until then label agent data as reconstructed. |

---

## 2. Product doctrine

### 2.1 The one-sentence promise

**JeRyu Flight Deck lets one developer operate hundreds of repos and autonomous agents from a Rust terminal UI, seeing live CI motion, exact bottlenecks, cache/test/runner truth, proof-backed releases, and safe actions with zero context switching.**

### 2.2 Non-negotiable UX laws

1. **Everything visible is addressable.** A row, badge, graph node, cache category, queue number, warning, proof item, runner, trace line, artifact digest, and agent grant all have an entity ID or an evidence ID.
2. **Every warning explains itself.** Red/yellow without cause, confidence, freshness, and next action is forbidden.
3. **Every green state has proof.** Green with no proof is rendered as `OK?`, `HEUR`, `NO PROOF`, or `STALE`, not green.
4. **`Esc` always goes up.** No modal traps. The navigation stack is sacred.
5. **`Enter` drills down.** On an entity it opens detail; on a selected action it opens preview; on a focused graph node it enters node detail.
6. **Arrow keys are spatial.** Up/down select rows; left/right move sibling panes, graph columns, or route up/down when unambiguous.
7. **Tabs switch focus worlds.** `Tab`/`Shift+Tab` move through panes, subtabs, or major tabs depending mode.
8. **The attention queue is always one gesture away.** `b` jumps to top blocker; `n` jumps to next action.
9. **No blank screens.** Show live, stale, loading, degraded, empty, or synthetic fallback — never silent emptiness.
10. **Motion must be truthful.** Animated edges/events indicate fresh updates; progress bars distinguish actual, estimated, unknown, stale, skipped, failed, canceled, manual.
11. **Actions are proof-gated.** Mutating operations require preview, risk tier, expected side effects, freshness requirements, grants, and receipts.
12. **Expert speed without hidden magic.** One-letter shortcuts are discoverable through `?` and command palette previews.

### 2.3 Trust language

Every datum has a trust label:

| Label | Meaning | Rendering |
|---|---|---|
| `LIVE` | Updated by stream or fresh poll within TTL. | Bright, pulsing cursor dot. |
| `FRESH` | Recent snapshot within TTL, not streaming. | Bright, static. |
| `STALE 12s` | Last known value is older than TTL. | Dimmed value, stale badge. |
| `LAST KNOWN` | Source unavailable; value is retained. | Dimmed + warning glyph. |
| `INFERRED` | Derived from partial data. | Italic/dim or dotted border. |
| `HEURISTIC` | Based on model/history, not exact. | `~` prefix and confidence. |
| `UNKNOWN` | System cannot know. | Explicit placeholder, no fake values. |
| `UNVERIFIED` | Claim lacks proof path. | Yellow `NO PROOF`. |

### 2.4 Design posture

The TUI should feel like a production control room, not an admin panel. It should blend:

- **A flight deck:** global posture, speed, altitude, danger, terrain, ETA.
- **A profiler:** critical path, utilization, cache misses, lost time, p50/p95 deltas.
- **A debugger:** jump from symptom to trace to failure capsule to code/test/artifact.
- **A policy court:** every merge/release/cache reuse/action has evidence.
- **An agent tower:** all autonomous work is visible, bounded, pausable, and auditable.

---

## 3. System mental model

### 3.1 Scope hierarchy

```text
Universe / Fleet
  ├─ Repo families                    veox-*, redline-*, isolated, infra, archived
  │   ├─ Repositories                 slug, project_id, provider, default branch
  │   │   ├─ Branches / refs          main, release/*, agent/*, race/*
  │   │   ├─ Merge requests / PRs     source/target SHA, approvals, discussions
  │   │   ├─ Workflows / pipelines    DAG, stages, jobs, child pipelines
  │   │   │   ├─ Jobs                 trace, artifacts, cache, VTI, capsule
  │   │   │   └─ Gates                manual, policy, release, security
  │   │   ├─ VTI / tests              plans, selected/skipped, misses, receipts
  │   │   ├─ Agents                   sessions, tasks, grants, branches, logs
  │   │   ├─ Bugs/issues              attempts, owners, evidence, status
  │   │   ├─ Quality/Jankurai         score, findings, controls, repair queue
  │   │   ├─ Release/version          canary, prod, rollback, passports
  │   │   ├─ Security/secrets         findings, secret sets, Vault audit
  │   │   ├─ Artifacts/provenance     digest, signature, SBOM, SLSA/provenance
  │   │   └─ Git sync/remotes         mirrors, hooks, admission decisions
  │   └─ Shared family policies       release gates, runner tags, ownership, standards
  └─ Infrastructure
      ├─ Runner pools / managers / nodes
      ├─ SmartCache / cache categories / registry mirror / sccache
      ├─ Docker / host storage / network / daemon health
      ├─ Vault / secret authorities / rotation
      ├─ GitLab / GitHub / GitHost adapters
      ├─ MCP / capability / action registry
      ├─ Autonomy / kill bell / freeze windows / verdicts
      ├─ LLM providers / key pools / budget
      └─ Evidence / event / audit ledger
```

### 3.2 Scope stack

The UI maintains a stack of scope routes. Every route is serializable so screenshots, saved lenses, replay, and bug reports can return to the same view.

```rust
pub enum Route {
    Fleet,
    RepoFamily { family: String },
    Repo { repo: RepoRef },
    MergeRequest { repo: RepoRef, mr: MrRef },
    Pipeline { repo: RepoRef, pipeline_id: i64 },
    Job { repo: RepoRef, project_id: i64, job_id: i64 },
    Trace { repo: RepoRef, project_id: i64, job_id: i64, offset: Option<u64> },
    Entity { entity: EntityRef, subtab: EntitySubtab },
    Domain { domain: DomainRoute },
    Search { query: String, scope: SearchScope },
    ActionPreview { request_id: String },
}
```

Breadcrumb example:

```text
Fleet › veox-* › veox-enclave › MR !843 › Pipeline #581 › integ:test-nextest-4 › Trace
```

### 3.3 Universal detail contract

Every entity detail view must have these subtabs, even if some show “not available”:

| Subtab | Purpose |
|---|---|
| Overview | Summary, state, freshness, IDs, owner, current blocker, next action. |
| Timeline | Events affecting this entity. |
| Evidence | Capsules, receipts, proofs, artifacts, digests, signatures, gates. |
| Relations | Parents, children, dependencies, downstream jobs, linked bugs/MRs/releases. |
| Logs/Output | Trace, agent logs, service logs, job stdout, audit excerpts. |
| Metrics | Durations, queue time, resource usage, cache hits, flake rate, score history. |
| Actions | Contextual actions with risk and preview. |
| Raw | Redacted JSON/source payload for debugging. |

---

## 4. Top-level information architecture

### 4.1 Primary tabs / lenses

Use numeric keys for the first ten, `g`-prefixed shortcuts for all. The left sidebar can show these as icons in narrow mode.

| Key | Lens | Primary question |
|---:|---|---|
| `1` / `g0` | **Fleet** | What is happening across everything right now? |
| `2` / `gq` | **Queue** | How close are we to theoretical CI limit, and why not closer? |
| `3` / `gr` | **Repos** | Which repo family/repo needs attention? |
| `4` / `gw` | **Workflow** | What is running, blocked, failing, skipped, or waiting? |
| `5` / `gu` | **Runners** | Are pools/nodes/slots healthy and enough? |
| `6` / `gc` | **Cache** | Are we full, slow, tainted, or wasting rebuilds? |
| `7` / `gv` | **VTI** | Is the smart test skipper saving time safely? |
| `8` / `ga` | **Agents** | What are agents doing and are they bounded? |
| `9` / `gb` | **Bugs** | What issues exist, who/what is working on them, what is blocked? |
| `0` / `gR` | **Release** | Can we ship, rollback, or promote safely? |
| `gj` | **Jankurai** | What quality/security/provenance debt threatens the fleet? |
| `gg` | **Git Sync** | Are repos/remotes/mirrors/hooks/admission states in sync? |
| `gs` | **Security** | What security/secrets/policy findings block trust? |
| `gi` | **Artifacts** | What was built, signed, attested, and deployed? |
| `ge` | **Evidence** | What is the durable proof timeline? |
| `gl` | **LLM/Autonomy** | How are providers, budgets, verdicts, kill bell, and freeze windows? |
| `gS` | **Settings/Source Doctor** | Which data sources/configs/docs/actions are stale or misconfigured? |

### 4.2 Persistent shell regions

Wide terminal target: 180×50 or larger. Must degrade to 120×35 and 80×24.

```text
╭─ JeRyu Flight Deck ─ scope: Fleet/all repos ─ LIVE cursor:1849912 ─ db:sqlite ─ gitlab:12ms ─ docker:ok ─ vault:ok ─╮
│ Tabs: 1 Fleet 2 Queue 3 Repos 4 Workflow 5 Runners 6 Cache 7 VTI 8 Agents 9 Bugs 0 Release  gj Jankurai  ge Evidence │
├───────────────────────┬────────────────────────────────────────────────────────────────────────────┬──────────────────────┤
│ LEFT NAV / SCOPE       │ MAIN WORKSPACE                                                            │ INSPECTOR / PROOF     │
│ families, saved lenses │ tables, DAGs, heatmaps, live queues, traces, editors                     │ selected entity detail│
│ watchlist, top blockers│                                                                            │ actions/evidence/logs  │
├───────────────────────┴────────────────────────────────────────────────────────────────────────────┴──────────────────────┤
│ Event tail: 12:41:09 job#881 failed | 12:41:10 cache miss storm | 12:41:11 agent a17 proposed patch | 12:41:12 vti miss│
├─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ↑↓←→ move  Tab pane  Enter drill  Esc up  / filter  Ctrl-K command  a actions  e evidence  l logs  ? help  q quit       │
╰─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### 4.3 Pane behavior

There are two interaction modes:

**Macro mode**

- Yellow/bright border indicates active pane.
- Arrow keys move pane focus spatially.
- `Tab` cycles panes in reading order.
- `Enter` enters micro mode or drills into selected object.
- `Esc` pops route or returns home if already at root.

**Micro mode**

- Pane title shows `[drill: Esc up]`.
- Arrow keys move inside list/graph/editor.
- `Enter` drills selected row/node.
- `Left` in a graph moves to dependency/predecessor; `Right` moves to dependent/successor.
- `Esc` returns to macro mode first, then route up on second press.

### 4.4 Inspector invariant

The right inspector always describes the focused object, even if the main pane is a graph or heatmap. The inspector never steals focus unless the user presses `i`, `Tab`, or `Enter` on an inspector row.

Inspector structure:

```text
┌ Inspector: job#882 integ:test-nextest-4 ─ LIVE 0.8s ─────────┐
│ status: running  elapsed: 08m13s  eta: ~02m41s  queue: 01m44 │
│ proof: pipeline#581 sha:9f31e4b ref:main source:GitLab+DB     │
├ Tabs: Overview Logs Evidence Deps Metrics Actions Raw ───────┤
│ blocker: none                                                │
│ warnings: cache cold, p95 duration +38%                      │
│ next: open trace / compare p50 / inspect cache misses         │
└───────────────────────────────────────────────────────────────┘
```

---

## 5. Visual language

### 5.1 Terminal capability targets

The default should assume a modern terminal with 24-bit color and Unicode box drawing. Provide fallbacks:

| Capability | Full mode | Fallback |
|---|---|---|
| Color | 24-bit RGB semantic palette | 256-color ANSI approximation; then 16-color fallback. |
| Symbols | Unicode glyphs, braille sparklines, box drawing | ASCII `+`, `-`, `|`, `*`, `!`, `?`. |
| Mouse | click, wheel, drag, right-click when supported | Keyboard-only complete functionality. |
| Resize | responsive recompute | minimum 80×24 compact mode. |
| Animation | 4–12 FPS UI heartbeat; stream-driven pulses | static values with timestamps when low-power/no-animate. |

### 5.2 Color palette

Use color to encode status, not decoration. The exact palette can be adjusted, but semantic names must remain stable in code.

| Semantic | Hex | ANSI fallback | Use |
|---|---:|---:|---|
| `ok` | `#2ee66b` | bright green | fresh success, healthy. |
| `good_dim` | `#198c45` | green | completed/low-emphasis success. |
| `warn` | `#ffcc33` | yellow | degraded, risk, stale-but-usable. |
| `hot` | `#ff8c2a` | bright yellow/red | queue pressure, high utilization. |
| `crit` | `#ff3b5c` | bright red | failed, blocked, unsafe, denied. |
| `unknown` | `#9aa4b2` | white/dim | unknown/not measured. |
| `info` | `#4db8ff` | bright blue | neutral events, links, metadata. |
| `agent` | `#b678ff` | magenta | agent activity, autonomy. |
| `cache` | `#25d0b4` | cyan | cache/storage/trust. |
| `vti` | `#8be35a` | green | selected/skipped tests and test intelligence. |
| `release` | `#ff6ec7` | magenta | release/canary/prod. |
| `evidence` | `#e4d7a1` | white/yellow | proof/receipts. |
| `muted` | `#5e6673` | black/bright black | stale labels, chrome. |
| `focus` | `#ffd166` | yellow | focused pane/object. |
| `selection` | `#234b6d` | blue background | selected row. |

Never rely on color alone. Pair every color with glyph/text.

### 5.3 Status glyphs

| Glyph | ASCII | Meaning |
|---|---|---|
| `●` | `*` | live/running. |
| `◌` | `o` | queued/waiting. |
| `✓` | `OK` | successful. |
| `✗` | `X` | failed. |
| `‼` | `!!` | critical/unsafe. |
| `▲` | `!` | warning/risk/degraded. |
| `◆` | `#` | proof/evidence artifact. |
| `◇` | `<>` | inferred/provisional. |
| `⏸` | `||` | paused/frozen/hold. |
| `↻` | `R` | retry/requeue/reconcile. |
| `⟳` | `S` | streaming refresh. |
| `⚿` | `K` | key/secret/grant. |
| `☷` | `C` | cache. |
| `⇄` | `<->` | sync/mirror/drift. |
| `⊘` | `NO` | denied/blocked. |

### 5.4 Progress bars

Progress bars must encode truth source:

```text
actual     [████████████░░░░] 73% actual  completed jobs/stages
estimated  [▓▓▓▓▓▓▓▓░░░░░░░] ~61% ETA    historical model confidence .78
unknown    [????????????????] unknown     missing DAG/duration data
stale      [████████░░░░░░░░] 52% stale 4m
blocked    [███████⊘░░░░░░░] blocked: approval
skipped    [████▒▒▒▒▒▒▒▒▒▒▒] VTI skipped 42 tests, confidence .94
```

### 5.5 Motion and “incredible moving activity” rules

Motion should make the terminal feel alive without making it noisy.

Allowed motion:

- **Live cursor pulse** in header every UI heartbeat while streams are fresh.
- **DAG edge shimmer** only for currently running jobs or active data flow.
- **Event ticker** for recent events, capped and pauseable.
- **Sparklines** for queue depth, cache hit ratio, runner utilization, VTI savings, failure rate.
- **Heatmap cells** update as events arrive, but preserve selection/scroll.
- **Trace follow mode** scrolls only when follow is enabled.
- **Activity particles** in graph edges are optional and disabled in reduced-motion mode.

Forbidden motion:

- No blinking red areas faster than 1 Hz.
- No animations that imply progress when progress source is unknown.
- No focus jumps caused by incoming events.
- No continuous full-screen repaint storms; render only dirty regions/frame.

### 5.6 Density modes

| Mode | Trigger | Behavior |
|---|---|---|
| `Ultra` | ≥200 columns | Many panes, graphs + tables + inspector + event tail. |
| `Wide` | 160–199 columns | Standard 3-column layout. |
| `Medium` | 120–159 columns | Left nav collapses; inspector can overlay. |
| `Compact` | 100–119 columns | Single primary pane + bottom inspector. |
| `Tiny` | 80–99 columns / 24 rows | List-first, no sidebars, command palette and detail overlays. |

---

## 6. Keyboard, mouse, and command model

### 6.1 Universal keys

| Key | Universal action |
|---|---|
| `↑` / `k` | Move selection up or graph focus up. |
| `↓` / `j` | Move selection down or graph focus down. |
| `←` / `h` | Move left pane/graph predecessor; in root with no sibling, go up/back preview. |
| `→` / `l` | Move right pane/graph successor; in graph/list, drill preview. |
| `Enter` | Drill selected entity or open action preview. |
| `Esc` | Exit micro mode, close overlay, or pop route stack. |
| `Backspace` | Route history back. |
| `Tab` | Next pane/subtab/top tab by mode. |
| `Shift+Tab` | Previous pane/subtab/top tab. |
| `Ctrl-K` or `:` | Command palette. |
| `/` | Filter current scope. |
| `Ctrl-/` | Global search. |
| `?` | Context help. |
| `a` | Actions for selected object. |
| `e` | Evidence for selected object. |
| `l` | Logs/traces for selected object. |
| `m` | Metrics for selected object. |
| `o` | Open external URL/path when safe. |
| `y` | Copy selected ID/SHA/path/digest. |
| `p` | Pin/unpin watch entity. |
| `f` | Toggle follow mode for current stream/log. |
| `b` | Jump to top blocker. |
| `n` | Jump to next recommended action. |
| `x` | Explain selected warning/metric/status. |
| `r` | Context retry/requeue/refresh preview. |
| `Ctrl-R` | Force refresh current scope. |
| `Ctrl-S` | Save/export current view. |
| `Ctrl-G` | Emergency close overlays and return to Fleet. |
| `q` | Quit; confirm if actions/streams are active. |

### 6.2 Go-to shortcuts

```text
g0 Fleet                  gq Queue                 gr Repos
gw Workflow               gu Runners               gc Cache
gv VTI                    ga Agents                go Autonomy
gb Bugs                   gg Git Sync              gj Jankurai
gh Churn/Risk             gs Security              gi Artifacts
gR Release                ge Evidence              gl LLM/Autonomy
gS Settings/Source Doctor
```

### 6.3 Command palette

The command palette is the fastest path for experts and the safest path for risky actions.

Requirements:

- Fuzzy search over pages, entities, actions, saved lenses, recent objects, docs, and proof timeline.
- Contextual actions at top.
- Disabled actions shown dimmed with reason.
- Right-side preview shows risk, side effects, required grant, freshness requirements, dry-run availability, undo/rollback availability, evidence consumed/created, and exact target.
- `Enter` opens preview or executes read-only command.
- `Shift+Enter` dry-runs if supported.
- `Ctrl+Enter` pins command to watch bar.
- Palette must be usable while a log follows; it should not stop streams.

Mock:

```text
╭─ Command Palette ─────────────────────────────────────────────────────────────────────╮
│ query: merge veox-enclave !843                                                        │
├──────────────────────────────────┬────────────────────────────────────────────────────┤
│ > Preview merge !843         M   │ Action: request_merge                               │
│   Open MR !843 evidence          │ Risk: production-adjacent high                      │
│   Explain blockers for !843      │ Freshness required: MR<10s CI<10s Security<5m        │
│   Run release preflight          │ Required proof: CI green, VTI valid, no hard findings │
│   Show changed files             │ Will create: action receipt, merge passport check    │
│                                  │ Disabled now: Jankurai score stale 2h                │
╰─ ↑↓ choose  Enter preview  Shift-Enter dry-run  Esc close  ? details ─────────────────╯
```

### 6.4 Mouse support

Mouse is optional but excellent:

- Click selects row/node.
- Double-click drills.
- Wheel scrolls current pane.
- Shift-wheel scrolls horizontally.
- Drag pans DAG/log/table when supported.
- Right-click opens action menu when terminal supports it; otherwise no lost functionality.
- Hover is not required because terminal hover support is inconsistent; selection drives inspector.

---

## 7. Core screens

The screens below are build contracts. Each screen defines purpose, layout, data, actions, drilldowns, and “incredible” touches.

### 7.1 Fleet Mission Control

**Purpose:** show everything important across all repo families in one screen and let the operator jump directly to the next useful action.

Questions answered:

- What is happening across all repos?
- Which family/repo/workflow needs attention first?
- Are we near capacity limit?
- Are cache/VTI/agents/releases/security healthy?
- What changed in the last few seconds?
- What should I do next?

Wide mock:

```text
╭─ JeRyu Flight Deck: Fleet ─ LIVE ● cursor:1849912 ─ scream:87 ─ safe merge:WARN ─ release:BLOCKED ─────╮
│ Repo families 38 | Repos 417 | Running 93 | Queued 42 | Failed 7 | Agents 18 active / 3 blocked | Cache 84% │
├─ Repo Families ───────────────┬─ Live Work / Critical Path ───────────────────────┬─ Attention / Next ───────────┤
│ Family        R Q F A Rel Risk │ ▶ veox-enclave #581  integ:test-nextest-4  08m/11m │ ‼ release blocked: canary e2e │
│ ▶ veox-*     51 18 3 9  blk ▲ │   build ✓ -> integ ● -> audit ◌ -> pkg ◌ -> deploy │   proof: rel#91 gate e2e     │
│   redline-*   9  2 1 2  ok  ▲ │   queue wait rust-fast +214% vs 7d                │   action: open trace / retry │
│   jeryu       6  1 0 1  ok  ✓ │                                                    │ ▲ cache pressure target 92%  │
│   isolated   11  0 2 0  n/a ✗ │ ▶ redlinedb #244    cargo-deny failed             │ ▲ VTI miss: auth_tests       │
│   infra       4  5 1 0  warn▲ │ ▶ veox-api !843    MR drift after review           │ ▲ agent a17 grant expires 3m │
├─ Capacity / Speed ────────────┴────────────────────────────────────────────────────┴──────────────────────────────┤
│ Theoretical 160 slots | Online 87 | Effective 104 | Busy 79 | Limit-distance 1.34× | Waste p50 4m35s | Cause: tags│
│ Queue pressure 0.91 last10m ▁▂▄▆██▇▆  Cache hit 0.78 ↓  VTI saved 61%  Miss risk .06  Agents ROI +14h / -$3.20 │
├─ Event tail ───────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ 12:41:09 job#881 ✗ | 12:41:10 cache miss storm rust crates | 12:41:11 agent a17 patch MR !843 | 12:41:12 vti miss │
╰─ Enter drill  b blocker  n next  x explain  / filter  Ctrl-K command  Tab panes  Esc back ───────────────────────╯
```

Main panels:

1. **Posture header:** source freshness, DB backend, GitLab/Docker/Vault/cache/broker health, active scope, cursor, running actions, kill bell/freeze state.
2. **Repo families:** grouped rows with running/queued/failed/agents/release/risk columns.
3. **Live Work / Critical Path:** top active pipelines sorted by attention impact, with mini DAGs.
4. **Attention / Next:** ranked blockers and recommended actions with proof links.
5. **Capacity / Speed strip:** theoretical limit, online/effective slots, queue pressure, limit distance, SCREAM index, primary bottleneck.
6. **Event tail:** recent important events, not raw spam.

Data required:

- `TuiReadModel.mission`
- repo family summaries
- attention queue
- queue/capacity summary
- active pipelines/jobs
- agents summary
- cache/VTI/release/security status
- source freshness
- event stream

Actions:

- Drill family/repo/pipeline/job/blocker.
- Open top blocker.
- Open next action preview.
- Pin entity to watchlist.
- Save lens.
- Export screenshot/capture.

Incredible touches:

- Header pulse color reflects global posture.
- The critical path graph animates only active edges.
- Attention queue uses “why + proof + action” rows, not vague alerts.
- Family rows show tiny queue/failure sparklines.
- Press `x` on SCREAM/limit-distance opens decomposition.

### 7.2 Queue / Theoretical Limit

**Purpose:** answer the user’s explicit question: “How close am I running to the theoretical limit?”

This is not simple CPU utilization. It is a multi-limit model that distinguishes physics, fleet, and policy.

#### 7.2.1 Three limits

| Limit | Meaning | Typical fix if bad |
|---|---|---|
| Physics limit | Lower bound from pipeline DAG using best-case job durations and zero queue delay. | Split serial jobs, add `needs`, reduce test duration. |
| Fleet limit | Lower bound with current runners/nodes/pools/tags/warm managers/cache/resource constraints. | Scale pools, rebalance tags, add nodes, warm images/cache, fix Docker/disk. |
| Policy limit | Lower bound including mandatory approvals, canary minimums, freeze windows, security/artifact gates. | Approve/review/fix policy blockers; runner scaling will not help. |

#### 7.2.2 Core formulas

```text
D_best(j)      = p10 historical duration for same repo/job/stage/ref class with hot cache
D_p50(j)       = median historical duration
D_p90(j)       = p90 historical duration
D_current(j)   = observed elapsed/completed duration
Deps(j)        = explicit needs + stage barriers + artifact/child-pipeline dependencies
Pools(j)       = eligible runner pools/tags/trust tiers
Policy(j)      = required gates, approvals, security, release constraints
```

Physics bound:

```text
physics_eta = longest_path_sum(D_best, DAG_deps)
physics_efficiency = physics_eta / max(actual_or_predicted_wall_clock, 1s)
```

Fleet bound:

```text
fleet_eta = simulate_schedule(
  jobs        = queued + running + ready + pending,
  durations   = D_p50 adjusted by cache/VTI/resource state,
  resources   = runner_slots_by_pool_node_tag,
  cold_start  = p50 manager/container startup,
  constraints = deps + tags + trust tier + remote affinity + request_concurrency
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

Capacity per pool:

```text
theoretical_slots(pool) = min(
  pool.max_managers * pool.runner_concurrency,
  pool.request_concurrency_limit,
  remote_node_available_slots(pool),
  gitlab_runner_limit(pool),
  optional_global_cap
)

online_slots(pool) = sum(manager.online ? manager.configured_concurrency : 0)
busy_slots(pool)   = count(running_jobs assigned_to pool/tag)
headroom(pool)     = theoretical_slots(pool) - online_slots(pool)

effective_slots(pool) = online_slots
  - paused_slots
  - unhealthy_slots
  - incompatible_tag_slots
  - trust_tier_blocked_slots
  - disk_pressure_blocked_slots
  - image_pull_backoff_slots
  - request_concurrency_blocked_slots
```

Queue drain:

```text
work_seconds(job) = remaining_observed_if_running
                 else historical_p50(repo, job_name, stage)
                 else stage_default_p50

drain_eta(pool) = sum(queued_work_seconds matching pool constraints)
                / max(1, effective_slots_freeing_rate)
```

Limit distance:

```text
critical_path_min = longest_path_sum(stage/job p10 or p50) over pipeline DAG assuming infinite runners
current_projection = simulated schedule over online/effective slots and dependency constraints
limit_distance = current_projection / critical_path_min
```

A value near `1.0×` means close to ideal. `>2.0×` means major waste.

SCREAM Index:

```text
scream = clamp(100 * weighted_mean([
  policy_efficiency,           weight .30,
  useful_runner_utilization,   weight .20,
  non_obsolete_work_ratio,     weight .15,
  cache_health_score,          weight .10,
  vti_confidence_score,        weight .10,
  source_freshness_score,      weight .10,
  blocker_resolution_score,    weight .05
]), 0, 100)

useful_runner_utilization = busy_runner_seconds_on_non_superseded_jobs / total_runner_capacity_seconds
non_obsolete_work_ratio   = active_non_superseded_jobs / max(active_jobs, 1)
cache_health_score        = hit_ratio * not_tainted_factor * not_full_factor
vti_confidence_score      = 1 - recent_selector_miss_rate_weighted
source_freshness_score    = min(source freshness scores)
```

#### 7.2.3 Queue screen mock

```text
╭─ Queue / Theoretical Limit ─ Fleet all repos ─ model:p50/p90 24h+7d ─ confidence:.81 ─ LIVE ─────────╮
│ Online slots 87 / Theoretical 160 / Effective 104 │ Busy 79 │ Queued 42 │ Drain ETA 18m p50 / 31m p90 │
│ Limit-distance 1.34× │ Physics floor 13m27s │ Projected 18m02s │ Waste 4m35s │ Cause: tag bottleneck TEST │
├─ Pools ──────────────────────────────────────┬─ Queue by constraint ───────────────────────────────────────┤
│ Pool              on/th/eff busy q util wait  │ Constraint              jobs work       fix                 │
│ ▶ rust-fast        24/48/31  24 18 100% 12m04 │ tag=rust-fast            18  9h12m      scale +9 managers    │
│   rust-default     33/60/41  29  8  88%  3m11 │ needs docker socket       7  2h01m      add remote capacity  │
│   gpu-audit         2/ 4/ 2   2  3 100% 21m40 │ serial release gate       4  1h10m      no runner fix        │
│   sec-scan          4/ 8/ 4   3  5  75%  8m32 │ image cold-start          9  2h20m      pre-pull/cache       │
│   remote-nyc       18/32/21  16  6  89%  4m50 │ disk pressure             6  1h44m      GC/buildkit          │
├─ Critical path ──────────────────────────────┴──────────────────────────────────────────────────────────────┤
│ veox-api#581 build ✓2m11 ─► integ ●9m/~14m ─► audit ◌4m ─► package ◌2m ─► release blocked by integ        │
│ Slow deltas vs 7d: integ +42%, cargo-deny +31%, image-build +28%, queue wait rust-fast +214%              │
├─ Recommendations ──────────────────────────────────────────────────────────────────────────────────────────┤
│ 1. Scale rust-fast +9 managers: saves ~5m40 p50, confidence .78, cost +$1.12/h, risk low                  │
│ 2. Pre-pull sec-scan image on remote-nyc: saves ~2m10 p50, confidence .65, risk low                       │
│ 3. Cancel 6 superseded pipelines: frees 12 slots now, confidence .95, risk low                            │
╰─ Enter drill  s scale preview  d diagnose  x explain  h history  / filter  Esc back ─────────────────────╯
```

Bottleneck classes:

| Bottleneck | Signal | Suggested action |
|---|---|---|
| Queue saturation | ready jobs > free eligible runners | Start managers, add nodes, adjust pool caps. |
| Tag fragmentation | idle runners exist but not eligible | Rebalance tags, pool config, job tags. |
| Cold starts | wait dominated by manager/image startup | Increase warm managers, pre-pull images, warm cache. |
| Cache miss storm | hit ratio drops; download/build time spikes | Inspect misses, taints, force refresh, cache categories. |
| Serial DAG | critical path stage has low parallelism | Split job, add `needs`, remove barriers. |
| VTI fallback | many full test plans due low confidence | Teach mappings, inspect selector misses. |
| Obsolete work | superseded pipelines still consuming slots | Cancel superseded pipelines. |
| Policy wait | release/canary/approval/freeze gate | Open proof; runner scaling will not help. |
| Security/artifact gate | unsigned artifact, SAST finding, secret leak | Drill security/artifact evidence. |
| Remote node pressure | CPU/mem/disk/SSH/Docker saturated | GC, rebalance, add node, drain/restart. |
| GitLab API/broker lag | webhook/API latency or consumer lag | Inspect source doctor, broker, GitLab readiness. |

### 7.3 Repos / Repo Family Atlas

**Purpose:** manage many repos and repo families (`veox-deploy`, `veox-enclave`, etc.) as first-class groups while preserving isolated repos.

Grouping rules:

1. Explicit config wins: `repo_family` in settings or repo metadata.
2. Pattern rules: prefix before second `-`, e.g. `veox-*`, `redline-*`.
3. Provider namespace: GitLab group/project path.
4. Dependency graph: repos sharing release/artifact/cache/test dependencies can be linked even across names.
5. Fallback: `isolated` family.

Family row fields:

| Field | Meaning |
|---|---|
| family name | `veox-*`, `jeryu`, `isolated`, etc. |
| repo count | active / archived / disabled. |
| live jobs | running, queued, failed, manual. |
| agents | active, blocked, racing, awaiting grant. |
| release posture | ok, blocked, canary, prod drift, rollback armed. |
| cache/VTI | hit ratio, taints, VTI savings/misses. |
| risk | security/Jankurai/artifact/admission gates. |
| trend | failure/queue/score sparkline. |
| next action | top family-level recommendation. |

Mock:

```text
╭─ Repo Atlas ─ families:12 repos:417 visible:all ─ sort:attention ─────────────────────────────────────╮
│ Family        Repos Run Que Fail Agents Release Cache VTI  Jank  Risk Trend        Next               │
│ ▶ veox-*        83   51  18   3    9    blocked 78%  61%  82.4  ▲▲  ▁▂▅██▆       open canary e2e     │
│   redline-*     14    9   2   1    2    ok      91%  44%  76.1  ▲   ▁▁▂▃▂▁       fix cargo-deny      │
│   jeryu          7    6   1   0    1    ok      84%  55%  88.9  ✓   ▁▁▁▂▁▁       review MR hook      │
│   infra          9    4   5   1    0    warn    69%  n/a  71.0  ▲   ▁▃▅▆▆▅       add remote node     │
│   isolated     304   11   0   2    0    n/a     72%  12%  61.3  ✗   ▁▁▁▁▁▁       triage stale repos  │
├─ Family detail: veox-* ───────────────────────────────────────────────────────────────────────────────┤
│ critical repos: veox-enclave, veox-deploy, veox-api | shared blockers: rust-fast tags, canary e2e     │
│ release dependency: veox-enclave -> veox-deploy -> prod | cache heavy: cargo registry + target dirs   │
╰─ Enter family  → repos  x explain risk  f filter family  p pin  Esc back ─────────────────────────────╯
```

Drilldowns:

- Family -> repo list filtered to family.
- Family -> shared queue constraints.
- Family -> release graph.
- Family -> shared cache categories.
- Family -> agents/bugs/security scoped to family.

### 7.4 Repo Cockpit

**Purpose:** show one repo’s operational state: workflows, current MRs/branches, queue, agents, bugs, cache, VTI, Jankurai, release/security, and Git sync.

Mock:

```text
╭─ Repo: veox-enclave ─ project:48 ─ branch:main sha:9f31e4b ─ LIVE ─────────────────────────────────────╮
│ posture: WARN │ running:4 queued:2 failed:1 │ MR:!843 drift:no │ VTI:61% saved miss-risk:.06 │ cache:78% │
├─ Workflow now ──────────────────────────────┬─ Repo health ─────────────────────┬─ Inspector ────────┤
│ #581 main  build ✓ -> integ ● -> audit ◌   │ CI p50 18m p90 31m  +14% vs 7d   │ selected pipeline   │
│ #580 agent/a17 patch  test ◌ waiting tag    │ Jankurai 82.4 cap:security       │ blockers: integ     │
│ #579 release  canary blocked e2e            │ Security critical:0 high:2       │ proof: rel#91       │
│ !843 MR  review ok, CI running              │ Bugs ready:7 in_progress:3       │ actions: trace/retry│
├─ Repo timeline ─────────────────────────────┴───────────────────────────────────┴─────────────────────┤
│ 12:35 push main -> VTI plan 92 selected 318 skipped | 12:37 pipeline #581 | 12:40 cache miss storm     │
╰─ Enter workflow  w DAG  l logs  e evidence  a actions  / filter  Esc family ──────────────────────────╯
```

Repo widgets:

- Active workflow cards.
- MR/PR rail with review/approval/drift/check states.
- Recent pushes and VTI plans.
- Top jobs by duration/failure/queue wait.
- Agents working on repo.
- Bugs mapped to repo.
- Cache footprint and hit/miss trend.
- Jankurai score and blocking categories.
- Security/artifact/release gate status.
- Git sync and admission events.

Actions:

- Open workflow DAG.
- Open live trace.
- Retry/cancel/requeue job.
- Run VTI plan/explain/validate.
- Submit/update bug.
- Spawn agent with bounded grant.
- Run Jankurai audit.
- Release preflight.
- Sync repo/mirror.

### 7.5 Workflow Atlas / Pipeline DAG

**Purpose:** show actual workflow diagrams with live progress, errors, edges, traces, child pipelines, artifacts, VTI decisions, and critical path.

Requirements:

- Support multiple active pipelines per repo/family/fleet, not first-active only.
- Show explicit `needs` edges when available.
- Show stage-barrier inferred edges when explicit edges are missing.
- Show bridge/downstream child pipelines.
- Show artifact dependencies.
- Show manual jobs/gates.
- Show skipped VTI jobs/tests distinctly from not-run jobs.
- Show failure annotations and capsules.
- Show critical path remaining and slack.
- Keep graph navigable with arrows.
- Inspector follows selected node.

Graph encoding:

| Node state | Visual |
|---|---|
| success | green border / `✓` |
| running | blue/green pulsing `●`, elapsed + ETA |
| queued | hollow `◌`, queue duration |
| failed | red `✗`, failure kind |
| blocked | red/yellow `⊘`, blocker label |
| skipped by VTI | green/gray `▒`, confidence badge |
| manual | yellow `⏸ manual` |
| canceled | dim `⊘ canceled` |
| child pipeline | nested card / bridge arrow |
| stale | dim node, stale badge |

Mock:

```text
╭─ Workflow Atlas ─ veox-enclave pipeline#581 main@9f31e4b ─ LIVE ─ ETA ~18m02 ─ critical path highlighted ─╮
│ Minimap: [build][test..........][audit..][pkg][release]                                                   │
├─ DAG ───────────────────────────────────────────────────────────┬─ Job Inspector ──────────────────────────┤
│ build:linux ✓2m11 ─┬─► test:unit ✓4m02 ─────┬─► audit:cargo-deny ◌ │ job: integ:test-nextest-4              │
│ build:docker ✓3m04 ┤                         ├─► audit:sast ◌      │ state: running 08m13 / ~11m            │
│ generate:schemas ✓ │                         └─► package ◌         │ queue: 01m44 pool: rust-fast           │
│                    └─► integ:test-nextest-4 ●━━━━━━━━━━━━━━       │ cache: cold registry misses 42         │
│ VTI skipped 318 tests confidence .94 ▒▒▒▒▒▒▒▒▒                     │ VTI: selected due auth+enclave diff    │
│ child:deploy-preview ◇ waiting parent package                      │ blocker: none; on critical path        │
│                                                                   │ actions: logs retry cancel capsule     │
├─ Trace preview ─────────────────────────────────────────────────┴─────────────────────────────────────────┤
│ 12:40:09 compiling crate enclave-core ...                                                                  │
│ 12:40:31 warning: cache miss registry index serde ...                                                      │
│ 12:41:02 test auth_rotation_should_reseal ...                                                              │
╰─ arrows navigate DAG  Enter drill node  l full trace  e evidence  c critical path  x explain ETA ─────────╯
```

Edge confidence:

| Edge kind | Source | Rendering |
|---|---|---|
| `needs_explicit` | GitLab pipeline config/API | solid line. |
| `stage_barrier` | stage order inferred | dotted/dim line. |
| `artifact_dependency` | artifact needs | line with artifact glyph. |
| `child_pipeline` | bridge/downstream | double arrow. |
| `release_gate` | release/canary/prod state | magenta gate edge. |
| `policy_gate` | action/release/security policy | yellow/red gate edge. |
| `unknown` | insufficient data | no fake edge; show unconnected node with warning. |

Drilldown keys:

- `Enter`: node detail.
- `l`: full live trace for job.
- `e`: evidence/capsule/artifacts for node.
- `c`: jump critical path.
- `[` / `]`: previous/next pipeline.
- `Shift+←/→`: parent/child pipeline.
- `x`: explain selected node status/ETA/blocker.

### 7.6 Live Trace Viewer

**Purpose:** make logs actionable, not a wall of text.

Requirements:

- Streaming transport preferred: WebSocket/SSE/bounded stream.
- Poll fallback with offset/range requests.
- Preserve scroll unless follow mode is on.
- Redact secrets.
- Annotate phases, timestamps, deltas, failures, warnings, cache misses, test names, artifact uploads.
- Jump to first error, previous/next warning, phase boundaries.
- Link trace lines to capsules/evidence.
- Support regex filter, bookmarks, copy excerpt, create bug.

Mock:

```text
╭─ Trace job#882 integ:test-nextest-4 ─ FOLLOW ● ─ offset:184231 ─ redacted:2 ─ annotations:17 ─────────────╮
│ 12:39:58 +00.0s phase: prepare sandbox                                                                    │
│ 12:40:09 +11.2s cargo test --workspace --profile ci                                                       │
│ 12:40:31 +22.0s ▲ cache miss: crates.io index serde v1.0.203  reason: epoch invalidated                  │
│ 12:41:02 +31.1s test auth_rotation_should_reseal ... ok                                                   │
│ 12:42:44 +1m42s test enclave_attestation_rejects_stale_quote ... FAILED                                   │
│ 12:42:44        ✗ assertion failed: quote.age_ms < 5000                                                    │
│ 12:42:45        ◆ capsule candidate: failure kind test_assertion path crates/enclave/tests/attestation.rs │
├─ Side rail ─ phases: prepare ✓ build ✓ test ✗ artifact ◌ cleanup ◌ │ first error line 1882 │ p95 +38% ─────┤
╰─ f follow  / filter  n next match  E first error  b bookmark  c capsule  B bug  y copy  Esc job ─────────╯
```

Trace annotation model:

```rust
pub struct TraceAnnotation {
    pub line_start: u64,
    pub line_end: u64,
    pub kind: TraceAnnotationKind,
    pub severity: Severity,
    pub summary: String,
    pub entity: Option<EntityRef>,
    pub evidence: Vec<EvidenceRef>,
    pub redacted: bool,
}
```

### 7.7 Cache MRI / SmartCache Observatory

**Purpose:** answer “Are we full? What types of files are taking storage? Is cache helping or hurting? Is reuse trusted?”

Questions:

- How full are all cache stores?
- Which categories consume space: Rust crates, Cargo git, target dirs, sccache, OCI layers, artifacts, CAS/materials, registry mirror, temp/buildkit?
- What is hit/miss ratio by repo/family/job/pool?
- Which objects are hot, stale, tainted, leased, promoted, or GC candidates?
- Which misses are avoidable?
- What cache verdict blocked reuse?

Cache categories:

| Category | Examples |
|---|---|
| Cargo registry | sparse index, `.crate` packages. |
| Cargo git | git checkouts/deps. |
| Rust target | `target/debug`, incremental, build scripts. |
| sccache | compiler object cache. |
| OCI layers | image layers, registry mirror. |
| CAS/materials | material objects, action cache, aliases. |
| Artifacts | job artifacts, test reports, release bundles. |
| Toolchains | rustup/toolchain fingerprints, generated toolchains. |
| Temp/buildkit | temporary builders and layer workdirs. |

Mock:

```text
╭─ Cache MRI ─ storage 337GiB / 400GiB 84% ─ hit 78% ↓ ─ taints:6 ─ singleflight saved 19GiB ─────────────╮
│ Category           Size    %     Hit   Trend     Taints  Hot objects              Action              │
│ ▶ cargo registry    94G   23.5   91%   ▁▁▂▂▃     0       serde, tokio, axum       ok                  │
│   rust target      118G   29.5   62%   ▁▂▄██     2       veox-enclave target      GC candidates 41G   │
│   sccache           37G    9.3   84%   ▅▆▇▇▆     0       rustc stable             increase?           │
│   OCI layers        52G   13.0   73%   ▁▃▅▆█     1       sec-scan:latest          pre-pull pin        │
│   CAS/materials     24G    6.0   88%   ▁▁▂▁▁     3       prod artifact base       inspect verdicts    │
│   artifacts         12G    3.0   n/a   ▁▁▁▁▁     0       junit, coverage          expire old          │
├─ Miss reasons ───────────────────────────────┬─ Verdicts / Trust ─────────────────────────────────────┤
│ epoch invalidated      42%                    │ denied: toolchain mismatch 17                         │
│ no lease               18%                    │ tainted: untrusted runner 6                            │
│ cold object            15%                    │ promoted: prod-safe 41                                 │
│ force refresh rule     12%                    │ leases active: 183                                     │
│ unknown                13%                    │ cache epochs: rust 2026-05-25                          │
╰─ Enter category  g GC preview  t taints  v verdict  h hot  x explain miss  Esc back ──────────────────╯
```

Required payload:

```rust
pub struct CacheDashboard {
    pub total_bytes: u64,
    pub budget_bytes: u64,
    pub hit_ratio: f64,
    pub singleflight_coalesced: u64,
    pub categories: Vec<CacheCategorySummary>,
    pub hot_entries: Vec<HotCacheEntry>,
    pub taints: Vec<CacheTaintSummary>,
    pub verdicts: Vec<CacheVerdictSummary>,
    pub gc_plan: Option<CacheGcPlan>,
    pub source: SourceFreshness,
}
```

Actions:

- Preview GC plan.
- Pin/promote object if safe.
- Explain cache verdict.
- Force refresh rule preview.
- Open object detail.
- Open taint source.
- Create bug from repeated miss storm.

Safety:

- Cache deletes require dry-run preview and lease/taint awareness.
- Do not purge prod-promoted/material-trusted objects without elevated confirmation.
- Show exact reclaimed bytes estimate and risk.

### 7.8 VTI Smart Test Skipper Cockpit

**Purpose:** prove the smart test skipper is working, saving time, and not hiding failures.

Questions:

- Which tests were selected/skipped and why?
- How much time did VTI save?
- What is recent selector miss rate?
- Which misses need learning/repair?
- Are fallback/full-test decisions appropriate?
- Are skipped tests safe for merge/release?

Mock:

```text
╭─ VTI / Tests ─ scope: veox-* ─ saved 61% CI minutes ─ miss risk .06 ─ confidence .91 ─────────────────╮
│ Plan latest: veox-enclave main 9f31e4b -> selected 92 / skipped 318 / forced 7 / fallback 0             │
├─ Plans ─────────────────────────────────────┬─ Selector misses ─────────────────┬─ Proof ──────────────┤
│ ▶ plan#771 veox-enclave  conf .94 saved 68% │ auth_rotation_should_reseal miss  │ changed files: 14     │
│   plan#770 veox-api      conf .87 saved 54% │ enclave_quote_expiry miss         │ subsystems: auth,sgx  │
│   plan#769 redlinedb     conf .73 fallback  │ db_wal_checkpoint miss            │ reason: mapping hit   │
│   plan#768 jeryu         conf .91 saved 49% │                                  │ receipt: vti#771      │
├─ Savings trend ─ ▁▂▄▅▇█▆ ─ false-skip risk trend ─ ▁▁▂▁▃▂ ─ top unmapped areas: auth, enclave, WAL ────┤
╰─ Enter plan  m misses  L learn  A audit  R run full  x explain skip  e receipt  Esc back ─────────────╯
```

Core metrics:

| Metric | Meaning |
|---|---|
| selected tests | Tests VTI chose to run. |
| skipped tests | Tests VTI judged irrelevant. |
| forced tests | Always-run critical tests. |
| fallback count | Times VTI chose full test due low confidence. |
| time saved | Estimated duration skipped minus VTI overhead. |
| miss rate | Selector misses over recent window, weighted by severity. |
| confidence | Model confidence for plan. |
| repair debt | Misses needing learned mapping or forced tests. |
| false-skip risk | Estimated chance a skipped test should have run. |

Guardrail rule:

```text
A merge/release proof may use VTI only when:
  confidence >= threshold for risk tier
  no unrepaired severe selector misses touch changed subsystem
  forced critical tests passed
  plan receipt references exact base/head SHA
  test mappings version is recorded
Otherwise show VTI as advisory and require full/fallback test proof.
```

Actions:

- Explain plan.
- Validate plan against selector misses.
- Run selected tests.
- Run full tests.
- Teach/learn mapping from miss.
- Mark critical test forced.
- Create bug from VTI miss.

### 7.9 Agent Tower

**Purpose:** show what every agent is doing, what authority it has, whether it is helping, and how to inspect/edit/pause/kill/revoke.

Agent lifecycle model:

```text
queued -> granted -> running -> waiting_ci -> waiting_review -> blocked -> fix_proposed -> verified -> merged/done
                                  |             |             └-> failed/abandoned
                                  └-> racing -> winner_selected -> cleanup
```

Dedicated tables to add:

```text
agent_sessions(id, agent_id, actor, provider, model, repo, started_at, ended_at, status, budget, cost, task_summary)
agent_tasks(id, session_id, bug_id, mr_iid, branch, base_sha, head_sha, status, priority, created_at, updated_at)
agent_steps(id, task_id, step_index, kind, status, summary, started_at, ended_at, evidence_id, log_ref)
agent_messages(id, session_id, role, redacted_content_ref, token_count, cost, latency, provider, raw_hash)
agent_artifacts(id, task_id, kind, path, url, digest, redacted, created_at)
agent_races(id, task_id, hypothesis_count, winner_branch, status, created_at, resolved_at)
```

Mock:

```text
╭─ Agent Tower ─ active:18 blocked:3 racing:2 cost:$3.20/h saved:14.2h est ─ kill bell:armed ─────────────╮
│ Agent      Repo           Task/Bug       State        Grant       Branch/MR       CI       Spend  ROI    │
│ ▶ a17      veox-api       bug#421 auth   waiting_ci   expires 3m  agent/a17 !843  ● #581   $0.42  +2.1h │
│   a22      redlinedb      bug#390 WAL    blocked      missing     agent/a22       ✗ #244   $0.31  ?      │
│   a09      jeryu          MR hook        running      ok 22m      agent/a09       ◌        $0.18  +0.7h │
│   race-7   veox-enclave   bug#388        racing       ok 14m      4 branches      mixed    $0.91  ?      │
├─ Selected a17 ───────────────────────────────┬─ Live log / intent ─────────────────────────────────────┤
│ authority: agent_task grant#991 project 48   │ 12:40 proposed patch to auth/session.rs                 │
│ allowed: propose_patch, run_tests, bug_attempt│ 12:41 pipeline #581 started                             │
│ denied: request_merge, release, secrets       │ 12:42 waiting CI critical path integ:test-nextest-4      │
│ evidence: bug#421, plan#771, mr!843           │ next: inspect trace or extend grant                      │
╰─ Enter detail  p pause  K kill  G grant/revoke  c config  l logs  r race view  Esc back ───────────────╯
```

Actions:

- Pause/resume agent.
- Revoke/extend grant.
- Open logs/messages.
- Open branch/MR/pipeline.
- Compare patch diff.
- Promote race winner.
- Cleanup losing race branches.
- Record bug attempt.
- Edit agent/autonomous workflow config.

Safety:

- Show every agent’s grant envelope: actor, project/ref/base SHA, allowed actions, denied actions, expiry, budget, idempotency.
- Mutating agent actions require capability envelope and nonce.
- Production/release/secret actions are unavailable unless explicitly granted with proof.

### 7.10 Autonomous Workflow Governance

**Purpose:** control autonomous workflows and their configs without losing safety.

Data:

- workflow ID/name/version
- repo/family scope
- trigger: webhook, schedule, event, manual
- enabled/paused/frozen state
- risk tier
- max budget/concurrency
- allowed actions/grants
- required evidence gates
- kill bell/freeze behavior
- last run, next run, success/failure
- config source path, checksum, last editor

Mock:

```text
╭─ Autonomy Governance ─ kill bell:armed ─ freeze:none ─ active workflows:12 ───────────────────────────╮
│ Workflow              Scope        Trigger        State    Risk  Budget  Last result   Next action    │
│ ▶ bug-fixer-rust       veox-*       bug.ready      enabled  med   $20/d   7 fixed       edit config    │
│   merge-passport       all          MR update      enabled  high  n/a     14 pass 2 deny audit gates   │
│   release-nightwatch   prod repos   release gate   paused   high  $5/d    stale 2h      resume?        │
│   cache-gc-advisor     all          hourly         enabled  low   n/a     ok            view plan      │
├─ Config editor preview ────────────────────────────────────────────────────────────────────────────────┤
│ yaml path: .jeryu/autonomy/bug-fixer-rust.yaml  checksum: sha256:...  schema: valid                    │
│ allowed_actions: propose_patch, run_tests, bug_record_attempt  denied: request_merge, secrets, release │
│ diff pending: max_concurrency 3 -> 5, budget 20 -> 25                                                  │
╰─ e edit  d diff  v validate  p pause  k kill bell  s save preview  Esc back ──────────────────────────╯
```

Config edit rules:

- Edits are form-based or `$EDITOR`-based with schema validation.
- Show diff before save.
- Dry-run validation checks grants, risk, budgets, repo patterns, forbidden actions.
- Save produces config event and evidence receipt.
- High-risk workflow config changes require typed confirmation and possibly reviewer approval.

### 7.11 Bugs / Issues Board

**Purpose:** unify bugs/issues across repos, show agent attempts, prevent duplicate effort, and connect failures/findings to work.

Lanes:

```text
needs_triage -> accepted -> ready -> in_progress -> fix_proposed -> reviewing -> verifying -> done
                  |            |             |              └-> blocked
                  └-> needs_info / duplicate / invalid / cannot_reproduce / wont_do
```

Mock:

```text
╭─ Bugs / Issues ─ scope:all ─ open:183 ready:41 in_progress:17 blocked:9 agent-ready:28 ────────────────╮
│ Lane: READY                              │ IN PROGRESS                         │ BLOCKED                 │
│ ▶ #421 veox-api auth token refresh high  │ #390 redlinedb WAL race a22         │ #377 veox-deploy secrets │
│   #418 veox-enclave quote expiry crit    │ #388 enclave attestation race-7     │ #369 jeryu MR hook       │
│   #402 infra remote disk pressure med    │ #379 cache taint false positive     │                         │
├─ Bug detail #421 ──────────────────────────────────────────────────────────────────────────────────────┤
│ title: auth token refresh flakes | source: job#882 trace line 1882 | severity:high priority:P1        │
│ attempts: a17 running MR!843 CI#581 | evidence: capsule#991, vti#771, trace excerpt, cache miss        │
│ acceptance: test auth_rotation_should_reseal passes and no selector miss for auth subsystem             │
╰─ Enter bug  A assign agent  N new bug from selected evidence  u update  l link  e evidence  Esc back ─╯
```

Bug detail must show:

- canonical report fields
- source/target project
- component, severity, priority, difficulty
- reproduction steps
- acceptance criteria
- evidence list
- linked jobs/MRs/branches/releases
- attempts and CI outcomes
- owner/agent assignment
- status timeline
- external refs/provider sync

Actions:

- Submit bug from selected entity/trace/finding.
- Update status/severity/priority/owner/component.
- Link evidence/MR/job/finding.
- Start/fail/complete attempt.
- Spawn agent with bounded grant.
- Mark duplicate/invalid/wont_do with reason.

### 7.12 Git Sync / Remote State

**Purpose:** show local/fleet repo sync, mirrors, hooks, admission decisions, branch drift, backups, standards compliance.

Mock:

```text
╭─ Git Sync ─ repos:417 dirty:12 drift:8 hooks missing:3 admission denies:2 ─────────────────────────────╮
│ Repo              Branch    Local SHA  Remote SHA Drift Dirty Hooks Mirror Admission Last sync         │
│ ▶ veox-api         main      9f31e4b    9f31e4b   no    no    ok    ok     audit 1     12s              │
│   veox-enclave     main      a13c0d1    b88e921   yes   no    ok    lag    deny 1      3m               │
│   redlinedb        perf-x    31aa029    31aa029   no    yes   miss  ok     ok          9m               │
│   jeryu            mr-hook   ff00291    eee9191   yes   yes   ok    ok     audit       1m               │
├─ Selected admission decision ──────────────────────────────────────────────────────────────────────────┤
│ ref: refs/heads/main actor:agent/a17 verdict:audit matched_grant:grant#991 reasons:[protected branch]  │
╰─ Enter repo  s sync preview  h hooks  m mirror  a admission  b backup  x explain drift ───────────────╯
```

Data:

- tracked repositories
- branch/head/dirtiness
- hooks installed/enabled
- mirror jobs
- Git command events
- ref updates
- risk approvals
- command artifacts
- admission decisions
- remote provider state
- MR/PR drift from GitHost adapters

### 7.13 CI Bottleneck Lab

**Purpose:** deep profiling of slow CI across jobs, stages, repos, families, pools, cache states, and time windows.

Dimensions:

- repo/family
- branch/ref class
- pipeline source
- stage/job name
- pool/tag/runner/node
- cache hit/miss/taint state
- VTI selected/full/fallback
- time window
- commit/churn size
- artifact size
- image/toolchain version
- remote/local execution
- retry/flaky status

Mock:

```text
╭─ CI Bottleneck Lab ─ scope:veox-* window:7d compare:previous 7d ───────────────────────────────────────╮
│ Bottleneck                     Impact       Δ vs prev   Cause confidence  Suggested fix                 │
│ ▶ integ:test-nextest           41h/wk       +42%        cache cold .78    warm target + split shards    │
│   cargo-deny                   12h/wk       +31%        advisory db .64   cache advisories              │
│   image-build sec-scan         10h/wk       +28%        OCI pull .81      pre-pull/pin base             │
│   queue rust-fast              31h/wk       +214%       tags .91          scale + rebalance             │
│   VTI fallback auth subsystem   8h/wk       +18%        misses .87        repair mappings               │
├─ Selected: integ:test-nextest ─────────────────────────────────────────────────────────────────────────┤
│ p50 11m p90 22m p99 39m | best 6m12 | cache hit p50 7m, miss p50 18m | top repos: enclave/api/deploy  │
│ critical path appearances: 74% | flake retries: 9% | trace signatures: quote expiry, db lock, network    │
╰─ Enter detail  s simulate fix  b bugs  c cache  v VTI  p pool  x explain  Esc back ───────────────────╯
```

Simulation mode:

- Scale pool by N.
- Warm cache category.
- Add node.
- Split job shards.
- Add explicit `needs` edges.
- Cancel superseded pipelines.
- Repair VTI selector.
- Pre-pull images.

Every simulation shows estimated saved time, confidence, assumptions, cost, and risk.

### 7.14 Runners / System Utilization

**Purpose:** show physical/virtual execution capacity, runner managers, remote nodes, Docker/container health, host storage, and scaling controls.

Mock:

```text
╭─ Runners / System ─ pools:12 managers:87 nodes:9 unhealthy:4 disk-warn:2 ─────────────────────────────╮
│ Pool          Managers on/max  Slots busy/eff/theor  Queue  p95 wait  Node pressure       State       │
│ ▶ rust-fast      24/48         24/31/48              18     12m04     cpu 72 mem 61 disk 84 saturated │
│   rust-default   33/60         29/41/60               8      3m11     cpu 55 mem 48 disk 66 ok        │
│   sec-scan        4/8           3/4/8                 5      8m32     image pulls high     warn      │
│   gpu-audit       2/4           2/2/4                 3     21m40     scarce              saturated │
├─ Nodes ───────────────────────────────────────────────────────────────────────────────────────────────┤
│ remote-nyc cpu 89% mem 76% disk 91% ssh 31ms docker ok managers 18/32  GC reclaimable 41G             │
│ local     cpu 44% mem 53% disk 68% docker ok managers 37/64                                             │
╰─ Enter pool/node  s scale preview  d drain  p pause  g GC  l logs  x explain pressure ────────────────╯
```

Required metrics to plumb:

- CPU/memory/disk/network per node.
- Docker daemon health.
- Container restart/OOM/die events.
- Manager image digest/config hash/version.
- Runner system ID and contacted_at.
- Queue per tag/pool.
- Warm/cold manager state.
- Remote node heartbeat, SSH latency, storage thresholds.
- GC actions and reclaimed bytes.

Actions:

- Scale pool preview.
- Pause/resume pool.
- Drain pool/manager.
- Rotate token.
- Restart manager.
- Open logs.
- Run node doctor.
- Trigger host/cache GC preview.

### 7.15 Jankurai Audit Center

**Purpose:** turn Jankurai into a first-class quality/security/provenance cockpit across repos.

Questions:

- Which repos have low scores or score caps?
- What specific controls/findings caused the cap?
- Which findings block merge/release?
- Which areas are auto-fixable or agent-ready?
- What is score history and trend?
- Are generated zones, security boundaries, proof lanes, ownership maps, and release controls healthy?

Proposed data model:

```rust
pub struct JankuraiRun {
    pub id: String,
    pub repo: RepoRef,
    pub sha: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub score: f64,
    pub score_cap: Option<f64>,
    pub cap_reason: Option<String>,
    pub findings: Vec<JankuraiFinding>,
    pub controls: Vec<JankuraiControlResult>,
    pub artifacts: Vec<EvidenceRef>,
}

pub struct JankuraiFinding {
    pub id: String,
    pub category: JankuraiCategory,
    pub severity: Severity,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub summary: String,
    pub proof: Vec<EvidenceRef>,
    pub auto_fixable: bool,
    pub blocks_merge: bool,
    pub blocks_release: bool,
}
```

Categories:

- duplication/rot
- generated-zone violations
- security boundary violations
- missing proof lanes
- missing ownership
- fragile release controls
- non-auditable scripts
- unsafe secrets handling
- dependency/toolchain drift
- test/CI anti-patterns
- docs/source drift
- policy bypass risk

Mock:

```text
╭─ Jankurai Audit Center ─ repos:417 avg:78.2 critical caps:6 runs live:3 ───────────────────────────────╮
│ Repo             Score Trend   Cap reason             Critical High AutoFix Agent-ready Release block  │
│ ▶ veox-enclave    82.4 ▁▂▄▇▆  security boundary       1        7    12      8           yes            │
│   redlinedb       76.1 ▁▁▃▃▂  duplicate query paths   0        9    21      14          no             │
│   jeryu           88.9 ▄▅▆▇█  docs/source drift       0        3    6       4           warn           │
├─ Findings: veox-enclave ──────────────────────────────────────────────────────────────────────────────┤
│ ‼ generated code edits outside generated zone  path:enclave/proto.rs  blocks release  proof:jank#771   │
│ ▲ missing ownership map for attestation boundary path:crates/enclave/* agent-ready bug#418             │
│ ▲ duplicate auth session renewal logic 4 clones  autofixable                                           │
╰─ Enter finding  b create bug  A assign agent  r rerun audit  p proof  x explain score cap ────────────╯
```

Actions:

- Run/rerun audit.
- Open finding proof.
- Create bugs from findings.
- Assign agent with scoped grant.
- Generate repair plan.
- Compare score history.
- Export report.

### 7.16 Code Change Volume / Risk

**Purpose:** show churn and change risk across repos/families and connect large changes to CI risk, VTI confidence, Jankurai findings, and review load.

Metrics:

- commits per window
- files changed
- lines added/deleted
- churn by subsystem
- generated vs handwritten changes
- test-to-code ratio
- risky file ownership/boundary changes
- dependency/toolchain changes
- reviewer load
- agent vs human changes
- reverted/flaky/failing change correlation

Mock:

```text
╭─ Code Churn / Risk ─ window:7d ─ commits:842 files:3910 +182k/-96k ─ risk:WARN ───────────────────────╮
│ Repo/family       Churn      Risk  Tests Δ  VTI conf  Jank Δ  Security  Review load  Notes             │
│ ▶ veox-*          +91k/-44k  high  +312     .89       -2.1    high:2    18 pending   auth boundary     │
│   redline-*       +41k/-30k  med   +87      .76       +1.2    high:0    6 pending    storage engine    │
│   jeryu           +22k/-9k   med   +44      .91       +0.8    high:0    4 pending    MR hook plumbing  │
╰─ Enter repo  o ownership  v VTI impact  j Jankurai  s security  r review queue ──────────────────────╯
```

### 7.17 Security / Secrets / Policy Center

**Purpose:** show security findings, secret/Vault lifecycle, policy violations, admission decisions, and release blockers without leaking secrets.

Security data sources:

- Jankurai findings.
- SAST/dependency/container scan artifacts.
- GitLab/GitHub security reports.
- Vault/secrets tables.
- Secret audit events.
- Admission decisions.
- Cache taints/verdicts.
- Signed artifacts/SBOM/provenance.
- Runner trust tier and image digest.
- Policy audit.

Mock:

```text
╭─ Security Center ─ critical:2 high:9 medium:31 ─ secrets:1 ─ vulnerable deps:6 ─ provenance gaps:3 ───╮
│ Finding                                      Repo        Severity Source       Blocks  Proof           │
│ ▶ secret pattern in web bundle               veox-web    critical scan+Jank    release scan#991        │
│   credentialed wildcard CORS                 veox-api    critical SAST         merge   sast#822        │
│   RUSTSEC advisory in cargo-deny             redlinedb   high     dep scan     warn    job#244         │
│   Vault rotation due                         jeryu       medium   secret audit warn    vault#12        │
│   runner image digest untrusted              infra       high     runner       merge   cache verdict   │
├─ Secrets / Vault ─────────────────────────────────────────────────────────────────────────────────────┤
│ authority: vault-main healthy initialized unsealed token:fingerprint:ab12 mount:kv prefix:jeryu/       │
│ release secret sets due: veox-deploy prod v2.7.4 expires 14h rotation recommended                      │
╰─ Enter finding  e evidence  b create bug  r rotate preview  p policy  x explain block ────────────────╯
```

Redaction rules:

- Never render secret values.
- Show token fingerprints, paths, mount, prefix, expiry, status, and audit metadata only.
- Copy action for secret path requires explicit confirmation and never copies value.
- Raw payload tabs must redact configured secret patterns.
- Screenshots/captures default to redacted mode.

### 7.18 Artifacts / Supply Chain

**Purpose:** track build outputs, signatures, SBOMs, provenance, release passports, and deployment lineage.

Artifact fields:

- artifact id/type/name
- repo/project/pipeline/job
- sha/ref/version
- digest/size/media type
- signature state
- SBOM state
- provenance/attestation
- source materials
- builder/runner trust
- scan results
- release/deployment relation
- retention/expiry
- download URL/path

Mock:

```text
╭─ Artifacts / Supply Chain ─ signed:284 unsigned:7 SBOM gaps:3 provenance gaps:5 ──────────────────────╮
│ Artifact                         Repo          Version  Digest       Sig  SBOM Prov Scan  Release      │
│ ▶ enclave-server.tar.zst          veox-enclave 2.7.4    sha256:9af.. ✓    ✓    ✓    warn  canary       │
│   web-bundle.tgz                  veox-web     2.7.4    sha256:31c.. ✗    ✓    gap  fail  blocked      │
│   redlinedb-cli                   redlinedb    0.9.2    sha256:77e.. ✓    gap  ✓    ok    n/a          │
├─ Provenance chain ────────────────────────────────────────────────────────────────────────────────────┤
│ source sha -> pipeline#581 -> job package#900 -> artifact digest -> signature -> release passport      │
╰─ Enter artifact  p provenance  s sign preview  b SBOM  d download/open  x explain gap ───────────────╯
```

### 7.19 Release / Rollback / Version Control

**Purpose:** show exact release state, canary, production, gates, evidence, rollback path, version control, and safe actions.

Release doctrine:

- Release is a proof graph, not a button.
- Every production action is tied to exact repo/ref/SHA/version/digest.
- Rollback plan must be visible before promotion.
- Canary telemetry and E2E gates are first-class.
- Human/agent approvals are durable evidence.

Mock:

```text
╭─ Release Control ─ current prod:2.7.3 ─ candidate:2.7.4 sha:9f31e4b ─ status:BLOCKED ─────────────────╮
│ Phase                 State   Evidence                         Age     Action                         │
│ source exact SHA       ✓      repo veox-deploy 9f31e4b          2m      open commit                    │
│ release pipeline       ✓      pipeline#581 success             1m      open pipeline                  │
│ artifact signatures    ▲      web-bundle unsigned              4m      sign / block                   │
│ canary deploy          ✓      canary URL healthy               3m      open canary                    │
│ canary E2E             ✗      e2e_auth_refresh failed          1m      open trace / retry             │
│ telemetry gate         ◌      waiting 5m min window            live    watch                         │
│ production promotion   ⏸      blocked by canary E2E + unsigned n/a     unavailable                   │
├─ Rollback plan ───────────────────────────────────────────────────────────────────────────────────────┤
│ target: prod 2.7.3 digest sha256:abc... | last known good: yes | rollback drill: passed 2d ago          │
╰─ e evidence  d doctor  p preflight  R rollback preview  A approve  W watch  x why blocked ───────────╯
```

Release actions:

- Preflight.
- Doctor/explain.
- Submit release.
- Approve gate.
- Promote prod.
- Rollback preview/execute.
- Watch/reconcile.
- Open release passport.

Production confirmation:

- Requires fresh release proof.
- Shows diff from prod version.
- Shows rollback path and expected blast radius.
- Requires typed phrase including version or SHA.
- Emits immutable action receipt.

### 7.20 Evidence Flight Recorder

**Purpose:** make all proof, audit, events, and receipts searchable/replayable.

The Evidence screen is the source of trust for every other screen.

Sources:

- `events`
- `evidence_capsules`
- `retry_decisions`
- `admission_decisions`
- `capability_intents`
- `capability_grants`
- `git_command_events`
- `git_ref_updates`
- `git_mirror_jobs`
- `git_risk_approvals`
- `secret_audit_events`
- `release_attempts`
- `cache_taints`
- `cache_verdicts`
- `cache_promotions`
- `test_plans`
- `selector_misses`
- `bug_events`
- `bug_attempts`
- `launch_ledger`
- `verdicts`
- `llm_budget_ledger`
- action previews/results

Mock:

```text
╭─ Evidence Flight Recorder ─ cursor:1849912 ─ filter:repo=veox-enclave sha=9f31e4b ────────────────────╮
│ Time       Kind                  Entity              Severity  Summary                      Proof       │
│ ▶ 12:41:12 vti.selector_miss     test auth_rotation  warn      selector missed auth test     vti#771     │
│   12:41:11 agent.patch_proposed  agent a17 / MR!843  info      patch pushed to agent/a17      grant#991   │
│   12:41:10 cache.verdict         cache target/...    warn      denied toolchain mismatch      cache#62    │
│   12:41:09 job.failed            job#882             crit      quote expiry assertion         cap#1042    │
│   12:40:02 admission.audit       ref main            warn      protected branch audited       adm#331     │
├─ Detail ───────────────────────────────────────────────────────────────────────────────────────────────┤
│ selected proof: cap#1042 | trace lines 1882-1904 | sha 9f31e4b | job#882 | bug link: #421             │
╰─ Enter proof  / filter  t time travel  r replay entity  y copy id  export  Esc back ──────────────────╯
```

Query API:

```http
GET /api/proof?entity=&kind=&since=&until=&actor=&severity=&repo=&sha=&branch=&cursor=&limit=
```

MCP resource:

```text
jeryu://proof?entity=job:882&cursor=1840000
```

Time-travel mode:

- Pick an event cursor/time and replay system state.
- Diff “then vs now” for a route.
- Export replay as bug/release proof.

### 7.21 LLM / Autonomy / Budget

**Purpose:** show LLM provider health, model usage, key pools, budgets, data-use policy, autonomy verdicts, kill bell, freeze windows, and evidence-gate status.

Mock:

```text
╭─ LLM / Autonomy ─ providers:7 healthy:5 degraded:1 down:1 ─ budget today:$18.42 / $50 ─ kill bell:armed ╮
│ Provider   State     p50 latency  errors  tokens today  cost   key source  data policy                 │
│ ▶ openrouter ok       1.2s         0.8%    1.8M          $8.44  jekko-3     scrubbed diffs              │
│   groq       ok       0.4s         1.1%    0.9M          $2.11  jekko-1     no secrets                  │
│   gemini     degraded 2.8s         7.2%    0.3M          $1.98  env         scrubbed                    │
│   ollama     local    0.9s         0.0%    n/a           $0     local       local only                  │
├─ Autonomy verdicts ────────────────────────────────────────────────────────────────────────────────────┤
│ MR!843 decision: pending CI | risk high | policy sha abc123 | reviewers 3/5 | superseded:no             │
│ freeze window: none | kill bell armed | foundry candidates: 2 | launch ledger latest: 12s               │
╰─ Enter provider  v verdict  k kill bell  f freeze  b budget  x explain routing ──────────────────────╯
```

Data:

- provider status/auth/rate/down
- model used
- token usage
- latency
- raw response hash
- user/key source
- cost estimate
- failure reason
- launch ledger
- verdicts
- kill bell state
- freeze windows
- foundry candidates
- PR drift

### 7.22 Settings / Source Doctor

**Purpose:** show runtime config, ports, feature flags, source freshness, generated docs/schema drift, action registry drift, and dependency health.

Mock:

```text
╭─ Source Doctor / Settings ─ profile:dev-sqlite-kafka ─ db:sqlite WAL ─ docs drift:WARN ───────────────╮
│ Component       State    Freshness  Version/Profile        Notes                                       │
│ GitLab REST     ok       0.8s       gitlab 17.9.2          latency 12ms                                │
│ DB              ok       live       sqlite jeryu.sqlite    WAL sync=NORMAL migrations ok              │
│ Docker          ok       1.1s       local+remote           4 unhealthy managers                        │
│ Broker          warn     12s        kafka                  consumer lag webhook.jobs 42                │
│ Cache gateway   ok       2.0s       :19800/:19801          summary auth token ok                       │
│ Vault           ok       4.0s       :18200                 initialized unsealed                        │
│ MCP             warn     n/a        HTTP :9778             resources missing, GET disabled             │
│ Action registry warn     build sha  mismatch               ListAllowedActions stale                    │
│ Docs/API        warn     build sha  stale                  generated docs differ from source           │
╰─ d diff docs  a action audit  h deep health  m migrations  r refresh  x explain drift ────────────────╯
```

Default ports/settings to display:

- GitLab HTTP/SSH: `8929` / `2224`
- Vault: `18200`
- webhook: `127.0.0.1:9777`
- MCP: `127.0.0.1:9778`
- cache proxy: `19800`
- OCI registry mirror: `19801`
- settings file: `~/.jeryu/settings.json`
- DB backend/path/URL
- feature profile
- TUI sync interval
- cache budget
- release repo/project defaults

---

## 8. Backend inspection plane

### 8.1 Architectural target

The dream TUI requires a single inspection plane. Screen code should not directly poll GitLab, Docker, DB, filesystem, Vault, cache, and MCP in scattered ways. Instead:

```text
Raw sources
  GitLab REST/webhooks, DB, Docker, Vault, cache proxy, broker, custom executor,
  Git hooks, autonomy server, Jankurai artifacts, LLM providers, filesystem
        │
        ▼
Collectors / normalizers
        │
        ▼
Unified Read Model + Event Stream + Entity Detail + Action Registry
        │
        ├── Rust TUI client
        ├── CLI read commands
        ├── MCP resources/tools
        ├── HTTP inspection API
        └── test/demo/replay harness
```

Golden rule:

> If two panes render the same fact, they must get it from the same typed model or entity store, not from two independent queries.

### 8.2 Minimum HTTP endpoints to add

```http
# Core read model
GET  /api/read-model
GET  /api/events?cursor=N&limit=500&kinds=&entity_kind=&entity_id=&severity=
GET  /api/entity/{kind}/{id}
GET  /api/proof?entity=&kind=&since=&until=&actor=&repo=&sha=&cursor=&limit=
GET  /api/runtime/profile
GET  /api/health/deep
GET  /metrics

# Domain dashboards
GET  /api/families
GET  /api/families/{family}/overview
GET  /api/repos
GET  /api/repos/{repo_slug}/overview
GET  /api/queue
GET  /api/workflows?scope=&repo=&family=&status=
GET  /api/pipelines/{project_id}/{pipeline_id}/graph
GET  /api/jobs/{project_id}/{job_id}/trace?offset=&limit=
GET  /api/runners/capacity
GET  /api/cache/dashboard
GET  /api/cache/object/{key}
GET  /api/vti/dashboard
GET  /api/vti/plans/{plan_id}
GET  /api/agents/dashboard
GET  /api/agents/{agent_id}
GET  /api/autonomy/dashboard
GET  /api/bugs/dashboard
GET  /api/bugs/{bug_id}
GET  /api/git-sync/dashboard
GET  /api/jankurai/dashboard
GET  /api/security/dashboard
GET  /api/artifacts/dashboard
GET  /api/release/dashboard
GET  /api/secrets/dashboard
GET  /api/llm/dashboard

# Actions
POST /api/action/preview
POST /api/action/execute
POST /api/action/cancel
GET  /api/action/{action_id}/status
```

### 8.3 Streaming endpoints

```http
GET /api/events/stream?cursor=N&filter=...                 # SSE event stream
GET /api/ws/events                                          # WebSocket event stream
GET /api/ws/entity/{kind}/{id}                              # entity-scoped updates
GET /api/ws/logs?project_id=&job_id=&offset=                # job trace chunks
GET /api/ws/action/{action_id}                              # action execution progress
GET /api/ws/release/{release_id}                            # release/canary gates
GET /api/ws/cache                                           # cache metrics/taints
GET /api/ws/runners                                         # Docker/runner/node events
```

Fallbacks:

1. WebSocket.
2. SSE.
3. Long-poll `GET /api/events`.
4. Direct domain polling.
5. Existing MCP tools/DB fallback.

The UI must visibly show which transport is active.

### 8.4 MCP resources to add

MCP tools remain for actions. MCP resources provide read-only browsing and watchability.

```text
jeryu://system/snapshot
jeryu://runtime/profile
jeryu://health/deep
jeryu://events?cursor=N
jeryu://proof?entity=&cursor=N
jeryu://families
jeryu://families/{family}
jeryu://repos
jeryu://repos/{slug}
jeryu://queue
jeryu://workflows?scope=
jeryu://pipelines/{project_id}/{pipeline_id}/graph
jeryu://jobs/{project_id}/{job_id}/trace
jeryu://runners/capacity
jeryu://cache/dashboard
jeryu://cache/object/{key}
jeryu://vti/dashboard
jeryu://agents/dashboard
jeryu://agents/{agent_id}
jeryu://autonomy/dashboard
jeryu://bugs/dashboard
jeryu://bugs/{bug_id}
jeryu://git-sync/dashboard
jeryu://jankurai/dashboard
jeryu://security/dashboard
jeryu://artifacts/dashboard
jeryu://release/latest
jeryu://secrets/dashboard
jeryu://llm/dashboard
jeryu://settings/effective
jeryu://action-registry
```

Watch semantics:

```text
jeryu.watch_events({ cursor, kinds?, entity_kind?, entity_id?, severity? })
resources/subscribe jeryu://events?cursor=N
resources/subscribe jeryu://jobs/{project_id}/{job_id}/trace
```

### 8.5 Unified read model

```rust
pub struct TuiReadModel {
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub event_cursor: u64,
    pub runtime: RuntimeProfile,
    pub freshness: Vec<SourceFreshness>,
    pub mission: MissionSnapshot,
    pub attention: Vec<AttentionItem>,
    pub next_action: Option<NextAction>,
    pub families: Vec<RepoFamilySummary>,
    pub repos: Vec<RepoSummary>,
    pub queue: CapacitySnapshot,
    pub workflows: WorkflowSummarySet,
    pub runners: RunnerFleetSummary,
    pub cache: CacheDashboardSummary,
    pub vti: VtiSummary,
    pub agents: AgentSummarySet,
    pub bugs: BugSummarySet,
    pub releases: ReleaseSummarySet,
    pub security: SecuritySummary,
    pub artifacts: ArtifactSummary,
    pub evidence: EvidenceSummary,
    pub llm: LlmSummary,
    pub source_doctor: SourceDoctorSummary,
}
```

### 8.6 Entity reference and detail

```rust
pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
    pub repo: Option<RepoRef>,
    pub project_id: Option<i64>,
    pub display: String,
}

pub enum EntityKind {
    Fleet,
    RepoFamily,
    Repo,
    Branch,
    MergeRequest,
    Pipeline,
    PipelineBridge,
    Job,
    TraceLine,
    RunnerPool,
    RunnerManager,
    RemoteNode,
    CacheCategory,
    CacheObject,
    CacheTaint,
    CacheVerdict,
    VtiPlan,
    TestCase,
    SelectorMiss,
    Agent,
    AgentSession,
    AgentTask,
    AgentGrant,
    Bug,
    BugAttempt,
    ReleaseAttempt,
    ReleaseGate,
    Artifact,
    SecretAuthority,
    SecretSet,
    SecurityFinding,
    JankuraiRun,
    JankuraiFinding,
    GitSyncEvent,
    AdmissionDecision,
    CapabilityIntent,
    CapabilityGrant,
    LlmProvider,
    AutonomyVerdict,
    EvidenceItem,
    Action,
    System,
}

pub struct EntityDetail {
    pub entity: EntityRef,
    pub state: EntityState,
    pub summary: String,
    pub freshness: SourceFreshnessSet,
    pub fields: Vec<FieldValue>,
    pub timeline: Vec<TuiEvent>,
    pub blockers: Vec<Blocker>,
    pub evidence: Vec<EvidenceRef>,
    pub related: Vec<Relation>,
    pub metrics: Vec<MetricSeries>,
    pub logs: Vec<LogRef>,
    pub actions: Vec<ActionDescriptor>,
    pub raw: Option<RedactedJson>,
}
```

### 8.7 Event model

```rust
pub struct TuiEvent {
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
    pub kind: TuiEventKind,
    pub severity: Severity,
    pub entity: Option<EntityRef>,
    pub parent: Option<EntityRef>,
    pub summary: String,
    pub correlation_id: Option<String>,
    pub request_id: Option<String>,
    pub actor: Option<String>,
    pub evidence: Vec<EvidenceRef>,
    pub next_actions: Vec<ActionDescriptor>,
    pub stale_after_ms: Option<u64>,
    pub payload: RedactedJson,
}
```

Required event families:

```text
system.health.updated
source.freshness.changed
repo.updated
repo.family.updated
mr.updated
mr.drift.detected
pipeline.created|updated|completed|failed|canceled
pipeline.graph.updated
job.created|queued|started|log.chunk|annotation|completed|failed|retried|canceled
failure_capsule.created
runner.pool.updated
runner.manager.started|stopped|oom|died|drained
remote_node.heartbeat|pressure|gc
cache.request|hit|miss|taint|verdict|promotion|gc_plan|gc_completed
vti.plan.created|validated|selector_miss|learned|fallback
agent.session.created|intent.started|intent.finished|patch.proposed|race.created|race.winner|grant.expiring
bug.created|updated|attempt.started|attempt.completed|linked
release.attempt.created|gate.updated|canary.started|promotion.ready|promoted|rollback.started|rollback.completed
secret.audit|rotation.due|rotation.completed|access.denied
security.finding.created|updated|resolved
artifact.created|signed|sbom.created|provenance.attested|scan.failed
jankurai.run.started|completed|finding.created|score.changed
admission.decision
capability.intent|grant.created|grant.revoked|grant.expired
autonomy.kill_bell.changed|freeze.changed|verdict.created|verdict.superseded
llm.call.started|completed|failed|budget.updated
action.previewed|started|progress|completed|failed|canceled
```

### 8.8 Source freshness

```rust
pub struct SourceFreshness {
    pub source: SourceKind,
    pub state: FreshnessState,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_event_cursor: Option<u64>,
    pub latency_ms: Option<u64>,
    pub stale_after_ms: u64,
    pub last_error: Option<String>,
    pub degraded: bool,
}

pub enum SourceKind {
    Db,
    GitLabRest,
    GitLabWebhooks,
    Broker,
    Docker,
    CacheGateway,
    Vault,
    Filesystem,
    Jankurai,
    Mcp,
    Capability,
    AutonomyServer,
    LlmProvider,
    SecurityScanner,
    GitHost,
}
```

Stale rendering rules:

- Header shows source summary.
- Pane title includes stale marker if source matters.
- Values dim but remain visible.
- Actions requiring fresh proof are disabled with reason.
- Attention item appears if source freshness affects safety.

---

## 9. Action and safety model

### 9.1 Action descriptor

```rust
pub struct ActionDescriptor {
    pub id: String,
    pub label: String,
    pub key_hint: Option<String>,
    pub risk: RiskTier,
    pub side_effect: SideEffectClass,
    pub supported_surfaces: Vec<ActionSurface>,
    pub dry_run: DryRunSupport,
    pub undo: UndoSupport,
    pub required_grant: Option<GrantRequirement>,
    pub required_freshness: Vec<SourceFreshnessRequirement>,
    pub description: String,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
}
```

Risk tiers:

| Tier | Examples | Confirmation |
|---|---|---|
| Read-only | open logs, fetch capsule, explain blocker | immediate. |
| Low | retry job, create local bug, pin watch, run doctor | preview optional/configurable. |
| Medium | run tests, propose patch, pause pool, cache GC dry-run, edit bug | preview required. |
| High | scale large pool, revoke grant, cleanup branches, cache purge, approve MR gate | explicit confirm + proof. |
| Production | merge, release promote, rollback, secret rotation/finalize, production config | typed phrase, fresh proof, receipt, possible approval. |
| Emergency | kill bell, production rollback in incident | typed phrase, two-step, strong receipt; may bypass some gates but records why. |

### 9.2 Action lifecycle

```text
Focused entity
  -> Action menu / command palette
  -> ActionPreview request
  -> Preview modal
  -> Optional dry run
  -> Confirmation
  -> Execute request with proof ack / idempotency key
  -> Streaming progress
  -> ActionResult receipt
  -> Evidence timeline event
```

Preview modal must include:

- exact target entity and IDs
- actor
- risk tier and side effects
- fresh/stale source checks
- grants required/present/missing
- policy blockers
- expected DB/GitLab/Docker/Vault/cache changes
- evidence consumed
- evidence to be created
- undo/rollback path
- dry-run result if available
- disabled reasons

Mock:

```text
╭─ Action Preview: Scale pool rust-fast +9 managers ─ Risk: MEDIUM ─────────────────────────────────────╮
│ Target: pool rust-fast | current 24/48 managers | theoretical slots +18 | estimated save 5m40 p50      │
│ Side effects: create runner managers, Docker containers, GitLab runner load                            │
│ Freshness: DB live, Docker 1.1s, GitLab 0.8s, Queue 1.0s                                                │
│ Cost: +$1.12/h estimated | Rollback: drain/remove new managers                                        │
│ Evidence consumed: queue#1849912, runners#884, bottleneck#77                                           │
│ Evidence created: action receipt, manager lifecycle events                                             │
│ Dry run: passed. No node disk pressure above 90%.                                                       │
╰─ Enter execute  d dry-run again  Esc cancel  ? policy ────────────────────────────────────────────────╯
```

### 9.3 Required action fixes before broad exposure

- Generate `ListAllowedActions` from the action registry or MCP manifest.
- Unit-test that all mutating actions are non-read-only.
- Make `request_merge` enforce documented evidence/risk gate on every path.
- Add runtime action registry hash and source SHA to Source Doctor.
- Fail closed if action side-effect metadata is missing.
- Persist MCP session and failed/invalid tool-call audit metadata.

---

## 10. Rust implementation architecture

### 10.1 Recommended stack

Use the existing Rust/Ratatui/crossterm direction. Ratatui provides terminal layout/widgets/rendering; crossterm provides cross-platform terminal event/raw-mode/alternate-screen/mouse support. Keep the stack boring where possible and invest complexity in data/model/render architecture.

Recommended crates/modules:

| Area | Recommendation |
|---|---|
| TUI rendering | `ratatui` |
| Terminal events/backend | `crossterm` |
| Async runtime | `tokio` |
| Serialization | `serde`, `serde_json` |
| HTTP/SSE/WebSocket | existing stack or `reqwest`/`tokio-tungstenite` depending repo norms |
| Errors | `thiserror`, `anyhow` or existing project error style |
| Time | `chrono` or existing time crate |
| Fuzzy search | `skim` matcher or lightweight in-house matcher |
| Tables/virtualization | custom model over Ratatui widgets |
| Testing | snapshot/golden tests + fake backend + event replay |

### 10.2 Module layout

```text
src/tui/
  mod.rs
  app.rs                    # App state, mode, route stack
  main_loop.rs              # terminal init, event loop, render loop
  input/
    mod.rs
    router.rs               # key/mouse routing
    keymap.rs               # key definitions/help
    command_palette.rs
  model/
    mod.rs
    entity.rs
    events.rs
    actions.rs
    freshness.rs
    routes.rs
    filters.rs
    search.rs
    theme.rs
  data/
    mod.rs
    client.rs               # InspectionClient trait
    http.rs                 # HTTP read/event/action client
    websocket.rs            # event/log streams
    sse.rs
    mcp.rs                  # MCP resource/tool fallback
    local_db.rs             # dev fallback, if allowed
    fixtures.rs
    replay.rs
  store/
    mod.rs
    entity_store.rs         # normalized entities
    event_store.rs          # ring buffer + cursor
    trace_store.rs          # bounded trace chunks
    table_store.rs          # virtualized row windows
    graph_store.rs          # DAGs and layouts
    selection_store.rs
    watch_store.rs
  pages/
    fleet.rs
    queue.rs
    repos.rs
    repo.rs
    workflow.rs
    trace.rs
    runners.rs
    cache.rs
    vti.rs
    agents.rs
    autonomy.rs
    bugs.rs
    git_sync.rs
    bottlenecks.rs
    jankurai.rs
    churn.rs
    security.rs
    artifacts.rs
    release.rs
    evidence.rs
    llm.rs
    source_doctor.rs
    settings.rs
  widgets/
    mod.rs
    status_header.rs
    tab_bar.rs
    breadcrumbs.rs
    attention_queue.rs
    entity_table.rs
    virtual_table.rs
    inspector.rs
    progress_bar.rs
    sparkline.rs
    heatmap.rs
    dag.rs
    graph_minimap.rs
    log_viewer.rs
    diff_viewer.rs
    proof_modal.rs
    action_modal.rs
    form_editor.rs
    timeline.rs
    capacity_meter.rs
    freshness_badge.rs
    event_ticker.rs
    watch_bar.rs
  theme/
    palette.rs
    symbols.rs
    accessibility.rs
  test_support/
    fake_backend.rs
    snapshots.rs
    event_replay.rs
    key_sequences.rs
```

### 10.3 App state

```rust
pub struct App {
    pub route: Route,
    pub nav_stack: Vec<Route>,
    pub focus: FocusPath,
    pub selected: Selection,
    pub mode: AppMode,
    pub filters: FilterState,
    pub lenses: SavedLensStore,
    pub watchlist: WatchStore,
    pub stores: Stores,
    pub data: Box<dyn InspectionClient>,
    pub keymap: KeyMap,
    pub theme: Theme,
    pub command_palette: CommandPaletteState,
    pub pending_action: Option<ActionFlow>,
    pub diagnostics: TuiDiagnostics,
    pub reduced_motion: bool,
}
```

### 10.4 Inspection client trait

```rust
#[async_trait]
pub trait InspectionClient: Send + Sync {
    async fn read_model(&self) -> Result<TuiReadModel>;
    async fn events_after(&self, cursor: u64, filter: EventFilter) -> Result<Vec<TuiEvent>>;
    async fn entity_detail(&self, entity: &EntityRef) -> Result<EntityDetail>;
    async fn workflow_graph(&self, project_id: i64, pipeline_id: i64) -> Result<WorkflowGraph>;
    async fn job_trace(&self, project_id: i64, job_id: i64, offset: u64) -> Result<TraceChunk>;
    async fn action_preview(&self, action: ActionRequest) -> Result<ActionPreview>;
    async fn action_execute(&self, action: ActionRequest, proof: ProofAck) -> Result<ActionResult>;

    fn subscribe_events(&self, filter: EventFilter) -> EventStream;
    fn subscribe_trace(&self, project_id: i64, job_id: i64, offset: u64) -> TraceStream;
}
```

Implementations:

- `HttpInspectionClient`
- `StreamingInspectionClient`
- `McpInspectionClient`
- `LocalDbInspectionClient` for dev fallback
- `FixtureInspectionClient`
- `ReplayInspectionClient`

### 10.5 Event loop

```text
terminal input ─┐
resize events ──┤
stream events ──┤       ┌───────────────┐       ┌──────────────┐
timers ─────────┼──────▶│ app reducer   │──────▶│ dirty render │
action results ─┤       └───────────────┘       └──────────────┘
trace chunks ───┘
```

Rules:

- Network never blocks render.
- All backend events become typed `AppEvent`s.
- Reducers update stores and mark dirty regions.
- Coalesce high-frequency logs/events into frame batches.
- Preserve focus and scroll position during updates.
- Apply backpressure: visible streams first, pinned streams second, offscreen summaries third.
- Drop old non-visible trace chunks before blocking input.
- Render on dirty state or heartbeat.
- Panic-free rendering: any widget failure degrades to an error pane.

### 10.6 Rendering performance targets

| Target | Requirement |
|---|---|
| Initial interactive paint | <500 ms with cached snapshot; <2s cold network. |
| Frame render | p95 <16 ms, p99 <33 ms for common screens. |
| Input latency | p95 <50 ms. |
| Event apply latency | p95 <100 ms from receipt to visible update. |
| Trace display latency | p95 <250 ms from backend chunk to screen. |
| Scale | 500 repos, 50k recent jobs, 10k events in memory window, 100 trace subscriptions with visible prioritized. |
| Memory | bounded event/trace stores; default target <250 MB. |
| Resize | no crash/flicker; recompute within one frame. |

### 10.7 Virtualization

Large data must be virtualized:

- Tables render visible window + overscan.
- Search indices update incrementally.
- Graphs layout cached by pipeline ID + edge hash + terminal size class.
- Trace buffers are chunked and indexed by byte offset/line number.
- Event store is ring-buffered with durable cursor anchor.
- Raw JSON is loaded lazily.

---

## 11. Algorithms and derived intelligence

### 11.1 Attention ranking

Attention items should be ranked by impact, urgency, confidence, scope, and actionability.

```text
attention_score = severity_weight
                * impact_scope_weight
                * urgency_weight
                * freshness_confidence
                * actionability_weight
                * not_suppressed_factor
```

Inputs:

- release/prod blockers
- failed critical path jobs
- queue bottlenecks
- VTI selector misses
- cache taints/miss storms
- security critical/high
- unsigned/provenance gaps
- agent blocked/grant expiring
- bug priority/severity
- source staleness affecting safety
- MR drift/review blockers

Every attention row shows:

```text
severity | entity | why | proof | recommended action | confidence | freshness
```

### 11.2 “Why not green?” explainer

Any scope can answer:

```text
Why is Fleet/Family/Repo/MR/Release not green?
```

Return a ranked explanation tree:

```text
Fleet not green because:
  1. Release candidate 2.7.4 blocked by canary E2E failure (proof rel#91/job#882)
  2. Queue limit-distance 1.34× due rust-fast tag bottleneck (proof queue#1849912)
  3. Security has 2 critical findings in veox-web/api (proof scan#991/sast#822)
  4. VTI selector miss in auth subsystem affects MR !843 (proof vti#771)
```

### 11.3 Repo family rollup

Family rollup should compute:

```text
family_status = max_severity(repo_statuses, release_status, security_status, queue_status)
family_attention = top N attention items scoped to family
family_capacity = aggregate queue/work/runners used by family jobs
family_release_graph = repos participating in release chain
family_cache = cache usage and miss ratio by family
family_vti = savings/miss risk by family
family_agent = active/blocked agents by family
```

### 11.4 Workflow DAG layout

Algorithm:

1. Build nodes from GitLab jobs + bridges + manual gates + release gates.
2. Add explicit `needs` edges where available.
3. Add bridge/downstream edges.
4. Add artifact edges where artifact dependency is known.
5. Add stage-barrier inferred edges only when explicit edges are missing; mark as inferred.
6. Compute critical path with durations/remaining estimates.
7. Assign columns by topological level/stage.
8. Assign rows minimizing edge crossings and grouping by stage/pool/status.
9. Cache layout key by node/edge hash + width class.
10. Preserve selected node across updates by entity ID.

### 11.5 Cache pressure score

```text
cache_pressure = weighted_mean([
  capacity_used_ratio,
  growth_rate_vs_budget,
  miss_rate_weighted_by_job_impact,
  taint_block_count,
  gc_reclaimable_ratio_inverse,
  hot_object_eviction_risk,
])
```

Render as:

- `OK` < 0.60
- `WARN` 0.60–0.80
- `HOT` 0.80–0.90
- `CRITICAL` > 0.90

### 11.6 VTI safety score

```text
vti_safety = confidence
           * (1 - weighted_selector_miss_rate)
           * forced_critical_tests_passed_factor
           * mapping_freshness_factor
           * exact_sha_receipt_factor
```

A release/merge gate may require different thresholds by risk tier.

### 11.7 Agent ROI score

```text
agent_roi = estimated_human_time_saved
          - operator_review_time
          - CI_minutes_extra_cost
          - LLM_cost_normalized
          - reverted_or_failed_attempt_penalty
```

Show ROI as a helpful estimate, not a hard truth. Mark as heuristic.

---

## 12. Backend plumbing backlog

### 12.1 P0: truth and contract hardening

- Generate API docs from Clap command tree, action registry, MCP tools, `AgentIntent`, DB schema metadata, and runtime profile.
- Fix action side-effect classifications.
- Generate `ListAllowedActions` from registry.
- Audit and gate `request_merge`.
- Show runtime build SHA/feature profile/action registry hash.
- Add deep health endpoint.

### 12.2 P0: unified read model and event stream

- Externalize `TuiReadModel`.
- Externalize `TuiEvent` with cursor.
- Add entity detail endpoint.
- Add action preview/execute endpoints.
- Add proof query.
- Add fake backend fixtures.

### 12.3 P0: realtime logs/events

- Add event WebSocket/SSE.
- Add job trace stream with offset resume.
- Add action progress stream.
- Add release/cache/runner streams or event kinds.
- Retain polling fallback.

### 12.4 P1: MR/PR state

- Parse Merge Request hooks into durable MR state.
- Persist approvals, reviewers, discussions, changed files, mergeability, labels, draft state, linked pipelines, head/base SHA, target policy SHA.
- Bridge GitHost PR state for GitHub/GitLab parity.

### 12.5 P1: pipeline graph and artifacts

- Build true pipeline DAG endpoint.
- Include child pipelines/bridges.
- Parse JUnit/xUnit, coverage, code-quality, SAST, dependency/container scans, benchmark JSON, release-gate JSON, nextest archives.
- Annotate jobs/traces with parsed artifacts.

### 12.6 P1: cache details

- Expand `/cache/summary` into dashboard/object/verdict/taint/GC endpoints.
- Add category attribution for Rust crates, Cargo git, target dirs, sccache, OCI layers, CAS/materials, artifacts.
- Add miss reason and hot object analytics.

### 12.7 P1: runner/node telemetry

- Plumb Docker stats and events.
- Plumb remote node heartbeat/SSH latency/storage thresholds.
- Add manager config hash/version/image digest.
- Add queue by tag/pool.

### 12.8 P1: agent lifecycle

- Add dedicated agent session/task/step/message/artifact/race tables.
- Persist grant expiry warnings.
- Add race status/winner/cleanup lifecycle tools.

### 12.9 P2: Jankurai/security/artifacts

- Normalize Jankurai runs/findings/score history.
- Normalize security scan reports.
- Normalize artifact signatures/SBOM/provenance.
- Connect these to merge/release proof.

### 12.10 P2: autonomy/LLM

- Bring autonomy kill bell, freeze, verdicts, launch ledger, foundry candidates, PR drift, and LLM provider health into main read model.
- Add budget/cost/resource attribution.

---

## 13. Testing strategy

### 13.1 Unit tests

- Key routing: every global key and context key.
- Reducers: all event kinds update stores correctly.
- Focus paths: pane movement and drill/up behavior.
- Filters/search parsing.
- Capacity formulas.
- Queue simulation edge cases.
- DAG layout stability.
- Freshness state transitions.
- Action risk/side-effect gating.

### 13.2 Golden render tests

Use deterministic fixtures and capture text buffers for:

- Fleet wide/medium/compact/tiny.
- Queue with high saturation, tag fragmentation, stale data.
- Repo family with many repos.
- Workflow DAG with success/running/failed/skipped/manual/child states.
- Trace viewer with annotations/redactions.
- Cache full/tainted/miss storm.
- VTI good/bad/fallback.
- Agents/race/grant expiring.
- Release blocked/rollback emergency.
- Security critical findings.
- Evidence replay.
- Source Doctor drift.

### 13.3 Interaction tests

Scripted key sequences:

```text
Fleet -> top blocker -> pipeline -> failed job -> trace -> capsule -> bug submit preview -> cancel
Fleet -> Queue -> pool -> scale preview -> dry run -> cancel
Repos -> veox-* -> veox-enclave -> Workflow -> critical path -> trace -> back to Fleet
Agents -> a17 -> grant -> revoke preview -> cancel
Release -> why blocked -> canary e2e trace -> rollback preview -> cancel
```

### 13.4 Fake backend and replay tests

- Fake backend emits event storms.
- Sources go stale/unavailable.
- GitLab returns errors/rate limits.
- Docker manager dies/OOM.
- Cache miss storm.
- VTI selector miss after green plan.
- Agent races with multiple branches.
- Release gate flips while user is in preview.
- Action preview becomes stale before execution.

### 13.5 Performance/load tests

- 500 repos.
- 50k jobs.
- 10k visible events.
- 100 trace streams with one visible.
- 1k events/sec burst for 10 seconds.
- Resizes while streaming.
- Slow backend source.
- High-latency SSH/remote nodes.

### 13.6 Safety tests

- Mutating action cannot execute without preview unless explicitly low-risk and configured.
- Production actions require typed confirmation and fresh proof.
- Stale MR state disables merge.
- Stale release gate disables promote.
- Secret values never render in normal/raw/screenshot/export.
- Read-only actions cannot mutate.
- Mutating MCP/capability actions are never classified read-only.
- Failed action emits receipt/error event.

---

## 14. Implementation phases

### Phase 0 — contract cleanup and fixtures

- Freeze entity/event/action schemas.
- Build fake backend fixtures covering every screen.
- Generate docs/action registry/runtime profile.
- Add Source Doctor MVP.
- Add snapshot/golden test harness.

### Phase 1 — TUI shell and navigation

- Global shell, header, tabs, breadcrumbs, inspector, event tail.
- Route stack and focus model.
- Command palette.
- Keymap/help overlays.
- Theme/density/responsive system.

### Phase 2 — unified read model client

- Implement `InspectionClient` trait.
- Consume `/api/read-model` and `/api/events` or local fallback.
- Entity store/event store/freshness model.
- Basic Fleet/Repos/Queue pages from model.

### Phase 3 — Workflow Atlas and traces

- Pipeline graph view.
- DAG layout and inspector.
- Trace viewer with annotations and follow mode.
- Failure capsule/evidence linking.
- Multi-pipeline support.

### Phase 4 — capacity/cache/VTI/runners

- Queue theoretical-limit model.
- Runner/system utilization.
- Cache MRI.
- VTI cockpit.
- Bottleneck Lab.

### Phase 5 — agents/bugs/git/autonomy

- Agent Tower.
- Bug board and detail.
- Git Sync.
- Autonomy governance/config editor.
- Race visualization.

### Phase 6 — release/security/artifacts/Jankurai/evidence

- Release/rollback control.
- Security/secrets center.
- Artifact/provenance center.
- Jankurai audit center.
- Evidence Flight Recorder and time-travel.

### Phase 7 — action execution and safety polish

- Full action preview/execute streams.
- Risk confirmations.
- Receipts and proof modals.
- Disabled-action explanations.
- Production-grade safeguards.

### Phase 8 — incredible polish

- Reduced-motion and accessibility.
- Saved lenses/watchlists.
- CI simulator/recommendations.
- Agent ROI dashboard.
- Time-to-green predictor.
- Headless capture/demo mode.
- Pair terminal mode.

---

## 15. Acceptance criteria

### 15.1 UX acceptance

- From Fleet, operator can reach any failed job trace in ≤4 keypresses.
- From Fleet, operator can answer “why not green?” in ≤2 keypresses.
- `Enter` drills and `Esc` goes up everywhere.
- Every pane has freshness/source state.
- Every warning has explanation and proof or says no proof.
- No blank screens during transient data failures.
- 80×24 remains usable.

### 15.2 Data acceptance

- Fleet shows all repo families and isolated repos.
- Queue shows theoretical, online, effective, busy slots and limit distance.
- Workflow DAG supports multiple pipelines, child pipelines, and explicit/inferred edge labels.
- Cache shows storage by category and trust/taint/verdicts.
- VTI shows selected/skipped/misses/savings/confidence.
- Agents show grant/task/branch/MR/CI/log/evidence state.
- Bugs show attempts/evidence/status across repos.
- Release shows exact SHA/version/gates/artifacts/rollback.
- Evidence is searchable by entity, actor, kind, SHA, time.

### 15.3 Performance acceptance

- p95 input latency <50 ms.
- p95 common render <16 ms.
- p95 event-to-visible <100 ms.
- Trace chunks visible <250 ms p95.
- Handles 500 repos, 50k recent jobs, 10k event window.
- No unbounded trace/event memory growth.

### 15.4 Safety acceptance

- Production merge/release/rollback cannot execute with stale required proof.
- Action metadata is generated/validated from one registry.
- Mutating actions cannot be classified read-only.
- Secret values never render/export.
- Every mutating action creates a receipt in Evidence.
- Agent grants are visible, revocable, expiry-aware.
- Kill bell/freeze state is always visible when autonomy is enabled.

---

## 16. Extra dream features worth building

### 16.1 Time-to-green predictor

Predict when each family/repo/MR/release will turn green, with confidence and top assumptions.

```text
veox-enclave likely green in 18m p50 / 31m p90
Assumptions: no new push, rust-fast scale unchanged, canary E2E passes on retry.
```

### 16.2 CI optimizer report

One-key report:

```text
You can save ~41h/week by:
1. scaling rust-fast warm managers from 24 to 33 during 10:00-18:00
2. pre-pulling sec-scan base images
3. splitting integ:test-nextest into 4 shards
4. repairing auth VTI selector misses
5. canceling superseded pipelines after push
```

### 16.3 Agent race arena

Visual comparison of multiple agent patches:

```text
hypothesis A: smaller patch, CI green, Jankurai +0.4, risk low
hypothesis B: bigger refactor, CI failed, Jankurai +1.8, risk medium
hypothesis C: minimal config, CI green, tests weaker, risk high
```

### 16.4 Trust replay

Pick a release and replay every proof from commit to production:

```text
commit -> VTI plan -> CI -> artifacts -> signatures -> canary -> telemetry -> approval -> promotion
```

### 16.5 Flake command center

Rank flaky tests by cost, severity, owner, recent failures, and agent-ready fixes.

### 16.6 Review queue / ownership router

Show who should review what based on ownership maps, changed files, risk, agent work, and review load.

### 16.7 Cost lens

CI minutes, runner cost, cache storage, LLM spend, and wasted work by family/repo/agent.

### 16.8 Dependency/toolchain drift

Rust toolchains, Cargo.lock changes, advisories, image base drift, generated code drift.

### 16.9 Pair terminal mode

Read-only spectator mode with shared route/cursor for pairing, incident rooms, and demos.

### 16.10 Natural-language explain without hidden magic

Local “ask this screen” powered by structured facts. It must cite exact entities/evidence and never invent state.

---

## 17. Build checklist for implementation agents

1. Define stable `EntityKind`, `TuiEventKind`, `Route`, `ActionDescriptor`, and `SourceFreshness` schemas.
2. Build fake backend fixtures before UI polish.
3. Implement shell, route stack, focus model, inspector invariant.
4. Implement keyboard model exactly: arrows move, `Enter` drill, `Esc` up, `Tab` focus.
5. Build Fleet, Queue, Repos first.
6. Add Workflow DAG and Trace viewer next.
7. Add Cache, VTI, Runners with derived metrics.
8. Add Agents, Bugs, Git Sync, Autonomy.
9. Add Release, Security, Artifacts, Jankurai, Evidence.
10. Add action preview/execution last, after metadata safety tests pass.
11. Add streaming transports and fallback logic.
12. Add golden screenshots for every screen and density.
13. Add performance tests with large fixture data.
14. Add safety tests for stale proof, secrets, and mutating actions.
15. Polish motion, colors, sparklines, and help overlays.

---

## 18. Final target experience

The final JeRyu Flight Deck should feel like this:

You open `jeryu tui` and immediately see the entire engineering fleet moving. Repo families breathe with live work. The queue meter tells you whether the system is near its theoretical limit or wasting time. A red release blocker points to a canary E2E failure. You press `Enter`, land in the release proof, press `l`, jump to the failing job trace, press `c`, create a failure capsule, press `b`, create or link a bug, press `A`, assign a bounded agent, then `Esc` back up to Fleet. While that happens, the Agent Tower shows the agent’s branch, grant, CI pipeline, logs, and cost. The Cache MRI shows that Rust target cache pressure is high and causing misses. The Queue screen proves scaling one pool saves five minutes but does not fix the canary gate. The Evidence Flight Recorder can replay every fact you saw.

No page lies. No action is hidden. No green lacks proof. No warning lacks a reason. No repo is invisible. No agent is unbounded. No release happens without a passport. The terminal feels alive because the system is alive, but it remains trustworthy because every motion is anchored to durable events.

That is the developer’s dream Rust TUI.
