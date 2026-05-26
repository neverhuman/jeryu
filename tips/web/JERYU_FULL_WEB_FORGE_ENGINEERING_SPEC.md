# JeRyu Full Web Forge Engineering Specification

**Deliverable:** Rust + Axum/WebSocket backend, Vite + TypeScript + React frontend, and shared typed API contracts that turn JeRyu from a powerful CLI/TUI control plane into a full modern GitHub/GitLab-class web forge.

**Target:** deliver the complete browser experience: all repositories, repository creation/import, code browsing, README/Markdown rendering to safe HTML, commits, branches, tags, compare, issues, pull/merge requests, approvals, file review, CI/jobs/logs, agents, cache/VTI, releases, settings, audit, notifications, and real-time activity over WebSocket.

---

## 1. Executive summary

JeRyu already has the most valuable foundation: a Rust control plane, typed control-plane API modules, Git host adapters, GitLab/GitHub-aware workflow concepts, SmartCache, VTI/test intelligence, runner orchestration, agent governance, release gates, secrets, evidence, and an advanced TUI. The missing piece is a first-class browser forge.

The current `apps/web` workspace is not a real web product. It is a QA/evidence placeholder with `ux-qa.ts`, `ux-qa.md`, and a simple Node check. The root `package.json` has an npm workspace pointed at `apps/web`, but the web workspace does not contain Vite, React, routing, API clients, pages, or build assets. The backend already includes `axum`, `tower-http`, `reqwest`, `sqlx`, `git2`, and event/read-model modules, but it needs a browser BFF layer, WebSocket hub, repository browsing service, Markdown renderer, review/merge services, and a settings API.

The recommended design is not to bolt on a thin GitLab/GitHub clone. Instead, make JeRyu a faster, safer, more intuitive forge by compressing GitHub/GitLab's many pages into a few context-rich workspaces:

1. **Mission Dashboard:** all repos, live activity, failing checks, pending approvals, stuck agents, runner pressure, cache pressure, VTI wins, and high-risk actions in one screen.
2. **Repo Home:** code, README, branch health, latest pipeline, open merge requests, agent evidence, and settings health in one view.
3. **Merge Room:** diff review, comments, approvals, CI, VTI explanation, agent evidence, security findings, release risk, and merge controls in one cockpit.
4. **Settings Studio:** searchable settings, policy preview, dry-run diffs, inherited org defaults, branch protection, approvals, agents, webhooks, secrets, CI, visibility, and audit retention.
5. **Realtime Activity Rail:** a global WebSocket-driven stream that explains what changed, why it changed, what is stale, and what action is available.

The implementation should reuse existing JeRyu surfaces rather than bypass them:

- `src/api/*` becomes the shared DTO/read-model contract used by both TUI and web.
- `src/git_host/*` remains the host abstraction, expanded for repository, branch, file, review, issue, and settings operations.
- `db/state.rs` remains the authoritative state path.
- `src/engine.rs` continues webhook/reconciliation; the web server runs beside it and subscribes to the same event bus.
- `apps/web` becomes the real Vite/React app while preserving UX-QA proof lanes as `apps/web/src/ux-qa/*` or `apps/web/qa/*`.

---

## 2. Current repository baseline

### 2.1 Current source layout that matters

Current repo structure includes the right high-level anchors:

```text
jeryu/
├── apps/
│   ├── api/                  # currently only AGENTS.md
│   └── web/                  # currently UX-QA placeholder, not a Vite app
├── crates/                   # domain/adapters/workers/proof utilities
├── db/                       # state backend and schema
├── docs/                     # product/API/TUI docs
├── src/
│   ├── api/                  # typed TUI/control-plane API projections
│   ├── git_host/             # GitHub/GitLab host adapter trait and implementations
│   ├── engine.rs             # webhook/reconciliation engine
│   ├── dispatch.rs           # CLI command dispatch
│   ├── cli_defs.rs           # top-level command definitions
│   ├── repo*.rs              # repository fleet/local/standard commands
│   ├── tui/                  # current mission-control UI
│   ├── cache*, test_intel*, pool*, release*, secrets*, agent* ...
│   └── state -> db/state.rs
├── Cargo.toml
└── package.json
```

### 2.2 Current strengths to keep

| Existing capability | Why it matters for web |
|---|---|
| Rust single-binary control plane | Web can ship as the same operational surface as CLI/TUI. |
| `axum` already present | Avoid introducing a second backend stack. |
| `src/api` typed projections | Prevent duplicating read models for web and TUI. |
| `src/git_host` trait | One UI can support GitHub, GitLab, and local bare repositories. |
| `db/state.rs` | State changes remain auditable and backend-neutral. |
| TUI read models and activity concepts | Web should consume the same high-signal domain state. |
| VTI/cache/agents/runners/releases/secrets | These are JeRyu's differentiators versus GitHub/GitLab. |
| Jansu-backed event thinking | Good fit for WebSocket fanout and replay. |

### 2.3 Current gaps

