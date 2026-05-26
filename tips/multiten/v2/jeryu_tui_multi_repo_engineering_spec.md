# Engineering spec: multi-repo Jeryu TUI scope and utilization cockpit

## Summary

Jeryu’s TUI should become a fleet cockpit: default to **All**, make it fast to narrow to a repo family such as `veox-*`, and make every pane honor that same scope. The key UX change is replacing the horizontal list of every repo with a compact scope rail: `All`, auto-discovered families, and any ungrouped repos.

This spec pairs with `jeryu_multi_repo_tui_scope.diff`.

## Current state studied

The repo already has the right seams:

- `TuiStateSnapshot` contains `fleet`, `recent_jobs`, `runner_feeds`, `pipeline_progress_view`, and delivery/workflow state.
- `App` already has `selected_repo_index`, `repo_detail_open`, `selected_repo()`, and `repo_filter()`.
- `FleetSnapshot` already centralizes repo health and counts.
- Workflow PRs already carry optional `repo_alias` and `repo_slug`, and the PR rail already accepts a repo filter.
- The one-line `repo_fleet_bar` is the current bottleneck because it renders every repo left-to-right.

## Goals

1. **Default All cockpit**: startup view shows all workflows, mission, release, approvals, jobs, agents, tests, pools, cache, evidence, bugs, secrets, LLMs, Git, and Jankurai.
2. **Fast family drill-down**: repos with dashed names auto-group into `<prefix>-*`, using slug basename before alias fallback.
3. **Repo-only drill-down**: individual ungrouped repos remain selectable; family overlay lists child repos.
4. **Recent-first work views**: jobs and runner feeds should favor the newest active work.
5. **Utilization-first surface**: the Jobs/Mission views should show active runners, queued jobs, failures, and idle capacity within the active scope.
6. **Isolation by default**: family grouping is display-only. Cache, evidence, logs, and runner data stay separated per project/repo unless explicitly configured otherwise.

## Non-goals

- Do not merge cache namespaces just because repos share a family prefix.
- Do not replace the existing Workflow canvas or Mission Control action pane.
- Do not require every historical record to gain repo metadata in the first patch; unscoped records remain visible only in `All`.

## UX design

### Scope rail

The row under the header becomes a scope selector:

```text
All run:9 fail:1 aged:0   veox-* repos:12 run:7 fail:1   redlinedb green r0 f0   jekko running r2 f0
```

Selection semantics:

- `All`: every data item is visible.
- `veox-*`: only repos whose slug basename or alias starts with `veox-` are visible.
- `redlinedb`: only that repo is visible.

### Navigation

| Key | Behavior |
| --- | --- |
| `Esc` | Reset scope to `All`; close overlays |
| focus FleetBar + `h/l` or left/right | Move between scope items |
| focus FleetBar + `Enter` on family | Open family detail overlay |
| focus FleetBar + `Enter` on repo | Open repo detail overlay |
| future `/` | Search repo/family scopes |

### Family overlay

The family overlay should show:

- family label and repo count;
- child repos sorted by running/failed/recent;
- each repo’s status, running count, failed count, latest run time, local dirty state, and score badge;
- clear reminder: “Family scope is active. Esc returns to All.”

### Jobs tab

Within active scope:

- left pane: runner feeds matching repo/family;
- top-right: pipeline progress if scoped pipeline exists;
- middle-right: job matrix from `scoped_recent_jobs()`, sorted `received_at DESC`;
- bottom-right: selected job inspector.

### Mission tab

Mission metrics should be scope-aware:

- Active Work: scoped jobs only;
- Live Runners: scoped feeds only;
- Top Signal: scoped failures first, then global infrastructure warnings.

Infrastructure metrics such as GitLab readiness, pool count, cache daemon health, and disk pressure remain global but should be labeled global.

## Data model

### Fleet registry

Add optional `project_id` to repo config:

```toml
[[repo]]
alias = "shared"
slug = "neverhuman/veox-shared"
project_id = 48
```

The parser should accept numeric TOML values and numeric strings. Non-numeric legacy strings are ignored rather than failing registry load.

### Filter model

