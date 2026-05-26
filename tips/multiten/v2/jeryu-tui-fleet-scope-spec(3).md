# JeRyu TUI Fleet Scope Navigator — Engineering Spec

## Summary

JeRyu’s TUI should shift from a horizontal repo strip into a hierarchical fleet navigator that scales to dozens or hundreds of mixed repositories. The operator’s default scope remains **ALL**, and every tab should respect the current fleet scope:

```text
ALL
 ├─ veox-*          # auto-grouped family from veox-shared, veox-deploy, veox-nht, ...
 │   ├─ veox-shared
 │   ├─ veox-deploy
 │   └─ veox-nht
 ├─ jeryu
 └─ other ungrouped repos
```

The goal is to make the TUI feel like a command center for runner utilization: first show the whole machine/fleet, then let an operator quickly drill into a project family or an individual repo without losing context.

## Current State Observed

The current TUI already has the right primitives, but the fleet navigation is too flat:

- `TuiStateSnapshot` includes `fleet: FleetSnapshot`, `recent_jobs`, `pipelines`, `runner_feeds`, and per-tab state.
- `FleetSnapshot` currently exposes `repos`, `events`, `counts()`, and `selected(index)` where `index == 0` means ALL.
- `RepoFilter` currently supports only `All` and `Only { alias, slug }`.
- `repo_fleet_bar.rs` renders a single-line left-to-right rail: `All` followed by every repo chip.
- `App::repo_select_next/prev/all()` currently moves through `selected_repo_index` directly against `fleet.repos`.
- `general.rs` uses Enter on the focused fleet bar to open the detail overlay, and Left/Right or h/l to cycle repos.

That design works for a few repositories, but degrades quickly with many repos and related repo families.

## Product Requirements

### R1 — Default is ALL

At startup, the selected scope must be ALL. In ALL scope, every tab renders all known data unless the tab cannot associate a datum to a repo.

### R2 — Every tab respects scope

The active fleet scope is global chrome, not a Jobs-tab-only control. Workflow, Mission, Release, Approvals, Jobs, Agents, Tests, Pools, Cache, Evidence, Bugs, Secrets, LLMs, Git, and Jankurai should all receive the same `RepoFilter`.

Minimum behavior:

- `All`: show all repo-associated and global data.
- `Family { family: "veox" }`: show only repos whose alias or slug tail belongs to `veox-*`.
- `Only { alias, slug }`: show only that repo.
- Unattributed global data should appear in ALL, but should not leak into repo-specific scopes unless it is explicitly marked as belonging to that scope.

### R3 — Auto group dashed repo families

Any repo whose alias or slug tail has a dash should be assigned a family key from the prefix before the first dash:

| Alias / slug tail | Family |
|---|---|
| `veox-shared` | `veox-*` |
| `veox-deploy` | `veox-*` |
| `veox-nht` | `veox-*` |
| `jeryu` | ungrouped repo |

Use alias first. If alias does not contain a dash, fall back to the slug tail. This supports aliases like `shared` with slug `neverhuman/veox-shared`.

### R4 — Drill-down navigation

Keyboard behavior:

- `Tab` / `Shift+Tab`: still changes tabs.
- Arrow keys at root focus: move panes as today.
- Focus Fleet Scope Navigator, then:
  - `←` / `h`: previous visible scope item.
  - `→` / `l`: next visible scope item.
  - `Enter` on a family: drill into that family.
  - `Enter` on a repo: open repo detail overlay.
  - `Enter` on ALL: open ALL detail overlay.
  - `Esc` inside a family: go back to root ALL/family list.
  - `Esc` at root: reset to ALL and close overlay.

Mouse behavior should follow the same hit model later: click a scope chip selects it; double-click or Enter drills/opens.

### R5 — Rank by active/recent work

Within a scope, jobs and repos should sort operator-first:

1. Running jobs / repos with running jobs.
2. Failed jobs / repos with failed jobs.
3. Waiting/pending jobs.
4. Recently updated workflow run / event.
5. Stale / inactive repos.
6. Alphabetical fallback.

This should apply to:

- root family chips,
- child repo chips inside a family,
- job/feed rows in Jobs tab,
- workflow PR/job rails where timestamps are available.

### R6 — Runner utilization visibility

The Jobs tab should make runner utilization obvious at every scope:

- Count active runners/jobs in the current scope.
- Show queued/pending/running/failing jobs ranked by recency.
- Keep live feed cycling by default, but allow pinning.
- In ALL scope, show cross-repo saturation.
- In family scope, show family-local saturation.
- In repo scope, show only that repo’s pipeline/jobs/log data.

### R7 — Cache/data isolation is explicit

The TUI must not imply cache sharing between projects. The UI should label each scope with an isolation hint:

```text
scope: veox-* · cache/data isolated per repo namespace
```

The TUI patch does not change cache/storage behavior directly. It adds the UX and filter model needed to surface isolation. A follow-up backend patch should ensure every runner/cache datum carries a `cache_namespace` or equivalent derived from repo identity, not just job/pool identity.

Recommended namespace:

```text
cache_namespace = sha256(provider + ":" + slug + ":" + default_branch_or_scope)
```

Never derive cache namespace from family alone. A family is a viewing scope, not a cache-sharing scope.

## Proposed UX

### Normal top chrome

```text
jeryu  scope:ALL(14)  GitLab:OK  ctrs:10 pools:3/3 rel:3eb58fcc agents:0 cache:0% ●
[0:Workflow] [1:Mission] ...
ALL r12 f1 aged0  veox-* 4 repos r9 f0  infra-* 3 repos r2 f1  jeryu local r0 f0
Enter family→drill · Enter repo→details · Esc→ALL · ←/→ select · / search
```

