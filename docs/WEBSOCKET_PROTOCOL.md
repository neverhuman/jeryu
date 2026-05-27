# JeRyu Web Forge — WebSocket protocol

> **Endpoint:** `GET /api/v1/ws` (Upgrade required).
>
> **Protocol identifier:** `jeryu.ws.v1` (server announces it in the
> `Hello` frame).
>
> **Schema:** [`schemas/websocket-events.schema.json`](../schemas/websocket-events.schema.json)
> (schemars).
>
> **TS types:** [`contracts/generated/ClientWsMessage.ts`](../contracts/generated/ClientWsMessage.ts),
> [`contracts/generated/ServerWsMessage.ts`](../contracts/generated/ServerWsMessage.ts),
> [`contracts/generated/WebEvent.ts`](../contracts/generated/WebEvent.ts).
>
> **Source plan:** WEB_WORK_CLAUDE.md §16 Appendix B (event kinds),
> §35.1.6 (per-scope perm checks), §35.1.12 (heartbeat), §35.1.13
> (backpressure / priority classes), §35.1.15 (scope vocabulary).

---

## 1. Upgrade

The browser opens the connection with a standard WebSocket upgrade against
the BFF:

```
GET /api/v1/ws HTTP/1.1
Host: jeryu.veox.internal
Cookie: __Host-jeryu-session=...
Cookie: __Host-jeryu-csrf=...
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Version: 13
Sec-WebSocket-Key: ...
```

Authentication runs on the upgrade request: the BFF resolves the viewer
from the session cookie (or `Authorization: Bearer …`) before
acknowledging the upgrade. An unauthenticated upgrade returns `401
unauthenticated` with the standard error envelope; the WS connection
never opens.

`Authorization: Bearer` callers can also pass `?access_token=<token>` as
a query parameter when the calling client cannot set custom headers on
the upgrade.

CSRF is **not** required on the upgrade because cookies are
`SameSite=Lax`; the BFF still re-checks the cookie's CSRF binding on
every mutating REST call.

---

## 2. Frames

All frames are JSON text frames. The envelope discriminator is a `type`
field. Both directions use the same discriminator vocabulary; the schema
emits two `oneOf` branches (`ClientWsMessage`, `ServerWsMessage`) so
TypeScript clients get exhaustive type narrowing.

### 2.1 Client frames

#### `hello`

```json
{
  "type": "hello",
  "resume_from": 12340,
  "subscriptions": [
    { "scope": "global.activity", "filters": {} },
    { "scope": "user.alice.notifications", "filters": {} }
  ]
}
```

- `resume_from` is the last `event.seq` the client observed. `null` on
  cold start.
- `subscriptions` is the initial scope set; can be amended later with
  `subscribe`/`unsubscribe`.

#### `subscribe`

```json
{
  "type": "subscribe",
  "subscriptions": [
    { "scope": "repo.repo-uuid", "filters": {} },
    { "scope": "mr.42", "filters": { "kind": ["mr.approved","mr.merged"] } }
  ]
}
```

The server re-checks each scope against the viewer's permissions
(§35.1.6). Unauthorized scopes are silently dropped from the
subscription set and the server replies with `Error { code:
"subscribe_forbidden", scopes: [...] }`.

#### `unsubscribe`

```json
{ "type": "unsubscribe", "scopes": ["mr.42"] }
```

#### `ack`

```json
{ "type": "ack", "seq": 12345 }
```

Optional. The server uses `ack` to trim its retention windows for
expensive scopes; clients are free to omit it.

#### `ping`

```json
{ "type": "ping", "nonce": "deadbeef" }
```

JSON-level heartbeat. Sent by clients every 15 s to detect proxies that
strip native WebSocket Ping frames. The server replies with `pong` carrying
the same `nonce`.

### 2.2 Server frames

#### `hello`

```json
{
  "type": "hello",
  "protocol": "jeryu.ws.v1",
  "current_seq": 12340,
  "server_time": "2026-05-27T10:11:12Z"
}
```

Always the first frame after upgrade. If `current_seq` is greater than
`client.resume_from + 1`, the client has missed events and **must**
trigger a snapshot refetch via `GET /api/v1/bootstrap`. The server will
also send a `snapshot_required` if it cannot service the gap from its
own retention.

#### `event`

