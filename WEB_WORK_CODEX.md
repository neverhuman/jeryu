# JeRyu Full Web Forge Work Plan

Status: implementation handoff plan only. No feature code is included here.

This plan is the controlling implementation plan for adding a full
Rust/Axum plus Vite/TypeScript/React web forge to JeRyu. It synthesizes the
material under `tips/web/*` and the current repository baseline inspected on
2026-05-26.

The desired product is a full GitHub/GitLab-class web experience, but with
JeRyu's stronger operational model: faster navigation, safer mutations,
server-owned Markdown rendering, real-time typed events, agent evidence,
explicit merge gates, and searchable settings.

## 1. Baseline Facts

Current baseline verified by Codex:

- `cargo check -p jeryu --message-format=json` passes on the current checkout.
- `apps/web` is currently only the `@jankurai/ux-qa` marker workspace. It is not
  a React app and has no Vite config, no routes, no browser UI, and no
  Playwright tests.
- Root `package.json` currently declares only `"apps/web"` as an npm workspace
  and only has UX-QA scripts.
- No npm lockfile currently exists in the repo root.
- Existing engine routes are `/health`, `/hooks`, and `/cache/summary`.
  These routes must keep their behavior.
- Existing API foundation lives in `src/api`.
- Existing provider foundation lives in `src/git_host`.
- Existing state store authority is SQLite by default through SQLx Any.
  RedlineDB is opt-in only through the explicit `redlinedb-backend` feature.
- DB and SQLx wiring must remain confined to `db/`, `src/db/`, and Cargo
  backend feature config.
- Several `tips/web/*.diff` files are conceptual or malformed. They are useful
  references, but agents must not apply them directly.

Important repo instructions:

- Read `AGENTS.md` and `agent/JANKURAI_STANDARD.md` before implementation.
- Do not route this work through `agent/MASTER_PLAN.md` or phase files unless a
  user explicitly asks for MASTER_PLAN/phase work.
- Generated artifacts must stay under their declared source commands.
- For changed paths, use the proof lane in `agent/test-map.json`.

## 2. Product Goal

Build a local-first web forge where users can:

- See all accessible repositories.
- Group repositories by owner, provider, family, health, and activity.
- Create, import, adopt, mirror, archive, and configure repositories.
- Open a repository home page with correctly rendered `README.md`.
- Browse branches, tags, commits, trees, blobs, raw files, history, blame, and
  comparisons.
- Render Markdown files as sanitized HTML with GitHub-like behavior.
- Review merge requests/pull requests in one Merge Room.
- Comment inline, submit reviews, approve exact SHAs, and merge only when gates
  pass.
- Manage issues, labels, milestones, notifications, CI, agents, evidence,
  webhooks, secrets metadata, branch protections, and settings.
- Receive real-time updates through WebSocket without manual refresh.
- Validate the full UX with Playwright, accessibility checks, screenshots, ARIA
  snapshots, and jankurai UX proof artifacts.

## 3. Non-Negotiable Architecture

### Rust Is The Authority

Rust owns:

- Authentication and sessions.
- Authorization and provider permission mapping.
- GitHub/GitLab/local provider calls.
- Repository lifecycle operations.
- Markdown parsing, link rewriting, and sanitization.
- Raw/blob access and path safety.
- Action preview and execution.
- Exact-SHA merge and approval safety.
- Durable state, audit records, action receipts, and WebSocket replay.
- Static asset serving and BFF routes.

React owns:

- Presentation.
- Route-level UI state.
- Keyboard navigation.
- Command palette state.
- Optimistic local presentation where backed by server events.
- Client-side defense-in-depth sanitization for already sanitized Markdown HTML.

React must not:

- Call GitHub/GitLab directly.
- Own permissions or security decisions.
- Own Markdown sanitization as the only defense.
- Store secrets or sensitive provider tokens in local storage.
- Execute mutations outside preview/execute contracts.

### API Shape

Mount browser APIs under `/api/v1`. Keep existing `/health`, `/hooks`, and
`/cache/summary` stable.

Preferred endpoints:

```text
GET    /api/v1/bootstrap
GET    /api/v1/repos
POST   /api/v1/repos/preview
POST   /api/v1/repos
POST   /api/v1/repos/import/preview
POST   /api/v1/repos/import
GET    /api/v1/repos/{repo_id}
PATCH  /api/v1/repos/{repo_id}
POST   /api/v1/repos/{repo_id}/archive
DELETE /api/v1/repos/{repo_id}

GET    /api/v1/repos/{repo_id}/refs
GET    /api/v1/repos/{repo_id}/branches
POST   /api/v1/repos/{repo_id}/branches
GET    /api/v1/repos/{repo_id}/tags
POST   /api/v1/repos/{repo_id}/tags
GET    /api/v1/repos/{repo_id}/commits
GET    /api/v1/repos/{repo_id}/commits/{sha}
GET    /api/v1/repos/{repo_id}/compare

GET    /api/v1/repos/{repo_id}/tree?ref=&path=
GET    /api/v1/repos/{repo_id}/blob?ref=&path=&render=
GET    /api/v1/repos/{repo_id}/raw?ref=&path=
GET    /api/v1/repos/{repo_id}/readme?ref=
GET    /api/v1/repos/{repo_id}/history?ref=&path=
GET    /api/v1/repos/{repo_id}/blame?ref=&path=
POST   /api/v1/markdown/render

GET    /api/v1/repos/{repo_id}/issues
POST   /api/v1/repos/{repo_id}/issues
GET    /api/v1/repos/{repo_id}/issues/{iid}
PATCH  /api/v1/repos/{repo_id}/issues/{iid}
POST   /api/v1/repos/{repo_id}/issues/{iid}/comments

GET    /api/v1/repos/{repo_id}/merge-requests
POST   /api/v1/repos/{repo_id}/merge-requests
GET    /api/v1/repos/{repo_id}/merge-requests/{iid}
PATCH  /api/v1/repos/{repo_id}/merge-requests/{iid}
GET    /api/v1/repos/{repo_id}/merge-requests/{iid}/diff
GET    /api/v1/repos/{repo_id}/merge-requests/{iid}/checks
GET    /api/v1/repos/{repo_id}/merge-requests/{iid}/blockers
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/comments
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/reviews
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/approve
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/request-changes
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/merge
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/rebase
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/close
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/reopen

GET    /api/v1/repos/{repo_id}/pipelines
GET    /api/v1/repos/{repo_id}/pipelines/{pipeline_id}
GET    /api/v1/repos/{repo_id}/jobs/{job_id}/log
POST   /api/v1/repos/{repo_id}/jobs/{job_id}/retry
POST   /api/v1/repos/{repo_id}/jobs/{job_id}/cancel

GET    /api/v1/repos/{repo_id}/settings
PATCH  /api/v1/repos/{repo_id}/settings
GET    /api/v1/repos/{repo_id}/members
PUT    /api/v1/repos/{repo_id}/members/{principal_id}
DELETE /api/v1/repos/{repo_id}/members/{principal_id}
GET    /api/v1/repos/{repo_id}/protection
PATCH  /api/v1/repos/{repo_id}/protection
GET    /api/v1/repos/{repo_id}/secrets
POST   /api/v1/repos/{repo_id}/secrets
POST   /api/v1/repos/{repo_id}/secrets/{secret_name}/rotate
DELETE /api/v1/repos/{repo_id}/secrets/{secret_name}

POST   /api/v1/actions/preview
POST   /api/v1/actions/execute
GET    /api/v1/activity
GET    /api/v1/ws
```

