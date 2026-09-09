# Generated Zones

Generated files must be declared in `agent/generated-zones.toml` before they are
edited or regenerated.

Rules:

- Do not hand-edit generated artifacts outside their declared zone.
- Generators must be deterministic and runnable from a local proof command.
- Review diffs at the source template or generator when possible.
- If a generated output changes, include the generator command in the relevant
  test-map route or coordination note.

The `BEGIN GENERATED JANKURAI ... DO NOT EDIT` blocks are rendered from the
protected sibling `jeryu-tool/tool-manifest.toml`. Operators rotate that
manifest through its reviewed lifecycle, render the complete Deploy projection,
and verify it with:

```bash
../jeryu-tool/ops/render-tool-manifest.sh --check --repo jeryu-deploy \
  --repo-root "jeryu-deploy=$(pwd)"
cargo test --locked --offline -p jeryu-api --features web --test jankurai_governance
```

Never correct one consumer by hand or treat a score artifact as generator
authority.