| Area | Current state | Required state |
|---|---|---|
| Frontend app | `apps/web` is QA evidence only | Vite + React + TypeScript app with routing, API client, WebSocket client, pages, components, tests. |
| Web server | `Serve` runs engine/webhooks only | `jeryu serve` runs engine + web API + static UI + WebSocket. `jeryu web dev` supports local frontend development. |
| Repository discovery | CLI/repo fleet primitives exist | Browser list of all repos with filters, search, families, pinning, health, activity. |
| Repository creation | Not exposed as web flow | Create/import repo with templates, default branch, visibility, README/license/gitignore, CI seed, agent policy. |
| Code browser | Git commands exist | Tree/blob APIs, blame, history, branch picker, file finder, rendered README. |
| Markdown | README shown in GitHub only | Safe backend-rendered HTML with sanitization, heading anchors, task lists, tables, code highlight, relative links/assets. |
| MR/PR review | Host adapter has partial PR/MR methods | Full Merge Room: changed files, comments, approvals, exact-SHA binding, checks, merge queues, squash/rebase/merge. |
| Settings | CLI settings exist | Searchable web settings covering repo/org/user/system/agent/CI/security. |
| Realtime | TUI and engine events exist | Browser WebSocket with topic subscription, replay, backpressure, optimistic action acknowledgements. |

---

## 3. Product north star

> JeRyu Web is a full Git forge where the fastest path to understanding and safely changing a repository is always visible.

This means:

- No hunting through separate pages for CI, reviews, agents, tests, settings, and activity.
- Every destructive or irreversible action has a preview, permission check, evidence link, and audit record.
- Every stale state is marked as stale with the reason: new commit, target branch moved, policy changed, cache evicted, runner disconnected, secret expired.
- Every long-running process streams live: clone/import, pipeline, job logs, agent runs, test selection, merge gate, release promotion.
- Every repo has a clear health score, but the UI always shows the underlying evidence so the score is not magic.

---

## 4. Target architecture

### 4.1 Authority boundaries

```text
Browser React UI
    ↓ HTTPS JSON / WebSocket
src/web/*  (BFF + auth + RBAC + static UI + WebSocket)
    ↓ typed DTOs
src/api/*  (shared read models, action contracts, events)
    ↓ domain services
src/repos/*, src/repo_browser/*, src/merge_requests/*, src/issues/*, src/settings/*
    ↓ adapters/state
src/git_host/*, git2, db/state.rs, src/engine.rs, src/pool.rs, src/test_intel.rs, src/cache.rs
    ↓ external systems
local bare repos, GitLab, GitHub, runners, Docker, Vault/secrets, Jansu/event bus
```

### 4.2 Server process modes

| Command | Behavior |
|---|---|
| `jeryu serve` | Default production mode. Starts Docker/GitLab reconciliation as today, starts SmartCache, starts engine, starts web API/UI, serves built assets from `apps/web/dist`. |
| `jeryu serve --no-ui` | Backward-compatible headless engine/webhook mode. |
| `jeryu serve --web-bind 0.0.0.0:7379` | Explicit web bind address. |
| `jeryu web dev --api-bind 127.0.0.1:7379 --vite-proxy http://127.0.0.1:5173` | Backend API + Vite dev server proxy mode. |
| `jeryu web build` | Runs npm build and writes `apps/web/dist/manifest.json`. |
| `jeryu web open` | Opens browser to the local app after health check. |
| `jeryu web token create --scope admin --ttl 8h` | Creates admin/API token for browser or automation. |

---

## 5. Target tree diagram

```text
jeryu/
├── Cargo.toml
├── package.json
├── apps/
│   ├── api/
│   │   └── AGENTS.md
│   └── web/
│       ├── AGENTS.md
│       ├── index.html
│       ├── package.json
│       ├── tsconfig.json
│       ├── tsconfig.node.json
│       ├── vite.config.ts
│       ├── vitest.config.ts
│       ├── playwright.config.ts
│       ├── qa/
│       │   ├── ux-qa-check.mjs
│       │   ├── ux-qa.md
│       │   └── ux-qa.ts
│       └── src/
│           ├── main.tsx
│           ├── app/
│           │   ├── App.tsx
│           │   ├── AppShell.tsx
│           │   ├── CommandPalette.tsx
│           │   ├── ErrorBoundary.tsx
│           │   ├── Hotkeys.tsx
│           │   ├── NotificationsRail.tsx
│           │   ├── RealtimeProvider.tsx
│           │   ├── routes.tsx
│           │   └── shortcuts.ts
│           ├── api/
│           │   ├── client.ts
│           │   ├── generated.ts
│           │   ├── queryClient.ts
│           │   └── zod.ts
│           ├── realtime/
│           │   ├── socket.ts
│           │   ├── topics.ts
│           │   └── reducer.ts
│           ├── design/
│           │   ├── tokens.css
│           │   ├── components/
│           │   └── icons.tsx
│           ├── features/
│           │   ├── dashboard/
│           │   ├── repositories/
│           │   ├── repo-browser/
│           │   ├── markdown/
│           │   ├── commits/
│           │   ├── branches/
│           │   ├── tags/
│           │   ├── compare/
│           │   ├── merge-requests/
│           │   ├── issues/
│           │   ├── ci/
│           │   ├── agents/
│           │   ├── cache/
│           │   ├── vti/
│           │   ├── releases/
│           │   ├── settings/
│           │   ├── audit/
│           │   └── search/
│           └── test/
│               ├── fixtures.ts
│               ├── msw.ts
│               └── render.tsx
├── db/
│   ├── migrations/
│   │   ├── 20260601_0001_web_forge_core.sql
│   │   ├── 20260601_0002_reviews_and_comments.sql
│   │   ├── 20260601_0003_settings_audit_notifications.sql
│   │   └── 20260601_0004_markdown_cache_search.sql
│   └── state.rs
├── docs/
│   ├── WEB_FORGE_ENGINEERING_SPEC.md
│   ├── WEB_API.md
│   ├── WEB_SOCKET_PROTOCOL.md
│   ├── WEB_SETTINGS_MATRIX.md
│   └── WEB_SECURITY_MODEL.md
├── src/
│   ├── api/
│   │   ├── mod.rs
│   │   ├── web_dto.rs
│   │   ├── repository.rs
│   │   ├── repo_browser.rs
│   │   ├── merge_request.rs
│   │   ├── issue.rs
│   │   ├── settings_projection.rs
│   │   ├── markdown_projection.rs
│   │   ├── notification_projection.rs
│   │   └── review_projection.rs
│   ├── web/
│   │   ├── mod.rs
│   │   ├── config.rs
│   │   ├── state.rs
│   │   ├── router.rs
│   │   ├── error.rs
│   │   ├── auth.rs
│   │   ├── rbac.rs
│   │   ├── csrf.rs
│   │   ├── event_hub.rs
│   │   ├── ws.rs
│   │   ├── static_assets.rs
│   │   ├── openapi.rs
│   │   ├── markdown.rs
│   │   ├── repo_browser.rs
│   │   ├── repo_admin.rs
│   │   ├── merge_requests.rs
│   │   ├── reviews.rs
│   │   ├── issues.rs
│   │   ├── settings.rs
│   │   ├── actions.rs
│   │   ├── search.rs
│   │   ├── notifications.rs
│   │   └── audit.rs
│   ├── web_events/
│   │   ├── mod.rs
│   │   ├── protocol.rs
│   │   ├── bus.rs
│   │   └── replay.rs
│   ├── repos/
│   │   ├── mod.rs
│   │   ├── service.rs
│   │   ├── create.rs
│   │   ├── import.rs
│   │   ├── families.rs
│   │   └── health.rs
│   ├── repo_browser/
│   │   ├── mod.rs
│   │   ├── git_tree.rs
│   │   ├── blob.rs
│   │   ├── blame.rs
│   │   ├── diff.rs
│   │   ├── search.rs
│   │   └── readme.rs
│   ├── merge_requests/
│   │   ├── mod.rs
│   │   ├── service.rs
│   │   ├── approvals.rs
│   │   ├── comments.rs
│   │   ├── merge_queue.rs
│   │   └── review_state.rs
│   ├── cli_defs_commands_web.rs
│   ├── cli_defs.rs
│   ├── dispatch.rs
│   └── lib.rs
└── tests/
    ├── web_api_tests.rs
    ├── web_markdown_tests.rs
    ├── web_ws_tests.rs
    └── web_permissions_tests.rs
```

