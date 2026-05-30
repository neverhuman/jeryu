# Boundaries

Jeryu keeps durable product truth behind typed Rust boundaries.

- `jeryu-core` owns domain objects, branch protection, checks, webhooks, and
  repairable domain errors.
- `jeryu-domain` exposes the canonical domain repair route for agents and audit
  tooling.
- `jeryu-gitd` owns Git repository state and protected ref enforcement.
- `jeryu-cache-*` owns content-addressed cache keys, receipts, poisoning
  defenses, and release cache rules.
- `jeryu-runner-*` owns runner trust decisions, sandbox plans, and execution
  receipts.
- `jeryu-signrail` owns release witnesses, signatures, checksums, and rollback
  metadata.

Cross-boundary calls must use typed ids, receipts, or explicit policy decisions;
direct state mutation from another layer is a bug.
