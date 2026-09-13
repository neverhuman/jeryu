# DB Agent Instructions

Tool Finder does not own a durable product schema. Discovery output is
JSON dossiers, not a second forge database.

Allowed edits:
- Keep this owner map current if a real schema is introduced.

Forbidden edits:
- Do not add `CREATE TABLE` here without a typed store and migrations.
- Do not bypass `jeryu-codegraph` for graph truth.

Proof lane:
- `just score`
