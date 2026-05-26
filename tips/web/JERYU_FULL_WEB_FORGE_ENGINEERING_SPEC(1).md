# JeRyu Full Web Forge: Rust + Vite + TypeScript + React Engineering Specification

**Deliverable status:** final consolidated engineering spec synthesized from the uploaded proposal bundle and the current `neverhuman/jeryu` repository shape observed on 2026-05-26.

**North star:** JeRyu becomes a full GitHub/GitLab-class web forge with a faster, clearer, real-time, agent-native experience. It must let a user see every repository, create/import/fork/mirror repositories, browse code, render `README.md` and other Markdown safely to HTML, review diffs, approve/merge changes, manage issues/boards/releases/CI, configure every meaningful repository/system setting, and watch all underlying activity live over WebSocket.

---

## 1. Current-state assessment

### 1.1 What exists and should be reused

The current repository is already a serious Rust control plane, not a blank slate. Keep and extend these strengths:

1. **Single Rust binary / control plane.** `jeryu` already owns bootstrap, serve, TUI, Git passthrough, repo fleet, bugs, policy, host, node, MCP, release, secrets, cache, pool, and pipeline commands. The web product must be another first-class surface on the same binary, not a separate backend.
2. **Typed API/read-model foundation.** `src/api` already declares itself as the single source of truth for typed projections, entity types, events, actions, freshness, inspection, proof, snapshots, and runtime profiles. The web BFF should extend this model instead of inventing ad-hoc JSON.
3. **Event-driven control-plane ideas.** `TuiEvent` already has monotonic `seq`, timestamp, kind, severity, entity ref, optional parent, summary, correlation ID, evidence refs, next actions, and stale-after semantics. That is exactly the right base for a replayable browser WebSocket.
4. **Git host adapter layer.** `src/git_host` already has a trait-based host plane, GitHub/GitLab adapter concepts, exact-SHA approval, checks/statuses, comments, live PR state, per-file diffs, and target-policy SHA. Extend it into a full forge provider layer.
5. **Repo fleet / repo standard CLI.** The current repo commands already understand repo init/adopt/mode/hooks/standard/fleet/shadow/backup. The web UI should expose those capabilities visually and add full CRUD/review/settings surfaces.
6. **Settings subsystem.** Settings are already deterministic, load from `~/.jeryu/settings.json`, and default missing keys while ignoring unknown keys. Add web settings there rather than scattering env vars.
7. **TUI operational model.** The TUI has mission-control semantics and live operational tabs. The web surface should reuse the same read models and action preview/execute engine, adding rich browser review and administration workflows.

### 1.2 Current gaps for the requested product

The present `apps/web` workspace is a UX-QA placeholder, not a production web application. It has only a small npm package that runs `ux-qa-check.mjs` against static QA marker files. It lacks Vite, React, routes, TypeScript API client, Markdown rendering, WebSocket state management, repo UI, MR UI, settings UI, and any deployable browser bundle.

The Rust server currently exposes the webhook/API engine routes for `/health`, `/hooks`, and `/cache/summary`. It does not yet mount a full browser BFF under `/api/web/*`, does not serve built static assets, and does not expose a browser WebSocket such as `/ws/activity`.

The existing Git host trait is valuable but still centered on approval/check/diff primitives. A full forge needs repository lifecycle, tree/blob/README rendering, branches, tags, commits, compare, issues, merge requests, reviews, CI, release, webhooks, secrets variables, branch protection, members/roles, notifications, audit log, and settings.

### 1.3 Product gap matrix

| Area | Current | Required final state |
|---|---|---|
| Web app | QA stub | Real Vite + React + TS app, route-level code splitting, command palette, live dock |
| Repo list | CLI fleet | Browser all-repos dashboard with family grouping, filters, bulk actions, live status |
| Repo creation | CLI-oriented | Create/import/fork/mirror UI with protection and agent defaults |
| Code browser | Not browser-native | Tree, blob, blame, history, search, edit, download, copy path, permalink |
| Markdown | README on GitHub only | Server-rendered sanitized HTML with GitHub-like anchors and relative link rewriting |
| Merge review | Host primitives | Full MR/PR room with files, commits, checks, approvals, inline comments, exact-SHA merge |
| Settings | Local JSON + host APIs | Complete user/org/repo/settings matrix with validation and preview |
| Realtime | TUI events | Browser WebSocket with topics, replay, backpressure, optimistic updates |
| Safety | Evidence gates | Preview/execute, RBAC, CSRF, exact SHA, audit, dry-run, risk tiers |
| Type contracts | Rust structs | Rust + generated TS contracts, schema validation, contract tests |

---

## 2. Product principles

### 2.1 Faster than GitHub/GitLab

JeRyu should reduce navigation cost. Common actions must be one command-palette action or one visible button away: create repo, open MR review, approve merge, copy clone URL, run pipeline, retry failed jobs, render README, change branch protection, open live logs, assign reviewer, and toggle agent autonomy.

Performance budgets:

| Surface | Target budget |
|---|---:|
| App shell first usable | < 1.2s local, < 2.0s LAN |
| Route transition | < 150ms after bundle loaded |
| Repo list first page | < 300ms API time |
| Repo search/filter | < 50ms client-side for 5k repos after initial load |
| README render cache hit | < 50ms server time |
| README render cache miss | < 250ms for typical README |
| WebSocket event fanout | < 100ms from control-plane event to browser paint |
| Diff virtualized file switch | < 100ms |

### 2.2 Less confusing than GitHub/GitLab

Every repository page should answer four questions immediately:

1. **What is this repo?** README, description, default branch, latest commit, ownership, health.
2. **What needs attention?** blocked MRs, failing checks, requested reviews, security issues, stale agents.
3. **What can I safely do next?** contextual action rail with preview and risk score.
4. **What is happening right now?** live dock with runs, agents, reviews, settings changes, logs.

### 2.3 Real-time by default

All expensive or long-running views subscribe to topics:

- `global.activity`
- `repo:{repo_id}`
- `repo:{repo_id}:code`
- `repo:{repo_id}:mrs`
- `repo:{repo_id}:issues`
- `repo:{repo_id}:ci`
- `repo:{repo_id}:agents`
- `repo:{repo_id}:settings`
- `user:{user_id}:notifications`
- `admin:audit`

The client must hydrate from REST, then apply WebSocket frames by monotonic sequence. If a gap is detected, it requests replay from `last_seq + 1`; if replay is unavailable, it refetches the affected read model.

### 2.4 Safety over raw power

Every mutating action uses the same pattern:

1. **Preview**: server explains target entities, risk tier, permissions, evidence, dry-run plan, and rollback story.
2. **Confirm**: client shows the high-signal confirmation UI only for high-risk operations.
3. **Execute**: server performs mutation with idempotency key.
4. **Audit**: event and audit rows are written even on failure.
5. **Stream**: WebSocket publishes action lifecycle and affected entities.

### 2.5 Agent-native by default

JeRyu’s advantage is not cloning GitHub; it is exposing agent operations safely:

- active agent sessions on repo cards;
- agent patches as first-class MR sources;
- evidence capsules linked in review;
- autonomy permissions inside settings;
- risk/confidence badges next to merges;
- live agent timeline in the right dock;
- “why is this blocked?” explanations next to disabled buttons.

