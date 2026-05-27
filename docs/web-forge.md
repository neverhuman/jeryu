# JeRyu Web Forge — Architecture

> **Status:** v1.0 (Phase 0–7 in flight). This document is the canonical
> architecture reference for the JeRyu Web Forge — the Vite + React + TypeScript
> SPA that ships alongside the `jeryu` binary and exposes the JeRyu mission
> control surface to a browser.
>
> **Audience:** operators deploying the binary, and contributors building or
> reviewing pages, services, or host adapters.
>
> **Companion docs**
>
> - REST: [`docs/WEB_API.md`](WEB_API.md)
> - WebSocket: [`docs/WEBSOCKET_PROTOCOL.md`](WEBSOCKET_PROTOCOL.md)
> - Markdown rendering and XSS posture: [`docs/README_RENDERING.md`](README_RENDERING.md)
> - Merge cockpit: [`docs/REVIEW_COCKPIT.md`](REVIEW_COCKPIT.md)
> - Frontend development guide: [`apps/web/README.md`](../apps/web/README.md)
> - Source plan (authoritative): [`WEB_WORK_CLAUDE.md`](../WEB_WORK_CLAUDE.md)

---

## 1. Overview

The Web Forge is a "GitHub/GitLab-class" experience powered by the existing
JeRyu binary. It lets an operator browse all configured repos, render
README/Markdown safely, navigate code and branches, manage merge requests with
exact-SHA approve/merge guarantees, change settings with blast-radius preview,
and watch live activity stream over a single WebSocket connection.

Three top-level design choices govern everything that follows:

1. **Single-binary deployment.** `jeryu web serve` boots an Axum BFF inside
   the existing `jeryu` process. The browser never speaks to the internal
   GitLab directly; the BFF owns credentials, caches, and host calls.
2. **Live by default.** Bootstrap delivers a snapshot in one round-trip; from
   that moment on the SPA mostly listens to WebSocket events on
   `GET /api/v1/ws` (`jeryu.ws.v1`) and only polls as a fallback.
3. **Safety binds to identity, not state.** Mutating endpoints require a
   normalised permission key, a CSRF token (cookie auth), an
   `Idempotency-Key` header, and either an `expected_state_hash` (settings,
   protection) or `expected_head_sha` (merge/approve) for optimistic
   concurrency. Everything mutating writes an audit row and a
   `web_action_receipts` row in the same transaction.

For the broader product vision (one Merge Passport, one command palette, one
attention dashboard, one renderer) see WEB_WORK_CLAUDE.md §1.

---

## 2. Target tree

Verbatim from WEB_WORK_CLAUDE.md §2.4. New paths use `[NEW]`; modified are
`[MOD]`; preserved as-is are `[KEEP]`.

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

`apps/web` was renamed from the legacy UX-QA placeholder to `apps/ux-qa`;
the new `@jeryu/web` workspace owns the SPA. See W-F-12 in the plan.

---

## 3. Data flow

```
Bootstrap                                   REST                                WebSocket
────────                                    ────                                ─────────

browser                                      browser                             browser
  │                                            │                                   │
  │  GET /api/v1/bootstrap                     │  GET  /api/v1/repos/…             │ GET /api/v1/ws  (Upgrade)
  │  (cookie + CSRF)                           │  PATCH /api/v1/repos/…/settings   │   ───────────────►
  │  ─────────────────►                        │  POST /api/v1/markdown/render     │       │
  │                                            │   ───────────────►                │   Hello {resume_from,
  │  ◄───────────────                          │                                   │           subscriptions[]}
  │  WebBootstrap {                            │  ◄───────────────                 │       │
  │    viewer, perms,                          │  RepositoryListResponse           │   ◄───────────────
  │    recent_repos,                           │  RepositorySettings               │   Hello {current_seq,
  │    snapshot, ws_url,                       │  RenderedMarkdown …               │           protocol:
  │    feature_flags }                         │                                   │           "jeryu.ws.v1"}
  │                                            │                                   │       │
  ▼                                            ▼                                   ▼   Event {seq, scope,
React Query cache primed              React Query refetches on              kind, payload, priority}
React Router resolves first route     focus / WS event invalidation         flowing while route open
```

1. **Bootstrap (cold load).** Browser issues `GET /api/v1/bootstrap`. The BFF
   returns viewer identity, normalised permissions, recent repos, the TUI
   read-model snapshot, the WS URL, and feature flags. One round-trip to
   first useful paint (<1.5 s on local).
