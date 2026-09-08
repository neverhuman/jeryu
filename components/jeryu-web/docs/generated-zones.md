# Generated Zones

Generated files are declared in `agent/generated-zones.toml` before they are
edited or regenerated.

- `contracts/generated/**` is the TypeScript mirror of the Rust contract types.
  It is generated in the owning sibling repository and is not hand-edited here.
- The `tsd` contract-drift lane (`npm run test:contracts`) fails the required
  check when the mirror drifts from the source types.
- When a contract changes, land the Rust source type, its drift test, and the
  regenerated output in the same change.
- Generators are deterministic and runnable from a local proof command.