---

## 3. Target architecture

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ Browser: Vite + React + TypeScript                                          │
│                                                                             │
│  AppShell ── Routes ── Pages ── Components ── Command Palette               │
│      │          │          │          │                                      │
│      │          │          │          ├─ MarkdownHtml / CodeViewer / Diff    │
│      │          │          │          ├─ Settings forms / Review widgets     │
│      │          │          │          └─ LiveDock / Toasts / Notifications   │
│      │          │          │                                                 │
│      ├──────────┴──────────┴── REST client with generated TS types           │
│      └──────────────────────── WebSocket reducer with seq replay             │
└─────────────────────────────────────────────────────────────────────────────┘
                    │ REST `/api/web/*`             │ WS `/ws/activity`
                    ▼                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ Rust single binary: `jeryu serve --web`                                      │
│                                                                             │
│  engine.rs                                                                  │
│    ├─ existing webhook routes: /health, /hooks, /cache/summary               │
│    ├─ web::router: REST API + static Vite dist                               │
│    ├─ web::ws: browser WebSocket with replay/backpressure                    │
│    └─ background loops publish to web::activity::ActivityHub                 │
│                                                                             │
│  src/api                                                                     │
│    ├─ existing TUI read models/events/actions                                │
│    ├─ repository.rs, repo_browser.rs, merge_request.rs, issue.rs             │
│    ├─ web_read_model.rs, settings.rs, websocket.rs                           │
│    └─ generated TS schema exports                                            │
│                                                                             │
│  src/web                                                                     │
│    ├─ auth/session/rbac/csrf                                                 │
│    ├─ REST handlers                                                          │
│    ├─ Markdown renderer + cache                                              │
│    ├─ provider/services layer                                                │
│    ├─ action preview/execute adapter                                         │
│    └─ static file serving                                                    │
│                                                                             │
│  src/git_host                                                                │
│    ├─ GitHost trait expanded to full forge provider                          │
│    ├─ GitLab adapter                                                         │
│    ├─ GitHub adapter                                                         │
│    └─ local Git adapter for offline/dev                                      │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ State and external systems                                                   │
│  SQLite/Postgres state DB · local bare repos · GitLab · GitHub · Vault       │
│  runner pools · cache · evidence store · logs · agent sessions · MCP         │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.1 Backend module boundaries

| Module | Purpose |
|---|---|
| `src/web/router.rs` | Mount REST, WS, static assets, CORS, compression, trace, request IDs |
| `src/web/state.rs` | Shared web state: DB, GitLab client, Docker, activity hub, Markdown cache, settings |
| `src/web/error.rs` | Typed API errors with stable codes and HTTP status mapping |
| `src/web/auth.rs` | Session, current user, RBAC, CSRF, auth extractors |
| `src/web/activity.rs` | In-memory + durable event fanout bridge with sequence replay |
| `src/web/ws.rs` | WebSocket protocol, topic subscription, replay, ping/pong, backpressure |
| `src/web/markdown.rs` | Safe Markdown-to-HTML, anchor slugging, syntax highlighting, cache keys |
| `src/web/rest/*` | HTTP handlers for repos, files, MRs, issues, CI, settings, actions |
| `src/web/services/*` | Domain service layer that composes DB + provider + existing JeRyu modules |
| `src/api/*` | Serializable contracts reused by Rust handlers and generated TS types |

### 3.2 Frontend architecture

Use **Vite + React + TypeScript**. Recommended libraries:

- `@tanstack/react-query` for REST server state and invalidation.
- `@tanstack/react-router` or React Router data routes.
- `zustand` for local UI state and command palette state.
- `@codemirror/view`, `@codemirror/state`, language packages for code viewing/editing.
- `react-virtual` / `@tanstack/react-virtual` for large repo lists and diffs.
- `dompurify` as a client-side belt-and-suspenders sanitizer for server-rendered Markdown HTML.
- `lucide-react` for iconography.
- `cmdk` for command palette.
- `vitest`, `@testing-library/react`, `playwright`, `axe-core` for tests.

---

## 4. Target repository tree diagram

```text
jeryu/
├── Cargo.toml
├── package.json
├── db/
│   ├── state.rs
│   └── migrations/
│       ├── 20260526_web_forge_core.sql
│       ├── 20260526_web_forge_reviews.sql
│       └── 20260526_web_forge_settings_audit.sql
├── docs/
│   ├── WEB_FORGE.md
│   ├── WEB_API.md
│   ├── WEB_SETTINGS.md
│   └── WEB_REALTIME.md
├── src/
│   ├── api/
│   │   ├── mod.rs
│   │   ├── repository.rs
│   │   ├── repo_browser.rs
│   │   ├── merge_request.rs
│   │   ├── issue.rs
│   │   ├── settings.rs
│   │   ├── websocket.rs
│   │   └── web_read_model.rs
│   ├── git_host/
│   │   ├── mod.rs
│   │   ├── github.rs
│   │   ├── gitlab.rs
│   │   ├── local.rs
│   │   └── forge_types.rs
│   ├── web/
│   │   ├── mod.rs
│   │   ├── state.rs
│   │   ├── error.rs
│   │   ├── router.rs
│   │   ├── auth.rs
│   │   ├── activity.rs
│   │   ├── ws.rs
│   │   ├── markdown.rs
│   │   ├── static_assets.rs
│   │   ├── rest/
│   │   │   ├── mod.rs
│   │   │   ├── session.rs
│   │   │   ├── repos.rs
│   │   │   ├── repo_files.rs
│   │   │   ├── commits.rs
│   │   │   ├── branches.rs
│   │   │   ├── merge_requests.rs
│   │   │   ├── reviews.rs
│   │   │   ├── issues.rs
│   │   │   ├── ci.rs
│   │   │   ├── releases.rs
│   │   │   ├── settings.rs
│   │   │   └── actions.rs
│   │   └── services/
│   │       ├── mod.rs
│   │       ├── repo_service.rs
│   │       ├── browser_service.rs
│   │       ├── merge_service.rs
│   │       ├── issue_service.rs
│   │       ├── settings_service.rs
│   │       ├── search_service.rs
│   │       └── notification_service.rs
│   ├── engine.rs
│   ├── lib.rs
│   ├── cli_defs.rs
│   ├── dispatch.rs
│   └── settings_types.rs
├── apps/
│   ├── ux-qa/
│   │   ├── package.json
│   │   ├── ux-qa-check.mjs
│   │   ├── ux-qa.ts
│   │   └── ux-qa.md
│   └── web/
│       ├── package.json
│       ├── index.html
│       ├── vite.config.ts
│       ├── tsconfig.json
│       ├── playwright.config.ts
│       └── src/
│           ├── main.tsx
│           ├── router.tsx
│           ├── generated/
│           │   └── api.ts
│           ├── api/
│           │   ├── client.ts
│           │   ├── queryKeys.ts
│           │   └── mutations.ts
│           ├── realtime/
│           │   ├── socket.ts
│           │   ├── reducer.ts
│           │   └── ActivitySocketProvider.tsx
│           ├── shell/
│           │   ├── AppShell.tsx
│           │   ├── CommandPalette.tsx
│           │   ├── LiveDock.tsx
│           │   ├── Sidebar.tsx
│           │   └── KeyboardShortcuts.tsx
│           ├── components/
│           │   ├── ActionButton.tsx
│           │   ├── ActionPreviewDialog.tsx
│           │   ├── MarkdownHtml.tsx
│           │   ├── ReadmePanel.tsx
│           │   ├── RepoCard.tsx
│           │   ├── RepoPicker.tsx
│           │   ├── DiffViewer.tsx
│           │   ├── FileTree.tsx
│           │   ├── CodeViewer.tsx
│           │   ├── StatusBadge.tsx
│           │   └── SettingsForm.tsx
│           ├── pages/
│           │   ├── DashboardPage.tsx
│           │   ├── ReposPage.tsx
│           │   ├── repo/
│           │   │   ├── RepoLayout.tsx
│           │   │   ├── RepoOverviewPage.tsx
│           │   │   ├── CodePage.tsx
│           │   │   ├── CommitsPage.tsx
│           │   │   ├── BranchesPage.tsx
│           │   │   ├── TagsPage.tsx
│           │   │   ├── MergeRequestsPage.tsx
│           │   │   ├── MergeRequestDetailPage.tsx
│           │   │   ├── IssuesPage.tsx
│           │   │   ├── PipelinesPage.tsx
│           │   │   ├── ReleasesPage.tsx
│           │   │   └── SettingsPage.tsx
│           │   └── admin/
│           │       ├── AuditPage.tsx
│           │       ├── UsersPage.tsx
│           │       └── RunnersPage.tsx
│           ├── styles/
│           │   ├── tokens.css
│           │   └── app.css
│           └── test/
│               ├── setup.ts
│               └── fixtures.ts
└── tests/
    ├── web_markdown.rs
    ├── web_repos_api.rs
    ├── web_ws_replay.rs
    └── web_settings_contract.rs
```

