# Jeryu Tool control contracts

This repository has no network service or product-data contract. Its public
surface is the versioned control data consumed by the family renderer and the
closed evidence accepted by CI:

- `tool-manifest.toml` is the immutable Jankurai pin and consumer-rendering authority.
- `tools-registry.toml` and `tasks/*.toml` are the reusable-tool and build-queue authorities.
- `schemas/*.schema.json` are the closed evidence and repair receipt contracts.

`ops/ci/contract-drift.sh` validates those authorities together with the
generated pin and registry summary. Contract changes therefore fail closed in
the named `contract-drift` proof lane; generated consumers remain renderer-only.
