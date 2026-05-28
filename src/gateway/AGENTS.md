# gateway — Registry Proxy (Cargo / Git / npm / OCI)

## Workspace Boundary

- Work only in the user-named active repo/worktree.
- Never switch to sibling clones, archives, backups, resolved symlink targets, `/tmp` worktrees, or duplicate roots.
- Never create repo copies or side folders outside the active repo; preserve work with git branches.
- Before edits, report `pwd`, `git rev-parse --show-toplevel`, and `git status --short --branch`.
- Use Jeryu APIs/CLI for local GitLab/MR work; no `glab`, credential scraping, or raw local GitLab API calls.

## Invariants

- All proxy modules route through `singleflight` for concurrent request deduplication.
- Never cache registry auth credentials — proxy response bodies only.

## Proof Commands

```bash
cargo check -p jeryu --message-format=json
cargo test -p jeryu -- gateway
```

Change type: `leaf-bugfix`. Promote to `api-change` if proxy endpoint config changes.
