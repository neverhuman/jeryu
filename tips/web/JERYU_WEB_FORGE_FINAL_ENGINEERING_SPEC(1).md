# JeRyu Web Forge: Full GitHub/GitLab++ Engineering Specification

Status: **Best-and-final combined engineering spec**  
Target stack: **Rust / Axum / Vite / TypeScript / React**  
Primary goal: turn JeRyu from a powerful agent-native Git/CI control plane with a placeholder browser workspace into a full modern Git hosting web experience that covers repository discovery, creation, code browsing, README rendering, merge approvals, review workflows, settings, activity, and real-time operational telemetry.

---

## 1. Executive Decision Summary

JeRyu should not become a GitHub clone by copying GitHub’s information architecture. It should become a **faster, clearer, real-time Git forge** that keeps GitHub/GitLab compatibility where users expect it, but compresses the confusing parts into fewer screens and more useful panels.

The final design uses three rules:

1. **One Rust authority**: Rust owns Git operations, permissions, Markdown sanitization, provider integrations, durable state, action execution, and WebSocket event fanout.
2. **One React command surface**: the browser app gives users a global command palette, live activity rail, repository dashboard, merge room, code browser, settings cockpit, and evidence-aware review flows.
3. **One typed contract**: every screen, command, and websocket frame uses typed DTOs in `src/api/*` and generated/hand-written TypeScript types in `apps/web/src/api/types.ts`.

This keeps the current JeRyu strengths—Git compatibility, CI/CD orchestration, agents, policy gates, TUI proof culture, and Git host adapters—while adding the missing browser product.

---

## 2. Current-State Baseline

### 2.1 Existing strengths to preserve

The repo already has a strong Rust control plane:

- Workspace-managed Rust crate with `jeryu` as the default binary.
- Existing Axum server in `engine.rs` for health, hooks, and cache summary.
- Existing typed API module for TUI/read-model/event/action surfaces.
- Existing `git_host` trait layer with GitHub/GitLab concepts such as repository refs, check runs, MR/PR comments, approval, open PR listing, live PR state, and PR diff retrieval.
- Existing agent, policy, runner, cache, release, secrets, local repo, remote node, MCP, and telemetry modules.
- Existing browser workspace under `apps/web`, but it is only a UX QA proof stub.

### 2.2 Current browser gap

The current `apps/web` package is not a real web application. It has no Vite config, no React entrypoint, no routes, no components, no frontend API client, no websocket client, and no Git host UI. It only runs a Node proof script over `ux-qa.ts` and `ux-qa.md`.

### 2.3 Product gap matrix

| Capability | Current state | Target state |
|---|---:|---|
| All repos dashboard | Partial CLI/domain support | Full searchable, grouped, real-time dashboard |
| Create repository | Not exposed as browser flow | Browser wizard with provider/local targets |
| Repository home | Missing | Overview, README, activity, branches, CI, agent evidence |
| Code browser | Missing | Tree, file viewer, blame, history, symbols, raw/copy/download |
| README rendering | Missing | Server-side Markdown-to-sanitized HTML endpoint + React panel |
| Branches/tags/commits | Partial Git/control plane primitives | Full UI and REST APIs |
| Merge requests / PRs | Host adapter primitives exist | Merge room with file review, approvals, checks, conflicts, policies |
| Issues/projects | Domain exists in pieces | GitHub/GitLab-style issues, boards, labels, milestones |
| Settings | CLI/settings exist | Full settings cockpit with permission-aware forms |
| Real-time activity | Engine loops and event sources exist | WebSocket fanout with replay, topic subscription, and activity rail |
| GitHub/GitLab parity | Control plane oriented | User-facing forge parity plus agent-native extensions |

---

## 3. Product North Star

JeRyu Web should feel like this:

> “I can see every repository, every merge, every check, every agent action, and every setting from one fast browser UI. I can move from a global fleet view to a single changed line, approve a merge with confidence, and see all live activity without refreshing.”

### Better than GitHub/GitLab means

- **Fewer context switches**: repo home, README, files, CI, merge health, recent activity, and agent evidence are visible as dockable panels.
- **Real-time by default**: activity, checks, reviews, runner state, policy gates, and agent actions update over websocket.
- **Action previews**: destructive or high-risk operations show a preview, risk level, required permission, and audit receipt before execution.
- **Command-first navigation**: every screen action is available from `Cmd/Ctrl+K`.
- **Clear merge state**: a single “merge readiness” model replaces scattered checks, approvals, conversations, conflicts, and agent gates.
- **Agent-native evidence**: policy and agent receipts are first-class review objects, not hidden logs.

---

## 4. Target Architecture

```text
┌─────────────────────────────────────────────────────────────────────┐
│                         Browser: apps/web                          │
│ ┌────────────┐ ┌─────────────┐ ┌──────────────┐ ┌───────────────┐ │
│ │ App Shell  │ │ Repo Pages  │ │ Merge Room   │ │ Settings Deck │ │
│ └─────┬──────┘ └──────┬──────┘ └──────┬───────┘ └──────┬────────┘ │
│       │ REST/JSON      │ REST/JSON      │ REST/JSON       │        │
│       └──────────────┬─┴───────────────┴────────────────┘        │
│                      │ WebSocket /api/ws                           │
└──────────────────────┼──────────────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────────────┐
│                        Rust BFF: src/web                            │
│ ┌────────────┐ ┌─────────────┐ ┌──────────────┐ ┌───────────────┐ │
│ │ router     │ │ auth/rbac   │ │ event_hub/ws │ │ static_assets │ │
│ └─────┬──────┘ └──────┬──────┘ └──────┬───────┘ └──────┬────────┘ │
│       │               │               │                │          │
│ ┌─────▼──────┐ ┌──────▼──────┐ ┌──────▼───────┐ ┌──────▼────────┐│
│ │ repo       │ │ merge       │ │ markdown      │ │ settings      ││
│ │ browser    │ │ requests    │ │ renderer      │ │ service       ││
│ └─────┬──────┘ └──────┬──────┘ └──────┬───────┘ └──────┬────────┘│
└───────┼───────────────┼───────────────┼────────────────┼─────────┘
        │               │               │                │
┌───────▼───────────────▼───────────────▼────────────────▼─────────┐
│                         JeRyu domain layer                         │
│ git_host │ gitlab_client │ repo_local │ state │ policy │ runners │
│ agents   │ CI/cache      │ release     │ secrets │ telemetry     │
└────────────────────────────────────────────────────────────────────┘
```

