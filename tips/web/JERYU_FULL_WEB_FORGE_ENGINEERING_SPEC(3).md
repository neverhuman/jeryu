# JeRyu Full Web Forge — Final Engineering Specification

**Date:** 2026-05-26  
**Repository studied:** `neverhuman/jeryu` / current accessible mirror `jeppsontaylor/JeRyu`  
**Requested outcome:** a Rust + Vite + TypeScript + React web experience that covers the full GitHub/GitLab workflow, improves the UX, and makes JeRyu activity real-time through WebSockets.

---

## 1. Executive Summary

JeRyu is already much more than a thin Git wrapper. The Rust side is a single-binary control plane with GitLab orchestration, runner pools, Docker/K8s/remote runner support, cache intelligence, release/canary flows, secrets, policy, admission, evidence gates, TUI, and an API/read-model boundary. What it does **not** yet have is a full browser product. The existing `apps/web` workspace is a UX QA stub, not a Vite/React app. The correct path is not to fork the product into a separate web service; it is to add a first-class `src/web` Axum BFF module to the existing binary, expose typed DTOs from `src/api`, reuse the existing `state::Db`, `git_host`, `repo`, `repo_fleet`, `approval`, `decision`, `release`, `settings`, and TUI read-model work, then replace the current web stub with a real Vite/React/TypeScript application.

The final architecture is:

```text
jeryu binary
├─ CLI: jeryu web serve / jeryu serve --web-compatible behavior
├─ Axum BFF: /api/v1/*, /ws, /assets/*, SPA fallback
├─ typed API contracts: src/api/forge.rs + src/api/web.rs
├─ web domain: repo browse, markdown render, MRs/reviews, settings, realtime events
├─ existing control plane: state, git_host, gitlab_client, repo_fleet, runner pools, decision gates
└─ Vite app: apps/web/dist embedded or served from disk
```

The browser product should be branded as **JeRyu Forge**: a full Git forge and CI control cockpit. It must support all repos, repo creation/import, code browsing, README rendering, commits, branches/tags, merge requests/pull requests, file review, approvals, branch protection, settings, issues, releases, activity, CI status, runner capacity, and an audit trail. It should feel faster than GitHub/GitLab by using one app shell, command palette navigation, predictive prefetching, virtualized lists, a live activity rail, and WebSocket fanout for repo/MR/pipeline/runner changes.

---

## 2. Current Repository Findings

### 2.1 Rust core is the real product today

Observed current structure shows the Rust workspace is the authoritative control plane. `Cargo.toml` has the root package `jeryu`, uses Rust 2024, and includes crates for witness tooling, domain, cache-brain adapters, TUI capture, and GCD support. The main package already depends on `tokio`, `clap`, `axum`, `tower-http`, `reqwest`, `bollard`, `sqlx`, `git2`, `ratatui`, `notify`, and the internal witness/cache adapters.

### 2.2 The browser app is currently a placeholder

The root `package.json` declares one workspace: `apps/web`. Its scripts only run UX QA checks. `apps/web/package.json` is named `@jankurai/ux-qa` and only invokes `ux-qa-check.mjs`. This must be replaced by a real Vite app while preserving the UX QA standards as scripts and test gates.

### 2.3 The API boundary already exists and should be extended, not bypassed

`src/api/mod.rs` is documented as the TUI control-plane API and the single source of truth for typed projections, event contracts, and action dispatch. That is exactly the right layer to reuse for the web BFF. Add web/forge DTO modules here, then have `src/web` call existing domain services through typed read/write boundaries.

### 2.4 The current engine is an Axum service but only exposes control-plane endpoints

`src/engine.rs` builds an Axum router with `/health`, `/hooks`, and `/cache/summary`, then starts reconciliation, Docker events, health loops, and message-log consumers. A browser product needs a separate web router or nested router under the same runtime, with explicit separation between webhook ingestion and authenticated browser API.

### 2.5 Settings are ready for web extension

`src/settings.rs` loads `~/.jeryu/settings.json`, creates defaults on first run, and exposes a process singleton. `Settings` already includes `gitlab`, `vault`, `git`, `mirror`, `webhook`, `mcp`, `pool`, `cache`, `sccache`, `release`, `sandbox`, and `tui`. Add `web`, `auth`, `realtime`, `markdown`, and repo-default settings there instead of scattering environment variables.

### 2.6 Git host abstractions already cover some MR behavior but need broad forge coverage

`src/git_host/mod.rs` already has a trait-based GitHost adapter surface, GitHub/GitLab modules, exact-SHA approval invariants, PR/MR summary/live-state types, diff fetching, check-runs, comments, approval, and target policy SHA hooks. This is the correct base, but the browser product needs to expand it from “evidence gate adapter” into “forge adapter”: repo CRUD, branch/tag CRUD, file browse, raw/blob access, branch protection, reviews, discussions, settings, issues, releases, webhooks, deploy keys, and permissions.

---

## 3. Product North Star

**JeRyu Forge is the one screen where a developer can understand, change, review, approve, and ship every repository.**

It must feel like GitHub/GitLab became faster, calmer, and more operationally aware:

1. **No confusing tab sprawl.** The global app shell keeps repos, activity, command palette, notifications, and quick actions always available.
2. **Everything streams.** CI, MR reviews, branch protection, runner health, file review state, comments, approvals, deployment gates, and audit events update without full refresh.
3. **Every important action explains itself.** Merge buttons show blockers inline; settings forms explain consequences; approval controls show exact SHA, policy, required checks, and human/agent approvals.
4. **Markdown is first-class.** README.md and docs render to safe HTML with GitHub-like tables, task lists, anchors, syntax highlight, Mermaid opt-in, relative link rewriting, image proxying, and XSS protections.
5. **Agents are native users, not hidden bots.** Agent suggestions, evidence packs, policy decisions, and auto-review comments should be visible, filterable, and reversible.
6. **The UI is fast even for huge repo fleets.** Use keyset pagination, virtual lists, search indexes, lazy tree loading, and blob/markdown caches keyed by repo/ref/path/blob SHA.