```json
{
  "type": "event",
  "event": {
    "seq": 12341,
    "timestamp": "2026-05-27T10:11:13Z",
    "scope": "mr.42",
    "entity": "mr",
    "kind": "mr.approved",
    "summary": "alice approved MR !42",
    "payload": { "mr_id": "42", "approver": "alice", "head_sha": "abc..." }
  }
}
```

`payload` is free-form per event kind; consult §3 below. The schema is
captured under `WebEvent` in `websocket-events.schema.json`.

#### `snapshot_required`

```json
{
  "type": "snapshot_required",
  "reason": "client_lagged",
  "current_seq": 12999
}
```

Issued when the server determines that the client cannot catch up from
its event log alone (the broadcast channel overflowed, retention window
elapsed, or the client failed to ack for too long). The client refetches
`/api/v1/bootstrap` and resumes from the snapshot's `current_seq`.

#### `pong`

```json
{ "type": "pong", "nonce": "deadbeef", "server_time": "2026-05-27T10:11:12Z" }
```

Reply to a client `ping`.

#### `error`

```json
{ "type": "error", "code": "subscribe_forbidden", "message": "no perm for repo.priv-…" }
```

Transient, non-fatal. The connection stays open. Codes mirror the REST
error envelope (`subscribe_forbidden`, `event_gap`, `rate_limited`,
`internal`, etc.).

---

## 3. Event kinds

Verbatim from WEB_WORK_CLAUDE.md §16 Appendix B; new kinds added in the
synthesis pass (§35) are marked.

```
repo.created           repo.updated           repo.deleted
repo.archived          repo.settings.changed
repo.branch.created    repo.branch.deleted    repo.branch.protection.changed
repo.file.changed      repo.readme.rendered

mr.created             mr.updated
mr.review.submitted    mr.thread.created      mr.thread.resolved
mr.approved            mr.merged              mr.merge.blocked

check.started          check.completed
workflow.run.started   workflow.run.completed
job.log.chunk

agent.session.started  agent.patch.proposed   agent.evidence.created

settings.preview.created
action.previewed       action.executed

audit.event.created
```

Each event carries:

- `seq: u64` (monotonic, durable).
- `timestamp: RFC3339`.
- `scope: string` (one of the values in §4).
- `entity: string` (e.g. `mr`, `repo`, `audit`).
- `kind: string` (from the list above).
- `summary: string` (one-line human description for the activity dock).
- `payload: object` (kind-specific structured data).

Payload shape examples:

```json
// repo.settings.changed
{ "repo_id": "...", "actor": "alice", "section": "general",
  "diff_hash": "...", "new_state_hash": "..." }

// mr.merged
{ "repo_id": "...", "mr_id": "42", "head_sha": "...",
  "merge_method": "squash", "merge_commit_sha": "..." }

// job.log.chunk
{ "repo_id": "...", "job_id": "...", "cursor": "<opaque>",
  "lines": ["...", "..."], "is_tail": true }
```

---

## 4. Scope vocabulary

Adopted from WEB_WORK_CLAUDE.md §35.1.15. Subscribers send scope strings
verbatim; the server matches by exact string.

```
global.activity                          system.health
user.{user_id}.notifications

repo.{repo_id}                           repo.{repo_id}.activity
repo.{repo_id}.refs                      repo.{repo_id}.checks
repo.{repo_id}.settings                  repo.{repo_id}.issues
repo.{repo_id}.merge_requests

mr.{mr_id}                               issue.{issue_id}
agent.{agent_id}                         runner.{runner_id}
cache.{repo_id}
```

Per-scope permission check (§35.1.6):

| Scope prefix | Permission required |
|---|---|
| `global.activity`, `system.health` | authenticated |
| `user.{me}.notifications` | viewer-id match (viewer can only subscribe to their own) |
| `repo.{repo_id}*` | `repo.read` for that repo |
| `mr.{mr_id}` | `mr.read` for the parent repo |
| `issue.{issue_id}` | `issue.read` for the parent repo |
| `agent.{agent_id}` | `agents.read` |
| `runner.{runner_id}` | `ci.read` |
| `cache.{repo_id}` | `repo.read` |

The server re-checks on every `subscribe` frame (not just at upgrade).

---

## 5. Priority classes

The bus has two underlying tokio broadcast channels (§35.1.13):

| Priority | Capacity | When the channel is full |
|---|---:|---|
| **High** | 4096 | Receivers that lag get disconnected; client reconnects and triggers `snapshot_required`. Never dropped silently. |
| **Medium** | 4096 | Same channel; same back-pressure rule. |
| **Low** | 1024 | Dropped first under pressure. The client does not see them; the BFF increments a metric. |