---

## 6. Backend engineering specification

### 6.1 `src/web` BFF responsibilities

`src/web` is a browser backend-for-frontend, not a second domain model. It should:

- authenticate sessions and API tokens;
- enforce RBAC and action risk tiers;
- convert domain/read-model state into browser DTOs;
- serve JSON routes under `/api/web/v1/*`;
- serve WebSocket under `/api/web/v1/ws`;
- render Markdown into sanitized HTML;
- serve static `apps/web/dist` assets;
- expose OpenAPI/TypeScript contract artifacts;
- produce audit records for every mutating action;
- broadcast event frames for realtime UI updates.

It should not:

- call raw SQL from route handlers;
- fork logic from TUI or CLI;
- perform git writes without action preview and RBAC;
- return unsanitized Markdown HTML;
- block WebSocket fanout on slow clients.

### 6.2 Module responsibilities

| Module | Responsibility |
|---|---|
| `web/config.rs` | Bind address, CORS, cookie/security flags, static-dir, dev-proxy, auth mode, feature flags. |
| `web/state.rs` | Shared `WebState`: `Db`, GitLab client, Docker controller optional, event hub, config, auth keys. |
| `web/router.rs` | Axum router composition, middleware, route grouping, fallback static serving. |
| `web/error.rs` | `WebError` to JSON problem response; maps domain errors to status codes. |
| `web/auth.rs` | Session extraction, API token extraction, login/logout, dev auth guard. |
| `web/rbac.rs` | Permission checks and risk-tier policies. |
| `web/csrf.rs` | CSRF token for cookie-auth mutating requests. |
| `web/event_hub.rs` | Broadcast channels, topic registry, replay cursors, backpressure strategy. |
| `web/ws.rs` | WebSocket handshake, subscribe/unsubscribe, ping/pong, replay, action ack frames. |
| `web/markdown.rs` | Markdown parsing, syntax highlight, safe HTML sanitization, cache keys. |
| `web/repo_browser.rs` | Tree/blob/readme/blame/diff endpoints. |
| `web/repo_admin.rs` | Repository create/import/archive/rename/visibility/settings endpoints. |
| `web/merge_requests.rs` | MR list/detail/diff/checks/merge endpoint aggregation. |
| `web/reviews.rs` | File comments, threads, suggestions, approve/request changes. |
| `web/issues.rs` | Issues, labels, milestones, boards, linking to MRs/commits. |
| `web/settings.rs` | User/org/repo/system settings retrieval, update, preview, inheritance. |
| `web/actions.rs` | Generic action preview/execute path used by dangerous controls. |
| `web/search.rs` | Global/repo search and command palette backend. |
| `web/notifications.rs` | Inbox, read/unread, subscriptions. |
| `web/audit.rs` | Audit query APIs. |
| `web/static_assets.rs` | Embedded or filesystem static asset serving. |
| `web/openapi.rs` | OpenAPI JSON + generated TypeScript schemas. |

### 6.3 State and transaction rule

All state writes must go through typed domain/state methods. Route handlers may orchestrate, but must not build SQL directly.

```rust
// Allowed in route handler
let preview = actions::preview(&state, actor, request).await?;
let receipt = actions::execute(&state, actor, preview.confirmation_token).await?;

// Not allowed in route handler
sqlx::query("update merge_requests set status = ...").execute(...).await?;
```

