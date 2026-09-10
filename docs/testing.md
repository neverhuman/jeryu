# Testing

Local and GitHub product checks use `bash scripts/ci.sh LANE`. The complete
required lane set is maintained by `scripts/ci-lanes.sh` and the GitHub workflow.
Use the same command and exact source revision when comparing their results.

```bash
./scripts/build.sh
bash scripts/ci.sh source
bash scripts/ci.sh web
```

These are selected contributor checks. The complete product qualification
command is `bash scripts/ci.sh all`; every required result must be present and
successful. Missing capabilities, failures, cancellation, timeout or skipped
required proofs cannot qualify the aggregate. See
[CI coverage](migration/CI-COVERAGE.md) for the maintained obligation inventory.

Rust checks preserve formatting, warning-denied Clippy, tests and supported
feature configurations. Web checks preserve contracts, lint, types, unit and
browser tests, production assets, Storybook, accessibility and performance
requirements. Runtime checks exercise source installation and the real server.
Product, security, split and owning component proofs remain independently
required until equivalent replacement coverage is demonstrated.

Native sandbox and OCI lanes need disposable Linux environments with the
required kernel and container capabilities. Read the owning command and
admission checks before running them. Installing the forge does not require
these privileged verification environments.

Jankurai uses the identity owned by
`components/jeryu-tool/tool-manifest.toml`. A complete audit census, protected
predecessor admission and live evidence publication remain qualification gates;
their implementation status is recorded in [current status](migration/STATUS.md).
Do not substitute a diagnostic subset, stale report or candidate-owned baseline.

`bash scripts/ci.sh redline` is a separate optional compatibility proof. Its
absence does not block the required SQLite matrix.

Record the exact commit, command, exit status and report identity. Keep original
failures and retry history. Preserve failed scratch for reviewed custody checks;
never remove original checkouts or runtime state as part of routine test cleanup.
