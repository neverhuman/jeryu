# crates/adapters

Cell guidance for the `crates/adapters/` reference-profile path. See `agent/boundaries.toml` (`queues.adapter_paths = ["crates/adapters/queues", "crates/adapters/src/queues"]`) and `agent/owner-map.json` (`crates/` → `tools`).

## Owns

Observable Rust adapters that translate between domain code and external runtimes. Today this directory hosts:

- `crates/adapters/cache-brain/` — sqlx-backed cache adapter crate (`cache-brain-adapter`, see its `Cargo.toml`). Real code; isolates `sqlx` away from domain modules.
- `crates/adapters/queues/` — declared queue-adapter path. **Scaffold only**: no Rust crate exists yet (see its `README.md`); the directory exists so the `[queues]` boundary routes through a real path when an adapter is added (Kafka today, Tansu / Iggy / Fluvio under the streaming exception).

## Forbidden

- Domain modules listed in `boundaries.toml` (`src/decision.rs`, `src/capsule.rs`, `src/impact.rs`) must not import client crates directly. Per `forbidden_domain_imports`, `sqlx::`, `rdkafka::`, `reqwest::`, etc. live behind adapters here.
- Domain code consuming this layer must depend on typed event contracts from `contracts/events/`, never on a raw client.
- Don't introduce a new client (`kafkajs`, `nats`, `redis streams`, …) without updating the streaming exception in `boundaries.toml`.

## Proof lane

```
cargo check -p cache-brain-adapter
cargo check -p jeryu --message-format=json     # proof-lanes.toml: lane.check
cargo nextest run -p jeryu --lib               # proof-lanes.toml: lane.unit
```

For state-touching changes, also run the integration lane: `cargo test -p jeryu --tests -- --test-threads=1`.
