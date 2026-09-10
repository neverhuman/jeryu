#!/usr/bin/env bash
# Local and hosted CI use this same entrypoint; `all` lists required lanes.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"
export CI=true
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
export JERYU_CI_JOBS=$CARGO_BUILD_JOBS
export JERYU_WEB_DIST="$root/components/jeryu-web/apps/web/dist"
export PATH="$root/target/ci-tools/bin:$PATH"

web_build() { npm ci; npm run build; }

case ${1:-all} in
  source)
    bash tests/ci-matrix.sh
    cargo run --locked -p jeryu-split-tool --bin jeryu-split -- monorepo-check
    cargo run --locked -p jeryu-split-tool --bin jeryu-split -- manifest --check-paths
    cargo run --locked -p jeryu-split-tool --bin jeryu-split -- proof-inventory --check
    ;;
  public)
    cargo run --locked -p jeryu-split-tool --bin jeryu-split -- public-preflight
    ;;
  redline)
    # Explicit compatibility proof, independent of the SQLite release matrix.
    cargo fmt --manifest-path components/jeryu-release-ops/tests/redline/Cargo.toml --all -- --check
    cargo clippy --locked --manifest-path components/jeryu-release-ops/tests/redline/Cargo.toml --all-targets -- -D warnings
    cargo test --locked --manifest-path components/jeryu-release-ops/tests/redline/Cargo.toml
    ;;
  auditor)
    source scripts/bootstrap-jankurai.sh
    bootstrap_public_jankurai
    ;;
  auxiliary)
    source scripts/bootstrap-jankurai.sh
    bootstrap_public_jankurai
    bash scripts/auxiliary-proofs.sh "${@:2}"
    ;;
  rust)
    web_build
    cargo fmt --all -- --check
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    # Full metadata resolves all targets, including crates a host-only build did not fetch.
    cargo fetch --locked
    # Runner transport tests require the actual immutable public auditor, even
    # on a fresh host. Absence or verification failure is a lane failure.
    source scripts/bootstrap-jankurai.sh
    bootstrap_public_jankurai
    cargo test --locked --workspace --all-features --exclude jeryu-sandbox-linux
    # Workspace feature unification otherwise hides the supported API without Web.
    cargo test --locked -p jeryu-api --no-default-features
    cargo clippy --locked -p jeryu-api --all-targets --no-default-features -- -D warnings
    ;;
  runner-governed)
    # Separately mandatory installed-authority handover proof. No candidate
    # receipt, file-existence gate or ignored test can substitute for this lane.
    JERYU_MONOREPO_CANDIDATE=0 cargo test --locked -p jeryu-runnerd \
      --test hosted_dependency_transport
    ;;
  web)
    bash scripts/contracts.sh --check
    npm ci
    npm run lint
    npm run typecheck
    npm test
    npm --workspace @jeryu/web run test:contracts
    npm run fixtures:test
    npm run build
    npm --workspace @jeryu/web run build-storybook
    npm --workspace @jeryu/web exec -- playwright install chromium
    cargo build --locked -p jeryu-cli --bin jeryu
    JERYU_PLAYWRIGHT_API_URL=http://127.0.0.1:18787 npm --workspace @jeryu/web run test:e2e:bff
    JERYU_PLAYWRIGHT_E2E_MODE=ui-only npm --workspace @jeryu/web run test:e2e
    npm --workspace @jeryu/web run test:e2e:matrix
    bash scripts/ci-perf.sh
    npm run ux-qa:test
    ;;
  runtime)
    bash scripts/build.sh
    JERYU_REQUIRE_WEB=1 bash scripts/test-source-install.sh
    ;;
  product)
    bash scripts/test-product-proofs.sh
    ;;
  splits)
    bash tests/split-export-hostiles.sh
    bash scripts/test-split-exports.sh
    ;;
  oci)
    bash components/jeryu-ci-runner/scripts/test-oci.sh
    ;;
  security)
    bash scripts/bootstrap-ci-tools.sh
    npm ci
    npm audit --audit-level=high
    cargo audit --deny warnings --file Cargo.lock
    cargo deny check
    gitleaks git --redact=100 --log-opts=HEAD
    actionlint .github/workflows/ci.yml
    zizmor .github/workflows/ci.yml
    mkdir -p target/security
    # This is the source inventory; caches and Git objects are not source inputs.
    # The native CycloneDX lane uses the same exclusions. Release archives need
    # their own artifact inventory before publication.
    syft dir:. --exclude './target/**' --exclude './.git/**' \
      --parallelism "$CARGO_BUILD_JOBS" --source-name jeryu \
      --source-version "$(git rev-parse HEAD)" \
      -o spdx-json=target/security/jeryu.spdx.json
    jq -e '.spdxVersion == "SPDX-2.3" and (.packages | type == "array" and length > 0)' \
      target/security/jeryu.spdx.json >/dev/null
    ;;
  sandbox)
    # Run only in a disposable Linux environment with the required privileges.
    [[ ${JERYU_DISPOSABLE_SANDBOX:-0} == 1 ]] || {
      printf 'sandbox lane requires JERYU_DISPOSABLE_SANDBOX=1 on a disposable Linux host\n' >&2; exit 1;
    }
    mkdir -p target/ci
    cargo run --locked -p jeryu-sandbox-linux --example required_capabilities
    sandbox_receipt_dir=$(mktemp -d "$root/target/ci/sandbox-receipt.XXXXXXXX")
    [[ -O $sandbox_receipt_dir && ! -L $sandbox_receipt_dir &&
       $(realpath -e -- "$sandbox_receipt_dir") == "$sandbox_receipt_dir" ]] || exit 1
    JERYU_SANDBOX_ENFORCEMENT_DIR="$sandbox_receipt_dir" \
      cargo test --locked -p jeryu-sandbox-linux --all-features -- --include-ignored --nocapture --test-threads=1 2>&1 | tee target/ci/sandbox.log
    # Only the escape producer writes here. A previous cached receipt cannot
    # satisfy this invocation, and zero executed producer tests cannot pass.
    [[ $(rg -F -x -c -- "enforcement receipt: $sandbox_receipt_dir/enforcement.json" target/ci/sandbox.log) == 1 &&
       -f $sandbox_receipt_dir/enforcement.json && ! -L $sandbox_receipt_dir/enforcement.json ]] || {
      printf 'sandbox escape producer did not publish this invocation receipt\n' >&2; exit 1
    }
    # Ordinary hosts may return early from these eight tests. Require their
    # actual execution here; the paid external-model smoke remains separate.
    sandbox_logs=(target/ci/sandbox.log)
    for test_target in driver_in_cell pty_driver cgroup_fail_closed; do
      test_filter=()
      filtered=0
      case $test_target in
        driver_in_cell) expected_tests=4 ;;
        pty_driver) expected_tests=3 ;;
        cgroup_fail_closed)
          expected_tests=1
          filtered=1
          test_filter=(--exact opt_out_driver_runs_on_this_no_delegation_host)
          ;;
      esac
      test_log="target/ci/agentbridge-$test_target.log"
      cargo test --locked -p jeryu-agentbridge --test "$test_target" -- \
        "${test_filter[@]}" --nocapture --test-threads=1 2>&1 | tee "$test_log"
      [[ $(rg -c '^test result:' "$test_log") == 1 ]] &&
        rg -q "^test result: ok\. $expected_tests passed; 0 failed; 0 ignored; 0 measured; $filtered filtered out;" "$test_log" || {
          printf 'Agentbridge %s did not execute its %s required cases\n' "$test_target" "$expected_tests" >&2
          exit 1
        }
      sandbox_logs+=("$test_log")
    done
    skip_status=0
    rg -i '(^|[[:space:]])skip[:[:space:]]|skipping|=> skipped|honestly skipped|"skipped"[[:space:]]*:[[:space:]]*[1-9]|[1-9][0-9]* ignored' "${sandbox_logs[@]}" || skip_status=$?
    [[ $skip_status == 1 ]] || {
      printf 'sandbox proof was skipped or its output could not be checked\n' >&2; exit 1
    }
    jq -e '.false_skips == 0 and (.escapes | length) == 4 and all(.escapes[]; .verdict == "blocked")' \
      "$sandbox_receipt_dir/enforcement.json" >/dev/null
    ;;
  legacy)
    # Matrix jobs and direct local lanes have independent prerequisite state.
    bash scripts/bootstrap-ci-tools.sh --legacy
    web_build
    source scripts/bootstrap-jankurai.sh
    bootstrap_public_jankurai
    export JERYU_TOOL_RENDER="$root/components/jeryu-tool/ops/ci/check-rendered-identity.sh"
    # Keep the original proof union active until each replacement is verified.
    bash ops/ci/pr-ci.sh
    for component in components/*; do
      if [[ -f "$component/scripts/ci-local.sh" ]]; then
        (cd "$component" && bash scripts/ci-local.sh required)
      else
        (cd "$component" && bash ops/ci/pr-ci.sh)
      fi
    done
    ;;
  all)
    source scripts/ci-lanes.sh
    jeryu_ci_all "$0"
    ;;
  *) printf 'usage: scripts/ci.sh {source|public|auditor|auxiliary|rust|runner-governed|web|runtime|product|security|sandbox|oci|splits|legacy|redline|all}\n' >&2; exit 2 ;;
esac
