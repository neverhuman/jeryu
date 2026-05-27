# JeRyu Web Forge — REST API reference

> **Base URL:** `http://127.0.0.1:8787` (default). Configured via `--bind`
> or `JERYU_WEB_BIND`.
>
> **Version:** all new routes mount under `/api/v1/...`. The engine routes
> `/health`, `/hooks`, and `/cache/summary` are preserved and **not**
> migrated under `/api/v1/`.
>
> **Generated schema:** [`schemas/web-api.openapi.json`](../schemas/web-api.openapi.json)
> (utoipa). DTOs are emitted as TypeScript in
> [`contracts/generated/*.ts`](../contracts/generated/) (ts-rs, 53 DTOs).
>
> **Source plan:** WEB_WORK_CLAUDE.md §35.7 is the canonical route map;
> this document expands each route with examples, error codes, audit
> emission, and WS events.

---

## 1. Conventions

### 1.1 Authentication

Cookie-auth (browser):

```
Cookie: __Host-jeryu-session=<opaque>
Cookie: __Host-jeryu-csrf=<token>
X-CSRF-Token: <token>            # mutating routes
```

Token-auth (CLI / integration):

```
Authorization: Bearer <token>
```

Cookies are `HttpOnly`, `Secure`, `SameSite=Lax`, `Path=/`. The session
secret rolls every 30 days. The CSRF token is double-submit; the value in
the cookie MUST equal the value in the `X-CSRF-Token` header for every
mutating route. Token-auth callers are exempt from CSRF.

### 1.2 Idempotency

Every create / merge / delete / archive / settings / secrets / actions
call requires `Idempotency-Key: <uuid v4>`. The server stores
`(action_kind, target_id, idempotency_key) → result` in
`web_action_receipts` for 24 h.

- Same key + same body → returns the stored result (`200`).
- Same key + different body → `409 idempotency_conflict`.

### 1.3 Optimistic concurrency

- Settings, branch protection, repo PATCH carry
  `If-Match: "<hex-state-hash>"`. Mismatch → `409 settings_hash_stale`.
- Approve/merge carry `expected_head_sha` in the body. Mismatch →
  `409 merge_sha_stale` with the live SHA in `details`.

### 1.4 Error envelope

Every non-success response is JSON with:

```json
{
  "error": {
    "code": "merge_sha_stale",
    "message": "The source branch changed after approval.",
    "details": { "expected": "abc123", "live": "def456" },
    "request_id": "req-7eda-...",
    "event_cursor": 12345
  }
}
```

`event_cursor` is the latest durable `web_events.seq`; clients use it to
realign WS state on error. Canonical error codes (lowercase snake_case):

```
unauthenticated     forbidden          csrf_invalid
not_found           bad_request        validation_failed
conflict            merge_sha_stale    settings_hash_stale
idempotency_replay  idempotency_conflict
rate_limited        upstream_unavailable  upstream_forbidden
subscribe_forbidden  event_gap
internal
```

### 1.5 Permission keys (24-key normalised set)

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

