# WEB_WORK_CLAUDE.md — JeRyu Web Forge Parallel Execution Plan

> **Deliverable file path:** `/home/ubuntu/jeryu/WEB_WORK_CLAUDE.md` (this content will be saved there after approval).
> **Plan mode mirror:** `/home/ubuntu/.claude/plans/please-study-home-ubuntu-jeryu-tips-web-splendid-volcano.md`
> **Document version:** v1.0 (2026-05-26)
> **Author:** Claude Opus 4.7 (1M context) on behalf of `jepson@veox.ai`

---

## 0. CONTEXT

### 0.1 Why this plan exists

The user requested a Vite + TypeScript + React web experience for the JeRyu monorepo that is "like GitHub/GitLab but better": browse all repos, render `README.md` correctly to HTML, navigate code, add new repos, change settings, and connect **directly to the internal GitLab** so live branches and code are visible. Full Playwright UX/UI coverage is required.

The user asked us to study `/home/ubuntu/jeryu/tips/web/*` and synthesize **one complete plan that any team of agents (or one agent doing everything) can execute end-to-end**. This file is that plan.

### 0.2 Source material (canonical inputs)

Inputs that already exist and SHOULD be treated as authoritative low-level reference. Do not duplicate; this plan REFERENCES them.

| Input | Purpose |
|---|---|
| `tips/web/JERYU_FULL_WEB_FORGE_ENGINEERING_SPEC_FINAL.md` | Canonical 132K-char engineering spec (DTOs, REST/WS contracts, migrations, code snippets). Always trust this for implementation detail. |
| `tips/web/JERYU_WEB_FORGE_ENGINEERING_SPEC.md` and friends | Earlier drafts; refer only to disambiguate. FINAL supersedes. |
| `tips/web/*.diff` (multiple) | Diff-style sketches of the change. Useful as code seeds but not authoritative. |
| `agent/boundaries.toml` | TS/web import bans, Rust domain restrictions. Plan MUST respect. |
| `agent/ux-qa.toml` | UX-QA proof requirements (loading/empty/error/success/permission-denied states, screenshots, ARIA snapshots, axe scans). |
| `AGENTS.md`, `agent/JANKURAI_STANDARD.md` | Project conventions, commit style, co-author footer. |
| `.jeryu/repos.toml` | Existing multi-repo registry shape (alias/slug/provider/local_root/...). |
| `src/git_host/{mod,gitlab,gitlab_client,gitlab_helpers,github}.rs` | Existing GitHost trait + GitLab adapter (merge-gate focused; needs expansion). |
| `src/api/{mod,actions,agent_session,entity,events,read_model,snapshot}.rs` | Existing typed-API foundation. New DTOs sit alongside. |
| `apps/web/{package.json,AGENTS.md,ux-qa-check.mjs,ux-qa.ts,ux-qa.md}` | Current UX-QA placeholder. Must be replaced while preserving the marker harness. |

### 0.3 How to use this document

1. **One agent doing all work:** read sections 1–4 for vision/architecture, then execute work packages in dependency order (section 7). Use sections 8–11 as live reference.
2. **Many agents in parallel:** read sections 1–4, claim work packages from section 7, follow the claim/sync protocol in section 12, and report per the Definition of Done in section 13.
3. **Auditor / reviewer:** read section 9 (acceptance criteria), section 10 (Playwright suite), and section 14 (risk register).

### 0.4 Reading aids

- All Rust file paths are relative to `/home/ubuntu/jeryu/`.
- All frontend file paths are relative to `/home/ubuntu/jeryu/apps/web/` unless noted.
- A bracket-tagged path `[NEW]` means create, `[MOD]` means modify, `[KEEP]` means preserve as-is.
- Work-package IDs use the form `W-<tier>-<NN>`. Tiers: `F` Foundation, `B` Backend, `H` Host adapter, `FE` Frontend, `T` Test, `D` Docs, `CC` Cross-cutting.

---

## 1. PRODUCT VISION

### 1.1 Mission (verbatim from FINAL spec §3, refined)

Build a GitHub/GitLab-class forge that feels **faster, safer, and less confusing** than the upstream products, by:

- Collapsing checks/comments/bots/CI/agents into one **Merge Passport** verdict.
- Streaming everything over WebSocket; polling only as fallback.
- Showing every dangerous mutation with **preview + risk tier + grant + audit receipt + exact-SHA binding** when applicable.
- Rendering Markdown twice-sanitized (server `ammonia` → client `DOMPurify`) and cached by `(repo, ref, path, blob_sha, renderer_version)`.
- Using virtualized lists/trees/diffs everywhere (TanStack Virtual).
- Driving 100% of high-value actions from a `⌘K` command palette and consistent keyboard shortcuts (`g r`, `g m`, `g s`, `[`, `]`, `j`/`k`, `Enter`, `Esc`, `?`).

### 1.2 Top differentiators ("better than GitHub/GitLab")

1. **One attention-driven dashboard** answering "what needs my attention right now?"
2. **One merge room** with diff, checks, comments, agents, gates, and blockers on one page.
3. **One live activity dock** (right rail) showing all relevant events streamed from the bus.
4. **One command palette** for every action with risk preview.
5. **One Merge Passport** boolean derived from approvals + checks + agent evidence + branch protection + Tip1 Law 4 (exact-SHA).
6. **One settings surface** with global search, blast-radius preview, and undo path on every change.
7. **One renderer** (`jeryu-markdown.v1`) for README, MR descriptions, comments, docs, release notes, evidence packs.

### 1.3 Non-goals (for v1)

- No "Discussions" tab clone (GitHub-style). Threads live in MRs and issues only.
- No GitHub Pages / GitLab Pages hosting from the web app itself.
- No marketplace/apps directory beyond installed integrations metadata.
- No mobile-first responsive layout for diff viewer (desktop-first; mobile keeps repo browsing and reading paths).
- No replacement of TUI; web and TUI share the read-model surface.

### 1.4 Connection to internal GitLab (user's stated requirement)

The web app **never** calls GitLab from the browser. Instead:

1. The browser calls `/api/...` on `src/web/` (Axum BFF inside the existing `jeryu` binary).
2. `src/web/` calls `src/repos::RepoService`, `src/repo_browser::RepoBrowserService`, `src/merge::MergeService`, `src/repos::settings::SettingsService`.
3. Those services consult the local DB cache (`db/migrations/202606010001_web_forge_core.sql`) and call `src/git_host/gitlab.rs::GitLabClient` when the cache is stale or a mutation is requested.
4. GitLab credentials live in the existing JeRyu secret chain (do not put tokens in the SPA bundle).
5. The internal GitLab base URL is configured via environment variable (e.g. `JERYU_GITLAB_BASE_URL`) consumed by `GitlabClient::new`.

The browser sees live branches, code, MRs because every page either subscribes to the relevant WebSocket scope (`repo:<slug>`, `mr:<slug>/<iid>`) or refetches via React Query on focus/interval — **the BFF keeps a synced cache so render is fast, and the client never blocks on a remote GitLab call inside the render path.**

---

## 2. ARCHITECTURE OVERVIEW

### 2.1 Process model

```
                                ┌─────────────────────────────────────┐
Browser                         │  jeryu binary (single process)      │
  │  ┌─────────────┐            │                                     │
  │──▶ apps/web    │──┐         │  ┌──────────────┐                   │
  │  │ Vite SPA    │  │  HTTP   │  │ src/web/     │  internal calls   │
  │  │ TS/React    │  │ /api/*  │  │ Axum BFF     │──┐                │
  │  └─────────────┘  ├────────▶│  │ + WS /api/ws │  │                │
  │  WebSocket        │         │  └──────────────┘  │                │
  │  (jeryu.ws.v1)    └────────▶│         ▲          │                │
  │                             │         │          ▼                │
  │                             │  ┌──────┴────────────────────────┐  │
  │                             │  │ src/repos / src/repo_browser  │  │
  │                             │  │ src/merge / src/repos::settings│ │
  │                             │  │ src/web_events::bus            │ │
  │                             │  └────────┬───────────────────────┘  │
  │                             │           │                          │
  │                             │           ▼                          │
  │                             │  ┌────────┴──────────┐               │
  │                             │  │ src/git_host::    │               │
  │                             │  │   GitLabClient    │──▶ GitLab API │
  │                             │  └─────────┬─────────┘               │
  │                             │            │                         │
  │                             │            ▼                         │
  │                             │  ┌────────┴──────────┐               │
  │                             │  │ src/db (SQLite)   │               │
  │                             │  └───────────────────┘               │
  └─────────────────────────────┴─────────────────────────────────────┘
```

Single binary `jeryu` exposes a new subcommand `jeryu web serve`. In dev mode it can proxy SPA assets from a Vite dev server at `127.0.0.1:5173`; in prod it serves the built bundle from `apps/web/dist`.

### 2.2 Data flow

1. **Bootstrap (cold load):** browser fetches `GET /api/bootstrap` → returns `WebBootstrap` (viewer, permissions, recent repos, TUI read-model snapshot, WS URL, feature flags). One round-trip to first useful paint (<1.5 s on local).
2. **WebSocket connect:** browser opens `GET /api/ws` (Upgrade); sends `Hello { resume_from, subscriptions: [{scope:"global"}, ...] }`. Server replies `Hello { current_seq, protocol: "jeryu.ws.v1" }`. Server then streams `Event` frames; client applies in seq order.
3. **Route navigation:** browser opens new route → React Query fires the relevant REST query (`/api/repos/{host}/{owner}/{repo}/...`). On focus return / WS event invalidation, React Query refetches.
4. **Mutation:** UI shows preview via `POST /api/actions/{action_id}/preview` or domain-specific preview endpoint, then executes via `POST /api/actions/{action_id}/execute` or `PATCH /api/.../settings` with `base_settings_hash` + `idempotency_key`. Server: refetch live state → validate → write → audit → publish WS event → respond.

### 2.3 Boundaries (do not violate)

- `agent/boundaries.toml` forbids the following in `apps/web/`, `packages/web/`, `packages/ui/`: `sqlx`, `mysql`, `@aws-sdk/client-s3`. The web bundle **must not import database drivers, cloud SDKs, or backend secrets**.
- Rust domain code (under `src/<domain>/`) MUST NOT import `std::fs`, `std::env`, `std::net`, `std::time::SystemTime`, `rand`, `sqlx`, `diesel`, `reqwest`, `jansu`, `tracing`, `log`. Domain code uses injected ports.
- `src/api/` remains the single typed-contract source of truth; the web BFF does not invent ad-hoc shapes.
- DB writes for new tables stay in `src/db/` per `agent/boundaries.toml`.

### 2.4 Canonical target tree (verbatim from FINAL spec §16)

```
jeryu/
├── apps/
│   ├── api/AGENTS.md                            [KEEP]
│   └── web/
│       ├── package.json                         [MOD]  @jeryu/web Vite/React/TS
│       ├── index.html                           [NEW]
│       ├── vite.config.ts                       [NEW]
│       ├── tsconfig.json                        [NEW]
│       ├── playwright.config.ts                 [NEW]
│       ├── ux-qa-check.mjs                      [MOD]  upgraded proof collector
│       ├── ux-qa.md                             [MOD]  expanded markers
│       ├── ux-qa.ts                             [MOD]  expanded markers
│       └── src/
│           ├── main.tsx                         [NEW]
│           ├── app/{App,router,providers}.tsx   [NEW]
│           ├── api/{client,endpoints,schemas,types,websocket}.ts [NEW]
│           ├── layout/{AppShell,CommandPalette,GlobalHeader,LeftNav,LiveActivityDock,RepoSwitcher,StatusBar}.tsx [NEW]
│           ├── pages/{Dashboard,Repositories,RepositoryOverview,RepositoryCode,RepositoryFile,RepositoryMergeRequests,MergeRequest,RepositoryActions,RepositorySettings,AdminSettings,NotFound}Page.tsx [NEW]
│           ├── components/{action,repo,browser,merge,settings}/*.tsx [NEW]
│           ├── hooks/use*.ts                    [NEW]
│           ├── stores/{realtime,selection,command,preferences}Store.ts [NEW]
│           ├── styles/{tokens,app}.css          [NEW]
│           └── test/{mocks,server}.ts           [NEW]
├── src/
│   ├── api/{repository,repo_browser,merge_request,issues,settings,web_read_model,review}.rs [NEW]
│   ├── api/mod.rs                               [MOD]  verify exports, add new modules
│   ├── web/{mod,command,state,router,error,auth,csrf,static_assets,markdown,ws}.rs [NEW]
│   ├── web/rest/{bootstrap,repos,repo_browser,merge_requests,reviews,issues,settings,actions,search,ci,agents,activity}.rs [NEW]
│   ├── web_events/{mod,protocol,bus,projection,subscription}.rs [NEW]
│   ├── repos/{mod,service,providers,policy,settings,search,create,host_sync,models,permissions}.rs [NEW]
│   ├── repo_browser/{mod,service,git_tree,blob,commits,compare,blame,markdown,render_cache,diff}.rs [NEW]
│   ├── merge/{mod,service,review,merge_gate,suggestions,reviews,guards}.rs [NEW]
│   ├── issues/{mod,service,labels,milestones,projects}.rs [NEW]
│   └── git_host/{mod,github,gitlab,codeowners}.rs [MOD] expand trait + adapters
├── db/migrations/202606010001_web_forge_core.sql [NEW]
├── docs/{web-forge,WEB_API,WEBSOCKET_PROTOCOL,README_RENDERING,REVIEW_COCKPIT}.md [NEW]
├── schemas/{web-api.openapi.json,websocket-events.schema.json} [NEW generated]
└── tests/{web_api_tests,web_markdown_tests,web_ws_tests,web_review_tests,repo_lifecycle_tests,repo_settings_tests,permissions_tests,audit_tests,search_tests,web_api_schema_tests}.rs [NEW]
```

---

## 3. TECHNOLOGY DECISIONS (verbatim, treat as binding)

### 3.1 Backend (Rust)

Append to `Cargo.toml` `[dependencies]` (see FINAL spec §6.1):

```toml
axum = { version = "0.8", features = ["json", "ws", "macros", "multipart"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace", "fs", "compression-gzip", "compression-br", "set-header", "request-id", "timeout"] }
axum-extra = { version = "0.10", features = ["typed-header", "cookie"] }
tokio-stream = { version = "0.1", features = ["sync"] }
headers = "0.4"
bytes = "1"
mime_guess = "2"
async-stream = "0.3"
pulldown-cmark = { version = "0.13", default-features = false, features = ["html"] }
ammonia = "4"
comrak = { version = "0.37", default-features = false, optional = true }
utoipa = { version = "5", features = ["chrono", "uuid", "axum_extras"] }
utoipa-swagger-ui = { version = "9", features = ["axum"] }
ts-rs = { version = "10", features = ["chrono-impl", "uuid-impl"] }
schemars = { version = "1", features = ["chrono", "uuid1"] }
indexmap = { version = "2", features = ["serde"] }
serde_with = "3"
parking_lot = "0.12"
url = "2"
```

Add feature flag:

```toml
[features]
default = ["profile-sqlite-kafka", "demo-fixtures"]
web = []
```

Renderer choice: `pulldown-cmark` is required (default). `comrak` is gated `optional = true` and only flipped on if we need full GFM HTML (footnotes/autolinks beyond pulldown-cmark) — TBD by W-B-08.

### 3.2 Frontend (Vite + TS + React)

`apps/web/package.json` (replace placeholder; see FINAL spec §6.13):

Dependencies: `@monaco-editor/react`, `@tanstack/{react-query,react-table,react-virtual}`, `cmdk`, `dompurify`, `lucide-react`, `react`, `react-dom`, `react-markdown`, `react-router-dom`, `rehype-{autolink-headings,highlight,raw,sanitize,slug}`, `remark-gfm`, `zod`, `zustand`.

Dev dependencies: `@playwright/test`, `@storybook/{react-vite,addon-a11y,addon-vitest}`, `@testing-library/{jest-dom,react,user-event}`, `@types/{dompurify,node,react,react-dom}`, `@vitejs/plugin-react`, `axe-core`, `eslint`, `eslint-plugin-{jsx-a11y,react-hooks}`, `jsdom`, `msw`, `typescript`, `vite`, `vitest`.

**Versions:** Spec says `latest` everywhere; the implementing agent MUST pin exact versions and commit `package-lock.json` (CI must be deterministic). Use the most recent stable releases as of 2026-05-26 unless a specific version is dictated by `boundaries.toml` or a known incompatibility (e.g. React Router 6.4+ for `createBrowserRouter`).

Scripts:
```
dev: vite --host 127.0.0.1 --port 5173
build: tsc -b && vite build
preview: vite preview --host 127.0.0.1 --port 4173
typecheck: tsc -b --pretty false
lint: eslint .
test: vitest run
test:watch: vitest
test:e2e: playwright test
storybook: storybook dev -p 6006
build-storybook: storybook build
ux-qa: node ./ux-qa-check.mjs build && node ./ux-qa-check.mjs test
```

### 3.3 Vite config (`apps/web/vite.config.ts`)

```ts
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': { target: 'http://127.0.0.1:8787', ws: true },
    },
  },
  build: { outDir: 'dist', sourcemap: true },
});
```

### 3.4 TypeScript config (`apps/web/tsconfig.json`)

Target ES2022, JSX `react-jsx`, strict, module bundler resolution. See FINAL spec §6.13 verbatim block.

### 3.5 Ports & dev URL convention

- Backend bind: `127.0.0.1:8787` (CLI flag `--bind`).
- Vite dev server: `127.0.0.1:5173`.
- Vite preview: `127.0.0.1:4173`.
- Storybook: `127.0.0.1:6006`.
- WebSocket: same origin as backend `ws://127.0.0.1:8787/api/ws` (or proxied through Vite at `ws://127.0.0.1:5173/api/ws`).

---

## 4. REPOSITORY LAYOUT — FILE OWNERSHIP MATRIX

