# Jeryu CI Testing & Evaluation Framework

This document outlines the Continuous Integration (CI) and testing methodologies for the **Jeryu** project (located in the `JeRyu` repository). It provides a full specification for external agents and developers on how we organize, run, and validate our tests, runners, and integrations.

## 1. Overview of the Test Suites

The `jeryu` codebase uses a comprehensive testing strategy combining unit tests, integration tests, and full End-to-End (E2E) environment simulations using the robust Rust `tokio::test` framework.

The tests are located primarily in the `tests/` directory:
- **`tests/e2e.rs`**: Full lifecycle tests. Protests the system by spinning up ephemeral GitLab runners, dynamically creating projects, committing test `.gitlab-ci.yml` files, scaling runner pools, and validating log trace outputs to ensure runners correctly pick up and execute jobs.
- **`tests/agent_tests.rs`**: Tests the Autonomous Agent flow. Verifies that `jeryu` can spawn an AI agent task, create a separate branch, commit code, and open a Merge Request back to `main`.
- **`tests/pool_tests.rs`** & **`tests/job_tests.rs`**: Validates runner pool limits, ephemeral pool teardowns, and job specific queue logic.
- **`tests/cache_integration_test.rs`**: Tests the `Jeryu SmartCache` mechanics to ensure pipeline data is correctly tracked and cached locally using embedded RedlineDB state and Docker runner/runtime surfaces.

### Local Execution Strategy
To run these environments locally, the following commands are used:
```bash
# Run standard library unit tests
cargo test --lib

# Run all integration tests (must have local GitLab and Docker running)
cargo test --tests
```

## 2. GitHub Actions CI Configuration

The core CI validation is defined in `.github/workflows/rust.yml`. This configuration enforces formatting (`clippy`, `rustfmt`), verifies build stability, and runs both library and integration tests on the Ubuntu CI runners.

### `rust.yml`

```yaml
name: Rust

on:
  push:
    branches: ["main"]
  pull_request:
    branches: ["main"]

env:
  CARGO_TERM_COLOR: always

jobs:
  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - name: Check formatting
        run: cargo fmt --all -- --check

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - name: Rust cache
        uses: Swatinem/rust-cache@v2
      - name: Run clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

  build:
    name: Build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
      - name: Rust cache
        uses: Swatinem/rust-cache@v2
      - name: Build workspace
        run: cargo build --verbose

  test-lib:
    name: Library Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
      - name: Rust cache
        uses: Swatinem/rust-cache@v2
      - name: Run library tests
        run: cargo test --lib --verbose

  test-integration:
    name: Integration Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
      - name: Rust cache
        uses: Swatinem/rust-cache@v2
      - name: Run integration tests
        run: cargo test --tests --verbose

  tui-smoke:
    name: TUI Smoke
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
      - name: Rust cache
        uses: Swatinem/rust-cache@v2
      - name: Render one TUI frame
        run: cargo run -- tui --once
```

## 3. E2E Environment & System Resilience

Since `jeryu` manages Docker containers and interacts with GitLab, the Integration tests interact heavily with actual systems.

A full E2E test requires a bootstrapped GitLab environment. The test infrastructure provides utilities located in `tests/common/` that:
- Ensure the `jeryu` `.env` bindings map to the correct local GitLab configuration (`GITLAB_HTTP_PORT`, `GITLAB_PAT`).
- Bootstrap isolated generic repositories uniquely hashed per test (e.g. `e2e-test-<uuid>`).
- Safely allocate and then tear down Ephemeral Runner Pools directly through the Docker Controller.

### Garbage Collection & Diagnostics

Due to the ephemeral and dynamic nature of test runner generation, stray Docker volumes or detached containers can accumulate, significantly impacting space. `jeryu` ships with an autonomous system-level garbage collector deployed to servers running the CI logic:

**`ops/ci/jeryu-gc.timer`** executes every 6 hours with a randomized delay to prevent thundering herds across multiple instances:
```ini
[Unit]
Description=Run Jeryu SmartCache GC every 6 hours

[Timer]
OnBootSec=15min
OnUnitActiveSec=6h
RandomizedDelaySec=300
Persistent=true

[Install]
WantedBy=timers.target
```

**`ops/ci/jeryu-gc.service`** maps cleanly to the `jeryu gc` CLI tool, tearing down lost assets while logging metrics to `journald`:
```ini
[Unit]
Description=Jeryu SmartCache Garbage Collection
Documentation=https://github.com/neverhuman/jeryu
After=network.target docker.service

[Service]
Type=oneshot
User=ubuntu
ExecStart=/home/ubuntu/.local/bin/jeryu gc
TimeoutStartSec=600
StandardOutput=journal
StandardError=journal
SyslogIdentifier=jeryu-gc
RemainAfterExit=no
```

These rules heavily mitigate the CI testing failure scenarios described by previous GitLab 502 bad gateway errors during pipeline congestion, ensuring resources are adequately released between heavy E2E workflow evaluations.