Priority assignment:

- **High** — action results, audit/security events, direct mutation
  receipts (`mr.approved`, `mr.merged`, `repo.settings.changed`,
  `repo.branch.protection.changed`).
- **Medium** — `check.completed`, `workflow.run.completed`, posture
  changes, MR thread events.
- **Low** — `job.log.chunk`, `agent.evidence.created` heartbeat-style
  events, `repo.readme.rendered` cache warming notifications.

---

## 6. Heartbeat

- Server pings every 15 s (`ping` JSON frame plus a native WebSocket Ping
  control frame for proxies that honor them).
- Server read timeout is 30 s — no client traffic in that window closes
  the connection with code `1011` and reason `read_timeout`.
- Client sends a `ping` every 15 s (mirroring) so server-side `tokio`
  proxies can detect dead clients even when the OS keeps the TCP socket
  open.

15 s is tighter than the 30 s figure in earlier drafts (§35.1.12); the
shorter cadence lets the SPA flip its "live" indicator faster when
connectivity dies.

---

## 7. Reconnect and gap recovery

```
client                              server
  │ open(/api/v1/ws)                  │
  │ ───────────────────────────────► │
  │                                  │
  │ ◄────── 101 Switching            │
  │                                  │
  │ hello { resume_from: 12340 }     │
  │ ───────────────────────────────► │
  │                                  │
  │ ◄────── hello { current_seq: 13099, protocol }
  │                                  │
  │  (gap: 13099 - 12340 > 1)        │
  │                                  │
  │ ◄────── snapshot_required        │
  │            { reason: gap,        │
  │              current_seq: 13099 }│
  │                                  │
  │ GET /api/v1/bootstrap            │
  │ ───────────────────────────────► │
  │                                  │
  │ ◄────── 200 WebBootstrap         │
  │                                  │
  │ subscribe { ... }                │
  │ ───────────────────────────────► │
  │                                  │
  │ ◄────── event { seq: 13100, ... }│
  │ ◄────── event { seq: 13101, ... }│
```

Reconnect policy on the client (`apps/web/src/api/websocket.ts`):

- Exponential backoff, base 500 ms, cap 30 s, full jitter.
- `lastSeq` is persisted to `sessionStorage` under `jeryu.ws.lastSeq.v1`
  so a page refresh resumes without a gap when the server still has the
  events.
- On `snapshot_required` the client invalidates the entire React Query
  cache and refetches `/api/v1/bootstrap` before re-subscribing.

---

## 8. Backpressure

The BFF tracks two metrics per priority class:

```
ws_events_published_total{priority="high|medium|low"}
ws_events_dropped_total{priority="low"}        # never high/medium
ws_subscriptions{scope_prefix="..."}
ws_clients
```

If `ws_events_dropped_total{priority="low"}` rises, lower the per-route
subscription set or split the affected event kind into a dedicated scope
that clients subscribe to opt-in. The merge cockpit, for instance, opts
out of `job.log.chunk` unless the user opens the CI tab.

The bounded broadcast (4096 high/medium + 1024 low) is per-process; in
multi-process deployments the bus is fronted by a per-process channel
backed by the durable `web_events` table.

---

## 9. Closing

| Source | Close code | Reason |
|---|---:|---|
| Server, normal shutdown | `1001` | `going_away` |
| Server, idle | `1011` | `read_timeout` |
| Server, lagged consumer | `1011` | `slow_consumer` |
| Server, auth revoked mid-session | `1008` | `unauthenticated` (client must re-login before reconnect) |
| Client, route change away | `1000` | `normal_closure` |
| Client, page unload | (browser) | (browser) |

Clients should **not** treat `1008 unauthenticated` as recoverable; they
re-login first, then reconnect.

---

## 10. Inspecting the live stream

A simple `websocat` invocation against a local backend:

```bash
websocat --header "Cookie: __Host-jeryu-session=..." \\
  --header "Cookie: __Host-jeryu-csrf=..." \\
  ws://127.0.0.1:8787/api/v1/ws
# then paste:
{"type":"hello","resume_from":null,"subscriptions":[{"scope":"global.activity","filters":{}}]}
```

For continuous tracing, scrape `/metrics` (Prometheus) and filter on
`ws_*`. The SPA also exposes the rolling event buffer through
`useRealtimeStore` (see `apps/web/src/stores/realtimeStore.ts`); the
Storybook story `LiveActivityDock/Live` renders it.