GitLab role mapping is in [`docs/web-forge.md`](web-forge.md#6-host-adapters).

### 1.6 Audit and WS emission

Every mutating route writes one `audit_events` row and one
`web_action_receipts` row in the same transaction, then broadcasts a WS
event on the appropriate scope. The full event vocabulary is in
[`docs/WEBSOCKET_PROTOCOL.md`](WEBSOCKET_PROTOCOL.md) §3.

### 1.7 Path/query conventions

- `{repo_id}` is the opaque stable UUID-shaped string from
  `web_repositories.id`. The SPA shows human paths (`/repos/gitlab/group/sub/project`)
  but calls the BFF with `repo_id`.
- `{iid}` is the merge-request internal id assigned by the host (matches
  GitLab `iid`).
- `cursor` query is opaque base64; `limit` defaults to 50, max 200.

---

## 2. Engine routes (preserved, unchanged)

| Method | Path | Notes |
|---|---|---|
| GET | `/health` | Engine health probe. Unchanged by `--features web`. |
| GET | `/hooks` | Webhook ingress endpoint. |
| GET | `/cache/summary` | Cache statistics. |

```bash
curl http://127.0.0.1:8787/health
# → {"status":"ok",...}
```

---

## 3. Bootstrap and auth

### 3.1 `GET /api/v1/bootstrap` → `WebBootstrap`

**Permissions:** none (returns viewer identity even when unauthenticated).
**Headers:** none (CSRF not required).
**Body:** N/A.
**Response 200:** `WebBootstrap` (see `contracts/generated/WebBootstrap.ts`)
containing `viewer`, `permissions`, `recent_repos`, `snapshot`, `ws_url`,
and `feature_flags`.
**Errors:** `500 internal` on DB unreachable.
**Audit/WS:** none.

```bash
curl -sS \\
  --cookie '__Host-jeryu-session=...' \\
  http://127.0.0.1:8787/api/v1/bootstrap
```

### 3.2 `POST /api/v1/auth/login`

**Permissions:** none (CSRF-exempt).
**Headers:** `Content-Type: application/json`.
**Body:** `{ "username": "...", "password": "...", "next": "/repos" }`.
**Response 200:** `Set-Cookie: __Host-jeryu-session=... __Host-jeryu-csrf=...`; body `{ "viewer": ..., "next": "..." }`.
**Errors:** `401 unauthenticated`, `400 validation_failed` (open redirect rejected when `next` is not a same-origin relative path).
**Audit:** `audit_events { kind: "session.login" }`. **WS:** none.

### 3.3 `POST /api/v1/auth/logout`

**Permissions:** authenticated.
**Headers:** `X-CSRF-Token`.
**Response 204:** `Set-Cookie: __Host-jeryu-session=; Max-Age=0`.
**Audit:** `audit_events { kind: "session.logout" }`.

---

## 4. Repositories

### 4.1 `GET /api/v1/repos` → `RepositoryListResponse`

**Permissions:** `repo.read` (the result is filtered to the viewer's
visible set).
**Query:** `search`, `host`, `owner`, `family`, `include_archived`,
`limit`, `cursor`.
**Response 200:** `{ items: RepositorySummary[], next_cursor: string|null }`.
**Errors:** `400 validation_failed` for malformed cursor.
**WS:** none (subscribe to `global.activity` for live additions).

```bash
curl -sS \\
  --cookie '__Host-jeryu-session=...' \\
  'http://127.0.0.1:8787/api/v1/repos?search=jeryu&limit=20'
```

### 4.2 `POST /api/v1/repos/preview` → `CreateRepositoryPreview`

**Permissions:** `repo.create`.
**Headers:** `X-CSRF-Token`, `Content-Type: application/json`.
**Body:** `CreateRepositoryRequest` (see
`contracts/generated/CreateRepositoryRequest.ts`).
**Response 200:** `CreateRepositoryPreview` — what would change if
executed, including blast-radius warnings.
**Audit:** `action.previewed` (low). **WS:** none.

### 4.3 `POST /api/v1/repos` (create)

**Permissions:** `repo.create`.
**Headers:** `X-CSRF-Token`, `Idempotency-Key`, `Content-Type: application/json`.
**Body:** `CreateRepositoryRequest`.
**Response 201:** `RepositorySummary`.
**Errors:** `403 forbidden`, `409 idempotency_conflict`, `409 conflict` (name in use), `502 upstream_unavailable`.
**Audit:** `audit_events { kind: "repo.created" }`. **WS:** `repo.created` on `global.activity` and on `repo.{repo_id}`.

```bash
curl -sS \\
  --cookie '__Host-jeryu-session=...' \\
  --header 'X-CSRF-Token: abc...' \\
  --header 'Idempotency-Key: 0a9e8b2f-4c0e-4e4d-9f0c-1234567890ab' \\
  --header 'Content-Type: application/json' \\
  --data '{ "host": "gitlab", "namespace": "veox", "name": "new-thing", "visibility": "private" }' \\
  http://127.0.0.1:8787/api/v1/repos
```

### 4.4 `POST /api/v1/repos/import/preview` and `POST /api/v1/repos/import`

**Permissions:** `repo.create`.
**Headers:** as §4.2 / §4.3.
**Body:** `ImportRepositoryRequest` (provider URL + token strategy).
**Response 201:** `RepositorySummary`.
**Errors:** `502 upstream_unavailable`, `403 upstream_forbidden`.
**Audit/WS:** `repo.imported`.

### 4.5 `GET /api/v1/repos/{repo_id}` → `RepositoryDetail`

**Permissions:** `repo.read` (and viewer must see the repo).
**Response 200:** `RepositoryDetail`.
**Errors:** `404 not_found`.

### 4.6 `PATCH /api/v1/repos/{repo_id}`

**Permissions:** `repo.write`.
**Headers:** `X-CSRF-Token`, `Idempotency-Key`, `If-Match`.
**Body:** partial `RepositoryDetail` fields (description, default_branch, visibility).
**Response 200:** `RepositoryDetail`.
**Errors:** `409 settings_hash_stale`, `403 forbidden`.
**Audit/WS:** `repo.updated` on `repo.{repo_id}`.

### 4.7 `POST /api/v1/repos/{repo_id}/archive`

**Permissions:** `repo.admin`.
**Headers:** `X-CSRF-Token`, `Idempotency-Key`.
**Response 200:** `RepositorySummary { archived: true }`.
**Audit/WS:** `repo.archived`.

### 4.8 `DELETE /api/v1/repos/{repo_id}`

**Permissions:** `repo.delete`.
**Headers:** `X-CSRF-Token`, `Idempotency-Key`.
**Response 204.**
**Errors:** `403 forbidden`, `409 conflict` (open MRs).
**Audit/WS:** `repo.deleted`.

---

## 5. Refs, commits, compare

### 5.1 `GET /api/v1/repos/{repo_id}/refs` → `RefSelectorItem[]`

**Permissions:** `code.read`.
**Response 200:** branches + tags, with `kind: "branch" | "tag"`.

### 5.2 `GET /api/v1/repos/{repo_id}/branches` and `POST .../branches`

**Permissions read:** `code.read`. **Permissions create:** `branch.create`.
**Body (POST):** `{ "name": "...", "from_ref": "...", "from_sha": "..." }`.
**Headers (POST):** `X-CSRF-Token`, `Idempotency-Key`.
**Audit/WS:** `repo.branch.created` on `repo.{repo_id}.refs`.

### 5.3 `GET /api/v1/repos/{repo_id}/tags` and `POST .../tags`

**Permissions:** `code.read` / `branch.create` (tags share the branch.create perm).
Body, headers, audit mirror §5.2 with kind=`tag`.

### 5.4 `GET /api/v1/repos/{repo_id}/commits` / `.../commits/{sha}`

**Permissions:** `code.read`.
**Query:** `ref`, `path`, `since`, `until`, `limit`, `cursor`.
**Response 200:** `CommitSummary[]` or `CommitDetail`.

### 5.5 `GET /api/v1/repos/{repo_id}/compare?base=&head=` → `CompareView`

**Permissions:** `code.read`.
**Response 200:** `CompareView { base, head, commits, diff_stats, files_changed }`.

---

## 6. Repo browser (tree, blob, raw, readme)

Path safety rules apply to every endpoint here: reject `..`, leading `/`,
NUL bytes, and backslashes; URL-encode every host call segment.

### 6.1 `GET /api/v1/repos/{repo_id}/tree?ref=&path=` → `TreeEntry[]`

**Permissions:** `code.read`.

### 6.2 `GET /api/v1/repos/{repo_id}/blob?ref=&path=&render=` → `BlobResponse`

**Permissions:** `code.read`.
**Query:** `render=md` to attach `rendered_markdown` (`RenderedMarkdown`).
**Response 200:** `BlobResponse` (`contracts/generated/BlobResponse.ts`) —
includes `mime`, `encoding`, optional `text`, optional `base64`, optional
`rendered_markdown`.
**Errors:** `400 validation_failed` for binary asked-to-render.

### 6.3 `GET /api/v1/repos/{repo_id}/raw?ref=&path=`

**Permissions:** `code.read`.
**Response 200:** raw bytes with `Content-Type` from `mime_guess`;
`Content-Disposition: attachment; filename=…` for non-text MIME.

### 6.4 `GET /api/v1/repos/{repo_id}/readme?ref=` → `BlobResponse`

**Permissions:** `code.read`.
**Lookup order:** `README.md`, `README.markdown`, `README.mdown`,
`README.txt`, then case-insensitive variants of each (§35.1.7). `README.rst`
is **download-only** in v1.
**Response 200:** `BlobResponse` with `rendered_markdown` attached.

### 6.5 `GET /api/v1/repos/{repo_id}/history?ref=&path=`

**Permissions:** `code.read`.
**Response 200:** `CommitSummary[]` for the given path.

### 6.6 `GET /api/v1/repos/{repo_id}/blame?ref=&path=`

**Permissions:** `code.read`.
**Response 200:** `BlameLine[]`.

### 6.7 `POST /api/v1/markdown/render`

**Permissions:** authenticated (no specific key).
**Headers:** `X-CSRF-Token`, `Content-Type: application/json`.
**Body:** `{ "markdown": "...", "context": { "repo_id": "...", "ref": "main" } }`.
**Response 200:** `{ "html": "...", "renderer_version": "jeryu-md-renderer.v1", "sanitizer_version": "jeryu-md-sanitizer.v1" }`.
**Errors:** `413` if `markdown` exceeds 1 MiB.
**Audit/WS:** none (read-only).

---

## 7. Issues (v1.5 stub)

`POST /api/v1/repos/{repo_id}/issues` returns `501 not_implemented` in
v1; GET endpoints return the cached read-model where available.

| Method | Path |
|---|---|
| GET | `/api/v1/repos/{repo_id}/issues` |
| POST | `/api/v1/repos/{repo_id}/issues` *(501 v1)* |
| GET | `/api/v1/repos/{repo_id}/issues/{iid}` |
| PATCH | `/api/v1/repos/{repo_id}/issues/{iid}` *(501 v1)* |

---

## 8. Merge requests

### 8.1 `GET /api/v1/repos/{repo_id}/merge-requests?state=`

**Permissions:** `mr.read`.
**Response 200:** `MergeRequestSummary[]`.

### 8.2 `POST /api/v1/repos/{repo_id}/merge-requests` (create)

**Permissions:** `mr.write`.
**Headers:** `X-CSRF-Token`, `Idempotency-Key`.
**Body:** `CreateMergeRequest` (source ref, target ref, title, description).
**Audit/WS:** `mr.created` on `repo.{repo_id}.merge_requests` and `mr.{mr_id}`.

### 8.3 `GET /api/v1/repos/{repo_id}/merge-requests/{iid}` → `MergeRequestDetail`

**Permissions:** `mr.read`.

### 8.4 `GET .../diff`, `/checks`, `/blockers`, `/threads`

**Permissions:** `mr.read`.
Each returns the corresponding read-model surface. `/blockers` returns
the live Merge Passport with one entry per blocked gate.

### 8.5 `POST .../threads` and `PATCH .../threads/{thread_id}`

**Permissions:** `mr.comment` (create) / `mr.write` (resolve).
**Headers:** `X-CSRF-Token`.
**Audit/WS:** `mr.thread.created` / `mr.thread.resolved` on `mr.{mr_id}`.

### 8.6 `POST .../comments`

**Permissions:** `mr.comment`.
**Headers:** `X-CSRF-Token`.

### 8.7 `POST .../reviews`

**Permissions:** `mr.review`.
**Body:** `{ "verdict": "approve" | "request_changes" | "comment", "summary": "..." }`.
**Audit/WS:** `mr.review.submitted` on `mr.{mr_id}`.

### 8.8 `POST .../approve` (exact-SHA)

**Permissions:** `mr.approve`.
**Headers:** `X-CSRF-Token`, `Idempotency-Key`, `Content-Type: application/json`.
**Body:** `{ "expected_head_sha": "abc123..." }`.
**Response 200:** updated `MergeRequestDetail`.
**Errors:** `409 merge_sha_stale` with `details: { expected, live }`.
**Audit/WS:** `mr.approved` on `mr.{mr_id}` (high priority).

```bash
curl -sS \\
  --cookie '__Host-jeryu-session=...' \\
  --header 'X-CSRF-Token: ...' \\
  --header 'Idempotency-Key: 8b4f...' \\
  --header 'Content-Type: application/json' \\
  --data '{ "expected_head_sha": "abc123def456" }' \\
  http://127.0.0.1:8787/api/v1/repos/repo-uuid/merge-requests/42/approve
```

### 8.9 `POST .../request-changes`

**Permissions:** `mr.review`.
**Audit/WS:** `mr.review.submitted` (with `verdict: "request_changes"`).

### 8.10 `POST .../merge` (exact-SHA + Passport)

**Permissions:** `mr.merge`.
**Headers:** `X-CSRF-Token`, `Idempotency-Key`.
**Body:** `{ "expected_head_sha": "abc...", "method": "merge" | "squash" | "rebase", "delete_source_branch": false }`.
**Response 200:** updated `MergeRequestDetail` with `state: "merged"`.
**Errors:**
- `409 merge_sha_stale` — source changed after preview.
- `409 conflict` with `details.code = "passport_blocked_<gate>"` —
  Merge Passport gates failed (see [`docs/REVIEW_COCKPIT.md`](REVIEW_COCKPIT.md) §2).
**Audit/WS:** `mr.merged` on `mr.{mr_id}` and `repo.{repo_id}.merge_requests` (high priority).

### 8.11 `POST .../rebase`

**Permissions:** `mr.merge` (rebase is a write on the source branch).
**Headers:** `X-CSRF-Token`, `Idempotency-Key`.

### 8.12 `POST .../close` and `POST .../reopen`

**Permissions:** `mr.write`.
**Audit/WS:** `mr.updated` with `state: "closed"` / `"opened"`.

---

## 9. CI (pipelines, jobs)

| Method | Path | Perm |
|---|---|---|
| GET | `/api/v1/repos/{repo_id}/pipelines` | `ci.read` |
| GET | `/api/v1/repos/{repo_id}/pipelines/{pipeline_id}` | `ci.read` |
| GET | `/api/v1/repos/{repo_id}/jobs/{job_id}/log` | `ci.read` |
| POST | `/api/v1/repos/{repo_id}/jobs/{job_id}/retry` | `ci.write` |
| POST | `/api/v1/repos/{repo_id}/jobs/{job_id}/cancel` | `ci.write` |

`POST retry` requires `Idempotency-Key`. **Audit/WS:** `workflow.run.started`,
`workflow.run.completed`, `check.started`, `check.completed`, `job.log.chunk`
(low priority).

---

## 10. Settings, members, protection, secrets

### 10.1 Settings

| Method | Path | Perm |
|---|---|---|
| GET | `/api/v1/repos/{repo_id}/settings` | `settings.read` |
| POST | `/api/v1/repos/{repo_id}/settings/preview` | `settings.write` |
| PATCH | `/api/v1/repos/{repo_id}/settings` | `settings.write` |

`PATCH` requires `Idempotency-Key` + `If-Match`. Body is partial
`RepositorySettings` (see `contracts/generated/RepositorySettings.ts`).
**Errors:** `409 settings_hash_stale` with `details.live_hash`.
**Audit/WS:** `repo.settings.changed` on `repo.{repo_id}.settings` (high priority).

```bash
curl -sS \\
  --cookie '__Host-jeryu-session=...' \\
  --header 'X-CSRF-Token: ...' \\
  --header 'Idempotency-Key: ...' \\
  --header 'If-Match: "9af1c2..."' \\
  --header 'Content-Type: application/json' \\
  --data '{ "general": { "description": "new desc" } }' \\
  --request PATCH \\
  http://127.0.0.1:8787/api/v1/repos/repo-uuid/settings
```

### 10.2 Members

| Method | Path | Perm |
|---|---|---|
| GET | `/api/v1/repos/{repo_id}/members` | `settings.read` |
| PUT | `/api/v1/repos/{repo_id}/members/{principal_id}` | `settings.write` |
| DELETE | `/api/v1/repos/{repo_id}/members/{principal_id}` | `settings.write` |

PUT/DELETE require `Idempotency-Key`. Audit/WS: `repo.settings.changed`.

### 10.3 Branch protection

| Method | Path | Perm |
|---|---|---|
| GET | `/api/v1/repos/{repo_id}/protection` | `settings.read` |
| PATCH | `/api/v1/repos/{repo_id}/protection` | `settings.write` |

PATCH requires `Idempotency-Key` + `If-Match`. Audit/WS:
`repo.branch.protection.changed`.

### 10.4 Secrets

| Method | Path | Perm |
|---|---|---|
| GET | `/api/v1/repos/{repo_id}/secrets` | `secrets.read_metadata` |
| POST | `/api/v1/repos/{repo_id}/secrets` | `secrets.write` |
| POST | `/api/v1/repos/{repo_id}/secrets/{secret_name}/rotate` | `secrets.write` |
| DELETE | `/api/v1/repos/{repo_id}/secrets/{secret_name}` | `secrets.write` |

GET returns metadata only — values are never returned after write. All
mutations require `Idempotency-Key`. Audit/WS: `audit.event.created` with
`kind: "secret.{created,rotated,deleted}"` (high priority; the secret
value is never logged).

---

## 11. Generic actions, activity, search, WebSocket

### 11.1 `POST /api/v1/actions/preview` and `POST /api/v1/actions/execute`

Generic command-palette execution surface. `execute` requires
`Idempotency-Key`. Both bodies are `{ action_id: "...", params: { ... } }`.

### 11.2 `GET /api/v1/activity?since=&limit=&scope=`

**Permissions:** scope-dependent (e.g. `repo.read` for `scope=repo.{id}`).
**Response 200:** durable event tail; useful for offline catch-up.

### 11.3 `GET /api/v1/search?q=&kinds=&limit=`

**Permissions:** results are filtered to the viewer's visible set.
**Query `kinds`:** comma-separated subset of `repo,mr,issue,commit,file`.

### 11.4 `GET /api/v1/ws` (WebSocket upgrade)

See [`docs/WEBSOCKET_PROTOCOL.md`](WEBSOCKET_PROTOCOL.md).

---

## 12. Status code matrix

| Status | Codes |
|---:|---|
| 200 | success (idempotent reads, idempotent replays) |
| 201 | resource created |
| 204 | success, no body (delete/logout) |
| 400 | `bad_request`, `validation_failed` |
| 401 | `unauthenticated`, `csrf_invalid` |
| 403 | `forbidden`, `upstream_forbidden` |
| 404 | `not_found` |
| 409 | `conflict`, `merge_sha_stale`, `settings_hash_stale`, `idempotency_conflict` |
| 413 | `validation_failed` (oversize body, e.g. markdown >1 MiB) |
| 429 | `rate_limited` |
| 500 | `internal` |
| 501 | `not_implemented` (Issues v1.5 stubs) |
| 502 | `upstream_unavailable` |

---

## 13. Headers used on mutating routes

- `Content-Type: application/json`
- `X-CSRF-Token: <cookie value>` (cookie-auth mode)
- `Authorization: Bearer <token>` (token-auth mode; alternate to CSRF)
- `Idempotency-Key: <uuid v4>` (per §1.2)
- `If-Match: "<hex-state-hash>"` (settings / protection / repo PATCH)

Response headers:

- `X-Request-Id: <uuid v7>` (mirrored into the error envelope as `request_id`)
- `Strict-Transport-Security: max-age=31536000; includeSubDomains`
- `Content-Security-Policy: …` (see [`docs/web-forge.md`](web-forge.md#42-tower-middleware-stack-outer--inner))
- `X-Content-Type-Options: nosniff`
- `Referrer-Policy: strict-origin-when-cross-origin`