---

## 5. Full user experience

### 5.1 Global app shell

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ JeRyu  ⌘K Search/action...   Repo: all ▾   Live ●   Agent Safe   User ▾      │
├──────────────┬───────────────────────────────────────────────┬───────────────┤
│ Dashboard    │ Main route content                            │ Live Dock     │
│ Repositories │                                               │ Activity      │
│ Merge Room   │                                               │ Checks        │
│ Reviews      │                                               │ Agents        │
│ CI / Runs    │                                               │ Logs          │
│ Agents       │                                               │ Alerts        │
│ Security     │                                               │ Notifications│
│ Settings     │                                               │               │
└──────────────┴───────────────────────────────────────────────┴───────────────┘
```

Global controls:

- `⌘K` / `Ctrl+K`: command palette for navigation and actions.
- `/`: focus route-local search/filter.
- `g r`: repositories.
- `g m`: merge room.
- `g i`: issues.
- `g c`: CI/runs.
- `g s`: settings.
- `[` / `]`: previous/next repository in active filter.
- `j/k` or arrow keys: move selection.
- `Enter`: open/drill down.
- `Esc`: close modal/go up.
- `?`: keyboard help overlay.
- `Shift+R`: refresh current read model.
- `Shift+L`: toggle live dock.
- `Shift+A`: open action rail for selected entity.

### 5.2 All repositories dashboard

Purpose: answer “what exists and what is happening?” across all repos.

Visible components:

- Repo family groups: `veox-*`, `jeryu-*`, org groups, personal repos, archived repos, mirrored repos.
- Table/card toggle.
- Columns: owner/name, family, visibility, default branch, latest commit, open MRs, open issues, failing checks, active agents, last activity, risk, cache pressure, runner pressure.
- Saved filters: blocked merges, needs review, agent active, CI red, stale, private, archived, created this week, no README, unprotected default branch, autonomy enabled.
- Bulk selection: archive, protect branch, mirror, backup, apply repo standard, pause agents, set label, export CSV.
- Quick actions per row: open, copy clone URL, create MR, create issue, run pipeline, settings, pin, star/watch.
- Live pulses: repo row animates on new commit, MR update, check transition, setting change, agent activity.

### 5.3 Repository creation/import/fork/mirror

Create repo wizard sections:

1. Identity: owner/namespace, name, description, visibility, family, topics.
2. Source: empty, README, template, import from URL, fork, mirror, adopt local checkout.
3. Defaults: default branch, license, `.gitignore`, README, initial protected branches.
4. Safety: branch protection, required reviews, required checks, signed commits, secret scanning, agent autonomy profile.
5. CI: starter pipeline, runner pool, cache mode, VTI mode.
6. Integrations: webhooks, mirror remote, GitHub/GitLab provider mapping.
7. Preview: dry-run plan, paths that will be written, remotes that will be added, host APIs that will be called.

### 5.4 Repository overview

Top strip:

- repo name, visibility, clone URLs, default branch, provider, mirror status;
- README/render health;
- merge posture: safe / blocked / unknown;
- CI posture: passing / failing / running;
- agents: idle / running / blocked;
- cache/runners status;
- security posture;
- quick actions.

Main body:

- README HTML panel with file source, branch, commit SHA, render timestamp, copy source, view raw, edit.
- Recent activity timeline.
- Open MRs requiring attention.
- Active runs and failed jobs.
- Agent sessions and evidence.
- Repo metadata: languages, topics, labels, CODEOWNERS, branch protection summary.

### 5.5 Code browser

Controls:

- Branch/tag/commit selector.
- Breadcrumb path navigation.
- Fuzzy file finder.
- Toggle tree/sidebar.
- Copy path, copy permalink, copy raw URL.
- View raw, download, blame, history, edit, create file, upload file, delete file.
- Search in file, search repo.
- Whitespace/wrap/minimap toggles.
- Render Markdown/HTML/SVG preview when safe.
- Large file warning and raw download fallback.
- Binary preview for images/PDF metadata but no unsafe inline execution.

README/Markdown behavior:

- Use server-rendered sanitized HTML.
- Rewrite relative links to JeRyu routes.
- Rewrite relative images through an image proxy endpoint.
- Generate deterministic heading anchors.
- Support GFM tables, task lists, strikethrough, fenced code blocks, footnotes, autolinks.
- Sanitize with a strict allowlist: no scripts, no event handlers, no unsafe protocols, no inline untrusted SVG script execution.
- Cache by `(repo_id, commit_sha, path, renderer_version, sanitizer_version)`.

### 5.6 Merge room / review cockpit

Purpose: replace scattered GitHub/GitLab PR screens with one review cockpit.

Layout:

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ MR #42 fix pool drain race  draft?  head abc123  base main  Safe? BLOCKED    │
├───────────────┬──────────────────────────────────────────────┬───────────────┤
│ Files/Commits │ Diff + inline comments + review threads      │ Decision Rail │
│ Checks        │                                              │ Approvals     │
│ Evidence      │                                              │ Required checks│
│ Timeline      │                                              │ Agent evidence │
│ Reviewers     │                                              │ Merge preview  │
└───────────────┴──────────────────────────────────────────────┴───────────────┘
```

Controls:

