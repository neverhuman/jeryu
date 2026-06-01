# Boundaries

Jeryu keeps durable product truth behind typed Rust boundaries.

The machine-readable boundary manifest is `agent/boundaries.toml`. It names the
domain, adapter, web, queue, data-truth, and agent-tool seams that local audits
must check before a cross-boundary change is merged.

- `jeryu-core` owns domain objects, branch protection, checks, webhooks, and
  repairable domain errors.
- `jeryu-domain` exposes the canonical domain repair route for agents and audit
  tooling.
- `jeryu-gitd` owns Git repository state and protected ref enforcement.
- `jeryu-cache-*` owns content-addressed cache keys, receipts, poisoning
  defenses, and release cache rules.
- `jeryu-runner-*` owns runner trust decisions, sandbox plans, and execution
  receipts.
- `jeryu-runnerd` owns the workcell warm pool, claim/release epoch fencing,
  startup rebase enforcement, branch-budget metadata, and quarantine-first
  tar import/export validation.
- `jeryu-signrail` owns release witnesses, signatures, checksums, and rollback
  metadata.

Cross-boundary calls must use typed ids, receipts, or explicit policy decisions;
direct state mutation from another layer is a bug.

## Workcells

Workcell claims can only flow through the runnerd control plane. The workcell
manager may claim a warm cell, fence stale heartbeats by epoch, freeze failed
CI runs into immutable snapshots, and mark a cell blocked if the startup
rebase fails. It may not merge, delete branches, or unpack tarball contents
outside approved repo roots.