### 4.1 Authority boundaries

| Layer | Owns | Must not own |
|---|---|---|
| React | presentation, local UI state, optimistic previews, keyboard routing | raw Git mutation, token handling, Markdown sanitization |
| Rust web BFF | REST/WS API, session, RBAC, action previews, DTOs, static assets | provider-specific HTTP leakage into UI |
| Git host/provider layer | GitHub/GitLab/local git adapter calls | browser-specific view models |
| State layer | durable rows, audit, notifications, settings, event replay cursor | hand-built SQL from route handlers |
| Engine/event hub | real-time production, replay, fanout | blocking UI requests |

---

## 5. Target Repository Tree Diagram

```text
jeryu/
├── Cargo.toml
├── package.json
├── apps/
│   └── web/
│       ├── AGENTS.md
│       ├── index.html
│       ├── package.json
│       ├── tsconfig.json
│       ├── tsconfig.node.json
│       ├── vite.config.ts
│       ├── ux-qa-check.mjs
│       ├── ux-qa.md
│       ├── ux-qa.ts
│       ├── public/
│       │   └── favicon.svg
│       └── src/
│           ├── main.tsx
│           ├── App.tsx
│           ├── router.tsx
│           ├── styles.css
│           ├── api/
│           │   ├── client.ts
│           │   ├── errors.ts
│           │   ├── queryKeys.ts
│           │   └── types.ts
│           ├── components/
│           │   ├── AppShell.tsx
│           │   ├── CommandPalette.tsx
│           │   ├── MarkdownHtml.tsx
│           │   ├── ReadmePanel.tsx
│           │   ├── RepoCard.tsx
│           │   ├── SettingsForm.tsx
│           │   └── StatusPill.tsx
│           ├── features/
│           │   ├── code/
│           │   ├── dashboard/
│           │   ├── issues/
│           │   ├── merge/
│           │   ├── search/
│           │   └── settings/
│           ├── pages/
│           │   ├── DashboardPage.tsx
│           │   ├── RepoHomePage.tsx
│           │   ├── CodeBrowserPage.tsx
│           │   ├── MergeRequestPage.tsx
│           │   ├── IssuesPage.tsx
│           │   ├── SettingsPage.tsx
│           │   └── ActivityPage.tsx
│           ├── realtime/
│           │   ├── ActivitySocketProvider.tsx
│           │   ├── protocol.ts
│           │   └── useLiveTopic.ts
│           └── test/
│               ├── msw.ts
│               └── setup.ts
├── db/
│   └── migrations/
│       └── 0010_web_forge.sql
├── docs/
│   └── web-forge.md
├── src/
│   ├── api/
│   │   ├── code.rs
│   │   ├── merge_request.rs
│   │   ├── repository.rs
│   │   ├── settings.rs
│   │   └── web.rs
│   ├── cli_defs_web.rs
│   └── web/
│       ├── mod.rs
│       ├── actions.rs
│       ├── audit.rs
│       ├── auth.rs
│       ├── config.rs
│       ├── csrf.rs
│       ├── errors.rs
│       ├── event_hub.rs
│       ├── markdown.rs
│       ├── merge_requests.rs
│       ├── openapi.rs
│       ├── pagination.rs
│       ├── rbac.rs
│       ├── repo_admin.rs
│       ├── repo_browser.rs
│       ├── router.rs
│       ├── search.rs
│       ├── settings.rs
│       ├── state.rs
│       ├── static_assets.rs
│       └── ws.rs
└── tests/
    ├── web_api_contract_tests.rs
    ├── web_markdown_tests.rs
    └── web_ws_tests.rs
```

---

## 6. Backend Engineering Specification

### 6.1 Cargo and dependency changes

Modify `Cargo.toml` to support:

- Axum websocket extractors.
- Static asset serving for production builds.
- Compression and request IDs.
- Markdown parsing and sanitization.
- Syntax highlighting for code blocks.
- Cookie/session signing and CSRF.
- Async broadcast for websocket fanout.
- OpenAPI generation.

Required dependency intent:

```toml
axum = { version = "0.8", features = ["json", "ws", "macros", "multipart"] }
tower = { version = "0.5", features = ["util", "timeout", "limit"] }
tower-http = { version = "0.6", features = ["cors", "trace", "fs", "compression-full", "request-id", "set-header", "limit"] }
tokio-stream = "0.1"
bytes = "1"
mime_guess = "2"
pulldown-cmark = { version = "0.12", default-features = false, features = ["html"] }
ammonia = "4"
syntect = { version = "5", default-features = false, features = ["html", "parsing", "regex-onig"] }
percent-encoding = "2"
serde_urlencoded = "0.7"
utoipa = { version = "5", features = ["axum_extras", "chrono", "uuid"] }
utoipa-swagger-ui = { version = "8", features = ["axum"] }
argon2 = "0.5"
cookie = "0.18"
hmac = "0.12"
subtle = "2"
async-broadcast = "0.7"
```

### 6.2 Rust module responsibilities

