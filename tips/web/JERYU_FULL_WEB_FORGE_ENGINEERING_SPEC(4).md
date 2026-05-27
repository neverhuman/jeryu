# JeRyu Full Web Forge — Final Engineering Specification

**Target repo:** `neverhuman/jeryu`  
**Target stack:** Rust single-binary control plane, Axum HTTP/WebSocket gateway, SQLite/RedlineDB-capable state, Vite + TypeScript + React frontend  
**Prepared:** 2026-05-26  
**Purpose:** Deliver the full GitHub/GitLab browser experience, modernized for JeRyu’s agent-native, proof-rich, realtime control plane.

---

## 1. Executive summary

JeRyu already has the hard parts of a next-generation Git forge: a Rust control plane, Git-compatible workflows, GitLab integration, CI/job/pool/cache/release/secrets/agent primitives, action safety patterns, typed read models, durable event concepts, and a powerful TUI. The missing product layer is a real web forge: all repositories, repository creation/import, browsing, README rendering, branch/tag/commit navigation, issues, merge requests, code review, protected settings, approvals, pipelines, audit, and realtime activity.

The final design is not “build a separate GitHub clone.” It is a JeRyu web forge built as a **browser BFF over the existing Rust authority**. The browser never talks directly to GitHub, GitLab, Docker, SQLite, RedlineDB, Vault, or raw Git. It consumes typed REST snapshots, applies resumable WebSocket deltas, and executes all mutations through JeRyu’s previewable action model.

The implementation replaces the current `apps/web` QA-only package with a real Vite/React product while preserving and expanding the UX proof lanes. It adds a `src/web` gateway, a `src/web_events` bus, repository/read-model API contracts, repository browser/Markdown services, merge/review services, settings/permission services, migrations, frontend routes, Playwright/Vitest coverage, and docs.

---

## 2. Baseline findings from the current repository

The current repository structure supports this direction well:

- Root workspace already includes `apps/`, `crates/`, `db/`, `docs/`, `src/`, `tests/`, and an existing `package.json` with `apps/web` as a Node workspace.
- `apps/web` currently exists but is a small QA/proof package named `@jankurai/ux-qa`, not an actual React app.
- `apps/api` exists but only contains an `AGENTS.md` marker; the Rust control plane remains the correct API home.
- `src/api` already contains typed surfaces such as actions, entity, events, event store, freshness, inspection, proof, read model, runtime profile, and snapshot.
- `src/git_host` already contains GitHub/GitLab host modules, GitLab client/helpers/types, CODEOWNERS support, and tests.
- `src/gateway` exists and should remain lower-level infrastructure; the new `src/web` module should be a product-level browser gateway built on top of it, not a replacement for it.
- The CLI currently has a bare `Serve` command. This should become configurable and should dispatch to the new web gateway.
- The root `Cargo.toml` already has Axum, tower-http, reqwest, sqlx, git2, ratatui, crossterm, notify, tracing, serde, chrono, uuid, and related dependencies. WebSocket/static-file/Markdown/type-generation features need to be added.

---

## 3. Final product north star

JeRyu Web should feel familiar enough for a GitHub/GitLab user to navigate immediately, but better in the areas where those products are slow, split across too many pages, or not agent-aware.

The user should be able to:

1. See **all repositories** across managed local repos, GitLab, GitHub, imported repos, repo families, pinned repos, and recently active repos.
2. Create, import, clone, archive, transfer, rename, fork, mirror, and configure repositories.
3. Browse files and branches with keyboard-first navigation, preserved context, quick previews, blame/history, and diff previews.
4. Render `README.md` and any Markdown file correctly to safe HTML with tables, task lists, anchors, relative links/images, syntax highlighting, and a table of contents.
5. Open merge requests, review files, add line/thread comments, approve/request changes, resolve threads, compare branches, see checks and evidence, and merge with exact-SHA safety.
6. See pipelines, jobs, logs, artifacts, runner pools, VTI/smart-test explanations, cache status, agent activity, releases, secrets, and audit context without leaving the merge/repo flow.
7. Use settings without confusion: general, visibility, collaborators, teams, roles, branch protections, merge rules, CI/CD variables, runners, webhooks, deploy keys, agents, secrets, notifications, integrations, audit, and danger zone.
8. Watch everything update in realtime through a resumable, filtered WebSocket model.
9. Execute mutating actions only after previewing risk, side effects, permissions, affected resources, and evidence receipt behavior.

---

## 4. Non-negotiable architecture decisions

### 4.1 Rust remains the authority

All mutations and all authoritative read models live in Rust. React is a fast typed client. It does not duplicate policy logic.

### 4.2 REST snapshot plus WebSocket deltas

Every page loads from a stable REST endpoint, then subscribes to topic-filtered realtime events with a cursor. If a cursor gap occurs, the client reloads the affected snapshot.

### 4.3 Action preview for every mutation

Repository creation, settings changes, MR approval, merge, branch deletion, CI retry/cancel, secret rotation, runner scaling, webhook edits, archive/delete, and token operations must use the same shape:

1. `POST /api/actions/preview`
2. render risk/side effects/permission gates
3. require confirmation when appropriate
4. `POST /api/actions/execute`
5. emit audit + evidence receipt + WebSocket event

### 4.4 Server-rendered Markdown

README and Markdown rendering must be backend-owned because it is security-sensitive and needs consistent GFM behavior, cache-by-blob-SHA, link rewriting, asset permissions, and sanitization.

### 4.5 Provider abstraction, not provider leakage

