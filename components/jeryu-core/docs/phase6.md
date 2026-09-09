# Phase 6 — JeryuCache cache/CAS

## Built surface

JeryuCache implements project-scoped source/cache/CAS with a strict policy layer and append-only receipts.

## Cache layers represented

- L1 runner-local project cache: modeled as repo-scoped compiled artifact entries.
- L2 registry/source blob cache: modeled as content-addressed source and registry blobs.
- L3 repo-scoped compiled artifact CAS: `CacheScope::Repo`.
- L4 tenant source/dependency CAS: `CacheScope::Tenant` for source/registry blobs.
- L5 explicitly shared compiled artifact CAS: `CacheScope::Shared` only after policy allowlist.
- L6 release-hermetic snapshot: `CacheKind::ReleaseVendorSnapshot`.

## Hard cache laws

- No false hits tolerated: restored bytes are verified against their indexed object digest; mismatched manifests trigger quarantine.
- No fork writes trusted cache: fork and public trust tiers are denied compiled artifact writes.
- No untrusted global writes: untrusted jobs may read immutable source cache only.
- No release mutable cache: release lanes safe-miss mutable compiled cache.
- No cross-project compiled cache by default: shared scopes require explicit configuration.
- No cache promotion without receipt: promotion requires green protected policy and emits a promotion receipt.

## Receipts

Receipts are written under `<vault-root>/receipts/` as individual JSON files and appended to `log.ndjson`.
