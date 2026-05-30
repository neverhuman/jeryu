# API Surface Implemented In This Bundle

This bundle currently exposes the typed Phase 10 API facade in `crates/jeryu-api`.
It is intentionally not an Axum/HTTP server yet; the checked-in tests exercise the
in-process `Router` contract that the future REST edge can wrap without changing
product-truth behavior.

Implemented typed routes:

- `GET /api/phase10/ready`
- `GET /api/phase10/benchmarks/scorecard`
- `GET /api/phase10/benchmarks/replay`
- `GET /api/phase10/slo/dashboard`
- `GET /api/phase10/reliability/soak`
- `GET /api/phase10/rbac/self-test`

Deferred GitHub-compatible REST routes from the engineering spec remain P0 for
the full product, but they are not implemented in this Phase 10/12 composite
archive. Requests outside the typed route table return `404` from the local
facade until the HTTP edge is added.
