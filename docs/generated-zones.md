# Generated Zones

Generated files must be declared in `agent/generated-zones.toml` before they are
edited or regenerated.

Rules:

- Do not hand-edit generated artifacts outside their declared zone.
- Generators must be deterministic and runnable from a local proof command.
- Review diffs at the source template or generator when possible.
- If a generated output changes, include the generator command in the relevant
  test-map route or coordination note.
