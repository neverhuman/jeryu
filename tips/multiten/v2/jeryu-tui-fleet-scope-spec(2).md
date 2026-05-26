# JeRyu TUI Fleet Scope & Utilization Spec

## Goal

JeRyu should be comfortable with dozens or hundreds of mixed repositories. The operator’s default view is **ALL**, which shows all workflows, mission state, release work, approvals, jobs, agents, tests, pools, cache, evidence, bugs, secrets, LLMs, and Git activity. From that global cockpit, the operator can drill into a repo family such as `veox-*`, and then into a specific child repo such as `veox-shared` or `veox-nht`.

The core operator question is: **are we keeping the machine close to perfect runner utilization without losing per-project isolation?** The TUI should answer that in the first second.

## Proposed UX model

### 1. Default scope: ALL

Startup and `Esc` return to `ALL`. `ALL` is not a repository. It is the global fleet scope and must include every repo-backed item plus unclassified items that do not yet carry repo metadata.

The header should show the scope explicitly:

```text
jeryu  scope:ALL(37)  GitLab:OK  ctrs:10  pools:3/3  cache:87%
```

When scoped to a family:

```text
jeryu  scope:veox-*  GitLab:OK  ctrs:10  pools:3/3  cache:87%
```

When scoped to a concrete repo:

```text
jeryu  scope:veox-nht  GitLab:OK  ctrs:10  pools:3/3  cache:87%
```

### 2. Replace the flat repo rail with a scope rail

The current one-row fleet bar works for a small number of repos but fails once the tracked fleet grows. The root rail should show these scope items, sorted by operational priority:

1. `ALL`
2. auto-family scopes such as `veox-*`, `jekko-*`, `apps-*`
3. standalone repos that do not belong to a multi-repo family

Each segment should display rollups:

```text
ALL run:18 fail:1 aged:2   veox-* r9 f0 repos:6   jekko-* r4 f1 repos:3   jeryu green r0 f0 score:97
```

The rail should be horizontally windowed around the selected scope, not a full left-to-right dump. It should show `‹` / `›` when more scopes exist off-screen.

### 3. Automatic family grouping

Family grouping is derived from the repo name segment of the slug. If the repo basename has a dash, the prefix before the first dash is the family key.

Examples:

| Slug | Family key | Root scope |
|---|---:|---:|
| `neverhuman/veox-shared` | `veox` | `veox-*` |
| `neverhuman/veox-deploy` | `veox` | `veox-*` |
| `neverhuman/veox-nht` | `veox` | `veox-*` |
| `neverhuman/jeryu` | none | `jeryu` |

Only groups with at least two repos should become family scopes. Singletons remain standalone to avoid hiding repos under fake families.

### 4. Drill-down behavior

Keyboard model:

| Key | Root rail | Family overlay | Repo overlay |
|---|---|---|---|
| `←/→` or `h/l` | select previous/next root scope | select family overview or child repo | select previous/next root scope |
| `Enter` | open selected scope | keep overlay open and apply selected child scope | open repo detail |
| `Esc` | return to `ALL` / close overlay | close overlay and return to `ALL` | close overlay and return to `ALL` |

Family overlay shape:

```text
Fleet scope: veox-*
Family  veox-*  Enter family overview or choose a child repo
Scope   filters UI only; cache/data remain isolated per repo

▶ family  veox-*       r9 f0 aged:0 repos:6
  repo    veox-shared  r2 f0 aged:0 repos:1 score:97
  repo    veox-deploy  r3 f0 aged:0 repos:1 score:78
  repo    veox-nht     r4 f0 aged:0 repos:1 score:95

←/→ selects family/child repo. Esc returns to ALL.
```

### 5. Sorting and ranking

Root scopes and family children should be ranked by:

1. failed count descending
2. running count descending
3. latest observed workflow time descending
4. family before repo when otherwise tied
5. label ascending

Job tables and live runner feeds should rank most-recent active work first, then failures, then queued work, then completed work. Within a status bucket, sort by `updated_at` descending. The operator should not need to hunt for the newest failing or running job.

### 6. Scope filtering contract

`RepoFilter` should become:

```rust
pub enum RepoFilter<'a> {
    All,
    Family { prefix: &'a str, label: &'a str },
    Only { alias: &'a str, slug: &'a str },
}
```

Matching rules:

- `All` matches everything.
- `Family` matches items whose repo slug basename or alias starts with `<prefix>-`.
- `Only` matches exact alias or slug.
- Items with no repo metadata appear only in `All`; they must not leak into a family or repo drill-down.

This prevents cross-project confusion when a legacy event has only a job id or project id and no tracked repo mapping.

### 7. Runner utilization surface

