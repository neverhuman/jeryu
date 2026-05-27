# JeRyu Web Forge — Final Engineering Specification

**Target repo:** `neverhuman/jeryu`  
**Generated:** 2026-05-26  
**Goal:** Add a Rust + Axum + WebSocket backend and a Vite + TypeScript + React web product that delivers a full GitHub/GitLab-class repository experience, while preserving JeRyu’s existing CLI, TUI, proof lanes, Git host abstractions, repo fleet concepts, runner pools, VTI, release/secrets/cache systems, and agent governance model.

---

## 1. Executive summary

JeRyu already has the hard part of a modern forge: a single-binary Rust control plane, typed API/read-model modules for the TUI, a durable event taxonomy, action preview/result contracts, GitHub/GitLab host adapters, exact-SHA merge gate concepts, repo fleet tooling, agent-aware admission and capability gates, runner orchestration, VTI/smart test selection, cache/release/secrets state, and a strong terminal UX.

The missing layer is the web forge product. The current `apps/web` workspace is a UX-QA evidence placeholder, not a Vite/React application. The final implementation should therefore replace that placeholder with a production app while preserving the evidence discipline. The Rust side should add a `src/web` BFF/gateway and `src/web_events` realtime bus, extend `src/api` with forge read models, and keep CLI/TUI/MCP/web on the same domain services.

The resulting product is not a skin over GitHub/GitLab. It is a clearer, faster, safer forge:

- One shell for all repos, repo families, branches, issues, merge requests, reviews, CI, releases, agents, cache, secrets, and settings.
- Safe server-side Markdown rendering for `README.md` and every `.md` blob, with sanitized HTML, stable anchors, and relative link/image rewriting.
- A keyboard-first merge room with file tree, diff, checks, approvals, inline threads, suggestions, exact-SHA merge passport, and live CI/agent evidence.
- A resumable WebSocket event hub with topic subscriptions, cursor replay, heartbeats, gap recovery, and backpressure.
- Settings that expose the controls users actually need without hiding them across many pages.
- A command palette and context inspector that make high-value actions one keystroke away while still requiring preview, confirmation, grants, and evidence for dangerous operations.

---

## 2. Current-state findings to anchor the implementation

### 2.1 Existing Rust workspace

The root Rust package is `jeryu`; the workspace includes the root package and supporting crates such as `cargo-witness`, `witness-rt`, `cargo-vrc`, `cargo-aer`, `arc-bench`, `tui-capture`, `domain`, `cache-brain`, and `jeryu-gcd`. The root package uses Rust 2024 and sets `default-run = "jeryu"`. The dependency set already includes `tokio`, `clap`, `axum`, `tower-http`, `reqwest`, `sqlx`, `serde`, `git2`, `ratatui`, `notify`, `dashmap`, and other useful foundations.

### 2.2 Existing frontend workspace

The root `package.json` declares one npm workspace: `apps/web`. Today it only exposes UX-QA marker scripts. `apps/web/package.json` is named `@jankurai/ux-qa` and only runs `node ./ux-qa-check.mjs build` and `test`. `ux-qa.ts` and `ux-qa.md` contain evidence-marker strings for Storybook, Playwright, accessibility, layout stability, API mocks, design tokens, and artifact-backed proof. This is useful QA scaffolding, but it is not a web product.

### 2.3 Existing API module

`src/api/mod.rs` states that the API module is the single source of truth for typed projections, event contracts, and action dispatch. Current modules include `actions`, `agent_session`, `capacity`, `dashboards`, `entity`, `events`, `freshness`, `inspection`, `proof`, `read_model`, `runtime_profile`, and `snapshot`.

Important current contracts to reuse:

- `TuiReadModel`, `MissionSnapshot`, `AttentionItem`, `NextActionRecommendation`, `SystemHealth`.
- `TuiEvent`, `TuiEventKind`, `EventStore`.
- `ActionPreview`, `ActionResult`, `ActionContext`, `actions_for_entity`.
- `EntityRef`, `EntityKind`, `Severity`, `HealthLevel`, `DataFreshness`.
- `TestPlanView`, `VtiStatus`, `CacheVerdict`, `EdgeKind`, `ValidationDecision`.

The web product should not create a separate untyped API. It should extend these contracts and export generated/hand-maintained TypeScript types for the SPA.

### 2.4 Existing Git host layer

`src/git_host` already has a trait-based host abstraction, GitHub and GitLab adapters, exact-SHA approval, check-run/status surfaces, open PR/MR summaries, live PR state, per-file PR diffs, and target-branch policy SHA computation. For a full forge, this layer needs to grow from merge-gate primitives into broad repository lifecycle, file browser, commit/ref, issue, MR/review, settings, webhook, branch protection, CI, and permissions surfaces.

### 2.5 Existing CLI dispatch

`src/cli_defs.rs` currently has a bare `Serve` command and `Tui` options. `src/dispatch.rs` handles `Commands::Serve` by ensuring workspace root defaults, loading the GitLab client, opening state, connecting Docker, bringing compose up, installing admission hooks, starting SmartCache, reconciling pools, spawning `engine::run_engine`, and waiting for Ctrl-C. The web implementation should preserve this behavior and add configurable web serving rather than replacing it.

---

## 3. Product principles

### 3.1 Full parity, fewer screens