- File tree with changed/added/deleted/renamed/copied/binary indicators.
- Viewed checkbox per file.
- Inline comment, suggestion, batch review, resolve thread.
- Whitespace ignore, split/unified diff, word diff, hide generated files.
- Commit list with verified signature and CI status.
- Checks panel with rerun failed/rerun all/cancel.
- Evidence panel: VTI decision, agent capsules, risk/confidence, policy SHA, CODEOWNERS.
- Approval controls: approve, request changes, comment, assign reviewer, request agent re-review.
- Merge controls: squash, merge commit, rebase, auto-merge, delete source branch, update branch.
- Exact-SHA binding: merge/approve disabled if `head_sha` changed since preview.
- “Why blocked?” explanation from existing decision/action systems.

### 5.7 Issues and project management

Issues controls:

- List/table/board/roadmap views.
- Filters: status, labels, assignee, milestone, linked MR, stale, priority, severity, component, agent-owned.
- Bulk edit: labels, assignee, milestone, close/reopen, priority.
- Issue detail: Markdown description, comments, tasks, linked branches/MRs/commits, agent attempts, timeline.
- Bug tracker integration: map JeRyu bug projects to issues; sync provider issues; show rank/readiness.
- Saved queries and keyboard triage.

Project controls:

- Kanban board with swimlanes by repo family, assignee, priority, agent status.
- Roadmap milestones with burndown.
- Dependency graph between issues/MRs/repos.

### 5.8 CI/CD, runners, tests, release

CI controls:

- Pipeline list with branch/MR/commit filters.
- Pipeline graph with dependencies and child pipelines.
- Job detail with live logs, artifacts, retry, cancel, terminal if allowed.
- VTI panel: selected/skipped/accelerated tests, confidence, selector misses, unknown paths, time saved.
- Cache panel: hit/miss/taint/denied, trust tier, GC plan.
- Runner pool panel: scale, pause, drain, rotate token, assign repo, capacity warnings.
- Release panel: candidates, gates, canaries, promotion, rollback, artifact publication, evidence.

### 5.9 Settings workspace

Settings must be searchable, grouped, and validated. Every setting row should show:

- setting name;
- current value;
- inherited value if applicable;
- source of truth: local JeRyu, GitLab, GitHub, repo file, environment;
- risk tier;
- validation result;
- last changed by / timestamp;
- reset-to-default / copy JSON pointer / open docs.

---

## 6. Settings inventory

### 6.1 User/UI settings

| Setting | Controls |
|---|---|
| Theme | system/light/dark/high contrast |
| Density | comfortable/compact/ultra-compact |
| Motion | normal/reduced/off |
| Font | UI font size, code font size, line height |
| Keyboard mode | GitHub-like, Vim-like, JeRyu power mode |
| Default landing | dashboard/repos/merge room/last repo |
| Live dock | default open, width, topic filters, severity threshold |
| Notifications | desktop, sound, email bridge, mention-only, quiet hours |
| Diff defaults | split/unified, whitespace, generated files, wrap, viewed tracking |
| Code defaults | branch, line numbers, minimap, syntax theme, tab width |

### 6.2 System/admin settings

| Setting | Controls |
|---|---|
| Web bind | host:port, public base URL, TLS mode, trusted proxy |
| Session | cookie name, TTL, idle timeout, secure/same-site, rotation |
| CSRF | enabled, header name, excluded paths |
| CORS | allowed origins for dev/prod |
| Static assets | Vite dist path, cache headers, SPA fallback |
| WebSocket | max clients, max topics/client, replay window, heartbeat, backpressure |
| Audit | retention, export target, redact policy |
| Markdown | cache size, max bytes, image proxy, Mermaid allowlist |
| Search | index path, incremental refresh interval |
| Rate limits | per-user, per-IP, per-action, burst |

### 6.3 Organization/namespace settings

| Setting | Controls |
|---|---|
| Namespace identity | name, slug, description, avatar |
| Members | invite, remove, role, team mapping |
| Default repo policy | visibility, branch protection, merge rules, CI template |
| Repo families | prefix patterns, color/icon, default owners, dashboards |
| Shared secrets | scoped variables, Vault path, rotation rules |
| Webhooks | namespace-level hooks and event filters |
| Compliance | required signed commits, DCO, CODEOWNERS, review threshold |

### 6.4 Repository general settings

| Setting | Controls |
|---|---|
| Identity | name, description, homepage, topics, avatar |
| Visibility | private/internal/public |
| Default branch | branch selector, protected default enforcement |
| Features | issues, wiki, releases, packages, snippets, discussions, projects |
| Archive/delete | archive, unarchive, transfer, delete with strong confirmation |
| Clone/remotes | HTTP/SSH clone URLs, mirror remotes, shadow remote, backup remote |
| Templates | issue templates, MR templates, branch naming rules |

### 6.5 Merge/review settings

| Setting | Controls |
|---|---|
| Merge methods | merge commit, squash, rebase, fast-forward |
| Auto-merge | enabled, conditions, queue strategy |
| Required approvals | count, codeowner approvals, security approval, agent approval |
| Stale approvals | dismiss on push, dismiss on target branch change |
| Threads | require resolved discussions |
| Status checks | required checks, gate check, VTI confidence threshold |
| Exact SHA | require preview SHA = current head SHA |
| Source branch | auto-delete, allow maintainer edits |
| Merge queue | concurrency, priority, batching, speculative checks |

### 6.6 Branch protection settings

| Setting | Controls |
|---|---|
| Protected branches | pattern, exact branch, wildcard |
| Push rules | allowed roles/users/teams, force-push, delete |
| Required checks | check names, strict up-to-date, flaky retry policy |
| Required signatures | signed commits, verified authors |
| Linear history | enforce/no |
| Lock branch | lock/unlock with reason |
| Agent relay | allow JeRyu-only main relay after approved evidence |

### 6.7 CI/CD settings

| Setting | Controls |
|---|---|
| Pipeline enablement | enable/disable, default branch only, MR pipelines |
| Runner pools | default pool, tags, capacity, cost cap |
| Variables/secrets | env vars, masked/protected, Vault-backed, rotation |
| Cache | strategy, budget, taint policy, registry mirror |
| Artifacts | retention, public/private, max size |
| VTI | smart test selection, min confidence, escalation rules |
| Schedules | cron, timezone, branch, variables |
| Webhooks | pipeline events, job events, deployment events |

### 6.8 Agent/autonomy settings

| Setting | Controls |
|---|---|
| Agent sessions | enabled, allowed agents, max concurrent |
| Autonomy profile | off/advisory/guarded/sovereign-plus |
| Allowed actions | create branch, push, open MR, comment, approve, merge, deploy |
| Risk gates | require human above risk tier, confidence threshold |
| Evidence | required capsule types, retention, public/private |
| Secrets | grant scopes, TTL, approval, denylist |
| Race mode | allow multiple agents, winner selection, evidence comparison |
| Auto-rejudge | on force-push, policy drift, target branch update |

### 6.9 Security/compliance settings