### 6.4 Action preview/execute contract

Every high-impact operation uses the same two-step contract.

1. `POST /api/web/v1/actions/preview`
2. UI shows exact changes, risk tier, required permission, stale-state checks, and confirmation phrase if needed.
3. `POST /api/web/v1/actions/execute` with `preview_id`, `idempotency_key`, and optional confirmation phrase.
4. Backend revalidates target SHA/settings/policy, executes, records audit event, emits WebSocket updates.

Risk tiers:

| Tier | Examples | Requirement |
|---|---|---|
| `read` | list repos, view README | session/token only |
| `low` | star/pin repo, mark notification read | permission check |
| `medium` | create issue, comment, update description | permission + audit |
| `high` | approve MR, rerun job, update branch protection | permission + preview + audit |
| `critical` | delete repo, rotate secret, force-push mirror, bypass gate | admin permission + preview + confirmation phrase + short-lived token |

### 6.5 Repository service

Repository records need a JeRyu canonical ID independent of provider ID.

```rust
RepositoryId = stable uuid/string
provider = local | gitlab | github
owner_slug = org/user/group
name = repo name
family = optional computed family, e.g. veox-* or explicit repo family
path = local bare/worktree path if local
provider_project_id = GitLab project id or GitHub node id
visibility = private | internal | public
state = active | archived | importing | failed | deleting
```

Core operations:

- list all repos with filters and live health;
- create local bare repo;
- create GitLab project;
- create GitHub repo if configured;
- import from URL;
- clone/cache metadata;
- group into families;
- compute health and capability matrix;
- archive, unarchive, rename, transfer, delete with preview.

### 6.6 Repository browser service

Use `git2` for local repos and provider APIs when remote-only.

Endpoints must support:

- branch/tag/commit ref resolution;
- tree listing with lazy directory expansion;
- blob fetch with size limits;
- LFS pointer detection;
- binary detection;
- raw download with content-type policy;
- blame;
- last commit per file;
- README resolution in this order: `README.md`, `README.markdown`, `README.mdx`, `README.rst`, `README.txt`, case-insensitive;
- safe Markdown HTML;
- path-based history;
- file search and symbol search later.

### 6.7 Markdown rendering pipeline

Markdown must render server-side to safe HTML so README and comments can be displayed consistently.

Pipeline:

```text
raw markdown bytes
  -> size guard and UTF-8 normalization
  -> GitHub-flavored Markdown parser options
  -> syntax highlight fenced blocks
  -> heading anchor injection
  -> relative link and image rewriting
  -> task-list/table/code extension support
  -> sanitize with strict allowlist
  -> add metadata: toc, headings, links, warnings, cache key
```

Security rules:

- no raw `<script>`;
- no event handlers like `onclick`;
- no `javascript:` or unsafe `data:` URLs;
- external images optionally proxied or blocked by repo setting;
- relative repo links resolve through web routes;
- HTML comments stripped by default;
- math/mermaid disabled until a sanitizer+renderer path is specified;
- cache keyed by repo/ref/path/blob SHA + renderer version + policy hash.

### 6.8 WebSocket event contract

Route: `GET /api/web/v1/ws?token=...` or cookie session.

Client messages:

```json
{ "type": "hello", "client_id": "browser-uuid", "last_seen_event_id": 123 }
{ "type": "subscribe", "topics": ["global", "repo:jeryu", "mr:jeryu:42"] }
{ "type": "unsubscribe", "topics": ["repo:jeryu"] }
{ "type": "ping", "nonce": "abc" }
```

Server messages:

```json
{
  "type": "event",
  "event_id": 124,
  "topic": "repo:jeryu",
  "occurred_at": "2026-05-26T17:00:00Z",
  "kind": "repo.updated",
  "subject": { "type": "repository", "id": "repo_123" },
  "actor": { "type": "user", "id": "u_1", "display": "ben" },
  "payload": { "branch": "main", "sha": "abc123" },
  "stale": false
}
```

Reliability:

- every event has monotonic `event_id` in the JeRyu instance;
- clients send `last_seen_event_id` for replay;
- server replays from durable event log if available, otherwise sends `snapshot_required`;
- slow clients receive compacted snapshots for high-volume topics;
- job logs use chunked stream frames with sequence numbers;
- WebSocket never blocks domain writes.

---

## 7. REST API surface

Base path: `/api/web/v1`.

### 7.1 Bootstrap/session

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/bootstrap` | Current user, permissions, feature flags, settings, version, WebSocket URL. |
| `GET` | `/health` | API health and dependency health. |
| `POST` | `/session/login` | Login/dev-token exchange. |
| `POST` | `/session/logout` | End session. |
| `GET` | `/me` | Current user profile, teams, permissions. |
| `PATCH` | `/me/preferences` | UI prefs, pinned repos, notification prefs. |

### 7.2 Repositories

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/repos` | All repos with filters/search/sort/family/grouping. |
| `POST` | `/repos` | Create repo. |
| `POST` | `/repos/import` | Import repo by URL. |
| `GET` | `/repos/{repo}` | Repo overview. |
| `PATCH` | `/repos/{repo}` | Rename/description/topics/avatar/default branch. |
| `POST` | `/repos/{repo}/archive` | Archive. |
| `POST` | `/repos/{repo}/unarchive` | Unarchive. |
| `DELETE` | `/repos/{repo}` | Delete after critical preview. |
| `GET` | `/repos/{repo}/health` | Health score and evidence. |
| `GET` | `/repo-families` | Family rollups. |