Use stable `repo_id` in API paths. Frontend routes may still show
provider/owner/name, but must resolve to `repo_id` through bootstrap/list data.
This avoids ambiguity for GitLab nested group paths.

### WebSocket Model

Use REST snapshots plus WebSocket deltas.

Requirements:

- Endpoint: `GET /api/v1/ws`.
- Every connection starts with server `hello`.
- Client sends `hello` with last seen cursor and subscriptions.
- Events have monotonic global `seq`.
- Client reducers are idempotent by `seq`.
- Server replays from durable event store where possible.
- Server sends `snapshot_required` when a replay gap cannot be covered.
- Heartbeat every 15 seconds.
- WebSocket subscription authorization repeats server permission checks.
- Backpressure policy drops low-priority activity first; never drop action
  results, audit/security events, or direct mutation receipts.

Client frames:

```text
hello { resume_from, subscriptions }
subscribe { subscriptions }
unsubscribe { scopes }
ack { seq }
ping { nonce }
```

Server frames:

```text
hello { server_time, current_seq, protocol }
event { event }
snapshot_required { reason, current_seq }
pong { nonce, server_time }
error { code, message, request_id }
```

Core topics:

```text
global.activity
system.health
user.{user_id}.notifications
repo.{repo_id}
repo.{repo_id}.activity
repo.{repo_id}.refs
repo.{repo_id}.checks
repo.{repo_id}.settings
repo.{repo_id}.issues
repo.{repo_id}.merge_requests
mr.{mr_id}
issue.{issue_id}
agent.{agent_id}
runner.{runner_id}
cache.{repo_id}
```

### Action Safety

Every mutation must follow:

```text
authenticate
resolve viewer
resolve target
check normalized permission
validate CSRF or bearer mode
validate schema
load current state
validate expected state hash or expected SHA
produce preview for medium/high-risk actions
require idempotency key for create/merge/delete/settings
execute provider/local state change
write audit receipt
write durable web event
broadcast websocket event
return updated read model or action receipt
```

Approval and merge actions:

1. UI sends `expected_head_sha` shown during preview.
2. Backend refetches live MR/PR state.
3. Backend rejects with `409 merge_sha_stale` if the live head differs.
4. Backend checks merge gates immediately before write.
5. Backend calls provider using exact SHA where supported.
6. Backend writes audit receipt and emits event.

## 4. Target Tree

The intended final tree is:

```text
apps/
  ux-qa/
    AGENTS.md
    package.json
    ux-qa-check.mjs
    ux-qa.md
    ux-qa.ts
  web/
    AGENTS.md
    README.md
    package.json
    package-lock.json
    index.html
    vite.config.ts
    tsconfig.json
    playwright.config.ts
    src/
      main.tsx
      app/
      api/
      layout/
      pages/
      components/
      features/
      hooks/
      stores/
      styles/
      test/
    e2e/
    stories/

src/
  api/
    repository.rs
    repo_browser.rs
    markdown.rs
    merge_request.rs
    review.rs
    settings.rs
    websocket.rs
    web_read_model.rs
  web/
    mod.rs
    command.rs
    state.rs
    router.rs
    error.rs
    auth.rs
    csrf.rs
    static_assets.rs
    ws.rs
    rest/
      bootstrap.rs
      repos.rs
      repo_browser.rs
      merge_requests.rs
      reviews.rs
      settings.rs
      actions.rs
      activity.rs
  web_events/
    mod.rs
    protocol.rs
    bus.rs
    store.rs
    topics.rs
  repos/
    mod.rs
    service.rs
    permissions.rs
    settings.rs
  repo_browser/
    mod.rs
    service.rs
    tree.rs
    blob.rs
    markdown.rs
    diff.rs
    blame.rs
  merge/
    mod.rs
    service.rs
    review.rs
    merge_gate.rs
  db/
    web_forge_repo.rs

db/
  migrations/
    202606010001_web_forge_core.sql

docs/
  WEB_FORGE.md
  WEB_FORGE_API.md
  README_RENDERING.md
  WEBSOCKET_PROTOCOL.md
```

