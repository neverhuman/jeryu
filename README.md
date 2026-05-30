# JitForge Nitro Composite Workspace

This bundle is a Rust-first JitForge Nitro workspace assembled from the phase
implementations in the engineering spec. It now enrolls the checked-in product
crates and binaries under one root workspace so local validation can address the
whole codebase instead of only the Phase 12 cache slice.

## Implemented surfaces in this archive

| Area | Crates / binaries |
| --- | --- |
| Git service foundation | `gitd` |
| Forge/API domain and typed API facade | `forge-core`, `jitforge-api` |
| CI IR, scheduler, cache/artifact planning | `ci-ir`, `ci-compiler`, `ci-scheduler`, `cache-policy`, `artifact-metadata`, `jit-ci` |
| Runner fabric | `runner-core`, `runner-native`, `runner-microvm`, `runner-oci`, `runner-protocol`, `runnerd` |
| Rust CI acceleration | `rustjet`, `rustjet-cli` |
| CrateVault cache/CAS | `cratevault-core`, `cratevault-service`, `cratevault-cli`, `cratevault-adversary`, `cratevault` |
| Jankurai proof and agent bridge | `proofcore`, `agentbridge` |
| Release provenance | `signrail` |
| Migration/backup | `mirrorvault`, `mirrorvault-cli` |
| Benchmark and observability | `benchlab`, `jitforge-obs` |
| Enterprise/operations layer | `jitforge-enterprise`, `phase11-*`, `nitro-kernel`, `tenant-guard`, `replay-verifier`, `jit-phase11` |

## Current Phase 12 cache gates

The CrateVault implementation is designed around these cache laws:

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
cargo run -p cratevault-cli -- self-test
cargo run -p cratevault-cli -- key --material examples/cache-key-material.json
cargo run -p cratevault-cli -- policy --request examples/fork-pr-write-request.json
```

The archive includes validation scripts that are safe to run even when macOS
AppleDouble `._*` files are present from tar extraction.

## Important implementation boundary

The engineering spec remains broader than this archive. The full GitHub-compatible
HTTP REST edge, complete Git protocol parity matrix, production signer adapters,
and full benchmark lab execution adapters are still future work. The checked-in
code is now wired as one workspace and the local typed APIs/tests reflect what is
actually present in this bundle.
