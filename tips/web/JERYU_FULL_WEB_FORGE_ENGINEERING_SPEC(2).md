# JeRyu Full Web Forge — Final Engineering Specification

**Date:** 2026-05-26  
**Target stack:** Rust + Axum + SQLite/RedlineDB-ready persistence + Vite + TypeScript + React  
**Primary goal:** turn JeRyu into a full modern Git forge experience with GitHub/GitLab parity plus faster navigation, clearer merge safety, real-time activity, excellent repository settings, and first-class README/Markdown rendering.

---

## 1. Executive summary

JeRyu already has the hardest part of a modern forge: a Rust control plane, typed API/read-model concepts, Git/GitLab/GitHub adapter foundations, CI orchestration, agent workflows, cache/runtime state, TUI telemetry, and a proof-oriented engineering culture. The current browser workspace, however, is only a UX-QA placeholder. The final architecture should **not** build a separate web product beside JeRyu. It should expose the existing Rust control plane through a new web BFF (`src/web`) and replace `apps/web` with a real Vite/TypeScript/React application that consumes typed Rust contracts over REST and WebSocket.

The finished product should feel like GitHub/GitLab, but calmer and faster:

- **Global all-repos view** with search, repo families, health, pending reviews, active CI, agent work, cache pressure, and live activity.
- **Repository overview** with rendered README, branch/commit/status summary, merge activity, release status, agent findings, and next actions.
- **Code browser** with fast tree navigation, branch/tag picker, file preview, Markdown rendering, blame/history, symbol search, and binary handling.
- **Merge room** that combines PR/MR conversation, files, checks, reviews, approvals, blockers, generated evidence, and exact-SHA merge safety in one screen.
- **Issues/bugs/projects** spanning repos, with triage, ownership, labels, milestones, boards, and agent assignment.
- **Settings** that are searchable, previewable, auditable, and mapped into provider-specific GitHub/GitLab behavior without exposing confusion to the user.
- **Real-time WebSocket** updates for repo activity, checks, merge posture, reviews, comments, settings, agents, cache, and CI.
- **Safe mutation model** where every destructive or security-relevant change has preview, idempotency, permissions, expected state hash, audit receipt, and websocket event emission.

This spec merges the best concepts from the uploaded solutions into a single final implementation plan. It is intentionally concrete: path-by-path code changes, tree diagram, API contracts, UX controls, settings inventory, database additions, test plan, rollout plan, and acceptance criteria.

---

## 2. Current repository baseline

### 2.1 What exists and should be reused

The current JeRyu repository is a Rust workspace with a mature control-plane shape. The root workspace includes the main `jeryu` package and supporting crates such as witness/proof, VRC/AER, TUI capture, domain, cache-brain adapter, and GCD integration. The Rust package already uses Axum, Tower HTTP, Tokio, SQLx, git2, Ratatui, Docker control, tracing, GitLab client/auth code, messaging, runner backends, telemetry, cache, agent, policy, repo, and TUI modules.

Key reusable foundations:

- `src/api/*`: typed read-model/action/event concepts used by the TUI. This should become the shared contract foundation for both TUI and web.
- `src/engine.rs`: Axum-based engine runtime already hosts `/health`, `/hooks`, `/cache/summary`, reconciliation loops, Docker event loop, and message-log consumer.
- `src/git/*`: bridge, mirror, snapshot, policy, event, store, executor, receipt primitives.
- `src/git_host/*`: GitHub/GitLab adapter nucleus with provider-specific types/stubs/helpers.
- `src/repo*`: repo/fleet/local/standard modules that should become the source of repository inventory/family grouping.
- `src/tui/*`: product knowledge for activity/status visualization, tests/VTI, cache, jobs, agents, and runtime state.
- `src/bugtracker`, `src/agent`, `src/approval`, `src/policy`, `src/settings`, `src/cache`, `src/telemetry`: domain modules that should surface through web UI instead of being rebuilt.

### 2.2 Current web gap

The existing root `package.json` only declares the `apps/web` workspace and UX-QA scripts. `apps/web/package.json` is currently a minimal placeholder package named `@jankurai/ux-qa` with `build` and `test` scripts that call `ux-qa-check.mjs`. There is no Vite application shell, React routes, API client, WebSocket store, repository browser, Markdown renderer, merge review UI, or settings UI.

The replacement must keep the quality intent of the placeholder — Storybook/Playwright/accessibility/visual proof — but rename and expand the workspace into a production app: `@jeryu/web`.

### 2.3 Current API gap

`src/api` is currently framed as a TUI Control-Plane API with modules like actions, dashboards, entity, events, freshness, read_model, runtime_profile, and snapshot. That is good, but it does not yet expose the forge concepts needed by the browser:

- repository inventory and creation;
- namespaces/owners/families;
- refs/branches/tags;
- tree/blob/README rendering;
- commits/compare/diff/blame;
- merge request / pull request review and approval;
- issue/bug/project-board views;
- repo/org/user settings;
- web notifications;
- web action previews and audit receipts;
- websocket subscription scopes.

### 2.4 Current runtime gap

The current `serve` command starts GitLab/Docker/control-plane background work and waits for Ctrl-C. It does not serve a browser app or static Vite assets. The final design should preserve `jeryu serve` compatibility while adding:

- `jeryu serve --web` for integrated control-plane + web UI;
- `jeryu web serve` for explicit browser UI hosting;
- `jeryu web dev` for local frontend development via Vite dev server proxy;
- `jeryu web openapi` / `jeryu web routes` for contract and route inspection.

---

## 3. Product north star

JeRyu Web Forge is not a clone of GitHub or GitLab. It is a **single, modern, real-time control surface for code, review, CI, agents, policy, and settings**.

A user should be able to answer these questions without hunting through tabs:

1. What repos do I have, grouped by owner/family/health/attention?
2. What needs my review or approval right now?
3. Which merge requests are blocked, and exactly why?
4. What changed in this branch, and is it safe to merge?
5. What is CI doing under the hood, and what is stale or flaky?
6. What agents are changing code, and what evidence supports those changes?
7. Is the README rendered correctly and safely?
8. Which settings matter for this repo, and what would changing them do?
9. What changed in settings, who did it, and can I undo it?
10. How close are repo/runner/cache systems to capacity limits?

### 3.1 Better-than-GitHub/GitLab differentiators

- **One merge passport.** Checks, reviews, approvals, required conversations, branch protection, stale SHA, VTI/test confidence, security policy, agent evidence, and risk are summarized in a single decision panel.
- **Real-time everywhere.** WebSocket updates are first-class, not occasional polling. The UI visibly shows whether it is live, reconnecting, stale, or catching up.
- **Command palette for everything.** Repo switch, create repo, open MR, approve, merge, assign, label, edit setting, rerun CI, search file, jump to agent, copy clone URL.
- **Settings are explainable.** Every setting has purpose, impact, current provider support, effective value, inheritance source, safety tier, and preview diff before save.
- **Actions are safe by default.** Mutations require preview and exact expected state where relevant. Every mutation writes an audit receipt and emits a live event.
- **README and Markdown are secure and correct.** Server-side rendering with sanitization, link rewriting, image proxying, heading anchors, syntax highlighting, task lists, tables, and fallback plain rendering.
- **Attention-first UX.** Views are sorted by what needs action, not alphabetically by default.
- **Repo families.** Repos like `veox-*` can be grouped into families with shared dashboards, cross-repo issues, shared cache/runner pressure, and family settings.
- **Agent-native.** Agent plans, patches, reviews, traces, grants, and evidence are part of the forge experience, not bolt-ons.

---

## 4. Target repository tree

The following tree is the intended final structure after the feature lands. Existing files remain unless explicitly marked as replaced.