---

## 4. Scope: GitHub/GitLab Experience Parity Plus JeRyu Improvements

### 4.1 Required GitHub/GitLab parity

| Area | Required capability |
|---|---|
| Global | view all repos, filter by owner/provider/health/language/activity, create/import repo, global search, command palette |
| Repository | overview, README, clone URLs, star/watch/pin/favorite, activity, contributors, languages, latest release, CI health |
| Code | file tree, branch/tag selector, path search, file preview, raw/download, edit/create/delete file, blame, history, symbols, large-file fallback |
| Markdown | README/docs rendering, tables, task lists, anchors, code blocks, safe HTML policy, relative links/images, diagram opt-ins |
| Commits | commit list, commit details, compare refs, changed files, signatures/checks, copy SHA |
| Branches/tags | list, create, delete, protect, compare, default branch setting, stale branch detection |
| Merge requests / PRs | list, create, diff review, line comments, threads, approvals, request changes, merge/squash/rebase/close/reopen, conflict state, draft/ready |
| Reviews | side-by-side/unified diff, file tree, viewed files, comment drafts, suggestions, batch review submission, CODEOWNERS visibility |
| Issues | list, create/edit/close/reopen, labels, milestones, assignees, linking to MRs/commits |
| CI/CD | pipelines, jobs, logs, artifacts, retry/cancel, runner pool visibility, environment/deployment gates |
| Releases | tags, releases, assets, changelog, canary/rollback status from existing release modules |
| Settings | general, visibility, permissions, collaborators, branch protection, merge rules, webhooks, deploy keys, tokens, secrets, CI, runners, integrations, danger zone |
| Admin/audit | audit log, user/session management, role mapping, policy registry, server status |

### 4.2 Better-than-GitHub/GitLab differentiators

| JeRyu improvement | UX behavior |
|---|---|
| Live activity rail | Every repo/MR/pipeline/runner event appears immediately, deduplicated and scoped to the current view. |
| Merge Room | A dedicated MR cockpit with blockers, evidence, file review, approvals, policy, CI, agents, and action controls in one screen. |
| Explain blockers | Merge button expands into exact blocker list with “who/what fixes this” next actions. |
| Fleet intelligence | Repos can be grouped into families; dashboards show repo family health, shared bottlenecks, runner saturation, and cache pressure. |
| Agent-native review | Agent reviews appear as structured evidence cards with confidence, exact file/hunk links, replay/rejudge controls, and audit receipts. |
| Command palette everywhere | `⌘K` supports repo jump, branch switch, create MR, approve, merge, search file, open settings, rerun CI. |
| Reduced setting anxiety | Settings use progressive disclosure, summaries, diffs, dry-run validation, and “what changes if I save?” previews. |
| Review velocity | Keyboard-first file review: `j/k`, `n/p`, `v` viewed, `a` approve, `r` request changes, `m` merge when clean. |

---

## 5. Target Repository Tree Diagram

```text
jeryu/
├─ Cargo.toml                         # add web/markdown/ws/static deps
├─ package.json                       # real web scripts + preserved UX QA gates
├─ apps/
│  └─ web/
│     ├─ package.json                 # Vite + React + TS app, no longer only UX QA
│     ├─ index.html
│     ├─ vite.config.ts
│     ├─ tsconfig.json
│     ├─ playwright.config.ts
│     ├─ src/
│     │  ├─ main.tsx
│     │  ├─ App.tsx
│     │  ├─ routes.tsx
│     │  ├─ styles/
│     │  │  ├─ tokens.css
│     │  │  └─ app.css
│     │  ├─ api/
│     │  │  ├─ client.ts
│     │  │  ├─ dto.ts              # generated or hand-mirrored from Rust DTOs
│     │  │  └─ queryKeys.ts
│     │  ├─ realtime/
│     │  │  ├─ socket.ts
│     │  │  ├─ eventStore.ts
│     │  │  └─ subscriptions.ts
│     │  ├─ shell/
│     │  │  ├─ AppShell.tsx
│     │  │  ├─ CommandPalette.tsx
│     │  │  ├─ ActivityRail.tsx
│     │  │  ├─ TopNav.tsx
│     │  │  └─ shortcuts.ts
│     │  ├─ features/
│     │  │  ├─ dashboard/
│     │  │  ├─ repos/
│     │  │  ├─ code/
│     │  │  ├─ markdown/
│     │  │  ├─ mergeRequests/
│     │  │  ├─ reviews/
│     │  │  ├─ issues/
│     │  │  ├─ ci/
│     │  │  ├─ releases/
│     │  │  ├─ settings/
│     │  │  ├─ audit/
│     │  │  └─ admin/
│     │  └─ test/
│     │     ├─ fixtures.ts
│     │     └─ msw.ts
│     ├─ ux-qa.md                    # keep and expand
│     └─ ux-qa-check.mjs             # keep as validation gate
├─ db/
│  ├─ state.rs                       # add web forge accessors only through Db
│  └─ migrations/
│     └─ 20260526_web_forge.sql
├─ src/
│  ├─ lib.rs                         # add pub mod web
│  ├─ cli_defs.rs                    # add Web subcommand module
│  ├─ cli_defs_web.rs                # new typed clap definitions
│  ├─ dispatch.rs                    # route Commands::Web to commands::web
│  ├─ commands/
│  │  ├─ mod.rs                      # pub mod web
│  │  └─ web.rs                      # CLI entry to web server/schema/dev proxy
│  ├─ api/
│  │  ├─ mod.rs                      # add forge/web modules
│  │  ├─ forge.rs                    # DTOs for repos/code/MRs/issues/settings
│  │  └─ web.rs                      # app/session/event DTOs
│  ├─ git_host/
│  │  ├─ mod.rs                      # extend trait or add ForgeHost companion trait
│  │  ├─ github.rs                   # implement/bridge forge operations
│  │  └─ gitlab.rs                   # implement/bridge forge operations
│  └─ web/
│     ├─ mod.rs
│     ├─ config.rs
│     ├─ state.rs
│     ├─ error.rs
│     ├─ router.rs
│     ├─ auth.rs
│     ├─ rbac.rs
│     ├─ csrf.rs
│     ├─ events.rs
│     ├─ ws.rs
│     ├─ markdown.rs
│     ├─ repository.rs
│     ├─ repo_files.rs
│     ├─ merge_requests.rs
│     ├─ reviews.rs
│     ├─ issues.rs
│     ├─ ci.rs
│     ├─ releases.rs
│     ├─ settings.rs
│     ├─ search.rs
│     ├─ notifications.rs
│     ├─ audit.rs
│     ├─ admin.rs
│     ├─ static_assets.rs
│     └─ openapi.rs
├─ tests/
│  ├─ web_api_tests.rs
│  ├─ web_markdown_tests.rs
│  └─ web_ws_tests.rs
└─ docs/
   ├─ web-forge.md
   ├─ web-api.md
   └─ web-security.md
```

