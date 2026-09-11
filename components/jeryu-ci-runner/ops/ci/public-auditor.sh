#!/usr/bin/env bash
# Runner export acquisition. The Tool verifier retains the complete authority checks.
# shellcheck disable=SC1090,SC1091,SC2317

runner_public_descriptor() {
  [[ -f $1/.jeryu-source.json && ! -L $1/.jeryu-source.json ]] || return 1
  jq -ser '
    if length != 1 then error("one exported source descriptor required") else .[0] end
    | select(.schema_version=="jeryu.split-provenance/v1" and .component=="jeryu-ci-runner"
      and .lock_regeneration_required==false
      and (.source_commit|type)=="string" and (.source_commit|test("^[0-9a-f]{40}$"))
      and (.original_component_tree|type)=="string"
      and (.original_component_tree|test("^[0-9a-f]{40}$")))
    | [.source_commit,.original_component_tree] | @tsv' "$1/.jeryu-source.json"
}

runner_public_selection() {
  runner_public_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P) || return 1
  local selected
  selected=$(runner_public_descriptor "$runner_public_root") || return 1
  source "$runner_public_root/scripts/source-build.sh"
  IFS=$'\t' read -r runner_public_revision runner_public_subtree <<< "$selected"
  runner_public_head=$(split_source_git "$runner_public_root" rev-parse HEAD) || return 1
  split_source_git "$runner_public_root" cat-file -e HEAD:.jeryu-source.json || return 1
  runner_public_export_before=$(split_source_snapshot "$runner_public_root" "$runner_public_head") || return 1
}

runner_public_source_binding() {
  local selected=$1 path
  [[ $selected != "$runner_public_root" ]] || return 1
  split_source_snapshot "$selected" "$runner_public_revision" jeryu-ci-runner "$runner_public_subtree" || return 1
  # Bind the actual consumer and its verifier/policy to this acquired Tool source.
  # Other exported product files are tested in their own checkout and CI identity.
  for path in ops/ci/lib.sh ops/ci/public-auditor.sh agent/audit-policy.toml; do
    cmp -- "$runner_public_root/$path" "$selected/components/jeryu-ci-runner/$path" || return 1
  done
  for path in scripts/source-build.sh rust-toolchain.toml; do
    cmp -- "$runner_public_root/$path" "$selected/$path" || return 1
  done
}

require_export_candidate_jankurai() {
  local selected=${JERYU_PUBLIC_AUDITOR_SOURCE_ROOT:-} before after
  runner_public_selection || return 1
  [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 &&
     ${JERYU_MONOREPO_EXPECTED_HEAD:-} == "$runner_public_revision" ]] || return 1
  before=$(runner_public_source_binding "$selected") || return 1
  source "$selected/components/jeryu-tool/ops/verify-public-candidate.sh"
  require_public_candidate_jankurai
  after=$(runner_public_source_binding "$selected") || return 1
  [[ $before == "$after" &&
     $(split_source_snapshot "$runner_public_root" "$runner_public_head") == "$runner_public_export_before" ]] || return 1
}

