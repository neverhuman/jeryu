# contracts

Cell guidance for the `contracts/` reference-profile path. See `agent/owner-map.json` (`contracts/` → `contracts`), `agent/boundaries.toml` (`queues.event_contract_paths`, `*.generated_contract_paths`), and `contracts/README.md`.

## Owns

OpenAPI / JSON Schema / protobuf **sources** and event-contract definitions. `contracts/events/` is the declared event-contract path consumed by `[queues]`; `contracts/generated/` is the declared generated-zone for codegen output (no generator is wired yet — see `contracts/generated/README.md`). The contract is append-only in practice: add fields before removing or renaming, keep consumers tolerant of unknown fields, preserve stable event names.

## Forbidden

- Hand-written types under `contracts/generated/` — only the codegen pipeline writes there (see `agent/generated-zones.toml`).
- Importing runtime crates (`sqlx`, `reqwest`, `rdkafka`, etc.) into contract sources; contracts describe wire shape, not transport.
- Treating event payload changes as leaf bugfixes — schema or wire changes must be documented here before broad rollout.

## Proof lane

Contract changes route through the generation / drift lane. Today no generator runs, so verify that downstream Rust consumers still build:

```
cargo check -p jeryu --message-format=json   # proof-lanes.toml: lane.check
cargo nextest run -p jeryu --lib             # proof-lanes.toml: lane.unit
```

When a code generator is wired up, record its regeneration command in `agent/generated-zones.toml` and run it as part of this lane.