---

## 6. Backend Architecture

### 6.1 Add `src/web` as the browser BFF

`src/web` owns HTTP/WebSocket concerns only. It must not duplicate core domain logic. It calls `state::Db`, `git_host`, `repo_fleet`, `gitlab_client`, `approval`, `decision`, `release`, `policy`, `runner_backend_registry`, and `api::*` projections.

Responsibilities:

- Build a nested Axum router.
- Serve `/api/v1/*` JSON endpoints.
- Serve `/ws` WebSocket event streams.
- Serve static Vite assets from disk in dev and embedded/dist in production.
- Normalize errors into API problem responses.
- Enforce auth, RBAC, CSRF, rate limits, path safety, and audit logging.
- Emit typed `WebEvent` notifications after every mutation.
- Render Markdown safely and cache it.

### 6.2 Do not pollute `engine.rs`

Keep `engine.rs` focused on webhook ingestion and reconciliation. Add one of these integration patterns:

**Recommended:** `jeryu web serve` starts a web router and also optionally starts the current engine loops when `--with-engine` is passed.

**Optional later:** `jeryu serve --web` nests the web router next to `/hooks` for single-port deployment.

This prevents browser auth/session concerns from leaking into webhook handling.

### 6.3 Add typed web configuration

Add these settings under `Settings`:

```rust
pub web: WebSettings,
pub auth: AuthSettings,
pub realtime: RealtimeSettings,
pub markdown: MarkdownSettings,
pub repo_defaults: RepoDefaultSettings,
```

Detailed settings matrix appears in section 14.

### 6.4 DTOs and generated TypeScript

Rust DTOs should live in `src/api/forge.rs` and `src/api/web.rs` and derive `Serialize`, `Deserialize`, and `TS` or JSON Schema export. Frontend types should be generated at build time where practical. Avoid hand-maintaining duplicate DTOs after phase 2.

Minimum DTO groups:

```text
SessionUser, CapabilitySet, WebProblem, WebEventEnvelope
RepositorySummary, RepositoryDetail, RepositoryCreateRequest, RepositorySettingsPatch
TreeEntry, BlobResponse, MarkdownRenderResponse, CommitSummary, BranchSummary, TagSummary
MergeRequestSummary, MergeRequestDetail, MergeRequestCreateRequest, ReviewThread, ReviewDraft
IssueSummary, IssueDetail, Label, Milestone
PipelineSummary, JobSummary, RunnerPoolSummary
Notification, AuditEvent, SearchResult
```

### 6.5 Repository data authority

`TrackedRepository` already exists in state. Extend the schema instead of replacing it:

- `repositories`: canonical repo identity, provider, owner, name, slug, paths, visibility, default branch, archived flag.
- `repo_settings`: UI/config settings not owned by Git host.
- `repo_permissions`: cached derived permissions for faster page loads.
- `protected_branches`: branch rules and merge policies.
- `repo_activity`: normalized activity feed.
- `markdown_cache`: rendered README/docs cache.

Do not store secrets in repo settings; secrets remain under existing secrets/vault paths.

### 6.6 Git provider model

Support three provider types:

1. `local` — local/bare Git repositories managed by JeRyu.
2. `gitlab` — current embedded GitLab/GitLab API path.
3. `github` — adapter path for GitHub-hosted repos and MRs.

Use `ForgeHost` as a companion trait to existing `GitHost` if extending the current trait would create churn:

```rust
#[async_trait]
pub trait ForgeHost: Send + Sync {
    async fn list_repositories(&self, query: RepositoryQuery) -> Result<Page<RepositorySummary>, HostError>;
    async fn create_repository(&self, input: CreateRepositoryInput) -> Result<RepositorySummary, HostError>;
    async fn get_repository(&self, repo: &RepoRef) -> Result<RepositoryDetail, HostError>;
    async fn list_tree(&self, repo: &RepoRef, ref_name: &str, path: &str) -> Result<Vec<TreeEntry>, HostError>;
    async fn get_blob(&self, repo: &RepoRef, ref_name: &str, path: &str) -> Result<BlobResponse, HostError>;
    async fn list_merge_requests(&self, repo: &RepoRef, filter: MrFilter) -> Result<Page<MergeRequestSummary>, HostError>;
    async fn get_merge_request(&self, repo: &RepoRef, id: &str) -> Result<MergeRequestDetail, HostError>;
    async fn submit_review(&self, input: SubmitReviewInput) -> Result<ReviewReceipt, HostError>;
    async fn merge(&self, input: MergeInput) -> Result<MergeReceipt, HostError>;
    async fn update_settings(&self, repo: &RepoRef, patch: RepositorySettingsPatch) -> Result<RepositorySettings, HostError>;
}
```

### 6.7 HTTP API surface

All endpoints are under `/api/v1` and return JSON. Mutations require CSRF/session or explicit API token auth.