| Path | Type | Owner work package | Notes |
|---|---|---|---|
| `Cargo.toml` | MOD | W-F-01 | Add deps + `web` feature flag |
| `package.json` (root) | MOD | W-F-09 | Add web workspace scripts |
| `src/api/mod.rs` | MOD | W-F-02 | Hygiene fix + new exports |
| `src/api/repository.rs` | NEW | W-F-03 | DTOs for repo summary/list/create |
| `src/api/repo_browser.rs` | NEW | W-F-03 | Refs/tree/blob/markdown DTOs |
| `src/api/merge_request.rs` | NEW | W-F-03 | MR summary, mergeability, postures |
| `src/api/settings.rs` | NEW | W-F-03 | Full settings tree |
| `src/api/web_read_model.rs` | NEW | W-F-03 | `WebBootstrap`, `Viewer`, feature flags |
| `src/api/review.rs` | NEW | W-F-03 | Review threads/comments |
| `src/api/issues.rs` | NEW | W-F-03 | Issue list/detail DTOs |
| `src/web/mod.rs` | NEW | W-B-01 | Module declarations |
| `src/web/state.rs` | NEW | W-B-01 | `WebState` Arc bundle |
| `src/web/router.rs` | NEW | W-B-02 | Axum Router assembly |
| `src/web/error.rs` | NEW | W-B-01 | `ApiError` + `IntoResponse` |
| `src/web/auth.rs` | NEW | W-B-01 | Session/CSRF middleware |
| `src/web/csrf.rs` | NEW | W-B-01 | CSRF token mint/verify |
| `src/web/static_assets.rs` | NEW | W-B-03 | SPA fallback service |
| `src/web/markdown.rs` | NEW | W-B-08 | Thin wrapper around `repo_browser::markdown` |
| `src/web/ws.rs` | NEW | W-B-04 | WebSocket handler |
| `src/web/command.rs` | NEW | W-F-10 | `jeryu web serve` impl |
| `src/web/rest/bootstrap.rs` | NEW | W-B-05 | `/api/bootstrap` |
| `src/web/rest/repos.rs` | NEW | W-B-06 | repo list/create/get |
| `src/web/rest/repo_browser.rs` | NEW | W-B-09 | refs/tree/blob/readme/compare |
| `src/web/rest/merge_requests.rs` | NEW | W-B-11 | MR list/detail/approve/merge |
| `src/web/rest/reviews.rs` | NEW | W-B-12 | threads/comments |
| `src/web/rest/issues.rs` | NEW | W-B-30 | (out of v1 critical path; placeholder) |
| `src/web/rest/settings.rs` | NEW | W-B-07 | settings get/preview/patch |
| `src/web/rest/actions.rs` | NEW | W-B-15 | preview/execute generic |
| `src/web/rest/search.rs` | NEW | W-B-16 | global search |
| `src/web/rest/ci.rs` | NEW | W-B-14 | runs/checks/jobs/logs |
| `src/web/rest/agents.rs` | NEW | W-B-31 | agent sessions/evidence |
| `src/web/rest/activity.rs` | NEW | W-B-17 | activity feed |
| `src/web_events/mod.rs` | NEW | W-F-06 | Module declarations |
| `src/web_events/protocol.rs` | NEW | W-F-06 | Client/Server WS messages, `WebEvent` |
| `src/web_events/bus.rs` | NEW | W-B-04 | `WebEventBus` with broadcast |
| `src/web_events/projection.rs` | NEW | W-B-04 | `TuiEvent -> WebEvent` |
| `src/web_events/subscription.rs` | NEW | W-B-04 | Per-connection state |
| `src/repos/mod.rs` | NEW | W-B-06 | Module declarations |
| `src/repos/service.rs` | NEW | W-B-06 | `RepoService` (list/create/get) |
| `src/repos/create.rs` | NEW | W-B-06 | Create preview + execute |
| `src/repos/host_sync.rs` | NEW | W-B-06 | Background sync from GitLab |
| `src/repos/models.rs` | NEW | W-B-06 | DB row structs |
| `src/repos/permissions.rs` | NEW | W-B-06 | Normalize host roles → JeRyu perms |
| `src/repos/settings.rs` | NEW | W-B-07 | `SettingsService` |
| `src/repos/search.rs` | NEW | W-B-16 | Repo search |
| `src/repos/providers.rs` | NEW | W-B-06 | Provider registry |
| `src/repos/policy.rs` | NEW | W-B-07 | Branch protection / merge policy logic |
| `src/repo_browser/mod.rs` | NEW | W-B-09 | Module declarations |
| `src/repo_browser/service.rs` | NEW | W-B-09 | `RepoBrowserService` |
| `src/repo_browser/git_tree.rs` | NEW | W-B-09 | Tree listing |
| `src/repo_browser/blob.rs` | NEW | W-B-09 | Blob fetch + binary detection |
| `src/repo_browser/markdown.rs` | NEW | W-B-08 | Renderer w/ `jeryu-markdown.v1` |
| `src/repo_browser/render_cache.rs` | NEW | W-B-08 | Cache by `(repo, ref, path, blob_sha, version)` |
| `src/repo_browser/commits.rs` | NEW | W-B-09 | Commit listing |
| `src/repo_browser/compare.rs` | NEW | W-B-10 | Branch compare |
| `src/repo_browser/blame.rs` | NEW | W-B-10 | Blame view |
| `src/repo_browser/diff.rs` | NEW | W-B-10 | Diff parsing |
| `src/merge/mod.rs` | NEW | W-B-11 | Module declarations |
| `src/merge/service.rs` | NEW | W-B-11 | `MergeService` |
| `src/merge/review.rs` | NEW | W-B-12 | Review service |
| `src/merge/reviews.rs` | NEW | W-B-12 | Thread state |
| `src/merge/merge_gate.rs` | NEW | W-B-13 | Merge Passport computation |
| `src/merge/guards.rs` | NEW | W-B-11 | Exact-SHA refetch + verify |
| `src/merge/suggestions.rs` | NEW | W-B-12 | Inline suggestion handling |
| `src/issues/mod.rs` | NEW | W-B-30 | (placeholder for v1.5) |
| `src/git_host/mod.rs` | MOD | W-H-01 | Expand trait, add Host* models |
| `src/git_host/gitlab.rs` | MOD | W-H-02..05 | Implement new trait methods |
| `src/git_host/github.rs` | MOD | W-H-07 | Mirror GitLab implementation |
| `src/git_host/codeowners.rs` | KEEP | — | Already complete |
| `src/cli.rs` | MOD | W-F-10 | Add `Web(WebCommand)` |
| `src/dispatch.rs` | MOD | W-F-10 | Wire `Command::Web` |
| `db/migrations/202606010001_web_forge_core.sql` | NEW | W-F-05 | Tables + indexes |
| `apps/web/package.json` | MOD | W-F-07 | Replace placeholder |
| `apps/web/index.html` | NEW | W-F-07 | Vite entry |
| `apps/web/vite.config.ts` | NEW | W-F-07 | Proxy to backend |
| `apps/web/tsconfig.json` | NEW | W-F-07 | Strict TS |
| `apps/web/playwright.config.ts` | NEW | W-T-08 | E2E config |
| `apps/web/ux-qa-check.mjs` | MOD | W-T-19 | Upgrade to real proof collector |
| `apps/web/src/main.tsx` | NEW | W-FE-02 | Root |
| `apps/web/src/app/*.tsx` | NEW | W-FE-02 | App/router/providers |
| `apps/web/src/api/*.ts` | NEW | W-FE-03 + W-FE-04 | client/endpoints/schemas/types/websocket |
| `apps/web/src/layout/*.tsx` | NEW | W-FE-01 | Shell layout |
| `apps/web/src/pages/*.tsx` | NEW | W-FE-07..12 | Per-feature page |
| `apps/web/src/components/action/*.tsx` | NEW | W-FE-13 | Action UX |
| `apps/web/src/components/repo/*.tsx` | NEW | W-FE-08 | Repo cards/table/dialogs |
| `apps/web/src/components/browser/*.tsx` | NEW | W-FE-09 + W-FE-10 | README + code |
| `apps/web/src/components/merge/*.tsx` | NEW | W-FE-11 | MR cockpit |
| `apps/web/src/components/settings/*.tsx` | NEW | W-FE-12 | Settings editors |
| `apps/web/src/hooks/*.ts` | NEW | W-FE-06 | React Query hooks |
| `apps/web/src/stores/*.ts` | NEW | W-FE-05 | Zustand stores |
| `apps/web/src/styles/*.css` | NEW | W-CC-01 | Design tokens |
| `apps/web/src/test/{mocks,server}.ts` | NEW | W-T-05 | MSW |
| `docs/web-forge.md` | NEW | W-D-01 | Architecture |
| `docs/WEB_API.md` | NEW | W-D-02 | REST reference |
| `docs/WEBSOCKET_PROTOCOL.md` | NEW | W-D-03 | WS protocol |
| `docs/README_RENDERING.md` | NEW | W-D-04 | Markdown security |
| `docs/REVIEW_COCKPIT.md` | NEW | W-D-05 | Merge room |
| `apps/web/README.md` | NEW | W-D-06 | Frontend guide |
| `README.md` | MOD | W-D-07 | Add Web Forge section |
| `tests/web_api_tests.rs` | NEW | W-T-03 | REST integration |
| `tests/web_markdown_tests.rs` | NEW | W-T-01 | Markdown XSS / GFM |
| `tests/web_ws_tests.rs` | NEW | W-T-04 | WS protocol |
| `tests/web_review_tests.rs` | NEW | W-T-03 | Review flow |
| `tests/repo_lifecycle_tests.rs` | NEW | W-T-02 | Repo create/import |
| `tests/repo_settings_tests.rs` | NEW | W-T-02 | Settings patch |
| `tests/permissions_tests.rs` | NEW | W-T-02 | Perm matrix |
| `tests/audit_tests.rs` | NEW | W-T-02 | Audit on every mutation |
| `tests/search_tests.rs` | NEW | W-T-02 | Global search |
| `tests/web_api_schema_tests.rs` | NEW | W-T-04 | Schema/TS export |

---

## 5. WORK-PACKAGE DEPENDENCY GRAPH

```
                        ┌──────────────────────────────────────┐
                        │  FOUNDATION (W-F-*)  must run first  │
                        └──────────────────────────────────────┘
                                        │
        ┌───────────────────────────────┼───────────────────────────────┐
        ▼                               ▼                               ▼
  ┌──────────┐                   ┌──────────────┐               ┌──────────────┐
  │ BACKEND  │ ◀─ depends on ─▶  │ HOST ADAPTER │               │ FRONTEND     │
  │ W-B-*    │                   │ W-H-*        │               │ W-FE-*       │
  └────┬─────┘                   └──────┬───────┘               └──────┬───────┘
       │                                │                              │
       │       ┌────────────────────────┼──────────────────────────────┘
       │       │                        │
       ▼       ▼                        ▼
   ┌───────────────────────────────────────┐
   │  TESTING  W-T-*  (parallel per scope) │
   └───────────────────────────────────────┘
                       │
                       ▼
                ┌──────────────┐
                │ DOCS  W-D-*  │
                └──────────────┘

Cross-cutting W-CC-* runs alongside in parallel from day one.
```

### Foundation tier internal dependencies

```
W-F-01 (Cargo deps)──┐
W-F-02 (api hygiene) │──▶ W-F-03 (DTOs) ──▶ W-F-04 (ts-rs export)
                     │
W-F-05 (DB migration)│ (independent)
W-F-06 (WS protocol) │ (independent)
W-F-07 (apps/web skel)──▶ W-F-08 (design tokens) (independent)
W-F-09 (root scripts) (independent)
W-F-10 (CLI stub) ──── requires W-F-01 only
```

All of W-F-* can be claimed in parallel except W-F-03 depends on W-F-02, and W-F-04 depends on W-F-03.

### Backend tier dependencies

```
W-B-01 (state/error/auth) ──┬──▶ W-B-02 (router)
                            ├──▶ W-B-03 (static assets)
                            ├──▶ W-B-04 (ws + bus)
                            ├──▶ W-B-05 (bootstrap)
W-B-08 (markdown renderer) (depends only on W-F-03)
W-B-06 (repos service) ────▶ depends W-B-01 + W-H-02
W-B-07 (settings service)──▶ depends W-B-01 + W-H-02 + W-H-03
W-B-09 (repo_browser svc)─▶ depends W-B-01 + W-B-08 + W-H-02
W-B-10 (compare/diff/blame)▶ depends W-B-09 + W-H-02
W-B-11 (merge service) ───▶ depends W-B-01 + W-H-04 + W-H-05
W-B-12 (reviews) ─────────▶ depends W-B-11 + W-H-04
W-B-13 (merge gate) ──────▶ depends W-B-11
W-B-14 (CI endpoints) ────▶ depends W-B-01 + W-H-06
W-B-15 (actions) ─────────▶ depends W-B-01
W-B-16 (search) ──────────▶ depends W-B-06
W-B-17 (activity) ────────▶ depends W-B-04
```

### Host adapter tier

```
W-H-01 (trait expansion) ──┬──▶ W-H-02 (GitLab read)
                           ├──▶ W-H-03 (GitLab write)
                           ├──▶ W-H-04 (GitLab MR)
                           ├──▶ W-H-05 (GitLab merge)
                           ├──▶ W-H-06 (GitLab CI)
                           └──▶ W-H-07 (GitHub mirror, optional v1)
```

All of W-H-02..06 can run in parallel once W-H-01 lands.

### Frontend tier

```
W-FE-02 (router/providers) ──┐
W-FE-03 (API client) ────────┤
W-FE-04 (WS client) ─────────┤── prerequisite for all pages
W-FE-05 (stores) ────────────┤
W-FE-06 (hooks) ─────────────┘

W-FE-01 (shell layout) ──▶ depends W-FE-02
W-FE-07 (Dashboard) ─────▶ depends W-FE-01..06
W-FE-08 (Repos list+create)▶ depends W-FE-01..06
W-FE-09 (Repo overview+README)▶ depends W-FE-01..06, W-B-08
W-FE-10 (Code browser)──▶ depends W-FE-01..06
W-FE-11 (MR cockpit) ───▶ depends W-FE-01..06
W-FE-12 (Settings) ─────▶ depends W-FE-01..06
W-FE-13 (Action UX) ────▶ depends W-FE-02
W-FE-14 (Command palette)▶ depends W-FE-01, W-FE-05
W-FE-15 (Error states) ─▶ no deps
W-FE-16 (Keyboard) ─────▶ depends W-FE-14
```

### Critical path (minimum viable web that renders a README)

```
W-F-01 → W-F-02 → W-F-03 → W-F-07 → W-F-10 → W-H-01 → W-H-02 → W-B-08 →
W-B-09 → W-B-01 → W-B-02 → W-B-05 → W-B-06 → W-FE-02 → W-FE-03 → W-FE-01 →
W-FE-09 → W-T-08 → W-T-11
```

Approx 19 packages on the critical path. Everything else parallelizes around this spine.

---

## 6. AGENT CLAIM & SYNC PROTOCOL

### 6.1 Branch convention

`web/<work-package-id>-<short-slug>`. Examples:
- `web/W-F-03-api-dtos`
- `web/W-B-08-markdown-renderer`
- `web/W-FE-09-repo-overview`

### 6.2 Claim procedure

1. Read `tips/web/CLAIMS.md` (create as empty file in W-F-00 if not present).
2. Append a row: `| W-X-NN | <agent-name> | <YYYY-MM-DD HH:MM UTC> | in-progress | <branch> |`.
3. Push the branch immediately so other agents can see the claim.
4. Move to `done` after PR merges. Move to `abandoned` with a reason if stopping.

### 6.3 Commit style (verbatim project convention)

```
<type>(<scope>): <imperative subject>

<optional body>

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

Types: `feat`, `fix`, `chore`, `style`, `ci`, `docs`, `test`, `refactor`. Scopes for this project: `web`, `api`, `web-events`, `repos`, `repo-browser`, `merge`, `git-host`, `db`, `cli`, `apps-web`, `tests`, `docs`, `ux-qa`.

### 6.4 PR convention

- Title under 70 chars; body documents which work-package(s) this PR closes.
- Body sections: **Summary** (1–3 bullets), **Test plan** (checklist), **Risk** (1–2 lines on blast radius).
- Mark draft until tests are green locally.
- Link to the next dependent work package(s) in the body so reviewers know what unblocks.

### 6.5 Cross-package collision prevention

Agents working in adjacent packages should:
- Use the file ownership matrix (section 4) as authoritative.
- If two packages must edit the same file (e.g., `src/web/router.rs`), the LATER package rebases on the EARLIER one and only adds new routes.
- For `Cargo.toml`, all dep additions go through W-F-01 first; later packages add deps via follow-on PRs only when necessary.

### 6.6 Proof lane

Every PR runs the JeRyu `just fast` lane locally before push. Web-specific PRs additionally run:

```
cd apps/web && npm run typecheck && npm run lint && npm run test
cd apps/web && npm run build
cargo nextest run -p jeryu --features web
```

UX-QA artifacts are produced by W-T-19's harness; PRs touching UI must include the latest artifact JSON receipt.

---

## 7. WORK PACKAGES (DETAILED)

> Each work package below uses the schema: **ID · Title · Tier · Depends-on · Files · Steps · Acceptance · Tests · Estimated size**. Size: `S` ≤ 0.5d, `M` 0.5–1.5d, `L` 1.5–3d agent-days.

### 7.0 Foundation tier (W-F-*)

#### W-F-00 · Initialize claim tracker · F · — · `tips/web/CLAIMS.md` · S
**Steps:** Create the empty CLAIMS.md table with column headers (id, agent, started, status, branch).
**Acceptance:** File exists; first claim appended by next package.

#### W-F-01 · Cargo dependencies + `web` feature flag · F · — · `Cargo.toml` · M
**Steps:** Add all crates listed in §3.1. Add `web = []` feature. Update `tower-http` features. Run `cargo check --workspace --features web`.
**Acceptance:** `cargo check --workspace` green; `cargo check --workspace --features web` green.
**Tests:** Existing test suite still passes (`cargo nextest run -p jeryu`).

#### W-F-02 · `src/api/mod.rs` hygiene · F · W-F-01 · `src/api/mod.rs` · S
**Steps:** Verify every export exists. Current exports: `actions`, `agent_session`, `entity`, `events`, `read_model`, `snapshot`. The spec mentions `capacity`, `dashboards`, `freshness`, `inspection`, `proof`, `runtime_profile` — check whether they exist; add them as empty modules `pub mod foo;` with placeholder bodies OR remove the export from `mod.rs`. **Do not silently drop** if downstream code references them — search with `rg 'crate::api::(capacity|dashboards|freshness|inspection|proof|runtime_profile)'` first.
**Acceptance:** `cargo check --workspace` green with no missing-module warnings.
**Tests:** Compile-only.

#### W-F-03 · Add API DTOs · F · W-F-02 · `src/api/{repository,repo_browser,merge_request,settings,web_read_model,review,issues}.rs` + `src/api/mod.rs` · L
**Steps:** Add modules verbatim from FINAL spec §6.2 (DTOs already provided in spec). Derive `Serialize`, `Deserialize`, `Debug`, `Clone`; tag `#[serde(rename_all = "snake_case")]` on enums. Add `#[derive(utoipa::ToSchema)]` and `#[derive(schemars::JsonSchema)]` for OpenAPI. Add `#[derive(ts_rs::TS)] #[ts(export)]` so types regen on `cargo test`.
**Acceptance:** `cargo check --workspace` green; `cargo test --workspace` regenerates `apps/web/src/api/types.ts` (or whatever ts-rs is configured to output).
**Tests:** Round-trip serde tests for each enum and a representative struct (`RepositorySummary`, `MergeRequestSummary`, `RepositorySettings`).

#### W-F-04 · ts-rs export wiring · F · W-F-03 · `apps/web/src/api/types.ts` (generated) · M
**Steps:** Configure ts-rs export path in DTO derive macros (`#[ts(export, export_to = "../../apps/web/src/api/")]`). Add a Cargo bin `jeryu-export-types` or hook into `cargo test`. Document in `docs/web-forge.md` how to regenerate.
**Acceptance:** Running `cargo test --workspace` (or `cargo run --bin jeryu-export-types`) writes `apps/web/src/api/{RepositorySummary,...}.ts`. Each file is valid TypeScript and imports compile under `tsc --noEmit`.
**Tests:** `tests/web_api_schema_tests.rs` asserts each Rust DTO has a matching TS file; CI fails if a Rust struct has no `#[ts(export)]` derive.

#### W-F-05 · Database migration · F · W-F-01 · `db/migrations/202606010001_web_forge_core.sql` · M
**Steps:** Copy SQL verbatim from FINAL spec §6.11. Confirm the migration runner picks it up (check `src/db/mod.rs` or wherever migrations are listed). Add a follow-up migration `202606010002_web_forge_indexes.sql` if any indexes are missing.
**Acceptance:** Fresh DB instantiates all tables; `sqlite3 jeryu.db ".schema"` shows the new tables.
**Tests:** `tests/repo_lifecycle_tests.rs` happy-path insert/select round-trip.

#### W-F-06 · WebSocket protocol structs · F · W-F-03 · `src/web_events/{mod,protocol}.rs` · M
**Steps:** Add `ClientWsMessage` and `ServerWsMessage` enums (verbatim spec §6.4). Add `WebEvent` struct. Implement `From<TuiEvent> for WebEvent`. Add JSON round-trip serde tests.
**Acceptance:** All variants serialize/deserialize; tagged-union format matches FINAL §8 examples byte-for-byte for snapshot fixtures.
**Tests:** `tests/web_ws_tests.rs::protocol::*` — one test per variant with a JSON fixture.

#### W-F-07 · `apps/web` Vite skeleton · F · — · `apps/web/{package.json,index.html,vite.config.ts,tsconfig.json,.eslintrc.cjs,.prettierrc.json}` · M
**Steps:** Replace `apps/web/package.json` with content from FINAL §6.13. Add `index.html`, `vite.config.ts`, `tsconfig.json`. Run `npm install` from repo root after W-F-09 lands. Add ESLint config (recommended + `jsx-a11y` + `react-hooks`). Add Prettier config (2-space indent, single quotes, no trailing comma rest).
**Acceptance:** `cd apps/web && npm install && npm run build` produces `dist/` with `index.html` and JS bundle; `npm run typecheck` green; `npm run lint` green with zero warnings.
**Tests:** Smoke build only.

#### W-F-08 · Design tokens + base styles · F · W-F-07 · `apps/web/src/styles/{tokens.css,app.css}` · M
**Steps:** Define CSS custom properties for color, spacing, font scale, radii, shadows, transitions. Provide light/dark/high-contrast variants via `@media (prefers-color-scheme)` and `[data-theme="..."]`. Include the JeRyu visual brand (mission-control aesthetic from TUI). Tokens to define minimally: `--color-bg-{0,1,2,3}`, `--color-fg-{primary,secondary,muted}`, `--color-accent-{primary,success,warning,danger,info}`, `--font-{sans,mono}`, `--space-{1..8}`, `--radius-{sm,md,lg}`, `--shadow-{1,2,3}`, `--ease-{standard,emphasized}`, `--duration-{fast,std,slow}`.
**Acceptance:** Demo app rendered against tokens with all three themes switchable in <50 ms; no `style="..."` hex/rgb literals in components (use tokens).
**Tests:** Storybook a11y addon passes for the token gallery story.

#### W-F-09 · Root npm workspace scripts · F · W-F-07 · `package.json` (root) · S
**Steps:** Update root `package.json` per FINAL spec §6.1 (add `dev`, `build`, `preview`, `typecheck`, `lint`, `test`, `test:e2e`, `storybook`, `build-storybook` scripts that delegate to `@jeryu/web`). Decide whether to keep `@jankurai/ux-qa` as a parallel workspace at `apps/ux-qa/` or fold its scripts into `@jeryu/web`. **Recommendation:** keep both workspaces — `apps/web` for the product, `apps/ux-qa` for the proof harness — and update the workspaces array.
**Acceptance:** `npm install` at repo root resolves both workspaces; `npm run typecheck` etc. succeed.

#### W-F-10 · CLI stub for `jeryu web serve` · F · W-F-01 · `src/cli.rs`, `src/dispatch.rs`, `src/web/{mod,command}.rs` · M
**Steps:** Add `Web(WebCommand)` to the CLI enum; `WebCommand::{Serve, Open, BuildAssets}`. Wire `Command::Web(cmd) => crate::web::command::run(cmd).await` in dispatch. Implement `command::run` as a stub that prints "Web Forge server (not yet implemented)" and exits. Add the `--bind`, `--open`, `--dev-assets` flags per FINAL §6.12.
**Acceptance:** `cargo run -p jeryu -- web serve --bind 127.0.0.1:8787` prints the stub message and exits 0; `jeryu web --help` shows the new subcommands.

### 7.1 Cross-cutting tier (W-CC-*)

#### W-CC-01 · Theme system · CC · W-F-08 · `apps/web/src/stores/preferencesStore.ts`, `apps/web/src/app/providers.tsx` · M
**Steps:** Implement Zustand `preferencesStore` (theme/density/font size/keyboard mode/date format). Apply `data-theme` to `<html>` on change. Persist to `localStorage` as `jeryu.preferences.v1`.

#### W-CC-02 · Error/empty/loading state primitives · CC · W-F-08 · `apps/web/src/components/state/{LoadingState,EmptyState,ErrorState,PermissionDeniedState}.tsx` · M
**Steps:** Reusable components with consistent layout, illustration slots, and CTA. Used by every page hook for one of the five required UX-QA states. Each component takes `title`, `description`, optional `action`, optional `icon`.

#### W-CC-03 · Accessibility scaffolding · CC · W-F-08 · `apps/web/src/test/axe-setup.ts` · M
**Steps:** Configure axe-core to run in Vitest jsdom env. Provide `axe(container)` helper. All non-trivial components ship with an axe smoke test.

