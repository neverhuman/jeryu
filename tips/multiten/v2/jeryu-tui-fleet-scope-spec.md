# JeRyu TUI Fleet Scope Navigation Spec

## Problem

The current TUI has the right core primitives, but the fleet rail is too flat for a workspace with many mixed repositories. It renders `All` plus every tracked repository left-to-right, so a growing fleet quickly becomes hard to scan. Operators need the opposite shape: default to the entire machine/workspace, then quickly narrow to a project family such as `veox-*`, then drill to one repo only when necessary.

The implementation already has several useful seams:

- `TuiStateSnapshot` carries a `fleet: FleetSnapshot` and the UI renders a one-line fleet bar below the tab header.
- `App::selected_repo_index` already represents `All` at `0` and a repository at `index - 1`.
- `App::repo_filter()` is already passed into the Workflow PR rail, so Workflow can hide non-matching repos without a wholesale TUI rewrite.
- `recent_jobs` are already ranked by live status and newest `received_at`, making the Jobs pane a good place to apply scope filtering instead of re-sorting every render tick.

## Goals

1. Keep the default view as **All** and make it a true cross-tab scope for Workflow, Mission, Release, Approvals, Jobs, Agents, Tests, Pools, Cache, Evidence, Bugs, Secrets, LLMs, Git, and Jankurai.
2. Automatically group any repeated `<prefix>-*` repositories into a family scope, for example `veox-shared`, `veox-deploy`, `veox-nht`, and `veox-proofs` become `veox-*`.
3. Sort fleet scopes by most-recent observed activity so the hot repos/families stay leftmost.
4. Let `Enter` on a family drill into its children while preserving the family aggregate as the first child scope.
5. Make filtering honest: if numeric GitLab `project_id` metadata exists, Jobs filters by it; if it does not exist yet, the pane stays visible and tells the operator the registry needs project IDs rather than pretending there are no jobs.
6. Never imply shared data/cache between repos. Family scopes are aggregate views only; cache and data namespaces remain per repo.

## Proposed Interaction Model

### Root rail

The rail shows:

```text
All run:<n> fail:<n> aged:<n>   veox-* running r9 f0 repos:4 score:97   jeryu local r0 f0 score:97   infra-* failed r0 f2 repos:3
```

`All` is always index `0`. Family aggregates are listed before ungrouped repos and sorted by latest observed activity. Solo repos that happen to have one dash are not grouped unless at least two repos share the same prefix.

### Family drill

When the operator focuses the fleet rail and presses `Enter` on `veox-*`, the rail switches to:

```text
veox-* › veox-* running r9 f0 repos:4   veox-deploy running r3 f0   veox-shared green r0 f0   veox-nht pending r1 f0
```

The first item is the family aggregate. Subsequent items are child repos sorted by latest activity. `Esc` closes the overlay/drill and returns to `All`, matching the current “safe reset” behavior.

### Cross-tab scope semantics

- **All:** no filtering.
- **Family:** show only data that can be associated with repos in the family.
- **Repo:** show only data for that repo.

Where data lacks repo metadata, the first implementation should prefer “visible with an isolation warning” over “empty and misleading.” The schema/backfill work below removes that limitation.

## Data Model Changes

### Registry parsing

Extend `RepoConfig` and `FleetRepoSnapshot` with:

```rust
pub project_id: Option<i64>,
pub project_key: Option<String>,
```

`project_id` accepts either a TOML integer or string. Numeric values drive GitLab job filtering. String labels remain useful for display and backward-compatible registries.

### Scope summaries

Add `FleetScopeSummary`:

```rust
pub struct FleetScopeSummary {
    pub kind: FleetScopeKind, // All | Family | Repo
    pub key: String,
    pub label: String,
    pub repo_indices: Vec<usize>,
    pub running_count: u32,
    pub failed_count: u32,
    pub aged_count: u32,
    pub status: String,
    pub score_badge: Option<String>,
    pub latest_activity_at: Option<String>,
}
```

