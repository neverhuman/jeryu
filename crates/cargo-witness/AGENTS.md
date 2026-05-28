# cargo-witness

## Workspace Boundary

- Work only in the user-named active repo/worktree.
- Never switch to sibling clones, archives, backups, resolved symlink targets, `/tmp` worktrees, or duplicate roots.
- Never create repo copies or side folders outside the active repo; preserve work with git branches.
- Before edits, report `pwd`, `git rev-parse --show-toplevel`, and `git status --short --branch`.
- Use Jeryu APIs/CLI for local GitLab/MR work; no `glab`, credential scraping, or raw local GitLab API calls.

Witness graph and repair routing for agent-native Rust workspaces.

## What This Crate Does

- `cargo witness build` — generates `.witness/witness-graph.json` with dual hashes per crate
- `cargo witness diff <prior> <new>` — classifies changes as interface vs implementation
- `cargo witness diagnose` — routes `cargo check` errors to owning ARCs
- `cargo witness repair` — assembles minimal repair bundles from failure packets

## Invariants

- Interface hashes capture all `pub` item signatures via `syn` parsing
- Implementation hashes exclude pub signatures
- Compile diagnostics are always routed to an owning ARC
- Repair bundles contain minimal sufficient context

## Commands

```bash
cargo check -p cargo-witness
cargo test -p cargo-witness
cargo test -p cargo-witness --doc
cargo run -p cargo-witness -- build
cargo run -p cargo-witness -- diagnose
```
