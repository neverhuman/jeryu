#!/usr/bin/env bash
# Exercise the actual bootstrap function with shell-only dependencies. No Cargo,
# Git, installer, auditor, compiler or network command is executed by these tests.
set -euo pipefail
source_root=${2:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)}
helper=${1:-$source_root/scripts/bootstrap-jankurai.sh}
helper=$(realpath -e -- "$helper")
# shellcheck source=/dev/null
source "$source_root/tests/scratch.sh"
umask 077
temporary=$(mktemp -d -t jeryu-bootstrap-hostiles.XXXXXXXX)
jeryu_record_test_scratch "$temporary"
cleanup() {
  local result=$?
  trap - EXIT
  jeryu_remove_test_scratch || result=1
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
passed=0
head=1111111111111111111111111111111111111111
receipt_sha=2222222222222222222222222222222222222222222222222222222222222222

run_case() (
  trap - EXIT
  local scenario=$1 expected_status=$2 expected_calls=$3
  local calls="$temporary/$scenario.calls" status=0
  local install="$temporary/install" actual
  : >"$calls"
  # Source the maintained function unchanged. Redirect only its root lookup so
  # an artifact afterimage can be exercised without a copied checkout layout.
  builtin source "$helper"
  dirname() {
    [[ $# == 2 && $1 == -- && $2 == "$helper" ]] || return 91
    printf '%s/scripts\n' "$source_root"
  }
  env() {
    if [[ $# -ge 9 && $1 == -i && $2 == PATH=/usr/bin:/bin &&
          $3 == GIT_CONFIG_GLOBAL=/dev/null && $4 == GIT_CONFIG_NOSYSTEM=1 &&
          $5 == /usr/bin/git && $6 == -C && $7 == "$source_root" ]]; then
      case "$8" in
        rev-parse)
          [[ $# == 9 && $9 == HEAD ]] || return 92
          printf 'head\n' >>"$calls"
          [[ $scenario != head-read-failure ]] || return 21
          if [[ $scenario == malformed-head ]]; then printf 'HEAD\n'; else printf '%s\n' "$head"; fi
          ;;
        status)
          [[ $# == 10 && $9 == --porcelain=v1 && ${10} == --untracked-files=all ]] || return 93
          printf 'status\n' >>"$calls"
          [[ $scenario != status-read-failure ]] || return 22
          [[ $scenario != dirty-source ]] || printf ' M Cargo.toml\n'
          ;;
        *) return 94 ;;
      esac
    elif [[ $# == 10 && $1 == -u && $2 == JANKURAI_NO_UPDATE_CHECK &&
            $3 == -u && $4 == GIT_TERMINAL_PROMPT && $5 == "JERYU_INSTALL_ROOT=$install" &&
            $6 == bash && $7 == "$source_root/components/jeryu-tool/ops/install-jankurai.sh" &&
            $8 == --public-candidate && $9 == --expected-head && ${10} == "$head" ]]; then
      printf 'install\n' >>"$calls"
      [[ $scenario != install-failure ]] || return 26
      printf '{"receipt":"%s/receipts/jankurai/sha256/%s.json"}\n' "$install" "$receipt_sha"
    else
      printf 'unexpected synthetic env dispatch\n' >&2
      return 95
    fi
  }
  cargo() {
    [[ $PWD == "$source_root" && $# == 4 && $1 == fetch && $2 == --locked &&
       $3 == --manifest-path && $4 == "$source_root/Cargo.toml" &&
       ${GIT_CONFIG_GLOBAL:-} == /dev/null && ${GIT_CONFIG_NOSYSTEM:-} == 1 ]] || return 96
    printf 'fetch\n' >>"$calls"
    [[ $scenario != fetch-failure ]] || return 23
  }
  bash() {
    [[ $# == 6 && $1 == "$source_root/components/jeryu-tool/ops/render-monorepo-candidate.sh" &&
       $2 == --monorepo-root && $3 == "$source_root" && $4 == --check &&
       $5 == --expected-head && $6 == "$head" ]] || return 98
    printf 'renderer\n' >>"$calls"
    [[ $scenario != renderer-failure && $scenario != drift-after-fetch ]] || return 24
  }
  source() {
    [[ $# == 1 && $1 == "$source_root/components/jeryu-tool/ops/verify-public-candidate.sh" ]] || return 99
    printf 'verifier-source\n' >>"$calls"
  }
  require_public_candidate_jankurai() {
    printf 'verify\n' >>"$calls"
  }
  rustup() {
    [[ $# == 4 && $1 == run && $2 =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && $3 == cargo && $4 == --version ]] || return 101
    printf 'toolchain\n' >>"$calls"
  }
  unset JAIN_RELEASE_CI JERYU_MONOREPO_CANDIDATE JERYU_MONOREPO_EXPECTED_HEAD \
    JERYU_GOVERNED_JANKURAI_BIN JERYU_JANKURAI_RECEIPT JERYU_CANDIDATE_JANKURAI_DESCRIPTOR
  export JERYU_AUDITOR_INSTALL_ROOT=$install
  case $scenario in
    release-refusal) export JAIN_RELEASE_CI=1 ;;
    reuse|fetch-failure|renderer-failure|drift-after-fetch)
      export JERYU_MONOREPO_CANDIDATE=1 JERYU_MONOREPO_EXPECTED_HEAD=$head \
        JERYU_GOVERNED_JANKURAI_BIN="$install/bin/jankurai" \
        JERYU_JANKURAI_RECEIPT="$install/receipts/jankurai/sha256/$receipt_sha.json"
      ;;
    held-other-candidate) export JERYU_CANDIDATE_JANKURAI_DESCRIPTOR=/fixture/held ;;
  esac
  # The caller can start outside the checkout. Every failure assertion uses a
  # conditional call, so the function must explicitly propagate failures.
  cd "$temporary"
  bootstrap_public_jankurai >"$temporary/$scenario.out" 2>&1 || status=$?
  actual=$(cat "$calls")
  [[ $status == "$expected_status" && $actual == "$expected_calls" ]] || {
    printf 'bootstrap %s: expected status=%s calls=%q; got status=%s calls=%q\n' \
      "$scenario" "$expected_status" "$expected_calls" "$status" "$actual" >&2
    return 1
  }
)

for scenario in reuse fresh; do
  if [[ $scenario == reuse ]]; then
    calls=$'head\nstatus\nfetch\nrenderer\nverifier-source\nverify'
  else
    calls=$'head\nstatus\nfetch\nrenderer\ntoolchain\ninstall\nverifier-source\nverify'
  fi
  run_case "$scenario" 0 "$calls"
  passed=$((passed+1))
done
run_case release-refusal 1 ''; passed=$((passed+1))
run_case head-read-failure 1 head; passed=$((passed+1))
for scenario in dirty-source malformed-head status-read-failure; do
  run_case "$scenario" 1 $'head\nstatus'; passed=$((passed+1))
done
run_case fetch-failure 1 $'head\nstatus\nfetch'; passed=$((passed+1))
for scenario in renderer-failure drift-after-fetch; do
  run_case "$scenario" 1 $'head\nstatus\nfetch\nrenderer'; passed=$((passed+1))
done
run_case held-other-candidate 1 $'head\nstatus\nfetch\nrenderer'; passed=$((passed+1))
run_case install-failure 1 $'head\nstatus\nfetch\nrenderer\ntoolchain\ninstall'; passed=$((passed+1))
printf '%s bootstrap ordering/failure cases passed (synthetic; no actual Cargo or installation).\n' "$passed"