2. **WebSocket connect.** `GET /api/v1/ws` upgrades; the client sends
   `Hello { resume_from, subscriptions }`. The server replies `Hello {
   current_seq, protocol: "jeryu.ws.v1" }` and streams `Event` frames in
   monotonically increasing `seq` order.
3. **Route navigation.** React Router resolves the URL; React Query fires
   the relevant REST query; the WS subscription set is adjusted to the
   minimum scope vocabulary for the route.
4. **Mutation.** UI shows a preview via `POST …/preview` (medium/high-risk
   actions) or `POST /api/v1/actions/preview` (generic). Execute calls
   carry `Idempotency-Key` plus, where relevant, `expected_state_hash`
   (settings/protection) or `expected_head_sha` (merge/approve). The
   handler runs the 14-step action-safety algorithm (§5) and broadcasts a
   high-priority WS event on success.

---

## 4. BFF architecture (Axum + tower)

The BFF binds Axum at `127.0.0.1:8787` (configurable via
`--bind`/`JERYU_WEB_BIND`). New routes mount under `/api/v1/...`. **The
engine routes `/health`, `/hooks`, and `/cache/summary` are preserved
exactly** — the web router merges them into the existing engine router
without modifying their handlers.

### 4.1 Module layout (`src/web/`)

| Module | Responsibility |
|---|---|
| `mod.rs` | Re-exports; module wiring. |
| `command.rs` | `jeryu web serve …` CLI subcommand and option parsing. |
| `state.rs` | `AppState`: handles to services + caches injected into every handler. |
| `router.rs` | Builds the Axum `Router`, mounts middleware, merges engine routes. |
| `error.rs` | `ApiError` enum + `IntoResponse` impl emitting the structured envelope (§35.1.11). |
| `auth.rs` | `__Host-jeryu-session` cookie validation; bearer-token alternate path. |
| `csrf.rs` | Double-submit token check via `X-CSRF-Token` header against `__Host-jeryu-csrf` cookie. |
| `idempotency.rs` | Middleware reading the `Idempotency-Key` header; stores/replays via `web_action_receipts`. |
| `static_assets.rs` | Production: serves `apps/web/dist`. Dev: proxies to `127.0.0.1:5173`. |
| `markdown.rs` | Thin REST adapter for `repo_browser::markdown` (the renderer/cache lives in the domain crate). |
| `ws.rs` | WebSocket upgrade + frame loop; bridges to `web_events::bus`. |
| `rest/*.rs` | One file per surface (bootstrap, repos, repo_browser, merge_requests, reviews, issues, settings, actions, search, ci, agents, activity). |

### 4.2 Tower middleware stack (outer → inner)

```
Trace             tower_http::trace                  request_id, latency, status
Compression       tower_http::compression            gzip + br for text/* and JSON
RequestId         tower_http::request_id             X-Request-Id (uuid v7)
SetHeader         tower_http::set_header             CSP, HSTS, X-Content-Type-Options, Referrer-Policy
Cors              tower_http::cors                   refuses '*' in production
Timeout           tower_http::timeout                30 s for REST, ∞ for WS
Auth              src/web/auth.rs                    cookie or bearer
Csrf              src/web/csrf.rs                    mutating + cookie-auth only
Idempotency       src/web/idempotency.rs             reads `Idempotency-Key` and replays
```

CSP baseline (W-CC-09):

```
Content-Security-Policy: default-src 'self'; script-src 'self';
  style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:;
  connect-src 'self' wss:; font-src 'self' data:; frame-ancestors 'none';
  base-uri 'self'; form-action 'self'
```

`'unsafe-inline'` on `style-src` is a deliberate v1 concession; tighten to
nonce-based in v1.5.

### 4.3 Boundaries

- The web bundle **must not** import `sqlx`, `mysql`, `@aws-sdk/client-s3`,
  or any other DB/cloud driver (enforced by `agent/boundaries.toml`).
- Rust domain code (under `src/<domain>/`) **must not** import
  `std::{fs,env,net,time::SystemTime}`, `rand`, `sqlx`, `diesel`,
  `reqwest`, `jansu`, `tracing`, or `log`. Domain code uses injected
  ports.
