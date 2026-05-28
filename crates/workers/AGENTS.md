# crates/workers/AGENTS.md

## Workspace Boundary

- Work only in the user-named active repo/worktree.
- Never switch to sibling clones, archives, backups, resolved symlink targets, `/tmp` worktrees, or duplicate roots.
- Never create repo copies or side folders outside the active repo; preserve work with git branches.
- Before edits, report `pwd`, `git rev-parse --show-toplevel`, and `git status --short --branch`.
- Use Jeryu APIs/CLI for local GitLab/MR work; no `glab`, credential scraping, or raw local GitLab API calls.

<!-- jankurai generated adapter -->
<!-- jankurai agent request v1 sha256:REPLACE_WITH_HASH -->
Read `AGENTS.md` first. Use `agent/JANKURAI_STANDARD.md` as the canonical jankurai standard.
When a user provides a paper, release, implementation, or handoff plan in the conversation, treat that plan as the controlling plan. Do not route such plans through the separate local phase workflow unless the user explicitly names MASTER_PLAN phase work.
Owns `crates/workers/`.
Forbidden: request handling, UI behavior, and direct user flow ownership.
Proof lane: `workflow / replay tests`.
If jankurai is installed, run `jankurai update --client-start --quiet` before work; do not apply updates unless the user asks.
