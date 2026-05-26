# Jeryu TUI Fleet Drilldown Engineering Spec

## Goal

Make the TUI usable when the operator is watching many mixed repositories at once. The default experience stays **ALL**, but the fleet selector becomes a real scope navigator:

- **ALL**: every workflow, mission, release, approval, job, agent, test, pool, cache, evidence, bug, secret, LLM, git, and Jankurai signal that Jeryu knows about.
- **Family scopes**: automatic `<prefix>-*` groups, such as `veox-*`, derived from repo slug/name or explicit registry override.
- **Repo scopes**: one concrete repo, with all panes showing only that repo's data.

This patch intentionally treats the repo scope as a first-class filter rather than a header-only selector. The selected scope flows into workflow PR rails, job views, activity/log targeting, fleet rollups, and future state-store queries.

## Current findings from repo review

The current TUI already has a 1-row fleet bar and per-repo filter plumbing. `TuiStateSnapshot` carries a `fleet: FleetSnapshot`, `App` carries `selected_repo_index`, and the header calls `app.repo_filter()` to display the current repo filter. The workflow tab already passes that filter into `draw_delivery_tab_with_chrome`, and the PR rail has a filter-aware `pr_at_column_filtered` path.

The weakness is the information architecture: the fleet bar is a flat left-to-right list, `selected_repo_index == 0` means ALL and every other index means one repo, and there is no family-level scope. That model breaks down when the number of repos grows and when repos are really project families.

## Proposed UX

### Top chrome

The top chrome keeps the high-signal status row:

```text
jeryu repo:All(42) GitLab:OK ctrs:10 pools:3/3 rel:3eb58fcc agents:0 cache:87% ●
```

When scoped:

```text
jeryu repo:veox-* GitLab:OK ctrs:10 pools:3/3 rel:3eb58fcc agents:0 cache:87% ●
```

or:

```text
jeryu repo:veox-shared GitLab:OK ctrs:10 pools:3/3 rel:3eb58fcc agents:0 cache:87% ●
```

### Fleet bar

Replace the flat repo strip with a compact summary:

```text
ALL run:12 fail:1 aged:0 repos:42  scope:veox-* r7 f1 repos:9  hot: veox-* r7 f1 jeryu r2 f0 redlinedb r1 f0 Enter:scope ↑↓:choose Esc:all
```

Only a small “hot scopes” preview is shown inline. Hot scopes are sorted by:

1. utilization pressure, derived from running/failing/stale signals;
2. latest activity timestamp;
3. running count;
4. failed count;
5. label.

### Fleet overlay

`Enter` on the FleetBar opens a vertical overlay. This is the main navigator.

Top-level view:

```text
◇ ALL                 repos:42  run:12 fail:1 aged:0 hot:80
▣ veox-*              repos:9   run:7  fail:1 aged:0 hot:75
▣ redlinedb-*         repos:5   run:2  fail:0 aged:0 hot:40
• jeryu               repos:1   run:0  fail:0 aged:0 hot:0
```

Family drilldown view after `Enter` on `veox-*`:

```text
▣ veox-* · ALL        repos:9   run:7  fail:1 aged:0 hot:75
• veox-nht            repos:1   run:3  fail:0 aged:0 hot:60
• veox-shared         repos:1   run:2  fail:0 aged:0 hot:40
• veox-deploy         repos:1   run:1  fail:1 aged:0 hot:55
```

The right side of the overlay shows details for the selected scope. For repo rows it also shows the cache namespace and data namespace so operators can quickly verify isolation.

## Keybindings

| Key | Behavior |
|---|---|
| `Enter` on FleetBar | Open scope overlay. |
| `↑/↓` or `k/j` in overlay | Move through scopes. |
| `Enter` on family at top-level | Drill into that family. Scope becomes the family. |
| `Enter` on `family · ALL` | Close overlay and watch the whole family. |
| `Enter` on repo | Close overlay and watch only that repo. |
| `Esc` in family drill | Return to top-level fleet scopes. |
| `Esc` at top-level overlay | Close overlay and reset to ALL. |
| `Left/Right` or `h/l` | Still cycles scopes for fast one-row navigation. |

## Data model

### RepoConfig additions

```rust
family: Option<String>
cache_namespace: Option<String>
data_namespace: Option<String>
```

These allow explicit overrides but default to deterministic derivation.

### FleetRepoSnapshot additions

