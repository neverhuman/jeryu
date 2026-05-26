# Jeryu TUI Fleet Scope & Runner Utilization Spec

## Goal

Make `jeryu tui` useful when a single machine is supervising many mixed repositories. The default view is **ALL**: every workflow, mission signal, release, approval, job, runner, cache, evidence record, bug, secret event, LLM event, Git event, and Jankurai result should be visible in one operator cockpit. From that cockpit, the operator can quickly drill into a repo family such as `veox-*`, then into a concrete repo such as `veox-shared`, without losing the same mental model or keybindings.

The design optimizes for three operator questions:

1. Are my runners close to fully utilized without starving pending work?
2. What is the freshest or most urgent work across the whole fleet?
3. Is this signal from ALL repos, a family, or one repo, and is cache/data isolation preserved?

## Current repo observations

The repository already has the right bones for this: a fleet registry/collector, repo metadata on delivery items, a repo filter, a top fleet bar, and filter-aware PR rail rendering. The missing piece is a first-class **scope model** that scales beyond a flat left-to-right chip list.

Relevant existing components:

- `repo_fleet.rs` defines the fleet registry, `FleetSnapshot`, `FleetRepoSnapshot`, and `RepoFilter`.
- `repo_fleet_bar.rs` renders a one-line flat rail: `All` plus every repo.
- `workflow/model.rs` already carries `repo_alias` and `repo_slug` on `PullRequestView` and has `next_pr_matching`, `prev_pr_matching`, `ensure_selection_matches`, and `count_matching`.
- `workflow/pr_rail.rs` already has filter-aware rendering and hit testing.
- `ui.rs` already passes `app.repo_filter()` into the delivery renderer.
- `mouse.rs` still uses unfiltered PR hit-testing, which can select hidden PRs when a repo scope is active.

## UX model

### Persistent scope rail

Keep the existing location: directly under the header/tabs. Replace the flat repo list with a hierarchical scope rail.

Root rail:

```text
ALL run:8 fail:1 queued:12 util:88%   veox-* run:5 fail:0 repos:6   jeryu local r0 f0 score:89   docs-* run:1 fail:1 repos:3
```

Family drill rail:

```text
veox-* /   veox-shared running r2 f0   veox-deploy dirty r1 f0   veox-nht green r0 f0   veox-proofs running r2 f0
```

Behavior:

- Default selection is `ALL`.
- Left/right or `h`/`l` moves across the visible scope items.
- Enter on a family at root drills into that family.
- Enter on a repo opens detail.
- Enter on `ALL` opens global detail.
- Esc always returns to `ALL` and closes detail.
- The selected scope is displayed in every pane title, for example `Workflow · veox-*` or `Live Jobs · veox-shared`.

### Scope semantics

| Scope | Meaning | Render behavior |
| --- | --- | --- |
| `ALL` | Every tracked repo | aggregate all panels |
| `family` | Repos with the same dash prefix, e.g. `veox-*` | aggregate only family members |
| `repo` | One concrete tracked repo | show only that repo’s data |

A repo family is auto-created when two or more repo names share the text before the first dash. For `neverhuman/veox-shared`, `neverhuman/veox-deploy`, and `neverhuman/veox-nht`, the promoted family key is `veox-*`. Singleton dash-prefixed repos stay as repo chips so the root rail does not become noisy.

## Data model changes

Add:

```rust
pub struct RepoFamilySnapshot {
    pub key: String,          // veox-*
    pub prefix: String,       // veox
    pub repo_count: usize,
    pub running_count: u32,
    pub failed_count: u32,
    pub stale_count: u32,
    pub members: Vec<String>, // aliases
    pub score_badge: Option<String>,
}

pub enum RepoFilter<'a> {
    All,
    Family { key: &'a str },
    Only { alias: &'a str, slug: &'a str },
}
```

`FleetSnapshot` owns `families: Vec<RepoFamilySnapshot>` and exposes `scope_items(drilled_family)` so renderers never duplicate grouping logic.

## Data isolation contract

Family and ALL views aggregate **metrics**, not mutable stores.

Default policy:

- logs: repo-scoped
- evidence: repo-scoped
- cache keys: repo-scoped
- artifacts: repo-scoped
- runner working directories: repo-scoped
- secrets: repo-scoped, with family/all views showing redacted rollups only

Recommended cache namespace:

```text
cache_scope = sha256(provider + ":" + slug)
```

A family can show combined cache hit/miss rates, but it must not share mutable cache entries across repos. A future explicit opt-in could allow immutable artifact sharing via `cache_policy = "shared-family-readonly"`, but the default should remain isolated.

## Job ranking and utilization

The operator’s default ALL view should rank work by an urgency-first comparator, with recency inside each bucket:

1. failed / blocked / approval needed
2. running / preparing / waiting_for_resource
3. pending / queued / created
4. recently completed
5. stale / inactive

Within each bucket, sort by `updated_at DESC`, then `started_at DESC`, then repo alias. This keeps the newest activity near the top while still preventing failures from disappearing under busy running jobs.