## 5. Claimable Work Packages

Each work package below is intended to be claimable by a separate agent. Agents
must coordinate on shared files (`Cargo.toml`, root `package.json`, `src/lib.rs`,
`src/api/mod.rs`, `src/cli_defs.rs`, `src/dispatch.rs`, `agent/test-map.json`)
before editing.

### W0 - Workspace Split And Frontend Tooling

Can start immediately.

Owner paths:

- `apps/web/`
- `apps/ux-qa/`
- `package.json`
- `package-lock.json`
- `agent/test-map.json`

Tasks:

- Move the current UX-QA workspace from `apps/web` to `apps/ux-qa`.
- Preserve package name `@jankurai/ux-qa`.
- Add `apps/ux-qa/AGENTS.md` by adapting existing `apps/web/AGENTS.md`.
- Create a new `apps/web` package named `@jeryu/web`.
- Add Vite, React, TypeScript, Vitest, Testing Library, Playwright, Storybook,
  axe, MSW, TanStack Query, React Router, Zustand, Zod, DOMPurify, and
  `lucide-react`.
- Add `apps/web/index.html`, `vite.config.ts`, `tsconfig.json`,
  `playwright.config.ts`, and empty app entrypoint.
- Update root workspaces to `["apps/web", "apps/ux-qa"]`.
- Add root scripts:
  - `web:dev`
  - `web:build`
  - `web:typecheck`
  - `web:lint`
  - `web:test`
  - `web:e2e`
  - `ux-qa`
- Commit `package-lock.json`.
- Update proof routing so `apps/web` routes to rendered UX/Playwright evidence
  and `apps/ux-qa` routes to marker proof.

Acceptance:

```bash
npm --workspace @jankurai/ux-qa run build
npm --workspace @jankurai/ux-qa run test
npm --workspace @jeryu/web run typecheck
npm --workspace @jeryu/web run build
```

### W1 - Rust Web Contracts And Type Export

Can start after or alongside W0.

Owner paths:

- `src/api/`
- `schemas/`
- `apps/web/src/api/`

Tasks:

- Add typed DTOs for repositories, refs, tree entries, blobs, Markdown render
  output, merge requests, reviews, settings, bootstrap, actions, and WebSocket
  frames.
- Add OpenAPI/JSON-schema export using `utoipa` or `schemars`.
- Generate TypeScript into `apps/web/src/api/generated.ts`.
- Add hand-written Zod guards for WebSocket frames and mutation responses.
- Record the generator command if generated files are committed.

Minimum DTO groups:

- `RepositoryId`
- `RepositorySummary`
- `RepositoryListResponse`
- `CreateRepositoryRequest`
- `CreateRepositoryPreview`
- `RefSelectorItem`
- `TreeEntry`
- `BlobResponse`
- `RenderedMarkdown`
- `MarkdownHeading`
- `MarkdownLink`
- `MergeRequestSummary`
- `Mergeability`
- `ReviewPosture`
- `CheckPosture`
- `RepositorySettings`
- `WebBootstrap`
- `Viewer`
- `WebFeatureFlags`
- `ClientWsMessage`
- `ServerWsMessage`
- `WebEvent`

Acceptance:

```bash
cargo check -p jeryu --message-format=json
cargo test -p jeryu --lib api
npm --workspace @jeryu/web run typecheck
```

### W2 - DB Boundary And Durable Web State

Depends on W1 DTO shape.

Owner paths:

- `src/db/web_forge_repo.rs`
- `src/db/mod.rs`
- `db/migrations/`
- `db/state.rs` if schema installation is added there

Tasks:

- Add typed `WebForgeRepo` under `src/db/`.
- Add idempotent schema installation for:
  - `web_repositories`
  - `web_repo_refs`
  - `web_blob_cache`
  - `web_markdown_cache`
  - `web_memberships`
  - `web_branch_protections`
  - `web_merge_requests`
  - `web_review_threads`
  - `web_review_comments`
  - `web_review_submissions`
  - `web_settings_snapshots`
  - `web_action_receipts`
  - `web_events`
  - `web_notifications`
  - `web_audit_events`
- Add marker migration `db/migrations/202606010001_web_forge_core.sql` matching
  the intended schema for audit/routing visibility.
- Keep all SQLx usage in the DB boundary.
- Add typed methods for repo list/upsert, Markdown cache, event append/replay,
  audit insert, settings snapshot, action receipt, and MR/review cache.
- Add migration idempotency tests using in-memory SQLite.

Acceptance:

```bash
cargo test -p jeryu --lib state
cargo test -p jeryu --lib db::web_forge_repo
cargo test -p jeryu --lib state_backend_detects_supported_urls -- --test-threads=1
```

### W3 - Web BFF Skeleton And CLI

Depends on W1, can use fixtures before W2 is complete.

Owner paths:

- `src/web/`
- `src/cli_defs.rs`
- `src/cli_defs_web.rs`
- `src/dispatch.rs`
- `src/lib.rs`
- `src/engine.rs`
- `Cargo.toml`

Tasks:

- Add web dependencies: Axum WebSocket feature, tower/tower-http static assets,
  compression, request ID, timeout, `mime_guess`, `headers`, `bytes`,
  `tokio-stream`, `comrak`, `ammonia`, and schema dependencies.
- Add `pub mod web;`, `pub mod web_events;`, `pub mod repos;`,
  `pub mod repo_browser;`, and `pub mod merge;`.
- Add `jeryu web serve`.
- CLI args:
  - `--bind`, default `127.0.0.1:8787`
  - `--open`
  - `--dev-assets`, optional Vite URL
- Add `WebState`.
- Add router mounted under `/api/v1`.
- Add static asset serving from `apps/web/dist`.
- Add dev asset mode that forwards or redirects to the Vite dev server.
- Integrate with existing engine while preserving `/health`, `/hooks`, and
  `/cache/summary`.
