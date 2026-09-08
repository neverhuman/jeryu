# Rollbacks

One `NNNN_name.sql` reversal per forward migration (referenced by the
migration's `.meta.toml` `rollback` field), each carrying a `-- timeout-guard:`
block. Reversals drop tables child-first so the `ON DELETE CASCADE` foreign keys
never dangle mid-teardown.

- `0001_fixture_model.sql` (+ `.meta.toml`) — drops the dev/e2e fixture store.
  Safe to run unconditionally: the store is synthetic and regenerable, so
  recovery is `npm run fixtures:build`, not a database restore.
