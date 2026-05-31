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
- Local `AGENTS.md` files under changed paths, for example `docs/AGENTS.md`
  and `crates/jeryu-api/AGENTS.md`
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

## Local Live Runtime

The first live target is local-only. `jeryu-api` can run an Axum server backed
by durable SQLite under the Rust data dir:

```bash
cargo run -p jeryu-api --features web -- web serve \
  --bind 127.0.0.1:8787 \
  --spa-dir web/dist \
  --data-dir ~/.local/share/jeryu
```

The server exposes `/health`, `/api/v1/bootstrap`, `/api/v1/bootstrap.tui`,
`/api/v1/repos`, basic source/README/markdown routes, and `/api/v1/ws`.
`~/.local/share/jeryu` is intentionally separate from the legacy
`~/.jeryu` config/secrets tree.

`jeryu-tui` has a deterministic capture path for fixture or live API data:

```bash
cargo run -p jeryu-tui -- --once --source api \
  --api-url http://127.0.0.1:8787 --tab mission --width 120 --height 40
```

Local Git directories can be registered into the SQLite forge store and a
host-local manifest under the data dir:

```bash
cargo run -p jeryu-mirror-cli -- import-local \
  --data-dir ~/.local/share/jeryu /path/to/repo-or-bare.git
```

## Local CI

Local CI is the source of truth. The default worker count is 40.

```bash
just fast
just ci
just full
just security
just audit
```

The tracker in `CI_TRACKER.md` records the latest passing counts, Jankurai
score, and phase-gate status. Do not make a missing capability look green; keep
it PENDING with evidence until the runtime exists.

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

The engineering plan remains broader than this workspace checkpoint. The
GitHub-compatible REST edge, React web surface, CLI, proof lanes, tool-adoption
evidence, DB migration evidence, and live runner escape matrix are present.
Complete Git protocol parity, production signer adapters, benchmark lab
execution adapters, and deeper daemon hardening remain post-v1 work. Checked-in
tests and local gates must reflect what is actually present, not aspirational
behavior.