JeRyu Web should preserve familiar forge concepts: organizations/groups, repositories, branches, commits, tags, trees, blobs, README, issues, labels, milestones, boards, merge requests/pull requests, reviews, inline comments, approvals, checks, pipelines, runners, artifacts, releases, packages/registry, webhooks, members, branch protection, deploy keys, secrets, and audit logs.

But it should not copy the scattered navigation of GitHub/GitLab. Related decisions belong together:

- Repo home combines README, activity, CI posture, open reviews, agents, releases, and quick settings.
- Merge room combines diff, checks, approvals, CI logs, comments, suggestions, exact SHA, and merge controls.
- Settings use a left rail and searchable command surface; no hunting across nested product menus.
- Activity is always visible through a collapsible realtime rail.

### 3.2 Realtime by default

Every repo page subscribes to relevant topics. The UI updates when branches move, files change, MRs receive comments, checks complete, jobs start/fail, runners scale, agents propose patches, secrets rotate, settings change, or releases promote.

Realtime rules:

- Initial page load uses an HTTP snapshot.
- WebSocket `hello` carries the last seen cursor and desired topics.
- Server replies with `hello_ack`, then replay events since cursor when possible.
- If replay is impossible, server sends `gap` and the client refetches snapshots.
- Events are monotonically sequenced and topic-scoped.
- Mutations publish action lifecycle events: `action.previewed`, `action.executed`, `action.failed`.

### 3.3 Safety over raw power

All dangerous mutations are two-step:

1. `POST /api/v1/actions/preview` returns blast radius, exact target refs/SHA, required permission/grant, expected evidence, reversibility, and undo path.
2. `POST /api/v1/actions/execute` requires the preview id, idempotency key, optional typed confirmation phrase, and current target SHA.

Examples requiring preview: merge, branch delete, force push, repository archive/delete, visibility change, branch protection change, secret rotation/deletion, deploy key creation, webhook secret change, runner token rotation, release promote/rollback.

### 3.4 Agent-native but human-governed

Agents are first-class actors, but the UI makes accountability obvious:

- Every agent action links to an evidence receipt.
- Merge checks collapse agent verdicts into one visible merge passport.
- Exact-SHA approval is non-negotiable.
- Production-impacting actions require explicit grants.
- Review UI shows whether a patch came from a human, agent, tool, or host sync.

### 3.5 Fast navigation

Performance targets:

- Repo dashboard first useful paint under 1 second for warm local state.
- Repo tree open under 200 ms for cached refs; under 750 ms cold for medium repos.
- Blob view under 250 ms for text files under 1 MB.
- README rendered HTML served from cache under 100 ms after first render.
- WebSocket reconnect with cursor replay under 500 ms on local network.
- Diff view virtualized; only visible hunks render.

---

## 4. Target repository tree

```text
jeryu/
├── Cargo.toml
├── package.json
├── apps/
│   └── web/
│       ├── AGENTS.md
│       ├── README.md
│       ├── index.html
│       ├── package.json
│       ├── vite.config.ts
│       ├── tsconfig.json
│       ├── tsconfig.node.json
│       ├── playwright.config.ts
│       ├── ux-qa-check.mjs
│       ├── ux-qa.md
│       ├── ux-qa.ts
│       └── src/
│           ├── main.tsx
│           ├── app/
│           │   ├── App.tsx
│           │   ├── providers.tsx
│           │   └── router.tsx
│           ├── api/
│           │   ├── client.ts
│           │   ├── types.ts
│           │   └── websocket.ts
│           ├── components/
│           │   ├── action/
│           │   │   └── ActionPreviewDialog.tsx
│           │   ├── browser/
│           │   │   ├── BlobToolbar.tsx
│           │   │   ├── CodeViewer.tsx
│           │   │   ├── FileTree.tsx
│           │   │   └── MarkdownRenderer.tsx
│           │   ├── command/
│           │   │   └── CommandPalette.tsx
│           │   ├── diff/
│           │   │   ├── DiffViewer.tsx
│           │   │   └── FileDiffList.tsx
│           │   ├── layout/
│           │   │   ├── ActivityRail.tsx
│           │   │   ├── AppShell.tsx
│           │   │   ├── EntityInspector.tsx
│           │   │   ├── RepoBreadcrumbs.tsx
│           │   │   └── TopNav.tsx
│           │   ├── review/
│           │   │   ├── ApprovalPanel.tsx
│           │   │   ├── ReviewComposer.tsx
│           │   │   └── ReviewThread.tsx
│           │   └── settings/
│           │       ├── DangerZone.tsx
│           │       └── SettingsSection.tsx
│           ├── routes/
│           │   ├── Dashboard.tsx
│           │   ├── IssueDetail.tsx
│           │   ├── IssueList.tsx
│           │   ├── MergeRequestDetail.tsx
│           │   ├── MergeRequestList.tsx
│           │   ├── NewRepo.tsx
│           │   ├── PipelineDetail.tsx
│           │   ├── PipelineList.tsx
│           │   ├── RepoBlob.tsx
│           │   ├── RepoHome.tsx
│           │   ├── RepoSettings.tsx
│           │   ├── RepoTree.tsx
│           │   ├── ReposList.tsx
│           │   └── SettingsHome.tsx
│           ├── stores/
│           │   ├── useCommandPalette.ts
│           │   ├── useEventStore.ts
│           │   ├── useRepoStore.ts
│           │   ├── useReviewStore.ts
│           │   └── useSettingsStore.ts
│           ├── styles/
│           │   ├── global.css
│           │   └── tokens.css
│           └── test/
│               ├── markdown-renderer.test.tsx
│               └── websocket-replay.test.ts
├── db/
│   └── migrations/
│       └── 202606010001_web_forge_core.sql
├── docs/
│   └── web-forge.md
├── src/
│   ├── api/
│   │   ├── merge_request.rs
│   │   ├── repo_browser.rs
│   │   ├── repository.rs
│   │   ├── settings.rs
│   │   └── web_read_model.rs
│   ├── merge/
│   │   ├── mod.rs
│   │   └── service.rs
│   ├── repo_browser/
│   │   ├── markdown.rs
│   │   └── mod.rs
│   ├── repos/
│   │   ├── mod.rs
│   │   └── service.rs
│   ├── web/
│   │   ├── command.rs
│   │   ├── error.rs
│   │   ├── markdown.rs
│   │   ├── mod.rs
│   │   ├── router.rs
│   │   ├── state.rs
│   │   ├── ws.rs
│   │   └── rest/
│   │       ├── actions.rs
│   │       ├── bootstrap.rs
│   │       ├── code.rs
│   │       ├── markdown.rs
│   │       ├── merge_requests.rs
│   │       ├── mod.rs
│   │       ├── repos.rs
│   │       └── settings.rs
│   └── web_events/
│       ├── bus.rs
│       ├── mod.rs
│       └── protocol.rs
└── tests/
    ├── markdown_rendering.rs
    └── web_api_smoke.rs
```