- Add typed error envelope:

```json
{
  "error": {
    "code": "merge_sha_stale",
    "message": "The source branch changed after approval.",
    "details": {},
    "request_id": "...",
    "event_cursor": 1234
  }
}
```

Acceptance:

```bash
cargo check -p jeryu --message-format=json
cargo run -p jeryu -- web serve --bind 127.0.0.1:8787
curl http://127.0.0.1:8787/health
curl http://127.0.0.1:8787/api/v1/bootstrap
```

### W4 - Auth, RBAC, CSRF, And Action Preview

Depends on W1 and W3.

Owner paths:

- `src/web/auth.rs`
- `src/web/csrf.rs`
- `src/repos/permissions.rs`
- `src/web/rest/actions.rs`
- `src/api/actions.rs`

Tasks:

- Implement local trusted mode for development.
- Implement bearer token/session-shaped auth for production extension.
- Add CSRF requirement for cookie-authenticated mutations.
- Normalize provider roles into permissions.
- Permission set:
  - `repo.read`
  - `repo.create`
  - `repo.write`
  - `repo.admin`
  - `repo.delete`
  - `code.read`
  - `code.write`
  - `branch.create`
  - `branch.delete`
  - `settings.read`
  - `settings.write`
  - `mr.read`
  - `mr.write`
  - `mr.comment`
  - `mr.review`
  - `mr.approve`
  - `mr.merge`
  - `issue.read`
  - `issue.write`
  - `ci.read`
  - `ci.write`
  - `secrets.read_metadata`
  - `secrets.write`
  - `agents.read`
  - `agents.write`
  - `agents.grant`
  - `audit.read`
  - `admin.audit`
- Add action preview and execute endpoints.
- Require `Idempotency-Key` for create, merge, delete, archive, settings, and
  secrets mutations.
- Persist action receipts and audit events through W2 repo methods.

Acceptance:

```bash
cargo test -p jeryu --test web_api_tests -- --test-threads=1
cargo test -p jeryu --lib api::actions
```

### W5 - Durable WebSocket Event Hub

Depends on W1, W2, W3.

Owner paths:

- `src/web_events/`
- `src/web/ws.rs`
- `src/api/websocket.rs`
- `apps/web/src/api/ws.ts`
- `apps/web/src/stores/realtimeStore.ts`

Tasks:

- Add durable event append/replay through `WebForgeRepo`.
- Add broadcast bus for active connections.
- Implement authorized topic subscription.
- Implement replay from cursor.
- Implement `snapshot_required` when cursor is too old or broadcast lag occurs.
- Add heartbeat and ping/pong.
- Add client store with reconnect and last-seen cursor.
- Add duplicate event suppression in frontend reducer.
- Mark affected route stale and refetch when gap is detected.

Acceptance:

```bash
cargo test -p jeryu --test web_ws_tests -- --test-threads=1
npm --workspace @jeryu/web run test -- websocket
```

### W6 - Markdown And README Rendering

Depends on W1 and W2.

Owner paths:

- `src/repo_browser/markdown.rs`
- `src/web/rest/repo_browser.rs`
- `src/api/markdown.rs`
- `apps/web/src/components/markdown/MarkdownRenderer.tsx`
- `tests/web_markdown_tests.rs`

Tasks:

- Parse Markdown with `comrak`.
- Enable GFM tables, task lists, strikethrough, autolinks, footnotes, heading
  anchors, and fenced code.
- Sanitize with `ammonia`.
- Rewrite relative links to JeRyu repo routes.
- Rewrite relative images to authenticated raw endpoints.
- Include external link policy: `rel="noopener noreferrer"`.
- Cache rendered output by repo, ref SHA, path, blob SHA/source hash, renderer
  version, and sanitizer version.
- README lookup order:
  - `README.md`
  - `README.markdown`
  - `README.mdown`
  - `README.txt`
  - case-insensitive variants
- Treat RST as source/download in v1 unless a safe renderer is explicitly added.
- Frontend runs DOMPurify before `dangerouslySetInnerHTML`.
- No component except `MarkdownRenderer` may use `dangerouslySetInnerHTML` for
  user content.

Acceptance:

```bash
cargo test -p jeryu --test web_markdown_tests -- --test-threads=1
npm --workspace @jeryu/web run test -- markdown
```

Required test cases:

- Heading anchors are stable.
- Tables render.
- Task lists render.
- Fenced code renders with language classes.
- Relative links rewrite.
- Relative images rewrite.
- `script`, event handlers, `javascript:`, unsafe SVG, iframe, object, embed,
  form, and style attributes are stripped.
- Binary blobs are rejected by renderer.

### W7 - Git Provider Expansion And Repo Service

Depends on W1, W2, W4.

Owner paths:

- `src/git_host/mod.rs`
- `src/git_host/github.rs`
- `src/git_host/gitlab.rs`
- `src/repos/`
- `src/web/rest/repos.rs`

Tasks:

- Extend `GitHost` with default `HostError::NotImplemented` methods for:
  - list repositories
  - get repository
  - create repository
  - update repository settings
  - list refs
  - list tree
  - get blob
  - get README
  - list branches/tags
  - list MRs/PRs
  - get MR/PR
  - list review threads
  - create review comment
  - submit review
  - merge MR/PR
  - branch protection
  - webhooks
  - secrets metadata
- Implement local/git2 or GitLab first, then GitHub where existing adapter
  support is strongest.
- Add `RepoService` for list/search/filter/group/create/import/adopt/mirror.
- Add repo families by explicit setting and naming pattern.
- All create/import/archive/delete calls must go through W4 action preview.

Acceptance:

```bash
cargo test -p jeryu --lib git_host::
cargo test -p jeryu --test web_api_tests -- --test-threads=1
```

### W8 - Frontend Shell, Routes, And Design System

