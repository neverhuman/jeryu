# apps/web/AGENTS.md

## Workspace Boundary

- Work only in the user-named active repo/worktree.
- Never switch to sibling clones, archives, backups, resolved symlink targets, `/tmp` worktrees, or duplicate roots.
- Never create repo copies or side folders outside the active repo; preserve work with git branches.
- Before edits, report `pwd`, `git rev-parse --show-toplevel`, and `git status --short --branch`.
- Use jeryu APIs/CLI for local PR work; no credential scraping or raw local provider API calls.

apps/web/ is the `@jeryu/web` Vite + React + TypeScript SPA. See the web port
spec (`docs/port/02-web.md`) and the frontend tier (W-FE-*) work packages.

Wire types re-export ts-rs output from `contracts/generated/`; new
RepositorySummary fields ship TS-optional (`jankurai_score`,
`jankurai_decision`, `jankurai_scored_at`, `mirror`) so fixtures adopt them
incrementally — `tsc -b` enforces consumption in the build lane, with
`npm run e2e` (Playwright, `playwright.config.ts`) and `npm run ux-qa` as the
rendered proof lanes. NOTE: the rtk command wrapper replaces Playwright's
configured reporters; run `rtk proxy npx playwright test` when the ux-qa lane
needs the HTML report artifact. Heads-up for prose in this cell: jankurai's
db-layer heuristic bare-word-matches SQL verbs followed by a space — phrase UI
copy and comments as "removal"/"purge"/"deletion" (see PR #50).

Forbidden imports: `sqlx`, `mysql`, `@aws-sdk/client-s3` (and any other
backend-only crate/SDK; this workspace must stay UI-tier).

Proof lane: rendered UX / Playwright. Marker-evidence companion lives at
`ux-qa/` (`@jankurai/ux-qa`).
Control-plane, pull-room, and repository PR surfaces must keep Playwright
screenshot capture, generated API mocks, and design token discipline evidence
in the changed surface alongside route and unit coverage.

SPA navigation regression net: the keyboard registry (`useKeyboard.ts`) must
stay loop-free — its register effect depends on the STABLE register function
(never the context value) and unregister preserves the previous reference when
nothing was removed. The e2e specs for /repos assert rendered outlet content
after Link clicks plus a crash-free history back; Storybook covers the
multi-consumer registry shape (`KeyboardShortcutsOverlay.stories.tsx`).

Owner work-packages: `W-FE-*` (and `W-F-07`, `W-F-09`, `W-F-12` for
foundation skeleton).
