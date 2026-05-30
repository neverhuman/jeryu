# Generated contract artifacts

Declared in `agent/boundaries.toml` (`typescript.generated_contract_paths`,
`queues.generated_type_paths`) and `agent/generated-zones.toml`.

Outputs of contract codegen (OpenAPI/JSON Schema bindings, event type
definitions) belong here. The directory exists so the boundary lane has a
concrete generated zone to reason about.

This repository ships Rust-generated TypeScript bindings here today. If a
new generator is added, keep its output under this path and record the
regeneration command in `agent/generated-zones.toml`.