- `src/api/` remains the single typed-contract source of truth; the BFF
  never invents ad-hoc shapes. Each DTO is `#[derive(TS, ToSchema,
  JsonSchema)]` so `ts-rs` emits `contracts/generated/*.ts` (53 DTOs),
  `utoipa` emits `schemas/web-api.openapi.json`, and `schemars` emits
  `schemas/websocket-events.schema.json`.

---

## 5. The 14-step action algorithm

Every mutating handler runs the same canonical algorithm (WEB_WORK_CLAUDE.md
§35.1.14). Steps 12 and 13 are atomic per mutation (same transaction).

```
1.  authenticate
2.  resolve viewer
3.  resolve target
4.  check normalized permission
5.  validate CSRF (cookie auth) OR bearer (token auth)
6.  validate schema (Zod / serde)
7.  load current state
8.  validate expected_state_hash (settings)
    OR expected_sha (merge / approve)
9.  produce preview for medium / high-risk actions
10. require Idempotency-Key header (create / merge / delete / archive /
    settings / secrets)
11. execute provider call OR local state change
12. write audit receipt
13. write durable web event row (seq, scope, kind, payload)
14. broadcast WebSocket event; return updated read model OR receipt
```

The `web_action_receipts` table enforces the unique constraint
`(action_kind, target_id, idempotency_key)` so replays are idempotent at
the DB layer; replay with the same key + same body returns `200` with the
stored result, replay with the same key + different body returns
`409 idempotency_conflict`. TTL is 24 h; nightly `DELETE … WHERE
created_at < now - 24h`.

---

## 6. Host adapters

The `src/git_host/` module exposes a `GitHost` trait. v1 ships:

| Adapter | Status | Notes |
|---|---|---|
| `gitlab.rs` | First-class | Talks to the internal GitLab via `GitlabClient` using `JERYU_GITLAB_BASE_URL` + `JERYU_GITLAB_TOKEN`. Supports conditional `If-None-Match` GETs via `provider_etag` on `web_repositories`. Nested groups (`group/subgroup/project`) are addressed by opaque `repo_id` rather than 3-segment paths. |
| `github.rs` | Stub | Returns `NotImplemented` from every mutating method; read paths return empty lists where safe. Full parity is **v1.5** (W-H-07). |

The opaque `repo_id` is a UUID-shaped string persisted in
`web_repositories.id`; the `RepositorySummary` DTO surfaces both `id:
RepositoryId` and `full_name: String` so the SPA shows pretty URLs while
calling the API with the stable id (§35.1.2).

GitLab role mapping (W-CC-07′, §35.1.18) — the 24-key normalised
permission set:

- `guest` → `repo.read`, `code.read`, `mr.read`, `mr.comment`, `issue.read`, `ci.read`.
- `reporter` → guest + `mr.review`, `secrets.read_metadata`, `audit.read`.
- `developer` → reporter + `code.write`, `branch.create`, `mr.write`, `ci.write`, `agents.read`, `issue.write`.
- `maintainer` → developer + `repo.write`, `branch.delete`, `settings.write`, `mr.approve`, `mr.merge`, `agents.write`.
- `owner` → maintainer + `repo.admin`, `repo.delete`, `repo.create`, `secrets.write`, `agents.grant`, `admin.audit`.

---

## 7. Event bus

`src/web_events::bus` is a tokio broadcast hub with **priority-class
routing**:

| Priority | Capacity | Drop policy | Examples |
|---|---:|---|---|
| **High** | 4096 | Never dropped while the channel has slots. Receivers that lag are disconnected and forced to reconnect via `snapshot_required`. | Action results, audit/security events, `mr.approved`, `mr.merged`, `settings.changed`. |
| **Medium** | 4096 | Same channel as high. | Posture changes, check completions, CI run lifecycle. |
| **Low** | 1024 | Best-effort; dropped first under pressure. | Per-event `job.log.chunk` spam, low-importance activity. |

Each `Event` carries `seq: u64` (monotonic, durable), `scope: WsScope`,
`kind: WsEventKind`, `payload: serde_json::Value`, and `priority: high |
medium | low`. The full vocabulary is in
[`docs/WEBSOCKET_PROTOCOL.md`](WEBSOCKET_PROTOCOL.md) §3 and the schema is
exported at `schemas/websocket-events.schema.json`.

Scope vocabulary (§35.1.15):

