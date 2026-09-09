#!/usr/bin/env bash
# Generated split check entrypoint. Full source qualification belongs to the monorepo.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
export CI=true
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
jq -e '.schema_version == "jeryu.split-provenance/v1" and .lock_regeneration_required == false' .jeryu-source.json >/dev/null
component=$(jq -r .component .jeryu-source.json)
case ${1:-ordinary} in
  ordinary)
    if [[ $component == jeryu-web ]]; then
      npm ci
      npm run lint
      npm run typecheck
      npm test
      npm --workspace @jeryu/web run test:contracts
      npm run fixtures:test
      npm run build
      npm --workspace @jeryu/web run build-storybook
      npm --workspace @jeryu/web exec -- playwright install chromium
      JERYU_PLAYWRIGHT_E2E_MODE=ui-only npm --workspace @jeryu/web run test:e2e
      npm --workspace @jeryu/web run test:e2e:matrix
      npm audit --audit-level=high
    else
      if [[ $component == jeryu-deploy ]]; then
        # The embedded browser is an explicit dependency of this integration
        # component. Build it from the same immutable public source revision.
        source_commit=$(jq -er .source_commit .jeryu-source.json)
        [[ $source_commit =~ ^[0-9a-f]{40}$ ]] || { printf 'invalid source commit\n' >&2; exit 1; }
        scratch=$(mktemp -d)
        scratch_identity=$(stat -c '%d:%i' -- "$scratch")
        cleanup() {
          local result=$? mounts
          mounts=$(findmnt -rn -o TARGET) || { printf 'cannot inspect mounts; retaining %s\n' "$scratch" >&2; exit 1; }
          if [[ -L "$scratch" || ! -d "$scratch" || $(realpath -e -- "$scratch") != "$scratch" \
                || $(stat -c '%d:%i' -- "$scratch") != "$scratch_identity" ]] \
              || awk -v root="$scratch" '$0 == root || index($0, root "/") == 1 {found=1} END {exit !found}' <<< "$mounts"; then
            printf 'retaining replaced or mounted source scratch: %s\n' "$scratch" >&2
            exit 1
          fi
          find "$scratch" -xdev -type l -print >&2
          rm -rf --one-file-system --preserve-root=all -- "$scratch"
          exit "$result"
        }
        trap cleanup EXIT
        git clone --no-local --no-checkout --quiet https://github.com/neverhuman/jeryu.git "$scratch/source"
        git -C "$scratch/source" fetch --quiet --no-tags origin "$source_commit"
        git -C "$scratch/source" -c core.hooksPath=/dev/null checkout --quiet --detach "$source_commit"
        [[ $(git -C "$scratch/source" rev-parse HEAD) == "$source_commit" ]]
        (cd "$scratch/source" && npm ci && npm run build)
        export JERYU_WEB_DIST="$scratch/source/components/jeryu-web/apps/web/dist"
        export JERYU_REQUIRE_WEB=1
      fi
      cargo fmt --all -- --check
      cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
      excluded=()
      if [[ $component == jeryu-ci-runner ]]; then excluded=(--exclude jeryu-sandbox-linux); fi
      cargo test --locked --workspace --all-features "${excluded[@]}"
      cargo build --locked --workspace
    fi
    ;;
  oci)
    [[ $component == jeryu-ci-runner ]] || { printf 'OCI proof requires runner component\n' >&2; exit 1; }
    bash scripts/test-oci.sh
    ;;
  sandbox)
    [[ $component == jeryu-ci-runner && ${JERYU_DISPOSABLE_SANDBOX:-0} == 1 ]] || {
      printf 'sandbox proof requires the runner component on a disposable capable Linux host\n' >&2; exit 1;
    }
    cargo run --locked -p jeryu-sandbox-linux --example required_capabilities
    mkdir -p target/ci
    cargo test --locked -p jeryu-sandbox-linux --all-features -- --include-ignored --nocapture --test-threads=1 2>&1 | tee target/ci/sandbox.log
    if rg -i '(^|[[:space:]])skip[:[:space:]]|skipping|=> skipped|[1-9][0-9]* ignored' target/ci/sandbox.log; then exit 1; fi
    jq -e '.false_skips == 0 and (.escapes | length) == 4 and all(.escapes[]; .verdict == "blocked")' \
      target/jankurai/runner-sandbox/enforcement.json >/dev/null
    ;;
  *) printf 'usage: bash scripts/split-ci.sh [ordinary|sandbox|oci]\n' >&2; exit 2 ;;
esac