| Setting | Controls |
|---|---|
| Secret scanning | enabled, block push, patterns, exemptions |
| Dependency scanning | enabled, severity threshold |
| Code scanning | enabled, required checks |
| Audit log | event classes, retention, export |
| Webhook secrets | rotate, reveal-once, last used |
| Access tokens | list, revoke, scope, expiry |
| IP allowlist | CIDRs, bypass for local |
| SSO/MFA | required, session duration |

---

## 7. REST API surface

All browser routes live under `/api/web/v1`. Use stable error envelopes:

```json
{
  "error": {
    "code": "repo.not_found",
    "message": "Repository not found",
    "request_id": "req_...",
    "details": {}
  }
}
```

### 7.1 Session and bootstrap

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/web/v1/bootstrap` | current user, settings, feature flags, initial activity seq |
| `GET` | `/api/web/v1/session` | current session/user |
| `POST` | `/api/web/v1/session/logout` | logout |
| `GET` | `/api/web/v1/command-palette` | searchable commands/actions for current context |

### 7.2 Repositories

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/web/v1/repos` | list repos, filters, families, pagination |
| `POST` | `/api/web/v1/repos/preview` | preview create/import/fork/mirror/adopt |
| `POST` | `/api/web/v1/repos` | execute repo create/import/fork/mirror/adopt |
| `GET` | `/api/web/v1/repos/{repo_id}` | repo overview |
| `PATCH` | `/api/web/v1/repos/{repo_id}` | update name/description/topics/features |
| `POST` | `/api/web/v1/repos/{repo_id}/archive` | archive/unarchive preview+execute |
| `DELETE` | `/api/web/v1/repos/{repo_id}` | delete repo with strong confirmation |

### 7.3 Code, Markdown, commits, branches

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/web/v1/repos/{repo_id}/tree` | file tree at ref/path |
| `GET` | `/api/web/v1/repos/{repo_id}/blob` | blob metadata/content |
| `GET` | `/api/web/v1/repos/{repo_id}/raw/{*path}` | raw file |
| `GET` | `/api/web/v1/repos/{repo_id}/readme` | best README with sanitized HTML |
| `POST` | `/api/web/v1/repos/{repo_id}/markdown/render` | render arbitrary repo markdown |
| `GET` | `/api/web/v1/repos/{repo_id}/commits` | commit list |
| `GET` | `/api/web/v1/repos/{repo_id}/commits/{sha}` | commit detail |
| `GET` | `/api/web/v1/repos/{repo_id}/compare` | compare refs |
| `GET` | `/api/web/v1/repos/{repo_id}/branches` | branch list |
| `POST` | `/api/web/v1/repos/{repo_id}/branches` | create branch |
| `PATCH` | `/api/web/v1/repos/{repo_id}/branches/{branch}` | protect/unprotect/update |
| `GET` | `/api/web/v1/repos/{repo_id}/tags` | tag list |
| `POST` | `/api/web/v1/repos/{repo_id}/tags` | create tag/release tag |

### 7.4 Merge requests / pull requests

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/web/v1/repos/{repo_id}/mrs` | list MRs/PRs |
| `POST` | `/api/web/v1/repos/{repo_id}/mrs` | create MR |
| `GET` | `/api/web/v1/repos/{repo_id}/mrs/{mr_id}` | MR overview |
| `GET` | `/api/web/v1/repos/{repo_id}/mrs/{mr_id}/diff` | paginated diff |
| `GET` | `/api/web/v1/repos/{repo_id}/mrs/{mr_id}/timeline` | events/comments/reviews |
| `POST` | `/api/web/v1/repos/{repo_id}/mrs/{mr_id}/comments` | general/inline comment |
| `POST` | `/api/web/v1/repos/{repo_id}/mrs/{mr_id}/reviews` | submit review |
| `POST` | `/api/web/v1/repos/{repo_id}/mrs/{mr_id}/approve/preview` | exact-SHA approval preview |
| `POST` | `/api/web/v1/repos/{repo_id}/mrs/{mr_id}/approve` | approve |
| `POST` | `/api/web/v1/repos/{repo_id}/mrs/{mr_id}/merge/preview` | merge preview |
| `POST` | `/api/web/v1/repos/{repo_id}/mrs/{mr_id}/merge` | merge with exact SHA |
| `POST` | `/api/web/v1/repos/{repo_id}/mrs/{mr_id}/update-branch` | update source branch |

### 7.5 Issues, projects, labels, milestones

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/web/v1/repos/{repo_id}/issues` | list/filter issues |
| `POST` | `/api/web/v1/repos/{repo_id}/issues` | create issue |
| `GET` | `/api/web/v1/repos/{repo_id}/issues/{issue_id}` | issue detail |
| `PATCH` | `/api/web/v1/repos/{repo_id}/issues/{issue_id}` | update/triage issue |
| `POST` | `/api/web/v1/repos/{repo_id}/issues/{issue_id}/comments` | comment |
| `GET` | `/api/web/v1/repos/{repo_id}/labels` | labels |
| `POST` | `/api/web/v1/repos/{repo_id}/labels` | create label |
| `GET` | `/api/web/v1/repos/{repo_id}/milestones` | milestones |
| `GET` | `/api/web/v1/projects` | project boards/roadmaps |

### 7.6 CI/CD, agents, release

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/web/v1/repos/{repo_id}/pipelines` | pipeline list |
| `GET` | `/api/web/v1/repos/{repo_id}/pipelines/{pipeline_id}` | pipeline graph/detail |
| `POST` | `/api/web/v1/repos/{repo_id}/pipelines` | run pipeline |
| `POST` | `/api/web/v1/repos/{repo_id}/jobs/{job_id}/retry` | retry job |
| `POST` | `/api/web/v1/repos/{repo_id}/jobs/{job_id}/cancel` | cancel job |
| `GET` | `/api/web/v1/repos/{repo_id}/jobs/{job_id}/logs` | log tail / chunks |
| `GET` | `/api/web/v1/repos/{repo_id}/tests/vti` | VTI read model |
| `GET` | `/api/web/v1/repos/{repo_id}/agents` | active agent sessions |
| `POST` | `/api/web/v1/repos/{repo_id}/agents/{session_id}/pause` | pause agent |
| `GET` | `/api/web/v1/repos/{repo_id}/releases` | releases |
| `POST` | `/api/web/v1/repos/{repo_id}/releases/{release_id}/promote/preview` | release promotion preview |

