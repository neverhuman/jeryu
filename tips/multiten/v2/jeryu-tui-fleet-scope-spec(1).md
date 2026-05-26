# JeRyu TUI Fleet Scope Redesign

## Goal

Make JeRyu usable when the operator is running many mixed repositories at once. The TUI should default to a fleet-wide **All** view, then let the operator drill into project families such as `veox-*`, then into a specific repo such as `veox-shared`, without losing the ability to see runner utilization, job progress, release state, approvals, and evidence across the whole machine.

This spec is paired with `jeryu-tui-fleet-scope.diff`.

## Current state observed

The existing TUI already has the right foundation:

- `src/tui/app.rs` keeps fleet state, `selected_repo_index`, and an existing `RepoFilter` concept.
- `src/repo_fleet.rs` parses `.jeryu/repos.toml` or tracked repositories into `FleetSnapshot` and `FleetRepoSnapshot`.
- `src/tui/repo_fleet_bar.rs` renders the current one-line repo rail.
- `src/tui/ui.rs` draws the header, fleet rail, active tab, activity log, and footer.
- `src/tui/workflow/model.rs` already has `repo_alias` and `repo_slug` on `PullRequestView`, so Workflow can become scope-aware immediately.

The problem is that the current fleet rail is a flat horizontal list. This works for 3-5 repos, but it breaks down when the fleet grows. It also makes families like `veox-shared`, `veox-deploy`, `veox-nht`, and `veox-proofs` look like unrelated peers instead of one parent project.

## Proposed UX

### 1. Default scope is All

On launch, the selected scope is always:

```text
scope:All(N)
All run:<n> fail:<n> aged:<n>
```

Every tab shows fleet-wide data by default: Workflow, Mission, Release, Approvals, Jobs, Agents, Tests, Pools, Cache, Evidence, Bugs, Secrets, LLMs, Git, and Jankurai.

### 2. Auto-family grouping

Any repo whose display alias or slug basename matches `<prefix>-<suffix>` gets a family key:

```text
veox-shared  -> veox-*
veox-deploy  -> veox-*
veox-nht     -> veox-*
veox-proofs  -> veox-*
```

The top-level rail becomes:

```text
All run:7 fail:0 aged:1   veox-* 4repos r6 f0 aged:1   jekko local r0 f0
```

Families are view scopes only. They do **not** imply shared cache, shared data, or shared credentials.

### 3. Enter drills into a family

When focus is on the fleet bar:

- `←/→` or `h/l` moves between visible fleet scopes.
- `Enter` on `veox-*` opens that family rail.
- `Enter` on an individual repo opens the detail overlay.
- `Esc` returns to All.

Family drill-down rail:

```text
veox-* / veox-* 4repos r6 f0 aged:1   veox-shared green r2 f0 score:97   veox-nht running r3 f0
```

### 4. Recency-ranked scopes and repos

Repos and family groups are ranked by:

1. Failed work.
2. Running work.
3. Dirty local checkout.
4. Most recent workflow observation.
5. Alias as a deterministic tie-breaker.

This keeps the newest and most operationally important work close to the keyboard cursor.

### 5. Scope is global, not a single-tab filter

The selected scope should be consumed by every tab that has repo-identifying metadata.

Immediate support:

- Workflow / delivery PRs: filter by `repo_alias` or `repo_slug`.
- Jobs matrix and Mission active-work counters: filter by `project_id` when the registry supplies it.
- Repo detail overlay: show repo family, project id, and cache namespace.

Follow-up support:

- Runner feed: add repo alias/project id to `RunnerFeed` and apply the same scope filter.
- Evidence: add repo alias/project id to evidence records or derive it from tracked projects.
- Cache: present cache namespaces per repo and family rollups.
- Release/Approvals: expose repo identity on release/approval rows so family scopes can isolate them.

## Data model changes

### `RepoConfig`

Add optional `project_id` from `.jeryu/repos.toml`:

```toml
[[repo]]
alias = "veox-shared"
slug = "neverhuman/veox-shared"
project_id = 48
```

The parser accepts either string or integer values.

