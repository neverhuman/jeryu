# Jeryu Composite Workspace

This bundle is a Rust-first Jeryu workspace assembled from the phase
implementations in the engineering spec. It now enrolls the checked-in product
crates and binaries under one root workspace so local validation can address the
whole codebase instead of only the Phase 12 cache slice.

## Implemented surfaces in this archive

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
| Migration/backup | `jeryu-mirror`, `jeryu-mirror-cli` |
| Benchmark and observability | `jeryu-bench`, `jeryu-obs` |
| Enterprise/operations layer | `jeryu-enterprise`, `phase11-*`, `jeryu-kernel`, `jeryu-tenant`, `jeryu-replay-verifier`, `jeryu-phase11-bin` |

## Current Phase 12 cache gates

The JeryuCache implementation is designed around these cache laws:

- fork and public/untrusted jobs read source caches only and cannot consume trusted compiled caches;
- trusted compiled cache writes require T1 protected-internal policy and a green protected decision;
- cache key material is verified before writes/restores and must match the request trust tier;
- cross-project compiled reads are denied unless explicitly allowlisted;
- release lanes ignore mutable compiled caches and use L6 hermetic snapshots;
- promotion from quarantine indexes the promoted artifact for later restore and emits receipts;
- restore without an explainable fingerprint is blocked;
- all cache events emit deterministic JSON receipts;
- adversarial false-hit checks compare key material and object digests before accepting reuse.

## Local commands

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

## Important implementation boundary

The engineering spec remains broader than this archive. The full GitHub-compatible
HTTP REST edge, complete Git protocol parity matrix, production signer adapters,
and full benchmark lab execution adapters are still future work. The checked-in
code is now wired as one workspace and the local typed APIs/tests reflect what is
actually present in this bundle.