#### Session and capabilities

```text
GET    /api/v1/session
POST   /api/v1/session/login
POST   /api/v1/session/logout
GET    /api/v1/capabilities
GET    /api/v1/settings/effective
PATCH  /api/v1/user/preferences
```

#### Repositories

```text
GET    /api/v1/repos
POST   /api/v1/repos
POST   /api/v1/repos/import
GET    /api/v1/repos/:owner/:repo
PATCH  /api/v1/repos/:owner/:repo
DELETE /api/v1/repos/:owner/:repo
GET    /api/v1/repos/:owner/:repo/activity
GET    /api/v1/repos/:owner/:repo/contributors
GET    /api/v1/repos/:owner/:repo/languages
```

#### Code and Markdown

```text
GET    /api/v1/repos/:owner/:repo/tree/:ref/*path
GET    /api/v1/repos/:owner/:repo/blob/:ref/*path
GET    /api/v1/repos/:owner/:repo/raw/:ref/*path
GET    /api/v1/repos/:owner/:repo/readme/:ref
POST   /api/v1/markdown/render
GET    /api/v1/repos/:owner/:repo/history/:ref/*path
GET    /api/v1/repos/:owner/:repo/blame/:ref/*path
```

#### Branches, tags, commits, compare

```text
GET    /api/v1/repos/:owner/:repo/branches
POST   /api/v1/repos/:owner/:repo/branches
DELETE /api/v1/repos/:owner/:repo/branches/:branch
GET    /api/v1/repos/:owner/:repo/tags
POST   /api/v1/repos/:owner/:repo/tags
GET    /api/v1/repos/:owner/:repo/commits
GET    /api/v1/repos/:owner/:repo/commits/:sha
GET    /api/v1/repos/:owner/:repo/compare/:base...:head
```

#### Merge requests / pull requests

```text
GET    /api/v1/repos/:owner/:repo/mrs
POST   /api/v1/repos/:owner/:repo/mrs
GET    /api/v1/repos/:owner/:repo/mrs/:iid
PATCH  /api/v1/repos/:owner/:repo/mrs/:iid
GET    /api/v1/repos/:owner/:repo/mrs/:iid/diff
GET    /api/v1/repos/:owner/:repo/mrs/:iid/checks
GET    /api/v1/repos/:owner/:repo/mrs/:iid/blockers
POST   /api/v1/repos/:owner/:repo/mrs/:iid/comments
POST   /api/v1/repos/:owner/:repo/mrs/:iid/reviews
POST   /api/v1/repos/:owner/:repo/mrs/:iid/approve
POST   /api/v1/repos/:owner/:repo/mrs/:iid/request-changes
POST   /api/v1/repos/:owner/:repo/mrs/:iid/merge
POST   /api/v1/repos/:owner/:repo/mrs/:iid/rebase
POST   /api/v1/repos/:owner/:repo/mrs/:iid/close
POST   /api/v1/repos/:owner/:repo/mrs/:iid/reopen
```

#### Issues, CI, releases, settings

```text
GET/POST/PATCH /api/v1/repos/:owner/:repo/issues[/:iid]
GET            /api/v1/repos/:owner/:repo/pipelines
GET            /api/v1/repos/:owner/:repo/pipelines/:id
POST           /api/v1/repos/:owner/:repo/pipelines/:id/retry
POST           /api/v1/repos/:owner/:repo/pipelines/:id/cancel
GET            /api/v1/repos/:owner/:repo/releases
POST           /api/v1/repos/:owner/:repo/releases
GET/PATCH      /api/v1/repos/:owner/:repo/settings/general
GET/PATCH      /api/v1/repos/:owner/:repo/settings/access
GET/PATCH      /api/v1/repos/:owner/:repo/settings/branches
GET/PATCH      /api/v1/repos/:owner/:repo/settings/merge-rules
GET/PATCH      /api/v1/repos/:owner/:repo/settings/ci
GET/PATCH      /api/v1/repos/:owner/:repo/settings/webhooks
GET/PATCH      /api/v1/repos/:owner/:repo/settings/secrets
GET/PATCH      /api/v1/repos/:owner/:repo/settings/integrations
```

---

## 7. Real-Time WebSocket Design

### 7.1 Endpoint

```text
GET /ws?token=<session-bound-short-token>
```

The socket authenticates against an existing browser session and then accepts subscribe/unsubscribe frames. Use a short-lived, session-bound WebSocket token to avoid long-lived bearer secrets in JS.

### 7.2 Frame contract

```json
{
  "type": "event",
  "id": "evt_01HY...",
  "seq": 184467,
  "topic": "repo:neverhuman/jeryu",
  "kind": "merge_request.updated",
  "scope": { "owner": "neverhuman", "repo": "jeryu", "mr": "42" },
  "entity": { "type": "merge_request", "id": "42" },
  "version": "2026-05-26",
  "generated_at": "2026-05-26T16:00:00Z",
  "actor": { "kind": "user", "login": "ben" },
  "payload": {}
}
```

### 7.3 Client control frames

```json
{ "type": "subscribe", "topics": ["global", "repo:neverhuman/jeryu", "mr:neverhuman/jeryu:42"] }
{ "type": "unsubscribe", "topics": ["runner:*"] }
{ "type": "ack", "seq": 184467 }
{ "type": "ping", "at": "2026-05-26T16:00:10Z" }
```

### 7.4 Topics

```text
global
repos
repo:<owner>/<repo>
repo:<owner>/<repo>:code
mr:<owner>/<repo>:<iid>
issue:<owner>/<repo>:<iid>
pipeline:<owner>/<repo>:<id>
runner:*
settings:<owner>/<repo>
audit
notifications:<user_id>
```

### 7.5 Event kinds