The UI sees `Repository`, `MergeRequest`, `Pipeline`, `Branch`, `Commit`, `ReviewThread`, and `ActionPreview`. It does not care whether data came from GitLab, GitHub, local Git, or a future JeRyu-native Git server.

### 4.6 Keep the TUI investments

The web app should reuse the same concepts as the TUI: events, actions, evidence, risk, entity references, snapshots, proof, runtime profile, capacity, and freshness.

---

## 5. Target repository tree

```text
jeryu/
├── Cargo.toml                         # add web/markdown/ws/static/typegen deps
├── package.json                       # npm workspaces + web scripts
├── db/
│   └── migrations/
│       └── 202606010001_web_forge_core.sql
├── docs/
│   ├── WEB_FORGE.md                   # product/operator docs
│   ├── WEB_API.md                     # generated or hand-maintained API map
│   └── WEB_SECURITY.md                # auth/CSP/Markdown/action safety
├── src/
│   ├── api/
│   │   ├── mod.rs                     # export new typed API modules
│   │   ├── repository.rs              # Repository, RepoSummary, RepoFamily, RepoCreate
│   │   ├── repo_browser.rs            # TreeEntry, BlobView, MarkdownView, DiffView
│   │   ├── merge_request.rs           # MR, reviews, threads, file diffs, mergeability
│   │   ├── issue.rs                   # issues, labels, milestones, projects
│   │   ├── settings.rs                # user/org/repo settings read models
│   │   ├── web_read_model.rs          # bootstrap/dashboard aggregates
│   │   └── type_export.rs             # TS export/OpenAPI schema bridge
│   ├── cli_defs.rs                    # Serve -> configurable Serve + Web subcommand
│   ├── cli_defs_web.rs                # WebCommand definitions
│   ├── dispatch.rs                    # dispatch Serve/Web to src/web/command.rs
│   ├── git_host/
│   │   ├── mod.rs                     # expanded trait contract
│   │   ├── github.rs                  # provider impl expansion
│   │   └── gitlab.rs                  # provider impl expansion
│   ├── repo_browser/
│   │   ├── mod.rs
│   │   ├── service.rs                 # trees, blobs, commits, branches, compare
│   │   ├── markdown.rs                # safe GFM renderer and cache keys
│   │   └── diff.rs                    # unified/split diff model
│   ├── repos/
│   │   ├── mod.rs
│   │   ├── service.rs                 # all repos, create/import, families
│   │   ├── permissions.rs             # roles and grants
│   │   └── settings.rs                # repo settings service
│   ├── merge/
│   │   ├── mod.rs
│   │   ├── service.rs                 # MR lifecycle and mergeability
│   │   ├── review.rs                  # comments, threads, approvals
│   │   └── merge_queue.rs             # optional later queue/batch support
│   ├── web/
│   │   ├── mod.rs
│   │   ├── command.rs                 # CLI entrypoint
│   │   ├── state.rs                   # AppState and service wiring
│   │   ├── router.rs                  # Axum routing, CORS, trace, static SPA
│   │   ├── error.rs                   # typed API errors
│   │   ├── security.rs                # auth/session/CSP/CSRF helpers
│   │   ├── static_assets.rs           # Vite dist serving
│   │   ├── ws.rs                      # WebSocket upgrade/replay/subscriptions
│   │   └── api/
│   │       ├── mod.rs
│   │       ├── bootstrap.rs
│   │       ├── repos.rs
│   │       ├── repo_files.rs
│   │       ├── merge_requests.rs
│   │       ├── reviews.rs
│   │       ├── issues.rs
│   │       ├── settings.rs
│   │       ├── ci.rs
│   │       ├── agents.rs
│   │       ├── notifications.rs
│   │       └── actions.rs
│   └── web_events/
│       ├── mod.rs
│       ├── protocol.rs                # ClientMessage/ServerFrame/EventEnvelope
│       ├── bus.rs                     # broadcast, filters, replay cursors
│       └── store.rs                   # optional durable event cursor bridge
├── apps/
│   └── web/
│       ├── package.json               # @jeryu/web
│       ├── index.html
│       ├── vite.config.ts
│       ├── tsconfig.json
│       ├── tsconfig.node.json
│       ├── src/
│       │   ├── main.tsx
│       │   ├── app/
│       │   │   ├── App.tsx
│       │   │   ├── router.tsx
│       │   │   ├── Shell.tsx
│       │   │   ├── commandPalette.tsx
│       │   │   └── shortcuts.ts
│       │   ├── api/
│       │   │   ├── client.ts
│       │   │   ├── queryClient.ts
│       │   │   └── types.generated.ts
│       │   ├── realtime/
│       │   │   ├── ActivitySocketProvider.tsx
│       │   │   ├── reducer.ts
│       │   │   └── protocol.ts
│       │   ├── components/
│       │   │   ├── ActionButton.tsx
│       │   │   ├── MarkdownHtml.tsx
│       │   │   ├── StatusPill.tsx
│       │   │   ├── DiffViewer.tsx
│       │   │   ├── FileTree.tsx
│       │   │   └── SettingsForm.tsx
│       │   ├── pages/
│       │   │   ├── DashboardPage.tsx
│       │   │   ├── RepositoriesPage.tsx
│       │   │   ├── RepoOverviewPage.tsx
│       │   │   ├── RepoCodePage.tsx
│       │   │   ├── RepoCommitsPage.tsx
│       │   │   ├── MergeRequestPage.tsx
│       │   │   ├── IssuesPage.tsx
│       │   │   ├── PipelinesPage.tsx
│       │   │   ├── SettingsPage.tsx
│       │   │   └── AuditPage.tsx
│       │   ├── styles/
│       │   │   ├── tokens.css
│       │   │   └── app.css
│       │   └── tests/
│       │       ├── markdown-rendering.test.tsx
│       │       ├── websocket-replay.test.ts
│       │       └── action-preview.test.tsx
│       ├── e2e/
│       │   ├── repo-dashboard.spec.ts
│       │   ├── readme-render.spec.ts
│       │   ├── merge-review.spec.ts
│       │   └── settings.spec.ts
│       └── ux-qa.md                   # retain and expand proof requirements
└── tests/
    ├── web_router_smoke.rs
    ├── web_markdown_rendering.rs
    ├── web_ws_replay.rs
    ├── repo_browser_service.rs
    └── merge_review_permissions.rs
```