`RepoFilter` becomes owned and supports:

```rust
RepoFilter::All
RepoFilter::Family { label: "veox-*", prefix: "veox" }
RepoFilter::Only { alias, slug }
```

A family match is based on repo slug basename first, alias second:

```text
neverhuman/veox-shared -> veox
shared                  -> no family unless slug is missing
```

### Unknown repo metadata

If a record has no repo alias, slug, or project id:

- show in `All`;
- hide in family/repo scopes;
- add instrumentation to identify which producer needs repo metadata.

## Cache and data isolation

Family grouping is visual only. Storage must remain repo/project scoped.

Recommended namespace key:

```text
scope_key = provider + ":" + slug-or-project-id
```

Apply to:

- runner working directories;
- cache roots;
- CAS metadata;
- taint scope;
- evidence capsules;
- live log capture;
- artifact downloads.

Example:

```text
cache/github:neverhuman/veox-shared/...
cache/github:neverhuman/veox-nht/...
evidence/gitlab:48/...
```

Optional shared cache policy should be explicit:

```toml
[cache]
share_with_family = false
```

Default remains false.

## Implementation phases

### Phase 1: scope rail and filtering

- Add `FleetScopeItem` and family grouping in `repo_fleet.rs`.
- Make `RepoFilter` support `Family`.
- Update `App::repo_filter()`, `selected_repo()`, and repo navigation to use scope items.
- Update `repo_fleet_bar` to render scope items instead of every repo.
- Filter Workflow PR rail, runner feeds, job matrix, and release CI history.

### Phase 2: repo metadata plumbing

- Add `project_id` to repo registry/config.
- Ensure GitLab job/runner feed producers set project id and repo slug/alias when possible.
- Add migration or adapter layer for evidence, cache, audit, and Git events to carry repo identity.

### Phase 3: utilization cockpit

Add a top Jobs/Mission utilization strip:

```text
Scope veox-*  runners 7/10 busy  queued 4  running 7  failed 1  idle 3  cache hit 88% global
```

Recommended fields:

- busy runners / total runners;
- queued jobs;
- running jobs;
- failed jobs;
- idle managers;
- oldest queued job age;
- slowest running job;
- cache hit ratio, explicitly marked global until repo-scoped cache metrics exist.

### Phase 4: search and deep drill

- Add `/` palette for repo/family filtering.
- Let `Enter` inside a family overlay descend into individual child repos.
- Add `Backspace` or `Esc` to move back from repo to family, then to All.

## Testing plan

Unit tests:

- `repo_fleet::dashed_repo_names_auto_group_into_family_scope`
- family filter matches slug basename even with short alias;
- unscoped data visible only in All;
- `Esc` resets selected scope to All;
- scoped jobs sort by newest `received_at`.

Render tests:

- default All bar;
- family bar segment;
- family overlay child list;
- Jobs tab with scoped feeds;
- empty family scope.

Manual validation:

```bash
cargo test -p jeryu --lib repo_fleet
cargo nextest run -p jeryu -- tui::repo_fleet_bar tui::runtime::input::navigation
cargo run -p jeryu -- tui --demo --tab jobs --once
```

## Risks and mitigations

| Risk | Mitigation |
| --- | --- |
| Historical records have no repo metadata | Show only in All; add producer telemetry |
| Family prefix accidentally groups unrelated repos | Use slug basename; allow future `family = "..."` override in registry |
| Scope rail still too long | Collapse low-activity ungrouped repos behind `Other` in a follow-up |
| Cache accidentally shared by family | Treat family as display-only and namespace cache by repo/project |
| Pipeline progress remains global | Phase 2 adds project id to progress view or computes per-scope progress |

## Acceptance criteria

- TUI starts in `All` scope.
- `veox-shared`, `veox-deploy`, and `veox-nht` appear as `veox-*`.
- Selecting `veox-*` filters Workflow, Jobs, runner feeds, and release CI history.
- Selecting a single repo excludes sibling family jobs.
- `Esc` always returns to `All`.
- Jobs are ranked by most recent within current scope.
- Cache/data paths are repo-scoped by default.