### 7.3 Code browsing

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/repos/{repo}/tree/{ref}/{*path}` | Directory tree. |
| `GET` | `/repos/{repo}/blob/{ref}/{*path}` | Blob metadata/content. |
| `GET` | `/repos/{repo}/raw/{ref}/{*path}` | Raw bytes/download. |
| `GET` | `/repos/{repo}/readme/{ref}` | Rendered README. |
| `POST` | `/markdown/render` | Render arbitrary Markdown preview. |
| `GET` | `/repos/{repo}/blame/{ref}/{*path}` | Blame. |
| `GET` | `/repos/{repo}/history/{ref}/{*path}` | File history. |
| `GET` | `/repos/{repo}/compare` | Compare refs. |

### 7.4 Branches, tags, commits

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/repos/{repo}/branches` | Branches with protection status. |
| `POST` | `/repos/{repo}/branches` | Create branch. |
| `PATCH` | `/repos/{repo}/branches/{branch}/protection` | Update protection. |
| `DELETE` | `/repos/{repo}/branches/{branch}` | Delete branch with preview. |
| `GET` | `/repos/{repo}/tags` | Tags/releases link. |
| `POST` | `/repos/{repo}/tags` | Create tag. |
| `GET` | `/repos/{repo}/commits` | Commit log. |
| `GET` | `/repos/{repo}/commits/{sha}` | Commit detail/diff/checks. |

### 7.5 Pull/merge requests

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/repos/{repo}/merge-requests` | MR list. |
| `POST` | `/repos/{repo}/merge-requests` | Create MR. |
| `GET` | `/repos/{repo}/merge-requests/{iid}` | MR detail, status, checks, approval state. |
| `GET` | `/repos/{repo}/merge-requests/{iid}/files` | Changed files and diff metadata. |
| `GET` | `/repos/{repo}/merge-requests/{iid}/diff` | Unified diff or structured hunks. |
| `POST` | `/repos/{repo}/merge-requests/{iid}/comments` | General comment. |
| `POST` | `/repos/{repo}/merge-requests/{iid}/threads` | File/hunk comment. |
| `PATCH` | `/repos/{repo}/merge-requests/{iid}/threads/{thread}` | Resolve/unresolve. |
| `POST` | `/repos/{repo}/merge-requests/{iid}/reviews` | Submit review. |
| `POST` | `/repos/{repo}/merge-requests/{iid}/approve` | Exact-SHA approval. |
| `POST` | `/repos/{repo}/merge-requests/{iid}/request-changes` | Request changes. |
| `POST` | `/repos/{repo}/merge-requests/{iid}/merge` | Merge/squash/rebase with preview token. |
| `POST` | `/repos/{repo}/merge-requests/{iid}/rebase` | Rebase/update branch. |
| `POST` | `/repos/{repo}/merge-requests/{iid}/assign` | Assign reviewer/assignee. |

### 7.6 Issues/planning

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/repos/{repo}/issues` | Issues list. |
| `POST` | `/repos/{repo}/issues` | Create issue. |
| `GET` | `/repos/{repo}/issues/{id}` | Issue detail. |
| `PATCH` | `/repos/{repo}/issues/{id}` | Update fields. |
| `POST` | `/repos/{repo}/issues/{id}/comments` | Comment. |
| `GET` | `/repos/{repo}/labels` | Labels. |
| `GET` | `/repos/{repo}/milestones` | Milestones. |
| `GET` | `/planning/boards` | Cross-repo boards. |

### 7.7 CI, agents, cache, VTI, releases

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/repos/{repo}/pipelines` | Pipelines. |
| `GET` | `/repos/{repo}/jobs` | Jobs. |
| `GET` | `/jobs/{job}/logs` | Log chunks. |
| `POST` | `/jobs/{job}/retry` | Retry job. |
| `POST` | `/pipelines/{pipeline}/cancel` | Cancel pipeline. |
| `GET` | `/agents` | Agent fleet. |
| `GET` | `/agents/{agent}` | Agent details/logs/capabilities. |
| `PATCH` | `/agents/{agent}` | Pause/resume/cap update. |
| `GET` | `/cache/summary` | Cache pressure. |
| `POST` | `/cache/prune` | Prune with preview. |
| `GET` | `/vti/summary` | VTI skip accuracy and time saved. |
| `GET` | `/releases` | Releases. |
| `POST` | `/releases/{release}/promote` | Canary/promote with gates. |

### 7.8 Settings

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/settings/schema` | Searchable settings schema and metadata. |
| `GET` | `/settings/effective` | Effective instance/user/org settings. |
| `PATCH` | `/settings/user` | User settings. |
| `PATCH` | `/settings/instance` | Instance admin settings. |
| `GET` | `/repos/{repo}/settings` | Effective repo settings. |
| `PATCH` | `/repos/{repo}/settings` | Update repo settings. |
| `POST` | `/settings/preview` | Diff/validate settings before apply. |
| `GET` | `/repos/{repo}/settings/audit` | Settings audit. |

---

## 8. Frontend engineering specification

### 8.1 Frontend stack

- Vite
- React
- TypeScript strict mode
- React Router
- TanStack Query
- Zustand or Redux Toolkit for UI-only state
- Zod for runtime validation until generated OpenAPI types are complete
- MSW for API mocking
- Vitest + Testing Library
- Playwright
- Storybook later or the existing UX-QA proof markers converted to real stories/tests

### 8.2 Route map

