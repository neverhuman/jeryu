#!/usr/bin/env bash
# Generated split check entrypoint. Full source qualification belongs to the monorepo.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
export CI=true
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
jq -e '.schema_version == "jeryu.split-provenance/v1" and .lock_regeneration_required == false' .jeryu-source.json >/dev/null
component=$(jq -r .component .jeryu-source.json)
lane=${1:-ordinary}
if (( $# )); then shift; fi
prepare_local='' prepare_before='' export_before='' source_commit='' component_tree=''
if (( $# )); then
  [[ $# == 2 && $1 == --prepare-local && $lane == ordinary ]] || exit 2
  prepare_local=$2
  # shellcheck source=scripts/source-build.sh
  source scripts/source-build.sh
  split_root=$(pwd -P)
  export_head=$(split_source_git "$split_root" rev-parse HEAD) || exit 1
  export_before=$(split_source_snapshot "$split_root" "$export_head") || exit 1
  split_source_git "$split_root" cat-file -e HEAD:.jeryu-source.json || exit 1
  selection=$(jq -ser '
    if length != 1 then error("one split provenance required") else .[0] end
    | select(.schema_version=="jeryu.split-provenance/v1" and .lock_regeneration_required==false
      and (.component|type)=="string" and (.component|test("^jeryu-[a-z-]+$"))
      and (.source_commit|type)=="string" and (.source_commit|test("^[0-9a-f]{40}$"))
      and (.original_component_tree|type)=="string" and (.original_component_tree|test("^[0-9a-f]{40}$")))
    | [.source_commit,.original_component_tree] | @tsv' .jeryu-source.json) || exit 1
  IFS=$'\t' read -r source_commit component_tree <<< "$selection"
  prepare_before=$(split_source_snapshot "$prepare_local" "$source_commit" "$component" "$component_tree") || exit 1
fi
split_finish_local() {
  local result=$1 observed
  if [[ -n $prepare_local ]]; then
    observed=$(split_source_snapshot "$prepare_local" "$source_commit" "$component" "$component_tree") || result=1
    [[ $observed == "$prepare_before" ]] || result=1
    observed=$(split_source_snapshot "$split_root" "$export_head") || result=1
    [[ $observed == "$export_before" ]] || result=1
  fi
  return "$result"
}
if [[ -n $prepare_local ]]; then
  finish_local() {
    local result=$?
    trap - EXIT
    split_finish_local "$result" || result=$?
    exit "$result"
  }
  trap finish_local EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
fi
# These components need only ordinary Cargo commands. Re-enter the unchanged
# public dispatch once under verified, invocation-scoped local transport; the
# outer trap still checks the raw source and generated tree after every outcome.
if [[ -n $prepare_local && $lane == ordinary &&
      ( $component == jeryu-cache || $component == jeryu-core || $component == jeryu-intelligence ) ]]; then
  split_source_run "$prepare_local" "$source_commit" "$component" "$component_tree" \
    bash "$split_root/scripts/split-ci.sh" ordinary
  exit 0
fi
split_cargo() {
  if [[ -n $prepare_local ]]; then
    split_source_run "$prepare_local" "$source_commit" "$component" "$component_tree" cargo "$@"
  else
    cargo "$@"
  fi
}
case $lane in
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
        # The helper owns the exact public Web source, physical bundle and cleanup.
        # shellcheck source=ops/ci/web-build.sh
        source ops/ci/web-build.sh
        finish_web() {
          local result=$?
          trap - EXIT
          jeryu_web_finish "$result" || { if (( result == 0 )); then result=1; fi; }
          split_finish_local "$result" || result=$?
          exit "$result"
        }
        trap finish_web EXIT
        trap 'exit 130' INT
        trap 'exit 143' TERM
        if [[ -n $prepare_local ]]; then
          jeryu_web_begin "$(pwd -P)" --prepare-local "$prepare_local"
        else
          jeryu_web_begin "$(pwd -P)"
        fi
      fi
      split_cargo fmt --all -- --check
      split_cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
      excluded=()
      if [[ $component == jeryu-ci-runner ]]; then excluded=(--exclude jeryu-sandbox-linux); fi
      split_cargo test --locked --workspace --all-features "${excluded[@]}"
      split_cargo build --locked --workspace
      if [[ $component == jeryu-deploy ]]; then
        jeryu_web_finish 0
        split_finish_local 0
        trap - EXIT
      fi
    fi
    ;;
  oci)
    [[ $component == jeryu-ci-runner ]] || { printf 'OCI proof requires runner component\n' >&2; exit 1; }
    bash scripts/test-oci.sh
    ;;
  sandbox)
    [[ $component == jeryu-ci-runner ]] || {
      printf 'sandbox proof requires the runner component\n' >&2; exit 1;
    }
    bash scripts/test-native-sandbox.sh --workspace-root "$(pwd -P)"
    ;;
  *) printf 'usage: bash scripts/split-ci.sh [ordinary [--prepare-local SOURCE]|sandbox|oci]\n' >&2; exit 2 ;;
esac
