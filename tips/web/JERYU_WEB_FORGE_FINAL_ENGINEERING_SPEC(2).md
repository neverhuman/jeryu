# JeRyu Full Web Forge — Final Engineering Specification

**Target repo:** `neverhuman/jeryu` / connector mirror `jeppsontaylor/JeRyu`  
**Date:** 2026-05-26  
**Stack:** Rust 2024, Axum, SQLx/SQLite/RedlineDB-compatible state, Git2/system Git, Vite, TypeScript, React, WebSocket event hub  
**Companion artifact:** `JERYU_WEB_FORGE_FINAL_ENGINEERING_DIFF.diff`  
**Status:** Final consolidated engineering spec and implementation blueprint, synthesized from the uploaded solution set and the live repo structure. This is intentionally implementation-level, but it is not an already-compiled commit.

---

## 1. Executive summary

JeRyu is already a strong Rust control plane: it wraps Git, runs CI/CD orchestration, tracks runners and pipelines, exposes a webhook/API engine, contains an agent governance model, provides a typed TUI API, has a state store, and has GitHub/GitLab host adapter foundations. The missing layer is a real browser forge: a modern, fast, full GitHub/GitLab-class repository product that is easier to navigate, safer for high-impact actions, and more real-time than the incumbent tools.

The final design adds a **Web Forge** layer without replacing JeRyu’s CLI/TUI strengths:

1. Replace the current `apps/web` UX-QA-only package with a real Vite + TypeScript + React app while preserving UX-QA artifact checks.
2. Add a Rust `src/web` backend-for-frontend layer mounted by the existing engine.
3. Add typed API DTOs under `src/api` for repositories, code, Markdown, merge requests, reviews, settings, notifications, activity, and WebSocket frames.
4. Add persistent web product tables for repositories, users, memberships, issues, merge requests, review threads, comments, settings, labels, milestones, notifications, audit events, README render cache, branch protection, webhooks, tokens, and activity.
5. Add a WebSocket event hub with scoped subscriptions, replay cursors, heartbeats, backpressure, and snapshot/delta recovery.
6. Render `README.md` and all Markdown through a server-side sanitizer using GitHub-flavored Markdown semantics, relative link rewriting, heading anchors, syntax highlighting metadata, and blob-SHA cache keys.
7. Deliver a unified UI that combines repository browsing, code review, merge approvals, CI evidence, agent status, settings, search, notifications, command palette, and live activity into one cockpit.

The result should feel like a **GitHub/GitLab successor**, not a clone: fewer disconnected pages, better context, keyboard-first navigation, safer mutations, live evidence, and immediate drill-down into exactly why a repo, branch, review, check, merge gate, runner, or agent is blocked.

---

## 2. Current repository findings

### 2.1 What exists and should be reused

The live repo is not greenfield. The design must reuse these foundations:

| Existing area | Reuse strategy |
|---|---|
| Root Rust workspace and `jeryu` package | Keep single-binary posture. Add web features to the existing package instead of creating a second authority. |
| `src/engine.rs` | Mount the Web Forge router next to existing `/health`, `/hooks`, and `/cache/summary` endpoints. Preserve webhook behavior. |
| `src/api/*` | Promote typed TUI/event/action/read-model contracts into shared web DTOs. Keep web rendering from typed projections, not direct raw SQL or Docker/GitLab state. |
| `src/git_host/*` | Extend GitHub/GitLab adapter traits for repo discovery, refs, commits, trees, blobs, comparisons, comments, approvals, merge checks, and webhook dispatch. |
| `db/state.rs` | Add web product read/write methods here or through domain repositories that still use `Db::pool()` and state invariants. Do not bypass state ownership. |
| `src/repo*` and `commands::repo` | Surface repo fleet registry, init/adopt/mode/hooks/standard/shadow/backup flows in the browser. |
| `src/tui/*` | Reuse mental model: mission, attention, evidence, agents, jobs, tests, pools, cache, secrets. Web should be a higher-bandwidth version, not a separate product. |
| `src/settings*` | Add `web`, `auth`, `markdown`, `notifications`, and `realtime` settings with deterministic defaulting and forward compatibility. |
| `apps/web` | Replace placeholder package with a real app, but keep UX-QA marker checks as proof gates. |

### 2.2 What is missing today

The current codebase does not yet have the browser product layer the user requested. The missing surfaces are:

- Full repository dashboard with all repos across local, GitLab, GitHub, mirrored, archived, personal, organization, and repo-family views.
- Repository creation/import/fork/mirror/adopt flows.
- Browser repository home with README render, activity, health, CI, branch, release, agent, and merge blockers.
- Code browser with branch/tag selector, tree virtualization, blob viewer, blame/history, file search, symbol search, Markdown rendering, binary preview, download/copy/open controls.
- Commit, branch, tag, compare, and release views.
- Merge request / pull request review room: file tree, unified/split diff, inline comments, batch review, approvals, unresolved threads, exact-SHA binding, merge gate passport, CI evidence, agent review, and merge controls.
- Issues, labels, milestones, saved filters, boards, linked MRs, and activity.
- Searchable settings with complete sections for access, branch protection, webhooks, secrets, deploy keys, runners, CI, agents, retention, audit, notifications, Markdown, security, integrations, mirroring, backups, and danger-zone operations.
- Real-time WebSocket subscriptions and activity replay.
- Web auth, sessions, CSRF, RBAC, audit logging, and safe mutation previews.

### 2.3 Product implication

The right implementation is **not** “make a few React pages.” It is a new product layer over JeRyu’s existing authority:

```text
Browser React UI
  ↓ typed fetch/WebSocket clients
Rust Web BFF: src/web/*
  ↓ typed projections, command previews, action execution
Existing JeRyu control plane: api/actions/events/read_model, state, git_host, repo_fleet, CI, agent, release, secrets
  ↓
Git repositories, GitHub/GitLab remotes, local GitLab, runners, Vault, cache, evidence store
```

The Web Forge is a projection and action surface. The state engine remains the authority.

---

## 3. Product principles

### 3.1 Faster than GitHub/GitLab

- Command palette for every high-value action.
- Repo switcher always available.
- Live activity rail, no refresh loops.
- Split-pane navigation: list → detail → inspector.
- Optimistic UI only for reversible low-risk actions; high-risk actions require preview and receipt.
- Virtualize large file trees and diffs.
- Cache README/Markdown renders by blob SHA.
- Batch network calls into bootstrap/read-model endpoints.

### 3.2 Less confusing than GitHub/GitLab

- One global shell instead of many unrelated page chrome variants.
- One “Merge Passport” for all merge requirements: approvals, unresolved threads, CI, policy, exact SHA, agents, code owners, secrets, deploy windows, branch protection.
- One settings search that jumps directly to the relevant setting and explains side effects.
- One activity model that combines Git, CI, agents, reviews, settings, and security events.
- Inline “why blocked” explanations for every disabled action.
- Safe mutation previews with “will do”, “will not do”, “requires”, “undo path”, and “receipt”.

### 3.3 Real-time by default

Every surface subscribes to scoped live events:

- repo list status and activity
- branch and ref updates
- new commits and force-pushes
- MR review comments, approvals, thread resolution
- CI jobs, traces, bottlenecks, VTI plans
- agent work sessions, evidence capsules, gates
- settings changes, webhook deliveries, secret rotations
- notifications and audit events

The WebSocket stream must support resume cursors so a browser sleep/wake or deploy does not lose state.

### 3.4 Safety over raw power

All mutating actions go through the same pattern:

1. `POST /api/actions/preview`
2. server computes risk, grant requirement, target SHA, blast radius, side effects, undo path, and evidence expected
3. UI shows the preview
4. user confirms with idempotency key
5. `POST /api/actions/execute`
6. server emits audit/event/evidence records
7. WebSocket streams progress and completion

No destructive action should exist only as a raw button.

### 3.5 Agent-native by default

JeRyu’s differentiator is agent-aware governance. The web product must show:

- which agent proposed a change
- what evidence it produced
- what policy/grant allowed it
- what changed since evidence was generated
- whether the target branch policy changed
- whether exact-SHA approval is still valid
- what agent action is recommended next

---

## 4. Target architecture

### 4.1 Runtime layers

```text
┌────────────────────────────────────────────────────────────────────────────┐
│ apps/web — Vite + React + TypeScript                                       │
│                                                                            │
│  AppShell  CommandPalette  RepoSwitcher  ActivityRail  Toast/Dialogs       │
│      │          │             │              │              │              │
│      ├── pages/repos/*                                                     │
│      ├── pages/repo-home/*                                                 │
│      ├── pages/code/*                                                      │
│      ├── pages/merge-requests/*                                            │
│      ├── pages/issues/*                                                    │
│      ├── pages/settings/*                                                  │
│      ├── pages/admin/*                                                     │
│      └── api/client.ts + api/ws.ts + generated DTOs                        │
└──────────────────────────────┬─────────────────────────────────────────────┘
                               │ HTTPS / WebSocket
┌──────────────────────────────▼─────────────────────────────────────────────┐
│ src/web — Rust Web Forge BFF                                               │
│                                                                            │
│ router.rs       REST + static assets + websocket mount                     │
│ state.rs        WebState wrapper over Db, GitHost registry, EventHub        │
│ auth.rs         sessions, dev auth, RBAC hooks                              │
│ actions.rs      preview/execute adapter to existing action registry         │
│ repo_browser.rs repositories, refs, trees, blobs, history, compare          │
│ markdown.rs     README/Markdown render, sanitize, cache, rewrite links      │
│ merge_requests.rs review room, approvals, merge passport                    │
│ issues.rs       issues, labels, milestones, planning                        │
│ settings_api.rs searchable settings, branch protection, webhooks, secrets   │
│ event_hub.rs    event fanout, cursors, replay, topic subscriptions          │
│ ws.rs           protocol frames, heartbeat, gap recovery                    │
│ static_assets.rs SPA fallback                                               │
└──────────────────────────────┬─────────────────────────────────────────────┘
                               │ typed domain calls
┌──────────────────────────────▼─────────────────────────────────────────────┐
│ Existing JeRyu authority                                                    │
│                                                                            │
│ api/actions/events/read_model/snapshot/entity                               │
│ state Db / migrations / repo_fleet / repo_local / repo_standard             │
│ git_host GitHub/GitLab adapters                                             │
│ engine webhook/reconciliation                                                │
│ CI, runners, jobs, cache, VTI, release, secrets, agents, admission           │
└──────────────────────────────┬─────────────────────────────────────────────┘
                               │
┌──────────────────────────────▼─────────────────────────────────────────────┐
│ Git repos, local GitLab, GitHub/GitLab remotes, Vault, Docker, runners      │
└────────────────────────────────────────────────────────────────────────────┘
```

### 4.2 Target repository tree

```text
jeryu/
├─ Cargo.toml                              # add web deps/features
├─ package.json                            # real web scripts + keep ux-qa
├─ proof-lanes.toml                        # add web proof lanes
├─ docs/
│  ├─ JERYU_WEB_FORGE.md                   # operator/user docs
│  └─ JERYU_WEB_FORGE_API.md               # generated/reference API docs
├─ db/
│  ├─ state.rs                             # Db accessors for web tables
│  └─ migrations/
│     └─ 0005_web_forge.sql                # web product schema
├─ src/
│  ├─ lib.rs                               # pub mod web; api module exports
│  ├─ engine.rs                            # mount web router on existing engine
│  ├─ settings_types.rs                    # add web/auth/markdown/realtime settings
│  ├─ api/
│  │  ├─ mod.rs                            # export new DTO modules
│  │  ├─ repository.rs
│  │  ├─ code.rs
│  │  ├─ markdown.rs
│  │  ├─ merge_request.rs
│  │  ├─ review.rs
│  │  ├─ issue.rs
│  │  ├─ notification.rs
│  │  ├─ settings_dto.rs
│  │  └─ web_event.rs
│  ├─ git_host/
│  │  ├─ mod.rs                            # extend trait for forge reads/writes
│  │  ├─ github.rs                         # implement added trait methods
│  │  └─ gitlab.rs                         # implement added trait methods
│  └─ web/
│     ├─ mod.rs
│     ├─ config.rs
│     ├─ state.rs
│     ├─ router.rs
│     ├─ errors.rs
│     ├─ auth.rs
│     ├─ rbac.rs
│     ├─ csrf.rs
│     ├─ event_hub.rs
│     ├─ ws.rs
│     ├─ actions.rs
│     ├─ markdown.rs
│     ├─ repo_browser.rs
│     ├─ repo_admin.rs
│     ├─ merge_requests.rs
│     ├─ reviews.rs
│     ├─ issues.rs
│     ├─ settings_api.rs
│     ├─ search.rs
│     ├─ notifications.rs
│     ├─ audit.rs
│     ├─ openapi.rs
│     └─ static_assets.rs
├─ apps/
│  └─ web/
│     ├─ package.json
│     ├─ index.html
│     ├─ vite.config.ts
│     ├─ tsconfig.json
│     ├─ playwright.config.ts
│     ├─ ux-qa-check.mjs                   # retained and extended
│     ├─ src/
│     │  ├─ main.tsx
│     │  ├─ app/App.tsx
│     │  ├─ app/router.tsx
│     │  ├─ app/providers.tsx
│     │  ├─ api/client.ts
│     │  ├─ api/ws.ts
│     │  ├─ api/types.ts                   # generated from Rust or checked in initially
│     │  ├─ stores/*
│     │  ├─ components/*
│     │  ├─ pages/*
│     │  ├─ markdown/MarkdownView.tsx
│     │  ├─ styles/tokens.css
│     │  └─ styles/app.css
│     └─ tests/
│        ├─ repo-home.spec.ts
│        ├─ markdown-render.spec.ts
│        ├─ merge-room.spec.ts
│        └─ settings.spec.ts
└─ tests/
   ├─ web_api_tests.rs
   ├─ web_markdown_tests.rs
   ├─ web_ws_tests.rs
   └─ web_merge_request_tests.rs
```

---

## 5. Backend design

### 5.1 `src/web` module responsibilities

| Module | Responsibility |
|---|---|
| `mod.rs` | public entry point for `build_router`, `serve`, and shared types |
| `config.rs` | convert `Settings` into web runtime config |
| `state.rs` | `WebState { db, event_hub, host_registry, markdown_cache, action_dispatcher }` |
| `router.rs` | HTTP route composition and SPA fallback |
| `errors.rs` | typed API errors, JSON error envelope, status mapping |
| `auth.rs` | current user, sessions, dev auth, token auth, logout |
| `rbac.rs` | permission checks and role model |
| `csrf.rs` | CSRF token issue/validate for browser session mutations |
| `event_hub.rs` | fanout, retention, replay cursors, subscription matching |
| `ws.rs` | WebSocket protocol handler and heartbeat |
| `actions.rs` | preview/execute endpoint adapter |
| `markdown.rs` | Markdown render pipeline, sanitizer, relative URL rewriter, cache |
| `repo_browser.rs` | all repository read APIs, refs, tree, blob, blame, history, compare |
| `repo_admin.rs` | create/import/fork/mirror/adopt/archive/delete repository flows |
| `merge_requests.rs` | MR list/detail/review/approve/merge/rebase/update branch |
| `reviews.rs` | inline comments, threads, batch review state |
| `issues.rs` | issues, labels, milestones, linked branches/MRs |
| `settings_api.rs` | global/repo settings reads and writes |
| `search.rs` | global search, scoped search, saved filters |
| `notifications.rs` | inbox, watch/unwatch, mark read, notification preferences |
| `audit.rs` | audit query endpoint and mutation audit writer |
| `openapi.rs` | schema generation and API docs route |
| `static_assets.rs` | static build serving and SPA fallback |