| Module | Responsibility |
|---|---|
| `src/web/mod.rs` | Public entrypoint for web server, config, doctor, assets. |
| `src/web/config.rs` | Bind address, public URL, static dir, auth mode, CORS, websocket limits. |
| `src/web/state.rs` | Shared web state: DB, GitLab client, Docker, event hub, config. |
| `src/web/router.rs` | Builds all `/api/*`, `/api/ws`, `/docs/openapi`, and static routes. |
| `src/web/errors.rs` | Typed API errors and response mapping. |
| `src/web/auth.rs` | Session extraction, login/logout, PAT/session/cookie support. |
| `src/web/rbac.rs` | Permission matrix and route/action guards. |
| `src/web/csrf.rs` | CSRF token minting/validation for mutating browser calls. |
| `src/web/event_hub.rs` | Broadcast and replay registry for live events. |
| `src/web/ws.rs` | WebSocket upgrade, subscriptions, heartbeats, replay cursor. |
| `src/web/markdown.rs` | README/Markdown parser, sanitizer, cache metadata. |
| `src/web/repo_browser.rs` | List repos, tree, blob, commit, branch, tag, compare projections. |
| `src/web/repo_admin.rs` | Create/import/archive/delete repository and protected branch settings. |
| `src/web/merge_requests.rs` | MR/PR list, files, comments, review decisions, approvals, merge queue. |
| `src/web/issues.rs` | Issues, labels, milestones, projects, saved filters. |
| `src/web/settings.rs` | Global/org/repo/user settings reads/writes. |
| `src/web/actions.rs` | Preview/execute action framework with audit receipts. |
| `src/web/search.rs` | Repo/code/issue/MR/search index endpoint. |
| `src/web/notifications.rs` | User notification inbox, mark read, subscriptions. |
| `src/web/audit.rs` | Audit timeline query APIs. |
| `src/web/static_assets.rs` | Serve built Vite assets; dev proxy support. |
| `src/web/openapi.rs` | JSON OpenAPI schema and Swagger UI. |

### 6.3 API DTO modules

Add these modules under `src/api`:

| File | Purpose |
|---|---|
| `src/api/web.rs` | Shared envelope, pagination, user/session, permission DTOs. |
| `src/api/repository.rs` | Repository summaries, create request, clone URLs, visibility, settings. |
| `src/api/code.rs` | Tree entries, blob views, README HTML, syntax metadata, compare DTOs. |
| `src/api/merge_request.rs` | MR summaries, review state, checks, threads, file comments, approval DTOs. |
| `src/api/settings.rs` | Typed global/repo settings, patch requests, validation errors. |

Keep API projections independent from provider-specific response shapes. The frontend should never know whether a repo came from local Git, GitLab, GitHub, or another future provider.

### 6.4 Provider model

Introduce a provider abstraction used by web services:

```rust
#[async_trait]
pub trait ForgeProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn list_repositories(&self, query: RepoListQuery) -> Result<Vec<RepositorySummary>, WebError>;
    async fn create_repository(&self, input: CreateRepositoryRequest) -> Result<RepositorySummary, WebError>;
    async fn get_tree(&self, repo: RepoSelector, rev: String, path: String) -> Result<TreeResponse, WebError>;
    async fn get_blob(&self, repo: RepoSelector, rev: String, path: String) -> Result<BlobResponse, WebError>;
    async fn render_readme(&self, repo: RepoSelector, rev: String) -> Result<RenderedReadme, WebError>;
    async fn list_merge_requests(&self, repo: RepoSelector, query: MrQuery) -> Result<Vec<MergeRequestSummary>, WebError>;
    async fn approve_merge_request(&self, repo: RepoSelector, id: String, input: ApproveRequest) -> Result<ActionReceipt, WebError>;
}
```

Provider implementations:

1. `LocalGitProvider`: uses `git2`, local workspaces, and JeRyu repo registry.
2. `GitLabProvider`: wraps current `gitlab_client` and runner/CI surfaces.
3. `GitHubProvider`: wraps current `git_host::GitHubClient` for PR/check/comment surfaces.
4. `CompositeProvider`: aggregates all providers for “All Repos”.

### 6.5 Data model additions

Add migration `db/migrations/0010_web_forge.sql`.

Core tables:

```sql
CREATE TABLE IF NOT EXISTS web_repositories (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    display_name TEXT NOT NULL,
    default_branch TEXT NOT NULL DEFAULT 'main',
    visibility TEXT NOT NULL DEFAULT 'private',
    description TEXT,
    local_path TEXT,
    remote_url TEXT,
    archived INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(provider, owner, name)
);

CREATE TABLE IF NOT EXISTS web_repo_settings (
    repo_id TEXT PRIMARY KEY,
    settings_json TEXT NOT NULL,
    updated_by TEXT,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(repo_id) REFERENCES web_repositories(id)
);

CREATE TABLE IF NOT EXISTS web_merge_requests (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    provider_iid TEXT NOT NULL,
    title TEXT NOT NULL,
    state TEXT NOT NULL,
    source_branch TEXT NOT NULL,
    target_branch TEXT NOT NULL,
    head_sha TEXT NOT NULL,
    base_sha TEXT,
    author TEXT NOT NULL,
    draft INTEGER NOT NULL DEFAULT 0,
    labels_json TEXT NOT NULL DEFAULT '[]',
    readiness_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(repo_id) REFERENCES web_repositories(id),
    UNIQUE(repo_id, provider_iid)
);

CREATE TABLE IF NOT EXISTS web_activity_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    topic TEXT NOT NULL,
    kind TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    actor TEXT,
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_web_activity_topic_sequence
    ON web_activity_events(topic, sequence);

CREATE TABLE IF NOT EXISTS web_markdown_cache (
    repo_id TEXT NOT NULL,
    rev TEXT NOT NULL,
    path TEXT NOT NULL,
    source_sha TEXT NOT NULL,
    html TEXT NOT NULL,
    toc_json TEXT NOT NULL,
    warnings_json TEXT NOT NULL DEFAULT '[]',
    rendered_at TEXT NOT NULL,
    PRIMARY KEY(repo_id, rev, path, source_sha)
);
```

### 6.6 Action preview and execution

Every mutating operation must support preview first:

```text
POST /api/actions/preview
POST /api/actions/execute
```

Action kinds:

- `repo.create`
- `repo.archive`
- `repo.delete`
- `repo.transfer`
- `branch.protect`
- `branch.delete`
- `mr.approve`
- `mr.unapprove`
- `mr.merge`
- `mr.rebase`
- `mr.close`
- `mr.mark-ready`
- `review.submit`
- `pipeline.retry`
- `pipeline.cancel`
- `runner.pause`
- `runner.resume`
- `settings.patch`
- `secret.rotate`
- `agent.grant`
- `agent.revoke`

Each preview returns:

```json
{
  "actionId": "uuid",
  "kind": "mr.merge",
  "risk": "medium",
  "requiresConfirmation": true,
  "requiresMfa": false,
  "requiredPermission": "merge_request.merge",
  "summary": "Merge feature/login into main",
  "effects": [
    { "kind": "git_ref_update", "target": "main", "from": "abc", "to": "def" },
    { "kind": "ci_trigger", "pipeline": "post-merge" }
  ],
  "blockingReasons": [],
  "auditDraft": { "actor": "ben", "entity": "mr:123", "receiptDigest": "sha256:..." }
}
```

---

## 7. REST API Surface

All endpoints return typed JSON envelopes:

```json
{
  "data": {},
  "meta": { "requestId": "...", "freshness": "live", "servedAt": "..." }
}
```

Errors:

```json
{
  "error": {
    "code": "permission_denied",
    "message": "You need repository.settings.write",
    "details": {},
    "requestId": "..."
  }
}
```

### 7.1 Session and bootstrap

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/bootstrap` | App bootstrap: current user, permissions, enabled providers, feature flags. |
| `GET` | `/api/session` | Current session and auth mode. |
| `POST` | `/api/session/login` | Login with local admin token/PAT/OIDC callback mode. |
| `POST` | `/api/session/logout` | Clear browser session. |
| `GET` | `/api/health` | Web API health. |
| `GET` | `/api/openapi.json` | OpenAPI schema. |

### 7.2 Repository APIs

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos` | List all repos across providers with filters, sort, grouping, repo family. |
| `POST` | `/api/repos` | Create new repo. |
| `GET` | `/api/repos/:repoId` | Repo home summary. |
| `PATCH` | `/api/repos/:repoId` | Update description, topics, visibility when allowed. |
| `POST` | `/api/repos/:repoId/archive` | Archive repo after action preview. |
| `POST` | `/api/repos/:repoId/import` | Import existing local/remote repo. |
| `GET` | `/api/repos/:repoId/activity` | Repo-scoped activity timeline. |
| `GET` | `/api/repos/:repoId/contributors` | Contributors and ownership. |

### 7.3 Code browsing

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:repoId/tree?rev=&path=` | Directory entries. |
| `GET` | `/api/repos/:repoId/blob?rev=&path=` | File content, encoding, language, size, binary flag. |
| `GET` | `/api/repos/:repoId/raw?rev=&path=` | Raw file bytes. |
| `GET` | `/api/repos/:repoId/readme?rev=` | Best README rendered HTML + source metadata. |
| `POST` | `/api/markdown/render` | Render arbitrary Markdown safely. |
| `GET` | `/api/repos/:repoId/blame?rev=&path=` | Blame rows. |
| `GET` | `/api/repos/:repoId/history?rev=&path=` | File or directory history. |
| `GET` | `/api/repos/:repoId/symbols?rev=&path=` | Symbol outline for known languages. |

### 7.4 Branches, tags, commits, compare

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:repoId/branches` | List branches and protection. |
| `POST` | `/api/repos/:repoId/branches` | Create branch from rev. |
| `PATCH` | `/api/repos/:repoId/branches/:branch/protection` | Branch protection settings. |
| `DELETE` | `/api/repos/:repoId/branches/:branch` | Delete branch with safety checks. |
| `GET` | `/api/repos/:repoId/tags` | List tags/releases. |
| `POST` | `/api/repos/:repoId/tags` | Create tag. |
| `GET` | `/api/repos/:repoId/commits` | Commit log. |
| `GET` | `/api/repos/:repoId/commits/:sha` | Commit detail. |
| `GET` | `/api/repos/:repoId/compare?base=&head=` | Compare branches/SHAs. |

### 7.5 Merge requests / pull requests

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:repoId/merge-requests` | List MRs/PRs. |
| `POST` | `/api/repos/:repoId/merge-requests` | Open MR/PR. |
| `GET` | `/api/repos/:repoId/merge-requests/:id` | MR overview and readiness. |
| `PATCH` | `/api/repos/:repoId/merge-requests/:id` | Title/body/labels/draft state. |
| `GET` | `/api/repos/:repoId/merge-requests/:id/files` | Changed files and diff hunks. |
| `GET` | `/api/repos/:repoId/merge-requests/:id/checks` | Checks, gates, CI, policies. |
| `GET` | `/api/repos/:repoId/merge-requests/:id/threads` | Review threads. |
| `POST` | `/api/repos/:repoId/merge-requests/:id/comments` | Add comment. |
| `POST` | `/api/repos/:repoId/merge-requests/:id/reviews` | Submit review. |
| `POST` | `/api/repos/:repoId/merge-requests/:id/approve` | Approve with exact SHA binding. |
| `POST` | `/api/repos/:repoId/merge-requests/:id/unapprove` | Remove approval. |
| `POST` | `/api/repos/:repoId/merge-requests/:id/rebase` | Rebase/update branch. |
| `POST` | `/api/repos/:repoId/merge-requests/:id/merge` | Merge/squash/rebase merge. |
| `POST` | `/api/repos/:repoId/merge-requests/:id/close` | Close MR/PR. |

### 7.6 Issues, projects, and planning

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:repoId/issues` | List issues. |
| `POST` | `/api/repos/:repoId/issues` | Create issue. |
| `GET` | `/api/repos/:repoId/issues/:id` | Issue detail. |
| `PATCH` | `/api/repos/:repoId/issues/:id` | Update title/body/status/assignees/labels. |
| `POST` | `/api/repos/:repoId/issues/:id/comments` | Comment. |
| `GET` | `/api/repos/:repoId/labels` | Labels. |
| `POST` | `/api/repos/:repoId/labels` | Create label. |
| `GET` | `/api/repos/:repoId/milestones` | Milestones. |
| `GET` | `/api/projects` | Cross-repo project boards. |