#### W-CC-04 · Keyboard shortcuts foundation · CC · W-F-07 · `apps/web/src/hooks/useKeyboard.ts`, `apps/web/src/components/KeyboardShortcutsOverlay.tsx` · M
**Steps:** Global `useKeyboardShortcut(key, handler, opts)` hook. Maintain registered shortcuts in a context for the `?` overlay. Implement default bindings: `⌘K/Ctrl+K` (palette), `/` (search), `g r`/`g m`/`g s`/`g d` (navigate), `j`/`k` (move selection), `Enter`/`Esc`, `[`/`]` (prev/next repo).

#### W-CC-05 · Audit infrastructure · CC · W-F-05 · `src/web/audit.rs` (NEW), `src/web/middleware/audit.rs` · M
**Steps:** A tower middleware that emits an audit row on every mutating response (status 200/201/204 to POST/PATCH/PUT/DELETE). Audit row contains: actor, action_id, target, risk_tier, preview_json, result_json, created_at. Writes via `src/db/audit_writer.rs`.

#### W-CC-06 · Idempotency keys · CC · W-F-05 · `src/web/idempotency.rs` · M
**Steps:** Store recent (key, response) tuples in a small SQLite table or in-memory LRU keyed by `(actor, action_id, idempotency_key)`. Repeated calls return the stored response without re-executing.

#### W-CC-07 · Permission gate middleware · CC · W-F-03 · `src/web/permissions.rs` · M
**Steps:** Map host roles → normalized perms (§3 below); attach to each request via `RequestPermissions` extractor. Every route lists `required_perms` and returns 403 when missing. UI obtains the viewer's perms from `WebBootstrap.viewer.global_permissions`.

#### W-CC-08 · Logging / telemetry · CC · W-F-01 · `src/web/telemetry.rs` · S
**Steps:** Tower request-id middleware + `tracing` instrumentation per route. Request-id surfaces in `ApiError.request_id`.

#### W-CC-09 · CSRF · CC · W-F-03 · `src/web/csrf.rs` · M
**Steps:** Double-submit cookie pattern; `__Host-jeryu-csrf` cookie + `X-CSRF-Token` header check on all mutating routes. Bypass for GET. Emit cookie on `/api/bootstrap`.

### 7.2 Backend tier (W-B-*)

#### W-B-01 · `src/web` state + error + auth · B · W-F-03, W-F-06 · `src/web/{mod,state,error,auth}.rs` · L
**Steps:** Implement `WebState` struct per FINAL §6.3 (Arc bundle of services). `ApiError` enum + `IntoResponse` impl (verbatim). `auth.rs`: cookie-based session lookup; on success injects a `Viewer` into request extensions; on failure short-circuits with 401.

#### W-B-02 · Router assembly · B · W-B-01 · `src/web/router.rs` · M
**Steps:** Verbatim from FINAL §6.3. Layered with compression, CORS (configurable; locked-down in prod), trace, request-id, timeout (30s), audit, idempotency middlewares.

#### W-B-03 · SPA static-asset fallback · B · W-B-01 · `src/web/static_assets.rs` · M
**Steps:** `spa_service()` returns a `ServeDir` rooted at `apps/web/dist` with a `not_found_service` that serves `index.html` so client-side routes resolve. In dev mode (`--dev-assets <url>`), reverse-proxy unmatched paths to the Vite dev server.

#### W-B-04 · WebSocket handler + event bus · B · W-F-06, W-B-01 · `src/web/ws.rs`, `src/web_events/{bus,projection,subscription}.rs` · L
**Steps:**
- `WebEventBus` from FINAL §6.4 (broadcast::channel capacity 4096 configurable).
- Per-connection subscription registry held in `ws.rs` (HashSet of scope strings).
- `handle_socket`: send Hello, loop tokio::select on incoming client msgs vs bus events; filter events against subscriptions before forwarding.
- `projection.rs`: convert `TuiEvent` to `WebEvent` (use the `From` impl from spec).
- Gap recovery: if the receiver returns `Lagged`, send `snapshot_required { reason: "lag" }`.

#### W-B-05 · `/api/bootstrap` · B · W-B-01, W-B-04, W-F-03 · `src/web/rest/bootstrap.rs` · M
**Steps:** Compose `WebBootstrap` from `Viewer` (from session), feature flags (from config), recent repos (from `RepoService::recent(viewer, 12)`), TUI read-model (from existing `read_model::current()`), WS URL (relative `/api/ws`).

#### W-B-06 · Repo service + REST · B · W-B-01, W-H-02, W-F-05 · `src/repos/{mod,service,create,host_sync,models,permissions,providers,search}.rs`, `src/web/rest/repos.rs` · L
**Steps:**
- `RepoService::list(query)` queries local cache; if cache is older than 5 minutes for a host, schedule a background `host_sync` via tokio::spawn that calls `GitLabClient::list_repositories(...)` and upserts rows.
- `RepoService::create(req)` calls `host.create_repository(...)`; persists; emits `repo.created` event.
- `RepoService::get(id)` reads cached row; if stale, refresh inline (short timeout).
- REST handlers: `GET /api/repos`, `POST /api/repos`, `GET /api/repos/{host}/{owner}/{repo}`. `POST` supports `dry_run=true` returning `CreateRepositoryPreview` without writes.

#### W-B-07 · Settings service + REST · B · W-B-01, W-H-02, W-H-03 · `src/repos/{settings,policy}.rs`, `src/web/rest/settings.rs` · L
**Steps:**
- `SettingsService::read(repo)` returns `RepositorySettings`.
- `SettingsService::preview_patch(repo, patch)` computes diff + blast radius (affected branches/MRs/jobs) + side effects + reversibility.
- `SettingsService::apply_patch(repo, base_hash, patch, idempotency_key)` rejects if `base_hash` ≠ current; otherwise applies via `host.update_repository_settings(...)`, persists, emits `repo.settings.changed`, writes audit.
- REST: `GET /api/repos/.../settings`, `POST /api/repos/.../settings/preview`, `PATCH /api/repos/.../settings`.

#### W-B-08 · Markdown renderer · B · W-F-03 · `src/repo_browser/{markdown,render_cache}.rs`, `src/web/markdown.rs` · L
**Steps:**
- `render_markdown(markdown, base_route) -> RenderedMarkdown` using `pulldown-cmark` with `ENABLE_TABLES | ENABLE_FOOTNOTES | ENABLE_STRIKETHROUGH | ENABLE_TASKLISTS | ENABLE_SMART_PUNCTUATION` (verbatim spec §6.6).
- Pipe HTML through `ammonia` with allowlists from spec: tags `table thead tbody tr th td input`; `<a>` attrs `href title rel target`; `<img>` attrs `src alt title width height`. Block `script`, event handlers (`on*`), `iframe`, `form`, style attrs unless allowed.
- `rewrite_relative_links(html, base_route)`: parse HTML with `scraper` or `html5ever`, rewrite `<a href="./foo.md">` to `<a href="/repos/<host>/<owner>/<repo>/blob/<ref>/foo.md">`. Same for `<img src="docs/x.png">` → authenticated blob URL.
- `extract_headings`: walk the markdown AST, collect `(depth, slug, text)`.
- `extract_links`: walk the AST, collect all anchor URLs + resolved routes + `external: bool`.
- `RENDERER_VERSION = "jeryu-markdown.v1"`.
- `render_cache.rs`: SQLite-backed cache keyed `(repo_id, ref_sha, path, blob_sha, renderer_version)`.

#### W-B-09 · Repo-browser service + REST · B · W-B-01, W-B-08, W-H-02 · `src/repo_browser/{mod,service,git_tree,blob,commits}.rs`, `src/web/rest/repo_browser.rs` · L
**Steps:**
- `RepoBrowserService::list_refs(repo)` → calls `host.list_refs(...)`, merges branches+tags.
- `RepoBrowserService::tree(repo, ref, path)` → `host.list_tree(...)` with last-commit enrichment (separate call to commit-touched-this-path API).
- `RepoBrowserService::blob(repo, ref, path, render)` → `host.get_blob(...)` + binary detection (look for NUL byte in first 8KB) + base64 fallback. If `render=html` and path ends in `.md`, render through `markdown.rs` and attach `rendered_markdown`.
- `RepoBrowserService::readme(repo, ref)` → `host.get_readme(...)` then render.
- REST handlers per spec §7.3.

#### W-B-10 · Compare / diff / blame · B · W-B-09 · `src/repo_browser/{compare,diff,blame}.rs`, REST in `repo_browser.rs` · L
**Steps:** `compare(repo, base, head)` returns list of changed files + summary (lines added/removed) + per-file diffs. Use `host.compare_refs` if available; else compute via two `list_tree` + `get_blob` calls. `blame.rs`: thin wrapper around `host.blame` (default impl returns `NotImplemented`; GitLab supports `/projects/:id/repository/files/<path>/blame`).

#### W-B-11 · Merge service + REST · B · W-B-01, W-H-04, W-H-05 · `src/merge/{mod,service,guards}.rs`, `src/web/rest/merge_requests.rs` · L
**Steps:**
- `MergeService::list(repo, state)` → `host.list_open_prs(...)` + enrich postures.
- `MergeService::get(repo, iid)` → `host.get_pr_state` + diff (cached).
- `MergeService::approve_exact_sha(repo, iid, expected_head_sha, idempotency_key)`: refetch live state; if `live.head_sha != expected_head_sha` return 409. Otherwise call `host.approve_mr(MrApproval { ..., head_sha: expected, .. })`. Write audit. Emit `mr.approved`.
- `MergeService::merge_exact_sha(...)`: same guard. Also verifies merge gates immediately before write.
- `guards.rs`: `verify_head_sha`, `verify_gates`.

#### W-B-12 · Reviews service + REST · B · W-B-11, W-H-04 · `src/merge/{review,reviews,suggestions}.rs`, `src/web/rest/reviews.rs` · L
**Steps:**
- `list_threads`, `create_thread`, `resolve_thread`, `submit_review`, `create_review_comment`. Each writes audit + emits WS event.
- Inline suggestions parsed from ```suggestion code blocks.

#### W-B-13 · Merge Passport / merge gate · B · W-B-11 · `src/merge/merge_gate.rs` · L
**Steps:** Compute the canonical Merge Passport from:
1. Required approvals met (count + CODEOWNERS).
2. Required checks all green.
3. No unresolved conversation threads.
4. Branch protection satisfied (linear history, signed commits, etc.).
5. `head_sha` is the current head (Tip1 Law 4).
6. Optional: agent evidence requirement (if enabled in settings).

Result: `MergePassport { status: Pass | Blocked, blockers: Vec<Blocker>, head_sha }`. Returned in `MergeRequestDetail`. UI's `MergeGatePanel` reads this.

#### W-B-14 · CI endpoints · B · W-B-01, W-H-06 · `src/web/rest/ci.rs` · M
**Steps:** `list_runs`, `get_run`, `list_jobs`, `get_job_logs(cursor)`, `list_checks`, `rerun_run`. Logs stream via WebSocket (`job.log.chunk` events) when subscribed.

#### W-B-15 · Generic action preview/execute · B · W-B-01 · `src/web/rest/actions.rs`, `src/api/actions.rs` (extend) · M
**Steps:** Generic `POST /api/actions/{action_id}/preview` and `/execute` that route to the existing TUI action registry. Returns `ActionPreview` and `ActionResult` (already in `src/api/actions.rs`). UI's `ActionButton` flows through this.

#### W-B-16 · Global search · B · W-B-06 · `src/web/rest/search.rs`, `src/repos/search.rs` · M
**Steps:** `GET /api/search?q=...&kinds=repo,file,commit,mr,issue,user&limit=20`. Repos: fuzzy match on cached owner/name. Files: prefix match within current repo or across. Commits/MRs/Issues: search via cached rows. Returns ranked, grouped results.

#### W-B-17 · Activity feed · B · W-B-04 · `src/web/rest/activity.rs` · M
**Steps:** `GET /api/activity?since=&limit=&scope=` returns recent `WebEvent`s from a rolling buffer (last 500). Used by `LiveActivityDock` for initial render.

### 7.3 Host adapter tier (W-H-*)

#### W-H-01 · GitHost trait expansion · H · W-F-03 · `src/git_host/mod.rs` · L
**Steps:** Add the new trait methods (verbatim FINAL §6.8) with default impl `Err(HostError::NotImplemented)`. Add host model structs (`Page`, `PageResult`, `HostRepository`, `CreateHostRepository`, `HostRepositorySettingsPatch`, `HostRef`, `HostTreeEntry`, `HostBlob`, `HostReviewThread`, `HostReviewComment`, `HostReviewCommentInput`, `HostSubmitReviewInput`, `HostReview`, `HostMergeInput`, `HostMergeResult`).

#### W-H-02 · GitLab adapter: read-only · H · W-H-01 · `src/git_host/gitlab.rs` (+helpers) · L
**Steps:** Implement:
- `list_repositories(owner, page)` → `GET /projects` (viewer scope) or `GET /groups/{group}/projects` (group scope). Paginate via `X-Next-Page`. Use `urlencoding::encode` for group path.
- `get_repository(repo)` → `GET /projects/<urlencode>`.
- `list_refs(repo)` → `GET /projects/<id>/repository/branches` + `/tags`.
- `list_tree(repo, ref, path)` → `GET /projects/<id>/repository/tree?path=&ref=&per_page=100`.
- `get_blob(repo, ref, path)` → `GET /projects/<id>/repository/files/<urlencode(path)>?ref=`; base64 decode.
- `get_readme(repo, ref?)` → try `README.md`, `README`, `Readme.md`, `readme.md` in order via `get_blob`.

Centralize path encoding: `encode_project_path(repo: &RepoRef) -> String { urlencoding::encode(&format!("{}/{}", repo.owner, repo.name)).to_string() }`.

#### W-H-03 · GitLab adapter: write · H · W-H-02 · `src/git_host/gitlab.rs` · M
**Steps:**
- `create_repository(input)` → `POST /projects` (user) or `POST /groups/{id}/projects` (group). Body fields: `name`, `description`, `visibility`, `default_branch`, `initialize_with_readme`.
- `update_repository_settings(repo, patch)` → `PUT /projects/<id>` with only the fields present in patch.

#### W-H-04 · GitLab adapter: MR & reviews · H · W-H-02 · `src/git_host/gitlab.rs` · L
**Steps:**
- `list_open_prs(repo)` already exists; extend with state filter.
- `get_pr_state(repo, iid)` already exists.
- `fetch_pr_diff(repo, iid)` already exists.
- `list_review_threads(repo, iid)` → `GET /projects/<id>/merge_requests/<iid>/discussions`. Each discussion has notes; resolved state from discussion.resolved.
- `create_review_comment(...)` → `POST /projects/<id>/merge_requests/<iid>/discussions` (positioned discussion for line-anchored).
- `submit_review(...)` → MR `approve` / `unapprove` endpoints; for "comment" / "request changes", post a note.

#### W-H-05 · GitLab adapter: merge with SHA · H · W-H-04 · `src/git_host/gitlab.rs` · M
**Steps:**
- `approve_mr` already exists with `?sha=` binding.
- `merge_mr(input)` → `PUT /projects/<id>/merge_requests/<iid>/merge` with body `{ sha, merge_commit_message, squash_commit_message, merge_when_pipeline_succeeds, squash }`. Map JeRyu's "method" (merge|squash|rebase) to GitLab params. If response is 409, return `HostError::Conflict`.

#### W-H-06 · GitLab adapter: CI · H · W-H-02 · `src/git_host/gitlab.rs` · M
**Steps:**
- Pipelines: `GET /projects/<id>/pipelines?ref=&status=&per_page=`.
- Jobs: `GET /projects/<id>/pipelines/<pid>/jobs`.
- Job logs (trace): `GET /projects/<id>/jobs/<jid>/trace` (plain text body).
- Status checks: `GET /projects/<id>/repository/commits/<sha>/statuses`.

#### W-H-07 · GitHub adapter: parity with GitLab v1 ops · H · W-H-01 · `src/git_host/github.rs` · L (optional v1.0)
**Steps:** Mirror W-H-02..06 endpoints with GitHub equivalents per FINAL §6.9. Mark v1.5 if internal GitLab is the only target initially.

### 7.4 Frontend tier (W-FE-*)

#### W-FE-01 · App shell layout · FE · W-FE-02..06 · `apps/web/src/layout/{AppShell,GlobalHeader,LeftNav,LiveActivityDock,StatusBar,RepoSwitcher}.tsx` · L
**Steps:** Implement the shell described in FINAL §4.1 (verbatim ASCII layout). Three-column flex: left nav (collapsible), main, live activity dock (collapsible). Header: brand, command palette button, repo switcher, live indicator, user menu. Status bar: connection state, last seq number, latency.

#### W-FE-02 · main.tsx / Router / Providers · FE · W-F-07 · `apps/web/src/main.tsx`, `apps/web/src/app/{App,router,providers}.tsx` · M
**Steps:** Verbatim from FINAL §6.13 with provider stack: `<QueryClientProvider>`, `<ThemeProvider>`, `<RealtimeProvider>` (auto-connects WS on mount), `<CommandPaletteProvider>`, `<TooltipProvider>` (radix). Router uses `createBrowserRouter`.

#### W-FE-03 · API client · FE · W-F-04, W-FE-02 · `apps/web/src/api/{client,endpoints,schemas}.ts` · M
**Steps:** `client.ts`: `apiGet<T>`, `apiSend<T>` per spec §6.13. Add `apiPatch`, `apiDelete`. All routes typed against generated `types.ts`. `endpoints.ts`: const URL builders (`repoBlob(repo, ref, path)`, etc.) — single source of truth so URL bugs surface at typecheck time. `schemas.ts`: optional Zod schemas for runtime validation (only on the network boundary; pass-through internally).

#### W-FE-04 · WebSocket client + store · FE · W-FE-02 · `apps/web/src/api/websocket.ts`, `apps/web/src/stores/realtimeStore.ts` · M
**Steps:** Implement `useRealtimeStore` per spec §6.13 with subscriptions, reconnect with backoff, gap detection → `bootstrap` refetch via React Query's `queryClient.invalidateQueries({queryKey: ['bootstrap']})`. Surface a `connect()` / `disconnect()` / `subscribe(scope)` / `unsubscribe(scope)` API. Persist `lastSeq` in `sessionStorage` so reconnect across page refresh resumes from where we left.

#### W-FE-05 · Zustand stores · FE · W-FE-02 · `apps/web/src/stores/{selectionStore,commandStore,preferencesStore}.ts` · M
**Steps:** Three Zustand stores from FINAL §6.13. `selectionStore` (currentRepoId, currentRef, currentPath, currentMrId). `commandStore` (isOpen, query, results, execute). `preferencesStore` (theme/density/keyboard/fontSize). Persist preferences to localStorage.

#### W-FE-06 · Data-fetch hooks · FE · W-FE-03 · `apps/web/src/hooks/{useBootstrap,useRepositories,useRepoTree,useBlob,useMarkdown,useMergeRequest,useRepoSettings,useWebsocket}.ts` · L
**Steps:** Thin wrappers over React Query that return typed responses. Invalidate on relevant WS events: `useRepositories` invalidates on `repo.created|repo.updated|repo.archived`; `useMergeRequest` on `mr.*` for the current iid; etc. Pattern: each hook returns `{ data, isLoading, error, refetch }` and consumes the `selectionStore` for current entity.

#### W-FE-07 · Dashboard page · FE · W-FE-01..06 · `apps/web/src/pages/DashboardPage.tsx`, components in `apps/web/src/components/dashboard/` · M
**Steps:** Implements FINAL §4.2's "What needs attention?" layout. Cards: blocked MRs, failing checks, agent activity, recent activity. Uses bootstrap's `tui.attention` and `recent_repositories`. Per-card click → navigate to the entity.

#### W-FE-08 · Repositories list + create dialog · FE · W-FE-01..06 · `apps/web/src/pages/RepositoriesPage.tsx`, `apps/web/src/components/repo/{RepoCard,RepoFamilyGroup,RepoHealthPill,RepoTable,CreateRepoDialog}.tsx` · L
**Steps:**
- `RepoTable` (TanStack Table) with virtualization, filters (host, owner, family, language, visibility, archived), sort (recent activity, name, open MRs, failing checks).
- `RepoCard` for compact view alternative.
- `RepoFamilyGroup` for grouping by family (e.g., `veox-*`).
- `RepoHealthPill` shows green/yellow/red.
- `CreateRepoDialog`: 2-step (preview → execute). Step 1 calls `POST /api/repos { ..., dry_run: true }`; step 2 calls again with `dry_run: false` and idempotency key. Shows preview side-effects + warnings between.

#### W-FE-09 · Repository overview + README · FE · W-FE-01..06, W-B-08 · `apps/web/src/pages/RepositoryOverviewPage.tsx`, `apps/web/src/components/browser/{ReadmePanel,MarkdownRenderer,Breadcrumbs,BranchSelector}.tsx` · L
**Steps:**
- Top strip: name, visibility, default branch, clone URL, health/CI/agents/cache postures.
- Main: rendered README (`ReadmePanel` calls `useMarkdown(repo, ref)` which hits `GET /api/repos/.../readme`).
- `MarkdownRenderer`: receives sanitized HTML from backend, runs DOMPurify again, sets `dangerouslySetInnerHTML`. Wires anchor clicks to local route navigation when matches `/repos/...`.
- `Breadcrumbs`: host / owner / repo / path.
- `BranchSelector`: shows current ref, opens combobox with branches + tags, fuzzy filter.

#### W-FE-10 · Code browser · FE · W-FE-01..06 · `apps/web/src/pages/{RepositoryCodePage,RepositoryFilePage}.tsx`, `apps/web/src/components/browser/{FileTree,CodeViewer}.tsx` · L
**Steps:**
- `FileTree`: virtualized (TanStack Virtual), expand/collapse directories, fuzzy file finder modal on `t`.
- `CodeViewer`: Monaco editor in read-only mode (fast syntax highlighting + line numbers + minimap). For Markdown files, render in split view (raw + rendered tabs).
- Route: `/repos/:host/:owner/:repo/code` lists tree; `/repos/.../blob/*` shows file.
- Copy permalink, raw, download, blame, history actions per file.

#### W-FE-11 · Merge request cockpit · FE · W-FE-01..06 · `apps/web/src/pages/{RepositoryMergeRequestsPage,MergeRequestPage}.tsx`, `apps/web/src/components/merge/*.tsx` · L
**Steps:** Implements FINAL §4.6. Three-pane layout: file tree with risk badges → diff viewer (virtualized, unified/split toggle) → review sidebar (checks/agents/blockers/actions).
- `DiffFileTree`: hierarchical, filters (owner, risk, viewed).
- `DiffViewer`: virtualized (TanStack Virtual), inline comment threading, "viewed" checkbox, hide whitespace toggle, hide generated files toggle.
- `InlineComment`: rich text with code suggestion support.
- `ChecksPanel`: list of status checks with details_url.
- `MergeGatePanel`: shows Merge Passport status + each blocker with explanation.
- `ReviewSidebar`: approval state, required approvals, "Approve exact SHA" button (sends `expected_head_sha`), "Merge" button (conditional on Passport pass).
- `ThreadList`: unresolved threads, jump-to-line.

#### W-FE-12 · Settings pages · FE · W-FE-01..06 · `apps/web/src/pages/RepositorySettingsPage.tsx`, `apps/web/src/components/settings/*.tsx` · L
**Steps:** Implements FINAL §4.7. Layout: searchable left nav with categories; main area shows current section. Every change triggers `SettingsDiffPreview` showing old→new, affected branches/MRs/jobs, warnings, reversibility, required permission. Save button calls preview → confirm modal → patch.
Sub-components:
- `BranchProtectionEditor`: rule-pattern list with per-rule editor.
- `MergePolicyEditor`: checkboxes + required approvals counter + CODEOWNERS toggle + Merge Passport toggle.
- `AgentPolicyEditor`: autonomous coding toggle, allowed agents, budget, evidence requirements.
- `SecretsMetadataTable`: list secret names, scopes, age, last access; no values shown.

#### W-FE-13 · Action UX primitives · FE · W-FE-02 · `apps/web/src/components/action/{ActionButton,ActionPreviewDialog,RiskBadge}.tsx` · M
**Steps:** `ActionButton` accepts an `actionId` and `params`; on click fires `/api/actions/{id}/preview`; opens `ActionPreviewDialog` showing risk tier, side effects, will-not-do, undo path; on confirm fires `/api/actions/{id}/execute` with idempotency key. `RiskBadge` (low/medium/high/critical with color + icon).

#### W-FE-14 · Command palette · FE · W-FE-01, W-FE-05 · `apps/web/src/layout/CommandPalette.tsx` · M
**Steps:** Uses `cmdk`. Categories: navigation (Go to repo, Go to MR), actions (Create repo, Approve MR, Merge MR), search (Files, commits, settings). Each command shows risk badge if applicable; selecting an action opens `ActionPreviewDialog`.

#### W-FE-15 · NotFound + permission denied + empty/error/loading pages · FE · W-CC-02 · `apps/web/src/pages/NotFoundPage.tsx` and shared state components · S
**Steps:** Wire the five required UX-QA states into a `PageStateProvider` per route; every page uses `<LoadingState />`, `<EmptyState />`, `<ErrorState />`, `<PermissionDeniedState />` from W-CC-02.

#### W-FE-16 · Keyboard shortcuts wiring · FE · W-CC-04, W-FE-14 · across pages · S
**Steps:** Wire shortcuts at the app shell level. Verify with Playwright that each shortcut performs the documented action.

### 7.5 Testing tier (W-T-*)

#### W-T-01 · Rust markdown tests · T · W-B-08 · `tests/web_markdown_tests.rs` · M
**Fixtures:**
- `tests/fixtures/markdown/gfm-tables.md`: table renders with `<table>`, `<thead>`, `<tbody>`.
- `tests/fixtures/markdown/task-list.md`: `- [x] done` renders `<input type="checkbox" checked disabled>`.
- `tests/fixtures/markdown/strikethrough.md`.
- `tests/fixtures/markdown/footnotes.md`.
- `tests/fixtures/markdown/headings.md`: TOC extraction asserts depths & ids.
- `tests/fixtures/markdown/relative-links.md`: `[docs](./docs/setup.md)` rewrites to JeRyu route.
- **XSS fixtures (CRITICAL):**
  - `<script>alert(1)</script>` → stripped.
  - `<img src=x onerror=alert(1)>` → `onerror` stripped.
  - `<a href="javascript:alert(1)">` → href stripped or anchor removed.
  - `<iframe src="evil">` → stripped.
  - `<style>body{background:url(evil)}</style>` → stripped.
  - SVG with `<script>` inside → stripped.
- Renderer version asserts `jeryu-markdown.v1`.

#### W-T-02 · Rust service tests · T · W-B-06,07,11 · `tests/{repo_lifecycle,repo_settings,permissions,audit,search}_tests.rs` · L
**Tests:**
- Repo create dry-run does not write to DB or host.
- Repo create with `dry_run=false` writes once even when idempotency-key replayed.
- Settings patch with stale base_settings_hash returns 409.
- Settings patch with current hash applies, writes audit, emits event.
- Permissions: viewer with `repo.read` but not `repo.write` returns 403 on PATCH.
- Audit event written for every mutation (parameterized over endpoints).
- Search: fuzzy on owner, name, file path; returns ranked results.

#### W-T-03 · Rust REST integration tests · T · W-B-02..15 · `tests/{web_api,web_review}_tests.rs` · L
**Tests:**
- `GET /api/bootstrap` returns valid `WebBootstrap`.
- `GET /api/repos` happy path with query params.
- `POST /api/repos { dry_run:true }` returns preview, no DB write.
- `GET /api/repos/.../readme` returns rendered HTML + raw + TOC.
- `POST /api/merge-requests/.../approve` rejects stale SHA with 409.
- `POST /api/merge-requests/.../approve` succeeds and emits event for current SHA.
- `POST /api/merge-requests/.../merge` rejects when Merge Passport blocked.

Use `axum::Router::into_make_service()` with `tower::ServiceExt::oneshot()`; mock `GitLabClient` via a trait object.

#### W-T-04 · Rust WS tests · T · W-B-04, W-F-06 · `tests/web_ws_tests.rs`, `tests/web_api_schema_tests.rs` · M
**Tests:**
- Client Hello with resume_from=0 receives server Hello + recent events.
- Client Hello with stale resume_from (gap) receives snapshot_required.
- Subscribe/Unsubscribe filters events correctly.
- Schema test asserts every `#[ts(export)]` struct has a matching TS file under `apps/web/src/api/`.
- Schema test asserts OpenAPI doc generates without warnings.

#### W-T-05 · Vitest setup + MSW · T · W-F-07 · `apps/web/src/test/{mocks,server}.ts`, `vitest.config.ts` · M
**Steps:** Configure Vitest with jsdom env, axe-core setup, MSW worker for component tests. Provide common mocks (bootstrap, repos, blob, MR).

#### W-T-06 · Frontend unit tests · T · W-T-05, W-FE-* · `apps/web/src/**/*.test.tsx` · L
**Per-feature tests:**
- `MarkdownRenderer.test.tsx`: malicious HTML is sanitized client-side; tables/lists/code render.
- `FileTree.test.tsx`: virtualization renders 10k entries without freezing; click navigates.
- `CommandPalette.test.tsx`: `⌘K` opens; type narrows results; Enter executes; Esc closes.
- `ActionPreviewDialog.test.tsx`: shows preview; confirm sends idempotency key; cancel does not.
- `SettingsDiffPreview.test.tsx`: renders old→new diff, warnings, affected entities.
- `useRealtimeStore.test.ts`: events apply in seq order; duplicate seq ignored; gap reloads.

#### W-T-07 · Storybook setup + stories · T · W-T-05 · `apps/web/.storybook/`, `apps/web/src/components/**/*.stories.tsx` · L
**Stories per FINAL §6.14:**
- `RepoCard`: healthy / warning / critical / archived / private.
- `ReadmePanel`: loading / empty / rendered / malicious HTML sanitized.
- `DiffViewer`: small / huge / binary / generated / with comments.
- `MergeGatePanel`: pass / blocked / stale SHA / approval required / agent evidence.
- `SettingsDiffPreview`: safe / reversible / irreversible / production-impact.
- `RiskBadge`: low / medium / high / critical.
- `CommandPalette`: closed / open / typing / no results / many results.

All stories include the addon-a11y panel which must pass.

#### W-T-08 · Playwright config + fixtures · T · W-F-07, W-T-05 · `apps/web/playwright.config.ts`, `apps/web/e2e/fixtures/` · M
**Steps:**
- `playwright.config.ts`: browsers `chromium`, `firefox`, `webkit`; baseURL `http://127.0.0.1:5173`; webServer block launches `cargo run -p jeryu -- web serve --bind 127.0.0.1:8787 --dev-assets http://127.0.0.1:5173` plus `npm run dev`; reporters `html`, `junit`; trace `on-first-retry`; screenshots `only-on-failure`; video `retain-on-failure`.
- Fixtures:
  - `mockGitLabServer`: in-process WireMock or msw/node that responds to GitLab REST. Pre-seeded with two repos (`neverhuman/jeryu`, `neverhuman/redlineDB`), branches, MR #42, README content, file tree.
  - `authenticatedSession`: a `page.context().addCookies(...)` setup that injects a valid session cookie.
  - `seedDb`: SQLite seed for repos, MRs, audit baseline.