### 5.2 Web router

Mount under the existing Axum server:

```rust
let web_state = crate::web::WebState::from_engine_state(state.clone());
let app = Router::new()
    .route("/health", get(health))
    .route("/hooks", post(handle_webhook))
    .route("/cache/summary", get(cache_summary))
    .nest("/api", crate::web::api_router(web_state.clone()))
    .route("/ws", get(crate::web::ws::ws_handler))
    .fallback_service(crate::web::static_assets::spa_service())
    .with_state(state.clone());
```

Compatibility requirement: `/health`, `/hooks`, and `/cache/summary` keep their current behavior.

### 5.3 API conventions

- JSON only for API responses.
- `X-Jeryu-Request-Id` on all responses.
- `Idempotency-Key` required for mutating actions.
- `X-CSRF-Token` required for browser session mutations.
- All mutation responses include `action_receipt` or an `event_cursor`.
- All list endpoints support `cursor`, `limit`, `sort`, `filter`, and `q` where applicable.
- All endpoints return typed error envelopes:

```json
{
  "error": {
    "code": "merge_sha_stale",
    "message": "The source branch changed after approval.",
    "details": { "expected_sha": "...", "actual_sha": "..." },
    "request_id": "...",
    "event_cursor": 1234
  }
}
```

### 5.4 Auth and RBAC

Initial implementation can support local/dev auth and token auth, but the shape must support production sessions.

Roles:

| Role | Scope | Capabilities |
|---|---|---|
| `viewer` | global/repo | read repositories, issues, MRs, CI, settings metadata |
| `reporter` | repo | create issues, comments, review comments |
| `developer` | repo | push branches, create MRs, run tests, retry jobs |
| `maintainer` | repo | approve, merge, manage branches/tags, manage runners/webhooks |
| `owner` | org/global | manage access, delete/archive, secrets, global settings |
| `agent` | grant-bound | only actions explicitly granted by capability policy |
| `system` | internal | reconciliation, webhooks, background jobs |

RBAC checks belong in the backend. The UI only hides or disables controls; it must never be the only enforcement point.

### 5.5 Action preview and execution

Endpoint pair:

```http
POST /api/actions/preview
POST /api/actions/execute
```

Preview output must include:

- `enabled`
- `disabled_reason`
- `risk`
- `side_effect_class`
- `blast_radius`
- `will_do`
- `will_not_do`
- `required_role`
- `required_grant`
- `target_entity`
- `target_sha`
- `expected_evidence`
- `undo_action`
- `confirmation_phrase`

Every destructive or production action must require a typed confirmation phrase and idempotency key.

---

## 6. REST API surface

### 6.1 Bootstrap and session

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/bootstrap` | Initial app config: user, permissions, feature flags, WebSocket URL, navigation counts, event cursor |
| `GET` | `/api/session` | Current user/session |
| `POST` | `/api/session/dev-login` | Local-only dev login |
| `POST` | `/api/session/logout` | Logout |
| `GET` | `/api/csrf` | Issue CSRF token |
| `GET` | `/api/health` | Web API health with component statuses |

### 6.2 All repositories

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos` | List all repos with filters, family grouping, live status, MR/issue/check counts |
| `POST` | `/api/repos` | Create repository |
| `POST` | `/api/repos/import` | Import from URL / GitHub / GitLab / local path |
| `POST` | `/api/repos/adopt` | Adopt existing checkout into JeRyu control |
| `GET` | `/api/repo-families` | Repo family summaries |
| `GET` | `/api/repos/:repoId` | Repo metadata |
| `PATCH` | `/api/repos/:repoId` | Rename/description/topics/default branch/visibility |
| `POST` | `/api/repos/:repoId/archive` | Archive repo |
| `POST` | `/api/repos/:repoId/unarchive` | Unarchive repo |
| `DELETE` | `/api/repos/:repoId` | Delete repo after danger-zone preview |

### 6.3 Repository home

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:repoId/overview` | README summary, repo health, branch, latest commit, open MRs/issues, CI, agents |
| `GET` | `/api/repos/:repoId/activity` | Paginated repository activity |
| `GET` | `/api/repos/:repoId/readme` | Rendered README HTML and metadata |
| `GET` | `/api/repos/:repoId/badges` | Status badge metadata |
| `GET` | `/api/repos/:repoId/clone-urls` | HTTP/SSH/local clone URLs |

### 6.4 Code browser

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:repoId/refs` | Branches/tags/default branch |
| `GET` | `/api/repos/:repoId/tree?ref=&path=` | Directory tree entries |
| `GET` | `/api/repos/:repoId/blob?ref=&path=` | File blob metadata/content or binary handle |
| `GET` | `/api/repos/:repoId/raw?ref=&path=` | Raw file download |
| `GET` | `/api/repos/:repoId/rendered?ref=&path=` | Render Markdown/AsciiDoc/text preview to safe HTML |
| `GET` | `/api/repos/:repoId/history?ref=&path=` | Commit history for path |
| `GET` | `/api/repos/:repoId/blame?ref=&path=` | Blame chunks |
| `GET` | `/api/repos/:repoId/commits/:sha` | Commit detail |
| `GET` | `/api/repos/:repoId/compare?base=&head=` | Compare refs and files |
| `GET` | `/api/repos/:repoId/search` | Code/path search |

### 6.5 Branches, tags, releases

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:repoId/branches` | List branches with protection and CI status |
| `POST` | `/api/repos/:repoId/branches` | Create branch |
| `DELETE` | `/api/repos/:repoId/branches/:branch` | Delete branch with preview |
| `GET` | `/api/repos/:repoId/tags` | List tags |
| `POST` | `/api/repos/:repoId/tags` | Create tag/release tag |
| `GET` | `/api/repos/:repoId/releases` | Release list |
| `GET` | `/api/repos/:repoId/releases/:id` | Release detail and evidence |

### 6.6 Issues and planning

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:repoId/issues` | List issues with saved filters |
| `POST` | `/api/repos/:repoId/issues` | Create issue |
| `GET` | `/api/repos/:repoId/issues/:issueId` | Issue detail |
| `PATCH` | `/api/repos/:repoId/issues/:issueId` | Update title/body/state/labels/assignees |
| `POST` | `/api/repos/:repoId/issues/:issueId/comments` | Comment |
| `GET` | `/api/repos/:repoId/labels` | Labels |
| `POST` | `/api/repos/:repoId/labels` | Create label |
| `GET` | `/api/repos/:repoId/milestones` | Milestones |
| `POST` | `/api/repos/:repoId/milestones` | Create milestone |
| `GET` | `/api/repos/:repoId/boards` | Boards/project views |

