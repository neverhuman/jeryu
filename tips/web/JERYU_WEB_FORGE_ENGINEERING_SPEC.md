# JeRyu Full Web Forge: Rust + Vite + TypeScript + React Engineering Spec

**Target outcome:** deliver a modern GitHub/GitLab-class web experience for JeRyu that can browse all repositories, create repositories, configure settings, review files and diffs, approve/merge changes, render `README.md` and other Markdown correctly to HTML, and stream all repository, CI, agent, review, and infrastructure activity in real time over WebSocket.

**Document status:** engineering specification and detailed code-change diff. This is written as an implementation-ready plan for the existing `neverhuman/jeryu` repository, not as a committed patch.

**Primary design rule:** do not replace JeRyu's existing Rust control-plane strengths. Promote the existing typed API/event/action/read-model architecture into a web-facing product layer.

---

## 1. Executive Summary

JeRyu already has the right backend primitives for a better-than-GitHub/GitLab experience:

- A typed API boundary with entity models, actions, read models, snapshots, and events.
- A real-time event taxonomy for jobs, pipelines, logs, agents, grants, caches, releases, security, and actions.
- A TUI mission-control surface that already thinks in terms of attention, next actions, drill-downs, evidence, cache, agents, tests, pools, secrets, and release posture.
- GitHub/GitLab host adapters with merge-gate and exact-SHA approval concepts.
- A repository layout that already includes `apps/web`, `src/api`, `src/git_host`, `src/tui`, `src/mcp`, `src/gateway`, `src/db`, `src/cache`, `src/agent_review`, `src/autonomy`, and `src/test_intel`.

The gap is that `apps/web` is currently not a web application. It is a small npm workspace that only runs a UX-QA marker check. The new work should replace that placeholder with a real Vite + React + TypeScript application and add a Rust web backend-for-frontend layer on top of the existing control-plane model.

The new web product should feel less like a traditional forge and more like a fast operational cockpit:

- **All repos:** searchable, grouped by owner/org/family, live status, quick create/import/fork/mirror.
- **Repo home:** README rendered correctly, repo health, live CI, active agents, recent changes, blocked merge requests.
- **Code browser:** virtualized tree, branch/tag selector, symbol search, file preview, syntax highlighting, blame/history, Markdown rendering.
- **Merge room:** single-page PR/MR review with file tree, inline comments, checks, agents, evidence, merge gates, exact-SHA approval, live logs.
- **Settings:** complete, searchable, safe-by-default configuration for repo, branch protection, webhooks, secrets, CI/runners, agents, access, security, notifications, retention.
- **Real time:** WebSocket event stream with resume cursors, subscriptions, snapshots, deltas, heartbeats, and gap recovery.
- **Command palette:** every high-value action is available in one keyboard-driven interface.
- **Action safety:** every mutation previews blast radius, required grant, undo path, evidence receipts, and exact target SHA when applicable.

---

## 2. Current Repository Findings

### 2.1 Rust core is the real product today

The repository is a Rust workspace with multiple crates and a primary `jeryu` package. The root `Cargo.toml` declares a workspace including the root package, `cargo-witness`, `witness-rt`, `cargo-vrc`, `cargo-aer`, `arc-bench`, `tui-capture`, `domain`, `cache-brain`, and `jeryu-gcd`. The primary package is named `jeryu`, uses Rust 2024, and sets `default-run = "jeryu"`.

### 2.2 The existing web workspace is not a real UI

The root `package.json` defines an npm workspace containing only `apps/web`. It exposes `ux-qa`, `ux-qa:build`, and `ux-qa:test` scripts. `apps/web/package.json` is named `@jankurai/ux-qa` and only runs `node ./ux-qa-check.mjs build` or `test`.

`ux-qa-check.mjs` reads `ux-qa.ts` and `ux-qa.md`, checks for evidence marker strings such as Storybook, Playwright visual coverage, accessibility, layout stability, API mocks, design tokens, and artifact-backed proof, and writes a JSON receipt under `target/jankurai/ux-qa/npm-workspace`.

This means the current web workspace is best treated as a QA placeholder that must become a production app.

### 2.3 `src/api` is the foundation for a web BFF

The current API module states that all TUI rendering consumes typed API projections, not raw DB/Docker/GitLab state, and that the API module is the single source of truth for entity types, event contracts, and action dispatch.

Important existing surfaces:

- `EntityRef`, `EntityKind`, `Severity`, `HealthLevel`, and `EntityDetail`.
- `TuiReadModel`, `MissionSnapshot`, `AttentionItem`, `NextActionRecommendation`, and `SystemHealth`.
- `TuiEvent`, `TuiEventKind`, and `EventStore`.
- `ActionPreview`, `ActionResult`, `ActionContext`, and `actions_for_entity`.
- `TestPlanView`, `VtiStatus`, `CacheVerdict`, `EdgeKind`, and `ValidationDecision`.

The web app should not invent a separate untyped ad-hoc API. It should extend these contracts into a web read model and use generated TypeScript types.

### 2.4 The Git host layer is close, but not broad enough

`src/git_host` already has the right philosophy:

- trait-based host abstraction;
- GitHub and GitLab adapters;
- exact-SHA approval concept;
- check-run/status surfaces;
- live PR state;
- per-file PR diffs;
- target-branch policy SHA computation.

For a full forge experience, the trait must grow from merge-gate primitives into complete repo lifecycle, code browser, settings, branch protection, review, issues, CI, webhooks, and metadata surfaces.

### 2.5 TUI work should be reused, not bypassed

`src/tui` is extensive and already includes mission, approvals, evidence, cache, pools, LLMs, Git panels, focus, graph, flow, runtime, widgets, action registry, activity, theme, app runtime, and jankurai-related panels. The web app should mirror the successful mental model from the TUI while presenting it through a browser-native shell.

### 2.6 Potential build hygiene issue

The fetched `src/api/mod.rs` exports modules named `capacity`, `dashboards`, `freshness`, `inspection`, `proof`, and `runtime_profile`. During inspection, some direct fetches for those files returned not found or were not visible in the directory listing. Before building the web layer, the first engineering step should be a build sanity pass:

- run `cargo check --workspace`;
- verify every exported module in `src/api/mod.rs` exists;
- either add missing module files or remove stale exports;
- preserve public API compatibility where tests expect those modules.

---


---

## 2A. Verified Current Tree Snapshot (2026-05-26)

This is the implementation baseline for the diff below. The web work must be additive around these surfaces and must not break existing control-plane behavior.

```text
neverhuman/jeryu
├── Cargo.toml                       # Rust workspace, primary package `jeryu`
├── package.json                     # npm workspace; currently only UX-QA scripts
├── apps/
│   └── web/
│       ├── AGENTS.md
│       ├── package.json             # currently `@jankurai/ux-qa`, not Vite/React
│       ├── ux-qa-check.mjs
│       ├── ux-qa.md
│       └── ux-qa.ts
├── db/
│   └── ...                          # existing SQLx/control-plane state and migrations
├── docs/
├── src/
│   ├── api/
│   │   ├── actions.rs               # typed action preview/result model
│   │   ├── entity.rs
│   │   ├── events.rs                # TuiEvent/EventStore/event taxonomy
│   │   ├── read_model.rs            # TuiReadModel/MissionSnapshot/SystemHealth
│   │   ├── snapshot.rs
│   │   └── mod.rs
│   ├── git_host/
│   │   ├── mod.rs                   # GitHost trait: check runs, comments, approvals, PR state/diff
│   │   ├── github.rs
│   │   ├── gitlab.rs
│   │   └── codeowners.rs
│   ├── engine.rs                    # Axum server now exposes /health, /hooks, /cache/summary
│   ├── messaging/
│   ├── tui/
│   ├── gateway/
│   ├── mcp/
│   ├── cache/
│   ├── release/
│   └── test_intel/
└── tests/
```

### Target Tree After This Work

```text
neverhuman/jeryu
├── package.json
├── Cargo.toml
├── apps/
│   └── web/
│       ├── index.html
│       ├── package.json
│       ├── vite.config.ts
│       ├── tsconfig.json
│       ├── playwright.config.ts
│       ├── src/
│       │   ├── main.tsx
│       │   ├── app/
│       │   │   ├── App.tsx
│       │   │   ├── router.tsx
│       │   │   ├── providers.tsx
│       │   │   └── layout/AppShell.tsx
│       │   ├── api/
│       │   │   ├── client.ts
│       │   │   ├── generated.ts
│       │   │   └── websocket.ts
│       │   ├── components/
│       │   │   ├── command/CommandPalette.tsx
│       │   │   ├── markdown/MarkdownRenderer.tsx
│       │   │   ├── realtime/ActivityDock.tsx
│       │   │   ├── repo/RepoCard.tsx
│       │   │   ├── repo/FileTree.tsx
│       │   │   ├── review/DiffViewer.tsx
│       │   │   ├── review/MergePassport.tsx
│       │   │   └── settings/SettingsForm.tsx
│       │   ├── routes/
│       │   │   ├── Dashboard.tsx
│       │   │   ├── Repos.tsx
│       │   │   ├── RepoHome.tsx
│       │   │   ├── CodeBrowser.tsx
│       │   │   ├── MergeRequest.tsx
│       │   │   ├── Settings.tsx
│       │   │   └── NotFound.tsx
│       │   ├── state/
│       │   ├── styles/
│       │   └── tests/
│       └── ux-qa-check.mjs
├── db/
│   └── migrations/
│       └── 202606010001_web_forge_core.sql
├── docs/
│   ├── WEB_FORGE_ENGINEERING_SPEC.md
│   ├── WEB_FORGE_API.md
│   └── WEB_FORGE_SECURITY.md
├── src/
│   ├── api/
│   │   ├── repository.rs
│   │   ├── repo_browser.rs
│   │   ├── merge_request.rs
│   │   ├── repo_settings.rs
│   │   └── web_read_model.rs
│   ├── web/
│   │   ├── mod.rs
│   │   ├── state.rs
│   │   ├── router.rs
│   │   ├── error.rs
│   │   ├── auth.rs
│   │   ├── permissions.rs
│   │   ├── markdown.rs
│   │   ├── spa.rs
│   │   ├── ws.rs
│   │   └── api/
│   │       ├── mod.rs
│   │       ├── bootstrap.rs
│   │       ├── repos.rs
│   │       ├── repo_files.rs
│   │       ├── merge_requests.rs
│   │       ├── issues.rs
│   │       ├── settings.rs
│   │       └── actions.rs
│   ├── web_events/
│   │   ├── mod.rs
│   │   ├── protocol.rs
│   │   └── bus.rs
│   ├── repos/
│   │   ├── mod.rs
│   │   ├── service.rs
│   │   ├── permissions.rs
│   │   └── git_store.rs
│   ├── repo_browser/
│   │   ├── mod.rs
│   │   ├── tree.rs
│   │   ├── blob.rs
│   │   ├── blame.rs
│   │   └── markdown.rs
│   ├── merge/
│   │   ├── mod.rs
│   │   ├── service.rs
│   │   ├── reviews.rs
│   │   └── merge_gate.rs
│   └── engine.rs
└── tests/
    ├── web_api_contract.rs
    ├── web_markdown_rendering.rs
    └── web_ws_resume.rs
```

## 3. Product Principles

### 3.1 Faster than GitHub/GitLab

The app must feel instant after first load.

- One global repo switcher with fuzzy search.
- Command palette for every action.
- Persistent layout and selection state.
- Optimistic local navigation with streamed freshness indicators.
- Virtualized trees, tables, diffs, log panes, and activity streams.
- WebSocket-first updates; polling only as fallback.
- Keyboard navigation everywhere.

### 3.2 Less confusing than GitHub/GitLab

The app should answer the user’s real questions directly:

- “What needs attention?”
- “Why can’t this merge?”
- “Who/what is blocking this?”
- “What changed?”
- “Is it safe to approve?”
- “What did the agent do?”
- “What will this setting change break?”
- “What is happening right now across all repos?”

### 3.3 Better real-time processing

The UI must not periodically scrape everything. The backend emits typed events and snapshots. The frontend subscribes to scopes and receives deltas.

### 3.4 Safety over raw power

Every dangerous operation uses the existing preview/risk/grant pattern:

- create repo: preview owner, visibility, template, default branch, initial files;
- change settings: preview old/new diff and affected branch/MR policies;
- approve review: bind to exact head SHA;
- merge: bind to exact head SHA and target branch state;
- delete/archive/transfer repo: typed confirmation, risk tier, audit event;
- secrets: metadata only in UI, never values after write.

