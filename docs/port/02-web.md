# Port Spec 02 — Web App Subsystem (React SPA + axum BFF → jeryu-api)

**Status:** authoritative, execution-ready. **Product:** `jeryu`. **Edition:** Rust 2024.
**Scope owner (this spec):** web subsystem only — `web/` (ported React SPA) + the REST/WS edge that the SPA talks to (lands as routes inside **`jeryu-api`**).
**Out of scope (Codex owns, DO NOT EDIT):** `jeryu-core` (forge-core), `jeryu-gitd`, `jeryu-runnerd`, `ci-scheduler`, runner crates, and the core PR/diff/check engine types. This spec *consumes* those; it does not author them.

**Locked decisions honored:** (D1) ZERO `gitlab`/`jitforge`/`JitForge`/`Nitro` literals survive; only `jeryu`/`jeryu-*`. (D2) engine crates renamed to `jeryu-*` (notably `forge-core→jeryu-core`, `jitforge-api→jeryu-api`). (D3) keep jeryu's SQLite+RedlineDB `db/` layer, the axum HTTP daemon, ratatui TUI, and the React web app; GitLab backend replaced 100% by jeryu-* core. (D4) **MergeRequest/MR → PullRequest/PR** everywhere (heaviest rewire). (D5) runners OCI-first then native sandbox (irrelevant to the SPA except CI check display).

> **Naming note for the porting agent:** every identifier below written as `MergeRequest`/`MR`/`mr`/`iid`/`merge-request(s)`/`pipeline`/`job`/`gitlab` is a *source* token to be REWRITTEN. The *target* tokens are `PullRequest`/`PR`/`pr`/`number`/`pulls`/`workflow run` (a.k.a. CI run)/`check run`/(host-neutral). The route segment `/merge-requests/{iid}` becomes `/pulls/{number}` and the URL param `:iid` becomes `:number`.

---

## 0. Reading map (sources studied)