### 6.7 Merge requests / pull requests

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:repoId/merge-requests` | List MRs/PRs |
| `POST` | `/api/repos/:repoId/merge-requests` | Open MR |
| `GET` | `/api/repos/:repoId/merge-requests/:mrId` | MR detail |
| `PATCH` | `/api/repos/:repoId/merge-requests/:mrId` | Edit MR |
| `GET` | `/api/repos/:repoId/merge-requests/:mrId/files` | Changed file tree |
| `GET` | `/api/repos/:repoId/merge-requests/:mrId/diff` | Paginated diff |
| `GET` | `/api/repos/:repoId/merge-requests/:mrId/passport` | Merge passport/gate state |
| `POST` | `/api/repos/:repoId/merge-requests/:mrId/comments` | Top-level comment |
| `POST` | `/api/repos/:repoId/merge-requests/:mrId/threads` | Inline thread |
| `PATCH` | `/api/repos/:repoId/merge-requests/:mrId/threads/:threadId` | Resolve/unresolve thread |
| `POST` | `/api/repos/:repoId/merge-requests/:mrId/reviews` | Submit review batch |
| `POST` | `/api/repos/:repoId/merge-requests/:mrId/approve` | Approve exact SHA |
| `POST` | `/api/repos/:repoId/merge-requests/:mrId/unapprove` | Remove approval |
| `POST` | `/api/repos/:repoId/merge-requests/:mrId/update-branch` | Update branch from target |
| `POST` | `/api/repos/:repoId/merge-requests/:mrId/rebase` | Rebase source branch |
| `POST` | `/api/repos/:repoId/merge-requests/:mrId/merge` | Merge with strategy |
| `POST` | `/api/repos/:repoId/merge-requests/:mrId/close` | Close MR |

### 6.8 CI, runners, agents, evidence

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:repoId/pipelines` | Pipelines |
| `GET` | `/api/repos/:repoId/pipelines/:pipelineId` | Pipeline detail |
| `GET` | `/api/repos/:repoId/jobs` | Jobs |
| `GET` | `/api/repos/:repoId/jobs/:jobId/log` | Job log stream/chunks |
| `POST` | `/api/repos/:repoId/jobs/:jobId/retry` | Retry job |
| `POST` | `/api/repos/:repoId/jobs/:jobId/cancel` | Cancel job |
| `POST` | `/api/repos/:repoId/jobs/:jobId/play` | Trigger manual job |
| `GET` | `/api/repos/:repoId/test-plan` | VTI test plan |
| `POST` | `/api/repos/:repoId/test-plan/run` | Run selected tests |
| `GET` | `/api/repos/:repoId/agents` | Active agents |
| `GET` | `/api/repos/:repoId/evidence` | Evidence capsules |
| `GET` | `/api/repos/:repoId/evidence/:evidenceId` | Evidence detail |

### 6.9 Settings

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/settings` | Global settings projection |
| `PATCH` | `/api/settings` | Update global settings through preview |
| `GET` | `/api/repos/:repoId/settings` | Repo settings projection |
| `PATCH` | `/api/repos/:repoId/settings/general` | General repo settings |
| `PATCH` | `/api/repos/:repoId/settings/access` | Access and membership |
| `PATCH` | `/api/repos/:repoId/settings/branches` | Branch protection |
| `PATCH` | `/api/repos/:repoId/settings/merge` | Merge rules |
| `PATCH` | `/api/repos/:repoId/settings/webhooks` | Webhooks |
| `PATCH` | `/api/repos/:repoId/settings/secrets` | Secrets references / rotation policy |
| `PATCH` | `/api/repos/:repoId/settings/ci` | CI/runners/cache |
| `PATCH` | `/api/repos/:repoId/settings/agents` | Agent permissions |
| `PATCH` | `/api/repos/:repoId/settings/notifications` | Notification rules |
| `PATCH` | `/api/repos/:repoId/settings/retention` | Retention and cleanup |
| `GET` | `/api/repos/:repoId/audit` | Audit log |

---

## 7. WebSocket design

### 7.1 Endpoint

```http
GET /ws?cursor=<last_seen_event_seq>&topics=repo:42,mr:42:7,notifications:self
```

The client connects after `/api/bootstrap`, passing the last seen cursor. Server replies with a hello frame:

```json
{
  "type": "hello",
  "connection_id": "ws_...",
  "server_time": "2026-05-26T00:00:00Z",
  "cursor": 11922,
  "heartbeat_ms": 15000,
  "replay_supported": true,
  "max_topics": 128
}
```

### 7.2 Client messages

```ts
type ClientWsFrame =
  | { type: 'ping'; nonce: string }
  | { type: 'subscribe'; topics: string[] }
  | { type: 'unsubscribe'; topics: string[] }
  | { type: 'resume'; cursor: number; topics: string[] }
  | { type: 'ack'; cursor: number }
```

### 7.3 Server messages

```ts
type ServerWsFrame =
  | { type: 'hello'; connection_id: string; cursor: number; heartbeat_ms: number }
  | { type: 'pong'; nonce: string; server_time: string }
  | { type: 'event'; event: WebEvent }
  | { type: 'snapshot'; topic: string; cursor: number; payload: unknown }
  | { type: 'gap'; from: number; to: number; recovery: 'refetch' | 'replay' }
  | { type: 'subscribed'; topics: string[] }
  | { type: 'error'; code: string; message: string; retry_after_ms?: number }
```

### 7.4 Topic model

| Topic | Meaning |
|---|---|
| `global` | global health and activity |
| `repos` | repo list summary changes |
| `repo:{repoId}` | repo-specific activity and state |
| `repo:{repoId}:refs` | branch/tag/ref updates |
| `repo:{repoId}:ci` | pipeline/job/test updates |
| `repo:{repoId}:agents` | agent sessions/evidence |
| `mr:{repoId}:{mrId}` | MR review, diff, approval, gate updates |
| `issue:{repoId}:{issueId}` | issue comments/state updates |
| `settings:{repoId}` | settings changes |
| `notifications:{userId}` | inbox and toast notifications |
| `audit:{repoId}` | audit event stream, maintainer+ only |

### 7.5 Event envelope

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebEvent {
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub topic: String,
    pub kind: WebEventKind,
    pub actor: ActorRef,
    pub entity: WebEntityRef,
    pub summary: String,
    pub payload: serde_json::Value,
    pub stale_after_ms: u64,
    pub dedupe_key: Option<String>,
}
```

### 7.6 Reliability rules

- Server retains recent events in memory and durable state. Initial default: 24 hours or 50,000 events, whichever comes first.
- Client persists last cursor in local storage per user/session.
- If replay cannot bridge a cursor gap, send `gap` and the client refetches affected read models.
- Slow clients get compressed snapshots instead of unbounded queues.
- Topic authorization is checked on subscribe and periodically on settings/membership changes.
- Event frames are versioned. Unknown event kinds must be ignored safely by old clients.

---

## 8. Markdown and README rendering

### 8.1 Requirements

The user specifically asked that `*.md` render correctly to HTML so users can see README content. Required behavior:

- Find README by preferred order: `README.md`, `README.markdown`, `README.mdown`, `README.rst`, `README.txt`, case-insensitive fallback.
- Render GitHub-flavored Markdown: tables, task lists, strikethrough, autolinks, fenced code blocks, footnotes where feasible.
- Sanitize all HTML. No script, event handler, unsafe URL, iframe, style injection, or untrusted raw HTML.
- Rewrite relative links and images:
  - relative file links → internal code browser route
  - relative image links → raw blob endpoint with same ref
  - anchor links → generated heading anchors
  - absolute `http`/`https` links allowed with `rel="noopener noreferrer"`
- Cache render by `(repo_id, ref_sha, path, blob_sha, renderer_version)`.
- Return metadata: title, headings, toc, links, images, warnings, source path, blob SHA.
- Provide plain text extraction for search and summaries.
- Render Markdown server-side; React receives sanitized HTML and never sanitizes less strictly.

### 8.2 Rendering pipeline

```text
resolve repo/ref/path
  ↓
read blob bytes from git2/system Git/host adapter
  ↓
UTF-8 decode with size limit
  ↓
comrak render with GFM extensions
  ↓
HTML sanitize with ammonia policy
  ↓
relative URL rewrite + heading anchor postprocess
  ↓
syntax highlighting metadata / code class normalization
  ↓
cache by blob SHA
  ↓
return RenderedMarkdown DTO
```

### 8.3 API response

```json
{
  "repo_id": "r_42",
  "ref_name": "main",
  "commit_sha": "abc123",
  "path": "README.md",
  "blob_sha": "sha1...",
  "html": "<h1 id=\"jeryu\">JeRyu</h1>...",
  "toc": [{ "depth": 1, "id": "jeryu", "text": "JeRyu" }],
  "links": [{ "href": "docs/API.md", "rewritten_href": "/root/jeryu/blob/main/docs/API.md" }],
  "warnings": []
}
```

### 8.4 UI controls

- Rendered/source toggle.
- Copy Markdown source.
- Copy rendered section link.
- Expand/collapse table of contents.
- Open relative link in code browser.
- Preview Markdown diffs in MR review.
- Warn when Markdown was partially rendered due to size/security limits.