### 7.7 Settings and actions

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/web/v1/settings` | effective global/user settings |
| `PATCH` | `/api/web/v1/settings` | update user/global settings |
| `GET` | `/api/web/v1/repos/{repo_id}/settings` | effective repo settings |
| `PATCH` | `/api/web/v1/repos/{repo_id}/settings` | update repo settings |
| `POST` | `/api/web/v1/actions/preview` | generic action preview |
| `POST` | `/api/web/v1/actions/execute` | generic action execute |
| `GET` | `/api/web/v1/audit` | audit log |
| `GET` | `/api/web/v1/notifications` | notifications |
| `PATCH` | `/api/web/v1/notifications/{id}` | mark read/snooze |

---

## 8. WebSocket protocol

### 8.1 Endpoint

`GET /ws/activity?since=<seq>&topics=global.activity,repo:123:mrs`

Authenticate via the same browser session cookie. Non-browser clients may use a scoped token if explicitly enabled.

### 8.2 Client frames

```json
{ "type": "hello", "client_id": "browser-uuid", "last_seq": 123, "topics": ["global.activity"] }
{ "type": "subscribe", "topics": ["repo:123", "repo:123:mrs"] }
{ "type": "unsubscribe", "topics": ["repo:123:mrs"] }
{ "type": "ack", "seq": 140 }
{ "type": "ping", "nonce": "..." }
```

### 8.3 Server frames

```json
{ "type": "welcome", "server_time": "2026-05-26T16:00:00Z", "replay_from": 124, "heartbeat_ms": 25000 }
{ "type": "event", "seq": 141, "topic": "repo:123:mrs", "event": { } }
{ "type": "read_model_patch", "seq": 142, "topic": "repo:123", "entity": { "kind": "repository", "id": "123" }, "patch": [] }
{ "type": "gap", "from": 125, "to": 130, "reason": "replay_window_expired" }
{ "type": "pong", "nonce": "..." }
{ "type": "error", "code": "topic.denied", "message": "Not allowed" }
```

### 8.4 Reliability requirements

- Every event has a durable monotonic sequence.
- Server keeps an in-memory replay ring plus durable audit/event table for critical events.
- Client tracks `lastAppliedSeq` in memory and `localStorage` per user/session.
- If `seq != lastAppliedSeq + 1`, client pauses incremental updates and asks for replay.
- If replay is denied or expired, client invalidates affected React Query keys and refetches.
- Backpressure policy: coalesce read-model patches, never drop audit/security/action lifecycle events.
- Heartbeat default: 25 seconds. Disconnect after two missed heartbeats.

---

## 9. Markdown-to-HTML rendering spec

### 9.1 Required behavior

The renderer must support the README and common Markdown views with GitHub-like output while staying safe.

Features:

- CommonMark + GFM tables/task lists/strikethrough/autolinks.
- Fenced code blocks with language class and optional server-side syntax highlighting.
- Heading slug anchors.
- Relative links rewritten to JeRyu repo routes.
- Relative images proxied through safe image endpoint.
- HTML input sanitized, not blindly trusted.
- Mermaid disabled by default; can be enabled with strict iframe/SVG sandbox policy later.
- Emoji shortcodes optional.
- Cache by content SHA and renderer/sanitizer versions.

### 9.2 Rust renderer pipeline

1. Resolve file and ref through provider/local Git.
2. Enforce byte limit and encoding detection.
3. Parse Markdown with `pulldown-cmark` or `comrak` configured for GFM features.
4. Generate HTML.
5. Sanitize with `ammonia` using explicit allowlist.
6. Rewrite anchors, links, and images using repo/ref/path context.
7. Store cache row: `repo_id`, `commit_sha`, `path`, `source_sha256`, `html_sha256`, `renderer_version`, `sanitizer_version`, `rendered_at`.
8. Return `RenderedMarkdown` with HTML, headings, links, render warnings, source metadata.

### 9.3 Security allowlist

Allowed tags include: `a`, `p`, `pre`, `code`, `span`, `blockquote`, `ul`, `ol`, `li`, `table`, `thead`, `tbody`, `tr`, `th`, `td`, `h1`-`h6`, `img`, `hr`, `br`, `strong`, `em`, `del`, `details`, `summary`.

Forbidden always: `script`, `iframe` by default, `object`, `embed`, `form`, event handler attributes, `javascript:` URLs, unproxied external SVG execution.

### 9.4 Frontend Markdown component

`MarkdownHtml` receives sanitized HTML from the server. It may run DOMPurify again as defense in depth. It must:

- preserve heading anchors;
- intercept relative route clicks and use client navigation;
- lazy-load proxied images;
- show render warnings;
- provide “view source”, “copy link to heading”, and “open raw” actions.

---

## 10. Data model additions

### 10.1 Core tables

```sql
CREATE TABLE web_repositories (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  provider_id TEXT,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  slug TEXT NOT NULL UNIQUE,
  family TEXT,
  description TEXT,
  visibility TEXT NOT NULL,
  default_branch TEXT NOT NULL DEFAULT 'main',
  local_path TEXT,
  bare_path TEXT,
  clone_http_url TEXT,
  clone_ssh_url TEXT,
  readme_path TEXT,
  archived BOOLEAN NOT NULL DEFAULT FALSE,
  disabled BOOLEAN NOT NULL DEFAULT FALSE,
  last_activity_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE web_repo_settings (
  repo_id TEXT NOT NULL,
  section TEXT NOT NULL,
  key TEXT NOT NULL,
  value_json TEXT NOT NULL,
  inherited_from TEXT,
  updated_by TEXT,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (repo_id, section, key)
);

CREATE TABLE web_markdown_cache (
  repo_id TEXT NOT NULL,
  commit_sha TEXT NOT NULL,
  path TEXT NOT NULL,
  source_sha256 TEXT NOT NULL,
  renderer_version TEXT NOT NULL,
  sanitizer_version TEXT NOT NULL,
  html TEXT NOT NULL,
  headings_json TEXT NOT NULL,
  links_json TEXT NOT NULL,
  warnings_json TEXT NOT NULL,
  rendered_at TEXT NOT NULL,
  PRIMARY KEY (repo_id, commit_sha, path, renderer_version, sanitizer_version)
);
```

### 10.2 Review and issue tables

```sql
CREATE TABLE web_merge_requests (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL,
  provider_iid TEXT,
  title TEXT NOT NULL,
  description TEXT,
  source_branch TEXT NOT NULL,
  target_branch TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  base_sha TEXT,
  author TEXT,
  state TEXT NOT NULL,
  draft BOOLEAN NOT NULL DEFAULT FALSE,
  merge_status TEXT,
  risk_score REAL,
  confidence REAL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE web_review_threads (
  id TEXT PRIMARY KEY,
  mr_id TEXT NOT NULL,
  file_path TEXT,
  old_line INTEGER,
  new_line INTEGER,
  resolved BOOLEAN NOT NULL DEFAULT FALSE,
  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE web_review_comments (
  id TEXT PRIMARY KEY,
  thread_id TEXT,
  mr_id TEXT NOT NULL,
  body_markdown TEXT NOT NULL,
  body_html TEXT,
  author TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE web_issues (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL,
  provider_iid TEXT,
  title TEXT NOT NULL,
  body_markdown TEXT,
  body_html TEXT,
  state TEXT NOT NULL,
  priority TEXT,
  severity TEXT,
  component TEXT,
  author TEXT,
  assignee TEXT,
  labels_json TEXT NOT NULL DEFAULT '[]',
  milestone TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

### 10.3 Activity, notifications, audit

```sql
CREATE TABLE web_activity_events (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  topic TEXT NOT NULL,
  event_json TEXT NOT NULL,
  severity TEXT NOT NULL,
  entity_kind TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  correlation_id TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE web_notifications (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  topic TEXT NOT NULL,
  title TEXT NOT NULL,
  body TEXT,
  entity_kind TEXT,
  entity_id TEXT,
  read_at TEXT,
  snoozed_until TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE web_audit_log (
  id TEXT PRIMARY KEY,
  actor TEXT NOT NULL,
  action TEXT NOT NULL,
  target_kind TEXT NOT NULL,
  target_id TEXT NOT NULL,
  request_id TEXT,
  risk_tier TEXT,
  preview_json TEXT,
  result_json TEXT,
  created_at TEXT NOT NULL
);
```

---

## 11. Rust API contracts

### 11.1 Repository model

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryVisibility {
    Private,
    Internal,
    Public,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RepositorySummary {
    pub id: String,
    pub provider: String,
    pub owner: String,
    pub name: String,
    pub slug: String,
    pub family: Option<String>,
    pub description: Option<String>,
    pub visibility: RepositoryVisibility,
    pub default_branch: String,
    pub archived: bool,
    pub latest_commit: Option<CommitSummary>,
    pub open_merge_requests: u32,
    pub open_issues: u32,
    pub failing_checks: u32,
    pub active_agents: u32,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub health: HealthLevel,
    pub risk_flags: Vec<String>,
}
```

### 11.2 Repo browser model

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RepoTreeEntry {
    pub name: String,
    pub path: String,
    pub kind: RepoEntryKind,
    pub mode: String,
    pub size: Option<u64>,
    pub sha: String,
    pub last_commit: Option<CommitSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RenderedMarkdown {
    pub repo_id: String,
    pub ref_name: String,
    pub commit_sha: String,
    pub path: String,
    pub source_sha256: String,
    pub html: String,
    pub headings: Vec<MarkdownHeading>,
    pub links: Vec<MarkdownLink>,
    pub warnings: Vec<MarkdownWarning>,
    pub rendered_at: DateTime<Utc>,
}
```

### 11.3 Merge request model

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MergeRequestDetail {
    pub id: String,
    pub repo_id: String,
    pub iid: String,
    pub title: String,
    pub description_markdown: String,
    pub description_html: String,
    pub state: MergeRequestState,
    pub draft: bool,
    pub source_branch: String,
    pub target_branch: String,
    pub head_sha: String,
    pub base_sha: Option<String>,
    pub author: UserSummary,
    pub reviewers: Vec<UserSummary>,
    pub labels: Vec<String>,
    pub checks: Vec<CheckSummary>,
    pub approvals: Vec<ApprovalSummary>,
    pub threads: Vec<ReviewThreadSummary>,
    pub evidence_refs: Vec<String>,
    pub mergeability: MergeabilitySummary,
    pub suggested_actions: Vec<ActionRef>,
}
```

### 11.4 Settings model

Settings responses should be typed but flexible enough for provider-specific values:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SettingValue<T> {
    pub value: T,
    pub inherited_from: Option<String>,
    pub source: SettingSource,
    pub risk_tier: RiskTier,
    pub last_changed_by: Option<String>,
    pub last_changed_at: Option<DateTime<Utc>>,
    pub validation: SettingValidation,
}
```

---

## 12. Provider / Git host expansion

The existing `GitHost` trait should be split or extended into capability groups so adapters can honestly report support:

- `RepositoryProvider`: list/get/create/import/fork/mirror/archive/delete repos.
- `ContentProvider`: tree/blob/raw/README/commit history/blame.
- `ReviewProvider`: MR/PR list/detail/diff/comments/reviews/approve/merge/update branch.
- `IssueProvider`: issues/comments/labels/milestones/project boards.
- `CiProvider`: pipelines/jobs/logs/artifacts/retry/cancel.
- `SettingsProvider`: branch protection, merge rules, hooks, variables, access.
- `SearchProvider`: repo/code/issues/MRs search.

Each provider method should return `HostError::NotImplemented` when unsupported; the UI should show disabled controls with “not supported by provider” explanations.

---

## 13. Frontend state strategy

### 13.1 Server state

Use React Query:

- each REST route has a stable query key;
- mutations call preview first unless explicitly marked safe;
- mutation success invalidates affected keys and emits optimistic patches when possible;
- WebSocket reducer applies targeted query cache patches.

### 13.2 Local UI state

Use Zustand or lightweight React context for:

- app shell layout;
- selected repo family;
- active filters;
- command palette;
- live dock topics;
- keyboard mode;
- unsaved settings form drafts;
- diff viewed state before persistence.

### 13.3 WebSocket reducer

Reducer rules:

1. Ignore events older than or equal to `lastAppliedSeq`.
2. If next seq is missing, set status `recovering` and request replay.
3. Apply entity-specific events to cache by type.
4. Coalesce noisy log frames.
5. Never drop security/action/audit frames.
6. Surface high-severity events as toasts and live dock entries.

---

## 14. Design system

### 14.1 Visual goals

- Dense but readable.
- Clear status color and icon semantics.
- One obvious next action per context.
- Live activity should be visible but not distracting.
- Keyboard-first navigation.
- Accessible contrast and focus rings.

### 14.2 Core components

| Component | Purpose |
|---|---|
| `AppShell` | global layout, nav, live dock |
| `CommandPalette` | global navigation and actions |
| `ActionButton` | preview/execute action wrapper |
| `ActionPreviewDialog` | risk, plan, confirm, execute |
| `RepoCard` / `RepoTable` | all repos dashboard |
| `ReadmePanel` | sanitized Markdown HTML + controls |
| `FileTree` | virtualized tree |
| `CodeViewer` | syntax, permalink, search |
| `DiffViewer` | virtualized split/unified diff |
| `MergeDecisionRail` | mergeability, checks, approvals, evidence |
| `SettingsForm` | typed settings rows and validation |
| `LiveDock` | activity stream, checks, agents, logs |

---

## 15. Full feature coverage checklist

### 15.1 Repositories

- [ ] all repos dashboard
- [ ] repo families
- [ ] saved filters
- [ ] create repo
- [ ] import repo
- [ ] fork repo
- [ ] mirror repo
- [ ] adopt local checkout
- [ ] archive/unarchive
- [ ] transfer/delete
- [ ] clone URLs
- [ ] topics/language metadata
- [ ] repo health and risk flags

### 15.2 Code

- [ ] tree browser
- [ ] blob viewer
- [ ] raw file
- [ ] Markdown render
- [ ] README selection
- [ ] edit/create/delete/upload file
- [ ] commit browser
- [ ] branch/tag selector
- [ ] blame/history
- [ ] compare refs
- [ ] code search
- [ ] permalink/copy path

### 15.3 Merge/review

- [ ] MR list
- [ ] MR create
- [ ] MR detail
- [ ] diff viewer
- [ ] inline comments
- [ ] review threads
- [ ] approve/request changes/comment
- [ ] checks/evidence panel
- [ ] exact-SHA merge preview
- [ ] merge/squash/rebase
- [ ] auto-merge/merge queue
- [ ] update branch
- [ ] delete source branch

### 15.4 Issues/projects

- [ ] issue list/detail/create/edit
- [ ] labels/milestones
- [ ] comments
- [ ] saved filters
- [ ] bulk triage
- [ ] project board
- [ ] roadmap
- [ ] bug tracker sync

### 15.5 CI/CD and operations

- [ ] pipeline list/detail/graph
- [ ] live job logs
- [ ] retry/cancel jobs
- [ ] artifacts
- [ ] VTI test intelligence
- [ ] cache status/taint/GC
- [ ] runner pools scale/pause/drain
- [ ] releases/promote/rollback
- [ ] secrets lifecycle

### 15.6 Settings/admin/security

- [ ] user settings
- [ ] repo settings
- [ ] org/namespace settings
- [ ] branch protection
- [ ] merge rules
- [ ] webhooks/integrations
- [ ] CI variables/secrets
- [ ] members/roles
- [ ] audit log
- [ ] notifications
- [ ] security policies

### 15.7 JeRyu-specific advantages

- [ ] live agent sessions
- [ ] evidence capsules in review
- [ ] action preview and risk tier everywhere
- [ ] explain blockers
- [ ] VTI confidence and time saved
- [ ] cache trust/taint panel
- [ ] repo fleet/family dashboards
- [ ] exact-SHA governance
- [ ] policy SHA drift detection
- [ ] MCP-compatible action surface

---

## 16. Implementation plan

### Phase 0 — contracts and scaffolding

- Move current `apps/web` QA package to `apps/ux-qa`.
- Create real `apps/web` Vite app.
- Add Rust web dependencies.
- Add `src/web` module skeleton.
- Add API contracts and TS generation.
- Add settings structs and defaults.
- Add DB migrations.

Exit criteria:

- `npm run build` builds the app shell.
- `cargo check -p jeryu` passes.
- `/api/web/v1/bootstrap` returns session/feature flags.
- `/ws/activity` accepts connection and sends welcome.

### Phase 1 — all repos + repo home + README

- Implement repo list read model from repo fleet/provider.
- Implement create/import/adopt preview and execute.
- Implement repo overview.
- Implement tree/blob/readme endpoints.
- Implement Markdown renderer and cache.
- Implement Repos page, Repo overview, Code page, Readme panel.

Exit criteria:

- User can see all repos.
- User can create/import/adopt repo.
- User can open a repo and see sanitized README HTML.
- Relative README links route correctly.

### Phase 2 — merge room/review

- Extend provider review APIs.
- Implement MR list/detail/diff/timeline.
- Implement inline comments/reviews.
- Implement approve/merge preview and exact-SHA execute.
- Build merge room UI.

Exit criteria:

- User can review files, comment, approve, and merge with exact-SHA safety.
- Push during review invalidates stale preview.

### Phase 3 — issues/projects

- Implement issue list/detail/create/edit/comment/triage.
- Implement labels/milestones.
- Integrate existing bug tracker surfaces.
- Build board/roadmap basics.

Exit criteria:

- User can manage issues and link them to MRs/bugs.

### Phase 4 — CI/release/agents/live operations

- Implement pipelines/jobs/logs/artifacts.
- Implement VTI/cache/runner read models.
- Implement agent session panels.
- Build live dock topics and event reducers.
- Implement release promotion/rollback previews.

Exit criteria:

- User can watch CI/agents live and operate common actions safely.

### Phase 5 — settings/admin/security

- Implement complete settings forms and validation.
- Implement branch protection/merge rules/secrets/webhooks/members.
- Implement audit/notifications.
- Add admin dashboards.

Exit criteria:

- All meaningful GitHub/GitLab repo settings are visible, searchable, validated, and actionable.

### Phase 6 — polish/performance/proof

- Virtualize huge lists/diffs.
- Add Storybook coverage.
- Add Playwright E2E.
- Add accessibility tests.
- Add load tests for WebSocket and repo lists.
- Add release docs.

---

## 17. Validation strategy

### 17.1 Rust tests

- Markdown sanitizer rejects XSS fixtures.
- Markdown renderer rewrites relative links/images correctly.
- Repo API pagination/filtering is stable.
- Settings patch validation rejects invalid branch protection/merge rules.
- WebSocket replay detects gaps.
- Action preview/execute is idempotent.
- Exact-SHA merge fails on stale head.
- Provider unsupported capability returns stable error.

### 17.2 Frontend tests

- Route smoke tests.
- App shell keyboard shortcuts.
- Command palette actions.
- Repo filters and bulk selection.
- README rendering and heading anchors.
- Diff viewer virtualization and comments.
- Settings forms dirty state and validation.
- WebSocket reducer replay/gap cases.

### 17.3 E2E tests

- Create repo → render README → edit file → open MR → review → merge.
- Import repo → apply branch protection → run pipeline.
- Force-push during review → stale merge preview blocked.
- Agent opens MR → evidence appears → human approves.
- WebSocket disconnect/reconnect with replay.

### 17.4 Security tests

- CSRF blocks mutating requests without token.
- Session cookie flags correct in prod.
- Markdown XSS fixture corpus.
- Permission-denied surfaces no secret details.
- Audit log generated for all mutations.
- Rate limiting for action execution.

---

## 18. Acceptance criteria

The feature is done when:

1. `jeryu serve --web` starts the existing engine and the web app from one binary.
2. Browser users can see every registered/provider repository.
3. Browser users can create/import/adopt repositories with preview and audit.
4. Repo home renders `README.md` as sanitized HTML with correct links/anchors.
5. Code browser supports tree/blob/raw/history/basic edit controls.
6. Merge room supports diff review, comments, approvals, exact-SHA merge preview, and merge execution.
7. Settings UI covers user, system, org/namespace, repo, branch protection, merge rules, CI/CD, agents, security, and integrations.
8. WebSocket updates repo, MR, CI, agent, settings, audit, and notification views live.
9. Mutating actions use preview/execute, idempotency keys, permission checks, and audit events.
10. Rust and frontend contract tests prove API stability.
11. Accessibility, keyboard navigation, and reduced-motion modes are implemented.
12. Performance budgets from Section 2 are met on representative data.

---

## 19. PR sequence

1. **PR 1: Workspace split and Vite shell.** Move UX QA to `apps/ux-qa`; create `apps/web`; add build scripts.
2. **PR 2: Rust web module and bootstrap.** Add `src/web`, settings, static serving, bootstrap endpoint.
3. **PR 3: Contracts and TS generation.** Add `src/api` web contracts and generated TS types.
4. **PR 4: Activity WebSocket.** Add activity hub, replay, client reducer, live dock.
5. **PR 5: Repositories dashboard.** List/filter/group repos; repo creation preview/execute.
6. **PR 6: Code browser and Markdown.** Tree/blob/raw/README render/cache.
7. **PR 7: Merge room.** MR APIs, diff viewer, comments, approvals, exact-SHA merge.
8. **PR 8: Issues/projects.** Issue APIs and bug tracker integration.
9. **PR 9: CI/release/agents.** Pipelines, logs, VTI, cache, runner pools, release, agents.
10. **PR 10: Settings/admin/security.** Full settings matrix, audit, notifications, access.
11. **PR 11: Performance and proof.** Virtualization, E2E, accessibility, load tests, docs.

---

## 20. Non-goals for the first production cut

These are not required for the first complete web forge but should be designed for:

- replacing Git wire protocol hosting itself;
- full package registry UI parity;
- browser IDE/Codespaces clone;
- enterprise SSO integration beyond token/session scaffolding;
- public multi-tenant SaaS hardening;
- full wiki/discussions parity unless explicitly enabled later.

