# Audit Rules

The pinned audit policy is `agent/audit-policy.toml`.

Required launch bar:

- minimum score: `85`
- hard findings allowed: `0`
- required tool: `jankurai 1.6.11`

Audit artifacts:

- `.jankurai/repo-score.json`
- `.jankurai/repo-score.md`
- `target/jankurai/repo-score.json`
- `target/jankurai/repo-score.md`
- `target/jankurai/security/evidence.json`
- `target/jankurai/migration-report.json`

Run `rtk just score` after changing CI, docs, generated zones, contracts, DB
migrations, or agent metadata.
