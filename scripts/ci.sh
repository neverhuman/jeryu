#!/usr/bin/env bash
# Local and hosted CI use this same entrypoint. Every command is required.
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
    cargo run --locked -p jeryu-split-tool --bin jeryu-split -- monorepo-check
    cargo run --locked -p jeryu-split-tool --bin jeryu-split -- manifest --check-paths
    cargo run --locked -p jeryu-split-tool --bin jeryu-split -- proof-inventory --check
    ;;
  public)
    cargo run --locked -p jeryu-split-tool --bin jeryu-split -- public-preflight
    ;;
  rust)
    web_build
    cargo fmt --all -- --check
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    cargo test --locked --workspace --all-features --exclude jeryu-sandbox-linux
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
  splits)
    bash scripts/test-split-exports.sh
    ;;
  security)
    bash scripts/bootstrap-ci-tools.sh
    npm ci
    npm audit --audit-level=high
    cargo audit --file Cargo.lock
    cargo deny check
    gitleaks git --redact=100 --log-opts=HEAD
    actionlint .github/workflows/ci.yml
    zizmor .github/workflows/ci.yml
    mkdir -p target/security
    syft dir:. -o spdx-json=target/security/jeryu.spdx.json
    ;;
  sandbox)
    # Run only in a disposable Linux environment with the required privileges.
    [[ ${JERYU_DISPOSABLE_SANDBOX:-0} == 1 ]] || {
      printf 'sandbox lane requires JERYU_DISPOSABLE_SANDBOX=1 on a disposable Linux host\n' >&2; exit 1;
    }
    mkdir -p target/ci
    cargo run --locked -p jeryu-sandbox-linux --example required_capabilities
    cargo test --locked -p jeryu-sandbox-linux --all-features -- --include-ignored --nocapture --test-threads=1 2>&1 | tee target/ci/sandbox.log
    if rg -i '(^|[[:space:]])skip[:[:space:]]|skipping|=> skipped|"skipped"[[:space:]]*:[[:space:]]*[1-9]|[1-9][0-9]* ignored' target/ci/sandbox.log; then
      printf 'sandbox proof did not execute every required case\n' >&2; exit 1
    fi
    jq -e '.false_skips == 0 and (.escapes | length) == 4 and all(.escapes[]; .verdict == "blocked")' \
      components/jeryu-ci-runner/target/jankurai/runner-sandbox/enforcement.json >/dev/null
    ;;
  legacy)
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
    for lane in source public rust web runtime security sandbox splits legacy; do "$0" "$lane"; done
    ;;
  *) printf 'usage: scripts/ci.sh {source|public|rust|web|runtime|security|sandbox|splits|legacy|all}\n' >&2; exit 2 ;;
esac
