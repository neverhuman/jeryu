# Boundaries

## Contract Boundary

Rust contracts are the source of truth. Regenerate TypeScript with:

```bash
rtk cargo run -p jeryu-jira --bin export_contracts
rtk just contract-drift
```

Do not commit generated Work TypeScript outside `contracts/generated/`.

## Data Boundary

SQLite schema changes start in `db/migrations/`. Runtime store initialization
applies the migration SQL directly, so tests and production open the same schema
source. Keep DB constraints aligned with Rust validation for enum values,
positive issue numbers, and non-empty user-authored text.

Direct repair SQL is not a normal fix path. If stored rows drift from the Work
contract, add a reviewed migration and rerun `rtk just migration-drift`.

## Integration Boundary

`jeryu-deploy` may mirror GitHub-compatible issue routes into Work. Mirror
failures must leave structured repair evidence so a later agent can replay or
repair the Work link without losing the GitHub-compatible API response.
