# Jeryu TUI Fleet Scope Navigator — Engineering Spec

## Problem

The current TUI already has the right bones for a multi-repo cockpit: it has a top header, tabs, a one-row fleet bar, workflow canvas, live jobs/log panes, and a repo filter that is passed into parts of the workflow renderer. The weakness is the operator experience when the fleet grows: the fleet bar renders `All` plus every repo left-to-right, so navigation becomes noisy and horizontally unbounded. Filtering is also partial: PR delivery already has repo-aware metadata, but job/log selection still mostly operates over the global `recent_jobs` list.

The operator goal is to keep the default scope as **All**, but make it very fast to drill into a family such as `veox-*`, then into a specific child repo such as `veox-shared`, while ensuring scoped views do not imply shared cache or data between projects.

## Goals

1. Keep **All** as the default scope across Workflow, Mission, Release, Approvals, Jobs, Agents, Tests, Pools, Cache, Evidence, Bugs, Secrets, LLMs, Git, and Jankurai.
2. Auto-group dash-named repos by their first segment: `veox-shared`, `veox-deploy`, and `veox-nht` become one `veox-*` family scope.
3. Let operators arrow through scopes by recency and status, not by registry order.
4. Enter on a family opens a vertical child drawer; Up/Down selects a child; Enter scopes to that child; Esc returns to All.
5. Make scoped Workflow/Jobs/log selection show only relevant data when repo/project metadata exists.
6. Preserve project isolation. Scope rollups are presentation-only; cache keys, state rows, artifacts, and data paths remain per project/repo.

## Non-goals

- Do not rewrite the TUI into a new framework.
- Do not change runner scheduling behavior in this patch.
- Do not merge caches between repo families.
- Do not make GitHub/GitLab API calls from render code.

## Current state observed

- `TuiStateSnapshot` already carries `fleet`, `recent_jobs`, `delivery_snapshot`, `live_log`, and other per-tab datasets.
- `FleetSnapshot` currently contains a flat list of `FleetRepoSnapshot` objects and activity events.
- `RepoFilter` currently supports `All` and single-repo `Only` modes.
- `repo_fleet_bar.rs` renders `All` and every repo as a one-line horizontal chip list.
- `workflow/pr_rail.rs` already has filter-aware rendering and hit-testing support, but the mouse handler uses the unfiltered hit-test.
- `JobEvent` has `project_id` but no repo alias/slug, so exact repo job filtering needs registry/project ID metadata.

## Proposed design

### 1. Scope model

Add fleet scope concepts derived from the flat repo list:

```rust
pub enum RepoScopeKind {
    All,
    Family(String),   // e.g. "veox"
    Repo(usize),      // index into FleetSnapshot.repos
}

pub struct FleetScopeSnapshot {
    pub kind: RepoScopeKind,
    pub label: String,          // All, veox-*, veox-shared
    pub repo_count: usize,
    pub running_count: u32,
    pub failed_count: u32,
    pub aged_count: u32,
    pub status: String,
    pub last_activity_at: Option<String>,
}
```

Each `FleetRepoSnapshot` gains:

```rust
pub display_name: String,       // prefer slug basename like veox-shared
pub family: Option<String>,     // first segment before '-'
pub project_id: Option<i64>,    // GitLab project ID when known
pub last_activity_at: Option<String>,
```

The family key is inferred from the visible repo name, not the short alias, because aliases can be shortened (`shared`) while the project identity remains `veox-shared`.

### 2. Ranking

Rank scopes and repos by operational urgency:

1. All remains first.
2. Families/singletons sort by `last_activity_at` descending.
3. Running scopes sort ahead of idle scopes when timestamps tie.
4. Failed scopes sort ahead of green scopes when timestamps tie.
5. Labels sort alphabetically as a final tie-breaker.

This makes the bar useful for watching active work without making the operator scan every repo manually.

### 3. Fleet scope navigator UI

The one-line bar becomes a **scope navigator**:

```text
All run:10 fail:0 aged:0   veox-*(12) r7 f0   jeryu(1) r0 f0   infra-*(5) r2 f1
```

The header badge changes from `repo:*` to `scope:*`:

- `scope:All(27)`
- `scope:veox-*`
- `scope:veox-shared`

Enter opens a scope drawer:

- On **All**: show summary, registry path, active families/repos, and help.
- On a **Family**: show a vertical child list sorted by recency/status.
- On a **Repo**: show slug, local branch/sha, dirty status, latest run, score, project ID, and next command.

Keyboard behavior:

| Key | Behavior |
|---|---|
| `A` | Jump to All and close drawer |
| `Left` / `Right`, `h` / `l` | Cycle scopes |
| `Enter` on family | Open child drawer; if drawer is open, select highlighted child repo |
| `Up` / `Down`, `k` / `j` in family drawer | Move child cursor |
| `Esc` | Close drawer and return to All |