Depends on W0 and W1. Can use mocked API.

Owner paths:

- `apps/web/src/app/`
- `apps/web/src/layout/`
- `apps/web/src/styles/`
- `apps/web/src/components/command/`
- `apps/web/src/components/realtime/`

Tasks:

- Add app providers: QueryClient, router, realtime store, preferences store.
- Add AppShell:
  - global header
  - repo switcher
  - command palette
  - left nav
  - live activity rail
  - status bar
  - toast center
  - keyboard help dialog
- Add route map:
  - `/`
  - `/repos`
  - `/repos/new`
  - `/repos/:provider/*fullName`
  - `/repos/:provider/*fullName/code`
  - `/repos/:provider/*fullName/blob/*`
  - `/repos/:provider/*fullName/merge-requests`
  - `/repos/:provider/*fullName/merge-requests/:iid`
  - `/repos/:provider/*fullName/issues`
  - `/repos/:provider/*fullName/settings/:section?`
  - `/merge-room`
  - `/notifications`
  - `/audit`
  - `/settings`
- Use `lucide-react` icons.
- Use restrained operational design. No landing page and no marketing hero.
- Add stable layout dimensions for sidebars, toolbars, trees, status pills, and
  buttons.
- Add command registry with ID, title, keywords, icon, permission, route/action,
  context predicate, shortcut, and risk tier.

Acceptance:

```bash
npm --workspace @jeryu/web run typecheck
npm --workspace @jeryu/web run test
npm --workspace @jeryu/web run build
```

### W9 - Repositories Dashboard And Repo Home

Depends on W6, W7, W8.

Owner paths:

- `src/web/rest/repos.rs`
- `apps/web/src/pages/RepositoriesPage.tsx`
- `apps/web/src/pages/RepositoryOverviewPage.tsx`
- `apps/web/src/components/repo/`
- `apps/web/src/components/browser/ReadmePanel.tsx`

Tasks:

- Implement dashboard search by name, description, owner, family, topic,
  language, active agent, failed check, blocker.
- Filters: owned, starred, recent, archived, private, public, local, GitHub,
  GitLab, mirrored, dirty, blocked, active CI, active agents.
- Grouping: owner/org, family, provider, health, last activity.
- Cards/table show: name, description, default branch, latest commit, CI status,
  open MRs/issues, active agents, blockers, unread notifications.
- Quick actions: create repo, import, adopt local checkout, open, clone,
  settings, run health check.
- Bulk actions: sync, mirror, archive, apply standard, run repo audit, update
  settings template.
- Repo home sections:
  - summary header
  - clone URL popover
  - branch selector
  - live health strip
  - README card
  - latest commit
  - open MRs and blockers
  - active issues and milestones
  - CI/test/pipeline summary
  - agent activity and evidence capsules
  - recent activity timeline

Acceptance:

- User can see all repos.
- User can create a private repo with README through preview.
- User can open repo overview and see sanitized README HTML.
- WebSocket event updates activity without refresh.

### W10 - Code Browser

Depends on W6, W7, W8.

Owner paths:

- `src/repo_browser/`
- `src/web/rest/repo_browser.rs`
- `apps/web/src/pages/RepositoryCodePage.tsx`
- `apps/web/src/pages/RepositoryFilePage.tsx`
- `apps/web/src/components/browser/`

Tasks:

- Implement refs, tree, blob, raw, history, blame, and compare endpoints.
- Normalize requested paths. Reject `..`, absolute paths, NUL bytes, and
  symlink escapes.
- Implement branch/tag selector.
- Implement breadcrumbs.
- Implement virtualized file tree.
- Implement go-to-file and tree search.
- Implement code viewer with line anchors and copy selected lines permalink.
- Implement Markdown rendered/source toggle.
- Implement binary/image preview. SVG must be sanitized or download-only by
  default.
- Implement large file fallback and raw/download controls.
- Implement copy path, copy permalink, copy raw URL, download, view raw,
  blame, history, compare.

Acceptance:

- User browses branches and files.
- Markdown files render safely.
- Binary file does not render as text.
- Large files do not lock the browser.

### W11 - Merge Room And Review Cockpit

Depends on W4, W5, W7, W8.

Owner paths:

- `src/merge/`
- `src/web/rest/merge_requests.rs`
- `src/web/rest/reviews.rs`
- `apps/web/src/pages/MergeRequestPage.tsx`
- `apps/web/src/components/merge/`

Tasks:

- Implement MR list/detail/diff/checks/blockers/review threads.
- Implement review comments and review submission.
- Implement approve exact SHA.
- Implement request changes.
- Implement merge preview and merge execute exact SHA.
- Build Merge Room:
  - MR header with source to target, head SHA, state, labels, actions
  - Merge Passport panel
  - changed-file tree
  - virtualized diff viewer
  - inline comments and suggestions
  - evidence/agents/checks side panel
  - sticky review bar
- Merge Passport checks:
  - source SHA unchanged since preview/approval
  - target branch SHA checked
  - target policy SHA checked where available
  - required approvals
  - code owners
  - all threads resolved
  - required CI green
  - VTI/test plan acceptable
  - agent evidence fresh and signed
  - branch protection
  - conflict status
  - release window/deploy freeze when relevant

Acceptance:

- User can review files and comment inline.
- User can approve exact SHA.
- Stale SHA conflict returns 409 and UI shows safe recovery.
- User can merge only when gates pass.
- Two tabs update through WebSocket.

### W12 - Settings Studio

Depends on W4, W7, W8.

Owner paths:

- `src/repos/settings.rs`
- `src/web/rest/settings.rs`
- `apps/web/src/pages/RepositorySettingsPage.tsx`
- `apps/web/src/components/settings/`

Tasks:

