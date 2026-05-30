# Jeryu

Jeryu is a local, GitHub-compatible forge implemented as a Rust-first workspace.
Agents should treat it as the local GitHub-shaped control plane: repositories,
issues, PRs, checks, Actions-compatible workflows, release receipts, and bounded
agent automation all live behind one workspace root.

The product does not depend on hosted GitHub, external forge source, or external
forge assets. Compatibility work is self-authored and fixture-driven. Retired
provider and retired review-request vocabulary are intentionally absent from
code, docs, fixtures, tests, generated artifacts, and operator scripts.

## Agent Start Here

Read these files before editing:

- `AGENTS.md`
- `agent/owner-map.json`
- `agent/test-map.json`
- `agent/generated-zones.toml`
- `agent/proof-lanes.toml`
- `agent/exceptions.toml`
- `docs/architecture.md`
- `docs/testing.md`
- `docs/errors.md`
- `docs/boundaries.md`
- `docs/generated-zones.md`
- `docs/audit-rubric.md`
- `docs/agent-native-standard.md`
- `CI_TRACKER.md`

Public and agent-facing review objects are PRs. Do not add aliases, flags,
fixtures, fields, docs, screenshots, or compatibility layers for retired review
request terminology.

## Implemented Surfaces

| Area | Crates / binaries |
| --- | --- |
| Git service foundation | `jeryu-gitd` |
| Forge/API domain and typed API facade | `jeryu-core`, `jeryu-api` |
| CI IR, scheduler, cache/artifact planning | `jeryu-ci-ir`, `jeryu-ci-compiler`, `jeryu-ci-scheduler`, `jeryu-cache-policy`, `jeryu-artifact-metadata`, `jeryu-ci-bin` |
| Runner fabric | `jeryu-runner-core`, `jeryu-runner-native`, `jeryu-runner-microvm`, `jeryu-runner-oci`, `jeryu-runner-protocol`, `jeryu-runnerd` |
| Rust CI acceleration | `jeryu-rustjet`, `jeryu-rustjet-cli` |
| JeryuCache cache/CAS | `jeryu-cache-core`, `jeryu-cache-service`, `jeryu-cache-cli`, `jeryu-cache-adversary`, `jeryu-cache` |
| Jankurai proof and agent bridge | `jeryu-proof`, `jeryu-agentbridge` |
| Release provenance | `jeryu-signrail` |
| GitHub-compatible backup and restore | `jeryu-mirror`, `jeryu-mirror-cli` |
| Benchmark and observability | `jeryu-bench`, `jeryu-obs` |
| Enterprise/operations layer | `jeryu-enterprise`, `phase11-*`, `jeryu-kernel`, `jeryu-tenant`, `jeryu-replay-verifier`, `jeryu-phase11-bin` |

## Local CI

Local CI is the source of truth. The default worker count is 40.

```bash
just fast
just ci
just full
just security
just audit
```

The tracker in `CI_TRACKER.md` records the latest passing counts and which
phase gates are PASS or honestly PENDING. Do not make a missing capability look
green; keep it PENDING with evidence until the runtime exists.

## Cache Laws

The Phase 12 JeryuCache implementation is designed around these cache laws:

- fork and public/untrusted jobs read source caches only and cannot consume trusted compiled caches;
- trusted compiled cache writes require T1 protected-internal policy and a green protected decision;
- cache key material is verified before writes/restores and must match the request trust tier;
- cross-project compiled reads are denied unless explicitly allowlisted;
- release lanes ignore mutable compiled caches and use L6 hermetic snapshots;
- promotion from quarantine indexes the promoted artifact for later restore and emits receipts;
- restore without an explainable fingerprint is blocked;
- all cache events emit deterministic JSON receipts;
- adversarial false-hit checks compare key material and object digests before accepting reuse.

## Useful Commands

```bash
just fast
just full
./scripts/ci-doctor.sh
cargo run -p jeryu-cache-cli -- self-test
cargo run -p jeryu-cache-cli -- key --material examples/cache-key-material.json
cargo run -p jeryu-cache-cli -- policy --request examples/fork-pr-write-request.json
```

The archive includes validation scripts that are safe to run even when macOS
AppleDouble `._*` files are present from tar extraction.

## Implementation Boundary

The engineering plan remains broader than this workspace checkpoint. The full
GitHub-compatible HTTP REST edge, complete Git protocol parity matrix,
production signer adapters, and full benchmark lab execution adapters are still
future work. Checked-in tests and local gates must reflect what is actually
present, not aspirational behavior.
