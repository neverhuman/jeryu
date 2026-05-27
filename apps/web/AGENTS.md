# apps/web/AGENTS.md

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