```text
jeryu/
├── Cargo.toml                                # add web deps/features/validation metadata
├── Cargo.lock                                # regenerated
├── package.json                              # root npm scripts: dev/build/test/typecheck/storybook/e2e/ux-qa
├── apps/
│   ├── api/                                  # keep existing if present
│   └── web/                                  # REPLACE placeholder with real app
│       ├── AGENTS.md                         # keep; update ownership/proof lanes
│       ├── README.md                         # frontend developer guide
│       ├── index.html
│       ├── package.json                      # @jeryu/web
│       ├── tsconfig.json
│       ├── tsconfig.node.json
│       ├── vite.config.ts
│       ├── vitest.config.ts
│       ├── playwright.config.ts
│       ├── ux-qa.md                          # update from placeholder into proof guide
│       ├── ux-qa-check.mjs                   # keep or replace with real checks
│       ├── public/
│       │   ├── favicon.svg
│       │   └── robots.txt
│       ├── src/
│       │   ├── main.tsx
│       │   ├── App.tsx
│       │   ├── router.tsx
│       │   ├── env.ts
│       │   ├── api/
│       │   │   ├── client.ts                 # fetch wrapper + idempotency + errors
│       │   │   ├── generated.ts              # generated from Rust/OpenAPI/TS export
│       │   │   ├── schemas.ts                # zod runtime guards for critical WS/events
│       │   │   ├── ws.ts                     # websocket protocol client
│       │   │   └── queryKeys.ts
│       │   ├── state/
│       │   │   ├── eventStore.ts             # Zustand live event cache
│       │   │   ├── sessionStore.ts
│       │   │   ├── commandStore.ts
│       │   │   ├── uiStore.ts
│       │   │   └── settingsDraftStore.ts
│       │   ├── routes/
│       │   │   ├── root.tsx
│       │   │   ├── dashboard.tsx
│       │   │   ├── repos.index.tsx
│       │   │   ├── repos.new.tsx
│       │   │   ├── repo.overview.tsx
│       │   │   ├── repo.code.tsx
│       │   │   ├── repo.blob.tsx
│       │   │   ├── repo.commits.tsx
│       │   │   ├── repo.branches.tsx
│       │   │   ├── repo.tags.tsx
│       │   │   ├── repo.compare.tsx
│       │   │   ├── repo.mergeRequests.index.tsx
│       │   │   ├── repo.mergeRequests.detail.tsx
│       │   │   ├── repo.mergeRequests.review.tsx
│       │   │   ├── repo.issues.index.tsx
│       │   │   ├── repo.issues.detail.tsx
│       │   │   ├── repo.projects.tsx
│       │   │   ├── repo.actions.tsx
│       │   │   ├── repo.insights.tsx
│       │   │   ├── repo.settings.tsx
│       │   │   ├── reviews.tsx
│       │   │   ├── merge-room.tsx
│       │   │   ├── agents.tsx
│       │   │   ├── notifications.tsx
│       │   │   ├── audit.tsx
│       │   │   └── not-found.tsx
│       │   ├── components/
│       │   │   ├── shell/
│       │   │   │   ├── AppShell.tsx
│       │   │   │   ├── TopBar.tsx
│       │   │   │   ├── LeftNav.tsx
│       │   │   │   ├── LiveDock.tsx
│       │   │   │   ├── RepoSwitcher.tsx
│       │   │   │   └── ConnectionBadge.tsx
│       │   │   ├── command/
│       │   │   │   ├── CommandPalette.tsx
│       │   │   │   ├── commandRegistry.ts
│       │   │   │   └── shortcuts.ts
│       │   │   ├── repos/
│       │   │   │   ├── RepoCard.tsx
│       │   │   │   ├── RepoTable.tsx
│       │   │   │   ├── RepoFamilyGroup.tsx
│       │   │   │   ├── CreateRepoDialog.tsx
│       │   │   │   ├── CloneUrlButton.tsx
│       │   │   │   └── RepoAttentionBadges.tsx
│       │   │   ├── code/
│       │   │   │   ├── BranchPicker.tsx
│       │   │   │   ├── FileTree.tsx
│       │   │   │   ├── CodeViewer.tsx
│       │   │   │   ├── BlobToolbar.tsx
│       │   │   │   ├── MarkdownRenderer.tsx
│       │   │   │   ├── MarkdownToc.tsx
│       │   │   │   ├── BinaryFileCard.tsx
│       │   │   │   └── CommitBreadcrumb.tsx
│       │   │   ├── diff/
│       │   │   │   ├── DiffViewer.tsx
│       │   │   │   ├── DiffFileList.tsx
│       │   │   │   ├── InlineCommentBox.tsx
│       │   │   │   └── ViewedFileCheckbox.tsx
│       │   │   ├── merge/
│       │   │   │   ├── MergePassport.tsx
│       │   │   │   ├── MergeBox.tsx
│       │   │   │   ├── ApprovalButton.tsx
│       │   │   │   ├── ReviewerMatrix.tsx
│       │   │   │   ├── ConversationTimeline.tsx
│       │   │   │   ├── ReviewThread.tsx
│       │   │   │   └── ChecksSummary.tsx
│       │   │   ├── issues/
│       │   │   │   ├── IssueList.tsx
│       │   │   │   ├── IssueComposer.tsx
│       │   │   │   ├── LabelPicker.tsx
│       │   │   │   └── ProjectBoard.tsx
│       │   │   ├── settings/
│       │   │   │   ├── SettingsLayout.tsx
│       │   │   │   ├── SettingsSearch.tsx
│       │   │   │   ├── SettingField.tsx
│       │   │   │   ├── SettingDiffPreview.tsx
│       │   │   │   ├── DangerZone.tsx
│       │   │   │   └── InheritancePill.tsx
│       │   │   ├── agents/
│       │   │   │   ├── AgentActivityCard.tsx
│       │   │   │   ├── AgentPatchPanel.tsx
│       │   │   │   ├── AgentGrantPanel.tsx
│       │   │   │   └── AgentEvidenceDrawer.tsx
│       │   │   └── common/
│       │   │       ├── EmptyState.tsx
│       │   │       ├── ErrorBoundary.tsx
│       │   │       ├── LoadingSkeleton.tsx
│       │   │       ├── RelativeTime.tsx
│       │   │       ├── IdempotentActionButton.tsx
│       │   │       ├── ConfirmActionDialog.tsx
│       │   │       └── VirtualList.tsx
│       │   ├── design/
│       │   │   ├── tokens.css
│       │   │   ├── theme.ts
│       │   │   └── density.ts
│       │   ├── test/
│       │   │   ├── mocks.ts
│       │   │   ├── fixtures.ts
│       │   │   └── render.tsx
│       │   └── stories/
│       │       ├── AppShell.stories.tsx
│       │       ├── RepoDashboard.stories.tsx
│       │       ├── MarkdownRenderer.stories.tsx
│       │       ├── MergePassport.stories.tsx
│       │       ├── DiffViewer.stories.tsx
│       │       └── Settings.stories.tsx
│       └── e2e/
│           ├── dashboard.spec.ts
│           ├── repo-create.spec.ts
│           ├── readme-render.spec.ts
│           ├── code-browser.spec.ts
│           ├── merge-review.spec.ts
│           ├── settings.spec.ts
│           └── websocket.spec.ts
├── db/
│   └── migrations/
│       ├── 202605260001_web_forge_core.sql
│       ├── 202605260002_web_forge_review.sql
│       ├── 202605260003_web_forge_settings.sql
│       └── 202605260004_web_forge_events_audit.sql
├── docs/
│   ├── web-forge.md
│   ├── web-forge-api.md
│   ├── web-forge-ws.md
│   ├── web-forge-markdown-security.md
│   └── web-forge-settings.md
├── src/
│   ├── lib.rs                              # add pub mod web; extend api exports
│   ├── cli_defs.rs                         # add Web command and serve flags
│   ├── cli_defs_commands_web.rs            # new web subcommands
│   ├── dispatch.rs                         # dispatch web serve/dev/openapi/routes
│   ├── engine.rs                           # optionally mount web API router in integrated serve
│   ├── api/
│   │   ├── mod.rs                          # add web/forge contract modules
│   │   ├── forge_events.rs
│   │   ├── web_contracts.rs
│   │   ├── repositories.rs
│   │   ├── repository_settings.rs
│   │   ├── repo_browser.rs
│   │   ├── markdown.rs
│   │   ├── merge_requests.rs
│   │   ├── issues.rs
│   │   ├── projects.rs
│   │   ├── notifications.rs
│   │   └── audit.rs
│   ├── web/
│   │   ├── mod.rs
│   │   ├── config.rs
│   │   ├── state.rs
│   │   ├── router.rs
│   │   ├── error.rs
│   │   ├── auth.rs
│   │   ├── rbac.rs
│   │   ├── csrf.rs
│   │   ├── static_files.rs
│   │   ├── ws.rs
│   │   ├── ws_protocol.rs
│   │   ├── events.rs
│   │   ├── audit.rs
│   │   ├── markdown.rs
│   │   ├── idempotency.rs
│   │   ├── handlers/
│   │   │   ├── mod.rs
│   │   │   ├── bootstrap.rs
│   │   │   ├── repos.rs
│   │   │   ├── repo_browser.rs
│   │   │   ├── commits.rs
│   │   │   ├── merge_requests.rs
│   │   │   ├── issues.rs
│   │   │   ├── projects.rs
│   │   │   ├── settings.rs
│   │   │   ├── notifications.rs
│   │   │   ├── agents.rs
│   │   │   ├── actions.rs
│   │   │   └── audit.rs
│   │   ├── services/
│   │   │   ├── mod.rs
│   │   │   ├── repo_service.rs
│   │   │   ├── repo_browser_service.rs
│   │   │   ├── markdown_service.rs
│   │   │   ├── merge_service.rs
│   │   │   ├── issue_service.rs
│   │   │   ├── settings_service.rs
│   │   │   ├── notification_service.rs
│   │   │   ├── permission_service.rs
│   │   │   └── search_service.rs
│   │   └── tests.rs
│   └── git_host/
│       ├── mod.rs                          # expose normalized provider trait
│       ├── provider.rs                      # normalized host API
│       ├── github.rs                        # expand implementation
│       ├── gitlab.rs                        # expand implementation
│       ├── settings_mapping.rs              # provider-specific settings mapping
│       └── merge_mapping.rs                 # approval/merge parity mapping
└── tests/
    ├── web_bootstrap_tests.rs
    ├── web_repo_tests.rs
    ├── web_markdown_tests.rs
    ├── web_merge_tests.rs
    ├── web_settings_tests.rs
    └── web_ws_tests.rs
```