---

## 5. Backend architecture

### 5.1 Module responsibilities

| Module | Responsibility |
|---|---|
| `src/web` | Axum BFF, REST router, WebSocket upgrade, static SPA serving, web command entrypoint. |
| `src/web/rest` | Thin HTTP handlers that call domain services and action preview/execute APIs. |
| `src/web_events` | Realtime event protocol, topic model, cursor ring, broadcaster, replay/gap handling. |
| `src/repos` | Repository listing, create/import/adopt/fork/mirror orchestration, repo family metadata. |
| `src/repo_browser` | Git-backed refs/tree/blob/commit/blame/history and Markdown rendering orchestration. |
| `src/merge` | MR/PR list/detail/review/approve/merge orchestration over Git host adapters and JeRyu gates. |
| `src/api/repository.rs` | Serializable repository/domain DTOs shared by HTTP and frontend. |
| `src/api/repo_browser.rs` | Serializable tree/blob/Markdown/commit/diff DTOs. |
| `src/api/merge_request.rs` | Serializable MR/review/check/approval DTOs. |
| `src/api/settings.rs` | Serializable global/repo settings DTOs and validation outputs. |
| `src/api/web_read_model.rs` | Initial web bootstrap snapshot and dashboard summary. |

### 5.2 REST router

Mount under `/api/v1` to keep existing routes stable.

```text
GET    /healthz
GET    /api/v1/bootstrap
GET    /api/v1/dashboard

GET    /api/v1/repos
POST   /api/v1/repos
GET    /api/v1/repos/:repo_id
PATCH  /api/v1/repos/:repo_id
POST   /api/v1/repos/:repo_id/archive
DELETE /api/v1/repos/:repo_id

GET    /api/v1/repos/:repo_id/refs
GET    /api/v1/repos/:repo_id/branches
POST   /api/v1/repos/:repo_id/branches
GET    /api/v1/repos/:repo_id/tags
GET    /api/v1/repos/:repo_id/commits
GET    /api/v1/repos/:repo_id/commits/:sha
GET    /api/v1/repos/:repo_id/compare

GET    /api/v1/repos/:repo_id/tree/*path
GET    /api/v1/repos/:repo_id/blob/*path
GET    /api/v1/repos/:repo_id/raw/*path
GET    /api/v1/repos/:repo_id/markdown/*path
POST   /api/v1/markdown/render
GET    /api/v1/repos/:repo_id/blame/*path
GET    /api/v1/repos/:repo_id/history/*path

GET    /api/v1/repos/:repo_id/issues
POST   /api/v1/repos/:repo_id/issues
GET    /api/v1/repos/:repo_id/issues/:issue_iid
PATCH  /api/v1/repos/:repo_id/issues/:issue_iid
POST   /api/v1/repos/:repo_id/issues/:issue_iid/comments

GET    /api/v1/repos/:repo_id/merge-requests
POST   /api/v1/repos/:repo_id/merge-requests
GET    /api/v1/repos/:repo_id/merge-requests/:mr_iid
PATCH  /api/v1/repos/:repo_id/merge-requests/:mr_iid
GET    /api/v1/repos/:repo_id/merge-requests/:mr_iid/diff
POST   /api/v1/repos/:repo_id/merge-requests/:mr_iid/comments
POST   /api/v1/repos/:repo_id/merge-requests/:mr_iid/reviews
POST   /api/v1/repos/:repo_id/merge-requests/:mr_iid/approve
POST   /api/v1/repos/:repo_id/merge-requests/:mr_iid/request-changes
POST   /api/v1/repos/:repo_id/merge-requests/:mr_iid/merge

GET    /api/v1/repos/:repo_id/pipelines
GET    /api/v1/repos/:repo_id/pipelines/:pipeline_id
GET    /api/v1/repos/:repo_id/jobs/:job_id/log
POST   /api/v1/repos/:repo_id/jobs/:job_id/retry
POST   /api/v1/repos/:repo_id/jobs/:job_id/cancel

GET    /api/v1/repos/:repo_id/releases
POST   /api/v1/repos/:repo_id/releases
GET    /api/v1/repos/:repo_id/packages
GET    /api/v1/repos/:repo_id/webhooks
POST   /api/v1/repos/:repo_id/webhooks

GET    /api/v1/settings
PATCH  /api/v1/settings
GET    /api/v1/repos/:repo_id/settings
PATCH  /api/v1/repos/:repo_id/settings
GET    /api/v1/repos/:repo_id/members
PUT    /api/v1/repos/:repo_id/members/:principal_id
DELETE /api/v1/repos/:repo_id/members/:principal_id
GET    /api/v1/repos/:repo_id/protection
PATCH  /api/v1/repos/:repo_id/protection
GET    /api/v1/repos/:repo_id/secrets
POST   /api/v1/repos/:repo_id/secrets
POST   /api/v1/repos/:repo_id/secrets/:secret_name/rotate
DELETE /api/v1/repos/:repo_id/secrets/:secret_name

POST   /api/v1/actions/preview
POST   /api/v1/actions/execute
GET    /api/v1/activity
GET    /ws
```

