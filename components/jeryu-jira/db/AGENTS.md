# Work Tracker DB

`db/migrations/` is the canonical SQLite schema source for Work Tracker.
Runtime store initialization must apply these migrations directly rather than
copying schema SQL into Rust source.

When changing schema, update migration tests and run:

```bash
rtk just migration-drift
rtk just check
```