- SPA root: `/home/ubuntu/jeryu/apps/web/` (React 18 + TS + Vite 5 + Playwright + Storybook + Vitest + MSW).
- axum BFF: `/home/ubuntu/jeryu/src/web/**` (router, rest/*, state, auth, csrf, ws, audit, idempotency).
- Realtime: `/home/ubuntu/jeryu/src/web_events/**` (bus, projection, protocol, subscription) + `src/api/websocket.rs`.
- Target domain types (Codex-owned, read-only): `/home/ubuntu/jeryuRUST/crates/forge-core/src/model.rs` (`PullRequest`, `PullRequestState`, `Review`, `ReviewComment`, `CheckRun`, `CheckSuite`, `CheckRunStatus`, `CheckConclusion`, `CommitStatus`, `WorkflowRun`, `BranchProtectionRule`), `crates/forge-core/src/phase7.rs`, `crates/forge-core/src/webhooks.rs` (`WebhookEventEnvelope`), `crates/ci-scheduler/src/` (`schedule.rs`, `merge_queue.rs`).
- Current jeryu-api facade (must grow GitHub-shaped routes): `/home/ubuntu/jeryuRUST/crates/jitforge-api/{API_SURFACE.md,src/routes.rs,tests/github_api.rs}`. NOTE: the GitHub-compat REST routes are currently *deferred* (404 fallback in `routes.rs:50-53`; documented "deferred P0" in `API_SURFACE.md:17-20`). **This subsystem requires them to be implemented** — see §4 ordering.

---

## 1. Source inventory

### 1.1 React SPA — app shell & infrastructure (`/home/ubuntu/jeryu/apps/web/src/`)

| File | Purpose |
|---|---|
| `main.tsx` | Vite entry; mounts `<App/>` into `#root`. |
| `app/App.tsx` | Root component: providers + RouterProvider. |
| `app/providers.tsx` | React Query client, theme/preferences, realtime store bootstrap. |
| `app/router.tsx` | `createBrowserRouter` route map (see §1.2). **Carries the `/merge-requests/:iid` routes + GitLab nested-group splat comments to rewrite.** |
| `api/client.ts` | `apiGet/apiSend/apiPatch/apiDelete` fetch wrappers; parses `{error:{code,message,details,request_id,event_cursor}}` envelope (§35.1.11) into `ApiError`; sets `Idempotency-Key` + `X-CSRF-Token`. |
| `api/endpoints.ts` | **Single source of truth for every REST URL** (all `/api/v1/...`). Heaviest rewrite target on the FE — every `mergeRequest*` builder → `pull*`. |
| `api/types.ts` | Re-exports ts-rs generated DTOs from `contracts/generated/*` + FE-local diff/checks/threads wire types (`MergeRequestDiff`, `MergeRequestCheck`, `MergeRequestChecks`, `MergeRequestThreadList`, `MergeApproveRequest`, `MergeMergeRequest`). |
| `api/websocket.ts` | `JeRyuWsClient`: raw socket lifecycle, `jeryu.ws.v1` protocol (Hello/Subscribe/Unsubscribe/Ping/Ack; server Hello/Event/SnapshotRequired/Pong/Error), exp-backoff reconnect, 15s heartbeat / 30s read timeout, resume-cursor gap detection. **Already named `jeryu.*`; preserve verbatim.** |
| `global.d.ts` | Ambient TS types. |

### 1.2 React SPA — routes & pages (`src/pages/`, `src/layout/`)

Route map (`app/router.tsx`):

| Route | Page component | Status today | Notes |
|---|---|---|---|
| `/` index | `DashboardPage` | implemented (minimal) | "what needs attention", bootstrap + realtime pill. |
| `/repos` | `RepositoriesPage` | implemented (W-FE-08) | list/grid, debounced search, family grouping, create dialog. |
| `/repos/new` | `RepositoriesPage mode="create"` | implemented | create flow flag. |
| `/repos/:provider/:fullName/code` | `RepositoryCodePage` | implemented (W-FE-10) | tree browser, fuzzy file finder (`t`). |
| `/repos/:provider/:fullName/blob/*` | `RepositoryFilePage` | implemented | splat = `${ref}/${path}`; raw/download/permalink. |
| `/repos/:provider/:fullName/merge-requests` | `RepositoryMergeRequestsPage` | **STUB** | PR list with filters → must become `/pulls`. |
| `/repos/:provider/:fullName/merge-requests/:iid` | `MergeRequestPage` | implemented (W-FE-11) | **3-pane PR cockpit; heaviest rewire** (§3). |
| `/repos/:provider/:fullName/issues` | `IssuesPage` | **STUB** | filters: state/label/assignee/milestone. |
| `/repos/:provider/:fullName/settings/:section?` | `RepositorySettingsPage` | implemented | settings studio (general/merge/branch-protect/CI/agents/secrets). |
| `/repos/:provider/:fullName/*` and `/repos/:provider/:fullName` | `RepositoryOverviewPage` | implemented (W-FE-09) | README, clone popover, sidebar. |
| `/merge-room` | `MergeRoomPage` | **STUB** | cross-repo PR cockpit. → `/pull-room` or keep path, but title/copy → PR. |
| `/notifications` | `NotificationsPage` | implemented (W-FE-18) | inbox grouped by date, from realtime store. |
| `/audit` | `AuditPage` | **STUB** | audit timeline backed by `web_action_receipts` + `audit_events`. |
| `/search` | `SearchResultsPage` | implemented | per-kind search hits; **builds `/repos/gitlab/...` URLs (literal to fix)**. |
| `/settings` | `AdminSettingsPage` | implemented (minimal) | theme/density; full §4.7 studio later. |
| `*` | `NotFoundPage` | implemented | 404. |

Layout (`src/layout/`): `AppShell.tsx` (outlet shell), `GlobalHeader.tsx`, `LeftNav.tsx`, `LiveActivityDock.tsx` (consumes realtime events), `RepoSwitcher.tsx`, `StatusBar.tsx`, `UserMenu.tsx`, `CommandPalette.tsx` + `useShellCommands.ts` (⌘K palette, registers nav/preference commands), `CommandPalette.stories.tsx`.

### 1.3 React SPA — feature components (`src/components/`)

| Dir/File | Purpose |
|---|---|
| `action/{ActionButton,ActionPreviewDialog,RiskBadge,index}.tsx` | Generic preview→execute action primitives + risk badge. |
| `browser/{BranchSelector,Breadcrumbs,CodeViewer,FileTree,MarkdownRenderer,ReadmePanel,index}.tsx` | Repo code-browsing widgets (Monaco viewer, virtualized tree, sanitized markdown). |
| `merge/{ChecksPanel,DiffFileTree,DiffViewer,InlineComment,MergeGatePanel,ReviewSidebar,ThreadList,index}.tsx` | **PR cockpit widgets — heaviest rewire.** `MergeGatePanel` renders the merge passport; `ChecksPanel` renders CI checks; `DiffViewer`/`DiffFileTree` render the unified diff; `ReviewSidebar` hosts approve/merge buttons; `ThreadList`/`InlineComment` render review threads. |
| `repo/{RepoCard,RepoFamilyGroup,RepoHealthPill,RepoTable,CreateRepoDialog,index}.tsx` | Repo list cards/table/health + create dialog (**`<option value="gitlab">` literal to fix**, `CreateRepoDialog.tsx:265`, default `gitlab` at `:82`). |
| `settings/{AgentPolicyEditor,BranchProtectionEditor,MergePolicyEditor,SecretsMetadataTable,SettingsDiffPreview,SettingsLayout,SettingsSection,index}.tsx` | Settings studio editors. `MergePolicyEditor` → "PR merge policy". |
| `state/{EmptyState,ErrorState,LoadingState,PermissionDeniedState,index}.tsx` | The five UX-QA states. |
| `KeyboardShortcutsOverlay.tsx` | `?` help overlay. |
| `NotificationInbox.tsx` | Header popover + list view; **builds `/repos/gitlab/...` and `.../merge-requests/${iid}` URLs (literals at `:193,:200` to fix)**. |

### 1.4 React SPA — hooks (`src/hooks/`)

| Hook | Endpoint(s) | Purpose |
|---|---|---|
| `useBootstrap.ts` | `GET /api/v1/bootstrap` | First-paint snapshot (viewer, flags, recent repos, ws url). |
| `useRepositories.ts` | `GET /api/v1/repos` | List + filters; comment lists wire hosts `gitlab/github/local` (`:27`). |
| `useRepository.ts` | `GET /api/v1/repos/{id}` | Repo detail. |
| `useResolveRepo.ts` | (list cache) | Maps `:provider/:fullName` → opaque repo id. |
| `useRefs.ts` | `GET .../refs` | Branch/tag selector data. |
| `useRepoTree.ts` | `GET .../tree?ref&path` | Lazy directory listing. |
| `useBlob.ts` | `GET .../blob?ref&path&render` | File content (+ markdown HTML). |
| `useMarkdown.ts` | `POST /api/v1/markdown/render` | Standalone markdown render. |
| `useSearch.ts` | `GET /api/v1/search` | Global search (FE-local `SearchResults` types). |
| `useMergeRequest.ts` | `GET .../merge-requests/{iid}` | **→ PR detail.** |
| `useMrDiff.ts` | `GET .../merge-requests/{iid}/diff` | **→ PR diff;** invalidated on `mr.diff_recomputed` WS event. |
| `useMrChecks.ts` | `GET .../merge-requests/{iid}/checks` | **→ PR check runs.** |
| `useMrThreads.ts` | `GET .../merge-requests/{iid}/threads` | **→ PR review threads;** invalidated on `mr.thread.*`. |
| `useApproveMr.ts` | `POST .../merge-requests/{iid}/approve` | **→ PR approve;** per-attempt `Idempotency-Key`; handles `merge_sha_stale` 409. |
| `useMergeMr.ts` | `POST .../merge-requests/{iid}/merge` | **→ PR merge;** carries `expected_passport_hash`. |
| `useRepoSettings.ts` | `GET .../settings` | Settings snapshot. |
| `usePreviewSettingsPatch.ts` | `POST .../settings/preview` | Diff preview + base hash. |
| `useApplySettingsPatch.ts` | `PATCH .../settings` | Apply patch (`Idempotency-Key` + `If-Match`). |
| `useRealtime.ts` | (ws store) | Ref-counted scope subscribe/unsubscribe per mount. |
| `useKeyboard.ts` | — | Keyboard shortcut binding. |

### 1.5 React SPA — stores, tests, config

- `stores/{commandStore,preferencesStore,realtimeStore,selectionStore}.ts` — Zustand. `realtimeStore.ts` owns the singleton `JeRyuWsClient`, `lastSeq` persisted to `sessionStorage` key `jeryu.ws.lastSeq.v1`, 200-event rolling buffer, ref-counted subscriptions, invalidation listeners. `selectionStore` tracks `currentRepo`/`currentMr` (→ `currentPr`).
- `test/{mocks,server,setup}.ts` — MSW handlers + Vitest setup.
- `e2e/` — Playwright: `01-bootstrap`, `02-repos`, `03-readme`, `04-code`, `05-mr-review`, `06-approve-sha`, `07-settings`, `08-ws-reconnect`, `09-permissions`, `10-a11y`; page objects in `e2e/pages/`; fixtures `e2e/fixtures/{auth,mocks,accessibility,data/bootstrap.json}`. **`05-mr-review` + `06-approve-sha` are the PR-cockpit gate; several specs carry `gitlab` literals (see §5).**
- `.storybook/`, `vite.config.ts`, `vitest.config.ts`, `playwright.config.ts`, `tsconfig.json`, `perf/lighthouse-budget.json`, `.lighthouseci/`, package.json scripts: `dev/build/typecheck/lint/test/test:e2e/storybook/ux-qa/perf`.
- Generated DTO tree (sibling of `apps/web`): `contracts/generated/*.ts` (ts-rs output). MUST be regenerated from the new `jeryu-api` Rust types (renamed `Pull*`).

### 1.6 axum BFF (`/home/ubuntu/jeryu/src/web/`)

| File | Purpose |
|---|---|
| `mod.rs` | Module wiring for the web BFF. |
| `router.rs` | **`build_web_router(state, engine, spa_dir)`** — assembles `/api/v1/*` (auth+CSRF), `/api/v1/ws`, merges the `engine` router (`/health`,`/hooks`,`/cache/summary`) and SPA static fallback. Full route table here. |
| `state.rs` | `WebState` Arc bundle: `event_bus`, `repo_service`, `browser_service`, `settings_service`, `merge_service`, `review_service`, `passport_service`, **`gitlab_client: Arc<GitLabClient>`**, `activity_buffer`, `session_store`, `db_pool`, `idempotency`, `action_receipts`. `new_for_serve` reads **`JERYU_GITLAB_BASE_URL`/`JERYU_GITLAB_TOKEN`** env (literals to replace). |
| `auth.rs` | Auth middleware (`auth_layer`), `Viewer` extension, `__Host-jeryu-session` cookie, `SESSION_COOKIE_NAME`. |
| `csrf.rs` | `csrf_layer`, `__Host-jeryu-csrf`, `CSRF_HEADER`, bypass paths (login). |
| `permissions.rs` | `perms::{REPO_READ,REPO_CREATE,MR_READ,MR_WRITE,MR_APPROVE,MR_REVIEW,MR_MERGE,CI_READ,CI_WRITE,SETTINGS_WRITE,...}`, `require()`, `perm_for_scope()`. **`MR_*` perm keys → `PR_*`.** |
| `sessions.rs` | `SessionStore` (sqlx `sessions` table). |
| `idempotency.rs` | Process-local idempotency cache. |
| `audit.rs` | `write_audit(pool, actor, action, target, RiskTier, payload)` → `audit_events`. |
| `error.rs` | `ApiError` → §35.1.11 envelope. |
| `telemetry.rs` | `instrument()` request-id/trace/timeout/compression. |
| `ws.rs` | `ws_handler` — WS upgrade, `jeryu.ws.v1` server side, scope-gated fan-out (high/medium on `subscribe_high`, low on `subscribe_low`), `SnapshotRequired{reason:"lag"}`. **Already `jeryu.*`; transport preserved.** |
| `openapi.rs` | utoipa OpenAPI doc assembly. |
| `static_assets.rs` | `spa_router(spa_dir)` ServeDir + index fallback. |
| `action_receipts.rs` | `WebActionReceiptStore` (`web_action_receipts`, `UNIQUE(action_kind,target_id,idempotency_key)`). |
| `rest/mod.rs` | REST submodule wiring. |
| `rest/bootstrap.rs` | `GET /api/v1/bootstrap` → `WebBootstrap` (viewer, TUI read-model, recent repos, ws url, flags). |
| `rest/repos.rs` | `GET/POST /repos`, `POST /repos/preview`, `GET/PATCH /repos/{id}`. Uses `jeryu::api::repository::*`, `RepoService`. |
| `rest/repo_browser.rs` | `refs/tree/blob/raw/readme/compare/commits/history/blame`. |
| `rest/settings.rs` | `GET/PATCH /settings`, `POST /settings/preview`. |
| `rest/merge_requests.rs` | **PR cockpit edge — heaviest rewire.** list/get/create/patch/diff/checks/blockers/approve/request-changes/merge/close/reopen/rebase. Calls `state.merge_service` + `GitHost::list_pipelines(state.gitlab_client,...)` (`merge_requests.rs:550-562`). |
| `rest/reviews.rs` | `GET/POST .../threads`, `PATCH .../threads/{thread_id}`, `POST .../comments`, `POST .../reviews`. |
| `rest/ci.rs` | **pipelines/jobs/checks edge.** `list_pipelines/get_pipeline/list_pipeline_jobs/get_job_log/retry_job/cancel_job/list_checks`. All via `GitHost::list_pipelines/list_jobs/get_job_log(state.gitlab_client,...)`. Emits WS events `workflow.run.started`/`check.completed`. |
| `rest/issues.rs` | issues list/get/create/patch (currently 501 compat). |
| `rest/agents.rs` | `agents/sessions`, `agents/evidence` (read-only). |
| `rest/actions.rs` | `POST /actions/preview`, `/actions/execute`. |
| `rest/search.rs` | `GET /api/v1/search`. |
| `rest/activity.rs` | `GET /api/v1/activity` — rolling 500-event window with scope/since/limit filters. |
| `rest/markdown.rs` | `POST /api/v1/markdown/render`. |
| `rest/auth.rs` | `POST /api/v1/auth/login` (provider `local` via `JERYU_LOCAL_USERS`; `gitlab` → 501) + `/logout`. **`provider:"gitlab"` 501 branch (`auth.rs:141-144`) → host-neutral provider; env literal stays `JERYU_LOCAL_USERS`.** |

### 1.7 Realtime event layer (`/home/ubuntu/jeryu/src/web_events/`)

| File | Purpose |
|---|---|
| `mod.rs` | `WebEventBus` re-export + module root (`§35.1.15` canonical entry). |
| `bus.rs` | `WebEventBus`: dual broadcast channels (high/low), `publish`, `current_seq`, `subscribe_high/low`, drop policy by `EventPriority`. |
| `protocol.rs` | `EventPriority::from_kind()` — classifies WS event kinds. **Contains `mr.approved`/`mr.merged` (High) kinds (`protocol.rs:39-40`) and `pipeline.created` test ref (`:84`) → `pull.approved`/`pull.merged`/`workflow.run.*`.** |
| `projection.rs` | DB event → `WebEvent` projection. |
| `subscription.rs` | `SubscriptionRegistry` / `SubscriptionSpec` (scope-keyed). |
| `src/api/websocket.rs` (physical home) | Wire types `ClientWsMessage`, `ServerWsMessage`, `WebEvent`, `SubscriptionSpec`. |

---

## 2. Target layout in `/home/ubuntu/jeryuRUST`

### 2.1 React SPA → `web/`

Port `apps/web/` to a top-level **`/home/ubuntu/jeryuRUST/web/`** with identical internal structure (`src/{api,app,components,hooks,layout,pages,stores,test}`, `e2e/`, `.storybook/`, configs). Rationale: jeryu's `apps/web` already uses the `jeryu` brand and `jeryu.ws.v1` protocol — this is a *rename of GitLab domain concepts*, not a rewrite of the app.

Generated contracts: keep a sibling **`/home/ubuntu/jeryuRUST/contracts/generated/`** ts-rs output dir; the SPA imports DTOs from there via `web/src/api/types.ts`. Regenerate from the renamed `jeryu-api` Rust types so `Pull*` DTOs replace `MergeRequest*`.

Module-level renames inside `web/src/`:
- `api/endpoints.ts`: `mergeRequests→pulls`, `mergeRequest→pull`, `mergeRequestDiff→pullDiff`, `mergeRequestChecks→pullChecks`, `mergeRequestThreads→pullThreads`, `mergeRequestReviews→pullReviews`, `mergeRequestComments→pullComments`, `mergeRequestApprove→pullApprove`, `mergeRequestMerge→pullMerge`. Path strings `/merge-requests/...`→`/pulls/...`, param `iid`→`number`.
- `pages/`: `MergeRequestPage.tsx→PullRequestPage.tsx`, `RepositoryMergeRequestsPage.tsx→RepositoryPullRequestsPage.tsx`, `MergeRoomPage.tsx→PullRoomPage.tsx` (keep `/merge-room` route alias optional; default to `/pull-room`).
- `hooks/`: `useMergeRequest→usePullRequest`, `useMrDiff→usePrDiff`, `useMrChecks→usePrChecks`, `useMrThreads→usePrThreads`, `useApproveMr→useApprovePr`, `useMergeMr→useMergePr`.
- `components/merge/` may keep dir name (`merge` is host-neutral) but `MergeGatePanel`/`ReviewSidebar`/`ChecksPanel` copy strings and types swap to PR/check-run.
- `stores/selectionStore.ts`: `currentMr→currentPr`, `setCurrentMr→setCurrentPr`.
- WS protocol module `api/websocket.ts` and `JeRyuWsClient`: **unchanged** (already branded `jeryu`).

### 2.2 axum BFF → routes inside `jeryu-api` (+ shared `jeryu-web` support)

The BFF is *not* a free-standing GitLab adapter; it is the HTTP edge the SPA talks to. Target:

```
crates/jeryu-api/                         (renamed from jitforge-api per D2)
  src/
    lib.rs / main.rs                      (existing)
    routes.rs                             (existing phase10 facade — KEEP)
    web/                                  (NEW: ported from jeryu/src/web)
      mod.rs
      router.rs        ← build_web_router(state, engine, spa_dir)
      state.rs         ← WebState (gitlab_client → core_client: Arc<ForgeClient over jeryu-core>)
      auth.rs csrf.rs sessions.rs idempotency.rs audit.rs error.rs
      telemetry.rs static_assets.rs action_receipts.rs permissions.rs ws.rs openapi.rs
      rest/
        bootstrap.rs repos.rs repo_browser.rs settings.rs
        pulls.rs       ← (renamed merge_requests.rs)
        reviews.rs ci.rs issues.rs agents.rs actions.rs search.rs activity.rs markdown.rs
  web_events/          (NEW: ported from jeryu/src/web_events) — bus/projection/protocol/subscription
```

`jeryu-api` `Cargo.toml` (D2 rename) depends on `jeryu-core` (renamed forge-core), `ci-scheduler`, `jeryu-gitd` (read paths), `jeryu-proof` (passport), `jeryu-obs` (metrics), plus axum/tower/sqlx/utoipa. The wire DTO crate that ts-rs reads from is co-located (e.g. `jeryu-api`'s `web_read_model` / `api::pull` modules) and exported to `contracts/generated/`.

> If Codex's `jeryu-api` ownership boundary forbids adding the web edge inside that crate, land it as a thin sibling crate **`jeryu-web`** that `use`s `jeryu_api` route handlers + `jeryu_core` types. Either layout satisfies D2/D3; pick whichever does not require editing Codex-owned core files. Default to embedding under `jeryu-api/src/web` since the SPA's `/api/v1` edge and the GitHub-shaped `/api/v1` core routes must share one axum app and one `WebState`.

---

## 3. Rewire map (MR → PR, pipeline → CI run, GitLab client → jeryu-core)

The SPA already targets `/api/v1/...`. The job is: (a) rename FE concept tokens, (b) re-point each route to **GitHub-shaped jeryu-api routes** backed by `jeryu-core` (forge-core) types, (c) delete the `GitLabClient`/`GitHost` substrate.

### 3.1 Domain-type rewire

| Source symbol / data | Current (GitLab) source | Target jeryu-* type / API |
|---|---|---|
| `MergeRequest` / `MergeRequestSummary` / `MergeRequestDetail` | `jeryu::api::merge_request::*`, `MergeService` over `GitLabClient` | `jeryu_core::PullRequest` (`forge-core/src/model.rs:241`) + a `PullRequestDetail` view (PR + passport). |
| MR `iid` (string) | GitLab internal id | `PullRequest.number: u64` (`model.rs:245`). FE param `:iid`→`:number`. |
| `MergeRequestState` | `jeryu::api::merge_request` enum | `jeryu_core::PullRequestState` (`model.rs:202`: Draft/Open/ReadyForReview/BlockedByPolicy/BlockedByChecks/Approved/Queued/SpeculativeMergeTesting/Mergeable/Merged/Closed). Map FE `open/closed/merged/draft` onto these. |
| `source_branch`/`target_branch`/`head_sha` | MR fields | `PullRequest.head: GitBranchRef` / `.base: GitBranchRef` (`model.rs:217`), `head.sha`. |
| `MergeRequestDiff` / `DiffFile` / hunks | `MergeService::diff` (GitLab compare) | jeryu-api PR diff route over `jeryu-gitd` diff (compare base..head). Keep FE `MergeRequestDiff` wire shape (rename to `PullRequestDiff`); fill `risk` from `jeryu-proof`. |
| `MergeRequestCheck` / `MergeRequestChecks` / `PipelineSummary` | `GitHost::list_pipelines` over GitLab | `jeryu_core::CheckRun` + `CheckRunList` (`model.rs:421/450`), `CheckRunStatus`/`CheckConclusion` (`model.rs:393/401`). Aggregate counts from `ci-scheduler`/`CheckSuite` (`model.rs:456`). |
| `ReviewThread` / `ReviewComment` / `ReviewVerdict` | `ReviewService` over GitLab notes | `jeryu_core::Review` + `ReviewComment` (`model.rs:303/315`), `ReviewState` (`model.rs:291`: APPROVED/CHANGES_REQUESTED/COMMENTED/DISMISSED). Thread grouping is a jeryu-api view over comments keyed by `path`+`line`. |
| `SubmitReviewRequest` / `CreateReviewCommentRequest` | jeryu api review | `jeryu_core::CreateReviewRequest` (`model.rs:337`) + `ReviewCommentInput` (`model.rs:329`). |
| `MergePassport` / `MergePassportBlocker` / `Mergeability` | `MergePassportService` over GitLab pipelines | `jeryu-proof` passport (proofcore) + `jeryu_core::BranchProtectionRule` (`model.rs:467`) gates + `CombinedStatus` (`model.rs:385`). Keep FE `MergePassport*` wire shape; rename copy to "PR merge passport". |
| approve receipt / merge receipt | `MergeService::approve_exact_sha`/`merge_exact_sha` | jeryu-api PR `approve`/`merge` writing through `jeryu-core` + `jeryu-proof`; `expected_head_sha` optimistic check preserved; merge via `ci-scheduler::merge_queue` (`crates/ci-scheduler/src/merge_queue.rs`). |
| Pipeline / `HostPipeline` | `GitHost::list_pipelines` | `jeryu_core::WorkflowRun` (`model.rs:615`) / `WorkflowRunList` (`model.rs:625`) backed by `ci-scheduler`. "pipeline" copy → "workflow run" / "CI run". |
| Job / `HostJob` / `JobItem` / job log | `GitHost::list_jobs`/`get_job_log` | `ci-scheduler` tasks (`schedule.rs`) → exposed as check runs / run jobs; job log via `jeryu-obs` log store. retry/cancel via `ci-scheduler` lease ops (`leases.rs`). |
| `RepositorySummary` / `RepositoryId` / facets `host: gitlab` | `RepoService` over GitLab projects | `jeryu_core::Repository` (`model.rs:74`). Host facet values become host-neutral (`jeryu`); drop `gitlab` literal. |
| tree/blob/refs/readme/compare/commits/blame | `RepoBrowserService` over GitLab | `jeryu-gitd` read APIs (object/ref/tree/blob/compare/log/blame). |
| `GitLabClient` / `GitHost` / `state.gitlab_client` | `jeryu::git_host` | DELETE. Replace with `core_client: Arc<jeryu_core::ForgeClient>` (or direct service structs over `jeryu-core` + `jeryu-gitd` + `ci-scheduler`) in `WebState`. |
| `JERYU_GITLAB_BASE_URL` / `JERYU_GITLAB_TOKEN` (`state.rs:132-134`) | env | `JERYU_CORE_URL` / `JERYU_CORE_TOKEN` (host-neutral). Default to in-process `jeryu-core` when unset (no remote). |
| `provider:"gitlab"` login branch (`auth.rs:141`) | GitLab OAuth stub | host-neutral provider (`jeryu` / OIDC); keep `local` provider + `JERYU_LOCAL_USERS`. |
| `perms::MR_*` (`permissions.rs`) | MR perm keys | `perms::PR_*` (`pr.read/write/approve/review/merge`); keep `ci.read/write`, `settings.write`, `repo.*`. |

### 3.2 Route rewire (FE endpoint → jeryu-api GitHub-shaped route)

The SPA keeps the `/api/v1` prefix (its single source of truth is `endpoints.ts`). jeryu-api must serve **GitHub-shaped** paths under `/api/v1`. Mapping (left = current SPA path / `endpoints.ts` builder; right = target jeryu-api route + backing type):

| SPA builder (`endpoints.ts`) | Current path | Target jeryu-api route | Backing |
|---|---|---|---|
| `bootstrap()` | `/api/v1/bootstrap` | unchanged | `rest/bootstrap.rs` (recent repos via `jeryu-core`). |
| `repos()` / `repo(id)` | `/api/v1/repos`, `/repos/{id}` | unchanged | `jeryu_core::Repository`. |
| `refs/tree/blob/raw/readme/compare` | `/api/v1/repos/{id}/...` | unchanged | `jeryu-gitd`. |
| `mergeRequests(id,state)` → `pulls` | `/repos/{id}/merge-requests` | `/repos/{id}/pulls?state=` | `jeryu_core::PullRequest` list. |
| `mergeRequest(id,iid)` → `pull` | `/repos/{id}/merge-requests/{iid}` | `/repos/{id}/pulls/{number}` | PR detail. |
| `mergeRequestDiff` | `.../merge-requests/{iid}/diff` | `/repos/{id}/pulls/{number}/files` (GitHub) **or** `/diff` | gitd compare. |
| `mergeRequestChecks` | `.../merge-requests/{iid}/checks` | `/repos/{id}/commits/{sha}/check-runs` (resolve head_sha) | `CheckRunList`. |
| `mergeRequestThreads` | `.../merge-requests/{iid}/threads` | `/repos/{id}/pulls/{number}/comments` grouped → threads view | `ReviewComment`. |
| `mergeRequestReviews` | `.../merge-requests/{iid}/reviews` | `/repos/{id}/pulls/{number}/reviews` | `Review` / `CreateReviewRequest`. |
| `mergeRequestComments` | `.../merge-requests/{iid}/comments` | `/repos/{id}/pulls/{number}/comments` (POST) | `ReviewCommentInput`. |
| `mergeRequestApprove` | `.../merge-requests/{iid}/approve` | `/repos/{id}/pulls/{number}/reviews` with `event:APPROVED` (or dedicated `/approve`) | `Review` + `jeryu-proof`. |
| `mergeRequestMerge` | `.../merge-requests/{iid}/merge` | `/repos/{id}/pulls/{number}/merge` | merge via `ci-scheduler::merge_queue`. |
| (server) blockers/request-changes/close/reopen/rebase | `.../merge-requests/{iid}/{...}` | `/repos/{id}/pulls/{number}/{blockers,request-changes,close,reopen,rebase}` (rebase optional) | passport / state transitions. |
| (server) pipelines | `/repos/{id}/pipelines[/{pid}[/jobs]]` | `/repos/{id}/actions/runs[/{run_id}[/jobs]]` | `WorkflowRun`. |
| (server) jobs log/retry/cancel | `/repos/{id}/jobs/{job_id}/{log,retry,cancel}` | `/repos/{id}/actions/jobs/{job_id}/{logs,rerun,cancel}` | ci-scheduler. |
| (server) checks?sha | `/repos/{id}/checks?sha=` | `/repos/{id}/commits/{sha}/check-runs` | `CheckRunList`. |
| `issues(id)` | `/repos/{id}/issues` | unchanged path; back with `jeryu-core` issue model (or keep 501 if core defers). |
| `settings` / `settingsPreview` | `/repos/{id}/settings[/preview]` | unchanged | settings service over `jeryu-core` `BranchProtectionRule` etc. |
| `ws()` | `/api/v1/ws` | unchanged | `jeryu.ws.v1` — preserve. |
| `markdownRender()` | `/api/v1/markdown/render` | unchanged | markdown service. |
| `search(q)` | `/api/v1/search` | unchanged | search over `jeryu-core`. |
| `activity()` | `/api/v1/activity` | unchanged | rolling window. |
| auth | `/api/v1/auth/{login,logout}` | unchanged paths | host-neutral provider. |

> **Decision for the implementing agent:** keep the SPA's `endpoints.ts` builder names (`pull*`) emitting GitHub-shaped `/api/v1/repos/{id}/pulls/{number}/...` paths, and serve exactly those in jeryu-api `rest/pulls.rs`/`ci.rs`. This keeps one URL contract and removes any GitLab-derived `/merge-requests/` path.

### 3.3 WS / SSE event-kind rewire (preserve transport, rename kinds)

`jeryu.ws.v1` transport, heartbeat, resume cursor, gap detection, priority channels, `SnapshotRequired`, scope-gated fan-out — **all preserved unchanged**. Rename event *kinds* in `web_events/protocol.rs` + projection + FE invalidators:

| Source kind | Target kind |
|---|---|
| `mr.approved` (High, `protocol.rs:39`) | `pull.approved` (High) |
| `mr.merged` (High, `protocol.rs:40`) | `pull.merged` (High) |
| `mr.diff_recomputed` (FE invalidator, `useMrDiff.ts`) | `pull.diff_recomputed` |
| `mr.thread.*` (FE invalidator, `useMrThreads.ts`) | `pull.thread.*` |
| `pipeline.created` (test ref, `protocol.rs:84`) | `workflow.run.created` |
| `job.log.chunk` / `job.log.annotation` (Low, `protocol.rs:42-43`) | unchanged (host-neutral) |
| `workflow.run.started` / `check.completed` (already emitted by `rest/ci.rs:480-482`) | unchanged — these are already GitHub-shaped. |
| FE subscription scopes `mr.${iid}` (`MergeRequestPage.tsx:108`) | `pull.${number}` |
| `repo.activity`, `repo.settings.changed`, `settings.changed`, `audit.event.created`, `secret.*`, `policy.violation`, `system.health*` | unchanged. |

There is no separate SSE channel in source; the activity dock + notifications consume the same WS stream plus the `/api/v1/activity` rolling buffer (REST polling fallback). Both preserved.

---

## 4. Dependencies & ordering

This subsystem is **downstream of the core rename + GitHub-shaped REST**. Hard prerequisites (Codex-owned), in order:

1. **D2 crate renames complete** — especially `forge-core→jeryu-core` and `jitforge-api→jeryu-api`, with `jeryu-core::{PullRequest, PullRequestState, Review, ReviewComment, CheckRun, CheckRunList, CheckSuite, CheckRunStatus, CheckConclusion, CommitStatus, CombinedStatus, WorkflowRun, WorkflowRunList, BranchProtectionRule}` available (today they live in `crates/forge-core/src/model.rs`).
2. **Persistence ready** — jeryu's SQLite+RedlineDB `db/` layer kept (D3); a repository-of-PRs/checks/reviews read path exists in `jeryu-core` (+ `ci-scheduler` for runs/jobs, `jeryu-gitd` for tree/blob/diff). The BFF's own tables (`sessions`, `web_action_receipts`, `audit_events`) port as-is.
3. **GitHub-shaped REST routes implemented in `jeryu-api`** — currently *deferred* (`API_SURFACE.md:17-20`; `routes.rs:50` returns 404 for `/repos/.../pulls`; the only live routes are `/api/phase10/*`). The web edge cannot function until `jeryu-api` serves `/api/v1/repos/{id}/pulls*`, `/files`(diff), `/reviews`, `/comments`, `/merge`, `/commits/{sha}/check-runs`, `/actions/runs*`, `/actions/jobs/*`. **This is the single biggest blocker.**
4. **`jeryu-proof` (proofcore) passport API** — needed for `MergeGatePanel`/blockers/`expected_passport_hash`.
5. **`ci-scheduler` merge queue + run/job/check status reads** — for merge execution and `ChecksPanel`/CI pages.
6. **`jeryu-obs`** — job-log retrieval + WS metrics.

What this subsystem then delivers (its own ordering):
1. Rename crate dir + `Cargo.toml` (`jitforge-api→jeryu-api`); add `web/` + `web_events/` modules.
2. Port `WebState` swapping `GitLabClient`→`jeryu-core` services; port auth/csrf/sessions/audit/idempotency/telemetry/static/ws verbatim.
3. Port `rest/*` re-pointing to GitHub-shaped routes (§3.2); `merge_requests.rs→pulls.rs`.
4. Regenerate `contracts/generated/*` (ts-rs) with `Pull*` DTOs.
5. Port `web/` SPA, apply token renames (§2.1, §3.1, §3.3), fix the 5 GitLab-literal files (§5).
6. Wire `build_web_router` into the daemon (D3 axum HTTP daemon) alongside the engine compat routes.

**Blocks nothing downstream** except the TUI/CLI subsystems that may reuse the same `jeryu-core` services (independent specs). The React app is a leaf consumer.

---

## 5. Tests / acceptance gate

Run from `/home/ubuntu/jeryuRUST/web/` unless noted. All must pass with the renamed routes/types.

**FE build + type + unit:**
```
cd /home/ubuntu/jeryuRUST/web && npm ci
npm run typecheck          # tsc -b --pretty false — must be clean after Pull* renames
npm run lint               # eslint .
npm run test               # vitest run (component + hook tests; MSW mocks updated to /pulls)
```

**Playwright e2e (no-regression gate — the porting agent must update fixtures to /pulls + jeryu host first):**
```
npm run test:e2e
# critical specs that must stay green:
#   e2e/05-mr-review.spec.ts   → PR review cockpit (rename to 05-pr-review)
#   e2e/06-approve-sha.spec.ts → approve with expected_head_sha drift recovery
#   e2e/08-ws-reconnect.spec.ts→ jeryu.ws.v1 reconnect + resume cursor (UNCHANGED transport)
#   e2e/09-permissions.spec.ts → pr.read/pr.merge perm gating
#   e2e/10-a11y.spec.ts        → axe a11y
```

**Storybook / UX-QA / perf (preserve the existing harness — this is the "tuiwright/Playwright/MCP" analog for web):**
```
npm run build-storybook
npm run ux-qa              # node ../ux-qa/ux-qa-check.mjs build && ... test  (UX-QA five-state gate)
npm run perf               # lhci autorun against perf/lighthouse-budget.json
```

**Backend (jeryu-api web edge):**
```
cargo test -p jeryu-api            # router + rest handler unit tests (idempotency-key, PR list/get/diff/checks)
cargo test -p jeryu-api --test github_api   # the deferred GitHub-compat route tests MUST now assert 200 for
                                            #   /api/v1/repos/{id}/pulls and /pulls/{number} (currently asserts 404)
cargo build -p jeryu-api           # edition 2024, no GitLab deps
```

**Invariants (must hold):**
- `endpoints.ts` is the only place URL strings are built; no inline `/pulls` literals scattered in pages/hooks.
- `jeryu.ws.v1` framing, heartbeat (15s) / read-timeout (30s), exp-backoff reconnect, `resume_from` gap detection, `SnapshotRequired` on lag — byte-identical behavior to source (`08-ws-reconnect` proves it).
- Optimistic concurrency preserved: every mutation sends `Idempotency-Key`; approve/merge send `expected_head_sha` (+ `expected_passport_hash` for merge); 409 `*_stale` renders the recovery banner (`06-approve-sha` proves it).
- Five UX-QA states (loading/empty/error/permission-denied/success) wired on every data page (ux-qa gate).
- CSRF (`X-CSRF-Token` ↔ `__Host-jeryu-csrf`) + session (`__Host-jeryu-session`) cookies unchanged.

**Zero-evidence gate (D1) — must return ZERO matches:**
```
grep -rniE 'gitlab|jitforge|nitro' /home/ubuntu/jeryuRUST/web/src /home/ubuntu/jeryuRUST/web/e2e \
  /home/ubuntu/jeryuRUST/contracts/generated \
  /home/ubuntu/jeryuRUST/crates/jeryu-api/src/web /home/ubuntu/jeryuRUST/crates/jeryu-api/web_events
grep -rniE 'merge[-_ ]?request|/merge-requests|\biid\b' /home/ubuntu/jeryuRUST/web/src   # MR concept fully gone
```
Known source offenders to fix (from `/home/ubuntu/jeryu/apps/web`): `components/NotificationInbox.tsx:193,200`; `components/repo/CreateRepoDialog.tsx:82,265`; `components/repo/RepoCard.stories.tsx`; `components/repo/__tests__/RepoCard.test.tsx`; `pages/SearchResultsPage.tsx:83,87,91`; `pages/RepositoriesPage.tsx:180` (`['gitlab','github','local']` → `['jeryu']`); `hooks/useRepositories.ts:27` (comment); `components/settings/SettingsDiffPreview.{stories,test}`; `components/browser/ReadmePanel.stories.tsx:33`; `layout/CommandPalette.stories.tsx:48`; all `e2e/*.spec.ts` + `e2e/fixtures/mocks.ts` + `e2e/pages/Repositor*.ts` carrying `gitlab`. Backend: `src/web/state.rs:130-137` (`GitlabClient`, env names), `src/web/rest/auth.rs:141` (`gitlab` provider), `src/web/rest/merge_requests.rs:550` + `src/web/rest/ci.rs` (`GitHost::list_pipelines`, `gitlab.example` test URLs), `src/web_events/protocol.rs:39-40,84` (`mr.*`/`pipeline.created`).

---

## 6. Risks & hardest seams

1. **`jeryu-api` GitHub-shaped REST is not implemented yet (only the phase10 facade).** `routes.rs` returns 404 for `/repos/.../pulls`; `tests/github_api.rs` *asserts* that 404. The entire web subsystem is blocked on Codex landing `/api/v1/repos/{id}/pulls*`, `/files`, `/reviews`, `/comments`, `/merge`, `/commits/{sha}/check-runs`, `/actions/runs*`. **Coordinate sequencing — do not start the FE re-point until these return 2xx.**

2. **MR-iid → PR-number type change.** Source uses string `iid` everywhere (route param, query keys, WS scope `mr.${iid}`, idempotency cache keys, recovery-banner SHA plumbing). Target `PullRequest.number` is `u64`. Every layer (router param, `endpoints.ts`, React Query keys in `useMergeRequest/Diff/Checks/Threads`, `selectionStore.currentMr`, WS subscription scope) must change in lockstep or live queries 404.

3. **Diff + threads shape mismatch.** Source `MergeService::diff` produces GitLab-flavored `DiffFile{path,lines_added,lines_removed,hunks:Vec<String>}` (server `rest/merge_requests.rs:117`) while the FE expects richer hunks (`MergeRequestDiffHunk` with `old_start/new_start/...` in `api/types.ts:88`). The target must produce the *FE-rich* shape from `jeryu-gitd` compare output, and review threads must be *synthesized* (group `jeryu_core::ReviewComment` by `path`+`line`) because `jeryu-core` has flat comments, not GitLab discussion threads.

4. **Merge passport / blockers depend on `jeryu-proof` + `ci-scheduler::merge_queue`.** `MergeGatePanel`, `/blockers`, and `expected_passport_hash` optimistic-merge rejection all need the proof passport API and the merge-queue path. If proofcore's passport hash semantics differ from GitLab's, the `merge_passport_stale` 409 recovery flow (`MergeRequestPage.tsx:285-314`) must be re-validated.

5. **Pipeline→WorkflowRun/CheckRun semantic gap.** Source `ChecksPanel`/`/checks` derive "checks" by filtering `list_pipelines` to a SHA (`rest/ci.rs:431`) and report counts (total/passing/failing/pending/skipped). Target should source `CheckRunList`/`CheckSuite` directly and map `CheckConclusion`→FE status strings (`success/failure/pending/skipped/cancelled/neutral`), avoiding the lossy pipeline-as-check proxy. retry/cancel currently only emit an audit event with no host write (`rest/ci.rs:474-477`); target should wire `ci-scheduler` lease rerun/cancel.

6. **ts-rs contract regeneration.** `web/src/api/types.ts` re-exports ~40 generated DTOs from `contracts/generated/`. These regenerate from the renamed Rust types; any DTO that does not regenerate cleanly (e.g. `MergeRequestSummary`→`PullRequestSummary`) breaks the FE typecheck. The FE-local Phase-3 wire types (`api/types.ts:77-164`) must move to generated once the backend emits them.

7. **WS transport must NOT regress while kinds rename.** The temptation is to refactor `JeRyuWsClient`/`ws.rs`; do not. Only event *kind strings* and *scope strings* (`mr.*`→`pull.*`) change. `08-ws-reconnect.spec.ts` is the canary; the bigint seq replacer, sessionStorage resume key (`jeryu.ws.lastSeq.v1`), and `SnapshotRequired` gap path are load-bearing.

8. **Auth provider neutrality.** `rest/auth.rs` hard-codes a `gitlab` OAuth 501 stub and the `local` provider. The `gitlab` branch must become a host-neutral provider (or be removed) without breaking the `JERYU_LOCAL_USERS` local flow that the e2e auth fixture depends on (`e2e/fixtures/auth.ts`).