### 5.3 WebSocket protocol

#### Endpoint

```text
GET /ws?cursor=<u64>&topics=global,repo:<id>,repo:<id>:mr:<iid>
```

#### Client message

```json
{
  "type": "hello",
  "client_id": "browser-tab-uuid",
  "last_cursor": 1240,
  "topics": ["global", "repo:42", "repo:42:mr:7"],
  "accept_snapshot": true
}
```

#### Server messages

```json
{ "type": "hello_ack", "server_time": "2026-05-26T16:00:00Z", "cursor": 1302, "replayed": 62 }
{ "type": "event", "cursor": 1303, "topic": "repo:42:mr:7", "event": { "kind": "merge_request.updated" } }
{ "type": "snapshot", "topic": "repo:42", "reason": "initial_or_gap", "value": { } }
{ "type": "gap", "from": 1240, "to": 1302, "reason": "cursor_evicted" }
{ "type": "heartbeat", "cursor": 1303, "server_time": "2026-05-26T16:00:05Z" }
{ "type": "error", "code": "unauthorized_topic", "message": "missing repo read permission" }
```

#### Topic hierarchy

```text
global
settings
repos
repo:<repo_id>
repo:<repo_id>:activity
repo:<repo_id>:refs
repo:<repo_id>:tree:<ref>
repo:<repo_id>:mr:<mr_iid>
repo:<repo_id>:issue:<issue_iid>
repo:<repo_id>:pipeline:<pipeline_id>
repo:<repo_id>:job:<job_id>
repo:<repo_id>:settings
repo-family:<family_id>
agent:<agent_id>
```

#### Event kinds to add

```text
repository.created
repository.updated
repository.deleted
repository.archived
repository.import.started
repository.import.completed
repository.visibility.changed
repository.member.added
repository.member.removed
repository.settings.updated
repository.branch.created
repository.branch.deleted
repository.branch.protected
repository.branch.unprotected
repository.default_branch.changed
repository.tag.created
repository.commit.created
repository.tree.changed
repository.blob.updated
repository.markdown.rendered
issue.created
issue.updated
issue.closed
issue.reopened
merge_request.created
merge_request.updated
merge_request.diff.updated
merge_request.thread.created
merge_request.thread.resolved
merge_request.review.submitted
merge_request.approved
merge_request.changes_requested
merge_request.merge_passport.updated
merge_request.merged
pipeline.created
pipeline.updated
job.log.chunk
runner.pool.updated
secret.rotated
webhook.delivered
webhook.failed
notification.created
action.previewed
action.executed
action.failed
```

---

## 6. Data model additions

Add a migration such as `db/migrations/202606010001_web_forge_core.sql`. Use the existing SQLite-default state approach and keep SQL mutations behind `state::Db` or domain repositories.

### 6.1 Core tables

