# API Surface Implemented In This Bundle

This bundle currently exposes the typed Phase 10 API facade in `crates/jeryu-api`.
It is intentionally not an Axum/HTTP server yet; the checked-in tests exercise the
in-process `Router` contract that the future REST edge can wrap without changing
product-truth behavior.

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
`Response` contract so a future Axum/HTTP edge can wrap it unchanged.

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

Status contract: `200` reads, `201` creates, `404` for unknown repos / PRs and
unmatched routes, `422` for invalid bodies / paths / conflicts, `405` when a
pull request is blocked by branch protection. Requests outside this table
return a GitHub-shaped `404` error object.