### 3.5 Agent-native by default

GitHub/GitLab show checks, comments, and bots as separate noise. JeRyu should fuse them:

- one “Merge Passport” verdict;
- agent evidence linked to exact diffs and commits;
- agent sessions and tasks visible beside human review;
- agent-generated patches reviewable as normal diffs;
- race/winner state visible without leaving the MR.

---

## 4. Target User Experience

## 4.1 Global App Shell

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ JeRyu  ⌘K Search/action...   Repo: neverhuman/jeryu ▾   Live ●  User ▾      │
├───────────────┬──────────────────────────────────────────────┬──────────────┤
│ Left Nav      │ Main Work Area                               │ Live Dock    │
│               │                                              │              │
│ Dashboard     │ route-specific content                       │ Activity     │
│ Repos         │                                              │ Checks       │
│ Merge Room    │                                              │ Agents       │
│ Reviews       │                                              │ Logs         │
│ CI / Runs     │                                              │ Alerts       │
│ Agents        │                                              │              │
│ Settings      │                                              │              │
└───────────────┴──────────────────────────────────────────────┴──────────────┘
```

Core shell behavior:

- `⌘K` / `Ctrl+K`: command palette.
- `/`: focus search in current view.
- `g r`: repositories.
- `g m`: merge room.
- `g s`: settings.
- `[` / `]`: previous/next repo.
- `j/k` or arrows: move selection.
- `Enter`: open/drill down.
- `Esc`: close modal or go up.
- `?`: keyboard shortcut overlay.

## 4.2 All Repositories Dashboard

Purpose: answer “what exists and what is happening?”

Components:

- Repo family groups: `veox-*`, `jeryu-*`, org groups, personal repos, archived repos.
- Repo cards or compact table with:
  - owner/name;
  - visibility;
  - default branch;
  - language mix;
  - open MRs/PRs;
  - failing checks;
  - active agents;
  - latest commit;
  - last activity;
  - risk flags;
  - cache/runner pressure;
  - quick actions.
- Create/import/fork/mirror buttons.
- Saved filters: “blocked merges”, “needs review”, “agent active”, “CI red”, “stale”, “private”, “archived”.
- Live updates: repo card pulses when a run starts, MR updates, setting changes, or agent posts evidence.

## 4.3 Repository Overview

Purpose: replace the confusing GitHub repo home with a clear status surface.

Top strip:

- repo name, visibility, default branch, clone URL, stars/watchers if mirrored from host;
- health posture: green/yellow/red;
- merge posture: safe/blocked/unknown;
- CI posture: passing/failing/running;
- agents: idle/running/blocked;
- cache/runners summary;
- latest release.

Main panels:

- rendered README;
- latest activity;
- open merge requests;
- failing checks;
- current agent tasks;
- recent releases;
- suggested next action.

## 4.4 Code Browser

Purpose: make source browsing and review fast.

Features:

- branch/tag/commit selector;
- sticky breadcrumb;
- fuzzy file finder;
- virtualized tree;
- file preview;
- Markdown rendering;
- syntax highlighting;
- copy permalink;
- raw/download;
- blame/history;
- compare against branch;
- quick create/edit/upload file if permitted;
- command palette actions scoped to file/path.

## 4.5 Markdown Rendering

`README.md` and all Markdown files must render correctly to HTML.

Requirements:

- GitHub-flavored Markdown tables/task lists/strikethrough/autolinks.
- Heading anchors.
- Sanitized HTML.
- Syntax-highlighted fenced code blocks.
- Relative links rewritten to JeRyu routes.
- Relative images resolved via authenticated blob URLs.
- Mermaid optional, disabled by default unless sanitized and sandboxed.
- Frontmatter support for docs pages.
- Same renderer used in README, issue/MR descriptions, comments, docs, release notes, and evidence packs.

Backend should return both raw Markdown and sanitized rendered HTML:

```http
GET /api/repos/{host}/{owner}/{repo}/readme?ref=main
GET /api/repos/{host}/{owner}/{repo}/blob?ref=main&path=README.md&render=html
```

Response:

```json
{
  "path": "README.md",
  "ref": "main",
  "sha": "...",
  "mime": "text/markdown",
  "raw": "# Title\n...",
  "html": "<h1 id=\"title\">Title</h1>...",
  "toc": [{ "depth": 1, "id": "title", "text": "Title" }],
  "links": [{ "href": "docs/setup.md", "resolved_route": "/repos/.../blob/docs/setup.md" }],
  "rendered_at": "...",
  "renderer_version": "jeryu-markdown.v1"
}
```

## 4.6 Merge Room

Purpose: a better PR/MR review experience.

Layout:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ MR #42: title   head abc123   target main   Merge Passport: BLOCKED         │
├─────────────┬───────────────────────────────────────────────┬───────────────┤
│ Files       │ Diff / File Viewer                            │ Review Panel  │
│             │                                               │               │
│ changed     │ unified/split diff                            │ checks        │
│ filters     │ inline comments                               │ agents        │
│ ownership   │ syntax highlighted                            │ blockers      │
│ risk badges │ virtualized                                   │ actions       │
└─────────────┴───────────────────────────────────────────────┴───────────────┘
```

High-value controls:

- “Why blocked?” explanation.
- “Approve exact SHA” button.
- “Request changes.”
- “Merge when green.”
- “Run missing checks.”
- “Ask agent to fix failing test.”
- “Open failing log at relevant line.”
- “Review only files owned by me.”
- “Hide generated/vendor files.”
- “Show agent evidence.”
- “Compare since my last review.”
- “Resolve all fixed threads.”

## 4.7 Settings Experience

Settings must be searchable, grouped, previewable, and safe.

Categories:

1. General
   - name;
   - description;
   - homepage;
   - topics/tags;
   - visibility;
   - default branch;
   - repository family;
   - archive/unarchive;
   - transfer;
   - delete.

2. Features
   - issues;
   - merge requests / pull requests;
   - wiki;
   - discussions;
   - projects;
   - packages;
   - releases;
   - actions/CI;
   - security advisories;
   - pages/docs.

3. Merge policy
   - merge commit;
   - squash merge;
   - rebase merge;
   - auto-merge;
   - delete head branch;
   - require linear history;
   - required approvals;
   - stale approval dismissal;
   - CODEOWNERS;
   - exact-SHA approval;
   - JeRyu Merge Passport required gate.

4. Branch protection
   - protected branch patterns;
   - required checks;
   - required deployments;
   - signed commits;
   - force-push policy;
   - deletion policy;
   - bypass actors;
   - conversation resolution;
   - status check freshness.

5. CI / runners
   - default runner pool;
   - runner labels;
   - concurrency caps;
   - queue policy;
   - cache policy;
   - artifact retention;
   - log retention;
   - failure capsule retention;
   - VTI policy.

6. Agents
   - allowed agents;
   - autonomous coding enabled;
   - branch naming policy;
   - max concurrent sessions;
   - max spend/budget;
   - approval requirements;
   - patch proposal policy;
   - tool access;
   - model/provider routing;
   - evidence requirements.

7. Access
   - collaborators;
   - teams;
   - roles;
   - deploy keys;
   - app installations;
   - fine-grained tokens;
   - audit export.

8. Secrets and variables
   - environment variables;
   - encrypted secrets metadata;
   - scopes;
   - rotation age;
   - access logs;
   - denied access events.

9. Webhooks and integrations
   - outgoing webhooks;
   - incoming host webhooks;
   - Slack/Discord/email;
   - MCP servers;
   - external issue trackers;
   - artifact stores.

10. Security
    - secret scanning;
    - dependency scanning;
    - SAST/DAST links;
    - allowed licenses;
    - branch policy drift;
    - agent sandboxing.

11. Notifications
    - personal watch settings;
    - repo events;
    - merge requests;
    - CI failures;
    - agent completions;
    - release events.

12. Retention and export
    - logs;
    - artifacts;
    - evidence;
    - comments;
    - audit;
    - backups;
    - export bundle.

Every settings change must show:

- current value;
- proposed value;
- blast radius;
- validation errors;
- affected branches/MRs/jobs;
- whether the change is reversible;
- audit receipt;
- required permission/grant.

---

## 5. Target Architecture

```
apps/web/                  Vite + React + TypeScript SPA
      │
      │ REST + WebSocket + generated types
      ▼
src/web/                   Rust web BFF: auth, REST, WS, static assets
      │
      ├── src/api/          existing typed read/action/event contracts
      ├── src/repos/        repository lifecycle + settings domain
      ├── src/repo_browser/ tree/blob/markdown/diff domain
      ├── src/merge/        review/approval/merge domain
      ├── src/git_host/     GitHub/GitLab host adapters
      ├── src/db/           persistence/cache/audit
      ├── src/tui/          existing TUI uses same read model/actions
      └── src/mcp/          MCP integrations and external tools
```

Important boundary: the browser never calls GitHub/GitLab directly. It calls JeRyu. JeRyu enforces permissions, preview, audit, exact-SHA safety, markdown sanitization, and event emission.

---

## 6. Detailed Code-Change Diff

This section is intentionally path-by-path. It is not a literal unified diff, because the change is larger than one patch, but it is written so engineers can implement it directly.

## 6.1 Root workspace and scripts

### MODIFY `package.json`

Replace the current UX-QA-only scripts with full app scripts while preserving QA gates.

```diff
 {
   "name": "jeryu-workspace",
   "private": true,
   "workspaces": [
     "apps/web"
   ],
   "scripts": {
+    "dev": "npm --workspace @jeryu/web run dev",
+    "build": "npm --workspace @jeryu/web run build",
+    "preview": "npm --workspace @jeryu/web run preview",
+    "typecheck": "npm --workspace @jeryu/web run typecheck",
+    "lint": "npm --workspace @jeryu/web run lint",
+    "test": "npm --workspace @jeryu/web run test",
+    "test:e2e": "npm --workspace @jeryu/web run test:e2e",
+    "storybook": "npm --workspace @jeryu/web run storybook",
+    "build-storybook": "npm --workspace @jeryu/web run build-storybook",
     "ux-qa": "npm --workspace apps/web run build && npm --workspace apps/web run test",
     "ux-qa:build": "npm --workspace apps/web run build",
     "ux-qa:test": "npm --workspace apps/web run test"
   }
 }
```

If keeping the package name `@jankurai/ux-qa` is important for existing CI, use npm aliases or create a second workspace `apps/ux-qa`. Preferred final state:

```json
"workspaces": ["apps/web", "apps/ux-qa"]
```

### MODIFY `Cargo.toml`

Add web/BFF dependencies.

```diff
 axum = { version = "0.8", features = ["json"] }
+tower = "0.5"
 tower-http = { version = "0.6", features = ["cors", "trace"] }
+axum-extra = { version = "0.10", features = ["typed-header", "cookie"] }
+tokio-stream = "0.1"
+headers = "0.4"
+mime_guess = "2"
+pulldown-cmark = { version = "0.13", default-features = false, features = ["html"] }
+ammonia = "4"
+comrak = { version = "0.37", default-features = false, optional = true }
+utoipa = { version = "5", features = ["chrono", "uuid", "axum_extras"] }
+utoipa-swagger-ui = { version = "9", features = ["axum"] }
+schemars = { version = "1", features = ["chrono", "uuid1"] }
+bytes = "1"
+async-stream = "0.3"
+parking_lot = "0.12"
+url = { version = "2", optional = false }
```

Update `tower-http` features:

```diff
-tower-http = { version = "0.6", features = ["cors", "trace"] }
+tower-http = { version = "0.6", features = ["cors", "trace", "fs", "compression-gzip", "compression-br", "set-header", "request-id", "timeout"] }
```

Add feature flag:

```diff
 [features]
 default = ["profile-sqlite-kafka", "demo-fixtures"]
+web = []
```

## 6.2 API module hygiene

### MODIFY `src/api/mod.rs`

Current exports should be verified. If files are missing, add them or remove stale exports. Preferred approach: add missing modules as real contracts because the web app needs them.

```diff
 pub mod actions;
 pub mod agent_session;
 pub mod capacity;
 pub mod dashboards;
 pub mod entity;
 pub mod events;
 pub mod freshness;
 pub mod inspection;
 pub mod proof;
 pub mod read_model;
+pub mod repository;
+pub mod repo_browser;
+pub mod merge_request;
+pub mod review;
+pub mod settings;
+pub mod web_read_model;
 pub mod runtime_profile;
 pub mod snapshot;
```