### `FleetRepoSnapshot`

Add:

```rust
family_key: Option<String>,
project_id: Option<String>,
last_observed_at: Option<String>,
```

### `RepoFamilySnapshot`

New rollup:

```rust
pub struct RepoFamilySnapshot {
    pub key: String,
    pub label: String,
    pub repo_indices: Vec<usize>,
    pub running_count: u32,
    pub failed_count: u32,
    pub stale_count: u32,
    pub last_observed_at: Option<String>,
}
```

### Fleet scope selection

The TUI keeps a canonical fleet selection separate from the legacy `selected_repo_index`:

```rust
selected_fleet_index: usize,
selected_fleet_family: Option<String>,
```

`selected_repo_index` remains synchronized for compatibility while the new grouped rail rolls out.

## Cache and data isolation

Family grouping must never share cache or mutable data by default.

Default cache namespace:

```text
repo_cache_namespace = repo.slug
```

Examples:

```text
neverhuman/veox-shared
neverhuman/veox-deploy
neverhuman/veox-nht
```

A family like `veox-*` is an aggregate lens only. It can show total cache usage across children, but it must not cause cache reuse between child repos.

Optional future explicit sharing:

```toml
[[repo]]
alias = "veox-shared"
cache_namespace = "veox-shared"

[[repo]]
alias = "veox-nht"
cache_namespace = "veox-nht"
```

No implicit `veox-*` cache namespace should exist.

## Acceptance criteria

1. `jeryu tui` launches with `scope:All(N)` selected.
2. Repos matching `<prefix>-<suffix>` collapse into one top-level `<prefix>-*` family when there are two or more family members.
3. `Enter` on a family drills into that family.
4. `Enter` on a repo opens repo detail.
5. `Esc` returns to All from any family/repo scope.
6. The Workflow tab filters PRs by repo or family using existing `repo_alias` / `repo_slug` fields.
7. The Mission active-work count and Jobs matrix filter by `project_id` when configured.
8. Unknown repo-less rows are shown only under All, not leaked into repo/family scopes.
9. Family grouping does not change cache namespaces or secrets/data boundaries.
10. Repo/family rails are ordered by operational urgency and recency.

## Suggested follow-up migrations

The patch intentionally avoids a heavy database migration, but the next iteration should add durable repo identity to runtime rows:

```sql
ALTER TABLE job_events ADD COLUMN repo_alias TEXT;
ALTER TABLE job_events ADD COLUMN repo_slug TEXT;
ALTER TABLE ci_job_runs ADD COLUMN repo_alias TEXT;
ALTER TABLE ci_job_runs ADD COLUMN repo_slug TEXT;
ALTER TABLE evidence_records ADD COLUMN repo_alias TEXT;
ALTER TABLE evidence_records ADD COLUMN repo_slug TEXT;
```

`RunnerFeed` should also grow:

```rust
pub repo_alias: Option<String>,
pub repo_slug: Option<String>,
pub project_id: Option<i64>,
```

Once this lands, all repo/family filters become exact instead of `project_id`-best-effort.

## Test plan

Recommended commands after applying the diff:

```bash
cargo fmt
cargo test -p jeryu --lib repo_fleet
cargo test -p jeryu --lib tui::repo_fleet_bar
cargo test -p jeryu --lib tui::runtime::input::navigation::general
cargo nextest run -p jeryu -- tui
jeryu tui --once
jeryu tui --capture --tab workflow --output /tmp/jeryu-workflow.png
jeryu tui --capture --tab jobs --output /tmp/jeryu-jobs.png
```

Manual smoke:

1. Add `veox-shared`, `veox-deploy`, `veox-nht`, and `veox-proofs` to `.jeryu/repos.toml`.
2. Launch `jeryu tui`.
3. Verify the rail shows `All` and `veox-*`, not every `veox-*` repo at top level.
4. Focus the FleetBar, press `Enter` on `veox-*`, then confirm the child repos appear.
5. Press `Esc` and confirm scope returns to All.
6. Start jobs in two child repos and verify All shows all jobs while the family shows only family jobs.