```text
repository.created
repository.updated
repository.deleted
repository.indexed
repository.health_changed
branch.created
branch.deleted
branch.protection_updated
tag.created
commit.pushed
file.changed
markdown.rendered
merge_request.created
merge_request.updated
merge_request.closed
merge_request.reopened
merge_request.merged
merge_request.blockers_changed
review.comment_added
review.thread_resolved
review.approved
review.changes_requested
ci.pipeline_started
ci.pipeline_updated
ci.job_started
ci.job_log_appended
ci.job_finished
runner.pool_changed
runner.capacity_changed
settings.updated
audit.event_created
notification.created
```

### 7.6 Backpressure and resume

- Each connection has a bounded queue.
- If a queue fills, coalesce low-priority events by topic and entity.
- Important events (`merge_request.merged`, `settings.updated`, `audit.event_created`) are never dropped; the server closes with `retry_after_ms` if the client cannot keep up.
- The client persists the last acked `seq`. On reconnect, call `GET /api/v1/events?after_seq=N` before resubscribing.

---

## 8. Markdown and README Rendering

### 8.1 Requirements

The renderer must produce GitHub-like HTML while being safe by default:

- GitHub-flavored Markdown: tables, task lists, strikethrough, fenced code, autolinks, heading anchors.
- README discovery order: `README.md`, `README.mdx` read-only fallback, `README.rst` later, `README.txt` plain text.
- Relative links rewritten to JeRyu routes.
- Relative images rewritten through a safe raw/blob proxy.
- HTML sanitized with a strict allowlist.
- CSP disallows inline scripts.
- Mermaid/PlantUML are opt-in and rendered in a sandboxed client component or server worker.
- Cache by `(repo_id, ref_name, path, blob_sha, renderer_version)`.
- Maximum render input size configurable; large READMEs fall back to raw/plain preview.

### 8.2 Renderer pipeline

```text
request repo/ref/path
→ resolve repo + permissions
→ resolve blob and SHA
→ markdown_cache lookup
→ pulldown-cmark render with GFM options
→ anchor generation
→ syntax highlighting for fenced code blocks
→ ammonia sanitize
→ link/image rewrite
→ store cache
→ return html + toc + warnings + source metadata
```

### 8.3 Response contract

```json
{
  "repo": "neverhuman/jeryu",
  "ref_name": "main",
  "path": "README.md",
  "blob_sha": "abc123",
  "html": "<article>...</article>",
  "toc": [{ "level": 2, "id": "installation", "title": "Installation" }],
  "warnings": [],
  "rendered_at": "2026-05-26T16:00:00Z",
  "cache": "hit"
}
```

---

## 9. Frontend Architecture

### 9.1 Stack

- Vite
- React 19.x
- TypeScript strict mode
- React Router
- TanStack Query for HTTP cache
- TanStack Virtual for large file/repo/MR lists
- Zustand or small reducer store for shell/realtime/session state
- Monaco or CodeMirror for code editing/diff enhancement after MVP
- MSW for mocks
- Vitest + Testing Library
- Playwright for user journeys
- Existing `ux-qa-check.mjs` preserved and expanded

### 9.2 Route map

```text
/                                      Global dashboard
/repos                                  All repos
/repos/new                              Create repo
/repos/import                           Import repo
/:owner/:repo                           Repo overview + README
/:owner/:repo/tree/:ref/*path           Code tree
/:owner/:repo/blob/:ref/*path           File viewer
/:owner/:repo/edit/:ref/*path           Edit file
/:owner/:repo/commits                   Commits
/:owner/:repo/commit/:sha               Commit detail
/:owner/:repo/branches                  Branches
/:owner/:repo/tags                      Tags
/:owner/:repo/compare/:base...:head     Compare
/:owner/:repo/merge-requests            MR list
/:owner/:repo/merge-requests/new        New MR
/:owner/:repo/merge-requests/:iid       Merge Room
/:owner/:repo/issues                    Issue list
/:owner/:repo/issues/:iid               Issue detail
/:owner/:repo/pipelines                 Pipeline list
/:owner/:repo/pipelines/:id             Pipeline detail
/:owner/:repo/releases                  Releases
/:owner/:repo/settings                  Settings shell
/:owner/:repo/settings/general          General settings
/:owner/:repo/settings/access           Members/roles
/:owner/:repo/settings/branches         Protected branches
/:owner/:repo/settings/merge            Merge rules
/:owner/:repo/settings/ci               CI/runners
/:owner/:repo/settings/webhooks         Webhooks
/:owner/:repo/settings/secrets          Secrets references
/:owner/:repo/settings/integrations     Integrations
/:owner/:repo/settings/danger           Archive/delete/transfer
/admin                                  Instance admin
/audit                                  Audit log
/notifications                          Notifications
```

### 9.3 App shell

The app shell is always present:

- left sidebar: repos, repo families, pinned filters
- top nav: breadcrumbs, branch/repo switcher, global search, command palette
- center: current route content
- right activity rail: live events, blockers, notifications, CI/runner status
- bottom status bar: WebSocket state, current user, selected repo/ref, pending review drafts

### 9.4 Key screens and controls

#### All Repositories dashboard

Controls:

- search by name/description/path/provider/language/topic
- filters: owner, namespace, provider, visibility, archived, favorite, repo family, CI health, review status, stale activity
- sort: recent activity, name, stars/favorites, open MRs, failing CI, runner cost, last push
- quick actions: create repo, import repo, clone command, open settings, create MR from branch
- saved views: “My repos”, “Needs review”, “Failing CI”, “Stale branches”, “Release blockers”

#### Repository overview

Controls:

- clone URL selector: HTTPS/SSH/local
- branch selector
- README docs tabs
- repo health cards: default branch, open MRs, failing checks, runner saturation, latest release, policy status
- activity feed
- quick create: file, branch, MR, issue, release
- “Explain this repo” summary using available metadata/evidence

#### Code browser

Controls:

- branch/tag/commit selector
- path breadcrumbs
- fuzzy file finder
- file tree with virtualization
- file preview with syntax highlight
- raw/download/copy permalink/copy blob SHA
- edit/create/delete file
- history/blame
- README/doc preview side panel
- large file warning and download fallback

