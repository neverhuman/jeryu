# jeryu-cache

**Audit status: PENDING revision-bound verification.** The
[proof gate](ops/ci/proof_evidence.sh) requires at least 91 and its existing ratchet.

JeryuCache policy, CAS, receipts, and adversarial poisoning tests.

Agent and reviewer entrypoint: [`AGENTS.md`](AGENTS.md).

`git.neverhuman.org` is the Git transport source of truth. This checkout is a
candidate until its exact commit has the protected `jeryu-cache/required`
check, an independent approval, and a fast-forward merge.

This repository was seeded from Jeryu source commit `cbecf7caa0e932c76a341b2521e66e911233860d` by
`ops/split/materialize.py`. It is part of the eleven-repository Jeryu split family and keeps source
paths stable where practical so ownership remains auditable.

## Quick Start

From the canonical checkout, run the exact local gate used to prepare hosted
proof:

```bash
rtk bash scripts/ci-local.sh required
```

## Owned Cargo Packages

- `crates/jeryu-cache-core`
- `crates/jeryu-cache-service`
- `crates/jeryu-cache-cli`
- `crates/jeryu-cache-adversary`
- `crates/jeryu-cache`

## Source Coverage

- `crates/jeryu-cache-core/**`
- `crates/jeryu-cache-service/**`
- `crates/jeryu-cache-cli/**`
- `crates/jeryu-cache-adversary/**`
- `crates/jeryu-cache/**`
- `tests/cache_poisoning_matrix.sh`
- `fixtures/cache-poisoning/**`
- `config/jeryu-cache-policy.toml`
- `policies/cache-laws.toml`
- `examples/cache-key-material.json`

## Local Commands

- `rtk just fast`
- `rtk just check`
- `rtk just score`
- `rtk just security`
- `rtk just artifact-support`
- `rtk just contract-drift`
- `rtk just contract-schema`

The stable `jeryu-cache-core::CacheReceipt` wire contract is inventoried in
[`contracts/README.md`](contracts/README.md) and defined by a closed JSON Schema.

## Governed auditor

Hosted proof and local sealing invoke only the receipt-verified
`/home/ubuntu/.jeryu/bin/jankurai` identity rendered by `jeryu-tool`. The
1.6.11 auditor cutover is CI authority only; it
does not change this repository's product version, release tag, or artifacts.