### ADD `src/api/repository.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::entity::{ActionRef, EntityRef, HealthLevel};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryHostKind {
    GitHub,
    GitLab,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryId {
    pub host: String,
    pub owner: String,
    pub name: String,
}

impl RepositoryId {
    pub fn slug(&self) -> String {
        format!("{}/{}/{}", self.host, self.owner, self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryVisibility {
    Public,
    Internal,
    Private,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositorySummary {
    pub id: RepositoryId,
    pub entity: EntityRef,
    pub description: Option<String>,
    pub visibility: RepositoryVisibility,
    pub default_branch: String,
    pub family: Option<String>,
    pub topics: Vec<String>,
    pub language: Option<String>,
    pub health: HealthLevel,
    pub open_merge_requests: u32,
    pub failing_checks: u32,
    pub running_jobs: u32,
    pub active_agents: u32,
    pub blocked_agents: u32,
    pub updated_at: DateTime<Utc>,
    pub clone_http_url: Option<String>,
    pub clone_ssh_url: Option<String>,
    pub available_actions: Vec<ActionRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryListResponse {
    pub generated_at: DateTime<Utc>,
    pub total: u64,
    pub repositories: Vec<RepositorySummary>,
    pub facets: RepositoryFacets,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepositoryFacets {
    pub hosts: Vec<String>,
    pub owners: Vec<String>,
    pub families: Vec<String>,
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRepositoryRequest {
    pub host: String,
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub visibility: RepositoryVisibility,
    pub initialize_readme: bool,
    pub gitignore_template: Option<String>,
    pub license_template: Option<String>,
    pub default_branch: Option<String>,
    pub topics: Vec<String>,
    pub family: Option<String>,
    pub template: Option<RepositoryId>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRepositoryPreview {
    pub normalized_name: String,
    pub target_owner: String,
    pub visibility: RepositoryVisibility,
    pub initial_files: Vec<String>,
    pub settings_to_apply: Vec<String>,
    pub side_effects: Vec<String>,
    pub warnings: Vec<String>,
}
```

### ADD `src/api/repo_browser.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::repository::RepositoryId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefSelectorItem {
    pub name: String,
    pub sha: String,
    pub kind: RefKind,
    pub protected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    Branch,
    Tag,
    Commit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEntry {
    pub path: String,
    pub name: String,
    pub kind: TreeEntryKind,
    pub sha: String,
    pub size_bytes: Option<u64>,
    pub last_commit_sha: Option<String>,
    pub last_commit_message: Option<String>,
    pub last_commit_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeEntryKind {
    File,
    Directory,
    Symlink,
    Submodule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobResponse {
    pub repo: RepositoryId,
    pub path: String,
    pub ref_name: String,
    pub sha: String,
    pub size_bytes: u64,
    pub mime: String,
    pub encoding: BlobEncoding,
    pub text: Option<String>,
    pub base64: Option<String>,
    pub rendered_markdown: Option<RenderedMarkdown>,
    pub is_binary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlobEncoding {
    Utf8,
    Base64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedMarkdown {
    pub html: String,
    pub toc: Vec<MarkdownHeading>,
    pub links: Vec<MarkdownLink>,
    pub renderer_version: String,
    pub rendered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownHeading {
    pub depth: u8,
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownLink {
    pub href: String,
    pub resolved_route: Option<String>,
    pub external: bool,
}
```

### ADD `src/api/merge_request.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::entity::{ActionRef, EntityRef, HealthLevel};
use super::repository::RepositoryId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeRequestSummary {
    pub repo: RepositoryId,
    pub iid: String,
    pub entity: EntityRef,
    pub title: String,
    pub author: String,
    pub source_branch: String,
    pub target_branch: String,
    pub head_sha: String,
    pub base_sha: String,
    pub state: MergeRequestState,
    pub draft: bool,
    pub mergeable: Mergeability,
    pub review: ReviewPosture,
    pub checks: CheckPosture,
    pub agents: AgentPosture,
    pub labels: Vec<String>,
    pub updated_at: DateTime<Utc>,
    pub available_actions: Vec<ActionRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeRequestState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mergeability {
    pub level: HealthLevel,
    pub can_merge: bool,
    pub reason: Option<String>,
    pub exact_head_sha: String,
    pub required_gate: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewPosture {
    pub required_approvals: u32,
    pub approvals: u32,
    pub changes_requested: u32,
    pub unresolved_threads: u32,
    pub user_review_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckPosture {
    pub total: u32,
    pub passing: u32,
    pub failing: u32,
    pub pending: u32,
    pub skipped: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPosture {
    pub active_sessions: u32,
    pub proposed_patches: u32,
    pub evidence_packets: u32,
    pub blockers: u32,
}
```

### ADD `src/api/settings.rs`

Define a full typed settings model. Do not use arbitrary `serde_json::Value` except for host-specific extension blocks.

```rust
use serde::{Deserialize, Serialize};

use super::repository::{RepositoryId, RepositoryVisibility};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositorySettings {
    pub repo: RepositoryId,
    pub general: GeneralSettings,
    pub features: FeatureSettings,
    pub merge: MergeSettings,
    pub branch_protection: Vec<BranchProtectionRule>,
    pub ci: CiSettings,
    pub agents: AgentSettings,
    pub access: AccessSettings,
    pub security: SecuritySettings,
    pub notifications: NotificationSettings,
    pub retention: RetentionSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    pub name: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub visibility: RepositoryVisibility,
    pub default_branch: String,
    pub topics: Vec<String>,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureSettings {
    pub issues: bool,
    pub merge_requests: bool,
    pub wiki: bool,
    pub discussions: bool,
    pub projects: bool,
    pub packages: bool,
    pub releases: bool,
    pub ci: bool,
    pub security_advisories: bool,
    pub pages: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeSettings {
    pub allow_merge_commit: bool,
    pub allow_squash_merge: bool,
    pub allow_rebase_merge: bool,
    pub allow_auto_merge: bool,
    pub delete_branch_on_merge: bool,
    pub require_linear_history: bool,
    pub required_approvals: u32,
    pub dismiss_stale_approvals: bool,
    pub require_codeowners: bool,
    pub require_exact_sha_approval: bool,
    pub require_jeryu_merge_passport: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchProtectionRule {
    pub pattern: String,
    pub required_checks: Vec<String>,
    pub required_approvals: u32,
    pub require_signed_commits: bool,
    pub require_conversation_resolution: bool,
    pub allow_force_pushes: bool,
    pub allow_deletions: bool,
    pub bypass_actors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiSettings {
    pub default_runner_pool: Option<String>,
    pub concurrency_limit: Option<u32>,
    pub artifact_retention_days: u32,
    pub log_retention_days: u32,
    pub cache_retention_days: u32,
    pub vti_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    pub autonomous_coding_enabled: bool,
    pub max_concurrent_sessions: u32,
    pub require_human_approval_for_writes: bool,
    pub allowed_agents: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub evidence_required: bool,
    pub budget_daily_usd: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessSettings {
    pub collaborators_count: u32,
    pub teams_count: u32,
    pub deploy_keys_count: u32,
    pub app_installations_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritySettings {
    pub secret_scanning: bool,
    pub dependency_scanning: bool,
    pub license_policy_enabled: bool,
    pub agent_sandbox_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub watch_default: String,
    pub notify_on_ci_failure: bool,
    pub notify_on_agent_completion: bool,
    pub notify_on_release: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionSettings {
    pub audit_days: u32,
    pub evidence_days: u32,
    pub workflow_run_days: u32,
    pub log_days: u32,
}
```

### ADD `src/api/web_read_model.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::read_model::TuiReadModel;
use super::repository::RepositorySummary;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebBootstrap {
    pub generated_at: DateTime<Utc>,
    pub schema_version: String,
    pub viewer: Viewer,
    pub tui: TuiReadModel,
    pub recent_repositories: Vec<RepositorySummary>,
    pub websocket_url: String,
    pub feature_flags: WebFeatureFlags,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Viewer {
    pub id: String,
    pub login: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub global_permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFeatureFlags {
    pub repo_create: bool,
    pub settings_write: bool,
    pub merge_write: bool,
    pub markdown_html: bool,
    pub agents: bool,
    pub mcp: bool,
}
```

## 6.3 Rust web backend

### ADD `src/web/mod.rs`

```rust
pub mod auth;
pub mod error;
pub mod extractors;
pub mod rest;
pub mod router;
pub mod state;
pub mod static_assets;
pub mod ws;
```

### ADD `src/web/state.rs`

```rust
use std::sync::Arc;

use crate::web_events::bus::WebEventBus;

#[derive(Clone)]
pub struct WebState {
    pub app_name: String,
    pub event_bus: Arc<WebEventBus>,
    pub repo_service: Arc<crate::repos::RepoService>,
    pub browser_service: Arc<crate::repo_browser::RepoBrowserService>,
    pub merge_service: Arc<crate::merge::MergeService>,
    pub settings_service: Arc<crate::repos::settings::SettingsService>,
    pub action_service: Arc<crate::actions_runtime::ActionService>,
}
```

If `actions_runtime` does not exist, add it as the execution layer that wraps the existing `api::actions` contracts and existing TUI action registry.

### ADD `src/web/router.rs`

```rust
use axum::{routing::{get, post, patch}, Router};
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};

use super::state::WebState;

pub fn router(state: WebState) -> Router {
    Router::new()
        .route("/api/bootstrap", get(super::rest::bootstrap::get_bootstrap))
        .route("/api/repos", get(super::rest::repos::list_repos).post(super::rest::repos::create_repo))
        .route("/api/repos/{host}/{owner}/{repo}", get(super::rest::repos::get_repo))
        .route("/api/repos/{host}/{owner}/{repo}/settings", get(super::rest::settings::get_settings).patch(super::rest::settings::patch_settings))
        .route("/api/repos/{host}/{owner}/{repo}/refs", get(super::rest::repo_browser::list_refs))
        .route("/api/repos/{host}/{owner}/{repo}/tree", get(super::rest::repo_browser::get_tree))
        .route("/api/repos/{host}/{owner}/{repo}/blob", get(super::rest::repo_browser::get_blob))
        .route("/api/repos/{host}/{owner}/{repo}/readme", get(super::rest::repo_browser::get_readme))
        .route("/api/repos/{host}/{owner}/{repo}/compare", get(super::rest::repo_browser::compare_refs))
        .route("/api/repos/{host}/{owner}/{repo}/merge-requests", get(super::rest::merge_requests::list_merge_requests).post(super::rest::merge_requests::create_merge_request))
        .route("/api/repos/{host}/{owner}/{repo}/merge-requests/{iid}", get(super::rest::merge_requests::get_merge_request))
        .route("/api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/approve", post(super::rest::merge_requests::approve_merge_request))
        .route("/api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/merge", post(super::rest::merge_requests::merge_merge_request))
        .route("/api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/reviews", get(super::rest::reviews::list_reviews).post(super::rest::reviews::submit_review))
        .route("/api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/threads", get(super::rest::reviews::list_threads).post(super::rest::reviews::create_thread))
        .route("/api/repos/{host}/{owner}/{repo}/runs", get(super::rest::ci::list_runs))
        .route("/api/repos/{host}/{owner}/{repo}/checks", get(super::rest::ci::list_checks))
        .route("/api/activity", get(super::rest::activity::list_activity))
        .route("/api/search", get(super::rest::search::search))
        .route("/api/actions/{action_id}/preview", post(super::rest::actions::preview_action))
        .route("/api/actions/{action_id}/execute", post(super::rest::actions::execute_action))
        .route("/api/ws", get(super::ws::ws_handler))
        .fallback_service(super::static_assets::spa_service())
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
```

### ADD `src/web/error.rs`

```rust
use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    pub request_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("upstream: {0}")]
    Upstream(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ApiError::Upstream(_) => (StatusCode::BAD_GATEWAY, "upstream"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        let body = ApiErrorBody {
            code: code.to_string(),
            message: self.to_string(),
            request_id: None,
        };
        (status, Json(body)).into_response()
    }
}
```

### ADD `src/web/ws.rs`

```rust
use axum::{extract::{State, WebSocketUpgrade}, response::IntoResponse};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};

use super::state::WebState;
use crate::web_events::protocol::{ClientWsMessage, ServerWsMessage};

pub async fn ws_handler(
    State(state): State<WebState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, socket))
}

async fn handle_socket(state: WebState, socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();
    let mut subscription = state.event_bus.subscribe_all();

    let hello = ServerWsMessage::hello(state.event_bus.current_seq()).unwrap_json();
    if sender.send(Message::Text(hello.into())).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            maybe_msg = receiver.next() => {
                match maybe_msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(msg) = serde_json::from_str::<ClientWsMessage>(&text) {
                            state.event_bus.apply_client_message(msg).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(_) => {}
                }
            }
            maybe_event = subscription.recv() => {
                match maybe_event {
                    Ok(event) => {
                        let text = ServerWsMessage::event(event).unwrap_json();
                        if sender.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }
}
```

## 6.4 Web event bus

### ADD `src/web_events/mod.rs`

```rust
pub mod bus;
pub mod protocol;
pub mod projection;
pub mod subscription;
```

### ADD `src/web_events/protocol.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::events::TuiEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientWsMessage {
    Hello { resume_from: Option<u64>, subscriptions: Vec<SubscriptionSpec> },
    Subscribe { subscriptions: Vec<SubscriptionSpec> },
    Unsubscribe { scopes: Vec<String> },
    Ack { seq: u64 },
    Ping { nonce: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionSpec {
    pub scope: String,
    pub filters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerWsMessage {
    Hello { server_time: DateTime<Utc>, current_seq: u64, protocol: String },
    SnapshotRequired { reason: String, current_seq: u64 },
    Event { event: WebEvent },
    Pong { nonce: String, server_time: DateTime<Utc> },
    Error { code: String, message: String },
}

impl ServerWsMessage {
    pub fn hello(current_seq: u64) -> Self {
        Self::Hello {
            server_time: Utc::now(),
            current_seq,
            protocol: "jeryu.ws.v1".to_string(),
        }
    }

    pub fn event(event: WebEvent) -> Self {
        Self::Event { event }
    }

    pub fn unwrap_json(&self) -> String {
        serde_json::to_string(self).expect("serialize ws message")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebEvent {
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
    pub scope: String,
    pub kind: String,
    pub entity: String,
    pub summary: String,
    pub payload: Value,
}

impl From<TuiEvent> for WebEvent {
    fn from(event: TuiEvent) -> Self {
        let scope = format!("{}:{}", event.entity.kind.label(), event.entity.id);
        Self {
            seq: event.seq,
            timestamp: event.timestamp,
            scope,
            kind: event.kind.label().to_string(),
            entity: event.entity.display(),
            summary: event.summary,
            payload: serde_json::json!({
                "severity": event.severity,
                "parent": event.parent,
                "correlation_id": event.correlation_id,
                "evidence_refs": event.evidence_refs,
                "next_actions": event.next_actions,
                "stale_after_ms": event.stale_after_ms
            }),
        }
    }
}
```

### ADD `src/web_events/bus.rs`

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;

use super::protocol::{ClientWsMessage, WebEvent};

pub struct WebEventBus {
    seq: AtomicU64,
    tx: broadcast::Sender<WebEvent>,
}

impl WebEventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { seq: AtomicU64::new(0), tx }
    }

    pub fn current_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }

    pub fn subscribe_all(&self) -> broadcast::Receiver<WebEvent> {
        self.tx.subscribe()
    }

    pub fn publish(&self, mut event: WebEvent) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        event.seq = seq;
        let _ = self.tx.send(event);
    }

    pub async fn apply_client_message(&self, _msg: ClientWsMessage) {
        // Per-connection subscription state belongs in ws.rs. This hook remains
        // for server-wide ack metrics and resume bookkeeping.
    }
}
```

## 6.5 Repository domain

### ADD `src/repos/mod.rs`

```rust
pub mod create;
pub mod host_sync;
pub mod models;
pub mod permissions;
pub mod service;
pub mod settings;

pub use service::RepoService;
```

### ADD `src/repos/service.rs`

Responsibilities:

- list all repositories across configured hosts;
- group repo families;
- enrich with health, CI, agent, and merge posture;
- create repositories via `GitHost` adapters;
- cache host metadata;
- publish `repo.created`, `repo.updated`, `repo.settings.changed` web events;
- enforce permissions.

```rust
use anyhow::Result;

use crate::api::repository::{CreateRepositoryPreview, CreateRepositoryRequest, RepositoryListResponse, RepositorySummary};

#[derive(Clone)]
pub struct RepoService {
    // db, host registry, event bus, permission service
}

impl RepoService {
    pub async fn list(&self, query: RepoListQuery) -> Result<RepositoryListResponse> {
        todo!("load from local DB cache, refresh stale host pages, enrich with health")
    }

    pub async fn preview_create(&self, req: &CreateRepositoryRequest) -> Result<CreateRepositoryPreview> {
        todo!("validate owner/name/visibility/template/default branch")
    }

    pub async fn create(&self, req: CreateRepositoryRequest) -> Result<RepositorySummary> {
        todo!("action preview already accepted; call host adapter; persist; emit event")
    }
}

#[derive(Debug, Clone)]
pub struct RepoListQuery {
    pub host: Option<String>,
    pub owner: Option<String>,
    pub family: Option<String>,
    pub search: Option<String>,
    pub include_archived: bool,
    pub limit: u32,
    pub cursor: Option<String>,
}
```

## 6.6 Repository browser domain

### ADD `src/repo_browser/mod.rs`

```rust
pub mod blob;
pub mod diff;
pub mod git_tree;
pub mod markdown;
pub mod service;

pub use service::RepoBrowserService;
```

### ADD `src/repo_browser/markdown.rs`

Renderer requirements:

- use a deterministic renderer version;
- sanitize every HTML output;
- rewrite relative links;
- collect heading TOC;
- never inline remote images through the server unless explicitly proxied and allowed;
- cache by `(repo_id, ref_sha, path, blob_sha, renderer_version)`.

```rust
use ammonia::Builder;
use pulldown_cmark::{html, Options, Parser};

use crate::api::repo_browser::{MarkdownHeading, MarkdownLink, RenderedMarkdown};

pub const RENDERER_VERSION: &str = "jeryu-markdown.v1";

pub fn render_markdown(markdown: &str, base_route: &str) -> RenderedMarkdown {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);

    let parser = Parser::new_ext(markdown, options);
    let mut raw_html = String::new();
    html::push_html(&mut raw_html, parser);

    let clean_html = Builder::default()
        .add_tags(["table", "thead", "tbody", "tr", "th", "td", "input"])
        .add_tag_attributes("a", ["href", "title", "rel", "target"])
        .add_tag_attributes("img", ["src", "alt", "title", "width", "height"])
        .clean(&raw_html)
        .to_string();

    RenderedMarkdown {
        html: rewrite_relative_links(&clean_html, base_route),
        toc: extract_headings(markdown),
        links: extract_links(markdown, base_route),
        renderer_version: RENDERER_VERSION.to_string(),
        rendered_at: chrono::Utc::now(),
    }
}

fn rewrite_relative_links(html: &str, _base_route: &str) -> String {
    // Implement with scraper/html5ever in the production patch. Placeholder
    // exists to keep the rendering pipeline explicit.
    html.to_string()
}

fn extract_headings(_markdown: &str) -> Vec<MarkdownHeading> {
    Vec::new()
}

fn extract_links(_markdown: &str, _base_route: &str) -> Vec<MarkdownLink> {
    Vec::new()
}
```

## 6.7 Merge/review domain

### ADD `src/merge/mod.rs`

```rust
pub mod guards;
pub mod reviews;
pub mod service;

pub use service::MergeService;
```

### ADD `src/merge/service.rs`

```rust
use anyhow::Result;

use crate::api::merge_request::MergeRequestSummary;
use crate::api::repository::RepositoryId;

#[derive(Clone)]
pub struct MergeService {
    // db, host registry, action preview service, event bus
}

impl MergeService {
    pub async fn list(&self, repo: RepositoryId) -> Result<Vec<MergeRequestSummary>> {
        todo!("load host MRs, enrich checks/reviews/agents/evidence")
    }

    pub async fn approve_exact_sha(&self, repo: RepositoryId, iid: String, expected_head_sha: String) -> Result<()> {
        // 1. refetch live MR state
        // 2. compare live head to expected_head_sha
        // 3. reject on mismatch
        // 4. preview action risk/grant
        // 5. call host approve_mr with exact SHA
        // 6. write audit event
        // 7. publish websocket event
        todo!()
    }

    pub async fn merge_exact_sha(&self, repo: RepositoryId, iid: String, expected_head_sha: String) -> Result<()> {
        // Same exact-SHA safety. Also verify merge gates immediately before write.
        todo!()
    }
}
```

## 6.8 Git host trait expansion

### MODIFY `src/git_host/mod.rs`

Add repository, browser, settings, review, and CI surfaces to the `GitHost` trait. Keep default implementations returning `HostError::NotImplemented` during phased rollout, except methods required by production web endpoints should be implemented for GitHub and GitLab before release.

```diff
 pub trait GitHost: Send + Sync {
     fn id(&self) -> &str;
     async fn ping_user(&self) -> Result<HostIdentity, HostError>;
+
+    async fn list_repositories(&self, owner: Option<&str>, page: Page) -> Result<PageResult<HostRepository>, HostError> {
+        Err(HostError::NotImplemented)
+    }
+
+    async fn create_repository(&self, input: CreateHostRepository<'_>) -> Result<HostRepository, HostError> {
+        Err(HostError::NotImplemented)
+    }
+
+    async fn get_repository(&self, repo: &RepoRef) -> Result<HostRepository, HostError> {
+        Err(HostError::NotImplemented)
+    }
+
+    async fn update_repository_settings(&self, repo: &RepoRef, patch: HostRepositorySettingsPatch<'_>) -> Result<HostRepository, HostError> {
+        Err(HostError::NotImplemented)
+    }
+
+    async fn list_refs(&self, repo: &RepoRef) -> Result<Vec<HostRef>, HostError> {
+        Err(HostError::NotImplemented)
+    }
+
+    async fn list_tree(&self, repo: &RepoRef, ref_name: &str, path: &str) -> Result<Vec<HostTreeEntry>, HostError> {
+        Err(HostError::NotImplemented)
+    }
+
+    async fn get_blob(&self, repo: &RepoRef, ref_name: &str, path: &str) -> Result<HostBlob, HostError> {
+        Err(HostError::NotImplemented)
+    }
+
+    async fn get_readme(&self, repo: &RepoRef, ref_name: Option<&str>) -> Result<HostBlob, HostError> {
+        Err(HostError::NotImplemented)
+    }
+
+    async fn list_review_threads(&self, repo: &RepoRef, mr_iid: &str) -> Result<Vec<HostReviewThread>, HostError> {
+        Err(HostError::NotImplemented)
+    }
+
+    async fn create_review_comment(&self, input: HostReviewCommentInput<'_>) -> Result<HostReviewComment, HostError> {
+        Err(HostError::NotImplemented)
+    }
+
+    async fn submit_review(&self, input: HostSubmitReviewInput<'_>) -> Result<HostReview, HostError> {
+        Err(HostError::NotImplemented)
+    }
+
+    async fn merge_mr(&self, input: HostMergeInput<'_>) -> Result<HostMergeResult, HostError> {
+        Err(HostError::NotImplemented)
+    }
 }
```

### ADD host model structs to `src/git_host/mod.rs`

```rust
#[derive(Debug, Clone)]
pub struct Page {
    pub per_page: u32,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PageResult<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub total: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct HostRepository {
    pub repo: RepoRef,
    pub id: String,
    pub description: Option<String>,
    pub visibility: String,
    pub default_branch: String,
    pub archived: bool,
    pub topics: Vec<String>,
    pub clone_http_url: Option<String>,
    pub clone_ssh_url: Option<String>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct CreateHostRepository<'a> {
    pub owner: &'a str,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub visibility: &'a str,
    pub auto_init: bool,
    pub gitignore_template: Option<&'a str>,
    pub license_template: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct HostRepositorySettingsPatch<'a> {
    pub description: Option<Option<&'a str>>,
    pub homepage: Option<Option<&'a str>>,
    pub visibility: Option<&'a str>,
    pub default_branch: Option<&'a str>,
    pub allow_merge_commit: Option<bool>,
    pub allow_squash_merge: Option<bool>,
    pub allow_rebase_merge: Option<bool>,
    pub allow_auto_merge: Option<bool>,
    pub delete_branch_on_merge: Option<bool>,
    pub archived: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct HostRef {
    pub name: String,
    pub sha: String,
    pub kind: String,
    pub protected: bool,
}

#[derive(Debug, Clone)]
pub struct HostTreeEntry {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub sha: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct HostBlob {
    pub path: String,
    pub sha: String,
    pub size_bytes: u64,
    pub bytes: Vec<u8>,
    pub is_binary: bool,
}

#[derive(Debug, Clone)]
pub struct HostReviewThread {
    pub id: String,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub resolved: bool,
    pub comments: Vec<HostReviewComment>,
}

#[derive(Debug, Clone)]
pub struct HostReviewComment {
    pub id: String,
    pub author: String,
    pub body: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct HostReviewCommentInput<'a> {
    pub repo: &'a RepoRef,
    pub mr_iid: &'a str,
    pub body: &'a str,
    pub path: &'a str,
    pub line: u32,
    pub side: &'a str,
}

#[derive(Debug, Clone)]
pub struct HostSubmitReviewInput<'a> {
    pub repo: &'a RepoRef,
    pub mr_iid: &'a str,
    pub head_sha: &'a str,
    pub event: &'a str, // approve | request_changes | comment
    pub body: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct HostReview {
    pub id: String,
    pub state: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HostMergeInput<'a> {
    pub repo: &'a RepoRef,
    pub mr_iid: &'a str,
    pub expected_head_sha: &'a str,
    pub method: &'a str,
    pub commit_title: Option<&'a str>,
    pub commit_message: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct HostMergeResult {
    pub merged: bool,
    pub sha: Option<String>,
    pub url: Option<String>,
}
```

## 6.9 GitHub adapter expansion

### MODIFY `src/git_host/github.rs`

Add implementations:

- `list_repositories`
  - `GET /user/repos` for viewer scope;
  - `GET /orgs/{org}/repos` when owner is org;
  - map permissions if included;
  - paginate.

- `create_repository`
  - `POST /user/repos` for user owner;
  - `POST /orgs/{org}/repos` for org owner;
  - require `dry_run=false` at action layer before calling;
  - do not create from web handler directly.

- `get_repository`
  - `GET /repos/{owner}/{repo}`.

- `update_repository_settings`
  - `PATCH /repos/{owner}/{repo}`;
  - only send fields present in patch.

- `list_refs`
  - `GET /repos/{owner}/{repo}/branches`;
  - `GET /repos/{owner}/{repo}/tags`.

- `list_tree`
  - `GET /repos/{owner}/{repo}/contents/{path}?ref=...` for simple browsing;
  - optionally use Git Trees API for recursive mode.

- `get_blob`
  - `GET /repos/{owner}/{repo}/contents/{path}?ref=...`;
  - decode base64;
  - detect binary.

- `get_readme`
  - `GET /repos/{owner}/{repo}/readme?ref=...`;
  - decode base64.

- `list_review_threads`
  - GraphQL review threads for resolved state;
  - REST fallback for comments.

- `create_review_comment`
  - `POST /repos/{owner}/{repo}/pulls/{pull_number}/comments`.

- `submit_review`
  - `POST /repos/{owner}/{repo}/pulls/{pull_number}/reviews`.

- `merge_mr`
  - `PUT /repos/{owner}/{repo}/pulls/{pull_number}/merge` with expected SHA.

Add tests with a mock HTTP server for:

- base URL override;
- request body fields;
- no writes on preview;
- exact-SHA merge rejection on mismatch;
- readme base64 decode;
- binary detection;
- settings patch sends only changed fields.

## 6.10 GitLab adapter expansion

### MODIFY `src/git_host/gitlab.rs`

Map equivalent operations:

- projects list/create/update;
- repository tree/blob/raw;
- README lookup by common names;
- merge requests list/create/get;
- approvals;
- discussions/notes;
- merge with SHA when supported;
- branch protection;
- variables/secrets metadata;
- pipelines/jobs/logs.

GitLab path encoding must be centralized:

```rust
fn encode_project_path(repo: &RepoRef) -> String {
    urlencoding::encode(&format!("{}/{}", repo.owner, repo.name)).to_string()
}
```

## 6.11 Database migrations

### ADD `db/migrations/202606010001_web_forge_core.sql`

```sql
CREATE TABLE IF NOT EXISTS vcs_hosts (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  base_url TEXT NOT NULL,
  display_name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS repositories (
  id TEXT PRIMARY KEY,
  host_id TEXT NOT NULL REFERENCES vcs_hosts(id),
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  visibility TEXT NOT NULL,
  default_branch TEXT NOT NULL,
  family TEXT,
  topics_json TEXT NOT NULL DEFAULT '[]',
  archived INTEGER NOT NULL DEFAULT 0,
  clone_http_url TEXT,
  clone_ssh_url TEXT,
  host_updated_at TEXT,
  synced_at TEXT NOT NULL,
  UNIQUE(host_id, owner, name)
);

CREATE TABLE IF NOT EXISTS repository_settings_cache (
  repository_id TEXT PRIMARY KEY REFERENCES repositories(id),
  settings_json TEXT NOT NULL,
  settings_hash TEXT NOT NULL,
  synced_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS branch_protection_rules (
  id TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id),
  pattern TEXT NOT NULL,
  rule_json TEXT NOT NULL,
  synced_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS merge_requests (
  id TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id),
  iid TEXT NOT NULL,
  title TEXT NOT NULL,
  author TEXT NOT NULL,
  source_branch TEXT NOT NULL,
  target_branch TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  base_sha TEXT,
  state TEXT NOT NULL,
  draft INTEGER NOT NULL DEFAULT 0,
  labels_json TEXT NOT NULL DEFAULT '[]',
  host_updated_at TEXT,
  synced_at TEXT NOT NULL,
  UNIQUE(repository_id, iid)
);

CREATE TABLE IF NOT EXISTS review_threads (
  id TEXT PRIMARY KEY,
  merge_request_id TEXT NOT NULL REFERENCES merge_requests(id),
  host_thread_id TEXT NOT NULL,
  path TEXT,
  line INTEGER,
  resolved INTEGER NOT NULL DEFAULT 0,
  comments_json TEXT NOT NULL DEFAULT '[]',
  synced_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS status_checks (
  id TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id),
  ref_sha TEXT NOT NULL,
  name TEXT NOT NULL,
  status TEXT NOT NULL,
  conclusion TEXT,
  details_url TEXT,
  synced_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS workflow_runs (
  id TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id),
  host_run_id TEXT NOT NULL,
  name TEXT,
  ref_name TEXT,
  head_sha TEXT,
  status TEXT,
  conclusion TEXT,
  url TEXT,
  started_at TEXT,
  updated_at TEXT,
  synced_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS rendered_markdown_cache (
  id TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id),
  ref_sha TEXT NOT NULL,
  path TEXT NOT NULL,
  blob_sha TEXT NOT NULL,
  renderer_version TEXT NOT NULL,
  html TEXT NOT NULL,
  toc_json TEXT NOT NULL DEFAULT '[]',
  links_json TEXT NOT NULL DEFAULT '[]',
  rendered_at TEXT NOT NULL,
  UNIQUE(repository_id, ref_sha, path, blob_sha, renderer_version)
);

CREATE TABLE IF NOT EXISTS audit_events (
  id TEXT PRIMARY KEY,
  actor_kind TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  action_id TEXT NOT NULL,
  target_entity TEXT NOT NULL,
  risk_tier TEXT NOT NULL,
  preview_json TEXT,
  result_json TEXT,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_repositories_family ON repositories(family);
CREATE INDEX IF NOT EXISTS idx_merge_requests_repo_state ON merge_requests(repository_id, state);
CREATE INDEX IF NOT EXISTS idx_status_checks_sha ON status_checks(repository_id, ref_sha);
CREATE INDEX IF NOT EXISTS idx_audit_events_created ON audit_events(created_at);
```

## 6.12 CLI integration

### MODIFY `src/cli.rs`

Add web commands:

```diff
 pub enum Command {
+    Web(WebCommand),
     ...
 }
+
+#[derive(Debug, clap::Subcommand)]
+pub enum WebCommand {
+    Serve {
+        #[arg(long, default_value = "127.0.0.1:8787")]
+        bind: String,
+        #[arg(long)]
+        open: bool,
+        #[arg(long)]
+        dev_assets: Option<String>,
+    },
+    Open,
+    BuildAssets,
+}
```

### MODIFY `src/dispatch.rs`

Add:

```rust
Command::Web(cmd) => crate::web::command::run(cmd).await,
```

### ADD `src/web/command.rs`

Responsibilities:

- construct `WebState`;
- bind axum server;
- serve SPA assets from `apps/web/dist` or embedded assets;
- support dev proxy to Vite server;
- print local URL;
- optional browser open.

## 6.13 Frontend application

### REPLACE `apps/web/package.json`

```json
{
  "name": "@jeryu/web",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite --host 127.0.0.1 --port 5173",
    "build": "tsc -b && vite build",
    "preview": "vite preview --host 127.0.0.1 --port 4173",
    "typecheck": "tsc -b --pretty false",
    "lint": "eslint .",
    "test": "vitest run",
    "test:watch": "vitest",
    "test:e2e": "playwright test",
    "storybook": "storybook dev -p 6006",
    "build-storybook": "storybook build",
    "ux-qa": "node ./ux-qa-check.mjs build && node ./ux-qa-check.mjs test"
  },
  "dependencies": {
    "@monaco-editor/react": "latest",
    "@tanstack/react-query": "latest",
    "@tanstack/react-table": "latest",
    "@tanstack/react-virtual": "latest",
    "cmdk": "latest",
    "dompurify": "latest",
    "lucide-react": "latest",
    "react": "latest",
    "react-dom": "latest",
    "react-markdown": "latest",
    "react-router-dom": "latest",
    "rehype-autolink-headings": "latest",
    "rehype-highlight": "latest",
    "rehype-raw": "latest",
    "rehype-sanitize": "latest",
    "rehype-slug": "latest",
    "remark-gfm": "latest",
    "zod": "latest",
    "zustand": "latest"
  },
  "devDependencies": {
    "@playwright/test": "latest",
    "@storybook/addon-a11y": "latest",
    "@storybook/addon-vitest": "latest",
    "@storybook/react-vite": "latest",
    "@testing-library/jest-dom": "latest",
    "@testing-library/react": "latest",
    "@testing-library/user-event": "latest",
    "@types/dompurify": "latest",
    "@types/node": "latest",
    "@types/react": "latest",
    "@types/react-dom": "latest",
    "@vitejs/plugin-react": "latest",
    "axe-core": "latest",
    "eslint": "latest",
    "eslint-plugin-jsx-a11y": "latest",
    "eslint-plugin-react-hooks": "latest",
    "jsdom": "latest",
    "msw": "latest",
    "typescript": "latest",
    "vite": "latest",
    "vitest": "latest"
  }
}
```

For deterministic CI, pin exact versions in the actual patch and commit `package-lock.json`.

### ADD `apps/web/index.html`

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>JeRyu</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

### ADD `apps/web/vite.config.ts`

```ts
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8787',
        ws: true,
      },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
  },
});
```

### ADD `apps/web/tsconfig.json`

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "allowJs": false,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "strict": true,
    "forceConsistentCasingInFileNames": true,
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "types": ["vitest/globals"]
  },
  "include": ["src", "tests", "vite.config.ts"]
}
```

### ADD frontend tree

```text
apps/web/src/
  main.tsx
  app/
    App.tsx
    router.tsx
    providers.tsx
  api/
    client.ts
    endpoints.ts
    schemas.ts
    types.ts
    websocket.ts
  layout/
    AppShell.tsx
    CommandPalette.tsx
    GlobalHeader.tsx
    LeftNav.tsx
    LiveActivityDock.tsx
    RepoSwitcher.tsx
    StatusBar.tsx
  pages/
    DashboardPage.tsx
    RepositoriesPage.tsx
    RepositoryOverviewPage.tsx
    RepositoryCodePage.tsx
    RepositoryFilePage.tsx
    RepositoryMergeRequestsPage.tsx
    MergeRequestPage.tsx
    RepositoryActionsPage.tsx
    RepositorySettingsPage.tsx
    AdminSettingsPage.tsx
    NotFoundPage.tsx
  components/
    action/
      ActionButton.tsx
      ActionPreviewDialog.tsx
      RiskBadge.tsx
    repo/
      CreateRepoDialog.tsx
      RepoCard.tsx
      RepoFamilyGroup.tsx
      RepoHealthPill.tsx
      RepoTable.tsx
    browser/
      BranchSelector.tsx
      Breadcrumbs.tsx
      CodeViewer.tsx
      FileTree.tsx
      MarkdownRenderer.tsx
      ReadmePanel.tsx
    merge/
      ChecksPanel.tsx
      DiffFileTree.tsx
      DiffViewer.tsx
      InlineComment.tsx
      MergeGatePanel.tsx
      ReviewSidebar.tsx
      ThreadList.tsx
    settings/
      SettingsLayout.tsx
      SettingsSection.tsx
      SettingsDiffPreview.tsx
      BranchProtectionEditor.tsx
      MergePolicyEditor.tsx
      AgentPolicyEditor.tsx
      SecretsMetadataTable.tsx
  hooks/
    useBootstrap.ts
    useRepositories.ts
    useRepoTree.ts
    useBlob.ts
    useMarkdown.ts
    useMergeRequest.ts
    useRepoSettings.ts
    useWebsocket.ts
  stores/
    realtimeStore.ts
    selectionStore.ts
    commandStore.ts
    preferencesStore.ts
  styles/
    tokens.css
    app.css
  test/
    mocks.ts
    server.ts
```

### ADD `apps/web/src/main.tsx`

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './app/App';
import './styles/tokens.css';
import './styles/app.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

### ADD `apps/web/src/app/App.tsx`

```tsx
import { RouterProvider } from 'react-router-dom';
import { AppProviders } from './providers';
import { router } from './router';

export function App() {
  return (
    <AppProviders>
      <RouterProvider router={router} />
    </AppProviders>
  );
}
```

### ADD `apps/web/src/app/router.tsx`

```tsx
import { createBrowserRouter } from 'react-router-dom';
import { AppShell } from '../layout/AppShell';
import { DashboardPage } from '../pages/DashboardPage';
import { RepositoriesPage } from '../pages/RepositoriesPage';
import { RepositoryOverviewPage } from '../pages/RepositoryOverviewPage';
import { RepositoryCodePage } from '../pages/RepositoryCodePage';
import { RepositoryFilePage } from '../pages/RepositoryFilePage';
import { RepositoryMergeRequestsPage } from '../pages/RepositoryMergeRequestsPage';
import { MergeRequestPage } from '../pages/MergeRequestPage';
import { RepositorySettingsPage } from '../pages/RepositorySettingsPage';
import { NotFoundPage } from '../pages/NotFoundPage';

export const router = createBrowserRouter([
  {
    path: '/',
    element: <AppShell />,
    children: [
      { index: true, element: <DashboardPage /> },
      { path: 'repos', element: <RepositoriesPage /> },
      { path: 'repos/:host/:owner/:repo', element: <RepositoryOverviewPage /> },
      { path: 'repos/:host/:owner/:repo/code', element: <RepositoryCodePage /> },
      { path: 'repos/:host/:owner/:repo/blob/*', element: <RepositoryFilePage /> },
      { path: 'repos/:host/:owner/:repo/merge-requests', element: <RepositoryMergeRequestsPage /> },
      { path: 'repos/:host/:owner/:repo/merge-requests/:iid', element: <MergeRequestPage /> },
      { path: 'repos/:host/:owner/:repo/settings/*', element: <RepositorySettingsPage /> },
      { path: '*', element: <NotFoundPage /> },
    ],
  },
]);
```

### ADD `apps/web/src/api/client.ts`

```ts
export class ApiError extends Error {
  constructor(public status: number, public code: string, message: string) {
    super(message);
  }
}

export async function apiGet<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: {
      Accept: 'application/json',
      ...init?.headers,
    },
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ code: 'http_error', message: res.statusText }));
    throw new ApiError(res.status, body.code, body.message);
  }
  return res.json() as Promise<T>;
}

export async function apiSend<T>(method: string, path: string, body: unknown): Promise<T> {
  const res = await fetch(path, {
    method,
    headers: {
      Accept: 'application/json',
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ code: 'http_error', message: res.statusText }));
    throw new ApiError(res.status, err.code, err.message);
  }
  return res.json() as Promise<T>;
}
```

### ADD `apps/web/src/api/websocket.ts`

```ts
import { create } from 'zustand';

export type WebEvent = {
  seq: number;
  timestamp: string;
  scope: string;
  kind: string;
  entity: string;
  summary: string;
  payload: unknown;
};

type RealtimeState = {
  connected: boolean;
  lastSeq: number;
  events: WebEvent[];
  connect: () => void;
};

export const useRealtimeStore = create<RealtimeState>((set, get) => ({
  connected: false,
  lastSeq: 0,
  events: [],
  connect: () => {
    const current = get();
    const proto = window.location.protocol === 'https:' ? 'wss' : 'ws';
    const ws = new WebSocket(`${proto}://${window.location.host}/api/ws`);

    ws.addEventListener('open', () => {
      set({ connected: true });
      ws.send(JSON.stringify({ type: 'hello', resume_from: current.lastSeq, subscriptions: [{ scope: 'global', filters: {} }] }));
    });

    ws.addEventListener('close', () => set({ connected: false }));

    ws.addEventListener('message', (message) => {
      const data = JSON.parse(message.data);
      if (data.type === 'event') {
        const event = data.event as WebEvent;
        set((state) => ({
          lastSeq: Math.max(state.lastSeq, event.seq),
          events: [event, ...state.events].slice(0, 500),
        }));
      }
      if (data.type === 'snapshot_required') {
        window.location.reload();
      }
    });
  },
}));
```

### ADD `apps/web/src/components/browser/MarkdownRenderer.tsx`

The backend should return sanitized HTML. The frontend should still treat HTML defensively.

```tsx
import DOMPurify from 'dompurify';
import { useMemo } from 'react';

type MarkdownRendererProps = {
  html: string;
  className?: string;
};

export function MarkdownRenderer({ html, className }: MarkdownRendererProps) {
  const safeHtml = useMemo(() => DOMPurify.sanitize(html), [html]);
  return (
    <article
      className={className ?? 'markdown-body'}
      dangerouslySetInnerHTML={{ __html: safeHtml }}
    />
  );
}
```

### ADD `apps/web/src/components/browser/FileTree.tsx`

Use virtualization for large repositories.

```tsx
import { useVirtualizer } from '@tanstack/react-virtual';
import { useRef } from 'react';

export type TreeEntry = {
  path: string;
  name: string;
  kind: 'file' | 'directory' | 'symlink' | 'submodule';
  sha: string;
  size_bytes?: number;
};

export function FileTree({ entries, onOpen }: { entries: TreeEntry[]; onOpen: (entry: TreeEntry) => void }) {
  const parentRef = useRef<HTMLDivElement>(null);
  const rowVirtualizer = useVirtualizer({
    count: entries.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 28,
  });

  return (
    <div ref={parentRef} className="file-tree" role="tree">
      <div style={{ height: rowVirtualizer.getTotalSize(), position: 'relative' }}>
        {rowVirtualizer.getVirtualItems().map((virtualRow) => {
          const entry = entries[virtualRow.index];
          return (
            <button
              key={entry.path}
              role="treeitem"
              className="file-tree-row"
              style={{ transform: `translateY(${virtualRow.start}px)` }}
              onClick={() => onOpen(entry)}
            >
              <span>{entry.kind === 'directory' ? '▸' : ' '}</span>
              <span>{entry.name}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
```

## 6.14 UX QA upgrade

### MODIFY `apps/web/ux-qa-check.mjs`

The current marker-only checker should become a real proof collector.

New checks:

- Vite build artifact exists and contains `index.html` plus JS/CSS bundles.
- TypeScript typecheck passed.
- Vitest unit tests passed.
- Storybook build exists.
- Playwright screenshots exist for:
  - repositories page;
  - repo overview with README;
  - code browser;
  - merge request page;
  - settings page;
  - permission denied page;
  - error/empty/loading states.
- Axe accessibility results exist.
- Layout shift budget is under threshold.
- Markdown renderer fixtures pass.
- WebSocket mock replay test passes.

### ADD `apps/web/tests/markdown-renderer.test.tsx`

Test tables, tasks, links, images, headings, fenced code, and raw HTML sanitization.

### ADD `apps/web/tests/websocket-replay.test.ts`

Test:

- hello/resume;
- event application in order;
- duplicate seq ignored;
- gap triggers snapshot refresh;
- reconnect uses last known seq.

### ADD Storybook stories

```text
apps/web/src/components/**/*.stories.tsx
```

Minimum required stories:

- `RepoCard`: healthy, warning, critical, archived, private.
- `ReadmePanel`: loading, empty, rendered, malicious HTML sanitized.
- `DiffViewer`: small, huge, binary, generated, comments.
- `MergeGatePanel`: pass, blocked, stale SHA, approval required, agent evidence.
- `SettingsDiffPreview`: safe, reversible, irreversible, production-impact.

---

## 7. REST API Specification

## 7.1 Bootstrap

```http
GET /api/bootstrap
```

Returns `WebBootstrap`.

Purpose: first paint without multiple round trips.

Includes:

- viewer;
- permissions;
- TUI read model;
- recent repositories;
- websocket URL;
- feature flags.

## 7.2 Repositories

```http
GET /api/repos?search=&host=&owner=&family=&include_archived=false&limit=50&cursor=
POST /api/repos
GET /api/repos/{host}/{owner}/{repo}
PATCH /api/repos/{host}/{owner}/{repo}/settings
```

`POST /api/repos` must support dry-run preview:

```json
{
  "host": "github",
  "owner": "neverhuman",
  "name": "new-repo",
  "description": "...",
  "visibility": "private",
  "initialize_readme": true,
  "dry_run": true
}
```

When `dry_run=false`, require permission and idempotency key.

## 7.3 Code browser

```http
GET /api/repos/{host}/{owner}/{repo}/refs
GET /api/repos/{host}/{owner}/{repo}/tree?ref=main&path=src
GET /api/repos/{host}/{owner}/{repo}/blob?ref=main&path=README.md&render=html
GET /api/repos/{host}/{owner}/{repo}/readme?ref=main
GET /api/repos/{host}/{owner}/{repo}/compare?base=main&head=feature
```

Binary file behavior:

- return `is_binary=true`;
- do not include text;
- include base64 only when explicitly requested and size below threshold;
- provide download URL through authenticated route.

## 7.4 Merge requests / pull requests

```http
GET /api/repos/{host}/{owner}/{repo}/merge-requests?state=open
POST /api/repos/{host}/{owner}/{repo}/merge-requests
GET /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}
POST /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/approve
POST /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/merge
POST /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/close
GET /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/reviews
POST /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/reviews
GET /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/threads
POST /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/threads
PATCH /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/threads/{thread_id}
```

Approval request:

```json
{
  "expected_head_sha": "abc123",
  "body": "Looks good.",
  "idempotency_key": "uuid"
}
```

Merge request:

```json
{
  "expected_head_sha": "abc123",
  "method": "squash",
  "commit_title": "...",
  "commit_message": "...",
  "idempotency_key": "uuid"
}
```

If live head SHA differs from `expected_head_sha`, return `409 Conflict`.

## 7.5 CI/checks/logs

```http
GET /api/repos/{host}/{owner}/{repo}/runs?branch=&status=&limit=
GET /api/repos/{host}/{owner}/{repo}/runs/{run_id}
GET /api/repos/{host}/{owner}/{repo}/runs/{run_id}/jobs
GET /api/repos/{host}/{owner}/{repo}/jobs/{job_id}/logs?cursor=
GET /api/repos/{host}/{owner}/{repo}/checks?sha=
POST /api/repos/{host}/{owner}/{repo}/runs/{run_id}/rerun
```

Logs should stream through WebSocket when open.

## 7.6 Settings

```http
GET /api/repos/{host}/{owner}/{repo}/settings
PATCH /api/repos/{host}/{owner}/{repo}/settings
POST /api/repos/{host}/{owner}/{repo}/settings/preview
```

Patch format:

```json
{
  "base_settings_hash": "sha256:...",
  "patch": {
    "merge": {
      "required_approvals": 2,
      "require_jeryu_merge_passport": true
    }
  },
  "idempotency_key": "uuid"
}
```

## 7.7 Actions

```http
POST /api/actions/{action_id}/preview
POST /api/actions/{action_id}/execute
```

All high-value UI buttons should use this pattern, even if there is a convenience endpoint.

---

## 8. WebSocket Specification

## 8.1 Endpoint

```http
GET /api/ws
```

## 8.2 Client hello

```json
{
  "type": "hello",
  "resume_from": 1234,
  "subscriptions": [
    { "scope": "global", "filters": {} },
    { "scope": "repo:github/neverhuman/jeryu", "filters": {} },
    { "scope": "mr:github/neverhuman/jeryu/42", "filters": {} }
  ]
}
```

## 8.3 Server hello

```json
{
  "type": "hello",
  "server_time": "2026-05-26T12:00:00Z",
  "current_seq": 1300,
  "protocol": "jeryu.ws.v1"
}
```

## 8.4 Event

```json
{
  "type": "event",
  "event": {
    "seq": 1301,
    "timestamp": "2026-05-26T12:00:01Z",
    "scope": "repo:github/neverhuman/jeryu",
    "kind": "repo.settings.changed",
    "entity": "project:github/neverhuman/jeryu",
    "summary": "Required approvals changed from 1 to 2",
    "payload": {
      "actor": "ben",
      "settings_hash": "sha256:..."
    }
  }
}
```

## 8.5 Gap handling

If `resume_from` is too old or a broadcast lag occurs:

```json
{
  "type": "snapshot_required",
  "reason": "event_gap",
  "current_seq": 9001
}
```

The frontend refetches `/api/bootstrap` or the current route snapshot and reconnects with the new cursor.

## 8.6 Event kinds to add

Add web-facing event kinds in addition to existing TUI kinds:

- `repo.created`
- `repo.updated`
- `repo.deleted`
- `repo.archived`
- `repo.settings.changed`
- `repo.branch.created`
- `repo.branch.deleted`
- `repo.branch.protection.changed`
- `repo.file.changed`
- `repo.readme.rendered`
- `mr.created`
- `mr.updated`
- `mr.review.submitted`
- `mr.thread.created`
- `mr.thread.resolved`
- `mr.approved`
- `mr.merged`
- `mr.merge.blocked`
- `check.started`
- `check.completed`
- `workflow.run.started`
- `workflow.run.completed`
- `job.log.chunk`
- `agent.session.started`
- `agent.patch.proposed`
- `agent.evidence.created`
- `settings.preview.created`
- `action.previewed`
- `action.executed`
- `audit.event.created`

---

## 9. Permissions and Safety

## 9.1 Permission model

Use these normalized permissions in web responses:

- `repo.read`
- `repo.create`
- `repo.write`
- `repo.admin`
- `settings.read`
- `settings.write`
- `code.read`
- `code.write`
- `mr.read`
- `mr.write`
- `mr.approve`
- `mr.merge`
- `ci.read`
- `ci.write`
- `secrets.read_metadata`
- `secrets.write`
- `agents.read`
- `agents.write`
- `audit.read`

Map host permissions into these normalized permissions server-side.

## 9.2 Action preview contract

Every mutation response must include or reference:

- action ID;
- actor;
- target entity;
- old state hash;
- proposed new state hash;
- risk tier;
- side effects;
- what will not happen;
- exact SHA if code/review/merge related;
- idempotency key;
- audit receipt.

## 9.3 Exact-SHA rules

For approval and merge:

1. UI sends `expected_head_sha`.
2. Backend refetches live PR/MR state.
3. Backend rejects if live head differs.
4. Backend validates required checks and merge passport.
5. Backend calls host with exact SHA when available.
6. Backend emits audit event and websocket event.

## 9.4 Markdown security

- Sanitize on backend.
- Sanitize again on frontend before `dangerouslySetInnerHTML`.
- Strip scripts, event handlers, iframes, forms, style attributes unless explicitly allowed.
- Rewrite relative links to JeRyu routes.
- External links get `rel="noopener noreferrer"` and open behavior per user preference.
- Images must respect auth and should not leak tokens.

## 9.5 Secrets security

- UI can list secret names, scopes, updated age, last access metadata.
- UI must never retrieve secret values after creation.
- Rotation flow writes a new value and shows receipt only.
- All access denied events stream to security/audit views.

---

## 10. Performance Targets

| Area | Target |
|---|---:|
| Initial app shell JS | < 350 KB gzip for critical shell |
| First useful paint on local server | < 1.5 s |
| Route transition after bootstrap | < 100 ms perceived |
| Repo list search/filter | < 50 ms client-side for 5k cached repos |
| File tree render | virtualized; supports 100k entries |
| Diff render | virtualized; supports 20k changed lines |
| WebSocket event delivery local p95 | < 250 ms |
| Markdown render cache hit | < 25 ms |
| Markdown render cache miss for README | < 150 ms typical |
| Settings preview | < 500 ms excluding host fetch |

---

## 11. Testing Plan

## 11.1 Rust tests

Required:

```bash
cargo check --workspace
cargo nextest run -p jeryu --lib
cargo nextest run --test mock_lifecycle_tests
cargo test -p jeryu --test '*' -- --test-threads=1
```

New test modules:

```text
src/web/router_tests.rs
src/web/ws_tests.rs
src/repo_browser/markdown_tests.rs
src/repos/service_tests.rs
src/merge/service_tests.rs
src/git_host/github_web_tests.rs
src/git_host/gitlab_web_tests.rs
```

Critical cases:

- no missing module exports;
- bootstrap returns schema and viewer;
- repo creation preview does not write;
- repo creation execute writes once with idempotency;
- settings patch rejects stale hash;
- markdown sanitizes malicious HTML;
- relative links rewritten;
- binary blob does not decode as text;
- approve rejects stale SHA;
- merge rejects stale SHA;
- websocket gap triggers snapshot required;
- audit event written for every mutation.

## 11.2 Frontend tests

Required:

```bash
npm run typecheck
npm run lint
npm run test
npm run build
npm run build-storybook
npm run test:e2e
npm run ux-qa
```

Critical cases:

- dashboard loads from bootstrap;
- repository list filters and groups;
- create repo dialog preview/execute flow;
- README renders tables/tasks/code/links;
- malicious markdown HTML is sanitized;
- code browser opens file from tree;
- MR page renders file tree/diff/checks;
- approve button sends expected SHA;
- stale SHA conflict displays safe recovery;
- settings page previews changes;
- websocket updates activity dock;
- keyboard shortcuts work;
- a11y checks pass.

## 11.3 Playwright scenarios

1. New user opens dashboard.
2. User creates private repo with README.
3. User opens repo overview and sees rendered README.
4. User navigates code tree and opens a Markdown doc.
5. User opens MR, reviews diff, comments inline.
6. User approves exact SHA.
7. Backend simulates force push; user attempts approve; UI shows stale SHA conflict.
8. User changes required approvals setting with preview.
9. WebSocket emits CI failure; live dock updates.
10. Permission denied user sees disabled controls and explanation.

---

## 12. Rollout Plan

## Phase 0: Build sanity and contracts

- Fix `src/api/mod.rs` missing/stale exports.
- Add repository/browser/settings/merge API structs.
- Add web feature flag.
- Add OpenAPI/schema generation.
- Add test skeletons.

Exit criteria:

- `cargo check --workspace` green.
- Type contracts serialize/deserialize.

## Phase 1: Web shell and bootstrap

- Replace `apps/web` placeholder with Vite React app.
- Add `/api/bootstrap`.
- Add static asset serving.
- Add app shell, dashboard shell, command palette.
- Add WebSocket hello/event plumbing.

Exit criteria:

- `jeryu web serve` opens app.
- App shows bootstrap data and live connection status.

## Phase 2: Repositories and README

- Implement repo list.
- Implement repo create preview/execute.
- Implement repo overview.
- Implement README endpoint and Markdown rendering.
- Implement Markdown renderer component.

Exit criteria:

- User can see all repos.
- User can create a repo.
- User can open a repo and see rendered README.

## Phase 3: Code browser

- Implement refs/tree/blob.
- Add virtualized file tree.
- Add code viewer.
- Add file search.
- Add relative link rewriting.

Exit criteria:

- User can browse branches and files quickly.

## Phase 4: Merge room

- Implement MR list/detail.
- Implement diff viewer.
- Implement review threads/comments.
- Implement exact-SHA approve.
- Implement exact-SHA merge.
- Implement merge gate panel.

Exit criteria:

- User can review files, approve, and merge safely.

## Phase 5: Settings

- Implement settings read/preview/patch.
- Implement settings sections.
- Add branch protection editor.
- Add merge policy editor.
- Add agents/CI/security settings.

Exit criteria:

- User can configure repo settings with preview and audit.

## Phase 6: CI, agents, activity

- Stream workflow runs/checks/jobs/logs.
- Surface agent sessions/evidence in repo and MR pages.
- Add activity dock subscriptions.
- Add failure capsules.

Exit criteria:

- User sees live work happening across repos and can drill down.

## Phase 7: Hardening

- Auth/session CSRF.
- Audit export.
- Rate limits.
- E2E tests.
- UX QA artifacts.
- Performance budget.
- Docs.

Exit criteria:

- Release-blocking profile green.
- Playwright/a11y/visual proof artifacts produced.

---

## 13. Documentation Changes

### ADD `docs/web-forge.md`

Contents:

- architecture overview;
- local dev;
- deployment;
- API contracts;
- WebSocket protocol;
- Markdown rendering/security;
- host adapter expectations;
- action safety model;
- troubleshooting.

### MODIFY `README.md`

Add:

```md
## Web Forge

Run the modern JeRyu web experience:

```bash
npm install
npm run build
cargo run -p jeryu -- web serve --open
```

For frontend development:

```bash
cargo run -p jeryu -- web serve --dev-assets http://127.0.0.1:5173
npm run dev
```
```

### ADD `apps/web/README.md`

Explain:

- frontend structure;
- API client;
- WebSocket store;
- Storybook;
- test commands;
- design tokens;
- accessibility rules;
- Markdown renderer fixtures.

---

## 14. Acceptance Criteria

The feature is done when all of the following are true:

1. `jeryu web serve --open` launches a browser UI.
2. The UI lists all accessible repos across configured hosts.
3. User can create a repository with preview, permission check, idempotency, audit event, and websocket update.
4. User can open any repo overview and see a correctly rendered sanitized README.
5. User can browse branches, trees, and files.
6. User can open Markdown files and see correct HTML rendering.
7. User can open merge requests/pull requests.
8. User can review changed files and submit comments.
9. User can approve a merge request bound to exact head SHA.
10. User can merge only when live gates pass and head SHA matches.
11. User can view and change repo settings through searchable settings pages with preview.
12. WebSocket updates activity, CI, checks, agents, settings, and merge posture in real time.
13. All mutating actions write audit receipts.
14. Frontend has Storybook, unit tests, Playwright E2E, accessibility checks, and visual proof artifacts.
15. Rust and frontend CI are green.

---

## 15. Highest-Risk Areas

1. **Host API mismatch:** GitHub/GitLab settings and review APIs differ. Mitigate with normalized models and host-specific extension fields.
2. **Markdown security:** sanitize twice and test malicious fixtures.
3. **Huge diffs/trees/logs:** virtualize and stream; never render huge arrays naïvely.
4. **Exact-SHA safety:** always refetch live state immediately before approval/merge.
5. **WebSocket backpressure:** use bounded channels, subscription scopes, and gap recovery.
6. **Permissions:** never trust frontend-hidden buttons; backend must enforce every action.
7. **Existing module drift:** fix stale/missing module exports before feature work.
8. **Frontend package drift:** pin dependency versions before committing.

---

## 16. Implementation Notes for “Better Than GitHub/GitLab”

The key differentiator is not merely cloning every GitHub screen. It is collapsing the important information into direct answers and safe actions.

GitHub/GitLab model:

- many tabs;
- many pages;
- checks, comments, bots, settings, and logs scattered;
- merge blockers often require hunting;
- settings are powerful but hard to understand;
- real-time updates are partial.

JeRyu model:

- one attention-driven dashboard;
- one merge room;
- one live activity dock;
- one command palette;
- one Merge Passport;
- every action has preview, risk, grant, audit, and evidence;
- every view updates from typed events.

This is how JeRyu can feel more modern, faster, and safer while still delivering the full forge feature set.


---

# Appendix: Best-of Solution Synthesis Addendum

The following sections consolidate the most complete controls, settings, review, repository-creation, better-than-forge, and acceptance material from the uploaded solution set. They are intentionally redundant with the core spec where redundancy improves implementation clarity.

## 15. UX controls that provide quick value

### 15.1 Global controls

- Global command palette: `⌘K` / `Ctrl+K`.
- Repo switcher: `/` from anywhere, or command palette action.
- Global search box: repositories, files, commits, issues, MRs, users, settings.
- Pinned repos and repo families.
- Live activity rail.
- Notification bell with unread counts.
- Keyboard shortcut overlay.
- Theme selector: system/light/dark/high-contrast.
- Density selector: comfortable/compact/ultra-compact.
- WebSocket status indicator: live/reconnecting/offline/stale.
- “Explain current blocker” action.
- “Copy deep link” action for every entity.

### 15.2 Repository list controls

- Filter by owner/org, repo family, visibility, language, topic, archived, fork/template, activity, failing checks, pending review.
- Sort by recent activity, name, stars, open MRs, open issues, failing CI, runner pressure.
- Quick actions per row: open, clone, star, pin, new MR, settings, copy URL.
- Bulk actions: archive, transfer, apply setting template, add topic, export.
- Create repo modal.
- Import repo modal.
- “Connect external GitHub/GitLab repo” modal.

### 15.3 Repository overview controls

- Branch selector.
- Clone dropdown: HTTPS, SSH, `jeryu clone`, token scopes.
- Star/watch/fork buttons.
- Repo health cards: CI, merge gates, agents, runners, cache, security.
- README card with source path, edit button, copy heading links.
- Latest release card.
- Recent commits.
- Open MRs and issues.
- Agent evidence summary.
- Repo settings shortcut.

### 15.4 File browser controls

- Tree/list split view.
- Branch/ref selector.
- Breadcrumb navigation.
- Fuzzy file finder.
- Raw, blame, history, copy path, copy permalink, download.
- Edit file, delete file, upload files, create file, create folder.
- Open in web editor.
- Syntax highlighting.
- Large file fallback.
- Binary/image preview.
- README auto-render when selected.

### 15.5 Merge request controls

- Overview/files/checks/commits/evidence tabs.
- Unified/split diff toggle.
- Hide whitespace.
- File tree with reviewed state.
- “Viewed” checkboxes.
- Inline comments.
- Multi-line comments.
- Suggestions.
- Start review / submit review.
- Approve / request changes / comment.
- Resolve thread / unresolve thread.
- Re-run checks.
- Request agent patch.
- Race agent patches.
- Run VTI plan.
- Update branch.
- Merge strategy selector: merge commit, squash, rebase, fast-forward if possible.
- Delete source branch checkbox.
- Auto-merge toggle.
- Merge preview modal with exact SHA.
- Conflict view and instructions.
- Required checks and branch protection explanation.
- “Why can’t I merge?” button.

### 15.6 Issue controls

- Saved filters: assigned to me, created by me, mentions, high priority, stale, agent-owned.
- Labels, milestones, assignees.
- Link issue to MR.
- Convert issue to MR branch.
- Close/reopen.
- Pin issue.
- Triage board.
- Agent bug attempt linkage.

### 15.7 Settings controls

Every settings page should have:

- Search within settings.
- Unsaved changes bar.
- Preview changes.
- Audit diff.
- Reset to inherited/default.
- Apply from template.
- Export/import settings JSON.
- Dangerous actions grouped in a clearly separated Danger Zone.

---

---

## 16. Settings matrix

### 16.1 User settings

- Profile: display name, email, avatar, bio, timezone.
- Preferences: theme, density, default landing page, keyboard mode, date format.
- Notifications: email/web/in-app, watch defaults, mention defaults, review request defaults.
- SSH keys.
- Personal access tokens.
- Sessions.
- OAuth/Git host connections.
- GPG/signing keys.
- Accessibility: reduced motion, high contrast, focus indicators, code font size.

### 16.2 Organization settings

- Name, slug, avatar, description, homepage.
- Visibility policy.
- Member default role.
- Teams.
- Invitations.
- External collaborator policy.
- Repository creation policy.
- Default repo settings template.
- Branch protection templates.
- Webhooks.
- Audit log retention.
- SSO/OIDC if configured.
- IP allowlist.
- Billing/storage if applicable.

### 16.3 Repository general settings

- Repository name.
- Description.
- Homepage.
- Topics.
- Visibility: private/internal/public.
- Default branch.
- Template repo toggle.
- Archive/unarchive.
- Transfer ownership.
- Delete repository.
- Forking enabled.
- Issues enabled.
- Wiki enabled.
- Projects enabled.
- Packages enabled.
- Releases enabled.
- Discussions enabled if implemented.
- LFS enabled.
- Snippets enabled if implemented.

### 16.4 Merge settings

- Allowed merge strategies.
- Default merge strategy.
- Squash commit title template.
- Squash commit message template.
- Merge commit title template.
- Merge commit message template.
- Rebase merge allowed.
- Fast-forward only.
- Linear history required.
- Auto-merge allowed.
- Delete source branch on merge.
- Require up-to-date branch.
- Require signed commits.
- Require conversation resolution.
- Allow maintainer edits.
- Draft MR restrictions.

### 16.5 Branch protection settings

For each rule/pattern:

- Branch pattern.
- Priority.
- Require pull/merge request.
- Required approval count.
- Required code owner review.
- Dismiss stale approvals on push.
- Require review from non-author.
- Require resolved conversations.
- Required status checks.
- Require exact check names.
- Require branch up to date.
- Allow force push.
- Allow deletion.
- Restrict push users/teams.
- Restrict bypass users/teams.
- Require signed commits.
- Require linear history.
- Lock branch.
- Agent merge policy.
- VTI confidence floor.
- Evidence capsule requirement.
- Merge passport requirement.

### 16.6 CI/CD settings

- Runner groups.
- Runner tags.
- Default timeout.
- Artifact retention.
- Cache retention.
- Concurrent job limit.
- Workflow permissions.
- Secrets.
- Variables.
- Environments.
- Deployment approvals.
- Webhook trigger policy.
- Scheduled pipelines.
- Required evidence capture.

### 16.7 Agent/autonomy settings

- Agents enabled.
- Allowed agent identities.
- Grant policy by risk tier.
- Max concurrent agent tasks.
- Patch proposal policy.
- Race-patches policy.
- Auto-review policy.
- Auto-merge policy.
- VTI threshold.
- Evidence retention.
- LLM provider/model settings.
- Prompt/policy source branch.
- Secret lease TTL.
- Human approval thresholds.
- Blocked path patterns.
- Owned path policy.

### 16.8 Security settings

- Secret scanning.
- Dependency scanning.
- Push protection.
- Token TTL policy.
- Deploy keys.
- Webhooks signing secret.
- Audit retention.
- IP allowlist.
- Required two-factor auth if supported.
- Session lifetime.
- Download restrictions.
- Public access restrictions.

### 16.9 Integration settings

- GitHub remote.
- GitLab remote.
- Mirror direction.
- Mirror schedule.
- Webhooks.
- Slack/Discord/email notifications.
- MCP tools.
- Artifact storage.
- Container registry.
- Package registry.

---

---

## 19. Merge request review product spec

### 19.1 MR detail page layout

```text
┌─────────────────────────────────────────────────────────────────┐
│ MR title, number, state, branches, exact head SHA, live status   │
├───────────────────────┬─────────────────────────────────────────┤
│ Summary               │ Checks / merge readiness / next action   │
│ - description         │ - required checks                         │
│ - labels/assignees    │ - approvals                               │
│ - reviewers           │ - conversations                           │
│ - timeline            │ - agent evidence                           │
├───────────────────────┴─────────────────────────────────────────┤
│ Tabs: Conversation | Files | Commits | Checks | Evidence          │
├───────────────────────┬─────────────────────────────────────────┤
│ changed files tree    │ diff viewer + inline comments             │
└───────────────────────┴─────────────────────────────────────────┘
```

### 19.2 Review state machine

```text
open/draft ── mark ready ──▶ open/ready
open/ready ── approve ─────▶ approved if branch rules satisfied
open/ready ── request changes ─▶ changes_requested
approved ── new push ──────▶ approval stale if rule says dismiss stale
approved + checks pass + threads resolved ── merge ─▶ merged
open ── close ─▶ closed
closed ── reopen ─▶ open
```

### 19.3 Merge button states

| State | Button | Explanation |
|---|---|---|
| Can merge | Enabled | All checks and approvals satisfied |
| Checks pending | Disabled | Shows running checks and elapsed time |
| Checks failed | Disabled | Shows failed check logs and rerun controls |
| Needs approval | Disabled | Shows required reviewers/code owners |
| Threads unresolved | Disabled | Shows unresolved thread count and jump list |
| Head stale | Disabled or update branch | Shows update branch control |
| Conflict | Disabled | Shows conflict details |
| Risk gate hold | Disabled | Shows agent/evidence/vibegate blocker |
| No permission | Disabled | Shows required role |

---

---

## 20. Repository creation/import spec

### 20.1 Create repository flow

Fields:

- Owner: user or org.
- Name.
- Description.
- Visibility.
- Initialize with README.
- `.gitignore` template.
- License template.
- Default branch.
- Template repo source.
- Enable issues/wiki/projects/packages/releases.
- Apply settings template.
- Apply branch protection template.
- Agent policy template.

Actions:

1. Preview repository creation.
2. Check name availability.
3. Create bare repository.
4. Initialize default branch if requested.
5. Write settings and audit event.
6. Emit `RepositoryCreated` and `CommitPushed` if initialized.
7. Redirect to repo overview.

### 20.2 Import repository flow

Sources:

- Git URL.
- GitHub repo.
- GitLab project.
- Local path if allowed.
- Upload bundle if implemented.

Options:

- Mirror or copy.
- Preserve branches/tags.
- Import issues/MRs if provider supports it.
- Import labels/milestones.
- Import releases.
- Import webhooks disabled by default.
- Map users.
- Run initial analysis: README, language stats, branch protections.

---

---

## 21. Better-than-GitHub/GitLab features

These are differentiators that should be visible in v1, not delayed forever:

1. **Next action everywhere:** every repo/MR/issue has one recommended action with reason and risk.
2. **Explain blockers:** every disabled merge/settings/agent action explains exactly why.
3. **Live event cursor:** user can see if data is live, stale, replaying, or offline.
4. **Repo families:** group repos by naming rules like `veox-*`, with shared dashboards and bulk settings.
5. **Agent evidence panel:** show evidence capsules, policies, VTI decisions, gate verdicts, and exact SHA bindings.
6. **Review cockpit:** comments, checks, evidence, file tree, and merge readiness in one screen.
7. **Settings search:** direct jump to any setting and explain inheritance/defaults.
8. **Safety previews:** every risky action shows side effects, will-not-do list, undo path, and audit output.
9. **Real-time CI/runners:** jobs, logs, runner pressure, cache, and failures stream live.
10. **Markdown correctness:** README and docs render beautifully and safely, with source-aware relative links.

---

---

## 28. Concrete acceptance checklist

The work is done when a user can:

- See all visible repositories.
- Create a new repository.
- Import an existing repository.
- Open a repository overview.
- See README.md rendered correctly as sanitized HTML.
- Browse files by branch.
- View raw files.
- View commits, branches, tags, and compare refs.
- Create a branch.
- Edit a file and commit it.
- Create a merge request.
- Review changed files.
- Comment inline.
- Approve or request changes.
- See required checks and agent evidence.
- Understand why a merge is blocked.
- Merge with exact SHA preview.
- Manage issues, labels, milestones, and assignees.
- Change repository settings.
- Configure branch protection.
- Manage collaborators.
- Configure webhooks.
- Manage secrets without reading back secret values.
- See audit events.
- Receive live updates without refresh.
- Recover from WebSocket reconnect without stale state.
- Navigate with keyboard shortcuts.
- Use command palette to find repos, settings, issues, MRs, and actions.

---

---

## Final Build Contract

This specification is accepted only when all of the following pass:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
npm install
npm run typecheck
npm run test
npm run build
npm run ux-qa
```

The README/Markdown renderer is accepted only when the web UI renders repository `README.md`, relative images, relative links, headings, task lists, tables, code fences, and sanitized embedded HTML correctly, and when malicious Markdown fixtures prove scripts/event handlers/unsafe URLs are removed.