#### Merge Room

Controls:

- approve, request changes, comment, merge, squash merge, rebase merge, close, reopen, mark draft/ready
- exact head SHA confirmation before approval/merge
- blocker panel: failing checks, unresolved threads, missing approvals, branch out of date, policy violations, conflicts, required CODEOWNERS
- diff mode: unified/side-by-side, whitespace toggle, viewed toggle, file filters, jump next change/comment
- review draft tray
- CI panel with logs/artifacts/retry/cancel
- evidence panel for JeRyu agent decisions and VibeGate passport
- activity timeline

#### Settings shell

Controls:

- settings nav grouped by General, Access, Branches, Merge, CI, Webhooks, Secrets, Integrations, Danger
- all forms show current value, source of truth, validation state, and a dry-run diff before saving
- dangerous operations require typed confirmation and show audit impact

---

## 10. Data Model Additions

Add migration `db/migrations/20260526_web_forge.sql`.

Core tables:

```sql
CREATE TABLE IF NOT EXISTS web_namespaces (
  id INTEGER PRIMARY KEY,
  provider TEXT NOT NULL,
  full_path TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  kind TEXT NOT NULL DEFAULT 'group',
  avatar_url TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS web_repositories (
  id INTEGER PRIMARY KEY,
  namespace_id INTEGER,
  slug TEXT NOT NULL UNIQUE,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  provider TEXT NOT NULL,
  remote_url TEXT,
  local_path TEXT,
  default_branch TEXT NOT NULL DEFAULT 'main',
  visibility TEXT NOT NULL DEFAULT 'private',
  description TEXT NOT NULL DEFAULT '',
  topics_json TEXT NOT NULL DEFAULT '[]',
  archived INTEGER NOT NULL DEFAULT 0,
  forked_from TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS web_repo_settings (
  repo_id INTEGER PRIMARY KEY,
  features_json TEXT NOT NULL,
  merge_rules_json TEXT NOT NULL,
  review_rules_json TEXT NOT NULL,
  ci_rules_json TEXT NOT NULL,
  security_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS web_protected_branches (
  id INTEGER PRIMARY KEY,
  repo_id INTEGER NOT NULL,
  pattern TEXT NOT NULL,
  require_pr INTEGER NOT NULL DEFAULT 1,
  require_approvals INTEGER NOT NULL DEFAULT 1,
  required_approvals INTEGER NOT NULL DEFAULT 1,
  require_codeowners INTEGER NOT NULL DEFAULT 0,
  require_status_checks INTEGER NOT NULL DEFAULT 1,
  required_checks_json TEXT NOT NULL DEFAULT '[]',
  allow_force_push INTEGER NOT NULL DEFAULT 0,
  allow_deletion INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(repo_id, pattern)
);
```

Merge/review tables:

```sql
CREATE TABLE IF NOT EXISTS web_merge_requests (
  id INTEGER PRIMARY KEY,
  repo_id INTEGER NOT NULL,
  iid TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  source_branch TEXT NOT NULL,
  target_branch TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  base_sha TEXT,
  author TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'open',
  draft INTEGER NOT NULL DEFAULT 0,
  merge_status TEXT NOT NULL DEFAULT 'checking',
  blockers_json TEXT NOT NULL DEFAULT '[]',
  labels_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(repo_id, iid)
);

CREATE TABLE IF NOT EXISTS web_review_threads (
  id INTEGER PRIMARY KEY,
  repo_id INTEGER NOT NULL,
  mr_iid TEXT NOT NULL,
  file_path TEXT,
  old_line INTEGER,
  new_line INTEGER,
  resolved INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS web_review_comments (
  id INTEGER PRIMARY KEY,
  thread_id INTEGER NOT NULL,
  author TEXT NOT NULL,
  body_markdown TEXT NOT NULL,
  body_html TEXT NOT NULL,
  system INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS web_review_submissions (
  id INTEGER PRIMARY KEY,
  repo_id INTEGER NOT NULL,
  mr_iid TEXT NOT NULL,
  reviewer TEXT NOT NULL,
  state TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  body_markdown TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL
);
```

Operational tables:

```sql
CREATE TABLE IF NOT EXISTS web_markdown_cache (
  id INTEGER PRIMARY KEY,
  repo_id INTEGER NOT NULL,
  ref_name TEXT NOT NULL,
  path TEXT NOT NULL,
  blob_sha TEXT NOT NULL,
  renderer_version TEXT NOT NULL,
  html TEXT NOT NULL,
  toc_json TEXT NOT NULL DEFAULT '[]',
  warnings_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  UNIQUE(repo_id, ref_name, path, blob_sha, renderer_version)
);

CREATE TABLE IF NOT EXISTS web_events (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  event_id TEXT NOT NULL UNIQUE,
  topic TEXT NOT NULL,
  kind TEXT NOT NULL,
  scope_json TEXT NOT NULL,
  actor_json TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS web_notifications (
  id INTEGER PRIMARY KEY,
  user_login TEXT NOT NULL,
  kind TEXT NOT NULL,
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  target_url TEXT,
  read_at TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS web_audit_events (
  id INTEGER PRIMARY KEY,
  actor TEXT NOT NULL,
  action TEXT NOT NULL,
  target_type TEXT NOT NULL,
  target_id TEXT NOT NULL,
  ip_hash TEXT,
  user_agent_hash TEXT,
  before_json TEXT,
  after_json TEXT,
  created_at TEXT NOT NULL
);
```

Indexes:

```sql
CREATE INDEX IF NOT EXISTS idx_web_repositories_owner_name ON web_repositories(owner, name);
CREATE INDEX IF NOT EXISTS idx_web_repositories_updated ON web_repositories(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_web_mrs_repo_state_updated ON web_merge_requests(repo_id, state, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_web_events_topic_seq ON web_events(topic, seq DESC);
CREATE INDEX IF NOT EXISTS idx_web_notifications_user_created ON web_notifications(user_login, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_web_audit_created ON web_audit_events(created_at DESC);
```