### 7.7 CI/CD, agents, runners, evidence

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/repos/:repoId/pipelines` | Pipeline list. |
| `GET` | `/api/repos/:repoId/pipelines/:id` | Pipeline DAG. |
| `POST` | `/api/repos/:repoId/pipelines/:id/retry` | Retry pipeline. |
| `POST` | `/api/repos/:repoId/pipelines/:id/cancel` | Cancel pipeline. |
| `GET` | `/api/jobs/:id/log` | Job log stream cursor. |
| `GET` | `/api/runners` | Runner pool health. |
| `PATCH` | `/api/runners/:id` | Pause/resume/drain/scale. |
| `GET` | `/api/agents/activity` | Agent activity feed. |
| `GET` | `/api/agents/evidence/:id` | Evidence receipt detail. |
| `POST` | `/api/agents/grants` | Grant scoped permission. |
| `DELETE` | `/api/agents/grants/:id` | Revoke grant. |

### 7.8 Settings APIs

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/settings/global` | Global settings. |
| `PATCH` | `/api/settings/global` | Patch global settings. |
| `GET` | `/api/repos/:repoId/settings` | Repo settings. |
| `PATCH` | `/api/repos/:repoId/settings` | Patch repo settings. |
| `GET` | `/api/users/me/settings` | User preferences. |
| `PATCH` | `/api/users/me/settings` | Patch user preferences. |

---

## 8. WebSocket Contract

Endpoint:

```text
GET /api/ws?topics=global,repos,repo:{repoId},mr:{mrId},ci,agents&cursor={sequence}
```

### 8.1 Client messages

```ts
type ClientFrame =
  | { type: 'hello'; clientId: string; lastSeenSequence?: number }
  | { type: 'subscribe'; topics: string[]; replayFrom?: number }
  | { type: 'unsubscribe'; topics: string[] }
  | { type: 'ping'; nonce: string }
  | { type: 'ack'; sequence: number };
```

### 8.2 Server messages

```ts
type ServerFrame =
  | { type: 'hello'; serverId: string; heartbeatMs: number; maxReplay: number }
  | { type: 'event'; sequence: number; topic: string; event: WebEvent }
  | { type: 'replay-complete'; topic: string; through: number }
  | { type: 'pong'; nonce: string; serverTime: string }
  | { type: 'error'; code: string; message: string; retryable: boolean };
```

### 8.3 Event kinds

```ts
type WebEvent =
  | { kind: 'repo.created'; repo: RepositorySummary }
  | { kind: 'repo.updated'; repoId: string; patch: Partial<RepositorySummary> }
  | { kind: 'branch.updated'; repoId: string; branch: string; oldSha?: string; newSha: string }
  | { kind: 'commit.pushed'; repoId: string; branch: string; commits: CommitSummary[] }
  | { kind: 'mr.opened'; repoId: string; mr: MergeRequestSummary }
  | { kind: 'mr.updated'; repoId: string; mrId: string; patch: Partial<MergeRequestSummary> }
  | { kind: 'mr.checks.updated'; repoId: string; mrId: string; checks: CheckSummary[] }
  | { kind: 'mr.review.submitted'; repoId: string; mrId: string; review: ReviewSummary }
  | { kind: 'pipeline.updated'; repoId: string; pipeline: PipelineSummary }
  | { kind: 'job.log.appended'; jobId: string; offset: number; text: string }
  | { kind: 'runner.updated'; runner: RunnerSummary }
  | { kind: 'agent.action'; action: AgentActionSummary }
  | { kind: 'settings.updated'; scope: string; key: string; actor: string }
  | { kind: 'notification.created'; notification: NotificationSummary };
```

### 8.4 Reliability requirements

- Every emitted event gets a monotonically increasing `sequence`.
- Store at least the last 10,000 events or 24 hours, whichever is larger.
- Client can reconnect with `cursor` to replay missed events.
- Server sends heartbeat every 20 seconds.
- Backpressure: if a client falls behind, send `error: slow_consumer` and close with reconnect advice.
- No mutating actions over websocket in v1; use REST action preview/execute to keep audit semantics clear.

---

## 9. Frontend Engineering Specification

### 9.1 Replace current web stub with real Vite app

The package becomes `@jeryu/web` and keeps the existing UX QA evidence scripts as a proof lane.

Core stack:

- React 19 or current stable React.
- Vite.
- TypeScript strict mode.
- TanStack Query for server state.
- TanStack Router or React Router for route tree.
- TanStack Virtual for huge files, repo lists, logs, and diffs.
- Zustand for app shell state: palette, panels, selection, view preferences.
- Zod for API response validation at boundaries.
- Monaco editor for code view and comment positioning where useful.
- MSW for mocks and Storybook state coverage.
- Playwright for screenshot and end-to-end proofs.
- Axe/pa11y/storybook addon for accessibility proofs.

### 9.2 Route tree

```text
/
├── /dashboard
├── /activity
├── /search
├── /new
├── /settings
│   ├── /profile
│   ├── /appearance
│   ├── /providers
│   ├── /security
│   └── /admin
└── /:owner/:repo
    ├── /overview
    ├── /tree/:rev/*path
    ├── /blob/:rev/*path
    ├── /commits/:rev
    ├── /branches
    ├── /tags
    ├── /compare/:base...:head
    ├── /merge-requests
    ├── /merge-requests/:id
    ├── /issues
    ├── /issues/:id
    ├── /pipelines
    ├── /releases
    ├── /packages
    ├── /security
    ├── /agents
    ├── /activity
    └── /settings
        ├── /general
        ├── /access
        ├── /branches
        ├── /merge-rules
        ├── /ci-cd
        ├── /webhooks
        ├── /agents
        ├── /secrets
        └── /danger-zone
```

### 9.3 App shell