---

## 9. Data model additions

The schema must support local SQLite and RedlineDB-compatible SQL through the existing state abstraction. Use text IDs with stable prefixes for product objects (`repo_`, `user_`, `mr_`) unless upstream host IDs are canonical.

### 9.1 Identity and authorization

```sql
CREATE TABLE web_users (
  id TEXT PRIMARY KEY,
  login TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  email TEXT,
  avatar_url TEXT,
  auth_provider TEXT NOT NULL DEFAULT 'local',
  provider_subject TEXT,
  status TEXT NOT NULL DEFAULT 'active',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE web_sessions (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES web_users(id),
  token_hash TEXT NOT NULL UNIQUE,
  csrf_secret_hash TEXT NOT NULL,
  user_agent TEXT,
  ip_hash TEXT,
  expires_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  last_seen_at TEXT NOT NULL
);

CREATE TABLE web_memberships (
  id TEXT PRIMARY KEY,
  subject_kind TEXT NOT NULL,     -- user | team | agent
  subject_id TEXT NOT NULL,
  scope_kind TEXT NOT NULL,       -- global | org | repo
  scope_id TEXT NOT NULL,
  role TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(subject_kind, subject_id, scope_kind, scope_id)
);
```

### 9.2 Repositories

```sql
CREATE TABLE web_repositories (
  id TEXT PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  provider TEXT NOT NULL,          -- local | github | gitlab | mirror
  provider_project_id TEXT,
  default_branch TEXT NOT NULL DEFAULT 'main',
  visibility TEXT NOT NULL DEFAULT 'private',
  local_path TEXT,
  remote_url TEXT,
  family TEXT,
  topics_json TEXT NOT NULL DEFAULT '[]',
  archived INTEGER NOT NULL DEFAULT 0,
  mirror_enabled INTEGER NOT NULL DEFAULT 0,
  last_indexed_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE web_repo_refs (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES web_repositories(id),
  ref_kind TEXT NOT NULL,          -- branch | tag
  name TEXT NOT NULL,
  sha TEXT NOT NULL,
  protected INTEGER NOT NULL DEFAULT 0,
  default_ref INTEGER NOT NULL DEFAULT 0,
  last_pipeline_status TEXT,
  updated_at TEXT NOT NULL,
  UNIQUE(repo_id, ref_kind, name)
);

CREATE TABLE web_branch_protections (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES web_repositories(id),
  pattern TEXT NOT NULL,
  require_mr INTEGER NOT NULL DEFAULT 1,
  required_approvals INTEGER NOT NULL DEFAULT 1,
  require_codeowners INTEGER NOT NULL DEFAULT 0,
  require_resolved_threads INTEGER NOT NULL DEFAULT 1,
  require_merge_passport INTEGER NOT NULL DEFAULT 1,
  allow_force_push INTEGER NOT NULL DEFAULT 0,
  allow_deletions INTEGER NOT NULL DEFAULT 0,
  stale_approval_policy TEXT NOT NULL DEFAULT 'dismiss_on_source_change',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(repo_id, pattern)
);
```

### 9.3 Issues, labels, milestones

```sql
CREATE TABLE web_labels (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES web_repositories(id),
  name TEXT NOT NULL,
  color TEXT NOT NULL,
  description TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(repo_id, name)
);

CREATE TABLE web_milestones (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES web_repositories(id),
  title TEXT NOT NULL,
  description TEXT,
  due_at TEXT,
  state TEXT NOT NULL DEFAULT 'open',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE web_issues (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES web_repositories(id),
  iid INTEGER NOT NULL,
  title TEXT NOT NULL,
  body TEXT NOT NULL DEFAULT '',
  author_id TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'open',
  labels_json TEXT NOT NULL DEFAULT '[]',
  assignees_json TEXT NOT NULL DEFAULT '[]',
  milestone_id TEXT,
  linked_mrs_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  closed_at TEXT,
  UNIQUE(repo_id, iid)
);
```

### 9.4 Merge requests and reviews

```sql
CREATE TABLE web_merge_requests (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES web_repositories(id),
  iid INTEGER NOT NULL,
  title TEXT NOT NULL,
  body TEXT NOT NULL DEFAULT '',
  author_id TEXT NOT NULL,
  source_branch TEXT NOT NULL,
  target_branch TEXT NOT NULL,
  source_sha TEXT NOT NULL,
  target_sha TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'open',
  draft INTEGER NOT NULL DEFAULT 0,
  labels_json TEXT NOT NULL DEFAULT '[]',
  assignees_json TEXT NOT NULL DEFAULT '[]',
  reviewers_json TEXT NOT NULL DEFAULT '[]',
  merge_strategy TEXT NOT NULL DEFAULT 'merge_commit',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  merged_at TEXT,
  closed_at TEXT,
  UNIQUE(repo_id, iid)
);

CREATE TABLE web_mr_review_threads (
  id TEXT PRIMARY KEY,
  mr_id TEXT NOT NULL REFERENCES web_merge_requests(id),
  file_path TEXT,
  old_path TEXT,
  side TEXT,                       -- old | new | unchanged
  line INTEGER,
  start_line INTEGER,
  resolved INTEGER NOT NULL DEFAULT 0,
  resolved_by TEXT,
  resolved_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE web_mr_comments (
  id TEXT PRIMARY KEY,
  mr_id TEXT NOT NULL REFERENCES web_merge_requests(id),
  thread_id TEXT REFERENCES web_mr_review_threads(id),
  author_id TEXT NOT NULL,
  body TEXT NOT NULL,
  body_html TEXT,
  system INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE web_mr_approvals (
  id TEXT PRIMARY KEY,
  mr_id TEXT NOT NULL REFERENCES web_merge_requests(id),
  user_id TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  status TEXT NOT NULL,            -- approved | revoked | stale
  receipt_digest TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(mr_id, user_id, head_sha)
);

CREATE TABLE web_merge_passports (
  id TEXT PRIMARY KEY,
  mr_id TEXT NOT NULL REFERENCES web_merge_requests(id),
  head_sha TEXT NOT NULL,
  status TEXT NOT NULL,            -- pass | blocked | running | unknown
  summary TEXT NOT NULL,
  checks_json TEXT NOT NULL,
  evidence_refs_json TEXT NOT NULL DEFAULT '[]',
  generated_at TEXT NOT NULL,
  UNIQUE(mr_id, head_sha)
);
```

### 9.5 Markdown, notifications, activity, audit

```sql
CREATE TABLE web_markdown_cache (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES web_repositories(id),
  commit_sha TEXT NOT NULL,
  path TEXT NOT NULL,
  blob_sha TEXT NOT NULL,
  renderer_version TEXT NOT NULL,
  html TEXT NOT NULL,
  toc_json TEXT NOT NULL DEFAULT '[]',
  links_json TEXT NOT NULL DEFAULT '[]',
  warnings_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  UNIQUE(repo_id, blob_sha, path, renderer_version)
);

CREATE TABLE web_activity_events (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  topic TEXT NOT NULL,
  kind TEXT NOT NULL,
  actor_kind TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  entity_kind TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  summary TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE web_notifications (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES web_users(id),
  topic TEXT NOT NULL,
  kind TEXT NOT NULL,
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  entity_kind TEXT,
  entity_id TEXT,
  read_at TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE web_audit_events (
  id TEXT PRIMARY KEY,
  actor_kind TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  action TEXT NOT NULL,
  scope_kind TEXT NOT NULL,
  scope_id TEXT NOT NULL,
  target_kind TEXT,
  target_id TEXT,
  risk TEXT NOT NULL,
  status TEXT NOT NULL,
  request_id TEXT NOT NULL,
  idempotency_key TEXT,
  payload_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);
```

---

## 10. Frontend UX design

### 10.1 Global app shell

Persistent regions:

