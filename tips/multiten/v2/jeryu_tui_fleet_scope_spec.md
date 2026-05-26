# JeRyu TUI Fleet Scope & Utilization Cockpit Spec

## Purpose

JeRyu is moving from a small set of horizontally listed repositories to a large mixed fleet. The TUI should make the default view a true fleet-wide command center, while letting an operator drill into repo families and individual repos without losing keyboard speed or context.

The design below keeps the current `All` default, adds automatic dash-prefix family grouping such as `veox-*`, and makes every major pane obey the active scope: `All`, `Family`, or `Repo`.

## Operator goals

1. **Start in All** and immediately see global health across workflows, mission, release, approvals, jobs, agents, tests, pools, cache, evidence, bugs, secrets, LLMs, git, and jankurai.
2. **Find hot work fast**: repos/families and jobs should be ranked by most recent activity, with active/running work winning ties.
3. **Drill down quickly**: Enter on `veox-*` opens that family; Enter on a repo opens repo detail; Esc backs out one level and eventually returns to All.
4. **Watch utilization**: see whether runners are saturated, idle, blocked, or queueing work across the machine.
5. **Avoid data bleed**: cache, artifacts, logs, evidence, secrets, approvals, and release state must remain isolated unless a project explicitly opts into a shared namespace.

## UX model

### Scope hierarchy

The fleet scope bar becomes a two-level navigator:

```text
All  veox-*  dougx-*  jekko  infra  docs
```

Selecting `veox-*` and pressing Enter drills into:

```text
← veox-* all  veox-shared  veox-deploy  veox-nht  veox-proofs
```

Rules:

- A repo name with a dash participates in a family using the prefix before the first dash.
- A family appears only when at least two repos share that prefix.
- Repos without a multi-repo family remain as direct root chips.
- `All` always means all tracked repositories.
- `veox-* all` means only repos in the `veox-` family.
- A single repo scope means only that repo.

### Keyboard interaction

| Context | Key | Behavior |
|---|---:|---|
| Any pane | focus FleetBar then Enter | open scope picker / drill current scope |
| Root fleet bar | Left/Right or h/l | move between All, families, and singleton repos |
| Root fleet bar | Enter on family | drill into family |
| Root fleet bar | Enter on repo | open repo detail |
| Family scope | Left/Right or h/l | move between family-all and child repos |
| Family scope | Esc | close detail first, then return to root scope list |
| Any selected scope | Esc | return to All when already at root |

### Scope ranking

Scopes should be sorted so the hottest work is closest to the left:

1. running or queued jobs/workflows
2. failures or blocks
3. most recently updated repo/workflow/job
4. stable alphabetical fallback

The visual ranking should be stable within a render tick to avoid jitter.

### Header

The header scope badge should become explicit:

```text
repo:All(17)
repo:veox-*(4)
repo:veox-shared
```

When the active scope is not All, the badge should use a high-contrast background so the operator always knows they are filtered.

### Main panes

Every pane should render through a shared `RepoFilter`:

```rust
RepoFilter::All
RepoFilter::Family { prefix: "veox" }
RepoFilter::Only { alias: "veox-shared", slug: "neverhuman/veox-shared" }
```

Required scope behavior:

- **Workflow**: PR/MR rail and DAG should show only active PRs in the scope.
- **Mission**: mission counts and actions should aggregate only scoped work.
- **Release**: release stage columns should show scoped release attempts; `All` shows fleet release rollup.
- **Approvals**: human approval queue should filter by scoped repo/family.
- **Jobs**: runner feed, job matrix, pipeline progress, and inspector should show scoped jobs.
- **Agents**: sessions and pipelines should show scoped agents.
- **Tests**: bottlenecks and latest failures should filter by repo/family.
- **Pools**: pool view should show all shared machine capacity, plus scoped job pressure overlays.
- **Cache**: show scope-specific cache namespace and never mix per-repo cache metrics unless explicitly shared.
- **Evidence**: evidence records must filter by scoped repo/family and should not leak unrelated repo logs into repo view.
- **Bugs/Git/Secrets/LLMs/Jankurai**: all rows should carry repo identity and obey scope.

## Data model changes

### Repo scopes

Add derived fleet scope objects from `FleetSnapshot`:

```rust
FleetScopeKind::All
FleetScopeKind::Family { prefix, repo_indices }
FleetScopeKind::Repo { index }
```

A `FleetScope` carries label, status counts, stale count, repo count, latest observed timestamp, and sort score. It is not persisted; it is derived from the registry snapshot.

### RepoFilter

Extend the existing filter model from `All` and `Only` to `All`, `Family`, and `Only`. `Family` should match both aliases and slug basenames:

