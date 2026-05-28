# tui — Ratatui TUI Dashboard

## Workspace Boundary

- Work only in the user-named active repo/worktree.
- Never switch to sibling clones, archives, backups, resolved symlink targets, `/tmp` worktrees, or duplicate roots.
- Never create repo copies or side folders outside the active repo; preserve work with git branches.
- Before edits, report `pwd`, `git rev-parse --show-toplevel`, and `git status --short --branch`.
- Use Jeryu APIs/CLI for local GitLab/MR work; no `glab`, credential scraping, or raw local GitLab API calls.

## Invariants

- `run_tui_once` is the smoke-test entry point — must render without panicking on empty state.
- All tab variants must be covered in `renders_all_primary_tabs_with_empty_state`.
- No business logic — all data via `state::TuiSession` through `App::refresh_now()`.

## Proof Commands

```bash
cargo check -p jeryu --message-format=json
cargo test -p jeryu -- tui
```

Change type: `leaf-bugfix`. Promote to `cross-module` if `app.rs` state types change.