```text
/
/dashboard
/repos
/repos/new
/repos/import
/:owner/:repo
/:owner/:repo/tree/:ref/*
/:owner/:repo/blob/:ref/*
/:owner/:repo/commits
/:owner/:repo/commit/:sha
/:owner/:repo/branches
/:owner/:repo/tags
/:owner/:repo/compare/:base...:head
/:owner/:repo/issues
/:owner/:repo/issues/:id
/:owner/:repo/merge-requests
/:owner/:repo/merge-requests/:iid
/:owner/:repo/merge-requests/:iid/files
/:owner/:repo/merge-requests/:iid/checks
/:owner/:repo/merge-requests/:iid/evidence
/:owner/:repo/pipelines
/:owner/:repo/jobs/:job
/:owner/:repo/releases
/:owner/:repo/agents
/:owner/:repo/cache
/:owner/:repo/vti
/:owner/:repo/settings/*
/settings/profile
/settings/notifications
/settings/admin/*
/audit
/search
```

### 8.3 Global shell

Global shell components:

- left nav with pinned repos and families;
- global top command/search box;
- repo/ref switcher when in repo context;
- activity rail with live events;
- pending approvals indicator;
- runner/cache pressure indicator;
- theme density and keyboard shortcut controls;
- breadcrumb that can jump between repo tabs;
- status strip showing connected/stale/reconnecting WebSocket state.

### 8.4 All repositories page

Controls:

- search by name/path/topic/owner/provider/family;
- group by family, owner, provider, status, language;
- sort by recent activity, failing checks, pending approvals, risk, name, size;
- filters: archived, importing, private/internal/public, has open MRs, failing CI, stale agents, secrets due, cache pressure;
- quick actions: new repo, import repo, clone URL copy, pin, open settings, run health scan;
- family summary rows for `veox-*` style groups;
- live row updates through WebSocket.

### 8.5 Repository overview

Single screen layout:

```text
Repo header: owner/name, visibility, default branch, clone, new MR, settings
Tabs: Code | MRs | Issues | CI | Agents | VTI | Cache | Releases | Audit | Settings
Main left: file tree + README/rendered docs
Main right: health card, latest pipeline, open MRs, assigned issues, agent runs, cache/VTI, branch protection
Bottom: activity timeline
```

### 8.6 Code browser

Controls:

- branch/ref picker;
- file finder;
- path breadcrumbs;
- copy path/permalink/raw/download;
- view raw/rendered/blame/history;
- binary preview fallback;
- README table of contents;
- Markdown warning banner for blocked unsafe HTML;
- quick create MR from branch/change if web editor is added later.

### 8.7 Merge Room

Merge Room is the flagship “better than GitHub/GitLab” experience.

Panels:

1. **Conversation:** description, comments, threads, timeline.
2. **Files:** diff tree, viewed state, comment threads, suggestions.
3. **Checks:** CI, VTI, runner health, cache hits/misses, flakiness, evidence.
4. **Approvals:** CODEOWNERS, reviewers, exact SHA, stale approvals, policy. 
5. **Agents:** agent runs, receipts, risk assessments, prompt/evidence lineage.
6. **Merge:** preview merge result, method choice, queue status, release risk, final action.

Controls:

- approve current SHA;
- request changes;
- comment;
- suggest patch;
- mark file viewed;
- filter changed files by language/status/ownership/risk;
- rerun failed jobs;
- rerun VTI selection;
- ask agent to explain failing check;
- merge/squash/rebase after preview;
- auto-merge when green;
- lock thread;
- copy review summary.

### 8.8 Settings Studio

Settings must be searchable, previewable, and explain inherited defaults.

Layout:

```text
Settings Studio
├── Search box: "branch protection", "webhook", "agent", "cache"
├── Left categories
├── Main form with validation + inherited/default indicators
├── Right preview panel
│   ├── Before/after diff
│   ├── Affected branches/MRs/runners/agents
│   ├── Required permission
│   ├── Audit record preview
│   └── Confirmation controls
└── History tab with restore/revert
```

---

## 9. Settings inventory

### 9.1 User settings

- profile display name/avatar/email;
- SSH/GPG signing keys;
- personal access tokens;
- active sessions;
- notification routing;
- notification subscriptions;
- theme, density, reduced motion;
- command palette preferences;
- pinned repos/families;
- default review filters;
- default diff mode;
- timezone/date format;
- accessibility preferences;
- API tokens and expiry;
- trusted devices.

### 9.2 Instance/system settings

- base URL;
- bind host/port;
- TLS/cookie/security flags;
- auth providers;
- default repo visibility;
- allowed import hosts;
- max repo size;
- retention windows;
- audit retention;
- WebSocket replay retention;
- job log retention;
- cache quotas;
- runner defaults;
- secret backend;
- LLM/agent provider allowlist;
- telemetry/export;
- rate limits;
- CORS;
- backup/restore;
- maintenance banner;
- feature flags.

### 9.3 Organization/workspace settings

- members/teams/roles;
- default branch naming;
- repo creation permissions;
- default branch protection;
- default approval rules;
- default CI templates;
- default agent policies;
- webhook defaults;
- labels/milestones templates;
- compliance rules;
- required evidence gates;
- CODEOWNERS defaults;
- package/registry permissions;
- repository family rules.

### 9.4 Repository settings

General:

- name, description, topics;
- visibility;
- archive/delete/transfer;
- default branch;
- README/license/gitignore templates;
- features enabled: issues, wiki, releases, packages, CI, agents, VTI, cache, discussions later.

Access:

- collaborators;
- teams;
- deploy keys;
- protected environments;
- permissions per feature;
- token scopes.

Branch protection:

- protected branch patterns;
- require PR/MR;
- required approvals;
- CODEOWNERS;
- dismiss stale approvals;
- require latest target branch;
- required status checks;
- required JeRyu merge passport;
- signed commits;
- linear history;
- no force-push/delete;
- bypass list;
- merge queue.