The Mission and Jobs tabs should expose a utilization strip in every scope:

```text
Runners  9/10 busy  90% saturation  queued:14  oldest:04m21s  idle-cap:1  cache-hit:87%
```

Recommended fields:

- active runner slots
- total runner capacity
- saturation percentage
- queued runnable jobs
- oldest queued age
- blocked jobs
- average active-job age
- cache hit ratio for the active scope
- number of jobs waiting for missing project cache/data

For `ALL`, the strip aggregates across all projects. For a family, it aggregates only the family’s repos. For a repo, it is exact to that repo.

### 8. Cache and data isolation

Scope filtering is a UI projection only. It must not merge cache namespaces, artifact state, runner data, repo-local state, or evidence ledgers across repos.

Cache key ownership should remain repo-qualified:

```text
<provider>/<owner>/<repo>/<branch-or-sha>/<toolchain>/<cache-key>
```

Family scopes may show rollups, but they must retain per-child breakdowns and should label any cache reuse as one of:

- `repo-local` — safe same-repo reuse
- `family-rollup` — display-only aggregation, no shared cache
- `global` — shared infrastructure metric only, not a project cache

The Cache tab should make this explicit under any non-ALL scope:

```text
scope: veox-* · cache namespaces: isolated per repo · shared view: rollup only
```

### 9. Engineering plan

#### Phase 0: Scope rail and family drill-down

- Add `FleetScopeItem` and `FleetScopeKind` to `repo_fleet`.
- Add `FleetSnapshot::root_scope_items()` and `FleetSnapshot::family_child_scope_items()`.
- Extend `RepoFilter` with `Family`.
- Replace flat fleet-bar rendering with a scope rail.
- Add family drill-down overlay.
- Update footer/header labels to say `scope`, not `repo`.

#### Phase 1: Propagate filtering everywhere

Apply `app.repo_filter()` in:

- Workflow PR rail and workflow DAG (already partially wired)
- Mission top signal and Active Work metrics
- Release stage cards
- Approvals queue
- Jobs runner feed, matrix, progress pane, inspector
- Agents sessions
- Tests bottlenecks and histories
- Pools utilization by active project where possible
- Cache, Evidence, Bugs, Secrets, Git panes

Any model lacking `repo_alias` / `repo_slug` should be extended at the collector boundary, not guessed in render code.

#### Phase 2: Utilization index

Introduce a small read-side projection for the TUI:

```rust
pub struct ScopeUtilizationView {
    pub scope_label: String,
    pub repo_slugs: Vec<String>,
    pub runner_slots_total: u32,
    pub runner_slots_busy: u32,
    pub queued_jobs: u32,
    pub runnable_jobs: u32,
    pub blocked_jobs: u32,
    pub failed_jobs: u32,
    pub oldest_queued_secs: Option<u64>,
    pub active_job_age_p50_secs: Option<u64>,
    pub active_job_age_p95_secs: Option<u64>,
    pub cache_hit_ratio: Option<f64>,
}
```

The TUI should render this projection instead of recomputing utilization from unrelated panes.

#### Phase 3: Fast search and command palette

Once the rail has many families, add a scope picker in `^K`:

```text
scope veox-*        family  6 repos  r9 f0
scope veox-shared   repo    r2 f0 score 97
scope jeryu         repo    r0 f0 score 97
```

Typing `scope veox` should jump directly to the family. Typing a child repo should enter that repo scope.

### 10. Acceptance tests

Minimum tests:

- dashed repo slugs form a family scope only when there are at least two members
- family rollups sum running/failed/stale counts correctly
- `RepoFilter::Family` matches `veox-*` repos and rejects `jekko-*` repos
- root rail renders `ALL` and at least one family scope for demo fixtures
- `Enter` on a family opens family overlay
- `Esc` clears overlay and returns to `ALL`
- workflow PR selection remains valid after applying a family filter
- jobs pane does not show metadata-less jobs in a repo/family scope

Suggested proof commands:

```bash
cargo fmt
cargo test -p jeryu --lib repo_fleet
cargo test -p jeryu --lib tui::repo_fleet_bar
cargo nextest run -p jeryu -- tui
```

### 11. Notes and risks

- GitLab project IDs and GitHub workflow runs need consistent repo mapping. Prefer resolving this in the collectors so renderers receive `repo_alias` and `repo_slug`.
- Recency sorting depends on workflow timestamps being RFC3339 strings; normalize at collection time.
- Some global infrastructure panes, such as runner pool health, are not owned by one repo. In non-ALL scopes, show them as capacity context but do not claim they are project-owned.
- This spec intentionally avoids sharing cache or data between projects. Family scopes are dashboards, not cache namespaces.