---

## 5. Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                               apps/web                                      │
│  Vite + React + TypeScript SPA                                              │
│  TanStack Router/Query, Zustand, Monaco/diff, Storybook, Playwright, a11y    │
└──────────────────────────────────┬──────────────────────────────────────────┘
                                   │ REST + WebSocket + typed contracts
                                   ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                               src/web                                       │
│  Axum BFF: auth/session/CSRF, REST routes, WS, static assets, markdown,      │
│  idempotency, permissions, action previews, audit, notification fanout       │
└─────────────┬────────────────────┬────────────────────┬─────────────────────┘
              │                    │                    │
              ▼                    ▼                    ▼
┌──────────────────────┐ ┌──────────────────────┐ ┌──────────────────────────┐
│ src/api              │ │ src/git_host          │ │ src/git / repo / state    │
│ Shared contracts,    │ │ Normalized GitHub/    │ │ Repo inventory, tree/blob │
│ read models, actions,│ │ GitLab provider trait │ │ cache, DB, audit, events  │
│ event schemas        │ │ and mapping layer     │ │                          │
└──────────────────────┘ └──────────────────────┘ └──────────────────────────┘
              │                    │                    │
              ▼                    ▼                    ▼
┌──────────────────────┐ ┌──────────────────────┐ ┌──────────────────────────┐
│ src/tui              │ │ External Git hosts    │ │ Engine / CI / agents      │
│ Same typed read      │ │ GitHub, GitLab, local │ │ Existing JeRyu runtime     │
│ model and events     │ │ bare repos            │ │ Webhooks, cache, runners   │
└──────────────────────┘ └──────────────────────┘ └──────────────────────────┘
```

### 5.1 Boundary rule

The browser never calls GitHub or GitLab directly. It calls JeRyu. JeRyu owns:

- provider credential use;
- permission normalization;
- repo and settings caching;
- Markdown sanitization;
- exact-SHA safety;
- branch protection/merge gate interpretation;
- action preview and audit;
- websocket fanout;
- rate-limit/backoff behavior;
- provider-specific fallbacks.

### 5.2 Runtime modes

| Mode | Command | Purpose |
|---|---|---|
| Engine only | `jeryu serve` | Preserve existing control-plane behavior. |
| Integrated web | `jeryu serve --web` | Start engine plus web BFF/static assets. |
| Web only | `jeryu web serve` | Serve browser app against local JeRyu state/services. |
| Frontend dev | `jeryu web dev --dev-assets http://127.0.0.1:5173` + `npm run dev` | Vite HMR with Rust API proxy. |
| Contract export | `jeryu web openapi` | Print/write REST + WS schema. |
| Route audit | `jeryu web routes` | Print Axum route tree and permission requirements. |

---

## 6. User experience specification

