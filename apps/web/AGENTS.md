# web/AGENTS.md

## Workspace Boundary

- Work only in the user-named active repo/worktree.
- Never switch to sibling clones, archives, backups, resolved symlink targets, `/tmp` worktrees, or duplicate roots.
- Never create repo copies or side folders outside the active repo; preserve work with git branches.
- Before edits, report `pwd`, `git rev-parse --show-toplevel`, and `git status --short --branch`.
- Use jeryu APIs/CLI for local PR work; no credential scraping or raw local provider API calls.

web/ is the `@jeryu/web` Vite + React + TypeScript SPA. See the web port
spec (`docs/port/02-web.md`) and the frontend tier (W-FE-*) work packages.

Forbidden imports: `sqlx`, `mysql`, `@aws-sdk/client-s3` (and any other
backend-only crate/SDK; this workspace must stay UI-tier).

Proof lane: rendered UX / Playwright. Marker-evidence companion lives at
`ux-qa/` (`@jankurai/ux-qa`).
Control-plane, pull-room, and repository PR surfaces must keep Playwright
screenshot capture, generated API mocks, and design token discipline evidence
in the changed surface alongside route and unit coverage.

Owner work-packages: `W-FE-*` (and `W-F-07`, `W-F-09`, `W-F-12` for
foundation skeleton).
