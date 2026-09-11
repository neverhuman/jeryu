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

The maintained root and split commands set `RUST_TEST_THREADS=1`. Process-custody
tests inspect every process with their account's UID, including concurrent test
children, and retain fixtures when any process cannot be inspected. Run those
suites under a dedicated unprivileged account with no unrelated supervisor
processes. The hosted Rust/runtime jobs prepare that identity and grant it only
traversal through workspace ancestors; the Rust job also needs Docker access
for the complete auditor build. Compiler parallelism remains two jobs. Explicit
concurrency inside individual regressions remains exercised.

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

The real browser server wrapper retains its private `jeryu-browser.*` fixture
on every exit. Playwright terminates that wrapper after success as well as
failure, so the wrapper cannot authenticate the browser test result. Its exit
message reports only the path, original directory identity and wrapper status;
these are custody metadata, not a passing-test receipt.

Inside that owner-only directory, `fixture-origin.txt` records the original
device/inode/owner/group/mode tuple, the server binary checksum observed before
launch and an unknown Playwright result. The checksum identifies an observed
binary, not qualified source provenance. `server.log` and the databases remain
private; do not publish them with browser reports. Retain the actual Playwright
result and run identity separately.

Fixture retirement is a separate reviewed action. Before removing an exact
fixture, authenticate the corresponding test result, preserve required failure
evidence, compare its physical path and identity with the original receipt,
and inspect links, mounts and open handles. The existing
[`tests/scratch.sh`](../tests/scratch.sh) removal guard checks the recorded root
identity and symlink/mount boundaries; it does not supply test-success evidence
or complete preservation admission. Do not re-record a replacement root as if
it were the original fixture. The browser wrapper never invokes that remover.
