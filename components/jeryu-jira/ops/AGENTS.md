# Work Tracker Ops

CI scripts in `ops/ci/` are the canonical hosted and local proof lanes.
Keep lane scripts narrow and observable: fast, check, migration drift,
contract drift, score, security, and artifact support should remain separate
entrypoints.

Before changing workflow or script behavior, run:

```bash
rtk just fast
rtk just check
rtk just score
```