runner_public_run() (
  set -euo pipefail
  umask 077
  local prepare='' prepare_before='' source_url=https://github.com/neverhuman/jeryu.git
  if [[ ${1:-} == --prepare-local ]]; then
    [[ $# -ge 3 ]] || exit 2
    prepare=$2; shift 2
  fi
  [[ $# -ge 2 && $1 == -- ]] || { printf 'usage: public-auditor.sh [--prepare-local SOURCE] -- COMMAND [ARG...]\n' >&2; exit 2; }
  shift
  [[ $(id -u) != 0 ]] || { printf 'public Runner qualification requires an unprivileged user\n' >&2; exit 1; }
  runner_public_selection || { printf 'invalid or changed Runner export source\n' >&2; exit 1; }
  if [[ -n $prepare ]]; then
    prepare_before=$(split_source_snapshot "$prepare" "$runner_public_revision" jeryu-ci-runner "$runner_public_subtree") || exit 1
    source_url="file://$prepare"
    printf 'Runner auditor: local-source-preparation; public source acquisition unproven\n' >&2
  fi
  for dependency in git jq cargo rustup docker lsof timeout; do
    command -v "$dependency" >/dev/null || { printf 'Runner auditor requires %s\n' "$dependency" >&2; exit 1; }
  done
  source "$runner_public_root/tests/scratch.sh"
  local scratch active=0
  scratch=$(mktemp -d -t jeryu-runner-auditor.XXXXXXXX)
  jeryu_record_test_scratch "$scratch" || { printf 'retained unadmitted Runner auditor scratch: %s\n' "$scratch" >&2; exit 1; }
  runner_public_finish() {
    local result=$? fd probe probe_status=0
    trap - EXIT
    # Close our Tool-held descriptors before proving no process retains scratch.
    for fd in "${JERYU_CANDIDATE_HELD_FDS[@]:-}"; do
      [[ -z $fd ]] || exec {fd}<&-
    done
    if (( result == 0 && active == 0 )); then
      probe=$(timeout --kill-after=2s 10 lsof -t +D "$scratch" 2>&1) || probe_status=$?
      if [[ $probe_status == 1 && -z $probe ]]; then
        jeryu_remove_test_scratch || result=1
      else
        result=1
      fi
    else
      (( result != 0 )) || result=1
    fi
    if (( result != 0 )); then printf 'retained failed or uncertain Runner auditor attempt: %s\n' "$scratch" >&2; fi
    exit "$result"
  }
  trap runner_public_finish EXIT
  trap 'exit 129' HUP
  trap 'exit 130' INT
  trap 'exit 143' TERM
  active=1
  split_source_git "$scratch" clone --no-local --no-checkout --quiet "$source_url" "$scratch/source"
  split_source_git "$scratch/source" fetch --quiet --no-tags origin "$runner_public_revision"
  split_source_git "$scratch/source" -c core.hooksPath=/dev/null checkout --quiet --detach "$runner_public_revision"
  active=0
  local source_before
  source_before=$(runner_public_source_binding "$scratch/source")
  export JERYU_PUBLIC_AUDITOR_SOURCE_ROOT="$scratch/source"
  export JERYU_AUDITOR_INSTALL_ROOT="$scratch/auditor"
  source "$scratch/source/scripts/bootstrap-jankurai.sh"
  active=1
  bootstrap_public_jankurai
  active=0
  require_export_candidate_jankurai
  printf 'Runner auditor verified: export=%s source=%s binary_sha256=%s receipt_sha256=%s\n' \
    "$runner_public_head" "$runner_public_revision" \
    "$(sha256sum "$JERYU_GOVERNED_JANKURAI_BIN" | cut -d ' ' -f1)" "$JERYU_JANKURAI_RECEIPT_SHA256"
  local result=0
  active=1
  if [[ -n $prepare ]]; then
    split_source_run "$prepare" "$runner_public_revision" jeryu-ci-runner "$runner_public_subtree" "$@" || result=$?
  else
    "$@" || result=$?
  fi
  active=0
  # Recheck source even after a failed test; failure and uncertain custody persist.
  [[ $(runner_public_source_binding "$scratch/source") == "$source_before" ]] || result=1
  [[ $(split_source_snapshot "$runner_public_root" "$runner_public_head") == "$runner_public_export_before" ]] || result=1
  if [[ -n $prepare ]]; then
    [[ $(split_source_snapshot "$prepare" "$runner_public_revision" jeryu-ci-runner "$runner_public_subtree") == "$prepare_before" ]] || result=1
  fi
  exit "$result"
)

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  # Public acquisition has no credential, caller Git routing or auditor override.
  # Keep only the actual compiler/cache/process paths required by this command.
  [[ ${JAIN_RELEASE_CI:-0} != 1 ]] || exit 1
  runner_public_env=()
  for runner_public_name in HOME PATH CARGO_HOME RUSTUP_HOME CARGO_TARGET_DIR TMPDIR; do
    [[ ! -v $runner_public_name ]] || runner_public_env+=("$runner_public_name=${!runner_public_name}")
  done
  # The new shell expands its own positional arguments after the environment closes.
  # shellcheck disable=SC2016
  exec env -i "${runner_public_env[@]}" CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 CI=true \
    /bin/bash -euo pipefail -c 'source "$1"; shift; runner_public_run "$@"' runner-public-auditor "$0" "$@"
fi