### 6.1 Global shell

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ JeRyu  ⌘K Search/action...   Repo: neverhuman/jeryu ▾   Live ●  User ▾      │
├───────────────┬──────────────────────────────────────────────┬──────────────┤
│ Dashboard     │ Main work area                               │ Live Dock    │
│ Repos         │ route-specific content                       │ Activity     │
│ Merge Room    │                                              │ Checks       │
│ Reviews       │                                              │ Agents       │
│ CI / Runs     │                                              │ Logs         │
│ Agents        │                                              │ Alerts       │
│ Cache         │                                              │              │
│ Issues        │                                              │              │
│ Audit         │                                              │              │
│ Settings      │                                              │              │
└───────────────┴──────────────────────────────────────────────┴──────────────┘
```

Controls:

- `⌘K` / `Ctrl+K`: global command palette.
- `/`: focus search in the current view.
- `g d`: dashboard.
- `g r`: all repos.
- `g m`: merge room.
- `g i`: issues.
- `g a`: agents.
- `g s`: settings.
- `[` / `]`: previous/next repo or MR in current list.
- `Enter`: open selected item.
- `Esc`: close modal/drawer or go up one context.
- `Shift+R`: refresh current route snapshot.
- `.`: open web IDE/file quick action.
- `y`: permalink current ref/SHA.
- `c`: copy clone URL or selected identifier.

### 6.2 All repos dashboard

The all-repos dashboard is the home screen. It answers “what needs attention?”

Required controls:

- Search by repo name, owner, family, language, topic, status, branch, actor, agent, issue label.
- Filter by host: all, GitHub, GitLab, local.
- Filter by visibility: public, internal, private, archived.
- Filter by health: healthy, failing CI, blocked merge, stale default branch, policy alert, cache pressure, agent active.
- Group by owner, family, language, host, health, attention.
- Sort by attention, recently pushed, failing checks, open MRs, open issues, name.
- Toggle table/card/compact view.
- Create repo button with preview and idempotency.
- Import/mirror repo button.
- Bulk actions: archive, tag topic, refresh provider cache, sync settings, assign family.
- Copy clone URL inline.
- Pin favorite repos.
- Keyboard multi-select.

Repo cards must show:

- repo name and host;
- description;
- visibility;
- default branch;
- last push;
- open MRs/PRs;
- open issues/bugs;
- active CI/runners;
- agent activity;
- README presence/render status;
- branch protection summary;
- attention badges;
- quick actions.

### 6.3 Create repository flow

The create repo dialog should be simpler than GitHub/GitLab while supporting all necessary settings:

1. Pick host/provider: GitHub, GitLab, local, mirror target.
2. Pick owner/namespace.
3. Name with slug validation and availability check.
4. Description.
5. Visibility: public/internal/private.
6. Initialize with README.
7. `.gitignore` template.
8. License template.
9. Default branch.
10. Repo family.
11. Topics/tags.
12. Branch protection preset.
13. Merge policy preset.
14. CI/template preset.
15. Agent policy preset.
16. Secret scanning/push protection preset.
17. Webhook template.
18. Dry-run preview.
19. Execute with idempotency key.
20. Post-create next actions: clone, open repo, add collaborators, create first MR, configure CI, import code.

### 6.4 Repository overview

Repository overview should combine what GitHub spreads across many screens:

- README rendered as HTML.
- Repo summary cards: default branch, latest commit, open MRs, checks, releases, agents, issues, branch protection, cache health.
- Activity feed scoped to repo.
- Next actions: review, merge, fix failing CI, update stale branch, configure missing protection.
- Clone URL with protocol selector.
- Branch/tag picker.
- Topic/family badges.
- Owner/collaborator quick list.
- Recent commits.
- Recent merge requests.
- Recent issues.
- Release/canary status.
- Agent evidence and warnings.

### 6.5 Code browser

Required code browser controls:

- Branch/tag/SHA selector with recent refs and typeahead.
- Breadcrumb path navigation.
- Virtualized file tree.
- File search within repo.
- Symbol search where index available.
- Open file by path (`t`).
- Copy path.
- Copy permalink to SHA.
- Raw view.
- Download file.
- History for file/path.
- Blame.
- Compare from current path.
- Open in local editor if configured.
- Render Markdown.
- Preview images/SVG safely.
- Binary file card for non-text.
- Large file guardrails.

### 6.6 README and Markdown rendering

Markdown support is non-negotiable. Users must be able to see `README.md` and every `.md` file rendered correctly to HTML.

Server pipeline:

1. Fetch blob at exact ref/SHA.
2. Detect encoding and size.
3. If Markdown and below limit, render on backend with `pulldown-cmark` configured for GFM-like extensions: tables, strikethrough, task lists, heading anchors, footnotes where supported.
4. Sanitize with `ammonia` allowlist.
5. Rewrite relative links to JeRyu routes.
6. Rewrite relative images through a safe asset proxy.
7. Syntax-highlight fenced code using `syntect` or a server-side highlighter.
8. Cache by `(repo_id, commit_sha, path, renderer_version, sanitizer_version)`.
9. Return rendered HTML, table of contents, source metadata, warnings, and content hash.
10. Frontend sanitizes again with DOMPurify before `dangerouslySetInnerHTML`.

Required Markdown features:

- headings with anchors;
- nested lists;
- task lists;
- tables;
- fenced code blocks with language labels;
- inline code;
- blockquotes;
- links and relative links;
- relative images;
- badges;
- emoji text preserved;
- HTML stripped or sanitized;
- Mermaid disabled by default unless explicitly enabled and sandboxed;
- alert/admonition blocks as optional extension;
- frontmatter displayed only when configured.

### 6.7 Merge room and review UX

The Merge Room is the biggest opportunity to beat GitHub/GitLab.

Required screens:

- Global merge queue: all MRs/PRs needing attention.
- Repo merge list: open/merged/closed/draft/blocked/ready.
- Merge detail overview.
- Conversation timeline.
- Files changed with virtualized diff.
- Review threads with resolved/unresolved filters.
- Checks and CI logs.
- Merge Passport.
- Agent evidence and policy warnings.
- Approval/merge action panel.

Merge Passport fields:

- `head_sha` and expected SHA status;
- base branch protection;
- required approvals and current approvals;
- required reviewers/codeowners;
- unresolved conversation count;
- required checks and status;
- VTI confidence/test selection summary;
- cache trust/taint state;
- security/policy status;
- agent risk score/evidence;
- merge method availability;
- stale branch/update needed;
- auto-merge availability;
- final decision: ready, blocked, caution, unknown.

Approval/merge safety:

1. UI sends `expected_head_sha` and `merge_passport_hash`.
2. Backend refetches host state.
3. Backend rejects if `head_sha` changed.
4. Backend re-evaluates checks, approvals, policy, branch protection, agent evidence.
5. Backend performs provider action using exact SHA when provider supports it.
6. Backend writes audit receipt.
7. Backend emits websocket event.
8. Frontend updates local cache and shows receipt.

### 6.8 Issues, bugs, and projects

JeRyu should unify external issues and its own bugtracker.

Controls:

- Global issue search across repos.
- Repo issue list.
- Filters: open/closed, severity, priority, label, milestone, assignee, component, project, agent, stale, blocked.
- New issue/bug composer.
- Markdown preview.
- Labels/milestones/assignees.
- Link issue to MR/commit/test failure/agent session.
- Convert failing CI or agent finding to issue.
- Boards by status/priority/component/owner.
- Bulk triage.
- Saved views.
- Agent assignment and attempt tracking.

### 6.9 Agents and autonomous workflows

Required controls:

- Agent session list.
- Agent status and health.
- Active intent/goal.
- Current repo/branch/MR.
- Patch proposed and diff.
- Evidence packets.
- Grants requested/approved/denied/expired.
- Race/winner outcomes.
- Logs/traces with redaction.
- Stop/pause/resume session.
- Approve/reject patch.
- Convert patch to MR.
- Configure per-repo agent policy.
- View agent config and ownership.

### 6.10 Live dock

The Live Dock appears on the right side and can be collapsed. It receives websocket events and shows:

- activity feed;
- running checks;
- pending review comments;
- agent activity;
- errors/alerts;
- cache pressure;
- runner pressure;
- settings/audit changes;
- connection status;
- event gap recovery status.

---

## 7. Settings inventory

Settings must be searchable and grouped. Each setting row must include label, effective value, inherited value, provider support, safety tier, description, last changed by, preview diff, and audit history.

### 7.1 User settings

- Profile display name/avatar/email mapping.
- Theme: system/light/dark/high-contrast.
- Density: comfortable/compact/ultra-compact.
- Default dashboard view.
- Favorite repos/families.
- Keyboard shortcut profile.
- Default clone protocol: HTTPS/SSH.
- Notifications: email/web/in-app/webhook/slack equivalent where configured.
- Review preferences: hide whitespace, diff side-by-side/unified, auto-mark viewed, collapse generated files.
- Markdown preferences: render HTML disabled, Mermaid enabled/sandboxed, line wrap, TOC position.
- Time display: relative/absolute/timezone.
- Live update aggressiveness: full, reduced, manual.
- Accessibility: reduce motion, larger font, screen-reader landmarks, color-blind-friendly badges.

### 7.2 Organization/namespace settings

- Namespace name/slug/description/avatar.
- Default visibility.
- Default branch name.
- Repo creation permissions.
- Member roles and teams.
- Default branch protection preset.
- Default merge policy.
- Default issue labels.
- Default CI templates.
- Default agent policy.
- Webhooks/integrations.
- Audit retention.
- Secret scanning defaults.
- Required signed commits/tags.
- SSO/session policy if applicable.
- Repo family definitions and glob rules.

### 7.3 Repository general settings

- Description.
- Homepage.
- Topics/tags.
- Visibility.
- Default branch.
- Features: issues, projects, wiki, discussions, packages, actions/CI, merge requests.
- Archive/unarchive.
- Rename repository.
- Transfer repository.
- Delete repository.
- Forking policy.
- Template repo flag.
- Mirror settings.
- Pull/push remote URLs.
- Repo family assignment.

### 7.4 Access and collaborators

- Collaborators.
- Teams/groups.
- Role mapping.
- Deploy keys.
- SSH keys.
- Access tokens/deploy tokens metadata.
- Protected environment reviewers.
- CODEOWNERS status.
- Inherited permissions view.
- Permission simulation: “can user X merge branch Y?”

### 7.5 Branch and tag protection

- Protected branches list.
- Pattern matching.
- Require PR/MR.
- Required approvals.
- Dismiss stale approvals.
- Require code owner review.
- Require conversation resolution.
- Require status checks.
- Require linear history.
- Require signed commits.
- Require deployments.
- Restrict push actors.
- Restrict force push.
- Allow deletion.
- Lock branch.
- Tag protection patterns.
- Default protection preset.

### 7.6 Merge policy

- Merge methods: merge commit, squash, rebase.
- Default merge method.
- Squash commit title/body templates.
- Merge commit title/body templates.
- Auto-merge.
- Delete source branch after merge.
- Update branch policy.
- Merge queue.
- Draft MR behavior.
- Required reviewers.
- Approval reset rules.
- Stale head behavior.
- Merge passport required.
- Agent-authored MR requirements.

### 7.7 CI/CD and runners

- Pipeline enablement.
- Default CI config path.
- Required checks.
- Runner pools.
- Runner tags.
- Concurrency limits.
- Timeout defaults.
- Cache policy.
- Artifact retention.
- Environment protection.
- Manual job permissions.
- Scheduled pipelines.
- VTI smart-test settings.
- Flake quarantine.
- Retry policy.
- Log retention.
- Webhook event subscriptions.

### 7.8 Security and compliance

- Secret scanning.
- Push protection.
- Dependency scanning.
- Container scanning.
- License policy.
- Signed commits.
- Signed tags.
- Verified provenance/SLSA settings where supported.
- Honeypot/sandbox policy.
- Admission hooks.
- Taint policy.
- Audit log retention.
- Required evidence for agent patches.
- Sensitive file patterns.
- Network/sandbox defaults for jobs/agents.

### 7.9 Markdown/rendering settings

- Render README by default.
- Allow raw HTML: off by default.
- Mermaid: off/sandboxed/on.
- SVG rendering: sanitized/proxy/download-only.
- External images: proxy/block/allow.
- Relative link behavior.
- Heading anchor style.
- Syntax theme.
- Max render size.
- Frontmatter handling.
- Security warning mode.

### 7.10 Integrations and webhooks

- GitHub/GitLab provider connection.
- Local bare repo roots.
- Webhook endpoints.
- Webhook secret rotation.
- Event subscriptions.
- Slack/Discord/email equivalents if integrated.
- MCP servers.
- Agent tool integrations.
- Mirror/push targets.
- Registry/package integrations.
- External issue tracker links.

### 7.11 Real-time settings

- WebSocket enabled.
- Poll fallback interval.
- Subscription scopes.
- Event retention.
- Max events per client.
- Backpressure policy.
- Notification rules.
- Live Dock filters.
- Desktop/browser notifications.

---

## 8. REST API specification

Use `/api/v1` for stable web API. Every response must be JSON unless returning explicit raw/blob content. Every mutation requires CSRF/session validation, permission check, idempotency key where applicable, action preview unless low-risk, audit receipt, and websocket emission.

### 8.1 Bootstrap

```http
GET /api/v1/bootstrap
```

Returns `WebBootstrap`:

```json
{
  "viewer": { "id": "user_1", "login": "ben", "display_name": "Ben" },
  "permissions": ["repo.read", "repo.create", "mr.approve"],
  "feature_flags": { "web_forge": true, "agents": true },
  "recent_repositories": [],
  "attention": [],
  "websocket_url": "/api/v1/ws",
  "event_cursor": 1234,
  "server_time": "2026-05-26T00:00:00Z"
}
```

### 8.2 Repositories

```http
GET  /api/v1/repos?search=&host=&owner=&family=&visibility=&health=&include_archived=false&limit=50&cursor=
POST /api/v1/repos
GET  /api/v1/repos/{host}/{owner}/{repo}
PATCH /api/v1/repos/{host}/{owner}/{repo}/settings
POST /api/v1/repos/{host}/{owner}/{repo}/refresh
POST /api/v1/repos/{host}/{owner}/{repo}/archive
POST /api/v1/repos/{host}/{owner}/{repo}/unarchive
DELETE /api/v1/repos/{host}/{owner}/{repo}
```

`POST /api/v1/repos` supports preview:

```json
{
  "host": "github",
  "owner": "neverhuman",
  "name": "new-repo",
  "description": "...",
  "visibility": "private",
  "initialize_readme": true,
  "gitignore_template": "Rust",
  "license_template": "MIT",
  "default_branch": "main",
  "family": "veox",
  "branch_protection_preset": "strict",
  "merge_policy_preset": "safe",
  "dry_run": true
}
```

### 8.3 Code browser

```http
GET /api/v1/repos/{host}/{owner}/{repo}/refs
GET /api/v1/repos/{host}/{owner}/{repo}/branches
GET /api/v1/repos/{host}/{owner}/{repo}/tags
GET /api/v1/repos/{host}/{owner}/{repo}/tree?ref=main&path=src&recursive=false
GET /api/v1/repos/{host}/{owner}/{repo}/blob?ref=main&path=README.md&render=html
GET /api/v1/repos/{host}/{owner}/{repo}/readme?ref=main
GET /api/v1/repos/{host}/{owner}/{repo}/commits?ref=main&path=
GET /api/v1/repos/{host}/{owner}/{repo}/commits/{sha}
GET /api/v1/repos/{host}/{owner}/{repo}/compare?base=main&head=feature
GET /api/v1/repos/{host}/{owner}/{repo}/blame?ref=main&path=src/lib.rs
GET /api/v1/repos/{host}/{owner}/{repo}/raw?ref=main&path=README.md
```

Blob response:

```json
{
  "repo": "github/neverhuman/jeryu",
  "ref_name": "main",
  "commit_sha": "abc123",
  "path": "README.md",
  "kind": "markdown",
  "is_binary": false,
  "size_bytes": 12983,
  "language": "markdown",
  "text": "# JeRyu...",
  "rendered_html": "<h1 id=\"jeryu\">JeRyu</h1>",
  "toc": [{ "level": 1, "id": "jeryu", "text": "JeRyu" }],
  "warnings": [],
  "content_hash": "sha256:..."
}
```

### 8.4 Merge requests / pull requests

Normalize GitHub PR and GitLab MR as `MergeRequestView`.

```http
GET  /api/v1/repos/{host}/{owner}/{repo}/merge-requests?state=open&review_state=&attention=
POST /api/v1/repos/{host}/{owner}/{repo}/merge-requests
GET  /api/v1/repos/{host}/{owner}/{repo}/merge-requests/{number}
GET  /api/v1/repos/{host}/{owner}/{repo}/merge-requests/{number}/files
GET  /api/v1/repos/{host}/{owner}/{repo}/merge-requests/{number}/checks
GET  /api/v1/repos/{host}/{owner}/{repo}/merge-requests/{number}/passport
POST /api/v1/repos/{host}/{owner}/{repo}/merge-requests/{number}/comments
POST /api/v1/repos/{host}/{owner}/{repo}/merge-requests/{number}/review
POST /api/v1/repos/{host}/{owner}/{repo}/merge-requests/{number}/approve
POST /api/v1/repos/{host}/{owner}/{repo}/merge-requests/{number}/unapprove
POST /api/v1/repos/{host}/{owner}/{repo}/merge-requests/{number}/update-branch
POST /api/v1/repos/{host}/{owner}/{repo}/merge-requests/{number}/merge
POST /api/v1/repos/{host}/{owner}/{repo}/merge-requests/{number}/close
POST /api/v1/repos/{host}/{owner}/{repo}/merge-requests/{number}/reopen
```

Approval request:

```json
{
  "expected_head_sha": "abc123",
  "passport_hash": "sha256:...",
  "body": "Reviewed via JeRyu",
  "idempotency_key": "uuid"
}
```

Merge request:

```json
{
  "expected_head_sha": "abc123",
  "passport_hash": "sha256:...",
  "method": "squash",
  "delete_source_branch": true,
  "commit_title": "feat: add web forge",
  "commit_body": "",
  "idempotency_key": "uuid"
}
```

### 8.5 Issues and projects

```http
GET  /api/v1/issues?repo=&state=&label=&assignee=&priority=&severity=&project=&cursor=
POST /api/v1/repos/{host}/{owner}/{repo}/issues
GET  /api/v1/repos/{host}/{owner}/{repo}/issues/{number}
PATCH /api/v1/repos/{host}/{owner}/{repo}/issues/{number}
POST /api/v1/repos/{host}/{owner}/{repo}/issues/{number}/comments
GET  /api/v1/projects?repo=&owner=
POST /api/v1/repos/{host}/{owner}/{repo}/projects
PATCH /api/v1/projects/{project_id}/cards/{card_id}
```

### 8.6 Settings

```http
GET   /api/v1/repos/{host}/{owner}/{repo}/settings/schema
GET   /api/v1/repos/{host}/{owner}/{repo}/settings/effective
POST  /api/v1/repos/{host}/{owner}/{repo}/settings/preview
PATCH /api/v1/repos/{host}/{owner}/{repo}/settings
GET   /api/v1/repos/{host}/{owner}/{repo}/settings/audit
POST  /api/v1/repos/{host}/{owner}/{repo}/settings/rollback
```

Settings patch request:

```json
{
  "expected_settings_hash": "sha256:old",
  "changes": [
    { "path": "merge.required_approvals", "old": 1, "new": 2 },
    { "path": "branch_protection.require_code_owner_review", "old": false, "new": true }
  ],
  "dry_run": true,
  "idempotency_key": "uuid"
}
```

### 8.7 Actions

For dangerous or cross-provider actions:

```http
POST /api/v1/actions/preview
POST /api/v1/actions/execute
GET  /api/v1/actions/{action_id}
```

Every action preview returns:

- action ID;
- actor;
- permission required;
- target entity;
- current state hash;
- proposed state hash;
- provider calls that would happen;
- exact SHA if code-related;
- risk tier;
- side effects;
- rollback availability;
- audit preview;
- idempotency key requirement.

---

## 9. WebSocket protocol

Endpoint:

```http
GET /api/v1/ws
```

### 9.1 Client hello

```json
{
  "type": "hello",
  "protocol": "jeryu.ws.v1",
  "resume_from": 1234,
  "subscriptions": [
    { "scope": "global", "filters": {} },
    { "scope": "repo:github/neverhuman/jeryu", "filters": {} },
    { "scope": "mr:github/neverhuman/jeryu/42", "filters": {} }
  ]
}
```

### 9.2 Server hello

```json
{
  "type": "hello",
  "protocol": "jeryu.ws.v1",
  "server_time": "2026-05-26T12:00:00Z",
  "current_seq": 1300,
  "heartbeat_ms": 15000
}
```

### 9.3 Event

```json
{
  "type": "event",
  "event": {
    "seq": 1301,
    "timestamp": "2026-05-26T12:00:01Z",
    "scope": "repo:github/neverhuman/jeryu",
    "kind": "repo.settings.changed",
    "severity": "info",
    "entity": { "kind": "repository", "id": "github/neverhuman/jeryu" },
    "summary": "Required approvals changed from 1 to 2",
    "payload": {
      "actor": "ben",
      "settings_hash": "sha256:new"
    }
  }
}
```

### 9.4 Gap handling

```json
{
  "type": "snapshot_required",
  "reason": "event_gap",
  "current_seq": 9001
}
```

The frontend must refetch the current route snapshot or `/api/v1/bootstrap`, then reconnect with the new cursor.

### 9.5 Event kinds to add

- `repo.created`
- `repo.imported`
- `repo.archived`
- `repo.deleted`
- `repo.settings.previewed`
- `repo.settings.changed`
- `repo.permission.changed`
- `repo.readme.rendered`
- `repo.branch.created`
- `repo.branch.deleted`
- `repo.tag.created`
- `repo.push.received`
- `repo.commit.indexed`
- `mr.created`
- `mr.updated`
- `mr.review.requested`
- `mr.review.submitted`
- `mr.comment.created`
- `mr.thread.resolved`
- `mr.approved`
- `mr.unapproved`
- `mr.passport.updated`
- `mr.merged`
- `mr.closed`
- `issue.created`
- `issue.updated`
- `issue.comment.created`
- `project.card.moved`
- `agent.patch.proposed`
- `agent.grant.requested`
- `agent.grant.approved`
- `audit.receipt.created`
- `notification.created`
- `websocket.snapshot_required`

---

## 10. Permissions and security

### 10.1 Normalized permissions

Use normalized permissions server-side and map providers into these:

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
- `project.read`
- `project.write`
- `ci.read`
- `ci.write`
- `secrets.read_metadata`
- `secrets.write`
- `agents.read`
- `agents.write`
- `agents.grant`
- `audit.read`
- `admin.audit`

### 10.2 Authentication/session

Minimum implementation:

- local session cookie;
- CSRF token for mutations;
- provider token loaded from existing GitLab/GitHub auth path;
- optional dev bypass only behind explicit local-only config;
- session expiration and logout;
- viewer bootstrap.

Future implementation:

- OAuth/OIDC provider login;
- SSO enforcement;
- passkeys/WebAuthn;
- device/session management.

### 10.3 Mutation safety rules

Every mutation must:

1. Authenticate viewer.
2. Validate CSRF.
3. Validate request schema.
4. Check normalized permission.
5. Load current target state.
6. Validate expected state hash or expected SHA when provided.
7. Produce preview for high/medium risk.
8. Require idempotency key for create/merge/delete/settings.
9. Execute provider/local state change.
10. Write audit receipt.
11. Emit durable event.
12. Broadcast websocket event.
13. Return updated read model or action receipt.

### 10.4 Markdown security

- Backend sanitization is mandatory.
- Frontend DOMPurify sanitization is mandatory.
- Inline event handlers are forbidden.
- Unknown HTML tags are stripped.
- External images are proxied or blocked by setting.
- SVG is sanitized or rendered as download-only by default.
- Mermaid is disabled by default; if enabled, render in sandboxed iframe/worker.
- Raw HTML rendering setting must clearly show warning and require admin permission.
- Markdown cache includes sanitizer version so security changes invalidate cache.

---

## 11. Data model and migrations

Add tables in a backend-neutral way through existing state/DB patterns.

### 11.1 Core repository tables

```sql
CREATE TABLE web_repositories (
  id TEXT PRIMARY KEY,
  host_kind TEXT NOT NULL,
  host_url TEXT NOT NULL,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  full_name TEXT NOT NULL,
  description TEXT,
  visibility TEXT NOT NULL,
  default_branch TEXT,
  clone_https_url TEXT,
  clone_ssh_url TEXT,
  web_url TEXT,
  family TEXT,
  archived INTEGER NOT NULL DEFAULT 0,
  fork INTEGER NOT NULL DEFAULT 0,
  template INTEGER NOT NULL DEFAULT 0,
  provider_id TEXT,
  provider_etag TEXT,
  pushed_at TEXT,
  refreshed_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(host_kind, owner, name)
);