#### W-T-09 · Playwright scenario: bootstrap & dashboard · T · W-T-08, W-FE-07 · `apps/web/e2e/01-bootstrap.spec.ts` · M
**Steps:**
1. `page.goto('/')`
2. Wait for `[data-testid="app-shell"]`.
3. Assert WS status indicator shows "live" within 2 s.
4. Assert dashboard shows attention cards.
5. Screenshot loading, empty, success states.
6. `axe.run(page)` returns zero violations.

#### W-T-10 · Playwright scenario: repos list & create · T · W-T-08, W-FE-08 · `apps/web/e2e/02-repos.spec.ts` · M
**Steps:**
1. Goto `/repos`.
2. Assert ≥1 repo card.
3. Filter by family "veox-*"; assert count matches mock.
4. Sort by "open MRs"; assert order.
5. Click "Create repo"; fill name, visibility=Private, initialize_readme=true.
6. Click "Preview"; assert preview shows initial files including `README.md`.
7. Click "Create"; assert success toast; assert new repo appears in list.
8. Screenshots: empty (filter to zero), loading, success.

#### W-T-11 · Playwright scenario: README rendering · T · W-T-08, W-FE-09 · `apps/web/e2e/03-readme.spec.ts` · M
**Steps:**
1. Goto `/repos/gitlab/neverhuman/jeryu`.
2. Wait for README to render.
3. Assert headings, tables, task lists, fenced code blocks render correctly.
4. Click an in-README link `./docs/setup.md` → URL becomes `/repos/.../blob/main/docs/setup.md`.
5. Inject a malicious README via mock (containing `<script>` and `<img onerror>`); assert NO `<script>` in DOM and NO event handlers.
6. Switch branch via `BranchSelector`; assert URL updates and README refetches.
7. Screenshot: rendered README; ReadmePanel loading state.

#### W-T-12 · Playwright scenario: code browser · T · W-T-08, W-FE-10 · `apps/web/e2e/04-code.spec.ts` · M
**Steps:**
1. Goto `/repos/gitlab/neverhuman/jeryu/code`.
2. Expand `src/`; click `web/mod.rs`.
3. Assert URL becomes `/blob/main/src/web/mod.rs`.
4. Assert Monaco renders with Rust syntax highlighting (visible token classes in DOM).
5. Press `t` → fuzzy file finder opens; type "marka" → assert `repo_browser/markdown.rs` highlighted; Enter navigates.
6. Switch branches; assert tree refetches.

#### W-T-13 · Playwright scenario: MR review · T · W-T-08, W-FE-11 · `apps/web/e2e/05-mr-review.spec.ts` · L
**Steps:**
1. Goto `/repos/gitlab/neverhuman/jeryu/merge-requests/42`.
2. Assert top strip shows title, head SHA, target, Merge Passport status.
3. Click a file in DiffFileTree; assert DiffViewer renders.
4. Toggle "hide whitespace"; assert visual change.
5. Click line gutter to add an inline comment; type; submit; assert comment appears.
6. Mark file as viewed; assert checkbox state persists.

#### W-T-14 · Playwright scenario: exact-SHA approve + stale conflict · T · W-T-08, W-FE-11 · `apps/web/e2e/06-approve-sha.spec.ts` · M
**Steps:**
1. Goto MR #42.
2. Click "Approve exact SHA" → confirm modal shows `expected_head_sha`.
3. Confirm → assert success; UI updates approval count.
4. Force-push simulation via mock: change head SHA on the server.
5. Try to approve again with old SHA in flight → expect 409; UI shows "head changed, refresh and re-review" recovery flow.
6. Click "Refresh"; UI fetches new head; new approve button now references new SHA.

#### W-T-15 · Playwright scenario: settings preview · T · W-T-08, W-FE-12 · `apps/web/e2e/07-settings.spec.ts` · M
**Steps:**
1. Goto `/repos/gitlab/neverhuman/jeryu/settings/merge`.
2. Change `required_approvals` from 1 to 2.
3. Click "Preview changes"; assert blast radius shows "Will block 3 open MRs".
4. Confirm; assert audit event ID surfaces.
5. Reload page; assert new value persists.
6. Verify a stale base hash submission (concurrent edit) yields 409 + safe recovery.

#### W-T-16 · Playwright scenario: WebSocket reconnect · T · W-T-08, W-FE-04 · `apps/web/e2e/08-ws-reconnect.spec.ts` · M
**Steps:**
1. Goto dashboard.
2. Disconnect WS via `page.context().route('/api/ws', r => r.abort())`.
3. Assert status indicator shows "offline" within 5s; banner appears.
4. Restore route; assert "live" returns within 5s.
5. Trigger a server-side event with old cursor; assert client triggers snapshot reload.

#### W-T-17 · Playwright scenario: permission denied · T · W-T-08, W-FE-15 · `apps/web/e2e/09-permissions.spec.ts` · M
**Steps:**
1. Switch session to a viewer with `repo.read` only (no `repo.write`).
2. Goto repo settings page → assert "permission denied" state with explanation; settings form disabled.
3. Goto MR page → assert "Approve" button is hidden (or disabled with tooltip).
4. Screenshot: permission-denied state.

#### W-T-18 · Playwright accessibility scans · T · W-T-08..17 · `apps/web/e2e/10-a11y.spec.ts` · M
**Steps:** For each major page (dashboard, repos, repo-overview, code, MR, settings), call `injectAxe()` and `checkA11y()`. Assert zero serious/critical violations. Save the JSON to `target/jankurai/ux-qa/playwright-axe-<page>.json`.

#### W-T-19 · UX-QA harness upgrade · T · W-T-09..18 · `apps/web/ux-qa-check.mjs`, `apps/web/ux-qa.{md,ts}` · L
**Steps:** Upgrade the existing marker checker to a real proof collector:
1. Verify Vite build artifacts exist (`apps/web/dist/{index.html, assets/}`).
2. Verify TypeScript typecheck output present (or rerun).
3. Verify Vitest pass receipt.
4. Verify Storybook build present.
5. Verify Playwright screenshots exist for required states across all major pages.
6. Verify axe scan JSON exists for each major page and has zero serious/critical.
7. Verify markdown XSS fixture proofs.
8. Verify WS replay test proof.
9. Verify performance: gzip JS bundle ≤350 KB; LCP ≤1.5 s on local; INP ≤200 ms.
10. Output proof receipt to `target/jankurai/ux-qa/web-forge.<timestamp>.json` with per-check pass/fail.

#### W-T-20 · Performance budgets · T · W-T-19 · `apps/web/perf/lighthouse.config.js`, CI job · M
**Steps:** Run Lighthouse CI on the built bundle; assert performance budgets per FINAL §10. Fail CI if any budget regresses by >5%.

### 7.6 Documentation tier (W-D-*)

#### W-D-01 · Architecture doc · D · most backend packages · `docs/web-forge.md` · M
**Sections:** Overview, target tree, data flow, BFF architecture, host adapters, event bus, markdown renderer, security model, deployment, troubleshooting.

#### W-D-02 · REST API reference · D · W-B-* · `docs/WEB_API.md` · M
**Steps:** Generate from OpenAPI doc + hand-written examples. Each endpoint: method, path, query/body, response, perms required, idempotency, audit, sample curl.

#### W-D-03 · WebSocket protocol · D · W-F-06, W-B-04 · `docs/WEBSOCKET_PROTOCOL.md` · M
**Steps:** Hello/Event/Snapshot-required/Subscribe semantics; gap recovery; per-scope event kinds; backpressure rules; reconnect strategy.

#### W-D-04 · Markdown rendering & security · D · W-B-08, W-T-01 · `docs/README_RENDERING.md` · M
**Steps:** Why double-sanitize; allow/block lists; relative link rewriting; image auth; renderer version; cache key. Include the XSS test matrix.

#### W-D-05 · Merge cockpit · D · W-FE-11, W-B-13 · `docs/REVIEW_COCKPIT.md` · M
**Steps:** Merge Passport rules; exact-SHA semantics; "Why blocked?" surface; agent evidence integration.

#### W-D-06 · Frontend guide · D · most W-FE-* · `apps/web/README.md` · M
**Steps:** Project structure, design tokens, theme system, API client, WS store, keyboard shortcuts, Storybook, test commands, accessibility rules, markdown renderer fixtures.

#### W-D-07 · Root README update · D · W-D-01..06 · `README.md` · S
**Steps:** Add "Web Forge" section pointing to `docs/web-forge.md` and showing dev/prod commands (`npm install && npm run build && cargo run -p jeryu -- web serve --open`).

---

## 8. PHASE PLAN (with exit criteria)

| Phase | Work packages | Exit criteria | Critical-path size |
|---|---|---|---|
| **Phase 0** — Foundations | W-F-00..10, W-CC-08, W-H-01 | `cargo check --workspace --features web` green; `npm run build` green; `jeryu web serve` stub responds 200 on `/api/bootstrap` (returning placeholder). | ~12 packages |
| **Phase 1** — Shell + Bootstrap | W-B-01..05, W-FE-01..06, W-T-09, W-CC-01..09 | Browser at `127.0.0.1:5173` shows app shell with real bootstrap data + live WS indicator. | ~17 packages |
| **Phase 2** — Repos & README | W-H-02, W-B-06,08,09, W-FE-08..09, W-T-10..11, W-T-01 | User can list all GitLab repos, create a repo via preview/execute, open any repo and see rendered sanitized README. | ~9 packages |
| **Phase 3** — Code browser | W-B-10, W-FE-10, W-T-12 | User can browse refs/tree/blob, open Markdown files, fuzzy-find files via `t`, syntax highlighting active. | ~3 packages |
| **Phase 4** — Merge room | W-H-04..05, W-B-11..13, W-FE-11, W-T-13..14 | User can review files, comment inline, approve with exact SHA (and recover on stale), merge when Passport passes. | ~7 packages |
| **Phase 5** — Settings | W-H-03, W-B-07, W-FE-12, W-T-15 | User can view & change settings; preview shows blast radius; audit + WS event emitted. | ~4 packages |
| **Phase 6** — CI / Agents / Activity | W-H-06, W-B-14..17, W-FE-* polish | Activity dock streams; CI runs visible; agent evidence linkable. | ~5 packages |
| **Phase 7** — Hardening | W-CC-05..09, W-T-16..20, W-D-* | All UX-QA states screenshot-proven; a11y zero serious; performance budgets met; docs complete. | ~12 packages |

Total: ~70 distinct work packages. A single agent doing all sequentially: estimate 35–55 agent-days assuming size totals. With 4–6 parallel agents: ~10–14 calendar days.

---

## 9. ACCEPTANCE CRITERIA (whole-system Definition of Done)

Verbatim from FINAL §14 plus testing additions:

1. `jeryu web serve --open` launches a browser UI.
2. The UI lists all accessible internal-GitLab repos.
3. User can create a repository with preview, permission check, idempotency key, audit event, and WebSocket update.
4. User can open any repo overview and see a correctly rendered sanitized README (XSS fixtures all pass).
5. User can browse branches, trees, and files; Markdown files auto-render.
6. User can open merge requests, review changed files, submit comments inline.
7. User can approve an MR bound to exact head SHA; stale SHA returns 409 and UI shows recovery flow.
8. User can merge only when live gates pass AND head SHA matches; Merge Passport state visible.
9. User can view and change repo settings through the searchable settings page with preview; settings patch with stale hash returns 409.
10. WebSocket updates activity, CI, checks, agents, settings, and merge posture in real time; reconnect recovers via `snapshot_required` / bootstrap refetch.
11. All mutating actions write audit receipts.
12. Frontend has Storybook, unit tests, Playwright E2E covering all 10 user scenarios, accessibility checks (zero serious/critical), and visual proof artifacts.
13. Rust and frontend CI lanes are green.
14. UX-QA proof receipts produced for: loading, empty, error, success, permission-denied states on every major page.
15. Performance budgets met (initial shell ≤350 KB gz; first useful paint ≤1.5s local; route transition ≤100 ms; WS p95 ≤250 ms; markdown cache hit ≤25 ms).
16. Docs (web-forge, WEB_API, WEBSOCKET_PROTOCOL, README_RENDERING, REVIEW_COCKPIT, apps/web/README) all merged.

---

## 10. PLAYWRIGHT TEST SUITE (full detail)

### 10.1 Test pyramid

```
                E2E (Playwright)            ~10 scenarios, ~50 tests
              ┌─────────────────────┐
              │   user-visible      │
              │   workflows         │
              └──────────┬──────────┘
       Component+integration (Vitest+jsdom)  ~80 tests
            ┌────────────────────────────┐
            │  components, hooks, stores │
            └──────────────┬─────────────┘
                  Unit (Vitest)            ~200 tests
        ┌────────────────────────────────────┐
        │  pure helpers, types, validators   │
        └────────────────────────────────────┘
                  Rust unit + integration    ~150 tests
        ┌────────────────────────────────────┐
        │  services, REST, WS, markdown      │
        └────────────────────────────────────┘
```