Merge requests:

- default merge method;
- squash rules;
- auto-delete source branch;
- draft policy;
- allowed target branches;
- reviewer auto-assignment;
- stale review invalidation;
- thread resolution required;
- exact-SHA approval required;
- agent review allowed/required;
- VTI evidence required.

CI/CD:

- pipelines enabled;
- required pipeline templates;
- variables/secrets references;
- runner pool assignment;
- job timeout;
- retry rules;
- artifact retention;
- cache key policy;
- schedule triggers;
- manual approval gates.

Agents/autonomy:

- allowed agents;
- max autonomy level;
- approval required by action type;
- allowed tools/capabilities;
- LLM provider/model limits;
- prompt/evidence retention;
- auto-fix allowed;
- auto-review allowed;
- auto-merge allowed only with required gates;
- sandbox/net policy;
- secret access policy.

Webhooks/integrations:

- outbound webhooks;
- event filters;
- secret rotation;
- retry policy;
- delivery logs;
- Slack/Teams/email integrations;
- GitHub/GitLab mirroring;
- package registry integration;
- vulnerability scanner integration.

Audit/compliance:

- audit enabled;
- retention period;
- export destination;
- immutable mode;
- approval receipts;
- evidence packs;
- policy drift detection;
- security finding thresholds.

---

## 10. Permission model

Roles should be composable; avoid hard-coding only GitHub-like names.

Base permissions:

- `repo.read`
- `repo.create`
- `repo.import`
- `repo.write`
- `repo.admin`
- `repo.delete`
- `code.read`
- `code.write`
- `branch.create`
- `branch.delete`
- `branch.protect`
- `mr.read`
- `mr.create`
- `mr.comment`
- `mr.review`
- `mr.approve`
- `mr.merge`
- `issue.read`
- `issue.write`
- `ci.read`
- `ci.run`
- `ci.cancel`
- `runner.admin`
- `cache.read`
- `cache.admin`
- `agent.read`
- `agent.operate`
- `agent.admin`
- `secrets.read_metadata`
- `secrets.write`
- `settings.read`
- `settings.write`
- `audit.read`
- `system.admin`

Mutating routes must call `rbac::require(actor, permission, resource)` and include the permission in action preview responses.

---

## 11. Data model additions

Core tables:

```sql
web_users
web_sessions
web_api_tokens
web_repositories
web_repository_families
web_repo_memberships
web_settings
web_settings_history
web_audit_log
web_notifications
web_activity_events
web_markdown_cache
web_review_threads
web_review_comments
web_merge_requests
web_merge_request_files
web_issue_cache
web_repo_import_jobs
web_saved_searches
web_pins
```

Important indexes:

- `web_repositories(provider, provider_project_id)` unique;
- `web_repositories(owner_slug, name)` unique;
- `web_repositories(family, updated_at)`;
- `web_activity_events(event_id)`;
- `web_activity_events(topic, event_id)`;
- `web_notifications(user_id, read_at, created_at)`;
- `web_markdown_cache(repo_id, ref_name, path, blob_sha, renderer_version, policy_sha)`;
- `web_review_threads(repo_id, mr_iid, file_path, line_new, resolved)`;
- `web_settings(scope_kind, scope_id, key)`;
- `web_audit_log(resource_kind, resource_id, created_at)`.

---

## 12. Testing and proof plan

### 12.1 Rust tests

- `web_markdown_tests.rs`: sanitizer, anchors, tables, task lists, relative links, malicious URLs.
- `web_api_tests.rs`: bootstrap, repo list, repo detail, tree, blob, readme.
- `web_permissions_tests.rs`: settings/merge/delete cannot bypass RBAC.
- `web_ws_tests.rs`: subscribe, replay, ping/pong, slow client compaction.
- `web_actions_tests.rs`: preview/execute idempotency and stale target SHA.
- `repo_browser_tests.rs`: local git fixtures for tree/blob/diff/blame.
- `merge_room_tests.rs`: exact-SHA approval and stale approval invalidation.

### 12.2 Frontend tests

- unit: API client, WebSocket reducer, Markdown component, route guards;
- component: repo list, repo header, README panel, file tree, Merge Room panels, settings forms;
- e2e: create repo, open README, open MR, approve current SHA, update settings with preview, reconnect WebSocket;
- visual/UX proof: loading, empty, error, permission-denied, stale/reconnecting states;
- accessibility: keyboard navigation and ARIA labels for command palette, diff tree, settings forms.

### 12.3 Validation commands

```bash
cargo fmt --check
cargo check -p jeryu --message-format=json
cargo nextest run -p jeryu --lib
cargo nextest run --test web_api_tests
cargo nextest run --test web_markdown_tests
npm --workspace @jeryu/web run typecheck
npm --workspace @jeryu/web run test
npm --workspace @jeryu/web run build
npm --workspace @jeryu/web run ux-qa
```

---

## 13. Implementation phases

### Phase 0 — foundation and contracts

- Move existing `apps/web` UX-QA files into `apps/web/qa`.
- Replace `apps/web/package.json` with Vite/React scripts.
- Add `src/web` skeleton, static asset serving, `/api/web/v1/health`, `/bootstrap`.
- Add `WebCommands` and `jeryu web` CLI.
- Add shared DTO modules under `src/api`.
- Add event hub and WebSocket handshake with mock events.

Acceptance:

- `jeryu serve` starts engine and web server.
- `npm --workspace @jeryu/web run build` emits `dist`.
- Browser loads shell and bootstrap data.