---

## 6. Backend architecture

### 6.1 Request path

```text
Browser
  -> Axum router in src/web/router.rs
  -> auth/session/security middleware
  -> typed handler in src/web/api/*
  -> domain service in repos/repo_browser/merge/settings/ci/agents
  -> provider abstraction or local DB/Git state
  -> typed API read model
  -> JSON response
```

### 6.2 Mutation path

```text
Browser action request
  -> POST /api/actions/preview
  -> action registry resolves permission + risk + side effects
  -> preview returned to UI
  -> user confirms
  -> POST /api/actions/execute with preview_id and idempotency_key
  -> exact-SHA / permission / lease checks repeated
  -> mutation executes
  -> audit/evidence written
  -> web event emitted
  -> UI applies delta or reloads snapshot
```

### 6.3 Service boundaries

| Service | Responsibility | Must not do |
|---|---|---|
| `web` | HTTP/WebSocket, auth/session, static SPA, error mapping | Direct business logic |
| `repos` | repo list/create/import/settings/families/permissions | Render frontend HTML |
| `repo_browser` | tree/blob/diff/README/Markdown/commits/branches/tags | Provider-specific HTTP details |
| `merge` | MR lifecycle, review state, mergeability, exact-SHA merge | Store plaintext secrets |
| `web_events` | event fanout, topics, cursors, replay/gap detection | Mutate domain state |
| `git_host` | provider abstraction + GitHub/GitLab/local implementations | Leak provider models to React |
| `api` | typed DTOs and read models | Blocking I/O or provider calls |

---

## 7. Data model additions

The migration should be additive and safe to run on existing JeRyu state. Tables use text UUIDs for public IDs, integer provider IDs where available, RFC3339 timestamps, JSON payloads for provider-specific extension fields, and normalized columns for UI-critical filters.

### 7.1 Core identity and repo tables

| Table | Purpose | Key columns |
|---|---|---|
| `web_users` | local/account/provider users | `id`, `provider`, `provider_user_id`, `username`, `display_name`, `avatar_url`, `email`, `role`, `created_at`, `last_seen_at` |
| `web_sessions` | browser sessions | `id`, `user_id`, `csrf_token_hash`, `created_at`, `expires_at`, `last_seen_at`, `user_agent_hash` |
| `repo_sources` | Git provider connections | `id`, `kind`, `name`, `base_url`, `auth_ref`, `sync_enabled`, `last_sync_at`, `created_at` |
| `repositories_web` | provider-neutral repository catalog | `id`, `source_id`, `provider_repo_id`, `owner`, `name`, `slug`, `description`, `default_branch`, `visibility`, `is_archived`, `is_mirror`, `local_path`, `last_activity_at`, `created_at`, `updated_at` |
| `repo_families` | logical grouped repos | `id`, `name`, `slug`, `description`, `color`, `created_at` |
| `repo_family_members` | repo-to-family mapping | `family_id`, `repo_id`, `role`, `sort_rank` |
| `repo_members` | direct collaborator grants | `repo_id`, `principal_kind`, `principal_id`, `role`, `source`, `created_at` |

### 7.2 Code and Markdown tables

| Table | Purpose | Key columns |
|---|---|---|
| `branches_cache` | branch list projection | `repo_id`, `name`, `sha`, `is_default`, `protected`, `ahead`, `behind`, `updated_at` |
| `tags_cache` | tag projection | `repo_id`, `name`, `sha`, `message`, `tagger`, `created_at` |
| `rendered_markdown_cache` | safe Markdown HTML cache | `repo_id`, `blob_sha`, `path`, `render_mode`, `html`, `toc_json`, `asset_refs_json`, `warnings_json`, `created_at` |
| `file_view_cache` | optional hot blob metadata | `repo_id`, `ref_name`, `path`, `blob_sha`, `mime`, `size_bytes`, `is_binary`, `last_viewed_at` |

### 7.3 Merge/review tables

| Table | Purpose | Key columns |
|---|---|---|
| `merge_requests` | provider-neutral PR/MR projection | `id`, `repo_id`, `provider_mr_id`, `number`, `title`, `description_md`, `author_id`, `source_branch`, `target_branch`, `head_sha`, `base_sha`, `state`, `draft`, `merge_status`, `risk`, `created_at`, `updated_at`, `closed_at`, `merged_at` |
| `merge_request_checks` | checks/status/VTI projection | `id`, `mr_id`, `name`, `kind`, `status`, `conclusion`, `target_url`, `evidence_ref`, `started_at`, `completed_at` |
| `review_threads` | line/general threads | `id`, `mr_id`, `file_path`, `old_line`, `new_line`, `side`, `is_resolved`, `created_by`, `created_at`, `resolved_by`, `resolved_at` |
| `review_comments` | comments within threads | `id`, `thread_id`, `author_id`, `body_md`, `body_html`, `created_at`, `updated_at`, `is_deleted` |
| `review_decisions` | approvals/changes/comments | `id`, `mr_id`, `reviewer_id`, `decision`, `body_md`, `commit_sha`, `created_at`, `dismissed_at` |
| `merge_queue_entries` | optional merge train/queue | `id`, `repo_id`, `mr_id`, `position`, `state`, `enqueued_by`, `enqueued_at`, `started_at`, `finished_at` |

