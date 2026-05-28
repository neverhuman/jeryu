# arc-bench

## Workspace Boundary

- Work only in the user-named active repo/worktree.
- Never switch to sibling clones, archives, backups, resolved symlink targets, `/tmp` worktrees, or duplicate roots.
- Never create repo copies or side folders outside the active repo; preserve work with git branches.
- Before edits, report `pwd`, `git rev-parse --show-toplevel`, and `git status --short --branch`.
- Use Jeryu APIs/CLI for local GitLab/MR work; no `glab`, credential scraping, or raw local GitLab API calls.

Use benchmarks here to make claims testable.

- Prefer simple, explainable scenarios over synthetic complexity.
- Report tradeoffs honestly; not every benchmark should crown one absolute winner.
- Keep output JSON stable so CI and the paper can diff it cleanly.