- **Top nav:** repo switcher, global search, create menu, command palette, notifications, user menu.
- **Left rail:** primary navigation and repo-family shortcuts.
- **Main content:** route-specific surface.
- **Right activity rail:** live events, blockers, recommendations, evidence, selected entity inspector.
- **Bottom status bar:** WebSocket status, event cursor, selected repo/ref, keyboard hints.

Keyboard defaults:

| Shortcut | Action |
|---|---|
| `⌘K` / `Ctrl+K` | command palette |
| `g r` | all repos |
| `g c` | current repo code |
| `g m` | current repo merge requests |
| `g i` | issues |
| `g s` | settings |
| `[` / `]` | previous/next file in diff |
| `j` / `k` | next/previous row/comment/hunk |
| `a` | approve current MR if available |
| `m` | open merge preview |
| `r` | reply/comment |
| `e` | edit selected item |
| `?` | keyboard help |

### 10.2 All repositories dashboard

Features:

- Search by name, description, owner, family, topic, status, language, active agent, failed check.
- Filters: owned, starred, recent, archived, private, public, local, GitHub, GitLab, mirrored, dirty, blocked, active CI, active agents.
- Grouping: owner/org, family, provider, health, last activity.
- Cards show: name, description, default branch, latest commit, CI status, open MRs/issues, active agents, blockers, unread notifications.
- Quick actions: create repo, import, adopt local checkout, open, clone, settings, run health check.
- Bulk actions: sync, mirror, archive, apply standard, run repo audit, update settings template.

### 10.3 Repository home

Sections:

- Repo summary header with clone URLs, default branch, visibility, topics, stars/watchers if external provider supports them.
- Live health strip: branch protection, CI, agents, runners, cache, secrets, release.
- README card using server-rendered Markdown.
- Latest commit and branch status.
- Open MRs and blockers.
- Active issues and milestones.
- CI/test/pipeline summary.
- Agent activity and evidence capsules.
- Recent activity timeline.

Controls:

- Switch branch/tag.
- New file, upload, create branch, open MR, run tests.
- Copy clone URL.
- Render/source README toggle.
- Pin repo, watch/unwatch, star/favorite.
- Open settings/direct route via command palette.

### 10.4 Code browser

Layout:

```text
┌ repo header / branch selector / path breadcrumbs / actions ┐
├ left tree (virtualized, filterable) ┬ blob viewer / preview ┤
│                                     ├ commit strip          │
│                                     ├ code/markdown/binary  │
│                                     └ right inspector       │
└ status: bytes, language, last commit, line selection, cursor ┘
```

Controls:

- Branch/tag selector with default branch pinned.
- Breadcrumb path.
- Tree search/filter.
- Go to file.
- Copy path.
- Copy permalink to SHA.
- Copy raw URL.
- Download file.
- View raw.
- View blame.
- View history.
- Open compare from selected commit.
- Edit file / propose change.
- New file / upload / delete file through preview.
- Markdown rendered/source toggle.
- Syntax highlighting and line anchors.
- Binary preview for images, SVG with sanitization, PDFs as download only unless safe viewer configured.

### 10.5 Merge Room

The Merge Room replaces the scattered PR/MR experience with one screen:

```text
┌ MR header: title, draft/state, source→target, head SHA, actions          ┐
├ Merge Passport: exact-SHA, approvals, threads, CI, agents, policy        ┤
├ left: changed-file tree + filters     ┬ center: diff viewer              ┤
│ filters: viewed/unviewed, comments,   │ unified/split, comments, hunks   │
│ owner, generated, test-only           │                                   │
├ timeline/comments/review summary      ┴ right: evidence/agents/checks    ┤
└ sticky review bar: comment, approve, request changes, merge preview       ┘
```

Merge Passport checks:

- Source SHA unchanged since approval.
- Target branch SHA and policy SHA checked.
- Required approvals count and roles.
- Code owners satisfied.
- All threads resolved.
- Required CI green.
- VTI/test plan acceptable.
- Agent evidence fresh and signed.
- Secrets/deploy policy satisfied.
- Branch protection satisfied.
- No stale review due to force-push.
- Merge conflict status.
- Release window/deploy freeze if production path touched.

Controls:

- Assign reviewers/assignees/labels/milestone.
- Toggle draft/ready.
- Update branch/rebase.
- Rerun checks or failed jobs.
- Ask agent for fix/explanation/test.
- Add inline comments, suggestions, batch review.
- Mark files viewed.
- Approve exact SHA.
- Request changes.
- Merge preview.
- Merge via merge commit/squash/rebase/fast-forward according to settings.
- Delete source branch.
- Close/reopen.

### 10.6 Settings experience

Settings must be searchable and command-palette addressable. Every section has:

- current value
- inherited/default value
- risk tier
- last changed by/at
- preview before mutation
- audit link
- reset-to-default where safe

Major sections:

1. General: name, description, avatar, topics, default branch, visibility, archive.
2. Access: members, teams, roles, invitations, agent identities, deploy keys.
3. Branches: protection rules, CODEOWNERS, required checks, force-push, deletion, signed commits.
4. Merge: approvals, stale reviews, merge strategies, auto-merge, delete source, squash message rules.
5. Webhooks: endpoints, secrets, events, delivery logs, retries, redelivery.
6. Secrets: Vault authority, secret references, rotation policy, environment scoping, audit.
7. CI/runners: runner pools, tags, concurrency, VTI policy, cache, artifacts, retention.
8. Agents: allowed actions, max risk, approval flows, grant TTLs, evidence requirements.
9. Notifications: watch rules, inbox, email/webhook/desktop preferences.
10. Markdown/rendering: renderer version, max size, allowed raw HTML, mermaid/diagrams, image proxy.
11. Security: token policy, sessions, MFA/OIDC hooks, audit retention, gitleaks, dependency scanning.
12. Integrations: GitHub/GitLab remotes, mirrors, MCP, IDE links.
13. Backups/mirrors: shadow remotes, backup schedule, restore drills.
14. Retention: logs, artifacts, activity, notifications, cache cleanup.
15. Danger zone: archive, transfer, rename slug, delete, purge mirrors.

---

## 11. Settings additions

Add to `Settings`:

```rust
pub struct Settings {
    // existing fields...
    pub web: WebSettings,
    pub auth: AuthSettings,
    pub markdown: MarkdownSettings,
    pub realtime: RealtimeSettings,
    pub notifications: NotificationSettings,
}
```

Recommended defaults:

```rust
pub struct WebSettings {
    pub enabled: bool,                    // true
    pub bind: String,                     // 127.0.0.1:9780 for standalone or same engine bind when nested
    pub public_base_url: Option<String>,
    pub static_dir: String,               // apps/web/dist
    pub api_prefix: String,               // /api
    pub ws_path: String,                  // /ws
    pub dev_cors_origins: Vec<String>,    // http://127.0.0.1:5173
    pub gzip_static_assets: bool,
    pub spa_fallback: bool,
}

pub struct AuthSettings {
    pub mode: String,                     // local_dev | token | oidc
    pub session_ttl_hours: u64,           // 168
    pub require_csrf: bool,               // true
    pub cookie_secure: bool,              // false for local
    pub cookie_same_site: String,         // lax
    pub dev_user: String,                 // local-admin
}

pub struct MarkdownSettings {
    pub max_bytes: usize,                 // 2_000_000
    pub cache_enabled: bool,              // true
    pub allow_raw_html: bool,             // false
    pub syntax_highlight: bool,           // true
    pub renderer_version: String,         // jeryu-comrak-v1
    pub image_proxy: bool,                // true later
}

pub struct RealtimeSettings {
    pub enabled: bool,                    // true
    pub heartbeat_ms: u64,                // 15000
    pub replay_window_events: usize,      // 50000
    pub replay_window_seconds: u64,       // 86400
    pub max_topics_per_connection: usize, // 128
    pub max_client_queue: usize,          // 2048
}

pub struct NotificationSettings {
    pub inbox_enabled: bool,
    pub desktop_enabled: bool,
    pub email_enabled: bool,
    pub default_watch_policy: String,     // participating
}
```