### 10.2 Playwright project layout

```
apps/web/
├── playwright.config.ts
├── e2e/
│   ├── 01-bootstrap.spec.ts
│   ├── 02-repos.spec.ts
│   ├── 03-readme.spec.ts
│   ├── 04-code.spec.ts
│   ├── 05-mr-review.spec.ts
│   ├── 06-approve-sha.spec.ts
│   ├── 07-settings.spec.ts
│   ├── 08-ws-reconnect.spec.ts
│   ├── 09-permissions.spec.ts
│   ├── 10-a11y.spec.ts
│   ├── fixtures/
│   │   ├── gitlab-server.ts        # mock GitLab
│   │   ├── auth.ts                 # session cookie injection
│   │   ├── seed.ts                 # DB + WS bus seed
│   │   └── data/                   # JSON fixtures (repos, MRs, files)
│   └── pages/                      # Page Object Model
│       ├── AppShellPage.ts
│       ├── DashboardPage.ts
│       ├── RepositoriesPage.ts
│       ├── RepositoryOverviewPage.ts
│       ├── CodeBrowserPage.ts
│       ├── MergeRequestPage.ts
│       └── SettingsPage.ts
└── e2e/screenshots/                # baseline images for visual regression
```

### 10.3 Page Object pattern (example)

```ts
// e2e/pages/MergeRequestPage.ts
import type { Page } from '@playwright/test';

export class MergeRequestPage {
  constructor(private page: Page) {}

  async goto(slug: string, iid: number) {
    await this.page.goto(`/repos/${slug}/merge-requests/${iid}`);
    await this.page.waitForSelector('[data-testid="mr-cockpit"]');
  }

  async approveExactSha(sha: string) {
    await this.page.getByRole('button', { name: /approve exact sha/i }).click();
    await this.page.getByText(sha).waitFor();
    await this.page.getByRole('button', { name: /confirm/i }).click();
  }

  async assertMergePassport(status: 'pass' | 'blocked') {
    await this.page.getByTestId('merge-passport').getByText(status, { exact: false }).waitFor();
  }
}
```

### 10.4 Fixture: mock GitLab server

Use `msw/node` for fine-grained REST mocks at the BFF layer (since the browser never hits GitLab directly, this is BFF-internal). Provide a `setupGitLabMock(server, scenario)` helper invoked from Playwright's `webServer` setup or via a `JERYU_TEST_MOCK_HOST=1` env variable that swaps the `GitLabClient` with a fake in `WebState::from_test_env()`.

Required fixture scenarios:
- `happy-path`: two repos, three MRs, README content, branches `main`/`feature-x`/`feature-y`, MR #42 with diff.
- `force-push`: triggers `head_sha` change after MR list, used by W-T-14.
- `permission-denied`: viewer has `repo.read` only.
- `empty`: zero repos returned.
- `error`: 500 from GitLab.

### 10.5 Visual regression

Each E2E spec snapshots key states with `await expect(page).toHaveScreenshot('name.png', { maxDiffPixelRatio: 0.01 })`. Baseline images live in `e2e/screenshots/<browser>/<spec>/`. CI updates baselines via `--update-snapshots` only in a controlled branch.

### 10.6 Accessibility

`@axe-core/playwright` injects axe at page level. Run `checkA11y(page, undefined, { detailedReport: true, axeOptions: { rules: { 'color-contrast': { enabled: true } } } })`. Save JSON to `target/jankurai/ux-qa/`.

### 10.7 CI integration

In CI:
1. `cargo build --release --features web`.
2. `cd apps/web && npm ci && npm run build`.
3. `npm run typecheck && npm run lint && npm run test`.
4. `npm run build-storybook`.
5. `npm run test:e2e` (sharded across 3 workers).
6. `npm run ux-qa` (verifies all proof receipts).
7. Upload `target/jankurai/ux-qa/` as an artifact.

CI fails if any of these fails.

---

## 11. RUNTIME / DEPLOYMENT

### 11.1 Dev mode

```bash
# Terminal 1: backend in dev-proxy mode
cargo run -p jeryu --features web -- web serve --bind 127.0.0.1:8787 --dev-assets http://127.0.0.1:5173

# Terminal 2: frontend
cd apps/web && npm install && npm run dev

# Browser
open http://127.0.0.1:5173
```

The backend serves `/api/*` and `/api/ws`; Vite serves the SPA and proxies API/WS to `127.0.0.1:8787`.

### 11.2 Prod mode

```bash
cd apps/web && npm ci && npm run build
cargo build --release --features web
./target/release/jeryu web serve --bind 0.0.0.0:8787 --spa-dir apps/web/dist --open
```

The backend serves both API and SPA on `:8787`.

### 11.3 Configuration

Environment variables:
- `JERYU_GITLAB_BASE_URL` — internal GitLab URL (e.g. `https://gitlab.veox.internal`).
- `JERYU_GITLAB_TOKEN` — token for the BFF to authenticate with GitLab.
- `JERYU_WEB_SESSION_SECRET` — symmetric key for session cookies.
- `JERYU_WEB_PUBLIC_URL` — public URL for `WebBootstrap.websocket_url`.
- `JERYU_WEB_CORS_ORIGINS` — comma-separated allowed origins (prod must NOT be `*`).
- `JERYU_WEB_BIND` — default for `--bind` flag.

### 11.4 Internal GitLab connection (user's requirement)

The user explicitly requested: "connect directly to our internal gitlab (jeryu) and show live branches, and code, just like github/gitlab".

Implementation: configure `JERYU_GITLAB_BASE_URL` to point at the internal GitLab. `GitLabClient` already takes a configurable base URL via `GitlabClient::new` (see `src/git_host/gitlab_client.rs`). Add a config loader that reads from env / config file and constructs the client at startup.