### 4. Filtering semantics

`RepoFilter` becomes:

```rust
pub enum RepoFilter<'a> {
    All,
    Family { family: &'a str },
    Only { alias: &'a str, slug: &'a str, project_id: Option<i64> },
}
```

Expected behavior:

- **All** matches everything.
- **Family** matches repo alias/slug/display family when repo metadata is present.
- **Only** matches alias, slug, or project ID.
- Jobs use `project_id` where possible; when a repo lacks project ID metadata, we should fail open for visibility until the registry is complete. This avoids hiding live jobs silently.

### 5. Tab-by-tab effect

| Tab | Scope behavior |
|---|---|
| Workflow | Delivery PR rail and workflow canvas filter by `repo_alias` / `repo_slug` / family. |
| Jobs | Recent jobs list filters by `project_id` when registry metadata exists. Selected log target follows the filtered visible list. |
| Activity / Logs | Shows selected job from the scoped Jobs list. |
| Pools | Still global by default; phase 2 can add repo/family usage attribution. |
| Cache | Show global summary under All; show per-project namespaces under repo/family when cache metadata is available. Never merge keys across projects. |
| Evidence | Filter once evidence rows include repo/project metadata. Until then, show All-only or clearly mark unscoped rows. |
| Bugs/Git/Secrets/Jankurai | Filter when those records have target project/repo metadata; otherwise mark as global/unscoped. |

### 6. Project/cache/data isolation

The scope navigator is a view-level filter. It must not change isolation boundaries.

Rules:

1. Cache namespace remains per repo/project.
2. Cache rollups may aggregate counts for `veox-*`, but each row should retain source repo/project.
3. No scoped action may operate on hidden repos unless the user is in All and the action explicitly says it is fleet-wide.
4. Any future mutating action in a scoped view must carry the selected `RepoFilter` or explicit repo slug/project ID in its action payload.

### 7. Data migration / registry update

Add optional `project_id` to `.jeryu/repos.toml` entries:

```toml
[[repo]]
alias = "shared"
slug = "neverhuman/veox-shared"
remote = "https://github.com/neverhuman/veox-shared.git"
local_root = "/home/ubuntu/veox-repos/veox-shared"
default_branch = "main"
project_id = 601
```

This is optional at first. If absent, the TUI can still group/filter PRs by slug/alias and display jobs in All; exact Jobs filtering becomes available once project IDs are supplied.

## Rollout plan

### Phase 0 — Scope model and read-only UI

- Add family inference and scope summaries to `repo_fleet.rs`.
- Replace flat bar rendering with scope chips.
- Add family drawer and child navigation.
- Keep default All.
- Update demo fixture so screenshots show `veox-*`.

### Phase 1 — Scope-aware Workflow and Jobs

- Pass `OwnedRepoFilter` into Workflow renderer.
- Fix mouse PR rail to use filtered hit-test.
- Filter visible Jobs by `project_id`.
- Make selected log target follow the visible Jobs list.

### Phase 2 — Scope-aware cache/evidence/utilization

- Add repo/project identifiers to cache/evidence rows where missing.
- Show runner utilization by scope: active, queued, waiting-for-resource, failed, stale.
- Add “near-perfect utilization” headline: `busy / capacity`, queue age p95, idle runners, blocked jobs.

### Phase 3 — Search and large fleet ergonomics

- Add `/` fuzzy search over scopes and repos.
- Add quick keys for family jumps when stable enough.
- Add persisted last-selected scope in user TUI state.

## Acceptance criteria

1. Launching TUI starts in `scope:All`.
2. Repos named `veox-shared`, `veox-deploy`, `veox-nht` appear as one `veox-*` scope.
3. Enter on `veox-*` shows the child repo list vertically.
4. Selecting `veox-shared` updates header to `scope:veox-shared` and Workflow PR rail shows only matching PRs.
5. Jobs tab selection and live log target are derived from the filtered visible jobs list.
6. Mouse clicks in the Workflow PR rail cannot select a hidden PR from another scope.
7. All scope preserves current behavior.
8. Cache and data isolation are unchanged; group rollups never share or rewrite cache namespaces.

## Suggested validation commands

```bash
cargo fmt
cargo test -p jeryu --lib repo_fleet tui::repo_fleet_bar
cargo nextest run -p jeryu -- tui
cargo run -p jeryu -- tui --once
cargo run -p jeryu -- capture tui --tab workflow --output target/tui-workflow.png --width 160 --height 50
```

## Notes from this pass

This spec and diff were prepared from repository source inspection. I could not compile the patch inside the sandbox because outbound DNS for cloning GitHub was unavailable, so treat the diff as an implementation-ready proposal that should still receive `cargo fmt` and the validation suite above in the normal development environment.