---

## 12. Git host adapter expansion

The existing `GitHost` trait already models checks, comments, approvals, open PRs, live PR state, and diffs. Extend it for forge reads/writes:

```rust
#[async_trait]
pub trait GitForgeHost: GitHost {
    async fn list_repositories(&self, scope: ForgeRepoScope) -> Result<Vec<ForgeRepository>, HostError>;
    async fn create_repository(&self, input: CreateRepositoryInput) -> Result<ForgeRepository, HostError>;
    async fn get_repository(&self, repo: &RepoRef) -> Result<ForgeRepository, HostError>;
    async fn list_refs(&self, repo: &RepoRef) -> Result<Vec<ForgeRef>, HostError>;
    async fn get_tree(&self, repo: &RepoRef, reference: &str, path: &str) -> Result<TreeListing, HostError>;
    async fn get_blob(&self, repo: &RepoRef, reference: &str, path: &str) -> Result<BlobContent, HostError>;
    async fn get_commit(&self, repo: &RepoRef, sha: &str) -> Result<CommitDetail, HostError>;
    async fn compare_refs(&self, repo: &RepoRef, base: &str, head: &str) -> Result<CompareResult, HostError>;
    async fn create_merge_request(&self, repo: &RepoRef, input: CreateMergeRequestInput) -> Result<MergeRequestDetail, HostError>;
    async fn merge_request_detail(&self, repo: &RepoRef, mr_iid: &str) -> Result<MergeRequestDetail, HostError>;
    async fn merge(&self, repo: &RepoRef, mr_iid: &str, input: MergeInput) -> Result<MergeResult, HostError>;
}
```

Local repositories can implement the same surface through `git2` and JeRyu state tables; GitHub/GitLab adapters map to provider APIs.

---

## 13. Frontend implementation details

### 13.1 Dependencies

Use:

- `@vitejs/plugin-react`
- `typescript`
- `react`, `react-dom`
- `@tanstack/react-query`
- `@tanstack/react-router` or `react-router-dom`
- `zustand`
- `cmdk`
- `lucide-react`
- `react-virtual`
- `monaco-editor` or lightweight code viewer in first phase
- `diff2html` or custom diff components
- `dompurify` only as a client-side defense-in-depth check; server sanitizer remains authoritative
- `playwright` for E2E
- `vitest` and Testing Library
- `msw` for API mocks

### 13.2 State boundaries

| State | Tool | Rule |
|---|---|---|
| Server data | React Query | Query keys mirror API resources; WebSocket invalidates or patches queries. |
| UI shell | Zustand | Sidebar, activity rail, command palette, layout prefs. |
| Review draft | Zustand + local storage | Draft comments and file viewed state; sync on submit. |
| WebSocket | dedicated client | Own reconnection/resume/ack loop. |
| Forms | local component state | Submit through action preview. |

### 13.3 Route map

```text
/
/repos
/new
/import
/:owner/:repo
/:owner/:repo/code/:ref/*path
/:owner/:repo/blob/:ref/*path
/:owner/:repo/commits/:ref
/:owner/:repo/commit/:sha
/:owner/:repo/compare/:base...:head
/:owner/:repo/issues
/:owner/:repo/issues/:issue
/:owner/:repo/merge-requests
/:owner/:repo/merge-requests/:mr
/:owner/:repo/pipelines
/:owner/:repo/jobs/:job
/:owner/:repo/agents
/:owner/:repo/evidence/:evidence
/:owner/:repo/settings/*section
audit
notifications
admin
```

### 13.4 Component inventory

Core layout:

- `AppShell`
- `TopNav`
- `RepoSwitcher`
- `CommandPalette`
- `ActivityRail`
- `EntityInspector`
- `KeyboardHelpDialog`
- `ActionPreviewDialog`
- `ToastCenter`
- `StatusBar`

Repository:

- `RepoCard`
- `RepoFamilySection`
- `RepoHealthPills`
- `CloneUrlPopover`
- `RepoHeader`
- `ReadmeCard`
- `BranchSelector`
- `TopicPills`

Code:

- `CodeBrowserLayout`
- `FileTree`
- `PathBreadcrumbs`
- `BlobViewer`
- `MarkdownView`
- `BinaryPreview`
- `CommitStrip`
- `BlameOverlay`
- `LineAnchorGutter`

Review:

- `MergeRoom`
- `MergePassport`
- `DiffFileTree`
- `DiffViewer`
- `DiffHunk`
- `InlineThread`
- `ReviewComposer`
- `ReviewSummaryPanel`
- `CheckRunPanel`
- `EvidencePanel`

Settings:

- `SettingsLayout`
- `SettingsSearch`
- `SettingsSectionCard`
- `BranchProtectionEditor`
- `WebhookEditor`
- `SecretReferenceEditor`
- `RunnerPoolEditor`
- `AgentPolicyEditor`
- `DangerZoneCard`

---

## 14. User controls inventory

This section intentionally enumerates high-value controls so implementation does not accidentally produce a passive read-only viewer.

### 14.1 Global controls

- Create repository
- Import repository
- Adopt local checkout
- Clone repository
- Switch repository
- Switch repo family
- Global search
- Command palette
- Notifications inbox
- Mark all notifications read
- Toggle activity rail
- Toggle compact/dense mode
- Toggle theme
- Open keyboard help
- Open admin dashboard
- Open audit log
- Open docs/API
- Open WebSocket status inspector

### 14.2 Repository controls

- Pin/unpin repo
- Watch/unwatch repo
- Star/favorite repo
- Copy clone URL
- Switch branch/tag
- Create branch
- Create tag
- Open new MR from branch
- Run repo health audit
- Sync remote
- Mirror now
- Backup now
- Apply repo standard
- Install hooks
- Open settings
- Archive/unarchive
- Delete with preview

### 14.3 Code controls

- Go to file
- Search tree
- Search code
- Copy path
- Copy permalink
- Copy raw URL
- Download file
- View raw
- Render Markdown
- View source
- Open blame
- Open history
- Compare with branch/tag
- Edit file
- Propose change
- Delete file
- Create file
- Upload file
- Fold/unfold directories
- Copy selected lines permalink
- Open selected path in MR diff if part of current MR

### 14.4 Issue controls

- Create issue
- Edit title/body
- Assign/unassign
- Add/remove labels
- Set milestone
- Link MR
- Convert task to issue
- Close/reopen
- Subscribe/unsubscribe
- Lock conversation
- Pin issue
- Create branch from issue
- Ask agent to attempt issue

### 14.5 MR controls

- Create MR
- Edit title/body
- Mark draft/ready
- Assign reviewers
- Assign assignees
- Add labels/milestone
- View changed files
- Mark file viewed
- Toggle unified/split diff
- Hide whitespace
- Filter generated files
- Add inline comment
- Add suggestion
- Start review
- Submit comment/approve/request changes
- Resolve/unresolve thread
- Rerun failed checks
- Run selected tests
- Ask agent to fix failure
- Update branch
- Rebase
- Approve exact SHA
- Revoke approval
- Merge preview
- Merge
- Squash merge
- Rebase merge
- Fast-forward merge
- Delete source branch
- Close/reopen

### 14.6 CI/agent controls

- Retry job
- Cancel job
- Play manual job
- View log live
- Download log
- Open failure capsule
- Explain blocker
- Open VTI test plan
- Run selected tests
- Run full tests
- Pause pool
- Resume pool
- Drain pool
- Scale pool
- Rotate runner token
- Spawn agent
- Pause agent
- Stop agent
- Grant capability
- Revoke grant
- Open evidence pack
- Compare evidence to current SHA

### 14.7 Settings controls

