# Architecture

Jeryu is a local GitHub-compatible forge implemented as Rust workspace crates and local operational scripts. Compatibility means matching observable API and workflow behavior where that is useful for users and agents; it does not mean copying GitHub source, bundling GitHub assets, or requiring a hosted GitHub dependency.

This checkout owns source only for `crates/jeryu-api`, `crates/jeryu-cli`, and
`crates/jeryu-split-tool`. Core, Git storage, CI, runner, cache, proof, agent,
codegraph, and signing packages are immutable Git dependencies owned by their
standalone repositories. Deploy may exercise their public contracts through
its API/CLI graph, but it must not present their source gates as local source.
See `docs/codegraph-oracle.md` for the composed codegraph contract.

The shared workcell control plane is part of the runner/CI stack, not a separate subsystem. `jeryu-runnerd` owns warm-pool claims, epoch-fenced release/heartbeat handling, startup rebase enforcement, and quarantine-first tar validation on top of the existing runner fabric.

Deploy's agent-run edge keeps one wire-type and routing authority in
`web/agent_runs.rs`. Child modules isolate HTTP/MCP/driver orchestration,
bounded run and raw-TTY state, and frozen-diff pull-request export. They expose
only the same `web`-scoped surface as the former single file; persistence,
runner authority, and serialized contracts remain owned by their existing
components.

The API keeps the same module and route authority while isolating split-family
catalog loading, bootstrap credential custody, session runtime preparation,
repository source reads, pull-request merge posture, and installed Jankurai
verification in child modules. The HTTP and MCP edges both apply one bounded
`x-request-id` middleware: portable caller IDs round-trip, while empty, hostile,
or oversized values are replaced before handlers observe them.

Native pull-request reviews cross an authenticated Core boundary: the API
translates real credentials and exact-head challenge submissions, while Core
owns observation, credential revalidation, event persistence and effective
verdicts. Gitd independently reads managed refs and trees. The API dispatches
those blocking operations outside its async worker threads and projects Core's
qualification blockers. Older review/check rows are visible advisory history;
the removed split readiness/finalization path cannot advance Git while the
durable merge executor is unavailable.

The R5 proof lane lives in `crates/jeryu-api` and closes the loop from claim to reviewed pull request: rebase, jailed edit, namespaced branch export, PR creation, and CI evidence verification. The export request carries the changed-file list so the pull request preserves branch ownership and reviewer-visible edit scope.

JMCP/control-plane intelligence is an API/read-model boundary over local truth:
the local forge store, runner fabric, workcells, agent runs, codegraph, and
tool-build clusters are authoritative, while GitHub mirror data is optional
read-only evidence that must degrade as `missing`, `stale`, `queued`, `failed`,
or `unknown` rather than becoming an implicit green signal.

The canonical reproducible validation surfaces are `Justfile`, `ops/ci/*.sh`,
`ops/ci/gates/*.sh`, and `agent/test-map.json`; protected hosted refs and the
exact-head required context are merge authority.

`tools/security-lane.sh` owns the security implementation. Compatibility
callers may enter through `ops/ci/security.sh`, but that file delegates without
reimplementing checks; the comprehensive and pull-request gates enable network
dependency audits explicitly.

## Standalone Delivery Authority

This split repository composes immutable sibling releases into the API and CLI
delivery graph. It consumes the governed Jankurai identity rendered from the
protected `jeryu-tool` manifest, but it never issues, signs, or silently
substitutes that identity. The release-broker path is a deployment boundary:
release CI must resolve the single physical broker binary before any score or
artifact decision is trusted.