Persistent layout:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ Top bar: logo | global search | repo switcher | create | activity | user   │
├──────────────┬──────────────────────────────────────────────┬───────────────┤
│ Sidebar      │ Main content                                 │ Activity rail │
│ - Dashboard  │ Route-specific page                           │ live events    │
│ - Repos      │ Context tabs                                  │ notifications  │
│ - MRs        │ Keyboard hints                                │ agent actions  │
│ - Issues     │ Action preview drawers                         │ CI updates     │
│ - CI         │                                              │               │
│ - Agents     │                                              │               │
│ - Settings   │                                              │               │
└──────────────┴──────────────────────────────────────────────┴───────────────┘
```

### 9.4 Dashboard page

Must support:

- All repositories across providers.
- Repo family grouping, e.g. prefix or configured family name.
- Filters: owner, provider, language, topic, visibility, archived, status, stale, active agents, failing CI.
- Sorting: recently active, name, open MRs, failing pipelines, risk, stars/favorites, size.
- View modes: compact list, cards, table, family board.
- Bulk controls: pin, archive, refresh, sync provider, export, bulk settings where permitted.
- Live badges: new push, MR update, failing check, runner pressure, agent action.

### 9.5 Repository overview page

Panels:

- README rendered HTML.
- File tree teaser.
- Recent commits.
- Branch health.
- Open MRs/PRs.
- Open issues.
- CI status.
- Agent evidence and pending approvals.
- Quick settings warnings.
- Clone URLs and copy controls.

Quick actions:

- New branch.
- New file.
- Upload file.
- New MR/PR.
- New issue.
- Run pipeline.
- Open command palette scoped to repo.
- Copy clone URL.
- Star/pin/watch.
- Open local path.

### 9.6 Code browser

Controls:

- Branch/tag/SHA switcher.
- Breadcrumb path.
- Fuzzy file finder.
- Tree expand/collapse.
- Copy path.
- Copy raw URL.
- Download file.
- Open raw.
- Blame.
- History.
- Permalink to line/range.
- Open in editor command.
- Create branch from here.
- Edit file where permitted.
- View rendered Markdown vs source.
- Toggle whitespace, wrap, minimap, comments.
- Symbol outline.

Performance:

- Virtualize file rows and long code files.
- Defer syntax highlighting for large files.
- Binary files show preview/download metadata only.
- Files over configured size require explicit load.

### 9.7 Merge room

The merge room replaces the scattered GitHub/GitLab PR page with an explicit readiness model.

Layout:

```text
┌────────────────────────────────────────────────────────────────────┐
│ MR title, state, source→target, author, labels, live readiness     │
├─────────────┬──────────────────────────────────────┬───────────────┤
│ Left rail   │ Diff / conversation / commits / CI   │ Merge panel   │
│ files       │                                      │ checks        │
│ reviewers   │ inline comments                      │ approvals     │
│ threads     │                                      │ conflicts     │
│ evidence    │                                      │ policy gates  │
└─────────────┴──────────────────────────────────────┴───────────────┘
```

Readiness dimensions:

- Target branch freshness.
- Exact head SHA approval binding.
- Required checks.
- Conversation resolution.
- Review approvals.
- Conflicts.
- Policy gates.
- Agent evidence receipts.
- Security/secrets checks.
- Branch protection.
- Merge strategy availability.

Merge controls:

- Approve.
- Request changes.
- Comment.
- Resolve/unresolve thread.
- Assign/reassign reviewers.
- Mark ready/draft.
- Rebase/update branch.
- Squash merge.
- Merge commit.
- Rebase merge.
- Auto-merge when ready.
- Cancel auto-merge.
- Cherry-pick after merge.
- Revert after merge.

### 9.8 Settings cockpit

Settings are organized as cards with inline validation, dirty-state tracking, and permission labels. Every destructive setting routes through action preview.

Settings pages:

- General.
- Access and teams.
- Branch protection.
- Merge rules.
- CI/CD.
- Runners.
- Webhooks.
- Deploy keys.
- Secrets.
- Agent automation.
- Notifications.
- Audit and retention.
- Advanced/danger zone.

---

## 10. Complete User Controls Inventory

### 10.1 Global controls

- Global search.
- Command palette.
- Repo switcher.
- New repository.
- Import repository.
- New issue.
- New MR/PR.
- Start pipeline.
- Toggle theme.
- Toggle density.
- Toggle activity rail.
- Toggle keyboard shortcut overlay.
- Open notifications.
- Mark all notifications read.
- Open personal settings.
- Open provider connection settings.
- Copy diagnostic bundle.

### 10.2 Repository list controls

- Filter by provider.
- Filter by owner/group.
- Filter by repo family.
- Filter by language/topic.
- Filter by archived/private/public/internal.
- Filter by stale/failing/active.
- Sort by activity/name/open MRs/failing CI/risk.
- Pin/unpin.
- Watch/unwatch.
- Bulk refresh.
- Bulk archive where permitted.
- Export visible list.
- Save filter.
- Share filter URL.

### 10.3 Repository home controls

- Copy SSH clone URL.
- Copy HTTPS clone URL.
- Copy local path.
- Open in terminal/editor.
- Switch branch/tag.
- New branch.
- New file.
- Upload file.
- Open README source/rendered.
- Run pipeline.
- Open latest pipeline.
- Create MR from branch.
- Create issue.
- Edit repo description/topics.
- Star/pin/watch.
- Archive/delete via action preview.

### 10.4 Code browser controls

- Branch/tag/SHA picker.
- Breadcrumb navigation.
- Fuzzy file finder.
- Tree expand/collapse.
- View source/rendered for Markdown.
- Copy line link.
- Copy range link.
- Copy file contents.
- Download file.
- View raw.
- Open blame.
- Open history.
- Toggle line numbers.
- Toggle whitespace.
- Toggle wrap.
- Toggle syntax theme.
- Open symbol outline.
- Jump to definition where indexed.
- Start review comment on line when in MR context.

### 10.5 README and Markdown controls

- Rendered/source/split view.
- Heading outline.
- Copy heading link.
- Collapse long sections.
- Show sanitized-content warnings.
- Open relative links through repo router.
- Open images with safe proxy.
- Copy code block.
- Download code block.
- View Mermaid/diagram fallback when enabled.

### 10.6 Merge request controls

- Open/close/reopen.
- Mark draft/ready.
- Edit title/body.
- Assign reviewers.
- Assign owners.
- Add/remove labels.
- Add/remove milestone.
- Subscribe/unsubscribe.
- Approve/unapprove.
- Request changes.
- Submit review.
- Add inline comment.
- Resolve thread.
- Rebase/update branch.
- Retry failed checks.
- Cancel pipeline.
- Merge with selected strategy.
- Enable/disable auto-merge.
- Copy MR URL.
- Copy branch checkout command.
- Download patch/diff.
- Show/hide whitespace.
- Viewed file tracking.
- File review filters: changed/unviewed/commented/owned/failing tests.

### 10.7 CI/pipeline controls

- Run pipeline.
- Retry pipeline.
- Cancel pipeline.
- Retry job.
- Cancel job.
- Pin job log.
- Download artifacts.
- Open trace.
- Follow live logs.
- Search logs.
- Filter DAG by failed/running/manual/skipped.
- Approve manual job where permitted.
- View VTI/test selection proof.
- Compare current pipeline to baseline.

### 10.8 Agent controls

- View agent activity.
- View evidence receipt.
- Grant scoped permission.
- Revoke permission.
- Require human approval.
- Replay agent action proof.
- Open agent-generated diff.
- Link agent action to MR thread.
- Quarantine suspicious action.
- Export evidence bundle.

### 10.9 Settings controls

- Update repo name/description/topics.
- Change visibility.
- Set default branch.
- Configure branch protection.
- Configure CODEOWNERS enforcement.
- Configure required checks.
- Configure approval count.
- Configure exact-SHA approval binding.
- Configure stale approval dismissal.
- Configure squash/merge/rebase strategies.
- Configure auto-merge.
- Configure protected tags.
- Configure webhooks.
- Configure deploy keys.
- Configure secrets.
- Configure runners.
- Configure retention.
- Configure notifications.
- Configure agent automation.
- Archive/delete/transfer repository via danger zone.

---

## 11. Settings Inventory

### 11.1 Global settings

| Category | Settings |
|---|---|
| Server | bind address, public URL, static dir, CORS origins, request body limit, websocket replay limit |
| Auth | local admin mode, OIDC mode, PAT mode, session TTL, cookie secure/same-site, MFA requirement |
| Providers | GitLab URL/token, GitHub token/app, local repo roots, provider sync interval |
| UI | default theme, density, activity rail default, command palette hints |
| Search | index roots, ignored globs, max file size, symbol index toggle |
| Markdown | allowed HTML policy, syntax highlighting, Mermaid/diagram toggle, external image policy |
| Activity | event retention, audit retention, notification retention |
| Agents | default grant TTL, require human approval for risk levels, evidence retention |
| CI/CD | default runner pool, artifact retention, log retention, VTI defaults |
| Security | secret redaction, gitleaks enforcement, allowed webhook targets, audit export |

### 11.2 Repository settings

| Category | Settings |
|---|---|
| General | name, description, topics, visibility, default branch, archive state |
| Access | users, teams, roles, deploy keys, service accounts |
| Branch protection | protected branches, push rules, force-push, deletion, signed commits |
| Merge rules | required approvals, CODEOWNERS, stale approval dismissal, required checks, exact SHA binding |
| Merge strategy | merge commit, squash, rebase, auto-merge, delete source branch |
| CI/CD | pipeline config path, runner tags, variables, cache, artifacts, manual gates |
| Issues | enable/disable, templates, labels, milestones, default assignee |
| Webhooks | endpoints, events, secrets, retry policy |
| Agents | allowed agents, allowed paths, risk policy, review routing |
| Notifications | default watch, mentions, CI failures, security events |
| Audit | retention, export, sensitive action recording |
| Danger zone | archive, transfer, delete, rename, change visibility |

### 11.3 User settings

| Category | Settings |
|---|---|
| Profile | display name, email, avatar |
| Preferences | theme, density, keyboard mode, default repo view, default MR diff mode |
| Notifications | email/browser/in-app preferences, quiet hours |
| Access tokens | create/revoke tokens, scopes, expiration |
| SSH/GPG | keys and signing configuration |
| Saved filters | dashboard, issues, MRs, CI |

---

## 12. Markdown and README Rendering Specification

### 12.1 Server-side rendering pipeline

1. Resolve README path in priority order:
   - `README.md`
   - `README.mdx` rendered as Markdown subset only unless MDX is explicitly enabled.
   - `README.markdown`
   - `README.txt` as escaped preformatted text.
2. Fetch blob by repo/rev/path.
3. Enforce size limit. Default: 2 MiB for rendered Markdown.
4. Parse Markdown with GFM-style extensions.
5. Rewrite relative links:
   - Relative file links route to `/owner/repo/blob/rev/path`.
   - Relative directory links route to `/owner/repo/tree/rev/path`.
   - Heading links route to sanitized heading IDs.
6. Rewrite relative image URLs through safe raw endpoint.
7. Highlight fenced code blocks.
8. Sanitize HTML with strict allowlist.
9. Extract table of contents.
10. Cache by repo, rev, path, and source SHA.

### 12.2 Security rules

- Do not trust frontend Markdown rendering for repository content.
- Do not allow `<script>`, inline event handlers, unsafe URLs, or arbitrary iframes.
- Do not load remote images unless policy allows them.
- Sanitize SVG carefully or serve as download.
- Add `rel="nofollow noopener noreferrer"` to external links.
- Add CSP headers for app shell and rendered Markdown container.
- Return warnings when content is stripped.

### 12.3 React rendering contract

The frontend receives pre-sanitized HTML:

```ts
export interface RenderedReadme {
  path: string;
  rev: string;
  sourceSha: string;
  html: string;
  toc: Array<{ id: string; depth: number; text: string }>;
  warnings: string[];
  renderedAt: string;
}
```

The React component must:

- Use a dedicated `.markdown-body` container.
- Not run client-side Markdown plugins on trusted repo content.
- Show stripped-content warnings.
- Support source/rendered/split view.
- Use event delegation for internal repo links.

---

## 13. Frontend State Boundaries

| State | Owner |
|---|---|
| API data | TanStack Query |
| WebSocket connection | `ActivitySocketProvider` |
| App shell preferences | Zustand + persisted local storage |
| Forms | React local state + Zod validation |
| URL filters | Router search params |
| Optimistic action preview | Action drawer local state |
| Notifications | Query cache hydrated by websocket events |
| Long logs/diffs | virtualized query windows |

WebSocket events should update query caches through narrow invalidation or patching. Do not refetch the entire dashboard for every event.

---

## 14. Security and Permission Model

### 14.1 Role model

| Role | Capabilities |
|---|---|
| Viewer | read repos, files, issues, MRs, CI summaries |
| Reporter | create issues/comments, download artifacts where allowed |
| Developer | push branches, create MRs, run pipelines |
| Maintainer | merge, approve, manage branch rules, runner controls |
| Owner/Admin | access, secrets, providers, danger zone |
| Agent | scoped, time-bound, path-bound action grants |

### 14.2 Permission strings

Examples:

```text
repository.read
repository.create
repository.write
repository.delete
repository.settings.read
repository.settings.write
code.read
code.write
branch.create
branch.delete
branch.protect
merge_request.read
merge_request.write
merge_request.approve
merge_request.merge
issue.read
issue.write
pipeline.read
pipeline.run
pipeline.cancel
runner.read
runner.write
secret.read_metadata
secret.write
agent.grant
agent.revoke
audit.read
```

### 14.3 Mutating request requirements

Every mutating request must require:

- Authenticated session.
- CSRF token for browser cookie sessions.
- Permission check.
- Action preview for high-risk actions.
- Audit event.
- WebSocket event emission after commit.
- Idempotency key for risky actions.

---

## 15. Performance Requirements

- Initial dashboard shell interactive in under 1 second on local network.
- Repository list supports 10,000 repos with virtualized rendering.
- Code tree supports huge directories using pagination and search.
- Blob endpoint streams or truncates large files with explicit “load full file”.
- Diff view virtualizes files and hunks.
- Job logs use cursor-based streaming.
- WebSocket event patching avoids broad invalidation.
- Markdown rendering cache avoids repeated parse/sanitize work.
- Static assets are compressed and cache-versioned by Vite hash.

---

## 16. Implementation Plan

### PR 1 — Web app foundation

- Replace `apps/web/package.json` with Vite/React package.
- Add Vite config, TS config, app shell, routes, styles.
- Preserve `ux-qa-check.mjs`, `ux-qa.ts`, and `ux-qa.md` proof lane.
- Add root npm scripts.
- Add Storybook/MSW/Playwright scaffolding.

Acceptance:

```bash
npm install
npm run web:build
npm run web:test
npm run ux-qa
```

### PR 2 — Rust web module and static serving

- Add `src/web/*` module skeleton.
- Add `jeryu web serve`, `jeryu web doctor`, `jeryu web assets`.
- Add static asset serving and `/api/bootstrap`.
- Mount web router into engine or serve standalone.

Acceptance:

```bash
cargo check -p jeryu --message-format=json
cargo nextest run -p jeryu --lib
cargo run -p jeryu -- web doctor
cargo run -p jeryu -- web serve --no-browser
```

### PR 3 — Repository APIs and dashboard

- Add repository DTOs.
- Add composite provider list.
- Add repo create/import APIs.
- Build dashboard page with filters, groups, pinned repos, live badges.

Acceptance:

```bash
cargo nextest run -p jeryu --test web_api_contract_tests
npm run web:e2e -- --grep "dashboard"
```

### PR 4 — Code browser and README rendering

- Add tree/blob/raw/readme endpoints.
- Add Markdown renderer, sanitizer, cache table.
- Add code browser and README panel.

Acceptance:

```bash
cargo nextest run -p jeryu --test web_markdown_tests
npm run web:e2e -- --grep "readme"
```

### PR 5 — WebSocket activity rail

- Add event hub, replay table writes, `/api/ws`.
- Add React `ActivitySocketProvider` and activity rail.
- Patch query cache from events.

Acceptance:

```bash
cargo nextest run -p jeryu --test web_ws_tests
npm run web:e2e -- --grep "activity"
```

### PR 6 — Merge room

- Add MR/PR APIs.
- Add diff viewer, review threads, approvals, merge readiness panel.
- Hook exact-SHA approval and check state from `git_host`.

Acceptance:

```bash
cargo nextest run -p jeryu -- git_host::
npm run web:e2e -- --grep "merge room"
```

### PR 7 — Settings cockpit

- Add settings DTOs and patch endpoints.
- Add permission-aware forms.
- Add branch protection, merge rules, CI/CD, webhooks, agents, danger zone.

Acceptance:

```bash
cargo nextest run -p jeryu --test web_api_contract_tests
npm run web:e2e -- --grep "settings"
```

### PR 8 — Issues/projects, search, polish

- Add issues/projects APIs and pages.
- Add search endpoints and UI.
- Add a11y, screenshot, visual proof coverage.
- Add docs and run full validation.

---

## 17. Validation Matrix

| Area | Validation |
|---|---|
| Rust compile | `cargo check -p jeryu --message-format=json` |
| Rust tests | `cargo nextest run -p jeryu --lib` |
| API contracts | `cargo nextest run -p jeryu --test web_api_contract_tests` |
| Markdown safety | `cargo nextest run -p jeryu --test web_markdown_tests` |
| WebSocket replay | `cargo nextest run -p jeryu --test web_ws_tests` |
| Frontend build | `npm run web:build` |
| Frontend unit | `npm run web:test` |
| Browser E2E | `npm run web:e2e` |
| Accessibility | Storybook a11y + axe/pa11y lane |
| Visual proof | Playwright screenshots + UX QA artifacts |
| Existing proof lane | `npm run ux-qa` |

---

## 18. Non-Negotiable Invariants

1. Rust owns Markdown sanitization.
2. Rust owns permission checks.
3. Rust owns mutating actions and audit receipts.
4. The frontend never receives provider tokens.
5. Every destructive action has a preview and audit trail.
6. Every websocket event has a replayable sequence.
7. Every MR approval is bound to the exact head SHA.
8. Every settings form shows permission and validation state.
9. Every route has loading, empty, error, success, and permission-denied states.
10. Existing UX QA evidence files remain part of the proof lane.

---

## 19. Final Product Definition

The completed JeRyu Web Forge delivers:

- Full all-repository dashboard.
- Repository creation/import.
- Repository home pages.
- Code browsing, history, blame, raw/download, permalinks.
- Safe README/Markdown HTML rendering.
- Branch, tag, commit, and compare views.
- Merge request / pull request creation, review, approval, and merge workflows.
- Issue, label, milestone, and project workflows.
- CI/CD, runner, job log, artifact, and VTI proof views.
- Agent activity and evidence receipts.
- WebSocket-powered live activity rail.
- Full global/user/repo settings cockpit.
- Security, audit, action preview, and permission-aware controls.
- A modern UI that is faster and less confusing than GitHub/GitLab because the important context is visible together instead of scattered across pages.
