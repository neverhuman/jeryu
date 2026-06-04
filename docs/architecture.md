# Architecture

Jeryu is a local GitHub-compatible forge implemented as Rust workspace crates and local operational scripts. Compatibility means matching observable API and workflow behavior where that is useful for users and agents; it does not mean copying GitHub source, bundling GitHub assets, or requiring a hosted GitHub dependency.

Core boundaries:
- `crates/jeryu-core` and `crates/jeryu-api` own forge domain and API behavior.
- `crates/jeryu-gitd` owns repository storage and Git protocol behavior.
- `crates/jeryu-ci-*`, `crates/jeryu-runner-*`, and `crates/jeryu-runnerd` own CI IR, scheduling, and execution.
- `crates/jeryu-cache*` owns cache/CAS policy and poisoning resistance.
- `crates/jeryu-proof` and `crates/jeryu-agentbridge` own proof routing and bounded agent mutation.
- `crates/jeryu-codegraph` owns hosted code-oracle indexing and impact packs
  for resolved repo refs; see `docs/codegraph-oracle.md`.

The shared workcell control plane is part of the runner/CI stack, not a separate subsystem. `jeryu-runnerd` owns warm-pool claims, epoch-fenced release/heartbeat handling, startup rebase enforcement, and quarantine-first tar validation on top of the existing runner fabric.

The R5 proof lane lives in `crates/jeryu-api` and closes the loop from claim to reviewed pull request: rebase, jailed edit, namespaced branch export, PR creation, and CI evidence verification. The export request carries the changed-file list so the pull request preserves branch ownership and reviewer-visible edit scope.

JMCP/control-plane intelligence is an API/read-model boundary over local truth:
the local forge store, runner fabric, workcells, agent runs, codegraph, and
tool-build clusters are authoritative, while GitHub mirror data is optional
read-only evidence that must degrade as `missing`, `stale`, `queued`, `failed`,
or `unknown` rather than becoming an implicit green signal.

Operational truth is local-first. The canonical validation surfaces are `Justfile`, `ops/ci/*.sh`, `ops/ci/gates/*.sh`, and `agent/test-map.json`.