```sql
CREATE TABLE web_repositories (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  provider TEXT NOT NULL,
  provider_repo_id TEXT,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  slug TEXT NOT NULL UNIQUE,
  family TEXT,
  description TEXT,
  visibility TEXT NOT NULL DEFAULT 'private',
  default_branch TEXT NOT NULL DEFAULT 'main',
  local_path TEXT,
  remote_url TEXT,
  avatar_url TEXT,
  archived INTEGER NOT NULL DEFAULT 0,
  forked_from TEXT,
  mirrored_from TEXT,
  topics_json TEXT NOT NULL DEFAULT '[]',
  settings_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE web_memberships (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  repo_id INTEGER NOT NULL REFERENCES web_repositories(id) ON DELETE CASCADE,
  principal_type TEXT NOT NULL,
  principal_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  role TEXT NOT NULL,
  granted_by TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(repo_id, principal_type, principal_id)
);

CREATE TABLE web_branch_protections (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  repo_id INTEGER NOT NULL REFERENCES web_repositories(id) ON DELETE CASCADE,
  pattern TEXT NOT NULL,
  require_merge_passport INTEGER NOT NULL DEFAULT 1,
  required_checks_json TEXT NOT NULL DEFAULT '[]',
  required_approvals INTEGER NOT NULL DEFAULT 1,
  require_codeowners INTEGER NOT NULL DEFAULT 0,
  allow_force_push INTEGER NOT NULL DEFAULT 0,
  allow_deletion INTEGER NOT NULL DEFAULT 0,
  stale_review_dismissal INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(repo_id, pattern)
);

CREATE TABLE web_markdown_cache (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  repo_id INTEGER NOT NULL REFERENCES web_repositories(id) ON DELETE CASCADE,
  ref_name TEXT NOT NULL,
  path TEXT NOT NULL,
  blob_sha TEXT NOT NULL,
  source_sha256 TEXT NOT NULL,
  html TEXT NOT NULL,
  headings_json TEXT NOT NULL DEFAULT '[]',
  links_json TEXT NOT NULL DEFAULT '[]',
  rendered_at TEXT NOT NULL,
  UNIQUE(repo_id, ref_name, path, blob_sha)
);

CREATE TABLE web_activity_events (
  cursor INTEGER PRIMARY KEY AUTOINCREMENT,
  topic TEXT NOT NULL,
  kind TEXT NOT NULL,
  repo_id INTEGER,
  actor_id TEXT,
  severity TEXT NOT NULL DEFAULT 'info',
  payload_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX idx_web_activity_topic_cursor ON web_activity_events(topic, cursor);
CREATE INDEX idx_web_activity_repo_cursor ON web_activity_events(repo_id, cursor);
```

### 6.2 Product model tables

Add at the same migration boundary or split into follow-up migrations:

- `web_issues`
- `web_issue_comments`
- `web_labels`
- `web_milestones`
- `web_boards`
- `web_merge_requests`
- `web_review_threads`
- `web_review_comments`
- `web_review_submissions`
- `web_merge_passports`
- `web_notifications`
- `web_subscriptions`
- `web_webhooks`
- `web_webhook_deliveries`
- `web_deploy_keys`
- `web_repo_secrets`
- `web_audit_log`
- `web_repo_families`
- `web_repo_mirrors`
- `web_saved_views`

### 6.3 Settings JSON schema

Use typed Rust structs and store the current effective settings as JSON in `web_repositories.settings_json` while also normalizing high-cardinality items into tables.

```rust
pub struct RepoSettingsView {
    pub general: GeneralRepoSettings,
    pub features: FeatureToggles,
    pub merge: MergeSettings,
    pub protection: Vec<BranchProtectionRule>,
    pub ci: CiSettings,
    pub agents: AgentSettings,
    pub webhooks: Vec<WebhookView>,
    pub secrets: Vec<SecretSummary>,
    pub access: AccessSettings,
    pub retention: RetentionSettings,
    pub danger_zone: DangerZoneState,
}
```

---

## 7. Git host trait expansion

The current `GitHost` trait already has the correct philosophy. Extend it without weakening exact-SHA rules.

### 7.1 Repository lifecycle

```rust
async fn list_repositories(&self, filter: RepoListFilter) -> Result<Vec<HostRepository>, HostError>;
async fn create_repository(&self, input: CreateRepository) -> Result<HostRepository, HostError>;
async fn update_repository_settings(&self, repo: &RepoRef, patch: RepoSettingsPatch) -> Result<HostRepository, HostError>;
async fn archive_repository(&self, repo: &RepoRef, dry_run: bool) -> Result<ActionReceipt, HostError>;
async fn delete_repository(&self, repo: &RepoRef, confirmation: &str, dry_run: bool) -> Result<ActionReceipt, HostError>;
```

### 7.2 Refs, commits, tree, blob

```rust
async fn list_refs(&self, repo: &RepoRef) -> Result<Vec<HostRef>, HostError>;
async fn create_branch(&self, repo: &RepoRef, name: &str, from: &str) -> Result<HostRef, HostError>;
async fn delete_branch(&self, repo: &RepoRef, name: &str, expected_sha: &str) -> Result<ActionReceipt, HostError>;
async fn list_tree(&self, repo: &RepoRef, ref_name: &str, path: &str) -> Result<TreeListing, HostError>;
async fn get_blob(&self, repo: &RepoRef, ref_name: &str, path: &str) -> Result<BlobView, HostError>;
async fn get_commit(&self, repo: &RepoRef, sha: &str) -> Result<CommitView, HostError>;
async fn compare_refs(&self, repo: &RepoRef, base: &str, head: &str) -> Result<CompareView, HostError>;
```

### 7.3 Issues and merge requests

```rust
async fn list_issues(&self, repo: &RepoRef, filter: IssueFilter) -> Result<Vec<IssueSummary>, HostError>;
async fn create_issue(&self, repo: &RepoRef, input: CreateIssue) -> Result<IssueDetail, HostError>;
async fn update_issue(&self, repo: &RepoRef, iid: &str, patch: IssuePatch) -> Result<IssueDetail, HostError>;

async fn list_merge_requests(&self, repo: &RepoRef, filter: MergeRequestFilter) -> Result<Vec<MergeRequestSummary>, HostError>;
async fn get_merge_request(&self, repo: &RepoRef, iid: &str) -> Result<MergeRequestDetail, HostError>;
async fn submit_review(&self, repo: &RepoRef, iid: &str, review: SubmitReview) -> Result<ReviewReceipt, HostError>;
async fn merge(&self, repo: &RepoRef, iid: &str, input: MergeInput) -> Result<MergeReceipt, HostError>;
```

