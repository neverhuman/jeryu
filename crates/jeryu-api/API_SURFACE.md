# API Surface Implemented In This Bundle

This bundle exposes the typed Phase 10 API facade, the in-process
GitHub-compatible dispatcher, and the first local live Axum server under the
`web` feature.

Implemented typed Phase 10 routes (`Router`):

- `GET /api/phase10/ready`
- `GET /api/phase10/benchmarks/scorecard`
- `GET /api/phase10/benchmarks/replay`
- `GET /api/phase10/slo/dashboard`
- `GET /api/phase10/reliability/soak`
- `GET /api/phase10/rbac/self-test`

## GitHub-compatible REST edge (`GithubRouter`)

The GitHub-compatible REST routes are implemented in `src/github.rs`, backed by
the in-memory `jeryu_core::ForgeCore` store and rendered as GitHub-shaped JSON.
The field shapes (PR `number`, `head`/`base` refs, check-run `conclusion`,
combined commit `state`, branch-protection booleans) are authored against
Jeryu's own parity assertions, not vendored from any external spec. The
`GithubRouter::handle(method, path, body)` dispatcher keeps the in-process
`Response` contract used by conformance tests and embedding callers.

- `GET /health`, `GET /api/v1/version`, `GET /user`
- `GET /repos`, `POST /repos`, `GET /repos/{owner}/{repo}`
- `GET /repos/{o}/{r}/pulls`, `POST /repos/{o}/{r}/pulls`,
  `GET /repos/{o}/{r}/pulls/{number}`, `PUT /repos/{o}/{r}/pulls/{number}/merge`
- `GET /repos/{o}/{r}/issues`, `POST /repos/{o}/{r}/issues`,
  `GET|POST /repos/{o}/{r}/issues/{number}/comments`
- `GET /repos/{o}/{r}/commits/{ref}/status`, `POST /repos/{o}/{r}/statuses/{sha}`
- `GET|POST /repos/{o}/{r}/check-runs`
- `GET|PUT /repos/{o}/{r}/branches/{branch}/protection`
- `GET|POST /repos/{o}/{r}/releases`
- `GET|POST /repos/{o}/{r}/hooks`
- `GET /repos/{o}/{r}/actions/runs`, `GET /repos/{o}/{r}/actions/runs/{id}`,
  `GET /repos/{o}/{r}/actions/runs/{id}/jobs`,
  `GET /repos/{o}/{r}/actions/workflows`,
  `GET /repos/{o}/{r}/actions/workflows/{workflow_id}`,
  `GET /repos/{o}/{r}/actions/workflows/{workflow_id}/runs`
  - These read routes are GitHub-shaped and are projected from local check-runs.
  - The workflow detail and workflow-run list routes accept either a numeric
    workflow id or the workflow file name, matching common `gh workflow view`
    and `gh run view` inspection flows.
  - `POST /repos/{o}/{r}/actions/...` write routes intentionally return a
    guided `501` JSON body with `jeryu_repair_hint`, `jeryu_connection`, and
    `jeryu_steering` pointing to the local MCP/CI path and the supported read
    inspection routes.
- `POST /graphql` for guided compatibility: read-only `viewer`, `__typename`,
  and simple `repository(owner, name)` probes are supported. All other GraphQL
  operations return `501` with `jeryu_repair_hint`, Jeryu MCP tool ids, and REST
  route alternatives.

Status contract: `200` reads, `201` creates, `404` for unknown repos / PRs and
unmatched routes, `422` for invalid bodies / paths / conflicts, `405` when a
pull request is blocked by branch protection, and `501` for unsupported
GraphQL operations and unsupported GitHub Actions writes. Requests outside
this table return a GitHub-shaped `404` error object with `jeryu_repair_hint`,
MCP tool ids, and REST route alternatives.

## Local live web feature

Build with `cargo run -p jeryu-api --features web -- web serve`. The default
bind is `127.0.0.1:8787`, the default SPA directory is `web/dist`, and the
default Rust data directory is `~/.local/share/jeryu`. The server opens
`forge.sqlite` under that data dir through `ForgeCore::open_sqlite`; it does not
reuse legacy `~/.jeryu` secrets or config.

Implemented HTTP/WebSocket routes:

- `GET /health`
- `GET /api/v1/bootstrap`
- `GET /api/v1/bootstrap.tui`
- `GET /api/v1/repos`, `GET /api/v1/repos/{id}`
- `GET /api/v1/repos/{id}/refs`
- `GET /api/v1/repos/{id}/tree`
- `GET /api/v1/repos/{id}/blob`
- `GET /api/v1/repos/{id}/raw`
- `GET /api/v1/repos/{id}/readme`
- `PUT /api/v1/repos/{id}/readme`
- `POST /api/v1/markdown/render`
- `GET /api/v1/codegraph/query`
- `GET /api/v1/codegraph/symbol`
- `GET /api/v1/codegraph/references`
- `GET /api/v1/codegraph/callers`
- `GET /api/v1/codegraph/callees`
- `POST /api/v1/workcells/{id}/run_agent`
- `POST /api/v1/agent-runs`
- `GET /api/v1/agent-runs/{id}`
- `POST /api/v1/agent-runs/{id}/control`
- `GET /api/v1/agent-runs/{id}/events?after_seq=N&limit=M`
- `POST /api/v1/agent-runs/{id}/export_pr`
- `GET /api/v1/ws`
- `POST /mcp`
- `POST /graphql`

The WebSocket sends a `jeryu.ws.v1` hello, responds to JSON
`{"type":"ping","nonce":"..."}` with `pong`, accepts `ack`, `subscribe`, and
`unsubscribe`, and can be reconnected without server-side session state.
Subscriptions to `agent_run.{id}` replay the latest run snapshot and then emit
live `agent_run.event` frames as PTY, stdout, stderr, control, and final events
are recorded.

`POST /mcp` uses the live web MCP backend for repository-scoped
`codegraph.query` and code navigation tools, and for
`agent_work.start/status/control/events/export_pr`. Calls without a `repo`
argument keep the deterministic in-memory fallback used by conformance tests.