- alias `veox-shared` matches `veox`
- slug `neverhuman/veox-shared` matches `veox`

### Repo identity on events

Today many views have project/job IDs but not a first-class repo identity. Add one or more of these fields to the relevant state projections:

```rust
repo_alias: Option<String>
repo_slug: Option<String>
project_key: Option<String>
project_id: Option<i64>
```

Minimum viable mapping:

1. Extend registry parsing to accept optional `project_id` or `project_key`.
2. Build an in-memory `project_id -> repo` lookup from registry + tracked repositories.
3. When hydrating recent jobs, CI runs, evidence, approvals, and release stages, attach repo identity before storing in `TuiStateSnapshot`.

## Cache and data isolation

Scope filtering is only a UI boundary. Cache/data isolation must be explicit at the backend layer.

Recommended policy:

```toml
[[repo]]
alias = "veox-shared"
slug = "neverhuman/veox-shared"
cache_namespace = "repo:neverhuman/veox-shared"
artifact_namespace = "repo:neverhuman/veox-shared"
secret_namespace = "repo:neverhuman/veox-shared"
allow_family_cache = false

[[cache_family]]
prefix = "veox"
namespace = "family:veox"
members = ["veox-shared", "veox-nht", "veox-deploy"]
allow_cross_repo_reuse = true
```

Defaults:

- repo-level cache namespace by slug
- no cross-repo cache sharing by default
- family cache requires explicit opt-in
- secrets never share across repos by family prefix
- evidence/log/artifact records remain repo-scoped even when displayed in family or All

The TUI should surface this in the Cache tab:

```text
scope: veox-*   cache: family:veox (opt-in)   secrets: per-repo   evidence: per-repo
```

## Runner utilization design

Add a compact utilization strip to the Jobs and Pools tabs:

```text
runners 9/10 busy · queued 4 · waiting_resource 2 · idle 1 · cache hit 78% · hot: veox-* / deploy
```

Key metrics:

- busy runner count
- configured capacity
- queue depth
- waiting-for-resource count
- idle but warm runners
- average queue duration
- longest queued job
- cache hit ratio by active scope
- failed/rerun count by active scope

The Jobs tab should default to a **hot job table**, sorted by:

1. currently running / waiting / pending
2. newest `received_at` or `observed_at`
3. longest queue duration
4. failed jobs needing attention

Suggested columns:

```text
scope       age     status    queue   runner/pool     pipeline   job
veox-*      00:13   running   04.2s   pool-a          123456     cargo test
veox-nht    00:09   pending   18.1s   -               123455     build image
```

## Implementation plan

### Phase 1: Fleet scope UX

- Add derived `FleetScope` objects in `repo_fleet.rs`.
- Extend `RepoFilter` with `Family`.
- Replace the one-row repo list with a scope-aware grouped bar.
- Add family drilldown and scope-aware selection methods on `App`.
- Update header scope badge.
- Keep Workflow PR filtering through the existing `RepoFilter` path.

### Phase 2: Scope every data plane

- Attach repo identity to job, release, evidence, approval, agent, test, bug, git, secret, and LLM projections.
- Add shared helper methods such as `visible_recent_jobs()` and `visible_evidence()` to avoid ad hoc filtering in renderers.
- Add tests proving unknown repo rows show only in All.

### Phase 3: Utilization cockpit

- Add `RunnerUtilizationSnapshot` to `TuiStateSnapshot`.
- Render the utilization strip in Jobs and Pools.
- Add a ranked hot job table and top-blocker view.
- Add a scope summary mini-chart in All and family scopes.

### Phase 4: Cache namespace visibility

- Parse cache namespace settings from registry.
- Show namespace mode in Cache tab and repo detail overlay.
- Add warnings when family view includes repos with different cache namespaces.

## Acceptance criteria

- TUI opens in All by default.
- `veox-shared`, `veox-deploy`, and `veox-nht` appear as `veox-*` at root when at least two are tracked.
- Enter on `veox-*` drills into the family.
- Esc from family returns to root; Esc from root returns to All.
- Workflow PR rail and hit testing obey All/Family/Repo filters.
- Recent active jobs are ranked active-first and newest-first.
- Unknown/unscoped rows appear only under All.
- Cache tab clearly displays whether data is per-repo or explicitly family-shared.
- Screenshot tests cover root scope bar, family drilldown, and filtered Workflow rail.

## Suggested proof commands

```bash
cargo test -p jeryu --lib repo_fleet
cargo test -p jeryu --lib tui::repo_fleet_bar
cargo nextest run -p jeryu -- tui::workflow::pr_rail
cargo nextest run -p jeryu -- tui
cargo run -p jeryu -- tui --screenshot --tab workflow --output target/tui-fleet-scope.png
```