- Implement effective settings read.
- Implement settings preview.
- Implement settings patch.
- Implement audit history per setting.
- Sections:
  - general
  - features
  - access
  - branch protection
  - merge policy
  - CI/runners
  - agents/autonomy
  - webhooks
  - secrets metadata
  - notifications
  - Markdown/rendering
  - security
  - integrations
  - retention
  - danger zone
- Every setting row shows:
  - current value
  - inherited/default value
  - risk tier
  - validation
  - last changed by/at
  - preview
  - audit link
- Secrets API returns existence, scope, fingerprint, last rotated, and last
  access metadata only. It must never return secret values after write.
- Dangerous actions require confirmation phrase and idempotency key.
- Support export settings JSON and import with preview.

Acceptance:

- User changes required approvals with preview.
- Branch protection update blocks merge as expected.
- Secret values never round-trip.
- Audit event appears after settings mutation.

### W13 - Issues, CI, Agents, Releases, And Activity

Can start after W7/W8; subareas can run in parallel.

Owner paths:

- `src/web/rest/issues.rs`
- `src/web/rest/ci.rs`
- `src/web/rest/activity.rs`
- `apps/web/src/components/issues/`
- `apps/web/src/components/ci/`
- `apps/web/src/components/agents/`

Tasks:

- Issues: list, detail, create, edit, comment, close, reopen, labels,
  milestones, assignees, link MR.
- CI: pipelines, jobs, live logs, retry, cancel, manual job, failure capsules.
- Agents: sessions, evidence packs, patch proposals, grants, VTI plans,
  pause/stop controls.
- Releases: list and create release through action preview.
- Activity dock streams repo, MR, CI, agents, settings, audit, notifications.
- Keep deeper package registry, wiki, and discussions as nav placeholders unless
  explicitly enabled later.

Acceptance:

- User can triage issue and link MR.
- CI failure event updates live dock.
- Agent evidence appears on repo and MR views.
- Logs stream without rendering huge arrays.

### W14 - Playwright, Storybook, Accessibility, And UX Proof

Depends on W8 and each vertical slice.

Owner paths:

- `apps/web/e2e/`
- `apps/web/src/**/*.test.tsx`
- `apps/web/src/**/*.stories.tsx`
- `apps/web/ux-qa-artifacts/` if generated locally, but committed artifacts
  should be avoided unless project policy requires them
- `agent/ux-qa.toml`

Tasks:

- Add Storybook stories for loading, empty, error, success, and
  permission-denied states.
- Add deterministic API mocks with MSW.
- Add Playwright fixtures for demo data.
- Add accessibility automation with axe.
- Add screenshot and ARIA snapshot capture.
- Add geometry/layout checks for:
  - text overflow
  - overlapping controls
  - target sizes
  - sidebars and activity rail
  - mobile layouts
- Required Playwright flows:
  1. Dashboard loads and WebSocket live badge connects.
  2. Create repo with README and open rendered README.
  3. Browse code tree and open Markdown/raw views.
  4. Open MR, review diff, inline comment, approve exact SHA.
  5. Simulated force-push causes stale SHA conflict.
  6. Change branch protection/settings with preview and audit.
  7. WebSocket disconnect/reconnect catches up.
  8. Permission-denied user sees disabled controls and explanations.
  9. Keyboard-only dashboard to repo to code to MR to settings navigation.
  10. Accessibility scan for dashboard, repo overview, code browser, merge
      room, and settings.

Acceptance:

```bash
npm --workspace @jeryu/web run test:e2e
jankurai ux audit --config agent/ux-qa.toml --out target/jankurai/ux-qa.json
```

### W15 - Docs, CI, Proof Routing, And Release Hygiene

Can proceed throughout, finalizes last.

Owner paths:

- `docs/`
- `README.md`
- `apps/web/README.md`
- `proof-lanes.toml`
- `agent/test-map.json`
- `agent/owner-map.json`
- `agent/generated-zones.toml`
- `.github/` or CI files if web proof is added to CI

Tasks:

- Add `docs/WEB_FORGE.md`.
- Add `docs/WEB_FORGE_API.md`.
- Add `docs/README_RENDERING.md`.
- Add `docs/WEBSOCKET_PROTOCOL.md`.
- Add `apps/web/README.md`.
- Update root README with:

```bash
npm install
npm run web:dev
cargo run -p jeryu -- web serve --dev-assets http://127.0.0.1:5173
```

Production:

```bash
npm run web:build
cargo run -p jeryu -- web serve --bind 127.0.0.1:8787
```

- Update proof routing for all new paths.
- Update generated-zone declarations for generated schemas/types.
- Document local dev, production serving, WebSocket protocol, Markdown security,
  action safety, provider adapter expectations, and troubleshooting.

Acceptance:

- All new paths have owner/test routing.
- Docs explain API, WebSocket, Markdown security, action safety, and local dev.
- Generated artifacts have recorded source commands.

## 6. Frontend UX Requirements

### Global Shell

Required controls:

- Repo switcher.
- Global search.
- Command palette with Ctrl/Cmd+K.
- Create button.
- Current actor menu.
- Sync/live status.
- Left nav: dashboard, repos, issues, merge requests, pipelines, agents,
  releases, settings.
- Live activity rail.
- Notifications inbox.
- Keyboard help.
- Theme and density controls.

Keyboard shortcuts:

```text
Ctrl/Cmd+K   command palette
/            focus current view search
g r          repositories
g m          merge room
g s          settings
[ and ]      previous/next repo where applicable
j/k          move selection in list contexts
Enter        open selected item
Esc          close modal or go up
?            keyboard help
```

### Repository Dashboard

Required dashboard behavior:

- Search by name, description, owner, family, topic, status, language, active
  agent, failed check.
- Filters: owned, starred, recent, archived, private, public, local, GitHub,
  GitLab, mirrored, dirty, blocked, active CI, active agents.