### Drilled family scope

```text
scope:veox-*  4 repos  r9 f0 aged0  cache/data isolated per repo
[.. ALL]  veox-shared running r3 f0  veox-deploy waiting r2 f0  veox-nht green r0 f0
Enter repo→details · Esc→families · ←/→ select
```

### Repo scope

```text
scope:veox-shared  running r3 f0 score:97  cache/data isolated
```

## Data Model Changes

### `RepoFilter`

Extend from:

```rust
pub enum RepoFilter<'a> {
    All,
    Only { alias: &'a str, slug: &'a str },
}
```

to:

```rust
pub enum RepoFilter<'a> {
    All,
    Family { family: &'a str },
    Only { alias: &'a str, slug: &'a str },
}
```

`matches()` should accept alias/slug metadata and include a datum when:

- `All`: always true.
- `Family`: datum alias or slug tail belongs to that family.
- `Only`: datum alias or slug equals the selected repo.

### `FleetScopeItem`

Add a view-layer scope item:

```rust
pub enum FleetScopeItem<'a> {
    All { metrics: FleetScopeMetrics },
    Family { family: &'a str, metrics: FleetScopeMetrics },
    Repo(&'a FleetRepoSnapshot),
}
```

### `FleetSnapshot` helpers

Add pure helpers:

- `family_key_for_repo(repo) -> Option<String>`
- `family_groups() -> Vec<FleetFamily>`
- `root_scope_items() -> Vec<FleetScopeItem>`
- `family_scope_items(family) -> Vec<FleetScopeItem>`
- `scope_items(current_family) -> Vec<FleetScopeItem>`
- `metrics_for_repos()`

These should be deterministic and unit-tested.

## App State Changes

Add:

```rust
pub selected_repo_family: Option<String>;
```

Keep `selected_repo_index` but reinterpret it as the index into the current visible scope items, not always `fleet.repos + 1`.

New methods:

- `repo_scope_items()`
- `repo_scope_item()`
- `repo_scope_enter()`
- `repo_scope_up()`
- `repo_scope_depth_label()`
- update `repo_select_next/prev/all()` to operate over current visible scope items.

## Rendering Changes

### `ui.rs`

Increase the fleet bar area from 1 line to 3 lines in normal and fullscreen layouts. The extra two lines buy a huge amount of navigation clarity without changing tab content structure.

### `repo_fleet_bar.rs`

Replace single-line repo rail with a scope navigator:

- Row 1: root scope list (`ALL`, families, ungrouped repos).
- Row 2: current family children or navigation hints.
- Row 3 (if available): isolation/status hint.

### `ui_chrome.rs`

Top chrome should show:

- `scope:ALL(n)`
- `scope:veox-*`
- `scope:veox-shared`

not only `repo:All` or `repo:<alias>`.

## Filtering Follow-ups

This patch updates the filter model and fleet navigator. Some panes already call `app.repo_filter()`, especially the Workflow delivery renderer. Any pane that still reads raw global vectors should be upgraded incrementally:

- Jobs: filter `runner_feeds`, `recent_jobs`, `pipeline_progress_view` where repo metadata exists.
- Release: filter release stages by repo/project when release cards gain repo identity.
- Approvals: ensure pending approvals carry repo slug.
- Evidence/Git/Secrets/Bugs: use existing target repo/project fields where present.

Where data models only have numeric project IDs (for example `JobEvent.project_id`) and no repo slug, add a resolver map from tracked repositories to project IDs, or persist repo alias/slug at ingestion time.

## Acceptance Tests

1. **Default scope**
   - New `App` starts with `selected_repo_family == None` and `selected_repo_index == 0`.
   - `repo_filter()` returns `RepoFilter::All`.

2. **Family grouping**
   - `veox-shared`, `veox-deploy`, `veox-nht` appear under one `veox-*` family.
   - Alias fallback works: alias `shared`, slug tail `veox-shared` still groups into `veox-*`.

3. **Family drill-down**
   - Select `veox-*`, press Enter, visible scope becomes `[.. ALL]`, `veox-shared`, `veox-deploy`, `veox-nht`.
   - `repo_filter()` returns `RepoFilter::Family { family: "veox" }` while family summary is selected.

4. **Repo detail**
   - Select `veox-shared`, press Enter, detail overlay opens.
   - `repo_filter()` returns `RepoFilter::Only { alias, slug }`.

5. **Esc behavior**
   - Esc from repo detail closes detail.
   - Esc inside family returns to root scope list.
   - Esc at root resets to ALL.

6. **Sorting**
   - A repo/family with running work sorts before idle repos.
   - A failed repo sorts before green idle repos.
   - RFC3339 `latest_run.updated_at` breaks ties by recency.

7. **Isolation label**
   - Family and repo scopes render `cache/data isolated per repo`.

## Rollout Plan

1. Land scope model + rendering behind existing TUI code paths.
2. Ensure Workflow continues to use `app.repo_filter()`.
3. Add filtering to Jobs tab next, because it is the most operator-critical tab for runner utilization.
4. Add repo metadata to any event/job records that cannot currently be scoped without guessing.
5. Add mouse hit maps for scope chips.
6. Add a `/` quick filter for typing repo/family names once fleet sizes exceed 25 repos.

## Non-goals

- Do not merge or share caches for repos in the same family.
- Do not change runner scheduling policy in the TUI patch.
- Do not hide global/unattributed warnings in ALL.
- Do not make family names configurable until the auto-prefix behavior has shipped and stabilized.