### 7.4 Settings and permissions

```rust
async fn list_members(&self, repo: &RepoRef) -> Result<Vec<MemberView>, HostError>;
async fn upsert_member(&self, repo: &RepoRef, input: MemberGrant) -> Result<MemberView, HostError>;
async fn remove_member(&self, repo: &RepoRef, principal_id: &str) -> Result<ActionReceipt, HostError>;
async fn get_branch_protection(&self, repo: &RepoRef) -> Result<Vec<BranchProtectionRule>, HostError>;
async fn update_branch_protection(&self, repo: &RepoRef, rules: Vec<BranchProtectionRule>) -> Result<ActionReceipt, HostError>;
async fn list_webhooks(&self, repo: &RepoRef) -> Result<Vec<WebhookView>, HostError>;
async fn create_or_update_webhook(&self, repo: &RepoRef, input: WebhookInput) -> Result<WebhookView, HostError>;
```

---

## 8. Markdown rendering requirements

### 8.1 Supported behavior

- Render `README.md`, `README.markdown`, `README.mdx` as repo home content, preferring the default branch.
- Render any `.md`, `.markdown`, `.mdown`, `.mkd` blob with source/render/split modes.
- Support GitHub-flavored Markdown tables, strikethrough, task lists, autolinks, heading anchors, footnotes, fenced code blocks, and frontmatter display.
- Rewrite relative links and images based on repo, ref, and path.
- Preserve source line references for headings when possible.
- Cache by `(repo_id, ref_name, path, blob_sha, source_sha256)`.
- Expose render metadata: headings, links, images, warnings, source digest, render time.

### 8.2 Security rules

- Server renders Markdown to HTML.
- Server sanitizes HTML with an allowlist.
- Strip scripts, event handlers, unknown protocols, inline JavaScript URLs, unsafe SVG content, and dangerous iframes.
- Default CSP: `default-src 'self'; script-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'`.
- User content HTML rendered in a contained component with no React `dangerouslySetInnerHTML` outside `MarkdownRenderer`.
- Relative image URLs proxy through authenticated raw/blob endpoints so private repo assets stay private.
- External links add `rel="nofollow noopener noreferrer"` and open behavior respects user preference.

### 8.3 Backend renderer

Use `comrak` for Markdown parsing and `ammonia` for sanitization. Add a small postprocess step for link rewriting and heading anchor collection.

### 8.4 Required tests

- `README.md` with headings produces stable slug anchors.
- Relative links from `docs/setup.md` rewrite against `docs/`.
- Relative image links rewrite to raw endpoint with repo/ref/path.
- `<script>`, `onerror`, `javascript:` URLs are removed.
- Task lists and tables render.
- Large files are rejected or streamed with a friendly error.
- Binary blobs are not passed to Markdown renderer.

---

## 9. Frontend UX specification

### 9.1 Global app shell

Persistent regions:

- Top nav: repo switcher, global search, command palette, create button, current actor, sync status.
- Left sidebar: dashboard, repos, issues, merge requests, pipelines, agents, releases, settings.
- Center workspace: route content.
- Right inspector: selected entity details, actions, evidence, related events.
- Bottom or right activity rail: realtime events and notifications.

Keyboard defaults:

```text
Cmd/Ctrl+K  command palette
g d         dashboard
g r         repositories
g i         issues
g m         merge requests
g p         pipelines
g s         settings
/           focus page search/filter
.           open web editor for current file
b           branch/ref switcher
r           refresh snapshot
[ ]         previous/next file or thread
v           mark file viewed
c           comment on selected line/hunk
A           approve MR
R           request changes
M           merge when passport allows
Esc         close drawer/dialog/preview
```

### 9.2 All repositories dashboard

Controls:

- Search by name, owner, description, topic, language, family.
- Filter by provider, visibility, owner, family, archived, stale, active MR, failing CI, agent activity.
- Sort by recent activity, name, health, open MRs, failing checks, stars/watchers if provider supports.
- Create/import/adopt/mirror/fork repo.
- Bulk pin, archive, family assign, default branch audit.
- Live badges for CI, reviews, agent races, runners, vulnerabilities, releases.

### 9.3 Repo home

Default layout:

- Header: repo identity, visibility, branch switcher, clone URLs, star/watch/pin, quick create, settings.
- Summary cards: open MR, failing check, active agent, latest release, cache posture, runner posture.
- README render with source/render/split toggles.
- File tree preview and recent commits.
- Activity and release timeline.
- Contextual quick actions: create branch, open web editor, new issue, new MR, run pipeline, create release, edit settings.

### 9.4 Code browser

Controls:

- Branch/tag/commit selector.
- Breadcrumb path navigation.
- Typeahead file finder.
- Render/source/raw/download/copy permalink/copy path.
- History, blame, compare, open in editor.
- Markdown render/split/source.
- Virtualized tree and blob line rendering.
- Binary file preview where safe; otherwise metadata and download.

### 9.5 Merge room

One page, no hidden state:

- Header: title, source → target, exact head SHA, target branch SHA, merge passport state.
- Left: file list with viewed state, changed counts, filters.
- Center: virtualized diff with inline comments, suggestions, resolved state.
- Right: checks, approvals, CODEOWNERS, agent evidence, CI jobs, reviewer summary, merge controls.
- Bottom: review composer and timeline.