CREATE INDEX idx_web_repositories_owner ON web_repositories(host_kind, owner);
CREATE INDEX idx_web_repositories_family ON web_repositories(family);
CREATE INDEX idx_web_repositories_attention ON web_repositories(updated_at, pushed_at);
```

### 11.2 Refs/tree/blob cache

```sql
CREATE TABLE web_repo_refs (
  repo_id TEXT NOT NULL,
  ref_kind TEXT NOT NULL,
  name TEXT NOT NULL,
  sha TEXT NOT NULL,
  protected INTEGER NOT NULL DEFAULT 0,
  default_ref INTEGER NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(repo_id, ref_kind, name)
);

CREATE TABLE web_blob_cache (
  repo_id TEXT NOT NULL,
  commit_sha TEXT NOT NULL,
  path TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  is_binary INTEGER NOT NULL DEFAULT 0,
  language TEXT,
  content_hash TEXT NOT NULL,
  text_content TEXT,
  created_at TEXT NOT NULL,
  PRIMARY KEY(repo_id, commit_sha, path)
);

CREATE TABLE web_markdown_cache (
  repo_id TEXT NOT NULL,
  commit_sha TEXT NOT NULL,
  path TEXT NOT NULL,
  renderer_version TEXT NOT NULL,
  sanitizer_version TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  html TEXT NOT NULL,
  toc_json TEXT NOT NULL,
  warnings_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY(repo_id, commit_sha, path, renderer_version, sanitizer_version)
);
```

### 11.3 Merge/review tables

```sql
CREATE TABLE web_merge_requests (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL,
  number INTEGER NOT NULL,
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
  UNIQUE(repo_id, number)
);

