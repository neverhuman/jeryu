# Migrations

Forward-only, numbered `NNNN_name.sql` migrations with a paired
`NNNN_name.meta.toml` (`# jankurai:migration v1` header). Every migration ships
with a `db/rollbacks/NNNN_name.sql` reversal, a `-- timeout-guard:` block, and
constraint rationale in `db/constraints.md`, in the same change.

- `0001_fixture_model` — the dev/e2e fixture relational model: viewer +
  permissions + feature flags, repositories -> pull_requests -> threads/checks,
  runner pools -> runners -> tasks, and work items. Applied by
  `db/seed/build-fixtures.mjs`.
