# Generated Zones

`agent/generated-zones.toml` declares paths whose contents are generated
and must not be hand-edited. This repo currently checks in contract
artifacts under `contracts/generated/` and `schemas/`.

## Convention

When a future change adds a generated artifact, declare it here and in
`agent/generated-zones.toml` using this shape:

```toml
[[zone]]
path = "contracts/generated"
generator = "cargo run --bin jeryu_export_types --features web"
owner = "contracts"
regenerate = "cargo run --bin jeryu_export_types --features web"
```

Rules for a zone:

- Files under `path` are overwritten by `regenerate`; do not edit them
  by hand and do not commit unrelated diffs into them.
- `generator` is the canonical command. CI must be able to reproduce
  the contents byte-for-byte from a clean checkout.
- `owner` matches a row in `agent/owner-map.json`.
- `regenerate` is the local shortcut an agent runs after touching the
  source schema or template.

## Cross-references

- Boundary manifest: `agent/boundaries.toml`
  (`[typescript].generated_contract_paths`,
  `[queues].generated_type_paths`).
- Owner map: `agent/owner-map.json`.
- Audit rule: zones not declared here but containing generated-looking
  output will surface in `agent/repo-score.md` under the relevant
  contract or data dimension.

## Why this file exists

Audit findings expect agent-readable docs at `docs/generated-zones.md`.
Keeping the explainer here means the next contributor can declare a zone
correctly the first time, without needing to read the upstream jankurai
source.
