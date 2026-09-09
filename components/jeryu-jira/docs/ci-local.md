# CI Local Parity

Local parity is `rtk just ci-local`. Hosted CI calls the same lane scripts:

1. `scripts/ci-doctor.sh`
2. `ops/ci/fast.sh`
3. `ops/ci/check.sh`
4. `ops/ci/property_tests.sh`
5. `ops/ci/migration_drift.sh`
6. `ops/ci/contract_drift.sh`
7. `ops/ci/score.sh`
8. `ops/ci/security.sh`
9. `ops/ci/release.sh`
10. `ops/ci/artifact_support.sh`

The pre-push hook runs `bash ops/ci/quality-gates.sh`. Enable it with:

```bash
git config core.hooksPath ops/git-hooks
```