- Group by owner/org, family, provider, health, and last activity.
- Show quick actions per row/card.
- Show live changes when CI, MR, settings, or agent state changes.

### Repository Home

Required sections:

- Header with repo identity, visibility, default branch, topics, clone URLs.
- Live health strip: branch protection, CI, agents, runners, cache, secrets,
  release.
- README rendered from server-sanitized Markdown.
- Latest commit.
- Open MRs and blockers.
- Issues and milestones.
- CI/test summary.
- Agent activity and evidence.
- Recent activity timeline.

### Code Browser

Required controls:

- Branch/tag selector.
- Breadcrumbs.
- Virtualized file tree.
- Tree search and go-to-file.
- Copy path.
- Copy permalink.
- Copy raw URL.
- Download file.
- View raw.
- View blame.
- View history.
- Compare with branch/tag.
- Edit/propose change through action preview.
- Markdown rendered/source toggle.
- Binary preview/download fallback.

### Merge Room

Required controls:

- Overview/files/checks/commits/evidence tabs or equivalent one-screen sections.
- Unified/split diff toggle.
- Hide whitespace.
- Filter generated files.
- File viewed state.
- Inline comments and suggestions.
- Start review and submit review.
- Approve exact SHA.
- Request changes.
- Resolve/unresolve thread.
- Rerun checks.
- Ask agent to fix or explain.
- Update branch/rebase.
- Merge preview.
- Merge strategies allowed by settings.
- Delete source branch option.
- Close/reopen.
- Why blocked explanation.

### Settings Studio

Every settings section must have:

- Search.
- Current value.
- Inherited/default value.
- Risk tier.
- Last changed by/at.
- Preview before mutation.
- Audit link.
- Reset to default where safe.
- Import/export settings JSON.
- Dangerous actions separated in danger zone.

## 7. Markdown Security Requirements

Backend sanitization is mandatory.

Allowed elements should be a constrained Markdown set:

- `a`, `p`, `pre`, `code`, `blockquote`
- `ul`, `ol`, `li`
- `table`, `thead`, `tbody`, `tr`, `th`, `td`
- `h1` through `h6`
- `img` with safe source rewriting
- `details`, `summary`
- `kbd`, `del`, `strong`, `em`, `hr`, `br`

Strip or block:

- `script`
- inline event handlers
- `style` attributes unless explicitly allowed later
- `iframe`
- `object`
- `embed`
- untrusted `svg`
- `javascript:` URLs
- unsafe data URLs
- forms

Markdown cache must include sanitizer version so sanitizer policy changes
invalidate cached HTML.

Mermaid and diagrams are disabled by default. If added later, they must render
in a sandboxed iframe or worker.

## 8. Database Schema Guidance

Use typed Rust structs and DB-boundary methods. Do not scatter SQL.

Core schema fields should include enough provider metadata for cache and replay,
but not provider-specific raw payloads as the primary contract.

Recommended core tables:

```sql
web_repositories(
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  provider_repo_id TEXT,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  full_name TEXT NOT NULL,
  slug TEXT NOT NULL UNIQUE,
  family TEXT,
  description TEXT,
  visibility TEXT NOT NULL,
  default_branch TEXT NOT NULL,
  local_path TEXT,
  remote_url TEXT,
  clone_https_url TEXT,
  clone_ssh_url TEXT,
  web_url TEXT,
  archived INTEGER NOT NULL DEFAULT 0,
  fork INTEGER NOT NULL DEFAULT 0,
  template INTEGER NOT NULL DEFAULT 0,
  topics_json TEXT NOT NULL DEFAULT '[]',
  settings_json TEXT NOT NULL DEFAULT '{}',
  provider_etag TEXT,
  pushed_at TEXT,
  refreshed_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

web_repo_refs(
  repo_id TEXT NOT NULL,
  ref_kind TEXT NOT NULL,
  name TEXT NOT NULL,
  sha TEXT NOT NULL,
  protected INTEGER NOT NULL DEFAULT 0,
  default_ref INTEGER NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(repo_id, ref_kind, name)
);

web_markdown_cache(
  repo_id TEXT NOT NULL,
  commit_sha TEXT NOT NULL,
  path TEXT NOT NULL,
  renderer_version TEXT NOT NULL,
  sanitizer_version TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  html TEXT NOT NULL,
  toc_json TEXT NOT NULL DEFAULT '[]',
  links_json TEXT NOT NULL DEFAULT '[]',
  warnings_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  PRIMARY KEY(repo_id, commit_sha, path, renderer_version, sanitizer_version)
);

web_merge_requests(
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL,
  iid TEXT NOT NULL,
  provider_id TEXT,
  title TEXT NOT NULL,
  body TEXT,
  state TEXT NOT NULL,
  draft INTEGER NOT NULL DEFAULT 0,
  author_login TEXT,
  source_branch TEXT NOT NULL,
  target_branch TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  base_sha TEXT,
  merge_status TEXT,
  passport_hash TEXT,
  web_url TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(repo_id, iid)
);

web_action_receipts(
  id TEXT PRIMARY KEY,
  actor_login TEXT NOT NULL,
  action_kind TEXT NOT NULL,
  target_kind TEXT NOT NULL,
  target_id TEXT NOT NULL,
  idempotency_key TEXT,
  expected_state_hash TEXT,
  resulting_state_hash TEXT,
  expected_sha TEXT,
  provider_calls_json TEXT NOT NULL,
  risk_tier TEXT NOT NULL,
  status TEXT NOT NULL,
  error TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(action_kind, target_id, idempotency_key)
);

web_events(
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  scope TEXT NOT NULL,
  kind TEXT NOT NULL,
  severity TEXT NOT NULL,
  entity_kind TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  summary TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  actor_login TEXT,
  created_at TEXT NOT NULL
);
```

Add indexes for repo owner/name, family, MR state, event scope/seq, audit time,
and notifications user/time.