### 7.4 Issues, notifications, audit

| Table | Purpose | Key columns |
|---|---|---|
| `issues` | issue projection | `id`, `repo_id`, `number`, `title`, `body_md`, `author_id`, `state`, `assignee_ids_json`, `label_ids_json`, `milestone_id`, `created_at`, `updated_at`, `closed_at` |
| `labels` | repo labels | `id`, `repo_id`, `name`, `color`, `description` |
| `milestones` | repo milestones | `id`, `repo_id`, `title`, `description`, `due_on`, `state` |
| `web_notifications` | user notification inbox | `id`, `user_id`, `kind`, `entity_ref`, `title`, `body`, `read_at`, `created_at` |
| `audit_log` | immutable user/agent/action trail | `id`, `actor_kind`, `actor_id`, `action`, `entity_ref`, `risk`, `payload_json`, `evidence_ref`, `created_at` |
| `web_events` | optional durable event replay | `cursor`, `topic`, `kind`, `entity_ref`, `payload_json`, `created_at` |

---

## 8. Typed API contract

### 8.1 Bootstrap

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/bootstrap` | current user, capabilities, feature flags, nav counts, pinned repos, event cursor |
| `GET` | `/api/me` | current profile, memberships, preferences |
| `PATCH` | `/api/me/preferences` | user UI preferences, saved filters, shortcuts |

### 8.2 Repositories

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos` | all repos with filters: provider, owner, family, visibility, language, health, activity |
| `POST` | `/api/repos` | create repo via action preview/execution path or direct safe draft endpoint |
| `POST` | `/api/repos/import` | import/mirror external repo |
| `GET` | `/api/repos/:owner/:repo` | repo overview aggregate |
| `PATCH` | `/api/repos/:owner/:repo` | rename/description/topics/homepage/visibility via action path |
| `POST` | `/api/repos/:owner/:repo/archive` | archive via action path |
| `POST` | `/api/repos/:owner/:repo/fork` | fork via action path |
| `POST` | `/api/repos/:owner/:repo/mirror/sync` | manual mirror sync |

