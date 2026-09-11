#!/usr/bin/env bash
# Receives the API inventory from a separate completed credential-bearing step.
set -euo pipefail
[[ $# == 2 && -z ${GH_TOKEN:-} && -z ${GITHUB_TOKEN:-} ]] || {
  printf 'usage: scripts/check-required-ci.sh RUN_JSON JOB_PAGES_JSON (without API credentials)\n' >&2
  exit 2
}
[[ ${RESULT:-} == success ]] || { printf 'A required proof failed\n' >&2; exit 1; }
[[ ${GITHUB_REPOSITORY:-} == neverhuman/jeryu &&
   ${GITHUB_RUN_ID:-} =~ ^[1-9][0-9]*$ &&
   ${GITHUB_RUN_ATTEMPT:-} =~ ^[1-9][0-9]*$ &&
   ${JERYU_CI_SOURCE:-} =~ ^[0-9a-f]{40}$ ]] || {
  printf 'Missing or invalid workflow identity\n' >&2
  exit 1
}
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
[[ $(git -C "$root" rev-parse HEAD) == "$JERYU_CI_SOURCE" ]] || {
  printf 'Aggregate source does not match the verified source\n' >&2
  exit 1
}
CARGO_BUILD_JOBS=2 cargo run --locked \
  --manifest-path "$root/Cargo.toml" -p jeryu-split-tool --bin jeryu-split -- \
  ci-required-check --run "$1" --jobs "$2" \
  --repository "$GITHUB_REPOSITORY" --source "$JERYU_CI_SOURCE" \
  --run-id "$GITHUB_RUN_ID" --attempt "$GITHUB_RUN_ATTEMPT"
