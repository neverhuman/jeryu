# Codegraph contracts

The owning Rust example `crates/jeryu-codegraph/examples/symbol-schema.rs`
generates `codegraph.schema.json` from `SymbolRow` serialization. Regenerate with
`cargo run --locked -p jeryu-codegraph --example symbol-schema`; do not hand-edit
the generated file. `just check` runs the contract drift test, and `just score`
retains the required audit gate. Schema changes must preserve the actual Rust
field names, accepted string values and integer domain.