### 8.3 Code browser and Markdown

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:owner/:repo/tree/:ref/*path` | directory listing |
| `GET` | `/api/repos/:owner/:repo/blob/:ref/*path` | file metadata/content with binary guard |
| `GET` | `/api/repos/:owner/:repo/raw/:ref/*path` | raw file with permission checks |
| `GET` | `/api/repos/:owner/:repo/readme/:ref` | first README candidate rendered to safe HTML |
| `GET` | `/api/repos/:owner/:repo/markdown/:ref/*path` | render any Markdown file |
| `GET` | `/api/repos/:owner/:repo/blame/:ref/*path` | blame chunks |
| `GET` | `/api/repos/:owner/:repo/history/:ref/*path` | path history |
| `GET` | `/api/repos/:owner/:repo/compare/:base...:head` | diff summary + files |

### 8.4 Branches, tags, commits

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:owner/:repo/branches` | branch list with protection/behind/ahead |
| `POST` | `/api/repos/:owner/:repo/branches` | create branch via action path |
| `DELETE` | `/api/repos/:owner/:repo/branches/:branch` | delete branch via action path |
| `GET` | `/api/repos/:owner/:repo/tags` | tag list |
| `POST` | `/api/repos/:owner/:repo/tags` | create tag/release tag via action path |
| `GET` | `/api/repos/:owner/:repo/commits/:sha` | commit detail, checks, refs |
| `GET` | `/api/repos/:owner/:repo/commits` | commit log with path/ref filters |

### 8.5 Merge requests and review

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:owner/:repo/merge-requests` | list/filter/search MRs |
| `POST` | `/api/repos/:owner/:repo/merge-requests` | create MR |
| `GET` | `/api/repos/:owner/:repo/merge-requests/:number` | MR detail aggregate |
| `PATCH` | `/api/repos/:owner/:repo/merge-requests/:number` | edit title/body/draft/labels/assignees |
| `GET` | `/api/repos/:owner/:repo/merge-requests/:number/files` | file diff list |
| `GET` | `/api/repos/:owner/:repo/merge-requests/:number/checks` | checks, CI, evidence, VTI |
| `POST` | `/api/repos/:owner/:repo/merge-requests/:number/comments` | general comment |
| `POST` | `/api/repos/:owner/:repo/merge-requests/:number/threads` | line thread |
| `PATCH` | `/api/repos/:owner/:repo/review-threads/:thread_id` | resolve/unresolve |
| `POST` | `/api/repos/:owner/:repo/merge-requests/:number/reviews` | approve/comment/request changes |
| `POST` | `/api/repos/:owner/:repo/merge-requests/:number/merge` | exact-SHA merge through action path |
| `POST` | `/api/repos/:owner/:repo/merge-requests/:number/rebase` | rebase/update branch through action path |
| `POST` | `/api/repos/:owner/:repo/merge-requests/:number/close` | close/reopen |

### 8.6 Issues, CI, agents, releases

| Method | Path | Purpose |
|---|---|---|
| `GET/POST` | `/api/repos/:owner/:repo/issues` | issue list/create |
| `GET/PATCH` | `/api/repos/:owner/:repo/issues/:number` | issue detail/edit |
| `GET` | `/api/repos/:owner/:repo/pipelines` | pipelines/checks |
| `GET` | `/api/repos/:owner/:repo/pipelines/:id/jobs` | jobs and stages |
| `GET` | `/api/repos/:owner/:repo/jobs/:id/logs` | streamed/paged logs |
| `POST` | `/api/repos/:owner/:repo/jobs/:id/retry` | retry action |
| `POST` | `/api/repos/:owner/:repo/pipelines/:id/cancel` | cancel action |
| `GET` | `/api/repos/:owner/:repo/agents` | agent sessions/actions/evidence |
| `GET` | `/api/repos/:owner/:repo/releases` | releases/tags/deploy state |

### 8.7 Settings

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:owner/:repo/settings` | complete settings read model |
| `PATCH` | `/api/repos/:owner/:repo/settings/general` | description/default branch/visibility/topics |
| `PATCH` | `/api/repos/:owner/:repo/settings/access` | collaborators/team grants |
| `PATCH` | `/api/repos/:owner/:repo/settings/branch-protection` | branch rules |
| `PATCH` | `/api/repos/:owner/:repo/settings/merge` | merge methods, required approvals, stale approvals |
| `PATCH` | `/api/repos/:owner/:repo/settings/ci` | variables, runners, artifacts, caches |
| `PATCH` | `/api/repos/:owner/:repo/settings/agents` | agent scopes, autonomy policy |
| `PATCH` | `/api/repos/:owner/:repo/settings/security` | secrets/deploy keys/webhooks/integrations |
| `GET` | `/api/repos/:owner/:repo/audit` | audit events |

### 8.8 Actions

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/api/actions/preview` | return risk, permissions, side effects, confirmation, dry-run details |
| `POST` | `/api/actions/execute` | execute previously previewed action with idempotency key |
| `GET` | `/api/actions/:id` | action status/evidence receipt |

---

## 9. WebSocket protocol

### 9.1 Endpoint

`GET /api/ws?cursor=<last_seen_cursor>` upgrades to WebSocket after session/auth validation.

### 9.2 Client messages

```json
{ "type": "hello", "client_id": "browser-tab-uuid", "last_cursor": 12345 }
{ "type": "subscribe", "topics": ["repo:acme/api", "mr:acme/api:42", "dashboard:all"] }
{ "type": "unsubscribe", "topics": ["repo:acme/api"] }
{ "type": "ping", "nonce": "..." }
```

### 9.3 Server frames

```json
{
  "type": "event",
  "cursor": 12346,
  "topic": "mr:acme/api:42",
  "kind": "review.thread.created",
  "entity": { "kind": "review_thread", "id": "thr_..." },
  "occurred_at": "2026-05-26T16:00:00Z",
  "payload": { "thread_id": "thr_...", "file_path": "src/lib.rs" }
}
```

Other server frame types:

- `hello_ack`: server version, current cursor, heartbeat interval, feature flags.
- `snapshot_required`: client cursor too old or gap detected; reload listed resources.
- `permission_changed`: invalidate affected queries.
- `pong`: heartbeat.
- `error`: typed recoverable/unrecoverable errors.

### 9.4 Topics

| Topic | Events |
|---|---|
| `dashboard:all` | repo activity, notifications, system health, assigned reviews |
| `repo:{owner}/{repo}` | branches, commits, tags, settings, pipelines, issues, MR counts |
| `repo:{owner}/{repo}:code:{ref}` | tree/blob invalidation, README update, branch head change |
| `mr:{owner}/{repo}:{number}` | comments, thread resolution, approvals, checks, mergeability, force-push |
| `pipeline:{owner}/{repo}:{id}` | job updates, logs, artifacts, cancel/retry |
| `agent:{id}` | agent actions, grants, evidence, policy decisions |
| `settings:{owner}/{repo}` | settings/audit/permission changes |

### 9.5 Reliability requirements

- Every event has a monotonic cursor.
- The server may buffer a bounded in-memory stream and optionally persist to `web_events`.
- Client reducer must be idempotent by cursor and entity version.
- If a gap is detected, server sends `snapshot_required` and client reloads only affected queries.
- Slow clients receive compressed/coalesced invalidations before disconnect.
- Server enforces topic authorization on every subscribe request.

---

## 10. Markdown and README rendering

### 10.1 Required behavior

The renderer must support:

- GitHub-Flavored Markdown tables, task lists, strikethrough, autolinks, fenced code blocks, and heading anchors.
- Syntax highlighting for common languages, with safe fallback to escaped plain text.
- Table of contents generation from headings.
- Relative link rewriting:
  - `./docs/x.md` -> web route for Markdown preview.
  - `./img/logo.png` -> permission-checked raw asset URL.
  - `#anchor` -> local anchor.
  - absolute URLs -> `rel="nofollow noopener noreferrer"`, safe target behavior.
- Mermaid/diagram support only as sanitized source blocks in first cut; client-rendered diagrams can be added later behind CSP + sandbox controls.
- Cache by `(repo_id, blob_sha, path, render_mode)`.
- Reject or strip scripts, event handlers, unsafe URLs, inline styles unless explicitly allowlisted.

### 10.2 Rust implementation

Use `pulldown-cmark` for Markdown, add a preprocessing step for task list metadata if needed, generate heading slugs deterministically, sanitize with `ammonia`, add syntax highlighting with `syntect` in a controlled wrapper, and cache sanitized output.

### 10.3 Test cases

- README at root, `.github/README.md`, `docs/README.md` candidate order.
- Relative Markdown links, relative images, nested directories, URL-encoded paths.
- Tables, task lists, fenced code, long code blocks, Unicode headings.
- XSS attempts: `<script>`, `javascript:`, image `onerror`, malformed HTML, SVG script, data URLs.
- Large files and binary detection.
- Cache hit by blob SHA after branch moves.

---

## 11. Frontend UX specification

### 11.1 Global shell

Persistent shell:

- top search/command bar
- global activity indicator
- current user/avatar/session menu
- repo switcher
- left nav with Dashboard, Repos, Review Queue, Issues, CI, Agents, Settings
- right realtime rail collapsible to notifications/activity/evidence

Keyboard shortcuts:

| Shortcut | Action |
|---|---|
| `Cmd/Ctrl+K` | open command palette |
| `g r` | all repos |
| `g d` | dashboard |
| `g i` | issues |
| `g m` | merge requests/review queue |
| `g s` | settings in current scope |
| `t` | quick file finder in repo |
| `b` | branch switcher |
| `.` | focus code/browser command mode |
| `Esc` | close modal/popover or move up one navigation depth |

### 11.2 All repositories dashboard

Controls:

- create repo
- import repo
- connect provider
- clone/open local path
- filter by provider/owner/family/visibility/language/status/activity
- saved filters
- pin/unpin repos
- group by family/owner/provider/status
- show health: open MRs, assigned reviews, failing pipelines, stale branches, agent activity, secret warnings
- realtime updates for active/failing repos

### 11.3 Repository overview

Sections:

- repo header: name, visibility, provider, default branch, clone URL, actions
- health strip: CI, reviews, issues, release, cache, agents, settings warnings
- README panel with safe rendered HTML
- recent commits
- active merge requests
- issues assigned to current user
- pipeline/job activity
- releases/tags
- audit/security highlights

Controls:

- branch switcher
- clone dropdown
- create branch
- create file/upload file first-cut optional
- create MR
- run pipeline
- open in provider
- settings shortcut
- watch/star/pin internal flags

### 11.4 Code browser

Layout:

```text
Repo header + branch/path breadcrumb
├── left: virtualized file tree
├── center: file viewer or rendered Markdown
└── right: context panel (symbols, blame, history, checks, actions)
```

Controls:

- branch/tag/ref switcher
- path breadcrumb
- fuzzy file finder
- raw/copy/download
- blame/history
- compare selected ref
- open MR for current branch
- Markdown/code toggle for `.md`
- wrap lines, whitespace, line numbers
- permalinks to lines/ranges

### 11.5 Merge Room

The merge request page should be a decision cockpit, not a tab maze.

```text
MR title/state/risk/merge button/sticky checks
├── left rail: changed files, reviewers, threads, checks, commits
├── center: conversation + file diffs + review composer
└── right rail: mergeability, approvals, evidence, VTI, agents, branch protection
```

Required controls:

- approve
- request changes
- comment
- create/resolve threads
- filter files by reviewed/changed/test/source/generated
- viewed checkbox per file
- split/unified diff
- whitespace toggle
- retry/cancel checks
- update branch/rebase
- squash/rebase/merge commit selection if allowed
- delete source branch toggle
- merge with exact SHA confirmation
- copy MR link
- subscribe/unsubscribe

Merge button states:

| State | UI |
|---|---|
| checks running | disabled or queueable with pending status |
| required approval missing | disabled with direct reviewer/action hint |
| unresolved required thread | disabled with jump links |
| stale head SHA | disabled, prompt reload/update branch |
| risky agent changes | gated by policy, show evidence |
| ready | enabled, preview required before execute |

### 11.6 Settings UX

Settings should be searchable, grouped, and previewable. Changes should show exact impact before saving.

Sections:

- General: name, description, topics, homepage, default branch, visibility, archive/transfer/delete.
- Access: collaborators, teams, roles, deploy keys, tokens.
- Branch protections: patterns, required checks, approvals, CODEOWNERS, stale approval dismissal, signed commits, linear history.
- Merge: allowed merge methods, squash defaults, merge queue, auto-merge, source branch deletion.
- CI/CD: variables, masked/protected flags, runners, artifacts, cache, schedules, webhook triggers.
- Agents: autonomy level, scopes, approval thresholds, model/provider policy, action allowlist, evidence requirements.
- Security: secret scanning, dependency policy, webhooks, integrations, audit, retention.
- Notifications: repo watch, review request, failed pipeline, security events.

Every dangerous section must require action preview and typed confirmation.

---

## 12. Settings matrix

| Scope | Settings |
|---|---|
| User | theme, density, preferred diff, keyboard map, notification rules, saved filters, PAT/session management |
| Organization | teams, owners, default repo visibility, required protections, billing/limits later, audit retention |
| Repository general | name, description, topics, homepage, visibility, default branch, archive, transfer, delete |
| Repository access | collaborators, teams, deploy keys, tokens, role grants, guest read options |
| Branch protections | pattern, required checks, approvals, CODEOWNERS, stale approvals, signed commits, status timeout |
| Merge rules | squash/rebase/merge commit, auto-merge, merge queue, delete branch, exact SHA |
| CI/CD | variables, runners, schedules, artifacts, cache, pipeline source policy, job permissions |
| Agents | allowed agents, autonomy level, high-risk approval policy, secrets access, branch scopes |
| Security | secret scanning, dependency rules, webhook secrets, audit export, retention, lock repo |
| Integrations | GitLab, GitHub, local Git, Slack/webhook later, MCP endpoints |

---

## 13. Provider abstraction requirements

`GitHostProvider` should expose provider-neutral async methods:

```rust
async fn list_repositories(&self, filter: RepoFilter) -> Result<Vec<RepositorySummary>>;
async fn create_repository(&self, input: CreateRepositoryInput) -> Result<Repository>;
async fn get_repository(&self, repo: RepoRef) -> Result<Repository>;
async fn list_branches(&self, repo: RepoRef) -> Result<Vec<BranchSummary>>;
async fn list_tags(&self, repo: RepoRef) -> Result<Vec<TagSummary>>;
async fn get_commit(&self, repo: RepoRef, sha: String) -> Result<CommitDetail>;
async fn list_tree(&self, repo: RepoRef, ref_name: String, path: String) -> Result<TreeView>;
async fn get_blob(&self, repo: RepoRef, ref_name: String, path: String) -> Result<BlobView>;
async fn compare(&self, repo: RepoRef, base: String, head: String) -> Result<CompareView>;
async fn list_merge_requests(&self, repo: RepoRef, filter: MrFilter) -> Result<Vec<MergeRequestSummary>>;
async fn get_merge_request(&self, repo: RepoRef, number: u64) -> Result<MergeRequestDetail>;
async fn merge(&self, repo: RepoRef, number: u64, input: MergeInput) -> Result<MergeResult>;
```

Provider implementations:

- `GitLabProvider`: expands existing GitLab client/types and maps GitLab MR/pipeline/branch APIs.
- `GitHubProvider`: maps GitHub repositories, pulls, checks, branches, contents, compare APIs.
- `LocalGitProvider`: uses `git2` for local/managed repos and stores JeRyu-native MR/review data locally.

---

## 14. Security model

### 14.1 Roles

| Role | Capabilities |
|---|---|
| Guest | public/internal read, if allowed |
| Reporter | read, clone, issues, comments |
| Developer | push branches, create MRs, run non-protected pipelines |
| Maintainer | merge, settings subset, branch protection management |
| Owner/Admin | destructive settings, provider connections, audit export, secret policy |
| Agent | scoped, expiring grants; no ambient authority |

### 14.2 Authorization

- Authorization checks live in Rust services and action previews.
- WebSocket topic subscribe repeats authorization.
- UI hides unavailable controls but backend always enforces.
- Settings PATCH endpoints reject fields outside actor role.
- Secrets values are never returned after write; only metadata is visible.

### 14.3 Session, CSRF, CSP

- First cut can support local trusted mode and token/session mode.
- Cookie sessions require SameSite=Lax/Strict, Secure in production, HttpOnly.
- Mutations require CSRF token or Authorization bearer mode.
- Static app uses strict CSP; Markdown HTML is sanitized and rendered in a constrained container.

### 14.4 Exact-SHA safety

Merge/update/rebase/delete-branch actions must include the SHA/ref observed during preview. Execution revalidates the SHA and rejects stale UI state.

---

## 15. Performance requirements

| Area | Requirement |
|---|---|
| Dashboard | first useful content under 1.5s local, under 3s remote provider with cache |
| Repo list | virtualized; handle 10k repos with server pagination |
| File tree | lazy/virtualized; do not fetch entire repo tree by default |
| Blob view | stream or page files over 1 MiB; binary guard; hard cap by config |
| Markdown | cache by blob SHA; sanitize once per blob/render mode |
| Diff view | virtualized files/hunks; lazy load large files |
| WebSocket | heartbeat, backpressure, coalescing, cursor replay/gap recovery |
| Frontend | route-level code splitting, React Query cache, no global rerender on every event |

---

## 16. Testing and proof plan

### 16.1 Rust tests

- `web_router_smoke.rs`: bootstrap, health, static fallback, CORS/CSP headers.
- `web_markdown_rendering.rs`: GFM, relative links, sanitization, cache hits.
- `web_ws_replay.rs`: subscribe, cursor replay, gap handling, unauthorized topic rejection.
- `repo_browser_service.rs`: tree/blob/compare provider mapping.
- `merge_review_permissions.rs`: approve/request changes/merge gates/exact-SHA rejection.
- `settings_action_preview.rs`: branch protection and dangerous settings previews.

### 16.2 Frontend tests

- Vitest/Testing Library: command palette, Markdown container, action preview modal, settings dirty state.
- WebSocket reducer tests: event idempotency, cursor gaps, snapshot invalidation.
- API client tests: typed errors, aborts, auth redirect, retry policy.

### 16.3 Playwright scenarios

1. all repos dashboard loads, filters, pins repo, receives realtime repo update.
2. repo overview renders README safely with relative links/images.
3. code browser navigates branch/path, opens raw and blame/history.
4. MR review adds thread, resolves it, approves, sees merge blocked/unblocked.
5. settings change shows preview, applies, audit event appears.
6. pipeline job log streams and retry/cancel actions update live.

### 16.4 UX proof lanes

Keep the current `apps/web/ux-qa.md` spirit and expand it:

- Storybook state coverage for loading/empty/error/success/permission-denied.
- Playwright screenshots and ARIA snapshots.
- axe/pa11y/accessibility automation.
- geometry checks for target size, edge clearance, modal focus trap.
- Lighthouse/web-vitals layout stability.
- MSW mocks and generated API fixtures.
- design token discipline and artifact-backed proof receipts.

---

## 17. CI and build integration

Root scripts:

```json
{
  "web:dev": "npm --workspace @jeryu/web run dev",
  "web:build": "npm --workspace @jeryu/web run build",
  "web:test": "npm --workspace @jeryu/web run test",
  "web:e2e": "npm --workspace @jeryu/web run e2e",
  "ux-qa": "npm --workspace @jeryu/web run ux-qa"
}
```

Rust validation:

```bash
cargo check --workspace
cargo test -p jeryu web_
cargo run -p jeryu -- web export-types --out apps/web/src/api/types.generated.ts
npm run web:build
npm run web:test
npm run web:e2e
```

Production packaging:

1. `npm --workspace @jeryu/web run build` outputs `apps/web/dist`.
2. `cargo build --release -p jeryu` embeds or serves `apps/web/dist` depending on feature/config.
3. `jeryu serve --bind 127.0.0.1:8787 --frontend-dir apps/web/dist` serves API + SPA.

---

## 18. Implementation milestones

### Phase 0 — contracts and skeleton

- Add API DTO modules.
- Add type export command.
- Add `src/web` router/state/error skeleton.
- Add configurable `serve` CLI.
- Replace `apps/web` package with Vite skeleton.
- Add migration file.

Exit criteria: `GET /api/bootstrap` and `/api/health` work; Vite app loads shell; generated types compile.

### Phase 1 — all repos and README

- Implement repo catalog read model.
- Implement `/api/repos`, repo overview, tree/blob, README render.
- Implement Markdown sanitizer/cache.
- Build dashboard, repo overview, code browser read-only.

Exit criteria: user can see all repos, open repo, browse files, and view rendered README.

### Phase 2 — branches, commits, compare, code UX

- Branch/tag/commit endpoints.
- Compare endpoint.
- File history/blame first cut.
- Fuzzy file finder, branch switcher, copy/raw/download.

Exit criteria: code browsing feels faster and more intuitive than basic GitHub navigation.

### Phase 3 — merge room and review

- MR list/detail/files/checks.
- Review comments/threads/approvals.
- Mergeability and exact-SHA merge action.
- Realtime MR updates.

Exit criteria: user can review, approve, and merge from the web UI with safety gates.

### Phase 4 — settings/admin parity

- Repo settings read/write.
- Branch protection and merge settings.
- Access/collaborators.
- Audit log.
- Dangerous action previews.

Exit criteria: repository settings cover the high-value GitHub/GitLab settings without confusing navigation.

### Phase 5 — CI, agents, releases, issues

- Issues list/detail/create.
- Pipelines/jobs/log streaming/actions.
- Agent evidence/activity panels.
- Releases/tags.
- Notifications.

Exit criteria: repo and merge pages show all operational context live.

### Phase 6 — hardening, scale, polish

- Performance budgets.
- Accessibility pass.
- Security review.
- Provider edge cases.
- Large diff/file virtualization.
- Full proof artifacts.

Exit criteria: production-ready single-binary JeRyu web forge.

---

## 19. Acceptance criteria

### Functional

- All repos are visible with filters, search, provider/family grouping, pinned/recent/health states.
- User can create/import repos through previewable actions.
- Repo overview renders README safely and correctly.
- File browser supports branch/path navigation, raw/copy/download, Markdown/code toggle, and large file guard.
- MR page supports file review, comments, threads, approval/request changes, checks, mergeability, and exact-SHA merge.
- Settings cover general/access/branch protection/merge/CI/agents/security/integrations/danger zone.
- WebSocket updates dashboard, repo overview, MR, pipeline, settings, notifications.

### Safety

- No mutating operation bypasses server permission checks.
- Risky/destructive operations require action preview and typed confirmation.
- Markdown sanitization prevents script execution and unsafe links.
- Secrets never echo plaintext.
- Audit log records actor, action, entity, risk, payload summary, evidence ref, timestamp.

### Quality

- Rust tests cover router, Markdown, WebSocket, repo browser, merge permissions, settings previews.
- Frontend unit tests cover reducers/components/actions.
- Playwright covers critical workflows.
- UX QA artifacts include screenshots, ARIA snapshots, accessibility reports, and proof receipts.
- Build commands are documented and stable.

---

## 20. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Provider API mismatch | provider-neutral DTOs; provider-specific extension JSON kept out of default UI |
| Markdown XSS | server render, sanitize, strict CSP, exhaustive tests |
| WebSocket event flood | topic filters, coalescing, backpressure, cursor replay, snapshot invalidation |
| Settings complexity | searchable grouped settings, preview diffs, audit trail |
| Large repos/diffs | pagination, virtualization, lazy tree loading, blob/diff size guards |
| Merge race conditions | exact-SHA action preview/execution contract |
| Duplicated TUI/web logic | reuse API/action/event/read-model concepts; no frontend policy duplication |
| Build/package drift | root scripts, CI lanes, generated types checked or reproducibly generated |

---

## 21. Recommended first patch set

The first PR should be intentionally small but vertically complete:

1. Add `src/web` skeleton with `/api/health`, `/api/bootstrap`, `/api/ws` hello, and SPA serving.
2. Convert `apps/web` from QA stub to Vite/React shell while preserving UX proof docs.
3. Add generated/shared TypeScript type path.
4. Add configurable `jeryu serve --bind --frontend-dir --dev-cors`.
5. Add tests for router, bootstrap, static fallback, and WebSocket hello.

That PR proves the architecture before large feature work begins.

---

## 22. Final implementation guidance

Build this as a product-grade JeRyu surface, not a collection of pages. The winning UX is a realtime, keyboard-fast, low-confusion forge where every page explains current risk, evidence, checks, and next actions. Familiar GitHub/GitLab concepts should be preserved where helpful, but JeRyu should make the system state more visible, mutations safer, and navigation faster.