---

## 11. Authorization and Roles

Roles should map to a capability set rather than ad hoc checks.

| Role | Capabilities |
|---|---|
| Guest | read public/internal metadata, read issues if allowed |
| Reporter | read code, download artifacts/logs, comment on issues/MRs |
| Developer | push non-protected branches, create MRs, run pipelines, manage own branches |
| Maintainer | approve/merge, manage branch protection, manage webhooks, manage runners, edit repo settings |
| Owner/Admin | delete/archive/transfer repo, manage access, instance settings, secrets, dangerous operations |
| Agent | scoped capabilities only through issued grants; no ambient admin |

Every mutating endpoint must call:

```text
authenticate → resolve user → resolve repo role → check capability → validate CSRF/token → dry-run domain action → persist/audit → emit event
```

---

## 12. Security Requirements

1. **Markdown XSS:** sanitize all rendered HTML; never trust raw Markdown HTML by default.
2. **Path traversal:** normalize requested paths; reject `..`, absolute paths, NUL bytes, and symlink escape attempts.
3. **CSRF:** all cookie-authenticated mutations require CSRF token.
4. **CORS:** default same-origin. Dev CORS must be explicit and logged.
5. **Sessions:** secure, HttpOnly, SameSite cookies by default; dev mode may relax only on localhost.
6. **Secrets:** never return secret values through settings APIs. Return existence, scope, fingerprint, and last-rotated metadata only.
7. **Audit:** every settings change, permission change, branch protection change, approval, merge, token/key change, and delete/archive operation records before/after JSON.
8. **Rate limiting:** login, markdown render, search, raw download, WebSocket subscribe, and CI log streaming need limits.
9. **Large files:** set text preview max bytes and raw download controls.
10. **Exact-SHA approvals:** approval/merge requests must include the head SHA visible in the UI; stale SHA gets 409 Conflict.

---

## 13. Performance Requirements

| Surface | Target |
|---|---|
| Dashboard initial data | under 300ms local P95 after warm DB |
| Repo overview | README cached render under 100ms; cold render under 500ms for normal README |
| File tree | lazy load by path; no full repo walk for large repos |
| MR diff | stream file list first, hunks on demand; virtualize hunk rendering |
| WebSocket update | visible within 100ms after event publication in local deployment |
| Search | prefix/fuzzy repo search immediate client-side after first page; server search debounced |
| Settings save | dry-run validation before mutation; commit/audit/event in one transaction |

Implementation choices:

- Keyset pagination for repo/MR/activity feeds.
- `ETag` and `If-None-Match` for blobs, markdown, repository summaries.
- Cache rendered Markdown by blob SHA.
- Use a broadcast hub plus event-log resume, not socket-only ephemeral state.
- Virtualize repo lists, file trees, diff files, comments, and job logs.
- Split frontend chunks by route.

---

## 14. Full Settings Matrix

### 14.1 Instance web settings

| Setting | Default | Description |
|---|---:|---|
| `web.enabled` | true | Enables the web BFF. |
| `web.bind` | `127.0.0.1:9780` | Browser server bind address. |
| `web.public_base_url` | null | External base URL for links/webhooks. |
| `web.api_prefix` | `/api/v1` | API prefix. |
| `web.static_dir` | `apps/web/dist` | Static asset directory in dev/unpackaged mode. |
| `web.spa_fallback` | true | Serve `index.html` for unknown non-API routes. |
| `web.dev_proxy_url` | null | Optional Vite dev server URL. |
| `web.cors_origins` | `[]` | Explicit dev/remote CORS origins. |
| `web.request_timeout_secs` | 30 | Default HTTP timeout. |
| `web.max_json_body_bytes` | 1048576 | Max JSON request body. |
| `web.max_upload_bytes` | 104857600 | Max upload. |

### 14.2 Auth/session settings

| Setting | Default | Description |
|---|---:|---|
| `auth.mode` | `local_or_gitlab` | Local session, GitLab OAuth, OIDC later. |
| `auth.session_cookie` | `jeryu_session` | Cookie name. |
| `auth.session_ttl_hours` | 12 | Session duration. |
| `auth.csrf_cookie` | `jeryu_csrf` | CSRF cookie name. |
| `auth.require_csrf` | true | Require CSRF for mutations. |
| `auth.secure_cookies` | true except localhost | Secure cookie behavior. |
| `auth.allowed_admins` | `[]` | Optional explicit admin logins. |
| `auth.agent_login_prefix` | `agent/` | Prefix for agent identities. |

### 14.3 Realtime settings

| Setting | Default | Description |
|---|---:|---|
| `realtime.enabled` | true | Enable `/ws`. |
| `realtime.max_connections` | 1000 | Instance connection cap. |
| `realtime.max_topics_per_connection` | 64 | Subscription cap. |
| `realtime.channel_capacity` | 1024 | Per-connection queue. |
| `realtime.heartbeat_secs` | 20 | Server heartbeat interval. |
| `realtime.resume_window_events` | 10000 | Event log resume window. |
| `realtime.coalesce_low_priority` | true | Coalesce noisy events. |

### 14.4 Markdown settings

| Setting | Default | Description |
|---|---:|---|
| `markdown.max_input_bytes` | 1048576 | Max Markdown source size. |
| `markdown.allow_raw_html` | false | Raw HTML allowed before sanitize. |
| `markdown.enable_tables` | true | Tables. |
| `markdown.enable_tasklists` | true | Task lists. |
| `markdown.enable_footnotes` | true | Footnotes. |
| `markdown.enable_mermaid` | false | Mermaid diagrams. |
| `markdown.syntax_highlight` | true | Code block highlighting. |
| `markdown.cache_enabled` | true | Persist rendered output. |
| `markdown.renderer_version` | `jeryu-md-v1` | Cache bust key. |

### 14.5 Repository defaults

