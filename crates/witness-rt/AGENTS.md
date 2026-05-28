# witness-rt

## Workspace Boundary

- Work only in the user-named active repo/worktree.
- Never switch to sibling clones, archives, backups, resolved symlink targets, `/tmp` worktrees, or duplicate roots.
- Never create repo copies or side folders outside the active repo; preserve work with git branches.
- Before edits, report `pwd`, `git rev-parse --show-toplevel`, and `git status --short --branch`.
- Use Jeryu APIs/CLI for local GitLab/MR work; no `glab`, credential scraping, or raw local GitLab API calls.

Runtime repair packet library for agent-native Rust.

## What This Crate Does

- Installs a panic hook that emits structured `RepairPacket` JSON
- Provides `agent_ensure!`, `agent_bail!`, `agent_expect!`, `agent_ok!` macros
- Maps panic locations to owning cells via `#[track_caller]`
- Zero external dependencies beyond `serde` / `serde_json`

## Invariants

- Repair packets always include file, line, column
- The panic hook must never panic itself
- Cell matching uses path-prefix comparison against registered `owned_paths`

## Commands

```bash
cargo check -p witness-rt
cargo test -p witness-rt
cargo test -p witness-rt --doc
```