Controls:

- Approve, request changes, comment, resolve/unresolve thread, apply suggestion, mark viewed, re-run checks, retry job, merge, squash merge, rebase, close/reopen, assign reviewers, label, milestone.
- All dangerous controls call action preview first.
- Merge button disabled unless exact-SHA passport is valid.

### 9.6 Settings UX

Settings sections:

1. General: name, description, topics, avatar, homepage, default branch, visibility, archive.
2. Features: issues, merge requests, wiki/docs, packages, releases, CI/CD, agents, discussions/snippets if supported.
3. Access: members, teams, roles, deploy keys, service accounts, agent identities.
4. Branch protection: patterns, required checks, approvals, CODEOWNERS, stale reviews, force push/delete, signed commits.
5. Merge policy: merge/squash/rebase allowed, auto-merge, delete source branch, merge train, required passport.
6. CI/CD: variables, runners, runner pools, artifacts, caches, schedules, pipeline triggers, environments.
7. Webhooks: endpoints, events, secrets, delivery logs, replay failed delivery.
8. Secrets: repo secrets, environment secrets, rotation policy, audit.
9. Integrations: GitHub/GitLab remotes, MCP endpoints, package/registry proxy, external issue tracker.
10. Notifications: watch rules, email/web/push/webhook preferences.
11. Retention: logs, artifacts, caches, evidence receipts, activity event window.
12. Danger zone: archive, transfer, mirror reset, delete, force unlock.

---

## 10. Action preview contract

### 10.1 Request

```json
{
  "action": "merge_request.merge",
  "entity": { "kind": "merge_request", "id": "repo:42:mr:7" },
  "parameters": {
    "method": "squash",
    "delete_source_branch": true,
    "expected_head_sha": "abc123",
    "expected_target_sha": "def456"
  }
}
```

### 10.2 Response

```json
{
  "preview_id": "apv_01HX...",
  "action": "merge_request.merge",
  "risk": "production_impact",
  "reversible": false,
  "allowed": false,
  "requires_confirmation": true,
  "confirmation_phrase": "merge abc123 into main",
  "required_permission": "repo.merge",
  "required_grant": "merge_production",
  "target_sha": "abc123",
  "blast_radius": [
    "updates target branch main",
    "closes MR !7",
    "deletes branch feature/foo",
    "publishes merge passport evidence"
  ],
  "blocked_by": [
    { "kind": "check", "id": "vibegate/merge-passport", "reason": "pending" }
  ],
  "evidence_expected": ["merge-passport", "approval-receipt", "ci-summary"],
  "undo": { "kind": "revert_commit", "available": true }
}
```

---

## 11. Implementation plan

### Phase 0 — Contracts and build hygiene

- Add `src/api/repository.rs`, `repo_browser.rs`, `merge_request.rs`, `settings.rs`, and `web_read_model.rs`.
- Add `src/web_events` protocol and bus.
- Add `src/web` skeleton router, error handling, state, WebSocket upgrade.
- Update root `package.json` scripts.
- Replace `apps/web/package.json` with real Vite scripts while keeping `ux-qa` scripts.
- Add dependencies to `Cargo.toml`.
- Add module declarations in `src/lib.rs` and `src/api/mod.rs`.

### Phase 1 — Repos dashboard and repo home

- Implement repo discovery from local workspace and configured providers.
- Implement `GET /api/v1/repos`, `POST /api/v1/repos`, `GET /api/v1/repos/:id`.
- Implement repo home route, README detection, Markdown render/cache.
- Add initial Dashboard, ReposList, NewRepo, RepoHome routes.

### Phase 2 — Code browser

- Implement refs, tree, blob, raw, history, blame endpoints.
- Add FileTree, CodeViewer, BlobToolbar, MarkdownRenderer.
- Add branch/ref switcher, file finder, source/render/split.

### Phase 3 — Merge room

- Implement MR list/detail/diff/review/approval/merge endpoints over Git host adapters.
- Add MergeRequestList and MergeRequestDetail routes.
- Add diff virtualization, inline threads, review composer, approval panel, exact-SHA merge passport.

### Phase 4 — Settings parity

- Add settings read/update endpoints.
- Add settings validation and preview for dangerous changes.
- Implement General, Access, Branch Protection, Merge Policy, CI/CD, Webhooks, Secrets, Integrations, Notifications, Retention, Danger Zone panels.

### Phase 5 — Realtime and observability

- Wire existing TUI/control-plane events into `WebEventBus`.
- Persist `web_activity_events` with cursor replay.
- Add topic subscriptions, client-side event store, reconnect/resume/gap recovery.
- Add metrics: active websocket connections, replay lag, event publish latency, markdown render latency, API route latency.

### Phase 6 — Polish and superiority features

- Command palette for every action.
- Entity inspector with evidence and next actions.
- Repo families and fleet-wide dashboard.
- Saved views and filters.
- Keyboard-first review completion.
- Multi-pane workspaces and pinned repo tabs.
- Accessibility hardening and Storybook/Playwright visual QA.

---

## 12. Validation plan

### 12.1 Rust

```bash
cargo fmt --check
cargo check -p jeryu --message-format=json
cargo nextest run -p jeryu --lib
cargo nextest run -p jeryu --test markdown_rendering
cargo nextest run -p jeryu --test web_api_smoke
cargo run -p jeryu -- web check --json
```