```rust
family: Option<String>
last_activity_at: Option<String>
cache_namespace: String
data_namespace: String
utilization_pressure: u16
```

### Scope model

```rust
FleetScopeKind::{All, Family, Repo}
FleetScopeRollup { repo_count, running_count, failed_count, aged_count, latest_activity_at, utilization_pressure }
FleetScopeItem { kind, label, family, repo_index, rollup, status, cache_namespace, data_namespace }
```

The scope list is computed from the current `FleetSnapshot`; it does not mutate repo registry data.

## Family grouping rules

1. Prefer explicit `family` from `.jeryu/repos.toml`.
2. Otherwise derive from the repo slug basename, e.g. `neverhuman/veox-shared` → `veox-*`.
3. If the slug basename has no dash, try the alias.
4. If neither has a dash, the repo is standalone.

This means aliases like `shared` still group correctly when the slug is `neverhuman/veox-shared`.

## Scope filtering rules

`RepoFilter` becomes:

```rust
All
Family { family: String }
Only { alias: String, slug: String }
```

`Family` matches a rendered item if the item alias/slug derives to the same family. Items with no repo metadata still show only in ALL to avoid leaking unrelated events into scoped views.

## Cache/data isolation

Each repo receives deterministic, separate namespaces:

```text
jeryu-cache-v1-neverhuman__veox_shared
jeryu-data-v1-neverhuman__veox_shared
```

Cache and data namespaces are separate even for the same repo. The overlay displays both. The intent is to make the UI reinforce that repos do not share cache/data unless an explicit future registry policy allows it.

Follow-up implementation should thread these namespaces into runner workspace setup, cache mounts, CAS prefixes, and any background cleanup jobs. This patch begins by surfacing namespace identity in the fleet model and UI.

## Pane behavior under scope

The scope filter should be applied consistently:

- Workflow PR rail: already filter-aware; family filter is added.
- Workflow canvas: selected PR must be coerced to a PR visible in the active scope.
- Jobs: list most recent jobs first, scoped when job metadata has repo identifiers.
- Release/Mission/Approvals: keep ALL as default; once source rows contain repo metadata, filter with the same `RepoFilter`.
- Cache: show global cache health in ALL; show namespace-local health in repo/family scope once backend metrics include namespace.
- Activity/logs: show scoped activity where metadata exists; keep unscoped items in ALL only.

## Acceptance criteria

1. Starting TUI defaults to ALL.
2. `Enter` on FleetBar opens a vertical selector, not an ever-growing horizontal list.
3. Repos named or slugged `veox-*` appear under a `veox-*` family row.
4. Selecting `veox-*` filters workflow PR rail to only `veox-*` PRs.
5. Drilling into `veox-*` shows child repos sorted by hot/recent work.
6. Selecting a child repo filters all repo-aware panes to only that repo.
7. `Esc` from a family drill goes back to top-level scopes; `Esc` at top-level returns to ALL.
8. Repo detail panel displays cache and data namespaces.
9. Scope ordering prioritizes active/failing/recent work.
10. Existing numeric tab navigation remains unchanged.

## Tests to add/run

```bash
cargo test -p jeryu --lib repo_fleet::tests::dash_repos_group_into_family_from_slug
cargo test -p jeryu --lib repo_fleet::tests::repo_filter_matches_family_without_leaking_standalone_items
cargo test -p jeryu --lib repo_fleet::tests::cache_and_data_namespaces_are_separate_per_repo
cargo test -p jeryu --lib tui::repo_fleet_bar
cargo test -p jeryu --lib tui::runtime::input::navigation::general
cargo nextest run -p jeryu -- tui
```

## Rollout plan

1. Merge the scope model and overlay behind existing TUI entrypoint; no CLI flags needed.
2. Run demo screenshot capture for `Workflow`, `Jobs`, and `Cache` tabs at 120x40 and 180x50.
3. Add real repo metadata to job/pipeline/event rows where missing.
4. Update cache/runner backends to consume `cache_namespace` and `data_namespace` from the fleet registry.
5. Add a small `f` or `/` fuzzy filter inside the overlay if the number of families grows beyond ~30.

## Known limitations of this patch

- Some panes can only be partially scoped until their source rows carry repo alias/slug metadata.
- The namespace values are surfaced in UI/model first; backends still need follow-up work to enforce these namespaces at every mount/cache boundary.
- I could inspect the repository through GitHub, but the sandbox could not clone it directly due DNS resolution failure, so this patch should be applied and validated with the test plan above before merging.