```
global.activity                   system.health
user.{user_id}.notifications
repo.{repo_id}                    repo.{repo_id}.activity
repo.{repo_id}.refs               repo.{repo_id}.checks
repo.{repo_id}.settings           repo.{repo_id}.issues
repo.{repo_id}.merge_requests
mr.{mr_id}                        issue.{issue_id}
agent.{agent_id}                  runner.{runner_id}
cache.{repo_id}
```

The frontend subscribes only to the minimum scopes for its current route.
The dashboard subscribes to `global.activity` + `system.health` + the
viewer-specific notification scope; a repo overview adds `repo.{id}` +
`repo.{id}.activity`; the merge cockpit adds `mr.{id}`.

---

## 8. Markdown renderer and cache

Markdown is rendered server-side by `repo_browser::markdown` and
re-sanitized client-side by DOMPurify before mounting. The renderer is
`jeryu-md-renderer.v1` (pulldown-cmark options); the sanitizer is
`jeryu-md-sanitizer.v1` (ammonia allow-list).

The cache key is

```
(repo_id, ref_sha, path, blob_sha, renderer_version, sanitizer_version)
```

so we can bump `ammonia` policy without bumping the parser version (or
vice versa). Both versions are public Rust constants:

```rust
pub const RENDERER_VERSION:  &str = "jeryu-md-renderer.v1";
pub const SANITIZER_VERSION: &str = "jeryu-md-sanitizer.v1";
```

The `web_markdown_cache` table primary key is `(repo_id, commit_sha, path,
renderer_version, sanitizer_version)`. Cache-hit latency is <25 ms; cold
render (README-sized) is <150 ms typical. Hard input cap is 1 MiB (over
that the renderer returns `413` with code `validation_failed`).

The full XSS posture, allow-list, and link-rewriting rules are in
[`docs/README_RENDERING.md`](README_RENDERING.md).

---

## 9. Security model

| Surface | Mechanism | Source |
|---|---|---|
| Identity | `__Host-jeryu-session` cookie (HttpOnly, Secure, SameSite=Lax, Path=/), 30-day rolling. Bearer-token alternate path for non-browser callers. | `src/web/auth.rs` |
| CSRF | Double-submit token; `X-CSRF-Token` header must match `__Host-jeryu-csrf` cookie on every mutating route (cookie auth only). | `src/web/csrf.rs` |
| Idempotency | `Idempotency-Key` header on every create/merge/delete/archive/settings/secrets/actions call. Stored in `web_action_receipts`. TTL 24 h. | `src/web/idempotency.rs` |
| Exact-SHA | Approve/merge handlers refetch live source/target/policy SHAs before acting; mismatch → `409 merge_sha_stale` with the live SHAs in `details`. | `src/merge/guards.rs` |
| Optimistic concurrency | `If-Match: "<hex-state-hash>"` on settings/protection patches; mismatch → `409 settings_hash_stale`. | `src/web/rest/settings.rs` |
| Permissions | 24-key normalised set checked per request; UI hiding is convenience, server is the source of truth. | `src/repos/permissions.rs` |
| Markdown | Server `ammonia` allow-list → client `DOMPurify` rebox. CSP forbids inline scripts. | `src/repo_browser/markdown.rs` + `apps/web/src/components/Markdown.tsx` |
| Errors | Structured envelope `{ error: { code, message, details, request_id, event_cursor } }`. The `event_cursor` lets the client realign WS state on error. | `src/web/error.rs` |
| Audit | Every mutation writes an `audit_events` row (human-facing) and a `web_action_receipts` row (machine-facing, with hashes for forensic replay). Atomic per mutation. | DB transaction in each handler. |
| WS auth | Connection-level auth on upgrade + per-`Subscribe` permission re-check. Unauthorized scopes are silently dropped and `Error { code: "subscribe_forbidden" }` is sent. | `src/web/ws.rs` + `src/web_events::subscription` |

Canonical error codes (lowercase snake_case):

```
unauthenticated     forbidden          csrf_invalid
not_found           bad_request        validation_failed
conflict            merge_sha_stale    settings_hash_stale
idempotency_replay  idempotency_conflict
rate_limited        upstream_unavailable  upstream_forbidden
subscribe_forbidden  event_gap
internal
```

---

## 10. Local development

### 10.1 Backend

```bash
# From the repo root
cargo run -p jeryu --features web -- web serve \\
  --bind 127.0.0.1:8787 \\
  --dev-assets http://127.0.0.1:5173

# In a second terminal
cd apps/web
npm ci
npm run dev          # vite on http://127.0.0.1:5173
```