### 12.2 Frontend

```bash
npm install
npm run typecheck
npm run lint
npm run test
npm run build
npm run ux-qa
npx playwright test
```

### 12.3 End-to-end scenarios

1. Create repository from web, see it in repo list without refresh.
2. Repo home renders `README.md` safely and rewrites relative image/link URLs.
3. Browse tree, open Markdown blob, toggle source/render/split.
4. Open MR, comment inline, approve, wait for merge passport, merge with exact SHA.
5. Force a CI job failure; activity rail receives job failure and MR passport updates.
6. Update branch protection; preview shows blast radius and WebSocket settings event fires.
7. Reconnect WebSocket with old cursor; receive replay or gap and refetch snapshot.
8. Attempt unsafe Markdown; scripts and unsafe protocols are removed.
9. Rotate repo secret; preview, execute, audit event, activity event.
10. Use keyboard-only navigation through dashboard, repo home, code browser, and merge room.

---

## 13. Acceptance criteria

### 13.1 Repo dashboard

- Shows all configured repos across local workspace, GitLab, GitHub, and imported mirrors.
- Search/filter/sort work client-side on loaded data and server-side for large fleets.
- Create/import/adopt/mirror flows exist.
- Repo family grouping is visible and configurable.

### 13.2 README and Markdown

- Repo home renders README as sanitized HTML.
- Any `.md` file has render/source/split controls.
- Relative links and images are correct for the selected ref/path.
- Unsafe HTML is removed.
- Render cache invalidates on blob SHA change.

### 13.3 Code browser

- Tree, blob, raw, history, blame, compare, refs work for local and provider repos.
- Large files and binary files receive safe fallback UI.
- Permalinks include immutable SHA option.

### 13.4 Merge room

- MR list/detail/diff/review/approve/request-changes/merge flows work.
- Inline threads and suggestions work.
- Merge requires valid passport and exact head SHA.
- CI, approvals, CODEOWNERS, and agent evidence are visible together.

### 13.5 Settings

- General, features, access, branch protection, merge policy, CI/CD, webhooks, secrets, integrations, notifications, retention, and danger zone are exposed.
- Dangerous changes require preview and confirmation.
- Changes publish events and audit records.

### 13.6 Realtime

- WebSocket subscribes, resumes, heartbeats, replays, and gap-recovers.
- Repo/MR/CI/settings/activity updates appear without manual refresh.
- Client degrades to polling if WebSocket fails.

### 13.7 UX

- Command palette covers all high-value actions.
- Keyboard-only review flow is complete.
- A11y checks pass for critical routes.
- Visual regression coverage exists for dashboard, repo home, code browser, merge room, and settings.

---

## 14. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Big-bang forge scope becomes too large. | Land vertical slices: repos + README, then code browser, then merge room, then settings. |
| Duplicate API models diverge from TUI. | Extend `src/api` and share read models/events/actions. |
| Markdown XSS. | Server-side sanitize, CSP, strict tests, no unsanitized HTML surfaces. |
| WebSocket memory growth. | Bounded ring, persisted activity replay, backpressure, topic caps. |
| GitHub/GitLab parity mismatch. | Provider trait has capability flags and graceful unsupported states. |
| Settings UI causes dangerous changes. | Preview/execute contract, confirmation phrases, grants, audit events. |
| Existing `Serve` behavior regresses. | Keep engine startup path, add `ServeCommand` options with defaults matching current behavior. |
| Frontend QA placeholder gets lost. | Preserve/upgrade `ux-qa` scripts and marker evidence in real app. |

---

## 15. Immediate implementation checklist

- [ ] Add Rust dependencies for WebSocket/static serving/Markdown/sanitization/schema support.
- [ ] Add `src/web`, `src/web_events`, `src/repos`, `src/repo_browser`, `src/merge` modules.
- [ ] Add `src/api` forge DTO modules and export them in `src/api/mod.rs`.
- [ ] Replace `apps/web` UX-QA-only package with Vite/React app, retaining UX-QA scripts.
- [ ] Add app shell, routes, API client, WebSocket client, stores, and design tokens.
- [ ] Add Markdown rendering service and tests.
- [ ] Add database migration for web forge state.
- [ ] Add `jeryu web serve/check/export-types` commands and update `jeryu serve` defaults.
- [ ] Add docs: `docs/web-forge.md` and `apps/web/README.md`.
- [ ] Wire first vertical slice: repo list → repo home → README render → activity socket.

---

## 16. Non-negotiable invariants

1. Main remains a dispatcher; no business logic goes into `src/main.rs`.
2. State mutations go through domain services or `state::Db`; no ad-hoc SQL in handlers.
3. Web/TUI/CLI/MCP use shared action preview/execute and shared evidence/gate semantics.
4. Merge approvals and merge execution bind to exact SHAs.
5. Markdown is rendered and sanitized server-side.
6. WebSocket events have monotonic cursors and a replay/gap story.
7. Dangerous settings changes require preview, confirmation, audit, and event publication.
8. Existing TUI and CLI behavior remain supported.
9. UX-QA evidence is upgraded, not removed.
10. Provider capabilities are explicit; unsupported GitHub/GitLab features render as disabled with explanation, not hidden failure.
