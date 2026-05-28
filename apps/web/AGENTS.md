# apps/web/AGENTS.md

## Workspace Boundary

- Work only in the user-named active repo/worktree.
- Never switch to sibling clones, archives, backups, resolved symlink targets, `/tmp` worktrees, or duplicate roots.
- Never create repo copies or side folders outside the active repo; preserve work with git branches.
- Before edits, report `pwd`, `git rev-parse --show-toplevel`, and `git status --short --branch`.
- Use Jeryu APIs/CLI for local GitLab/MR work; no `glab`, credential scraping, or raw local GitLab API calls.

apps/web/ is the `@jeryu/web` Vite + React + TypeScript SPA per
`/home/ubuntu/jeryu/WEB_WORK_CLAUDE.md`. See §7.4 frontend tier (W-FE-*)
for work packages.

Forbidden imports per `agent/boundaries.toml`: `sqlx`, `mysql`,
`@aws-sdk/client-s3` (and any other backend-only crate/SDK; this
workspace must stay UI-tier).

Proof lane: rendered UX / Playwright. Marker-evidence companion lives at
`apps/ux-qa/` (`@jankurai/ux-qa`).

Owner work-packages: `W-FE-*` (and `W-F-07`, `W-F-09`, `W-F-12` for
foundation skeleton).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