The Vite dev server proxies `/api/*`, `/health`, `/hooks`, `/cache/summary`,
and `/api/ws` to `127.0.0.1:8787` via `vite.config.ts`. When the BFF runs
with `--dev-assets`, navigating directly to `127.0.0.1:8787` reverse-proxies
the Vite bundle so a single origin is preserved (cookies + CSP).

Environment (`apps/web/.env.development` and the BFF):

```
JERYU_GITLAB_BASE_URL=https://gitlab.veox.internal
JERYU_GITLAB_TOKEN=<token>
JERYU_WEB_SESSION_SECRET=<32-hex>
JERYU_WEB_PUBLIC_URL=http://127.0.0.1:8787
JERYU_DB_PATH=/var/lib/jeryu/jeryu.db
JERYU_BACKEND_PROFILE=mock   # optional: seeds 5 repos for offline dev
RUST_LOG=info,jeryu=debug
```

### 10.2 Frontend

```bash
cd apps/web
npm run dev               # vite dev server
npm run typecheck         # tsc -b --pretty false
npm run lint              # eslint .
npm run test              # vitest run
npm run test:e2e          # playwright test
npm run storybook         # http://127.0.0.1:6006
npm run build             # tsc -b && vite build → apps/web/dist
```

The frontend reads contracts from `contracts/generated/*.ts` (ts-rs output;
re-generated by `cargo run --bin jeryu_export_types`). Schemas live at
`schemas/web-api.openapi.json` and `schemas/websocket-events.schema.json`
(re-generated by `cargo run --bin jeryu_export_schemas`). Both bins are
registered in `agent/generated-zones.toml` so CI re-runs and fails on drift.

---

## 11. Deployment

The web bundle ships inside the existing `jeryu` binary; there is no
separate web server. The reference deployments are:

### 11.1 systemd

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

### 11.2 nginx (reverse proxy with WS pass-through)

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

### 11.3 Docker

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

See WEB_WORK_CLAUDE.md §26 for the full deployment reference (backup
schedule, env matrix, alternate proxies).

---

## 12. Troubleshooting

The full operator runbook (17 common incidents with first-check and fix
columns) lives in WEB_WORK_CLAUDE.md §27. A short orientation:

- **`/api/v1/bootstrap` 500** — tail logs for `request_id`; usually DB
  unreachable. Check `JERYU_DB_PATH` is writable.
- **WebSocket disconnects every ~30 s** — nginx `proxy_read_timeout`; raise
  to 3600 s.
- **README shows escaped HTML** — `--features web` not enabled, or
  `WebFeatureFlags.markdown_html=false`. Re-run with feature flag set.
- **403 for a known-valid user** — perm mapping drift. Inspect
  `src/repos/permissions.rs` for the role-key mapping change.
- **HTTPS works but WS fails** — confirm the nginx `location /api/ws`
  block in §11.2 is applied with the `Upgrade`/`Connection` headers.
- **GitLab "project not found"** — token lacks scope. `read_repository`
  is the minimum; `write_repository` is needed for mutations.

For incidents outside the runbook, capture `/metrics` (Prometheus scrape;
`ws_events_published_total`, `ws_subscriptions`,
`markdown_render_cache_*`), the SPA error envelope (carries `request_id`
+ `event_cursor`), and the matching `request_id` in `web.log`.

---

## 13. Reference index

| Topic | Section in this doc | Source plan |
|---|---|---|
| REST surface | §3, §4.1 | WEB_WORK_CLAUDE.md §35.7, `schemas/web-api.openapi.json`, `docs/WEB_API.md` |
| WS protocol | §3, §7 | WEB_WORK_CLAUDE.md §16, §35.1.6, `docs/WEBSOCKET_PROTOCOL.md` |
| Markdown security | §8 | WEB_WORK_CLAUDE.md §35.1.4, §35.3.5, `docs/README_RENDERING.md`, `tests/web_markdown_tests.rs` |
| Merge passport | §5 | WEB_WORK_CLAUDE.md §35.2.4, `docs/REVIEW_COCKPIT.md` |
| Frontend layout | §4 | `apps/web/README.md` (W-D-06) |
| Performance budgets | — | WEB_WORK_CLAUDE.md §18 |
| Release roadmap | — | [`ROADMAP.md`](../ROADMAP.md) |