`FleetSnapshot::scope_entries(drilled_family)` returns either root scopes or child scopes.

## Rendering Changes

1. Replace flat repo iteration in `repo_fleet_bar.rs` with `App::repo_scope_entries()`.
2. Show family labels as `<prefix>-*`, including repo count and aggregate run/fail/aged/score.
3. Enhance the detail overlay:
   - All scope: total repos, aggregate counts, registry path.
   - Family scope: child repo table, latest activity, explicit “aggregate-only; cache/data remain per repo” line.
   - Repo scope: slug, local branch/SHA/dirty, latest run, score, suggested next command, project key.
4. Keep the one-line rail cheap to render; only the overlay uses a vertical detail table.

## Filtering Changes

### Workflow

`RepoFilter` gains `Family { prefix }` and continues to match PR metadata via alias or slug. Existing Workflow PR rendering already accepts a filter and should work with this extension.

### Jobs

`App::filtered_recent_jobs()` applies the selected scope to `recent_jobs`:

- All: every job.
- Family/repo with numeric project mappings: job `project_id` must match a repo in scope.
- Family/repo without numeric mappings: keep jobs visible, and surface a registry warning in the overlay/spec follow-up.

The selected job index must be synchronized against the filtered list, not the raw `recent_jobs` vector.

### Mission / Release / Approvals / Tests / Evidence / Git / Bugs

Front-end scope should be plumbed now, but some panes require state-store metadata to be perfect. The next state migration should add `repo_slug`, `repo_alias`, and/or `project_id` to:

- approvals queue records
- release attempts and release stage cards
- evidence records
- test bottlenecks / test executions
- Git command events
- bug records
- cache namespace telemetry

Until that migration, show pane-local summaries with a small “scope: All/veox-*/repo” label and avoid hiding data that cannot be attributed.

## Isolation Rules

The TUI must not create or imply cross-repo cache/data sharing.

- Family views are aggregations only.
- Cache panels should group by repo namespace once cache telemetry exposes namespace IDs.
- Job log tails remain keyed by `(project_id, job_id)`.
- Any action launched from a scoped pane must carry the concrete repo/project ID. Family-level actions are only allowed when the backend explicitly supports batch mode and shows the affected repo list.

## Operator Hotkeys

- `←/→` or `h/l`: move between fleet scopes when the rail is drilled/open.
- `Enter`: open detail; on a family, drill into child scopes.
- `Esc`: close detail/drill and return to All.
- Existing tab shortcuts remain unchanged.

## Acceptance Criteria

1. With repos `veox-shared`, `veox-deploy`, `veox-nht`, and `jeryu`, the root rail shows `All`, `veox-*`, and `jeryu` rather than all three `veox-*` children.
2. `veox-*` aggregates running, failed, aged, and score fields from child repos.
3. Family and child scopes are sorted by latest run activity descending.
4. Pressing `Enter` on `veox-*` drills to `veox-*`, `veox-deploy`, `veox-shared`, `veox-nht`, etc.
5. Jobs pane selection and log tailing operate on filtered jobs.
6. Workflow PR rail filters correctly for All, family, and repo scopes.
7. The overlay explicitly says that family scope is an aggregate-only view and cache/data namespaces remain per repo.
8. Existing `cargo test -p jeryu --lib tui::repo_fleet_bar` and `cargo test -p jeryu --lib repo_fleet` are extended to cover family grouping and drill behavior.

## Rollout Plan

1. Merge the front-end patch in this diff.
2. Backfill `.jeryu/repos.toml` with numeric GitLab `project_id` for every GitLab-backed repo.
3. Add state-store repo attribution columns in a separate migration.
4. Update each pane to use a common `App::scope_label()` and pane-specific filtered iterators.
5. Add a `tui --scope <all|family|repo>` smoke mode so CI screenshots can prove All, family, and repo views.
