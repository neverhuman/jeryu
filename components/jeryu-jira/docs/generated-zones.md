# Generated Zones

Generated zones are declared in `agent/generated-zones.toml`.

- `.jankurai/` is written by `rtk just score` and is not committed.
- `contracts/generated/` is written by
  `rtk cargo run -p jeryu-jira --bin export_contracts`.

Manual edits to generated TypeScript are not allowed. Change
`crates/jeryu-jira/src/contracts.rs`, regenerate, and run
`rtk just contract-drift`.
