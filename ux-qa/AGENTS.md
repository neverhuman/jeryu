# apps/ux-qa/AGENTS.md

## Workspace Boundary

- Work only in the user-named active repo/worktree.
- Never switch to sibling clones, archives, backups, resolved symlink targets, `/tmp` worktrees, or duplicate roots.
- Never create repo copies or side folders outside the active repo; preserve work with git branches.
- Before edits, report `pwd`, `git rev-parse --show-toplevel`, and `git status --short --branch`.
- Use jeryu APIs/CLI for local PR work; no credential scraping or raw local provider API calls.

<!-- jankurai generated adapter -->
<!-- jankurai agent request v1 sha256:REPLACE_WITH_HASH -->
Read `AGENTS.md` first. Use `agent/JANKURAI_STANDARD.md` as the canonical jankurai standard.
When a user provides a paper, release, implementation, or handoff plan in the conversation, treat that plan as the controlling plan. Do not route such plans through the separate local phase workflow unless the user explicitly names MASTER_PLAN phase work.
This is the UX-QA marker proof workspace (npm package `@jankurai/ux-qa`); the product web app lives at `apps/web/` as `@jeryu/web`.
Owns `apps/ux-qa/` (rendered UX QA marker evidence + harness).
Forbidden: product truth, backend authority, and direct DB writes.
Proof lane: `rendered UX / Playwright` (marker check only — Playwright evidence collection itself ships from `apps/web/`).
If jankurai is installed, run `jankurai update --client-start --quiet` before work; do not apply updates unless the user asks.
