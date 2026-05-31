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

- `GET /health`, `GET /api/v1/version`
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
- `POST /graphql` for guided compatibility: read-only `viewer`, `__typename`,
  and simple `repository(owner, name)` probes are supported. All other GraphQL
  operations return `501` with `jeryu_repair_hint`, Jeryu MCP tool ids, and REST
  route alternatives.

Status contract: `200` reads, `201` creates, `404` for unknown repos / PRs and
unmatched routes, `422` for invalid bodies / paths / conflicts, `405` when a
pull request is blocked by branch protection, and `501` for unsupported
GraphQL operations. Requests outside this table return a GitHub-shaped `404`
error object.

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
- `POST /api/v1/markdown/render`
- `GET /api/v1/ws`
- `POST /graphql`

The WebSocket sends a `jeryu.ws.v1` hello, responds to JSON
`{"type":"ping","nonce":"..."}` with `pong`, accepts `ack`, `subscribe`, and
`unsubscribe` as no-ops, and can be reconnected without server-side session
state.
