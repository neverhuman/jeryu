# Reusable-tool registry

`jeryu-tool` has two jobs. The first is the jankurai **audit** control plane
(`tool-manifest.toml`). The second — this document — is the family's
**reusable-code-tool registry**: the catalogue of shared crates, TypeScript
libraries, React components, Vite plugins, and shell libraries that exist so
repos stop copy-pasting the same code.

The registry is declarative. `jeryu-tool` still ships no product code: it owns
the *list* of tools and the *build queue*, while the tools themselves live in
their canonical home repos and the discovery lives in `jeryu-tool-finder`.

## Files

| Path | Purpose |
|---|---|
| `tools-registry.toml` | The single source of truth for reusable tools: one `[[tool]]` per tool, with adoption and LOC-saved bookkeeping. |
| `tasks/NNNN-*.toml` | The build queue. One file per "build this tool / migrate these repos" task. |
| `ops/registry-summary.sh` | Runs the locked Rust validator for both inputs and computes the golden-box summary. `--check` is wired into `just check`. |

## The loop

```
jeryu-tool-finder scans all repos
        │  (cross-repo repeated-code clusters)
        ▼
  dossier per cluster  ──►  an agent decides: worth a tool?
        │ yes
        ▼
  [[tool]] entry (status=proposed) + tasks/NNNN-*.toml   ← here
        │  build it
        ▼
  status=building → published ; repos migrate
        │
        ▼
  candidate_repos → adopting_repos ; loc_saved grows
        │
        ▼
  forge golden box on /repos shows tools, adopters, LOC saved
```

## `[[tool]]` fields

| Field | Meaning |
|---|---|
| `id` | Stable kebab-case identifier, unique across the registry. |
| `name` | Human label shown in the golden box. |
| `kind` | One of `rust-crate`, `ts-lib`, `react-component`, `vite-plugin`, `shell-lib`, `jankurai-tool`. |
| `status` | `proposed` → `building` → `published` (→ `deprecated`). |
| `source` | Canonical home of the tool (`repo/crate` or `repo/path`). Required once `published`. |
| `description` | What the tool is / what duplication it replaces. |
| `origin_cluster` | The `jeryu-tool-finder` cluster id that motivated it (provenance, optional). |
| `adopting_repos` | Repos that already use the tool. |
| `candidate_repos` | Repos that still carry their own copy and should adopt. |
| `loc_saved` | **Realized** lines removed, summed across adopters. |
| `loc_saved_estimate` | **Anticipated** lines still to be removed once the candidates migrate. |

## `tasks/` fields

`id`, `tool_id` (must match a registered tool), `title`, `status`
(`open` | `in-progress` | `done`), `origin_cluster`, `anticipated_loc_saved`,
`target_repos`, and a `rollout` list of per-repo migration steps.

## LOC-saved definition

LOC numbers are **conservative, window-scoped lower bounds** derived from the
finder's clusters. For a cluster whose normalized window appears `n` times
across the family, the anticipated saving is `(n − 1) × window_lines` — the
lines removed by collapsing all but one copy. `loc_saved_estimate` starts at the
cluster estimate and shrinks as `loc_saved` (realized) grows during migration;
their sum is the total opportunity. The extracted tool itself adds a small fixed
cost that is intentionally not modelled — the registry tracks *removed
duplication*, not net diff.

## Summary surface

`ops/registry-summary.sh` (and the forge handler
`GET /api/v1/tools/registry/summary`) aggregate the registry into:
`tool_count`, per-status counts, distinct `adopting_repo_count` /
`candidate_repo_count`, `open_task_count`, and `realized_loc_saved` /
`anticipated_loc_saved`. The Rust handler and this script read the same TOML, so
keep their (trivial) aggregation in sync.
