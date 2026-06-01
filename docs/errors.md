# Error Repair Surface

Jeryu domain errors expose an `AgentRepairHint` with five required fields:
`purpose`, `reason`, `common_fixes`, `docs_url`, and `repair_hint`.
Agents should route failures from this typed surface instead of scraping display
strings.

## Not Found

The requested repository, pull request, queue entry, receipt, or other domain
entity was not present in the current read model. Verify the typed id, refresh
the read model, and rerun the owning crate test.

## Invalid Input

The request failed boundary validation before the domain operation ran. Add or
rerun the boundary test for the rejected input shape before changing policy.

## Policy Denied

A branch, proof, queue, cache, runner, or release policy intentionally blocked
the operation. Preserve the guard and supply the required proof, approval, trust
receipt, or signed witness.

## Conflict

The operation would violate merge or state consistency. Refresh base state,
recompute the witness, and retry through the queue path.

## Missing Receipt

The operation needs durable evidence before mutation. Produce the required
release, cache, scheduler, webhook, or audit receipt and rerun the mapped proof
lane.

## Missing Proof Witness

The merge path needs proof for the exact head commit and owned paths. Run the
owner/test-map proof lane and regenerate the witness before retrying merge.

## Workcell Control Plane

Workcell claims, heartbeats, startup rebases, tar quarantine checks, and
branch-budget enforcement are repairable failures, not silent fallbacks. The
runnerd helpers return a typed `WorkcellError` with the same five-field repair
shape used elsewhere in the product:

- `purpose`
- `reason`
- `common_fixes`
- `docs_url`
- `repair_hint`

Use the docs-linked sections in `docs/testing.md#workcells` and
`docs/boundaries.md#workcells` to repair claim, epoch, path, or merge/delete
denials.