- Search settings
- Export settings JSON
- Import settings JSON with preview
- Reset section to defaults
- View audit history per setting
- Preview setting mutation
- Save as template
- Apply template to repo family
- Manage members/teams
- Create branch protection rule
- Edit branch protection rule
- Test webhook
- Redeliver webhook
- Rotate webhook secret
- Add secret reference
- Rotate secret
- Configure runner pool
- Configure cache/VTI
- Configure agent max risk
- Configure notification policy
- Configure Markdown renderer
- Configure retention
- Archive/delete danger-zone flows

---

## 15. Engineering phases

### Phase 0 — Safety and scaffolding

- Add Rust dependencies/features.
- Replace `apps/web` package with Vite scaffold while retaining UX-QA check.
- Add `src/web` module and mount `/api/bootstrap`, `/api/health`, and `/ws` hello.
- Add settings defaults.
- Add proof lanes and CI checks.

Acceptance:

- `npm run web:build` succeeds.
- `cargo check -p jeryu` succeeds.
- Existing webhook routes still respond.
- Browser app loads with bootstrap data.

### Phase 1 — Repos and README

- Add repo list/read endpoints.
- Add repo home with README render.
- Add Markdown cache and sanitizer.
- Add code tree/blob read endpoints.
- Add basic WebSocket repo activity.

Acceptance:

- All registered repos show in browser.
- README renders as sanitized HTML with relative links/images rewritten.
- Code browser opens text and Markdown files.
- Live repo activity updates without refresh.

### Phase 2 — Merge Room

- Add MR list/detail/files/diff endpoints.
- Add review threads/comments/approvals.
- Add Merge Passport projection.
- Add action preview for approve/merge/update branch/rebase.

Acceptance:

- Reviewer can inspect diffs, comment inline, approve exact SHA, and merge only when passport passes.
- Stale SHA approval is visibly invalidated on source branch changes.

### Phase 3 — Issues and settings

- Add issues, labels, milestones.
- Add repo/global settings projections and mutations.
- Add audit views.
- Add webhook and branch protection editors.

Acceptance:

- Settings are searchable and previewed before mutation.
- Branch protection and webhooks can be configured through UI.
- Audit log records every mutation.

### Phase 4 — Full real-time cockpit

- Expand WebSocket topics for CI/jobs/logs/agents/evidence/settings/notifications.
- Add notification inbox.
- Add activity rail inspector.
- Add reconnect/resume/gap tests.

Acceptance:

- Sleeping/waking browser catches up using cursor replay or refetch gap recovery.
- Activity rail shows repo, MR, CI, agent, settings, and notification updates live.

### Phase 5 — Better-than-incumbent polish

- Command palette coverage for every high-value action.
- Keyboard-first review.
- Repo family dashboard.
- Bulk actions/templates.
- Performance budgets and virtualized large repo/diff tests.
- Accessibility and visual regression proof.

Acceptance:

- 95th percentile navigation under target budgets on large repo fixtures.
- Keyboard-only user can complete repo browse, review, approve, merge, and settings flows.

---

## 16. Testing and proof lanes

### 16.1 Rust tests

Add:

- `tests/web_api_tests.rs`
- `tests/web_markdown_tests.rs`
- `tests/web_ws_tests.rs`
- `tests/web_merge_request_tests.rs`
- `src/web/*_tests.rs` for unit-level modules

Coverage:

- bootstrap payload
- auth/RBAC enforcement
- CSRF rejection
- idempotency enforcement
- repo list filters
- tree/blob path traversal rejection
- Markdown sanitizer blocks scripts/events/unsafe URLs
- relative links/images rewrite correctly
- README fallback ordering
- WebSocket subscribe/ack/resume/gap
- MR exact-SHA approval and stale invalidation
- merge preview blocks when passport fails
- audit record creation

### 16.2 Frontend tests

Add:

- Vitest component tests for layout, stores, API client, WebSocket reducer.
- Playwright E2E for repo home, Markdown render, code browser, Merge Room, settings search, command palette.
- Accessibility checks on core routes.
- Visual regression screenshots for app shell, repo dashboard, README, code browser, Merge Room, settings.

### 16.3 Proof lanes

Add to `proof-lanes.toml`:

```toml
[lanes.web-api]
paths = ["src/web/**", "src/api/**", "tests/web_*", "db/**"]
commands = [
  "cargo check -p jeryu",
  "cargo nextest run -p jeryu --test web_api_tests",
  "cargo nextest run -p jeryu --test web_markdown_tests"
]

[lanes.web-ui]
paths = ["apps/web/**", "package.json"]
commands = [
  "npm --workspace @jeryu/web run build",
  "npm --workspace @jeryu/web run test",
  "npm --workspace @jeryu/web run ux-qa"
]

[lanes.websocket-realtime]
paths = ["src/web/ws.rs", "src/web/event_hub.rs", "apps/web/src/api/ws.ts"]
commands = [
  "cargo nextest run -p jeryu --test web_ws_tests",
  "npm --workspace @jeryu/web run test -- ws"
]

[lanes.review-merge]
paths = ["src/web/merge_requests.rs", "src/api/merge_request.rs", "apps/web/src/pages/merge-requests/**"]
commands = [
  "cargo nextest run -p jeryu --test web_merge_request_tests",
  "npm --workspace @jeryu/web run e2e -- merge-room"
]
```

---

## 17. Performance budgets

| Surface | Target |
|---|---:|
| Initial shell bootstrap local | < 700 ms |
| Repo dashboard with 1,000 repos | < 1.5 s first useful render |
| Repo search/filter | < 100 ms local UI response |
| Tree open 10,000 entries | virtualized, no long main-thread block > 50 ms |
| README render cached | < 100 ms backend |
| README render uncached 500 KB | < 500 ms backend |
| MR diff 500 files | paginated, first file visible < 1.5 s |
| WebSocket event delivery local | p95 < 250 ms after server publish |
| Settings search | < 50 ms UI response |

---

## 18. Security requirements

- Bind locally by default.
- No public exposure without explicit settings.
- CSRF required for cookie/session mutations.
- Session cookies HTTP-only, same-site lax/strict, secure when public base URL is HTTPS.
- Path traversal blocked for all file APIs.
- Markdown HTML sanitized server-side.
- Raw file downloads set safe content disposition for risky MIME types.
- Secrets never returned in settings projections; only references/fingerprints/status.
- Webhook secrets are write-only after creation.
- Tokens are hashed at rest.
- Audit every mutation with actor, target, request ID, idempotency key, risk, status.
- RBAC checked server-side.
- WebSocket subscription authorization checked server-side.
- Destructive actions require preview and confirmation.

---

## 19. Acceptance criteria for “full GitHub/GitLab experience, but better”

The implementation is complete when a user can:

1. Open the browser app and see every repository they can access.
2. Create, import, adopt, mirror, archive, and configure repositories.
3. Browse repository files and history across branches/tags.
4. View `README.md` and Markdown files as safe rendered HTML with correct links/images.
5. Search and navigate faster than incumbent forge page hopping.
6. Open issues, labels, milestones, and linked work.
7. Open MRs/PRs, inspect diffs, comment inline, submit reviews, approve exact SHA, and merge.
8. Understand all merge blockers in one Merge Passport.
9. Watch CI/jobs/logs/agents/evidence update live through WebSockets.
10. Configure branch protection, webhooks, access, secrets, CI, agents, notifications, retention, and danger-zone operations.
11. Use keyboard and command palette for all high-value actions.
12. Trust every mutation because it has preview, RBAC, idempotency, audit, and evidence.

---

## 20. Key implementation invariants

- Web is a BFF/projection/action surface, not a second source of truth.
- Existing CLI/TUI/webhook routes must keep working.
- Read models are typed; React does not scrape raw logs or SQL-shaped payloads.
- All mutations go through action preview or an equivalent specialized preview.
- README/Markdown HTML is sanitized before it reaches the browser.
- WebSocket events have monotonic cursors and recovery behavior.
- Git approvals bind to exact source SHA.
- Settings merges preserve explicit user config and tolerate unknown keys.
- Tests cover security-sensitive flows before UI polish.