| Setting | Default | Description |
|---|---:|---|
| `repo_defaults.visibility` | `private` | New repo visibility. |
| `repo_defaults.default_branch` | `main` | New repo branch. |
| `repo_defaults.initialize_readme` | true | Create README on empty repo. |
| `repo_defaults.gitignore_template` | null | Optional template. |
| `repo_defaults.license_template` | null | Optional license. |
| `repo_defaults.merge_method` | `squash` | Preferred merge. |
| `repo_defaults.delete_branch_on_merge` | true | Cleanup after merge. |
| `repo_defaults.require_approval_count` | 1 | Default protected branch approvals. |
| `repo_defaults.require_status_checks` | true | Require CI/status checks. |
| `repo_defaults.required_checks` | `["vibegate/merge-passport"]` | Default required checks. |

### 14.6 Repo-level settings categories

The UI must expose:

- General: name, description, topics, visibility, default branch, avatar, homepage, archive state.
- Features: issues, merge requests, wiki/docs, releases, packages, snippets, actions/pipelines, projects, discussions later.
- Access: members, teams/groups, roles, deploy keys, service accounts, agent scopes.
- Branches: protected branch patterns, push/merge permissions, force-push/delete rules, signed commits, linear history.
- Merge: methods allowed, squash defaults, auto-merge, delete branch, merge trains, stale approvals, CODEOWNERS, conversations resolved.
- CI: pipeline enablement, required checks, runner pools, variables metadata, cache policy, artifacts retention.
- Webhooks: endpoint URL, events, secret fingerprint, SSL verification, retries, last delivery.
- Secrets: references/fingerprints only, vault path, rotation, scope, expiration.
- Integrations: GitHub/GitLab remote, Slack/webhook notifications, issue tracker bridges.
- Danger: rename slug, transfer, archive, unarchive, delete, mirror reset, prune refs.

---

## 15. Frontend UX QA Gates

Preserve and expand the existing `apps/web/ux-qa.md` intent:

- Storybook or component harness for shell, repo cards, code browser, Markdown, MR diff, settings forms.
- Playwright coverage for: create repo, browse README, open file, create MR, approve, merge blocked, save settings, WebSocket reconnect.
- Accessibility: axe/pa11y checks, keyboard navigation, focus traps, contrast, reduced motion.
- Geometry: screenshot checks at 1280×720, 1440×900, 1920×1080, mobile breakpoint.
- Performance: Lighthouse/web-vitals budgets for route chunks and initial load.
- Mocks: MSW fixtures for all API states, including loading/error/empty/permission-denied/stale-SHA.

---

## 16. Implementation Plan

### Phase 0 — Contracts and scaffolding

- Add Rust dependencies.
- Add settings structs/defaults.
- Add `src/api/forge.rs` and `src/api/web.rs` DTOs.
- Add `src/web` skeleton router, error type, state, and health endpoint.
- Add `jeryu web serve` CLI.
- Replace `apps/web` stub with Vite app shell and preserve UX QA scripts.

### Phase 1 — Repos + README

- Repos list/create/import/detail endpoints.
- Code tree/blob/raw endpoints.
- README render endpoint and cache table.
- Frontend dashboard, repo overview, code browser, MarkdownRenderer.
- WebSocket basic connect/subscribe/event display.

### Phase 2 — Merge Room and review

- MR list/detail/diff/blockers endpoints.
- Review comment/thread/submit/approve/request-changes/merge endpoints.
- Diff viewer, file review state, draft tray, blocker panel.
- Exact-SHA conflict handling.

### Phase 3 — Settings and admin

- Repo settings GET/PATCH endpoints with dry-run validation.
- Branch protection and merge rules.
- Webhooks, deploy keys, secrets metadata.
- Audit log and notifications.

### Phase 4 — CI/release/runner intelligence

- Pipeline/job/log/artifact endpoints.
- Runner capacity cards.
- Release/canary/rollback integration.
- Repo families and cross-repo fleet dashboards.

### Phase 5 — Hardening

- OpenAPI/TypeScript generation.
- E2E tests and UX QA gates.
- Security review for Markdown, auth, CSRF, path traversal, secrets.
- Performance budgets and production packaging.

---

## 17. Acceptance Criteria

The implementation is complete when:

1. `npm run web:dev` starts Vite against a local JeRyu API.
2. `npm run web:build` emits `apps/web/dist`.
3. `cargo run -p jeryu -- web serve` serves the SPA, API, and WebSocket.
4. The All Repos page lists local/GitLab/GitHub-backed repos with filters and real-time updates.
5. Creating/importing a repo works and emits `repository.created`.
6. Repo overview renders README.md safely to HTML with correct relative links/images.
7. Code browser can browse tree/blob/raw/history for a repo/ref/path.
8. Merge Room supports diff review, line comments, approvals, request changes, merge, and blocker explanations.
9. Settings pages cover the settings matrix with validation, dry-run, save, audit event, and realtime event.
10. WebSocket reconnect/resume works after refresh/network drop.
11. Dangerous operations are permissioned, audited, and require confirmation.
12. Tests pass: `cargo check -p jeryu`, web API tests, Markdown tests, WebSocket tests, `npm run web:typecheck`, `npm run web:test`, `npm run web:e2e`, and UX QA.

---

## 18. Notes on the Attached Prior Solutions

The uploaded archive contained multiple Markdown spec/diff attempts. The final design above merges the best parts:

- broad GitHub/GitLab parity and settings coverage from the full Git host experience specs,
- concrete `src/web` module decomposition from the platform implementation diffs,
- WebSocket topic/event contracts from the web platform specs,
- Markdown/README rendering requirements from the full web forge spec,
- frontend route/component/control detail from the Vite/React solution drafts,
- and current-repo-specific integration points from live JeRyu files.

The companion `.diff` file is a detailed patch blueprint against the current tree, with concrete file paths, dependency changes, Rust module contracts, frontend scaffolding, SQL migration, and tests to implement this spec.