Sync strategy:
- On startup, sync repos for the configured groups (configurable via `JERYU_GITLAB_GROUPS`).
- Refresh every 5 min (configurable) in background via tokio interval.
- On user-triggered actions, fresh-fetch the relevant entity (don't trust stale cache for mutations).
- Subscribe to GitLab system hooks if available (optional v1.5); else poll for updates.

---

## 12. RISK REGISTER (verbatim FINAL §15 + additions)

1. **Host API mismatch (high):** Internal GitLab may have a non-standard schema or auth flow. Mitigation: pin GitLab version in docs; add a `gitlab_compatibility_test` against a real instance; treat unknown fields as optional.
2. **Markdown security (critical):** Sanitize twice; test XSS fixtures W-T-01 (the matrix is exhaustive); never run user-controlled JS.
3. **Huge diffs/trees/logs (high):** Virtualize everything; backend streams logs through WS; binary files do not return text body.
4. **Exact-SHA safety (critical):** Always refetch live state before approve/merge; never trust UI-supplied SHA without recheck; never have a code path that approves without `expected_head_sha`.
5. **WebSocket backpressure (medium):** Bounded channel (4096); subscription scopes filter event volume; gap recovery via snapshot_required.
6. **Permissions (critical):** Never trust UI-hidden buttons; backend enforces every mutation; permission tests in W-T-02.
7. **Existing module drift (medium):** W-F-02 hygiene fix happens early and blocks feature work.
8. **Frontend package drift (high):** Pin exact versions; commit `package-lock.json`; CI uses `npm ci`.
9. **Internal GitLab availability (medium):** Backend tolerates GitLab downtime via cached reads; mutations fail clean with `502 Upstream` and idempotency lets the client retry safely.
10. **Token leakage (critical):** GitLab token lives only in BFF process; never serialized into responses; never exposed to the SPA; CSRF on all mutations.
11. **Bundle size (medium):** Monaco editor is heavy; lazy-load via `React.lazy` only on the file view route; keep app shell under 350 KB gz.
12. **Accessibility regressions (medium):** axe-core in CI; Storybook addon-a11y; manual keyboard pass per phase exit.
13. **Test flake on WS reconnect (medium):** Use deterministic backoff in tests; mock the WS endpoint with `page.context().route` for full control.

---

## 13. DEFINITION OF DONE (per work package)

A work package is **done** when ALL of these are true:

- [ ] Branch merged to main.
- [ ] Every file listed in the package matrix is created/modified as described.
- [ ] Local proof lane (`just fast`) green on the branch tip.
- [ ] Web-specific proof: `npm run typecheck && npm run lint && npm run test && npm run build` green where applicable.
- [ ] All tests listed in the package run and pass.
- [ ] If the package added an endpoint: it is documented in `docs/WEB_API.md`.
- [ ] If the package added a WS event kind: it is documented in `docs/WEBSOCKET_PROTOCOL.md`.
- [ ] If the package added a UI component: a Storybook story exists with at least the five required UX-QA states represented (loading, empty, error, success, permission-denied) where relevant.
- [ ] Audit event written for any mutation (verified by audit test in W-T-02).
- [ ] Idempotency key honored for mutations (verified).
- [ ] CLAIMS.md updated to `done`.
- [ ] PR body lists which downstream packages this unblocks.

The whole **system** is done when all 16 acceptance criteria in section 9 are met AND the UX-QA receipt JSON shows zero failed checks AND Lighthouse budgets pass.

---

## 14. CROSS-REFERENCES

### 14.1 To existing JeRyu conventions

- Commit/PR style: per `agent/JANKURAI_STANDARD.md` and recent `git log` patterns.
- Co-author footer required.
- Proof lane: `just fast` (default), `just security` (security-touching PRs), `just check` (release-blocking).
- Generated zones: `target/jankurai/ux-qa/` for proof receipts.
- Boundaries enforced by `agent/boundaries.toml`.

### 14.2 To FINAL spec

- Spec § → this plan §: §3 → §1.1; §4 → §1.2 + §7.4; §5 → §2.1; §6.1 → §3.1 + W-F-01; §6.2 → W-F-03; §6.3 → W-B-01..04; §6.4 → W-F-06 + W-B-04; §6.5 → W-B-06; §6.6 → W-B-08; §6.7 → W-B-11..13; §6.8 → W-H-01; §6.9 → W-H-07; §6.10 → W-H-02..06; §6.11 → W-F-05; §6.12 → W-F-10; §6.13 → W-F-07 + W-FE-02..16; §6.14 → W-T-19; §7 → W-B-* REST; §8 → W-F-06 + W-B-04; §9 → §1.4 + W-CC-05..09; §10 → §9; §11 → §10; §12 → §8; §13 → W-D-*; §14 → §9; §15 → §12.

### 14.3 To Playwright scenarios

Each scenario in W-T-09..18 maps to one user story in FINAL §11.3.

---

## 15. APPENDIX A — REST API at-a-glance

```
GET  /api/bootstrap                                            → WebBootstrap
GET  /api/repos?search=&host=&owner=&family=&include_archived=&limit=&cursor=
                                                                → RepositoryListResponse
POST /api/repos                                                 → CreateRepositoryPreview | RepositorySummary (dry_run)
GET  /api/repos/{host}/{owner}/{repo}                           → RepositoryDetail
GET  /api/repos/{host}/{owner}/{repo}/refs                      → RefSelectorItem[]
GET  /api/repos/{host}/{owner}/{repo}/tree?ref=&path=           → TreeEntry[]
GET  /api/repos/{host}/{owner}/{repo}/blob?ref=&path=&render=   → BlobResponse
GET  /api/repos/{host}/{owner}/{repo}/readme?ref=               → BlobResponse + RenderedMarkdown
GET  /api/repos/{host}/{owner}/{repo}/compare?base=&head=       → CompareView
GET  /api/repos/{host}/{owner}/{repo}/settings                  → RepositorySettings
POST /api/repos/{host}/{owner}/{repo}/settings/preview          → SettingsDiffPreview
PATCH /api/repos/{host}/{owner}/{repo}/settings                 → RepositorySettings
GET  /api/repos/{host}/{owner}/{repo}/merge-requests?state=     → MergeRequestSummary[]
POST /api/repos/{host}/{owner}/{repo}/merge-requests            → MergeRequestSummary
GET  /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}      → MergeRequestDetail
POST /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/approve
POST /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/merge
POST /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/close
GET  /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/reviews
POST /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/reviews
GET  /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/threads
POST /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/threads
PATCH /api/repos/{host}/{owner}/{repo}/merge-requests/{iid}/threads/{thread_id}
GET  /api/repos/{host}/{owner}/{repo}/runs?branch=&status=&limit=
GET  /api/repos/{host}/{owner}/{repo}/runs/{run_id}
GET  /api/repos/{host}/{owner}/{repo}/runs/{run_id}/jobs
GET  /api/repos/{host}/{owner}/{repo}/jobs/{job_id}/logs?cursor=
GET  /api/repos/{host}/{owner}/{repo}/checks?sha=
POST /api/repos/{host}/{owner}/{repo}/runs/{run_id}/rerun
GET  /api/activity?since=&limit=&scope=
GET  /api/search?q=&kinds=&limit=
POST /api/actions/{action_id}/preview
POST /api/actions/{action_id}/execute
GET  /api/ws                                                    (WebSocket upgrade)
```

Every mutation: requires CSRF token + (optionally) `idempotency_key` body field; writes audit; emits WS event.

## 16. APPENDIX B — WebSocket event kinds (verbatim FINAL §8.6)

```
repo.created, repo.updated, repo.deleted, repo.archived, repo.settings.changed
repo.branch.created, repo.branch.deleted, repo.branch.protection.changed
repo.file.changed, repo.readme.rendered
mr.created, mr.updated, mr.review.submitted, mr.thread.created, mr.thread.resolved
mr.approved, mr.merged, mr.merge.blocked
check.started, check.completed
workflow.run.started, workflow.run.completed, job.log.chunk
agent.session.started, agent.patch.proposed, agent.evidence.created
settings.preview.created, action.previewed, action.executed
audit.event.created
```

## 17. APPENDIX C — Permission keys (verbatim FINAL §9.1)

```
repo.read, repo.create, repo.write, repo.admin
settings.read, settings.write
code.read, code.write
mr.read, mr.write, mr.approve, mr.merge
ci.read, ci.write
secrets.read_metadata, secrets.write
agents.read, agents.write
audit.read
```

Mapping: GitLab roles → JeRyu perms via `src/repos/permissions.rs`:
- GitLab `guest` → `repo.read`, `code.read`, `mr.read`, `ci.read`.
- GitLab `reporter` → guest + `secrets.read_metadata`, `audit.read`.
- GitLab `developer` → reporter + `code.write`, `mr.write`, `ci.write`, `agents.read`.
- GitLab `maintainer` → developer + `repo.write`, `settings.write`, `mr.approve`, `mr.merge`, `agents.write`.
- GitLab `owner` → maintainer + `repo.admin`, `secrets.write`, `repo.create`.

## 18. APPENDIX D — Performance budgets (verbatim FINAL §10)

| Area | Target |
|---|---:|
| Initial app shell JS (gzip) | < 350 KB |
| First useful paint (local) | < 1.5 s |
| Route transition (after bootstrap) | < 100 ms perceived |
| Repo list filter/sort (5k cached) | < 50 ms client-side |
| File tree render | 100k entries (virtualized) |
| Diff render | 20k changed lines (virtualized) |
| WebSocket event delivery (local p95) | < 250 ms |
| Markdown render cache hit | < 25 ms |
| Markdown render cache miss (README) | < 150 ms typical |
| Settings preview | < 500 ms excl. host fetch |

## 19. APPENDIX E — Verification checklist (final pre-merge gate)

Before declaring v1.0 done, run end-to-end:

```bash
# Backend
cargo check --workspace --features web
cargo fmt --check
cargo clippy --workspace --features web --all-targets -- -D warnings
cargo nextest run --workspace --features web

# Frontend
cd apps/web
npm ci
npm run typecheck
npm run lint
npm run test
npm run build
npm run build-storybook
npm run test:e2e
npm run ux-qa

# Bundle size check
du -b dist/assets/index-*.js | awk '{print $1}' | xargs -I {} expr {} \\< 358400 \\|\\| echo "JS BUDGET EXCEEDED"

# Lighthouse
npx lhci autorun

# Visual smoke test
cargo run --release -p jeryu --features web -- web serve --open
# manually verify: repo list, README, code browser, MR cockpit, settings page

# Internal GitLab smoke
JERYU_GITLAB_BASE_URL=https://gitlab.veox.internal JERYU_GITLAB_TOKEN=$INT_TOKEN \\
  cargo run --release -p jeryu --features web -- web serve --bind 127.0.0.1:8787
# Visit http://127.0.0.1:5173 and confirm live branches/code visible
```

All must pass. UX-QA receipt JSON must show zero failures. Lighthouse performance ≥90. Then v1.0 is shipped.

---

## 20. OPEN QUESTIONS / DECISIONS REMAINING

These remain explicit so an agent picking up this work knows what to clarify with the user:

1. **GitLab base URL & token:** What is the internal GitLab URL and how is the BFF token provisioned (env var, secret manager, JeRyu existing chain)?
2. **`apps/ux-qa` workspace split:** Should we split the `@jankurai/ux-qa` placeholder into its own `apps/ux-qa/` workspace or fold the proof-marker checker into the new `@jeryu/web` workspace? Recommendation in W-F-09 is to split.
3. **Issues feature:** Issues are in the spec but not in the user's stated v1 requirements. Recommendation: ship Issues at v1.5; leave `src/issues/` stub + `src/api/issues.rs` DTOs in place.
4. **Monaco vs lighter syntax highlighter:** Monaco ships ~3 MB; if bundle budget is critical, swap to `shiki` lazy-load. Decision: start with Monaco lazy-loaded on file view route only; reassess if budget breaks.
5. **GitHub adapter parity (W-H-07):** User said internal GitLab is the source — should GitHub remain v1.5+ work? Default: keep stub trait impl with `NotImplemented`; mark W-H-07 as v1.5.
6. **Mermaid diagrams in Markdown:** Disabled by default per spec. Decision needed: ship behind a feature flag or out of scope?
7. **Comrak vs pulldown-cmark:** Spec lists both. Recommendation: start with pulldown-cmark + ammonia (already in deps); enable comrak feature only if footnote/autolink quality is insufficient.
8. **Real-time on internal GitLab:** Does internal GitLab support system hooks / push notifications? If so, wire those to feed `WebEventBus`; else fall back to poll-and-diff every 5 min.

---

---

## 21. INITIAL DEV ENVIRONMENT SETUP

### 21.1 Prerequisites checklist

```bash
# Rust toolchain (pinned by rust-toolchain.toml at the repo root)
rustup show
cat /home/ubuntu/jeryu/rust-toolchain.toml
cargo --version

# Node + npm
node --version    # >=20
npm --version     # >=10

# Playwright browsers (after npm install)
cd /home/ubuntu/jeryu/apps/web && npx playwright install chromium firefox webkit

# Recommended
cargo install cargo-nextest --locked
cargo install cargo-watch  --locked
```

### 21.2 First-time setup from a clean clone

```bash
git clone <jeryu-repo>
cd jeryu

# Backend
cargo check --workspace --features web   # 5-15 min cold
cargo nextest run -p jeryu               # baseline tests

# Frontend
cd apps/web
npm ci                                    # use the lockfile, not 'npm install'
npm run typecheck && npm run lint && npm run build
cd ..

# DB migrations
cargo run -p jeryu -- db migrate

# Backend dev server (Terminal 1)
JERYU_GITLAB_BASE_URL=https://gitlab.veox.internal \
JERYU_GITLAB_TOKEN=$INT_TOKEN \
JERYU_WEB_SESSION_SECRET=$(openssl rand -hex 32) \
cargo run --features web -- web serve --bind 127.0.0.1:8787 --dev-assets http://127.0.0.1:5173

# Frontend dev server (Terminal 2)
cd apps/web && npm run dev

# Browser: http://127.0.0.1:5173 — expect dashboard with live indicator
```

### 21.3 Common first-run gotchas

| Symptom | Cause | Fix |
|---|---|---|
| `cargo check` fails on missing `tower-http` features | W-F-01 not merged | Apply W-F-01 first |
| `npm run build` fails on missing `@types/*` | corrupt `node_modules` | `rm -rf node_modules package-lock.json && npm install` then commit lockfile |
| `/api/bootstrap` returns 500 | session secret missing | Set `JERYU_WEB_SESSION_SECRET` (32 hex bytes) |
| WebSocket never connects | dev proxy missing `ws: true` | Fix `vite.config.ts` server.proxy `/api` |
| Playwright tests time out | webServer block missing | Configure `playwright.config.ts` per W-T-08 |
| GitLab 401 | token scope insufficient | Needs `read_api`, `api`, `read_repository`, `write_repository` |
| GitLab list empty | viewer has no group access | Set `JERYU_GITLAB_GROUPS` to a group the token can read |

### 21.4 Seeded dev data (no internal GitLab needed)

Set `JERYU_BACKEND_PROFILE=mock` to activate `WebState::from_test_env()`. Seeds:
- 5 fake repos (`acme/web|api|docs|infra|legacy`).
- 3 MRs on `acme/web` (open / draft / approved); MR #42 has a 12-file diff.
- Fake viewer `@dev` with `repo.write` and `mr.approve` perms.
- README exercising every GFM feature used by W-T-01.
- WebSocket bus emits a synthetic CI run every 30 s to verify live updates.

Useful for: UI development without internal-GitLab access; CI; offline demos.

---

## 22. AUTHENTICATION & SESSION FLOW

### 22.1 Lifecycle

```
        Unauth user hits /  ->  302 -> /login?next=/
        Login page (SPA)    ->  POST /api/auth/login {provider:"gitlab", code:"..."}  (CSRF-exempt)
        Backend             ->  OAuth code exchange with internal GitLab
        Backend             ->  Mints session row + __Host-jeryu-session (HttpOnly, Secure, SameSite=Lax)
                                                        + __Host-jeryu-csrf (readable, double-submit)
        Backend             ->  302 to /next
```

### 22.2 Session storage

Opaque random 32-byte ID stored in `sessions`:

```sql
CREATE TABLE sessions (
  id TEXT PRIMARY KEY,           -- 32-byte hex
  actor_id TEXT NOT NULL,        -- GitLab user ID
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,      -- now + 30 days, rolling
  last_seen_at TEXT NOT NULL,
  user_agent TEXT,
  ip TEXT
);
CREATE INDEX idx_sessions_actor ON sessions(actor_id);
```

Why opaque (not JWT): revocable; no claims bloat; renewal is one UPDATE; revocation is one DELETE.

### 22.3 Cookie attributes

```
__Host-jeryu-session=<id>;  Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=2592000
__Host-jeryu-csrf=<token>;  Path=/;          Secure; SameSite=Lax; Max-Age=2592000
```

- `__Host-` prefix locks to current host (no `Domain=` allowed).
- Session HttpOnly so JS cannot exfiltrate.
- CSRF cookie JS-readable: client mirrors it in `X-CSRF-Token` header on mutations (double-submit).
- `SameSite=Lax` allows top-level GET; blocks cross-site POST.

### 22.4 CSRF enforcement

Middleware on POST/PATCH/PUT/DELETE compares header to cookie. Mismatch -> 403. GET/HEAD/OPTIONS bypass.

### 22.5 Logout

```
POST /api/auth/logout      (CSRF required)
-> DELETE FROM sessions WHERE id=?
-> Set-Cookie: __Host-jeryu-session=; Max-Age=0
-> Set-Cookie: __Host-jeryu-csrf=;    Max-Age=0
-> 204
```

### 22.6 WebSocket auth

Cookies sent on the WS Upgrade. Reject if session invalid. Once upgraded, no per-frame auth — the connection inherits the session and is closed on revocation by a background task scanning active connections.

### 22.7 Per-request permission resolution

`AuthLayer` middleware:
1. Extract `__Host-jeryu-session` cookie.
2. Load session row; bail 401 if missing/expired.
3. Load actor record (cached / refreshed from GitLab).
4. Compute normalized perms via `src/repos/permissions.rs` (see §Appendix C mapping).
5. Attach `Viewer { id, login, perms: HashSet<Perm> }` to request extensions.
6. Update `last_seen_at` (deduplicated to once per minute per session).

Route handlers receive `Extension<Viewer>` and check perms explicitly.

---

## 23. OBSERVABILITY

### 23.1 Tracing

`tower-http::trace::TraceLayer` adds a span per request: `request_id`, `method`, `uri`, `status`, `latency_ms`. Handlers add custom fields via `tracing::Span::current().record(...)` (e.g. `actor`, `repo`, `risk_tier`, `idempotency_key`).

### 23.2 Structured logging

JSON in prod, one line per event:

```json
{"ts":"2026-05-26T12:00:00Z","level":"INFO","span":"GET /api/repos","request_id":"abc-123","actor":"jepson","latency_ms":42,"status":200,"target":"jeryu::web::rest::repos"}
```

`tracing-subscriber` + `EnvFilter` (env: `RUST_LOG=info,jeryu=debug`).

### 23.3 Metrics (Prometheus, optional)

Behind `--metrics-bind` flag, off by default. Series:

- `jeryu_http_requests_total{method,route,status}` counter
- `jeryu_http_request_duration_seconds{method,route}` histogram
- `jeryu_ws_connections{state}` gauge
- `jeryu_ws_events_published_total{kind}` counter
- `jeryu_ws_subscriptions{scope}` gauge
- `jeryu_gitlab_calls_total{endpoint,status}` counter
- `jeryu_gitlab_call_duration_seconds{endpoint}` histogram
- `jeryu_audit_events_total{action_id,risk_tier}` counter
- `jeryu_markdown_render_cache{outcome}` counter (hit/miss)

### 23.4 Error reporting

- Local dev: stderr + rotating file `target/jankurai/logs/web-errors.<date>.log`.
- Prod: stderr -> journald; optional Sentry-like sink in v1.5.
- `ApiError::Internal(anyhow::Error)` logs full backtrace before returning generic 500.

### 23.5 Slow query detection

Routes taking >1 s log a WARN with span context. Use to find missing indexes early.

---

## 24. CACHING ARCHITECTURE

Four layers, smallest TTL first.

### 24.1 L1 — Browser HTTP cache

`Cache-Control: private, max-age=30, stale-while-revalidate=120` on idempotent GETs the SPA also queries. Mutations: `Cache-Control: no-store`.

### 24.2 L2 — React Query

Defaults: `staleTime: 30_000`, `gcTime: 5*60_000`. Per-resource:

| Hook | staleTime | Invalidated by WS event |
|---|---|---|
| `useBootstrap` | 60 s | (focus refetch only) |
| `useRepositories` | 30 s | `repo.created`, `repo.updated`, `repo.archived` |
| `useRepoTree` | 5 min (immutable per sha) | `repo.file.changed` |
| `useBlob` | 5 min | `repo.file.changed` |
| `useMarkdown` | 5 min | `repo.readme.rendered` |
| `useMergeRequest` | 10 s | `mr.*` for this iid |
| `useRepoSettings` | 60 s | `repo.settings.changed` |

### 24.3 L3 — In-memory LRU on BFF

`moka` crate. Hot reads cached in-process:
- `gitlab_project_cache` — 5 min TTL, max 1000, key `(host_id, owner, name)`.
- `gitlab_user_cache` — 10 min TTL, max 200.
- `branch_cache` — 60 s TTL, max 5000.

### 24.4 L4 — SQLite-backed cache

- `repositories` table — cache of GitLab project list; `synced_at` controls staleness.
- `rendered_markdown_cache` — renderer cache (W-B-08).
- `repository_settings_cache` — settings reads.

Background `tokio::spawn` task: every 5 min loops rows with `synced_at < now - 5min` and refreshes.

### 24.5 Invalidation

- Client: WS events -> `queryClient.invalidateQueries`.
- Server: `RepoService::create` writes new row directly; `update_repository_settings` re-syncs immediately; LRU invalidated via `cache.invalidate(key)`.

---

## 25. RATE LIMITING & QUOTAS

### 25.1 Per-actor

Default 600 req/min per session (~10/sec sustained). Mutating routes 60 req/min. Token-bucket via `tower-governor`.

### 25.2 Per-IP (pre-auth)

100 req/min on `/api/auth/*`.

### 25.3 Per-route override

| Route pattern | Limit |
|---|---|
| `GET /api/search` | 30 req/min per actor |
| `PATCH /api/repos/.../settings` | 6 req/min per actor |
| `POST /api/merge-requests/.../merge` | 6 req/min per actor |
| `POST /api/repos { dry_run: false }` | 6 req/min per actor |
| `GET /api/.../jobs/.../logs` | 30 req/min per actor (prefer WS for logs) |

### 25.4 Upstream GitLab handling

`HostError::RateLimited { retry_after_ms }` propagates as `429` with `Retry-After`. BFF does not aggressively retry; caller decides.

---

## 26. DEPLOYMENT

### 26.1 Systemd unit (single-server)

`/etc/systemd/system/jeryu-web.service`:

```ini
[Unit]
Description=JeRyu Web Forge
After=network.target

[Service]
Type=simple
User=jeryu
Group=jeryu
WorkingDirectory=/opt/jeryu
EnvironmentFile=/etc/jeryu/web.env
ExecStart=/opt/jeryu/bin/jeryu web serve --bind 127.0.0.1:8787 --spa-dir /opt/jeryu/apps/web/dist
Restart=on-failure
RestartSec=5s
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
ReadWritePaths=/var/lib/jeryu

[Install]
WantedBy=multi-user.target
```

`/etc/jeryu/web.env`:

```
JERYU_GITLAB_BASE_URL=https://gitlab.veox.internal
JERYU_GITLAB_TOKEN=<token>
JERYU_WEB_SESSION_SECRET=<32-hex>
JERYU_WEB_PUBLIC_URL=https://jeryu.veox.internal
JERYU_WEB_CORS_ORIGINS=https://jeryu.veox.internal
JERYU_DB_PATH=/var/lib/jeryu/jeryu.db
RUST_LOG=info,jeryu=debug
```

### 26.2 Nginx reverse proxy

```nginx
upstream jeryu_web { server 127.0.0.1:8787; }

server {
  listen 443 ssl http2;
  server_name jeryu.veox.internal;
  ssl_certificate     /etc/letsencrypt/live/.../fullchain.pem;
  ssl_certificate_key /etc/letsencrypt/live/.../privkey.pem;

  client_max_body_size 50m;

  location /api/ws {
    proxy_pass http://jeryu_web;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_read_timeout 3600s;
    proxy_send_timeout 3600s;
  }

  location / {
    proxy_pass http://jeryu_web;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto https;
  }
}

server { listen 80; server_name jeryu.veox.internal; return 301 https://$host$request_uri; }
```

### 26.3 Dockerfile

```dockerfile
FROM rust:1.83-slim AS rust-build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY src ./src
COPY db ./db
RUN cargo build --release --features web -p jeryu

FROM node:22-slim AS web-build
WORKDIR /build
COPY apps/web/package.json apps/web/package-lock.json ./
RUN npm ci
COPY apps/web ./
RUN npm run build

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
RUN useradd -r -u 1001 jeryu
WORKDIR /opt/jeryu
COPY --from=rust-build /build/target/release/jeryu /opt/jeryu/bin/jeryu
COPY --from=web-build  /build/dist /opt/jeryu/apps/web/dist
ENV JERYU_DB_PATH=/var/lib/jeryu/jeryu.db
USER jeryu
VOLUME /var/lib/jeryu
EXPOSE 8787
CMD ["/opt/jeryu/bin/jeryu","web","serve","--bind","0.0.0.0:8787","--spa-dir","/opt/jeryu/apps/web/dist"]
```

### 26.4 Backup

Hourly: `sqlite3 jeryu.db ".backup /var/backups/jeryu/jeryu.$(date +%Y%m%d%H).db"` via cron. Retain 7 days. Daily snapshot rsync to a backup host.

---

## 27. OPERATOR TROUBLESHOOTING RUNBOOK

| Symptom | First check | Fix |
|---|---|---|
| `/api/bootstrap` returns 500 | tail logs for the `request_id` | Likely DB unreachable. Check `JERYU_DB_PATH` exists/writable. |
| `/api/bootstrap` returns 401 | session cookie in browser? | Re-login. |
| WebSocket disconnects every ~30 s | nginx `proxy_read_timeout` | Raise to 3600 s; `nginx -T \| grep timeout`. |
| README shows escaped HTML | renderer feature flag | Run with `--features web`; check `WebFeatureFlags.markdown_html=true`. |
| Stale data never refreshes | WS event not arriving | `curl /metrics \| grep ws_events_published_total` — if zero, bus wedged. Restart. |
| 403 for known-valid user | perm mapping | `tail -f web.log \| grep permission_denied` shows missing perm; check `src/repos/permissions.rs`. |
| Memory grows over hours | WS subscription leak | `curl /metrics \| grep ws_subscriptions` — should match open connections. |
| GitLab 401 storm | token revoked/expired | Rotate `JERYU_GITLAB_TOKEN`; `systemctl restart jeryu-web`. |
| Audit table growing fast | no retention configured | Set `JERYU_AUDIT_RETENTION_DAYS=90`; nightly cron `DELETE FROM audit_events WHERE created_at < datetime('now','-90 day')`. |
| Slow markdown renders | low cache hit rate | `curl /metrics \| grep markdown_render_cache` — if <50%, raise LRU size or increase TTL. |
| Sessions log out frequently | server clock skew | `timedatectl status`; ensure NTP. |
| HTTPS works but WS fails | proxy missing Upgrade headers | Confirm §26.2 nginx WS block applied. |
| Storybook addon-a11y throws | version mismatch | Pin `@storybook/addon-a11y` to match `@storybook/react-vite`; rerun `npm ci`. |
| Playwright "browsers not installed" | first-run | `npx playwright install` in `apps/web`. |
| Vitest jsdom errors | env missing | `vitest.config.ts` sets `test.environment = 'jsdom'`. |
| GitLab "project not found" | token lacks scope | Token needs `read_repository` (and `write_repository` for mutations). |

---

## 28. CONCRETE CODE STUBS (top-5 hardest pieces)

Each stub is a starter for the matching work-package PR. Agents flesh out edge cases per package acceptance criteria.

### 28.1 Markdown renderer with link rewriting (W-B-08)

```rust
// src/repo_browser/markdown.rs
use ammonia::Builder;
use pulldown_cmark::{html, Event, HeadingLevel, Options, Parser, Tag};

use crate::api::repo_browser::{MarkdownHeading, MarkdownLink, RenderedMarkdown};
use crate::api::repository::RepositoryId;

pub const RENDERER_VERSION: &str = "jeryu-markdown.v1";

pub struct MarkdownContext<'a> {
    pub repo: &'a RepositoryId,
    pub ref_name: &'a str,
    pub current_path: &'a str,
}

pub fn render_markdown(md: &str, ctx: &MarkdownContext<'_>) -> RenderedMarkdown {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);

    let mut headings: Vec<MarkdownHeading> = Vec::new();
    let mut current: Option<(u8, String)> = None;
    let mut links: Vec<MarkdownLink> = Vec::new();

    let parser = Parser::new_ext(md, opts).map(|event| {
        match &event {
            Event::Start(Tag::Heading(level, _, _)) => current = Some((heading_depth(*level), String::new())),
            Event::End(Tag::Heading(_, _, _)) => {
                if let Some((depth, text)) = current.take() {
                    let id = slugify(&text);
                    headings.push(MarkdownHeading { depth, id, text });
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, ref mut text)) = current { text.push_str(t); }
            }
            Event::Start(Tag::Link(_, href, _)) => {
                let raw = href.to_string();
                let resolved = resolve_relative(&raw, ctx);
                let external = is_external(&raw);
                links.push(MarkdownLink { href: raw, resolved_route: resolved, external });
            }
            _ => {}
        }
        event
    });

    let mut raw_html = String::new();
    html::push_html(&mut raw_html, parser);

    let clean = Builder::default()
        .add_tags(["table","thead","tbody","tr","th","td","input"])
        .add_tag_attributes("a", ["href","title","rel","target"])
        .add_tag_attributes("img", ["src","alt","title","width","height"])
        .add_tag_attributes("input", ["type","checked","disabled"])
        .url_relative(ammonia::UrlRelative::PassThrough)
        .clean(&raw_html)
        .to_string();

    let with_anchors = inject_heading_anchors(&clean, &headings);
    let with_routes  = rewrite_relative_links(&with_anchors, ctx);

    RenderedMarkdown {
        html: with_routes,
        toc: headings,
        links,
        renderer_version: RENDERER_VERSION.to_string(),
        rendered_at: chrono::Utc::now(),
    }
}

fn heading_depth(l: HeadingLevel) -> u8 {
    match l { HeadingLevel::H1=>1,HeadingLevel::H2=>2,HeadingLevel::H3=>3,HeadingLevel::H4=>4,HeadingLevel::H5=>5,HeadingLevel::H6=>6 }
}
fn slugify(s: &str) -> String {
    s.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>().split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}
fn is_external(href: &str) -> bool {
    href.starts_with("http://") || href.starts_with("https://") || href.starts_with("//")
}
fn resolve_relative(href: &str, ctx: &MarkdownContext<'_>) -> Option<String> {
    if is_external(href) || href.starts_with('#') { return None; }
    let normalized = normalize_relative(ctx.current_path, href)?;
    Some(format!("/repos/{}/{}/{}/blob/{}/{}", ctx.repo.host, ctx.repo.owner, ctx.repo.name, ctx.ref_name, normalized))
}
fn normalize_relative(base: &str, target: &str) -> Option<String> {
    let target_clean = target.trim_start_matches("./");
    let base_dir: Vec<&str> = base.rsplitn(2, '/').nth(1).map(|d| d.split('/').collect()).unwrap_or_default();
    let mut parts: Vec<String> = base_dir.iter().map(|s| (*s).to_string()).collect();
    for seg in target_clean.split('/') {
        match seg { ".." => { parts.pop(); } "." | "" => {} other => parts.push(other.to_string()) }
    }
    Some(parts.join("/"))
}
fn inject_heading_anchors(html: &str, _headings: &[MarkdownHeading]) -> String { html.to_string() }
fn rewrite_relative_links(html: &str, _ctx: &MarkdownContext<'_>) -> String { html.to_string() }
```

Production version walks the DOM with `html5ever`/`scraper` to rewrite `<a>` and `<img>` correctly. XSS fixtures in W-T-01 are the contract.

### 28.2 WebSocket handler with subscription filter (W-B-04)

```rust
// src/web/ws.rs
use std::collections::HashSet;
use axum::{extract::{State, WebSocketUpgrade}, response::IntoResponse};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast::error::RecvError;

use super::state::WebState;
use crate::web_events::protocol::{ClientWsMessage, ServerWsMessage};

pub async fn ws_handler(State(state): State<WebState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, socket))
}

async fn handle_socket(state: WebState, socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.event_bus.subscribe_all();
    let mut subs: HashSet<String> = HashSet::new();

    let hello = ServerWsMessage::hello(state.event_bus.current_seq());
    if sender.send(Message::Text(hello.unwrap_json().into())).await.is_err() { return; }

    loop {
        tokio::select! {
            msg = receiver.next() => match msg {
                Some(Ok(Message::Text(t))) => {
                    if let Ok(m) = serde_json::from_str::<ClientWsMessage>(&t) {
                        match m {
                            ClientWsMessage::Hello { subscriptions, .. }
                            | ClientWsMessage::Subscribe { subscriptions } => {
                                for s in subscriptions { subs.insert(s.scope); }
                            }
                            ClientWsMessage::Unsubscribe { scopes } => {
                                for s in scopes { subs.remove(&s); }
                            }
                            ClientWsMessage::Ping { nonce } => {
                                let pong = ServerWsMessage::Pong { nonce, server_time: chrono::Utc::now() };
                                let _ = sender.send(Message::Text(pong.unwrap_json().into())).await;
                            }
                            ClientWsMessage::Ack { .. } => {}
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
            evt = rx.recv() => match evt {
                Ok(event) => {
                    if subs.contains("global") || subs.contains(&event.scope) {
                        let frame = ServerWsMessage::event(event).unwrap_json();
                        if sender.send(Message::Text(frame.into())).await.is_err() { break; }
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    let snap = ServerWsMessage::SnapshotRequired { reason: "lag".into(), current_seq: state.event_bus.current_seq() };
                    let _ = sender.send(Message::Text(snap.unwrap_json().into())).await;
                    break;
                }
                Err(RecvError::Closed) => break,
            }
        }
    }
}
```

### 28.3 Exact-SHA approve guard (W-B-11)

```rust
// src/merge/guards.rs
use crate::api::repository::RepositoryId;
use crate::git_host::{GitHost, RepoRef};
use crate::web::error::ApiError;

pub async fn verify_head_sha(
    host: &dyn GitHost,
    repo: &RepositoryId,
    iid: &str,
    expected: &str,
) -> Result<String, ApiError> {
    let host_repo = RepoRef { owner: repo.owner.clone(), name: repo.name.clone() };
    let live = host.get_pr_state(&host_repo, iid).await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;
    if live.head_sha != expected {
        return Err(ApiError::Conflict(format!(
            "head sha changed: expected {} got {}", expected, live.head_sha
        )));
    }
    Ok(live.head_sha)
}
```

```rust
// src/merge/service.rs (excerpt)
pub async fn approve_exact_sha(
    &self,
    repo: RepositoryId,
    iid: String,
    expected_head_sha: String,
    idempotency_key: String,
    actor: &str,
) -> Result<(), ApiError> {
    if let Some(prev) = self.idempotency.find(actor, "mr.approve", &idempotency_key).await {
        return prev.into_result();
    }
    let host = self.host_for(&repo)?;
    let bound = guards::verify_head_sha(host.as_ref(), &repo, &iid, &expected_head_sha).await?;
    let host_repo = RepoRef { owner: repo.owner.clone(), name: repo.name.clone() };
    let receipt = host.approve_mr(MrApproval {
        repo: &host_repo,
        mr_iid: &iid,
        head_sha: &bound,
        agent_id: actor,
        receipt_digest: &compute_receipt(&iid, &bound),
        dry_run: false,
    }).await.map_err(|e| ApiError::Upstream(e.to_string()))?;
    self.audit.write_approve(actor, &repo, &iid, &bound, &receipt).await?;
    self.bus.publish_repo_event(&repo, "mr.approved",
        serde_json::json!({"iid":iid,"head_sha":bound,"receipt":receipt}));
    self.idempotency.store(actor, "mr.approve", &idempotency_key, ()).await;
    Ok(())
}
```

### 28.4 Settings preview blast radius (W-B-07)

```rust
// src/repos/settings.rs (excerpt)
pub async fn preview_patch(
    &self,
    repo: &RepositoryId,
    patch: &RepositorySettingsPatch,
) -> Result<SettingsDiffPreview, ApiError> {
    let current  = self.read(repo).await?;
    let proposed = apply_patch_in_memory(&current, patch);
    let affected = self.compute_affected(repo, &current, &proposed).await?;
    Ok(SettingsDiffPreview {
        old: current.clone(),
        new: proposed.clone(),
        diff: diff_settings(&current, &proposed),
        affected_branches: affected.branches,
        affected_merge_requests: affected.merge_requests,
        affected_jobs: affected.jobs,
        warnings: warnings_for(&proposed, &affected),
        reversible: is_reversible(patch),
        required_permission: required_perm_for(patch),
    })
}

async fn compute_affected(
    &self,
    repo: &RepositoryId,
    old: &RepositorySettings,
    new: &RepositorySettings,
) -> Result<AffectedEntities, ApiError> {
    let mut a = AffectedEntities::default();
    if new.merge.required_approvals > old.merge.required_approvals {
        let open = self.merge_svc.list(repo.clone()).await?;
        for mr in open {
            if mr.review.approvals < new.merge.required_approvals {
                a.merge_requests.push(mr.iid);
            }
        }
    }
    if new.branch_protection.len() > old.branch_protection.len() {
        for rule in &new.branch_protection {
            if !old.branch_protection.iter().any(|o| o.pattern == rule.pattern) {
                a.branches.push(rule.pattern.clone());
            }
        }
    }
    Ok(a)
}
```

### 28.5 ts-rs export bin (W-F-04)

```rust
// src/bin/jeryu_export_types.rs
use jeryu::api::{merge_request, repo_browser, repository, settings, web_read_model};
use ts_rs::TS;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    repository::RepositorySummary::export()?;
    repository::RepositoryDetail::export()?;
    repo_browser::RefSelectorItem::export()?;
    repo_browser::TreeEntry::export()?;
    repo_browser::BlobResponse::export()?;
    repo_browser::RenderedMarkdown::export()?;
    merge_request::MergeRequestSummary::export()?;
    merge_request::MergeRequestDetail::export()?;
    settings::RepositorySettings::export()?;
    web_read_model::WebBootstrap::export()?;
    eprintln!("ts-rs export OK");
    Ok(())
}
```

`tests/web_api_schema_tests.rs` walks the output dir and asserts every Rust DTO has a matching TS file under `apps/web/src/api/`.

---

## 29. GLOSSARY

| Term | Meaning |
|---|---|
| **BFF** | Backend-For-Frontend. `src/web/` exposes only typed JeRyu APIs; the SPA never calls GitLab directly. |
| **Merge Passport** | Single fused verdict combining approvals + checks + threads + branch protection + exact-SHA freshness + (optional) agent evidence. Replaces GitHub's "many bot checks" UX with one Pass/Blocked indicator. |
| **Exact-SHA binding (Tip1 Law 4)** | Approve/merge carries the head SHA the reviewer saw; backend refetches live state and returns 409 on mismatch. Prevents TOCTOU between review and merge. |
| **Blast radius** | Count of branches/MRs/jobs/users affected by a proposed settings change. Surfaced in `SettingsDiffPreview`. |
| **Risk tier** | `low / medium / high / critical`. Drives confirmation strictness, audit detail, grant requirements. |
| **Audit receipt** | Server-generated `audit_event_id` returned by every mutation; indexed in `audit_events`; visible in audit log view. |
| **Idempotency key** | Client-generated UUID per intent. Repeated identical mutations return the stored response without re-executing. |
| **Renderer version** | `jeryu-markdown.v1` string baked into the rendered-Markdown cache key; bumping forces global re-render. |
| **`jankurai`** | Agent-native standard governing this repo: proof lanes, generated zones, agent boundaries, install protocol. |
| **Proof lane** | CI subset: `just fast` (default health), `just security`, `just check` (release). Each PR runs the appropriate lane. |
| **UX-QA** | Quality gate requiring Playwright screenshots, ARIA snapshots, axe scans, and proof of five required states (loading / empty / error / success / permission-denied) per page. |
| **Action preview/execute** | Two-step UX: preview (no side effects, shows risk + impact + reversibility) -> execute (with idempotency key). |
| **Live activity dock** | Right-rail panel subscribed to global WS events; shows them in real time. |
| **Subscription scope** | WS channel string: `global`, `repo:<host>/<owner>/<name>`, `mr:.../<iid>`. Filters server-forwarded events. |
| **Snapshot required** | WS message server sends when it detects an event gap; client refetches `/api/bootstrap` and reconnects from the new cursor. |
| **VTI** | "Validation Test Index" — JeRyu's cached-test-result concept; surfaces in CI settings. |
| **Family** | Repository grouping (e.g. `veox-*`, `jeryu-*`) used for list grouping and bulk actions. |
| **CODEOWNERS** | `.github/CODEOWNERS` or `.gitlab/CODEOWNERS`; codifies which paths require which reviewers. |

---

## 30. AGENT QUICK-START (5-minute onboarding)

1. **Read three files (5 min):** `AGENTS.md`, `agent/JANKURAI_STANDARD.md`, this file §0..§4.
2. **Pick scope (5 min):** scan §7; pick an unclaimed package in `tips/web/CLAIMS.md` matching your skills (Rust -> `W-B/H/F-*`; React/TS -> `W-FE-*`/`W-T-*`; docs -> `W-D-*`). Re-read package end-to-end plus FINAL-spec cross-reference (§14.2).
3. **Claim:** append a row to `tips/web/CLAIMS.md`; `git checkout -b web/W-X-NN-<slug>`; push the empty branch.
4. **Develop:** implement files per matrix; run the proof commands in the package's "Tests" section; `just fast` locally before push.
5. **Submit:** PR title `<type>(<scope>): <imperative>` (<70 chars); body lists closes/unblocks + test plan + risk; include co-author footer `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.
6. **After merge:** update CLAIMS.md to `done`; notify dependents.

If stuck, the FINAL spec at `tips/web/JERYU_FULL_WEB_FORGE_ENGINEERING_SPEC_FINAL.md` is the canonical implementation reference. This document is the execution plan; the FINAL spec is the design.

---

## 31. PREVIOUSLY-UNDEFINED WORK PACKAGES (newly fleshed out)

#### W-B-30 · Issues service + REST (v1.5 placeholder) · B · W-B-01 · `src/issues/{mod,service,labels,milestones,projects}.rs`, `src/web/rest/issues.rs` · M
**Steps:** Create the module tree with stub services returning `HostError::NotImplemented`. Register routes in router returning 501 "Issues not yet implemented in v1; see ROADMAP." DTOs live in `src/api/issues.rs` (W-F-03). Carves out the surface so the frontend compiles against typed shapes.
**Acceptance:** Routes compile and return 501; type contracts exist; no broken `apps/web/src/api/types.ts` references.
**Tests:** Smoke route test asserts 501; type-export round-trip.

#### W-B-31 · Agents service + REST (read-only v1) · B · W-B-01 · `src/web/rest/agents.rs` · M
**Steps:** Stub endpoints `/api/repos/.../agents/sessions` and `.../agents/evidence`. Wire to existing `src/agent_review/` and `src/autonomy/` (no new mutations). DTOs reuse `src/api/agent_session.rs`.
**Acceptance:** Routes return existing agent session data; no writes; "Agents" tab on repo overview renders read-only.
**Tests:** `tests/web_api_tests.rs::agents::*` asserts read returns existing rows; write attempts return 405.

#### W-FE-17 · Per-page state skeletons · FE · W-CC-02, all W-FE-pages · `apps/web/src/pages/*Page.tsx` enhancements · M
**Steps:** Every page renders the right UX-QA state from hook result: `isLoading` -> `<LoadingState />`, `data === undefined` -> `<EmptyState />`, `error` -> `<ErrorState />`, `error.status === 403` -> `<PermissionDeniedState />`, else success. Each state reachable from at least one mock fixture so Playwright can screenshot it.
**Acceptance:** Five states screenshot-proven for Dashboard, Repositories, Repository Overview, Code Browser, MR Cockpit, Settings.

#### W-FE-18 · Notifications inbox (lite) · FE · W-FE-01, W-FE-04 · `apps/web/src/components/NotificationInbox.tsx`, `apps/web/src/pages/NotificationsPage.tsx` · M
**Steps:** Bell with unread badge in header. Click opens inbox listing last 50 viewer-relevant events from `realtimeStore`. v1 is read-only; granular notification rules ship in v1.5.
**Acceptance:** Unread count updates live; click on event navigates; "Mark all as read" clears badge.

#### W-FE-19 · User menu + preferences page · FE · W-CC-01, W-FE-01 · `apps/web/src/components/UserMenu.tsx`, `apps/web/src/pages/AdminSettingsPage.tsx` · M
**Steps:** Avatar dropdown: Profile, Preferences, Theme toggle, Shortcuts (`?`), Logout. Preferences page exposes `preferencesStore`. Logout calls `POST /api/auth/logout`.
**Acceptance:** Preferences persist across reload; theme switches <50 ms; logout clears cookie and redirects to `/login`.

#### W-FE-20 · Search results page · FE · W-CC-04, W-B-16 · `apps/web/src/pages/SearchResultsPage.tsx` · M
**Steps:** Triggered by `/` or palette -> input -> results grouped by kind. Each: icon, primary text, context, navigate on click. Command palette shares the backend hook.
**Acceptance:** `/` opens search; results within 100 ms (debounce 50 ms); Enter navigates.

#### W-D-08 · ROADMAP & versioning doc · D · — · `ROADMAP.md` · S
**Steps:** Document v1.0/v1.1/v1.5/v2 milestones (§32). Linked from root README and `docs/web-forge.md`.
**Acceptance:** Doc exists; each milestone has date estimate + scope bullets.

---

## 32. RELEASE MILESTONES

| Milestone | Scope | Estimate (4 parallel agents) |
|---|---|---|
| **v0.1 Alpha** | Phase 0–1: shell + bootstrap + WS + basic dashboard. | Week 1 |
| **v0.3 Repos+README** | Phase 2: list, create, render README, XSS proof. | Week 2 |
| **v0.5 Beta** | Phase 3+4: code browsing + MR review + exact-SHA approve/merge. | Week 3–4 |
| **v0.8 RC** | Phase 5+6: settings + CI/activity streaming. | Week 5 |
| **v1.0** | Phase 7 hardening; all 16 acceptance criteria; docs complete. | Week 6 |
| **v1.1** | Polish: more shortcuts, advanced filters, density modes. | Week 7–8 |
| **v1.5** | Issues; Agents UI surface; GitHub adapter parity; Mermaid behind flag. | Month 2–3 |
| **v2.0** | Plugins, custom dashboards, multi-tenant, mobile responsive. | Quarter 2 |

---

## 33. SECURITY THREAT MODEL

Per OWASP top-10 with project-specific additions.

| Threat | Mitigation | Tested by |
|---|---|---|
| SQL injection | `sqlx` parameterized queries; no string concat | sqlx compile-time check |
| XSS via Markdown | Server `ammonia` + client `DOMPurify`; CSP `script-src 'self'`; renderer version baked into cache key | W-T-01 fixtures |
| CSRF | `__Host-` cookie + double-submit token on mutations; `SameSite=Lax` | W-T-03 negative test |
| Broken auth | HttpOnly opaque session; 30-day rolling; 32-byte secret | W-T-03 |
| Broken access control | Server enforces normalized perms on every mutation; UI hiding is convenience | W-T-02 permissions |
| Security misconfiguration | Prod refuses `CORS *`; secrets env-only; Docker non-root; HSTS | W-T-20 + manual audit |
| Sensitive data exposure | Secrets metadata only — values never returned after write; logs scrub tokens via regex | W-T-02 + scrub regex test |
| Vulnerable deps | `cargo audit`, `npm audit`, `audit-ci` in CI; lockfiles committed | nightly job |
| Insufficient logging/audit | Every mutation writes audit row; structured JSON logs | W-T-02 audit test |
| SSRF | BFF only fetches `JERYU_GITLAB_BASE_URL`; URL allowlist | code review |
| Mass assignment | Settings patch strongly typed; unknown fields rejected | W-T-02 |
| TOCTOU on merge | Exact-SHA refetch before approve/merge | W-T-14 |
| WS abuse | Bounded broadcast (4096); per-connection sub cap (50); ping/pong 30 s | W-T-04 |
| Token leakage | Token in env only; CSP forbids inline scripts; `dompurify` strips style/script | W-T-01 |
| Markdown DoS | Renderer hard-limit `len > 1 MiB` -> 413; tokio render timeout | W-T-01 |
| Path traversal in blob | Reject `..`; URL-encode every host call | W-T-03 |
| Open redirect on login `?next=` | Allow only same-origin relative paths | W-T-03 |
| Dependency confusion | Pin exact versions; commit lockfiles; CI uses `npm ci` | W-F-01 + W-F-07 |
| Unsigned commits on protected branches | Branch protection rule enforces signed commits when enabled | W-T-15 |

Recommended CSP baseline:

```
Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' wss:; font-src 'self' data:; frame-ancestors 'none'; base-uri 'self'; form-action 'self'
```

`'unsafe-inline'` on `style-src` is regrettable but required for some component-library inline styles. Tighten to nonce-based in v1.5.

---

## 34. CHANGELOG / VERSION HISTORY

- **v1.0 — 2026-05-26.** Initial comprehensive plan. Synthesized from `tips/web/JERYU_FULL_WEB_FORGE_ENGINEERING_SPEC_FINAL.md` plus ecosystem analysis. Covers ~78 work packages across 8 phases. Author: Claude Opus 4.7 (1M context) on behalf of `jepson@veox.ai`.

Subsequent revisions append rows here with date, summary, and authoring agent.

---

---

## 35. CODEX PLAN REVIEW — SYNTHESIS & CORRECTIONS

A parallel plan by Codex was authored at `/home/ubuntu/jeryu/WEB_WORK_CODEX.md` (~43 KB). This section reconciles the two. **Section 35 is authoritative where it conflicts with earlier sections** — apply these revisions in implementation.

### 35.1 Critical adoptions from Codex (MUST apply)

These are correctness/safety improvements that supersede earlier sections.

#### 35.1.1 API versioning prefix — adopt `/api/v1/`

All BFF routes mount under `/api/v1/...`, **not** `/api/...`. This reserves `/api/v2/` for future breaking changes without URL conflicts. Existing engine routes (`/health`, `/hooks`, `/cache/summary`) stay exactly where they are — they are NOT migrated under `/api/v1/`.

**Impact:** §2.4 router structure, §4 router.rs paths, §15 cheat sheet — every URL written as `/api/X` should be read as `/api/v1/X`. The canonical list is §35.7 below.

#### 35.1.2 Stable `repo_id` in API paths (not `host/owner/name`)

Internal GitLab supports nested groups (e.g. `group/subgroup/project`), which breaks any URL scheme that uses `/{host}/{owner}/{repo}` as a 3-segment path. Codex correctly uses an opaque stable `repo_id` (UUID-shaped, persisted in `web_repositories.id`) for backend routes.

**Adopted scheme:**
- Backend routes: `/api/v1/repos/{repo_id}/...`
- Frontend routes: human-readable like `/repos/:provider/:fullName/*` and resolve to `repo_id` via the repo list/bootstrap cache.
- The `RepositorySummary` DTO carries both `id: RepositoryId` and `full_name: String` so the SPA can show pretty URLs while calling the API with the stable id.

**Impact:** §15, §28, all route handlers in `src/web/rest/`.

#### 35.1.3 `Idempotency-Key` HTTP header (not body field)

Codex correctly uses the standard `Idempotency-Key` HTTP header rather than a body field. Adopt for create / merge / delete / archive / settings.patch / secrets mutations.

```
POST /api/v1/repos/{repo_id}/merge-requests/{iid}/merge
Idempotency-Key: 0a9e8b2f-...
Content-Type: application/json

{ "expected_head_sha": "abc123", "method": "squash", ... }
```

The body no longer carries `idempotency_key`. W-CC-06 middleware reads the header.

#### 35.1.4 Markdown cache key includes `sanitizer_version`

Codex correctly separates renderer and sanitizer versions in the cache key. Replace the W-B-08 / §24.4 / §28.1 cache-key tuple `(repo, ref_sha, path, blob_sha, renderer_version)` with `(repo, ref_sha, path, blob_sha, renderer_version, sanitizer_version)`. This lets us bump `ammonia` policy without bumping the parser version.

Constants:
```rust
pub const RENDERER_VERSION:  &str = "jeryu-md-renderer.v1";   // pulldown-cmark options
pub const SANITIZER_VERSION: &str = "jeryu-md-sanitizer.v1";  // ammonia allowlist
```

DB migration `web_markdown_cache` table adds the column accordingly; primary key becomes `(repo_id, commit_sha, path, renderer_version, sanitizer_version)`.

#### 35.1.5 Preserve existing engine routes

The engine binary already serves `/health`, `/hooks`, `/cache/summary` (see Codex §1 baseline). The web BFF **must not** rebind or remove these. W-B-02 (router) merges the new `/api/v1/*` and `/api/ws` paths into the existing engine router without disturbing the legacy three.

Acceptance addition for W-B-02: `curl http://127.0.0.1:8787/health` and `/hooks` and `/cache/summary` continue to return what they did before the web feature was enabled.

#### 35.1.6 WebSocket per-scope permission check

Authentication on WS upgrade is necessary but not sufficient. Each `Subscribe { subscriptions: [...] }` frame must be re-checked against the viewer's perms; an unauthorized scope is silently dropped from the subscription set and an `Error { code: "subscribe_forbidden", scopes: [...] }` frame is sent back.

Apply to W-B-04 handler. Without this, a low-privilege actor could subscribe to a private repo's `repo:<id>` scope by guessing the id.

#### 35.1.7 README lookup order — broaden

The W-H-02 `get_readme` and W-B-09 `RepoBrowserService::readme` should try (in order):
1. `README.md`
2. `README.markdown`
3. `README.mdown`
4. `README.txt`
5. case-insensitive variants of each (`readme.md`, `Readme.MD`, …)

RST (`README.rst`) is download-only in v1 — render is v1.5.

#### 35.1.8 Generic `POST /api/v1/markdown/render` endpoint

Adopt the standalone Markdown render endpoint so the UI can preview MR/issue/comment bodies before posting without owning a sanitizer:

```
POST /api/v1/markdown/render
Content-Type: application/json
{ "markdown": "...", "context": { "repo_id": "...", "ref": "main" } }
→ 200 { "html": "...", "renderer_version": "...", "sanitizer_version": "..." }
```

Owner: W-B-08 (markdown service exposes a thin REST wrapper). Used by W-FE-11 inline comments and W-FE-12 settings notes.

#### 35.1.9 Explicit `/raw` endpoint distinct from `/blob`

Codex correctly separates raw download (`/raw?ref=&path=`) from JSON blob fetch (`/blob?ref=&path=&render=`). The raw endpoint:
- Returns the bytes with `Content-Type` derived from `mime_guess`.
- Uses `Content-Disposition: attachment; filename=…` for non-text MIME.
- Authorizes via `code.read` and the same path-safety rules.

Add `/api/v1/repos/{repo_id}/raw?ref=&path=` to W-B-09 REST routes.

#### 35.1.10 Path safety — explicit

W-B-09 and W-H-02 must reject any path query containing `..`, leading `/`, NUL bytes, or backslashes. URL-encode every host call segment. Add to W-T-03 negative tests.

#### 35.1.11 Structured error envelope

Adopt Codex's structured error shape; replace simple `ApiErrorBody` in §6.3 / `src/web/error.rs` with:

```json
{
  "error": {
    "code": "merge_sha_stale",
    "message": "The source branch changed after approval.",
    "details": { "expected": "abc123", "live": "def456" },
    "request_id": "req-...-...-..",
    "event_cursor": 12345
  }
}
```

The `event_cursor` lets clients realign their WS state on error.

Canonical error codes (lowercase snake_case):
- `unauthenticated`, `forbidden`, `csrf_invalid`
- `not_found`, `bad_request`, `validation_failed`
- `conflict`, `merge_sha_stale`, `settings_hash_stale`
- `idempotency_replay`, `idempotency_conflict`
- `rate_limited`, `upstream_unavailable`, `upstream_forbidden`
- `subscribe_forbidden`, `event_gap`
- `internal`

#### 35.1.12 Heartbeat cadence — 15 s

W-B-04 / W-FE-04: WebSocket heartbeat ping every 15 s (server) with 30 s read timeout. Codex's 15 s is tighter than my earlier "30 s ping" — adopt 15 s for faster offline detection.

#### 35.1.13 Backpressure priority classes

W-B-04 broadcast: when the bounded channel is full, drop low-priority events first; **never** drop:
- Action results (caller is waiting).
- Audit/security events.
- Direct mutation receipts (`mr.approved`, `mr.merged`, `settings.changed`).

Implement by separating the bus into two channels (priority + best-effort) with different capacities, or by tagging events `priority: high | medium | low` and dropping `low` first when at capacity.

#### 35.1.14 Action safety algorithm (canonical 14-step)

W-CC-05 / W-CC-06 / every mutation handler MUST follow this order (verbatim, expanded from Codex §3):

```
1.  authenticate
2.  resolve viewer
3.  resolve target
4.  check normalized permission
5.  validate CSRF (cookie auth) OR bearer (token auth)
6.  validate schema (Zod / serde)
7.  load current state
8.  validate expected_state_hash (for settings) OR expected_sha (for merge/approve)
9.  produce preview for medium/high-risk actions (if not already executed via /preview)
10. require Idempotency-Key header for create / merge / delete / archive / settings / secrets
11. execute provider call OR local state change
12. write audit receipt (with expected_state_hash, resulting_state_hash, provider_calls, risk_tier)
13. write durable web event row (seq, scope, kind, payload)
14. broadcast WebSocket event; return updated read model or action receipt
```

Steps 12 and 13 are atomic per-mutation (same transaction).

#### 35.1.15 WS topics — granular scopes

Codex's scope vocabulary is more granular than mine. Adopt:

```
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

The frontend subscribes to the minimum scopes for its current route. The dashboard subscribes to `global.activity` + `system.health` + the viewer-specific notification scope. A repo overview adds `repo.{id}` + `repo.{id}.activity`. The merge cockpit adds `mr.{id}`. This dramatically lowers event volume per connection vs subscribing to the whole world.

#### 35.1.16 `web_action_receipts` table

Adopt Codex's dedicated action-receipt table (separate from `audit_events`):

```sql
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
```

The unique constraint enforces idempotency at the DB layer. The `audit_events` table records the *human-facing* audit log; `web_action_receipts` records the *machine-facing* execution receipt with hashes for forensic replay.

Apply in W-F-05 migration alongside the existing schema.

#### 35.1.17 `provider_etag` on `web_repositories`

Adopt the `provider_etag TEXT` column so the host-sync background task can do conditional `If-None-Match` GETs and skip unchanged projects. Lowers GitLab API load.

#### 35.1.18 Permission key expansion

Adopt Codex's larger normalized permission set:

```
repo.read, repo.create, repo.write, repo.admin, repo.delete
code.read, code.write
branch.create, branch.delete
settings.read, settings.write
mr.read, mr.write, mr.comment, mr.review, mr.approve, mr.merge
issue.read, issue.write
ci.read, ci.write
secrets.read_metadata, secrets.write
agents.read, agents.write, agents.grant
audit.read, admin.audit
```

(Mine had 18; this has 24.) The `branch.create/delete` and `mr.comment/review` splits are valuable for fine-grained delegation. Update §Appendix C accordingly.

#### 35.1.19 Specific path-safety + binary-file rules

Codex spells out:
- "Normalize requested paths. Reject `..`, absolute paths, NUL bytes, and symlink escapes."
- "Binary blobs are rejected by renderer."
- "SVG must be sanitized or download-only by default."
- "Large files do not lock the browser."

Add explicit assertions to W-T-01 and W-T-12.

### 35.2 Adopted UX improvements

#### 35.2.1 Frontend route map

Adopt Codex's route map (richer than mine):

```
/
/repos
/repos/new
/repos/:provider/*fullName
/repos/:provider/*fullName/code
/repos/:provider/*fullName/blob/*
/repos/:provider/*fullName/merge-requests
/repos/:provider/*fullName/merge-requests/:iid
/repos/:provider/*fullName/issues
/repos/:provider/*fullName/settings/:section?
/merge-room
/notifications
/audit
/settings
```

Note: `*fullName` is a splat route to support GitLab nested-group paths. The frontend resolves it to `repo_id` via React Query cache.

#### 35.2.2 Geometry / layout checks in Playwright

Codex requires explicit geometry checks:
- text overflow
- overlapping controls
- target sizes (44×44 px hit targets for touch)
- sidebars and activity rail dimensions
- mobile layouts

Add to W-T-18 (a11y) as a parallel layer using `boundingBox()` assertions. Failures produce screenshots tagged `geom-*.png`.

#### 35.2.3 Command registry shape

Codex's command-registry entry shape (more complete than mine):

```ts
type Command = {
  id: string;
  title: string;
  keywords: string[];
  icon: string;                 // lucide-react icon name
  permission?: Permission;
  routeOrAction: { kind: 'route'; path: string } | { kind: 'action'; actionId: string };
  contextPredicate?: (ctx: AppContext) => boolean;
  shortcut?: string;
  riskTier?: 'low' | 'medium' | 'high' | 'critical';
};
```

Apply in W-FE-14.

#### 35.2.4 Merge Passport check list — definitive

Codex specifies the exact gates (use this list in W-B-13):

1. Source SHA unchanged since preview/approval.
2. Target branch SHA checked.
3. Target policy SHA checked where available (existing `fetch_target_policy_sha`).
4. Required approvals.
5. Code owners.
6. All threads resolved.
7. Required CI green.
8. VTI/test plan acceptable.
9. Agent evidence fresh and signed.
10. Branch protection.
11. Conflict status (no rebase needed).
12. Release window / deploy freeze when relevant.

### 35.3 Adopted operational items

#### 35.3.1 `jankurai` proof tools in CI

Codex hooks into the project's existing jankurai tooling. Add to the verification checklist (§19):

```bash
jankurai doctor --fail-on critical
jankurai ux audit --config agent/ux-qa.toml --out target/jankurai/ux-qa.json
```

#### 35.3.2 Schema export to `schemas/`

Codex puts OpenAPI/JSON-schema artifacts in `schemas/`. Add to W-F-04 and the target tree:

```
schemas/web-api.openapi.json
schemas/websocket-events.schema.json
```

Source command: `cargo run --bin jeryu_export_schemas` (new bin alongside `jeryu_export_types`). Register under `agent/generated-zones.toml`.

#### 35.3.3 Provider implementation order

Codex's pragmatic order: **GitLab first** (matches the user's stated requirement and the existing adapter is partially complete), **GitHub second** (existing check/comment/approval foundation accelerates), **defer package registry / wiki / discussions / enterprise SSO parity**.

This aligns with my W-H-02..06 plan; explicitly mark W-H-07 (GitHub parity) as **v1.5**.

#### 35.3.4 Explicit non-goals (broader than mine)

Adopt Codex's non-goal list. Updated §1.3 to include:
- Replacing Git wire protocol hosting itself.
- Full package registry UI parity.
- Browser IDE / Codespaces clone.
- Enterprise SSO/OIDC beyond token/session scaffolding.
- Public multi-tenant SaaS hardening.
- Full wiki / discussions parity.
- Full RST rendering (download-only in v1).
- Mermaid rendering unless sandboxed (v1.5 behind a feature flag).

#### 35.3.5 Markdown allow-list — broader element set

Codex's broader Markdown allow-list (more useful for real docs):

Allowed:
`a p pre code blockquote ul ol li table thead tbody tr th td h1 h2 h3 h4 h5 h6 img details summary kbd del strong em hr br`

Strip:
`script` tags, inline event handlers, `style` attributes (unless allowlisted later), `iframe`, `object`, `embed`, untrusted `svg`, `javascript:` URLs, unsafe `data:` URLs, `form`.

Update W-B-08 ammonia builder accordingly.

### 35.4 What I'm keeping from my plan (not in Codex)

These are improvements my plan offers that Codex does not — keep them:

1. **§4 file-ownership matrix** (one row per file with owning W-package) — prevents collisions; Codex has only per-package "Owner paths" lists.
2. **§5 dependency graph + critical path** (ASCII + 19-package critical path identified) — Codex has English "depends on Wx" only.
3. **§6 agent claim & sync protocol** with explicit branch naming, claim tracker `tips/web/CLAIMS.md`, and collision rules — Codex assumes coordination ad-hoc.
4. **§13 per-package Definition of Done checklist** — Codex's acceptance per package is briefer.
5. **§21 first-time dev environment setup walkthrough with gotchas table** — Codex assumes you know how.
6. **§22 authentication & session flow diagram + cookie attrs** — Codex says "session-shaped auth" without specifying.
7. **§24 four-layer caching architecture** — Codex mentions ETag but not the layer story.
8. **§25 rate-limiting per-route quotas table** — Codex doesn't.
9. **§26 deployment** (systemd unit, Dockerfile, nginx with WS pass-through) — Codex doesn't.
10. **§27 operator troubleshooting runbook** (17 common issues) — Codex doesn't.
11. **§28 concrete code stubs** for the 5 hardest pieces — Codex relies on FINAL spec for code.
12. **§29 glossary** — Codex doesn't.
13. **§30 5-minute agent onboarding** — Codex doesn't.
14. **§32 release milestones with calendar estimates** — Codex has "recommended PR order" only.
15. **§33 security threat model** OWASP-style table with CSP baseline — Codex has security requirements scattered.

### 35.5 Where we deliberately diverge

| Topic | Mine | Codex | Decision |
|---|---|---|---|
| Markdown parser | `pulldown-cmark` default, `comrak` optional | `comrak` only | **Adopt `comrak` as default** (richer GFM); drop the optional flag. Per FINAL spec §6.1 comrak is already listed. |
| Mock dev profile | `JERYU_BACKEND_PROFILE=mock` seeds 5 repos | not explicit | **Keep mine** — needed for offline dev. |
| RepoId field | `{ host, owner, name }` struct | opaque `repo_id` string | **Adopt opaque** but keep `host/owner/name` accessible as `RepositoryId.parts` for display. |
| WS protocol name | `jeryu.ws.v1` | `jeryu.ws.v1` (implicit) | Same. |
| Web feature flag | `web = []` in Cargo.toml | not gated | **Keep mine** — allows minimal builds without web stack. |
| CSP `unsafe-inline` on style | yes (regrettable, fix in v1.5) | not mentioned | **Keep mine** — explicit decision is better than implicit. |

### 35.6 New / corrected work packages from synthesis

These supersede or augment §7.

#### W-F-11 (NEW) · OpenAPI / JSON-schema export to `schemas/` · F · W-F-03, W-F-04 · `src/bin/jeryu_export_schemas.rs`, `schemas/*.json`, `agent/generated-zones.toml` · M
**Steps:** Bin emits `schemas/web-api.openapi.json` (from utoipa) and `schemas/websocket-events.schema.json` (from schemars). Register source command in `agent/generated-zones.toml`. CI re-runs and fails on drift.
**Acceptance:** Both files exist; CI drift check passes; docs can render them.

#### W-F-12 (NEW) · Workspace split — move `apps/web` → `apps/ux-qa` · F · — · `apps/ux-qa/*`, `apps/web/*`, `package.json` (root) · M
**Steps:** (Replaces W-F-09's "recommended" with a definitive plan.) `git mv apps/web apps/ux-qa`; preserve package name `@jankurai/ux-qa`; copy `apps/web/AGENTS.md` to `apps/ux-qa/AGENTS.md` with light adaptation. Create fresh `apps/web` as `@jeryu/web` per W-F-07. Update root `workspaces: ["apps/web", "apps/ux-qa"]`. Update `agent/test-map.json` so the new `apps/web` routes to Playwright/UX evidence and `apps/ux-qa` routes to marker proof.
**Acceptance:** Both workspaces install; legacy UX-QA build/test still green; new `@jeryu/web` typecheck/build green.

#### W-F-13 (NEW) · Preserve legacy engine routes · F · W-F-10 · `src/web/router.rs`, `tests/engine_routes_preserved_test.rs` · S
**Steps:** Router merges legacy `/health`, `/hooks`, `/cache/summary` with new `/api/v1/*` + `/api/ws`. Add an integration test that asserts all three legacy routes return their pre-feature responses.
**Acceptance:** Test passes; `curl` against a running `jeryu web serve` returns expected legacy responses.

#### W-B-08′ (REVISION of W-B-08) · Markdown — dual versioning + comment endpoint · B · — · same files plus `src/web/rest/markdown.rs` · L
**Additions to W-B-08:**
- Cache key includes both `renderer_version` and `sanitizer_version`.
- Public renderer constants per §35.1.4.
- Implement `POST /api/v1/markdown/render` (§35.1.8) and wire it in router.

#### W-B-11′ (REVISION of W-B-11) · Merge service — exact-SHA + canonical 14-step · B · — · same files · L
**Additions:** Each handler explicitly follows the 14-step action algorithm (§35.1.14). Write `web_action_receipts` row with `expected_state_hash`, `resulting_state_hash`, `provider_calls_json`. Persist `passport_hash` on `web_merge_requests` after Passport recomputation so we can detect re-evaluation drift.

#### W-B-13′ (REVISION of W-B-13) · Merge Passport — 12 explicit gates · B · — · same files · L
**Additions:** Implement the 12-item gate list from §35.2.4. Each blocker carries its own `code` (e.g. `passport_blocked_approvals`, `passport_blocked_threads`, `passport_blocked_policy_sha`) so the UI can show targeted explanations.

#### W-CC-06′ (REVISION of W-CC-06) · Idempotency via header · CC · — · `src/web/idempotency.rs` middleware · M
**Additions:** Middleware reads `Idempotency-Key` header. Unique store in `web_action_receipts(action_kind, target_id, idempotency_key)`. Replays return the stored receipt. Conflict on stored result vs new attempt with same key on different params returns `409 idempotency_conflict`.

#### W-CC-07′ (REVISION of W-CC-07) · Permissions — 24-key set · CC · — · `src/repos/permissions.rs` · M
**Additions:** Expand normalized permissions per §35.1.18. Update GitLab role mapping accordingly:
- `guest` → `repo.read`, `code.read`, `mr.read`, `mr.comment`, `issue.read`, `ci.read`.
- `reporter` → guest + `mr.review`, `secrets.read_metadata`, `audit.read`.
- `developer` → reporter + `code.write`, `branch.create`, `mr.write`, `ci.write`, `agents.read`, `issue.write`.
- `maintainer` → developer + `repo.write`, `branch.delete`, `settings.write`, `mr.approve`, `mr.merge`, `agents.write`.
- `owner` → maintainer + `repo.admin`, `repo.delete`, `repo.create`, `secrets.write`, `agents.grant`, `admin.audit`.

### 35.7 Canonical REST API map (supersedes §15)

```
GET    /health                                            (engine — UNCHANGED)
GET    /hooks                                             (engine — UNCHANGED)
GET    /cache/summary                                     (engine — UNCHANGED)

GET    /api/v1/bootstrap                                  → WebBootstrap
POST   /api/v1/auth/login                                 (CSRF-exempt)
POST   /api/v1/auth/logout

GET    /api/v1/repos?search=&host=&owner=&family=&include_archived=&limit=&cursor=
POST   /api/v1/repos/preview                              → CreateRepositoryPreview
POST   /api/v1/repos                                      (Idempotency-Key)
POST   /api/v1/repos/import/preview
POST   /api/v1/repos/import                               (Idempotency-Key)
GET    /api/v1/repos/{repo_id}
PATCH  /api/v1/repos/{repo_id}
POST   /api/v1/repos/{repo_id}/archive                    (Idempotency-Key)
DELETE /api/v1/repos/{repo_id}                            (Idempotency-Key)

GET    /api/v1/repos/{repo_id}/refs
GET    /api/v1/repos/{repo_id}/branches
POST   /api/v1/repos/{repo_id}/branches                   (Idempotency-Key)
GET    /api/v1/repos/{repo_id}/tags
POST   /api/v1/repos/{repo_id}/tags                       (Idempotency-Key)
GET    /api/v1/repos/{repo_id}/commits
GET    /api/v1/repos/{repo_id}/commits/{sha}
GET    /api/v1/repos/{repo_id}/compare?base=&head=

GET    /api/v1/repos/{repo_id}/tree?ref=&path=
GET    /api/v1/repos/{repo_id}/blob?ref=&path=&render=
GET    /api/v1/repos/{repo_id}/raw?ref=&path=
GET    /api/v1/repos/{repo_id}/readme?ref=
GET    /api/v1/repos/{repo_id}/history?ref=&path=
GET    /api/v1/repos/{repo_id}/blame?ref=&path=
POST   /api/v1/markdown/render                            (generic, see §35.1.8)

GET    /api/v1/repos/{repo_id}/issues
POST   /api/v1/repos/{repo_id}/issues                     (v1.5 stub returns 501)
GET    /api/v1/repos/{repo_id}/issues/{iid}
PATCH  /api/v1/repos/{repo_id}/issues/{iid}

GET    /api/v1/repos/{repo_id}/merge-requests?state=
POST   /api/v1/repos/{repo_id}/merge-requests             (Idempotency-Key)
GET    /api/v1/repos/{repo_id}/merge-requests/{iid}
GET    /api/v1/repos/{repo_id}/merge-requests/{iid}/diff
GET    /api/v1/repos/{repo_id}/merge-requests/{iid}/checks
GET    /api/v1/repos/{repo_id}/merge-requests/{iid}/blockers
GET    /api/v1/repos/{repo_id}/merge-requests/{iid}/threads
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/threads
PATCH  /api/v1/repos/{repo_id}/merge-requests/{iid}/threads/{thread_id}
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/comments
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/reviews
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/approve   (Idempotency-Key)
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/request-changes
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/merge     (Idempotency-Key)
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/rebase
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/close
POST   /api/v1/repos/{repo_id}/merge-requests/{iid}/reopen

GET    /api/v1/repos/{repo_id}/pipelines
GET    /api/v1/repos/{repo_id}/pipelines/{pipeline_id}
GET    /api/v1/repos/{repo_id}/jobs/{job_id}/log
POST   /api/v1/repos/{repo_id}/jobs/{job_id}/retry        (Idempotency-Key)
POST   /api/v1/repos/{repo_id}/jobs/{job_id}/cancel

GET    /api/v1/repos/{repo_id}/settings
POST   /api/v1/repos/{repo_id}/settings/preview
PATCH  /api/v1/repos/{repo_id}/settings                   (Idempotency-Key + If-Match)
GET    /api/v1/repos/{repo_id}/members
PUT    /api/v1/repos/{repo_id}/members/{principal_id}     (Idempotency-Key)
DELETE /api/v1/repos/{repo_id}/members/{principal_id}     (Idempotency-Key)
GET    /api/v1/repos/{repo_id}/protection
PATCH  /api/v1/repos/{repo_id}/protection                 (Idempotency-Key + If-Match)
GET    /api/v1/repos/{repo_id}/secrets
POST   /api/v1/repos/{repo_id}/secrets                    (Idempotency-Key)
POST   /api/v1/repos/{repo_id}/secrets/{secret_name}/rotate (Idempotency-Key)
DELETE /api/v1/repos/{repo_id}/secrets/{secret_name}      (Idempotency-Key)

POST   /api/v1/actions/preview
POST   /api/v1/actions/execute                            (Idempotency-Key)
GET    /api/v1/activity?since=&limit=&scope=
GET    /api/v1/search?q=&kinds=&limit=
GET    /api/v1/ws                                         (WebSocket upgrade)
```

Headers used on mutating routes:
- `Content-Type: application/json`
- `X-CSRF-Token: <cookie value>` (cookie-auth mode)
- `Authorization: Bearer <token>` (token-auth mode; either CSRF or Authorization)
- `Idempotency-Key: <uuid>` (where shown)
- `If-Match: "<hex-state-hash>"` (settings/protection — optimistic concurrency)

### 35.8 Canonical idempotency contract

- `Idempotency-Key` is required on: create-repo, archive, delete, branch create/delete, tag create, MR create, MR merge, MR approve, MR rebase, run retry, settings patch, members PUT/DELETE, protection patch, secrets create/rotate/delete, actions execute.
- Server stores `(action_kind, target_id, idempotency_key) → result` in `web_action_receipts`.
- Replay with same key + same body → returns stored result (200).
- Replay with same key + different body → 409 `idempotency_conflict`.
- TTL: 24 h (`DELETE FROM web_action_receipts WHERE created_at < now - 24h` nightly).

### 35.9 Final acceptance criteria delta

Acceptance criteria from §9 are unchanged in spirit; apply these clarifications:

- Item 1: launches at `127.0.0.1:8787` (or configured `--bind`).
- Item 2: lists repos via stable `repo_id` internally; SPA displays human paths.
- Item 7: 409 returned with structured envelope per §35.1.11 and code `merge_sha_stale`.
- Item 9: settings 409 uses code `settings_hash_stale`.
- Item 10: gap recovery uses `snapshot_required` + bootstrap refetch; clients dedup by `seq`.
- Item 14: UX-QA receipt JSON is at `target/jankurai/ux-qa/web-forge.<ts>.json` AND `target/jankurai/ux-qa.json` (the latter from `jankurai ux audit`).

### 35.10 Where Codex's plan could be improved (notes for any reviewer)

Recording observations a reviewer might raise about Codex's plan, for transparency:

1. **No file-ownership matrix** — agents working on overlapping files will collide unless someone tracks file → owner.
2. **No dependency graph** — the English "depends on Wx" is correct but doesn't surface the critical path.
3. **No agent onboarding / claim protocol** — first-day friction.
4. **No concrete deployment** — systemd, Docker, reverse-proxy missing.
5. **No operator runbook** — first-incident friction.
6. **`apps/ux-qa-artifacts/` mention** — Codex flags "but committed artifacts should be avoided unless project policy requires them" — correct; my §28+ keeps proof receipts in `target/jankurai/` outside the repo. Confirm with `agent/generated-zones.toml` author.
7. **`POST /api/v1/repos/preview`** as a separate route — fine, but consider `POST /api/v1/repos { dry_run: true }` for uniformity with other previews. We'll standardize on Codex's approach (separate `/preview` endpoint) since it's clearer in the URL and easier to permission separately.

### 35.11 Summary

This synthesis pulls 19 critical adoptions from Codex's plan, keeps 15 unique improvements from this plan, and resolves 6 deliberate divergences. The composite plan is now the canonical execution plan for v1.0. Any implementing agent should:

1. Read §0–§4 for vision/architecture.
2. Read §35 for the canonical URL/protocol/idempotency rules.
3. Pick a work package from §7 / §31 / §35.6.
4. Execute per §6 claim protocol and §13 DoD.

Both plans coexist in the repo (`WEB_WORK_CLAUDE.md` and `WEB_WORK_CODEX.md`); when they conflict, **this document's §35 is authoritative**.

---

## 36. CHANGELOG / VERSION HISTORY (updated)

- **v1.1 — 2026-05-26** (later same day): Synthesis pass after reading `WEB_WORK_CODEX.md`. Adopted: `/api/v1/` versioning, stable `repo_id` in paths, `Idempotency-Key` header, dual cache-key (renderer + sanitizer), preserved engine routes, WS per-scope perms, structured error envelope, 14-step action algorithm, granular WS scopes, expanded permission set, generic markdown render endpoint, explicit `/raw`, broader README lookup, geometry checks in Playwright, jankurai proof commands. New packages: W-F-11, W-F-12, W-F-13. Revisions: W-B-08′, W-B-11′, W-B-13′, W-CC-06′, W-CC-07′. Author: Claude Opus 4.7 (1M context).
- **v1.0 — 2026-05-26.** Initial comprehensive plan. Synthesized from `tips/web/JERYU_FULL_WEB_FORGE_ENGINEERING_SPEC_FINAL.md` plus ecosystem analysis. Covers ~78 work packages across 8 phases. Author: Claude Opus 4.7 (1M context) on behalf of `jepson@veox.ai`.

---

> **End of plan.** The implementing agent(s) should start with W-F-00 (claim tracker), then W-F-01..13 in any order (most parallel), and proceed through phases 1..7 per section 8. Apply the §35 synthesis throughout — §35 supersedes earlier sections on URLs, idempotency, cache keys, error envelopes, action algorithm, WS scopes, and permission keys.
>
> Canonical low-level design reference: `tips/web/JERYU_FULL_WEB_FORGE_ENGINEERING_SPEC_FINAL.md`.
> Sibling plan: `WEB_WORK_CODEX.md` — many ideas adopted in §35; conflicts resolved in this document's favor per §35.5.
>
> **Self-test:** finish every package in §7 + §31 + §35.6, hit every acceptance criterion in §9 (with §35.9 clarifications), produce every artifact in §10.7 / §19, check the Definition of Done in §13 for every package — then v1.0 is shipped. No hidden requirements.
