# cargo-vrc

## Workspace Boundary

- Work only in the user-named active repo/worktree.
- Never switch to sibling clones, archives, backups, resolved symlink targets, `/tmp` worktrees, or duplicate roots.
- Never create repo copies or side folders outside the active repo; preserve work with git branches.
- Before edits, report `pwd`, `git rev-parse --show-toplevel`, and `git status --short --branch`.
- Use Jeryu APIs/CLI for local GitLab/MR work; no `glab`, credential scraping, or raw local GitLab API calls.

Keep this crate deterministic and explainable.

- Prefer explicit metadata fields over inferred magic.
- Every plan must explain why a ring was selected or skipped.
- Avoid over-promising precision when only heuristics are available.