## 9. Git Provider Requirements

The browser must never depend on provider-specific API shapes. Normalize through
Rust services and `GitHost`.

Required provider operations:

- Repository list/get/create/update/archive/delete.
- Repository import/adopt/mirror metadata.
- Refs, branches, tags, commits, compare.
- Tree, blob, raw, README, history, blame.
- Issues, labels, milestones, comments.
- Merge requests/pull requests.
- Diff, review threads, inline comments, review submission.
- Approve, request changes, merge.
- Checks, pipelines, jobs, logs.
- Branch protection.
- Webhooks and delivery logs.
- Members/collaborators.
- Secrets metadata only.

Provider implementation order:

1. Local/git2 or GitLab first if it is easiest to validate in the existing
   environment.
2. GitHub second where the existing adapter already has check/comment/approval
   foundation.
3. Defer full package registry, wiki, discussions, and enterprise SSO parity.

## 10. Performance Requirements

Targets:

- Initial app shell useful paint: under 1.5s local.
- Dashboard initial data: under 300ms local P95 after warm DB.
- Repo overview: README cache hit under 100ms.
- README cold render: under 500ms for normal README.
- Repo list search/filter: under 50ms client-side for cached data.
- File tree: lazy load by path; do not full-walk large repos.
- Diff viewer: virtualized; supports large MRs without UI lockups.
- WebSocket local event to visible paint: under 250ms; target under 100ms where
  feasible.
- Reconnect with replay: under 1s local.

Implementation requirements:

- Keyset pagination for repo, MR, and activity feeds.
- `ETag` and `If-None-Match` for blobs, Markdown, and repo summaries where
  useful.
- Virtualize repos, trees, diffs, comments, and logs.
- Split frontend chunks by route.
- Debounce and cancel stale search requests.
- Stream logs; do not append unbounded arrays to React state.

## 11. Validation Matrix

Rust:

```bash
cargo check -p jeryu --message-format=json
cargo nextest run -p jeryu --lib
cargo test -p jeryu --test '*' -- --test-threads=1
cargo test -p jeryu --lib state_backend_detects_supported_urls -- --test-threads=1
jankurai doctor --fail-on critical
```

Frontend:

```bash
npm install
npm --workspace @jeryu/web run typecheck
npm --workspace @jeryu/web run lint
npm --workspace @jeryu/web run test
npm --workspace @jeryu/web run build
npm --workspace @jeryu/web run test:e2e
npm --workspace @jankurai/ux-qa run build
npm --workspace @jankurai/ux-qa run test
jankurai ux audit --config agent/ux-qa.toml --out target/jankurai/ux-qa.json
```

Security/correctness:

- Markdown XSS corpus.
- Path traversal corpus.
- Permission matrix tests.
- CSRF and idempotency tests.
- Exact-SHA stale approval/merge tests.
- WebSocket replay/gap/backpressure tests.
- Secrets metadata-only tests.

## 12. Playwright Required Flows

Add deterministic Playwright coverage for:

1. Dashboard loads and WebSocket live badge connects.
2. Repo list search, filter, and grouping.
3. Create repo with README and open rendered README.
4. Browse code tree and open Markdown/source/raw views.
5. Open MR, review diff, inline comment, approve exact SHA.
6. Simulated force-push causes stale SHA conflict.
7. Change branch protection/settings with preview and audit.
8. WebSocket disconnect/reconnect catches up.
9. Permission-denied user sees disabled controls and explanations.
10. Keyboard-only dashboard to repo to code to MR to settings navigation.
11. Accessibility scan for dashboard, repo overview, code browser, merge room,
    and settings.

Required proof artifacts:

- `page.screenshot`
- `locator.screenshot`
- trace
- ARIA snapshot
- axe results
- geometry/layout checks
- Playwright HTML report
- jankurai UX audit JSON

## 13. Final Acceptance Criteria

The work is complete only when:

- `jeryu web serve` launches the browser UI.
- The UI lists and searches accessible repositories.
- Users can create/import/adopt repositories through preview-backed flows.
- Repository overview renders README and Markdown as sanitized HTML with
  correct anchors and relative links.
- Users can browse branches, trees, blobs, raw files, history, blame, and
  compare refs.
- Users can review MRs in one Merge Room with diffs, checks, comments,
  evidence, exact-SHA approval, and exact-SHA merge.
- Users can manage settings through searchable, preview-backed forms with
  audit.
- WebSocket updates repo, MR, CI, agent, settings, audit, and notification
  views without refresh.
- Secrets values are never returned after write.
- Playwright covers the full UX flows listed above.
- Rust, frontend, UX-QA, and jankurai proof lanes are green.

## 14. Explicit Non-Goals For First Production Cut

Design for these, but do not block the first complete web forge on them:

- Replacing Git wire protocol hosting itself.
- Full package registry UI parity.
- Browser IDE/Codespaces clone.
- Enterprise SSO/OIDC beyond token/session scaffolding.
- Public multi-tenant SaaS hardening.
- Full wiki/discussions parity.
- Full RST rendering.
- Mermaid rendering unless sandboxed implementation is deliberately added.

## 15. Recommended Implementation Sequence

Recommended PR order:

1. Workspace split and Vite shell.
2. Rust web module and bootstrap.
3. Contracts and TypeScript generation.
4. Durable WebSocket activity hub.
5. Repositories dashboard and repo creation.
6. README and Markdown renderer.
7. Code browser.
8. Merge Room.
9. Settings Studio.
10. Issues, CI, agents, and activity polish.
11. Playwright, accessibility, UX-QA, performance, docs, and proof routing.

Do not build a GitHub clone page by page. Build a typed
entity/action/event platform with a forge UI on top. JeRyu's advantage is the
combination of typed state, live events, evidence, agent governance, exact-SHA
safety, and previewed actions.
