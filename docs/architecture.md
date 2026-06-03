# Architecture

Jeryu is a local GitHub-compatible forge implemented as Rust workspace crates and local operational scripts. Compatibility means matching observable API and workflow behavior where that is useful for users and agents; it does not mean copying GitHub source, bundling GitHub assets, or requiring a hosted GitHub dependency.

Core boundaries:
- `crates/jeryu-core` and `crates/jeryu-api` own forge domain and API behavior.
- `crates/jeryu-gitd` owns repository storage and Git protocol behavior.
- `crates/jeryu-ci-*`, `crates/jeryu-runner-*`, and `crates/jeryu-runnerd` own CI IR, scheduling, and execution.
- `crates/jeryu-cache*` owns cache/CAS policy and poisoning resistance.
- `crates/jeryu-proof` and `crates/jeryu-agentbridge` own proof routing and bounded agent mutation.
- `crates/jeryu-egress` owns the opt-in live-agent egress contract. The deterministic edit-bot path in `jeryu-agentbridge` stays network-denied; live agent execution must explicitly choose `egress-only`, named secret handling, and a budget gate before launch.

The shared workcell control plane is part of the runner/CI stack, not a separate subsystem. `jeryu-runnerd` owns warm-pool claims, epoch-fenced release/heartbeat handling, startup rebase enforcement, and quarantine-first tar validation on top of the existing runner fabric.

Operational truth is local-first. The canonical validation surfaces are `Justfile`, `ops/ci/*.sh`, `ops/ci/gates/*.sh`, and `agent/test-map.json`.