Add a sort toggle later:

- `u`: utilization priority
- `r`: most recent
- `s`: status urgency
- `p`: repo/family grouping

### Fleet work item projection

Normalize mixed providers into one display row:

```rust
pub struct FleetWorkItem {
    pub repo_alias: String,
    pub repo_slug: String,
    pub backend: String,       // github, gitlab, local
    pub kind: String,          // workflow_run, job, approval, release, proof_lane
    pub status: String,
    pub title: String,
    pub runner: Option<String>,
    pub pool: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub pipeline_id: Option<i64>,
    pub job_id: Option<i64>,
    pub cache_scope: String,
    pub url: Option<String>,
}
```

Every tab can filter these rows through `RepoFilter::matches`.

## Views to update

### Workflow

Already mostly wired. Required fixes:

- Ensure selected PR matches active scope before drawing.
- PR rail arrow navigation must only cycle visible PRs.
- Mouse hit testing must use `pr_at_column_filtered`.
- Empty state should say `No PRs/jobs in veox-*` rather than global `No active pull requests`.

### Mission

Mission metrics should be scoped:

- `ALL`: aggregate across fleet.
- `family`: aggregate family only.
- `repo`: only that repo.

Add two high-signal tiles:

- `Runner Utilization`: busy / capacity / queued.
- `Queue Health`: queued age p50/p95 and oldest waiting job.

### Jobs

Replace `recent_jobs`-only list with `FleetWorkItem` when the fleet registry exists. The list should default to the selected scope and sort by urgency/recency. The inspector should show repo, provider, runner, cache namespace, pipeline/job IDs, and direct URL.

### Pools

Add per-scope utilization:

```text
all: 18/20 busy 90%  queued:12 oldest:6m
veox-*: 9/10 busy 90% queued:4 oldest:2m
veox-shared: 3/4 busy 75% queued:1 oldest:44s
```

### Cache

Show aggregate hit ratio at scope level while preserving repo-isolated namespaces. The detail pane should explicitly show `cache_scope` so operators can verify no accidental cross-repo sharing.

### Evidence / Secrets / Git / Bugs / Jankurai

All lists should accept the same `RepoFilter`. Items without repo metadata should show only in `ALL`, not leak into family/repo scope.

## Implementation plan

### P0 — scope rail and selection correctness

Implemented by the attached proposal diff:

- Adds family grouping to `repo_fleet.rs`.
- Adds `RepoFilter::Family`.
- Adds hierarchical scope selection to `App`.
- Replaces flat fleet rail rendering with root/family drill behavior.
- Keeps selected PR inside the active scope.
- Fixes PR rail mouse hit testing to respect the active scope.

### P1 — scope all tabs

Audit all renderers and list builders. Any row that has repo metadata must call `app.repo_filter().matches(...)`. Any row without repo metadata should render only when `RepoFilter::All` is active.

### P2 — normalize work across providers

Add `FleetWorkItem` and collectors for:

- GitHub Actions workflow runs/jobs
- GitLab pipelines/jobs
- local proof lanes
- approvals
- release stages
- evidence capsules

Use `FleetWorkItem` for ALL/family/repo Jobs and Mission active-work metrics.

### P3 — runner utilization model

Add `RunnerUtilizationSnapshot`:

```rust
pub struct RunnerUtilizationSnapshot {
    pub scope_label: String,
    pub capacity: usize,
    pub busy: usize,
    pub idle: usize,
    pub queued: usize,
    pub oldest_queued_secs: Option<u64>,
    pub utilization_pct: u16,
}
```

This powers Mission, Jobs, and Pools.

### P4 — cache/data isolation hardening

Add a repo-scoped cache namespace to every work item and cache metric. Family/all views aggregate only on read. Add tests proving that selecting `veox-*` sums metrics without changing per-repo cache IDs.

## Acceptance tests

- Startup selection is `ALL`.
- `veox-shared`, `veox-deploy`, and `veox-nht` produce one root family chip: `veox-*`.
- Singleton repos are still visible at root.
- Enter on `veox-*` drills into `veox-* / veox-shared / veox-deploy / veox-nht`.
- Esc from anywhere in the scope rail returns to `ALL`.
- Workflow PR rail shows only PRs matching the active scope.
- Left/right in drilled PR rail cannot select a hidden PR.
- Mouse click on PR rail cannot select a hidden PR.
- Cache namespace for `veox-shared` differs from `veox-deploy` even when viewing `veox-*`.
- Mission and Jobs counts in family/repo scope are strict subsets of ALL.

## Operator keybindings

```text
←/→ or h/l   move scope chip when fleet rail is focused/drilled
Enter        drill into family, or open detail for ALL/repo
Esc          return to ALL and close detail
/            future: scope search
u/r/s/p      future: utilization/recency/status/project sort modes
```

## Notes

The attached diff intentionally focuses on front-end scope semantics and selection correctness. The deeper provider-wide `FleetWorkItem` collector should be a second PR so it can be tested independently against GitHub, GitLab, local proof lanes, and the state store.