CREATE TABLE web_review_threads (
  id TEXT PRIMARY KEY,
  merge_request_id TEXT NOT NULL,
  path TEXT,
  line INTEGER,
  side TEXT,
  resolved INTEGER NOT NULL DEFAULT 0,
  provider_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE web_review_comments (
  id TEXT PRIMARY KEY,
  thread_id TEXT,
  merge_request_id TEXT NOT NULL,
  author_login TEXT NOT NULL,
  body TEXT NOT NULL,
  system INTEGER NOT NULL DEFAULT 0,
  provider_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE web_approvals (
  id TEXT PRIMARY KEY,
  merge_request_id TEXT NOT NULL,
  reviewer_login TEXT NOT NULL,
  state TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  submitted_at TEXT NOT NULL,
  provider_id TEXT
);
```

### 11.4 Settings/audit/events

```sql
CREATE TABLE web_settings_snapshots (
  id TEXT PRIMARY KEY,
  scope_kind TEXT NOT NULL,
  scope_id TEXT NOT NULL,
  settings_hash TEXT NOT NULL,
  settings_json TEXT NOT NULL,
  provider_support_json TEXT NOT NULL,
  inherited_from TEXT,
  created_by TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE web_action_receipts (
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

CREATE TABLE web_events (
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

CREATE INDEX idx_web_events_scope_seq ON web_events(scope, seq);
CREATE INDEX idx_web_events_created ON web_events(created_at);
```

---

## 12. Backend implementation details

### 12.1 `src/web/state.rs`

`WebAppState` should hold shared dependencies:

- `Db`;
- `DockerCtl` optional/disconnected for dev/static modes;
- `GitlabClient`;
- `BackendRegistry` where needed;
- `ProviderRegistry` for GitHub/GitLab/local;
- `WebEventBus`;
- `MarkdownRenderer`;
- `PermissionService`;
- `SettingsService`;
- `IdempotencyStore`;
- `WebConfig`;
- static asset config.

### 12.2 `src/web/router.rs`

Build the API as a nestable router:

```rust
pub fn build_api_router(state: WebAppState) -> Router {
    Router::new()
        .route("/bootstrap", get(handlers::bootstrap::bootstrap))
        .route("/ws", get(ws::ws_handler))
        .nest("/repos", handlers::repos::routes())
        .nest("/issues", handlers::issues::global_routes())
        .nest("/projects", handlers::projects::global_routes())
        .nest("/actions", handlers::actions::routes())
        .nest("/notifications", handlers::notifications::routes())
        .nest("/audit", handlers::audit::routes())
        .layer(csrf::layer())
        .layer(auth::layer())
        .with_state(state)
}
```

### 12.3 Error model

Every error response:

```json
{
  "error": {
    "code": "stale_head_sha",
    "message": "The merge request head changed. Refresh before approving.",
    "details": { "expected": "abc", "actual": "def" },
    "request_id": "req_...",
    "actionable": true
  }
}
```

Error codes:

- `unauthenticated`
- `forbidden`
- `not_found`
- `validation_failed`
- `provider_unavailable`
- `provider_rate_limited`
- `stale_state_hash`
- `stale_head_sha`
- `merge_blocked`
- `markdown_too_large`
- `binary_blob`
- `event_gap`
- `conflict`
- `idempotency_conflict`
- `internal_error`

### 12.4 Provider abstraction

Add a normalized trait in `src/git_host/provider.rs`:

```rust
#[async_trait::async_trait]
pub trait GitHostProvider: Send + Sync {
    fn kind(&self) -> HostKind;
    async fn list_repositories(&self, query: RepoQuery) -> Result<Page<RepositorySummary>>;
    async fn create_repository(&self, req: CreateRepositoryRequest) -> Result<RepositorySummary>;
    async fn get_repository(&self, repo: RepoKey) -> Result<RepositoryDetail>;
    async fn list_refs(&self, repo: RepoKey) -> Result<RefsView>;
    async fn get_tree(&self, repo: RepoKey, req: TreeRequest) -> Result<TreeView>;
    async fn get_blob(&self, repo: RepoKey, req: BlobRequest) -> Result<BlobView>;
    async fn get_readme(&self, repo: RepoKey, req: ReadmeRequest) -> Result<BlobView>;
    async fn compare(&self, repo: RepoKey, req: CompareRequest) -> Result<CompareView>;
    async fn list_merge_requests(&self, repo: RepoKey, q: MergeRequestQuery) -> Result<Page<MergeRequestSummary>>;
    async fn get_merge_request(&self, repo: RepoKey, number: u64) -> Result<MergeRequestDetail>;
    async fn approve_merge_request(&self, repo: RepoKey, req: ApproveRequest) -> Result<ActionReceipt>;
    async fn merge_merge_request(&self, repo: RepoKey, req: MergeRequestMergeRequest) -> Result<ActionReceipt>;
    async fn get_settings(&self, repo: RepoKey) -> Result<RepositorySettingsView>;
    async fn patch_settings(&self, repo: RepoKey, patch: SettingsPatch) -> Result<RepositorySettingsView>;
}
```

GitHub/GitLab divergences stay inside provider implementations. The UI consumes normalized models plus `provider_extensions` for rare unsupported details.

---

## 13. Frontend implementation details

### 13.1 App stack

- Vite for build/dev.
- React 18+.
- TypeScript strict.
- TanStack Router for typed routes.
- TanStack Query for server state.
- Zustand for live event/session/UI stores.
- Monaco or lightweight code viewer for code.
- Virtualized lists for repos, trees, diffs, logs.
- DOMPurify for final HTML sanitization.
- Zod for runtime guards on websocket events and critical mutation responses.
- Playwright for E2E.
- Vitest + Testing Library for unit/component.
- Storybook for visual scenarios.
- Axe/accessibility checks.

### 13.2 State model

- REST snapshots are canonical for route loads.
- WebSocket events update caches optimistically only when event sequence is continuous.
- If a gap is detected, mark route stale and refetch.
- Mutations use action preview and idempotent execute.
- Query keys include host/owner/repo/ref/path/hash.
- Do not persist sensitive data in local storage.

### 13.3 Route model

Suggested route patterns:

```text
/
/repos
/repos/new
/:host/:owner/:repo
/:host/:owner/:repo/code/:ref/*path
/:host/:owner/:repo/blob/:ref/*path
/:host/:owner/:repo/commits
/:host/:owner/:repo/commit/:sha
/:host/:owner/:repo/branches
/:host/:owner/:repo/tags
/:host/:owner/:repo/compare/:base...:head
/:host/:owner/:repo/merge-requests
/:host/:owner/:repo/merge-requests/:number
/:host/:owner/:repo/merge-requests/:number/files
/:host/:owner/:repo/issues
/:host/:owner/:repo/issues/:number
/:host/:owner/:repo/projects
/:host/:owner/:repo/actions
/:host/:owner/:repo/insights
/:host/:owner/:repo/settings/:section?
/reviews
/merge-room
/agents
/notifications
/audit
/settings
```

### 13.4 Command palette registry

Commands should be registered with:

- ID;
- title;
- keywords;
- icon;
- permission requirement;
- route/action;
- context predicate;
- keyboard shortcut;
- risk tier.

Examples:

- `repo.open`
- `repo.create`
- `repo.copy_clone_url`
- `repo.render_readme`
- `code.open_file`
- `code.copy_permalink`
- `mr.approve`
- `mr.merge`
- `mr.request_review`
- `mr.resolve_thread`
- `issue.create`
- `settings.open`
- `settings.preview_save`
- `agent.open_session`
- `agent.approve_grant`
- `ci.rerun_failed`

---

## 14. Engineering diff overview

A separate `.diff` artifact accompanies this spec. Its required high-level changes are:

1. Modify `Cargo.toml` dependencies/features for Axum WebSocket/static files, Markdown rendering, session/auth, event streaming, and type/schema generation.
2. Modify root `package.json` to expose real web app scripts while preserving UX-QA.
3. Replace `apps/web/package.json` and add full Vite/React/TS app files.
4. Add `src/cli_defs_commands_web.rs`.
5. Modify `src/cli_defs.rs` for `Web` command and `Serve` flags.
6. Modify `src/dispatch.rs` to launch web server/dev/openapi/routes.
7. Modify `src/lib.rs` to export `web` and new API modules.
8. Modify `src/engine.rs` to support integrated web router or share state with `src/web`.
9. Add `src/api` web forge contracts.
10. Add `src/web` runtime, handlers, services, websocket, markdown, auth, audit, idempotency.
11. Expand `src/git_host` provider abstraction.
12. Add DB migrations for repos, markdown cache, review, settings, events, audit.
13. Add Rust integration tests.
14. Add frontend unit/E2E/Storybook/UX-QA tests.
15. Add docs and README updates.

---

## 15. Testing and proof plan

### 15.1 Rust validation

Required commands:

```bash
cargo check --workspace
cargo nextest run -p jeryu --lib
cargo nextest run --test mock_lifecycle_tests
cargo test -p jeryu --test '*' -- --test-threads=1
```

New test modules:

```text
src/web/tests.rs
src/web/ws_tests.rs
src/web/markdown_tests.rs
src/web/settings_tests.rs
tests/web_bootstrap_tests.rs
tests/web_repo_tests.rs
tests/web_markdown_tests.rs
tests/web_merge_tests.rs
tests/web_settings_tests.rs
tests/web_ws_tests.rs
```

Critical Rust cases:

- bootstrap returns viewer, permissions, websocket URL, and event cursor;
- repo list supports filters/pagination;
- repo create preview does not write provider/local state;
- repo create execute writes once under idempotency;
- README renders sanitized HTML;
- malicious Markdown/HTML is stripped;
- relative Markdown links are rewritten;
- binary files are not decoded as text;
- large Markdown falls back safely;
- settings patch rejects stale hash;
- approval rejects stale head SHA;
- merge rejects stale head SHA;
- merge rejects failing passport;
- action receipt is written for every mutation;
- websocket hello/resume works;
- websocket backpressure returns snapshot required;
- route permissions are enforced backend-side.

### 15.2 Frontend validation

Required commands:

```bash
npm run typecheck
npm run lint
npm run test
npm run build
npm run build-storybook
npm run test:e2e
npm run ux-qa
```

Critical frontend cases:

- app loads bootstrap and shows live badge;
- all repos dashboard filters/groups/sorts;
- create repo preview/execute flow;
- repo overview renders README HTML;
- code browser handles text/Markdown/binary/large files;
- Markdown TOC/anchors work;
- MR detail shows Merge Passport;
- approval disabled when SHA stale;
- merge disabled when passport blocked;
- settings page search and preview work;
- websocket event updates live dock;
- websocket gap triggers refetch;
- keyboard navigation works;
- command palette actions respect permissions;
- a11y tests pass for primary flows.

### 15.3 Visual and UX proof

Keep and extend the UX-QA evidence lane:

- Storybook state coverage: empty/loading/error/success/stale/live/disconnected/danger.
- Playwright screenshots for dashboard, repo overview, README, code browser, MR review, settings, command palette.
- Geometry checks: no overlapping panels, sticky headers work, Live Dock collapse works.
- Accessibility automation: landmarks, focus traps, labels, keyboard-only flows.
- Layout stability: virtualized diff and tree do not thrash.
- MSW mocks for full flows.
- Proof receipts for every critical scenario.

---

## 16. Rollout plan

### Phase 0: Contracts and build hygiene

- Add web API contract modules in `src/api`.
- Add provider trait skeleton.
- Add web feature/dependencies.
- Add DB migrations as no-op-compatible if needed.
- Add `jeryu web routes` placeholder.

Exit criteria:

- `cargo check --workspace` green.
- Contracts serialize/deserialize.
- No behavior change to existing `jeryu serve`.

### Phase 1: Web shell and bootstrap

- Replace `apps/web` placeholder with Vite app.
- Add app shell, router, query client, event store.
- Add `/api/v1/bootstrap`.
- Serve static assets.
- Add `jeryu web serve`.
- Add websocket hello/heartbeat without full event fanout.

Exit criteria:

- `jeryu web serve --open` launches UI.
- UI shows viewer, recent repos placeholder, live connection badge.

### Phase 2: Repositories and README

- Implement repo list from provider/local cache.
- Implement repo create preview/execute.
- Implement repo overview.
- Implement README endpoint and Markdown rendering.
- Implement MarkdownRenderer component.

Exit criteria:

- User can see all repos.
- User can create a repo.
- User can open repo and see sanitized rendered README.

### Phase 3: Code browser

- Implement refs/tree/blob/commits/compare.
- Add virtualized file tree.
- Add code viewer and binary handling.
- Add branch/tag picker.
- Add link rewriting/permalinks.

Exit criteria:

- User can browse branches, directories, files, Markdown, and diffs.

### Phase 4: Merge room and review

- Implement MR/PR list/detail.
- Implement files changed/diff viewer.
- Implement comments/review threads.
- Implement Merge Passport.
- Implement approval and merge with exact-SHA safety.

Exit criteria:

- User can review files, comment, approve, and merge safely.

### Phase 5: Issues/projects/agents

- Implement issue lists/details/creation.
- Implement project boards.
- Implement agent activity and evidence panels.
- Link agent findings to issues/MRs.

Exit criteria:

- User can triage issues and monitor/approve agent work from web.

### Phase 6: Settings and audit

- Implement settings schema/effective view.
- Implement searchable settings UI.
- Implement preview/patch/rollback/audit.
- Map provider-specific settings.

Exit criteria:

- User can safely change repo settings with preview and audit receipt.

### Phase 7: Realtime completeness and polish

- Durable event store.
- Scoped subscriptions.
- Gap recovery.
- Live Dock filters.
- Notifications.
- Visual/performance/a11y hardening.

Exit criteria:

- Primary views update in real time and recover from disconnects.

---

## 17. Performance targets

| Area | Target |
|---|---:|
| Initial critical JS gzip | < 350 KB before route chunks |
| First useful paint local | < 1.5 s |
| Route transition after bootstrap | < 100 ms perceived |
| Repo list filtering cached 5k repos | < 50 ms |
| File tree | virtualized; handles 100k entries |
| Diff viewer | virtualized; handles 20k lines |
| WebSocket local p95 delivery | < 250 ms |
| Markdown render cache hit | < 25 ms |
| Markdown render README cache miss | < 150 ms typical |
| Settings preview | < 500 ms excluding provider fetch |
| Merge passport refresh | < 1 s typical |

---

## 18. Highest-risk areas and mitigations

1. **Provider API mismatch.** GitHub and GitLab settings/review APIs differ. Mitigate with normalized models plus provider extension fields and a `ProviderCapability` matrix.
2. **Markdown security.** Mitigate with backend sanitize, frontend sanitize, malicious fixtures, cache versioning, external image proxy/blocking.
3. **Huge diffs/trees/logs.** Mitigate with pagination, streaming, virtualization, hard size limits, and lazy route chunks.
4. **Exact-SHA safety.** Mitigate by refetching live state immediately before approval/merge and rejecting stale heads.
5. **Permissions.** Mitigate by enforcing every action server-side; frontend permission hiding is only UX.
6. **WebSocket backpressure.** Mitigate with bounded channels, subscriptions, event cursors, snapshot-required recovery.
7. **Settings complexity.** Mitigate with schema-driven settings UI, preview-only first, and provider capability explanations.
8. **Dependency sprawl.** Mitigate by pinning versions, using minimal dependencies where feasible, and tracking app bundle size.
9. **Existing behavior regression.** Keep `jeryu serve` engine-only behavior unchanged unless `--web` is passed.
10. **State drift.** Use provider ETags/hashes, explicit refresh, and stale state hash rejection.

---

## 19. Acceptance criteria

The feature is complete when all of the following are true:

1. `jeryu web serve --open` launches a browser UI.
2. `jeryu serve --web` launches integrated engine + browser UI while preserving existing engine behavior.
3. UI lists all accessible repositories across configured hosts.
4. User can create a repository with dry-run preview, permission check, idempotency, audit receipt, and websocket update.
5. User can open any repository overview and see a correctly rendered sanitized README.
6. User can browse branches, tags, trees, files, commits, compare views, and Markdown files.
7. User can open merge requests/pull requests.
8. User can review changed files and submit comments.
9. User can approve a merge request only when expected head SHA matches.
10. User can merge only when live gates pass and expected head SHA matches.
11. User can view and change repo settings through searchable settings pages with preview and audit.
12. WebSocket updates activity, CI/checks, agents, settings, notifications, and merge posture in real time.
13. WebSocket disconnect/gap recovery is visible and correct.
14. All mutating actions write audit receipts.
15. Frontend has Storybook, unit tests, Playwright E2E, accessibility checks, and visual proof artifacts.
16. Rust, frontend, UX-QA, and integration tests are green.
17. README and `.md` rendering is safe, cached, link-rewritten, and covered by malicious fixture tests.

---

## 20. Definition of “better than GitHub/GitLab”

GitHub/GitLab parity is the baseline. JeRyu wins when the user spends less time hunting and more time deciding.

The final UX should make these actions one or two steps:

- “Show me every repo needing my attention.”
- “Create a private Rust repo with strict branch protection and README.”
- “Open the failing merge request and show me the exact blocker.”
- “Approve this MR only if the head SHA is still what I reviewed.”
- “Render README.md and keep relative links working.”
- “Show all settings that affect merging.”
- “Preview what this settings change will do before applying it.”
- “Show me live activity across all repos.”
- “Show agent patches and the evidence behind them.”
- “Explain why this cannot merge yet.”

That is the JeRyu Web Forge experience: a safer, faster, clearer, real-time forge for code, CI, agents, and review.