### Phase 1 — all repos + repo home + README

- Add repo list/read/create/import APIs.
- Add repo dashboard page.
- Add repo overview page.
- Add tree/blob/readme APIs.
- Add safe Markdown renderer.
- Add README panel with relative link rewriting.

Acceptance:

- User can see all repos.
- User can create/import a repo.
- User can open repo and see rendered README as HTML.

### Phase 2 — code browser and history

- Add branch/tag/commit APIs.
- Add tree lazy loading, file finder, blob viewer, raw download.
- Add compare and file history.
- Add blame.

Acceptance:

- User can navigate code faster than GitHub/GitLab with no full page reloads.

### Phase 3 — Merge Room

- Add MR list/detail/diff/review APIs.
- Add exact-SHA approvals.
- Add check aggregation and VTI/evidence panels.
- Add comments/threads/resolution.
- Add merge preview and execute.

Acceptance:

- User can review files, approve, and merge with current SHA safety.

### Phase 4 — issues, planning, notifications

- Add issue APIs and UI.
- Add labels/milestones/boards.
- Add notification inbox and subscriptions.
- Add cross-repo saved searches.

### Phase 5 — CI/agents/cache/VTI/release

- Add live jobs/logs.
- Add runner/cache pressure pages.
- Add VTI explanations.
- Add agent details/config controls.
- Add release promotion controls.

### Phase 6 — settings studio and admin

- Add full searchable settings schema.
- Add preview/execute for settings updates.
- Add audit and restore history.
- Add branch protection, merge rules, CI, agent, webhooks, secrets, user/org/system settings.

### Phase 7 — hardening

- OpenAPI/TypeScript generation.
- Session/token hardening.
- CSRF, rate limits, strict CORS.
- Accessibility and performance budgets.
- Production static asset embedding.
- Full integration tests.

---

## 14. Performance targets

- Repo list initial render under 1s for 1,000 repos with cached metadata.
- Global search command palette p95 under 150ms for local indexed metadata.
- Tree expand p95 under 200ms for warm local repos.
- README render p95 under 250ms for cached blob, under 600ms uncached for normal README.
- WebSocket event-to-visible update p95 under 250ms on local network.
- Job log stream with bounded memory; UI virtualizes logs and diffs.
- Diff view virtualizes large files; default collapse generated/binary/vendor files.

---

## 15. Security invariants

- All writes require RBAC.
- Dangerous writes require preview/execute and idempotency keys.
- Merge approvals are bound to exact SHA.
- Stale approvals are visibly invalidated when head SHA, target branch SHA, or policy SHA changes.
- Markdown HTML is sanitized server-side.
- Secrets values are never returned to browser; only metadata and rotation controls.
- API tokens are hashed at rest.
- Sessions use secure, HTTP-only cookies in production.
- CSRF required for cookie-auth mutations.
- Audit records include actor, resource, action, risk tier, before/after summary, request ID, and evidence links.
- WebSocket topics enforce authorization per topic.

---

## 16. What makes this better than GitHub/GitLab

1. **Merge Room instead of scattered tabs:** review, CI, VTI, agents, settings risk, and merge controls in one cockpit.
2. **Realtime by default:** no manual refresh for pipelines, approvals, comments, repo imports, agent runs, cache pressure, or settings changes.
3. **Evidence-first UI:** every green/red state links to the reason, logs, VTI proof, agent receipt, or policy rule.
4. **Action preview everywhere:** settings, merge, delete, archive, secrets, cache prune, runner drain, and agent changes are previewed before execution.
5. **Repo families:** first-class grouping for fleets like `veox-*` with cross-repo health, queue, and activity rollups.
6. **Agent-native governance:** agent actions are not hidden bot comments; they are structured evidence with permissions, risk, and replayable receipts.
7. **Faster navigation:** command palette, pinned repos, ref switcher, virtualized diffs/logs, and persistent context rail.
8. **Less confusion:** settings are searchable, inherited defaults are explained, stale states are explicit, and all dangerous buttons tell the user what will happen.

---

## 17. Open implementation decisions

| Decision | Recommendation |
|---|---|
| Static serving | Serve from `apps/web/dist` in dev; optionally `include_dir` embedded assets for release. |
| Auth v1 | Local admin/dev token + API tokens first; OIDC later. |
| Type generation | Start with serde DTO + hand-written TS/Zod; move to `utoipa` OpenAPI once stable. |
| Provider parity | Use local/git2 + GitLab first, GitHub second where adapter already exists. |
| Markdown comments | Same renderer as README, with stricter HTML policy for comments. |
| Search | Metadata/search endpoint first; tantivy later. |
| Wiki/packages/discussions | Reserve nav slots; implement after core forge parity. |

---

## 18. Final implementation checklist

- [ ] Replace `apps/web` placeholder with Vite app.
- [ ] Preserve UX-QA evidence proof lanes in `apps/web/qa`.
- [ ] Add `web-ui` feature and backend dependencies.
- [ ] Add `src/web` BFF modules.
- [ ] Add `src/web_events` event bus/replay modules.
- [ ] Add repository/readme/markdown APIs.
- [ ] Add repo creation/import APIs.
- [ ] Add MR review/approval/merge APIs.
- [ ] Add issues/planning APIs.
- [ ] Add settings schema/effective/update/preview APIs.
- [ ] Add WebSocket subscribe/replay/backpressure.
- [ ] Add DB migrations.
- [ ] Add React app shell, dashboard, repo home, code browser, Merge Room, settings studio.
- [ ] Add tests and proof commands.
- [ ] Update README and docs.
