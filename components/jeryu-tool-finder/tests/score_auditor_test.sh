#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
score="$repo_root/ops/ci/score.sh"
# shellcheck source=/dev/null
source "$repo_root/tests/scratch.sh"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/jeryu-tool-finder-auditor.XXXXXX")"
jeryu_record_test_scratch "$test_root"
fake="$test_root/fake-jankurai"
marker="$test_root/fake-executed"
stderr_log="$test_root/stderr.log"
evidence="$repo_root/target/jankurai/evidence.json"
saved=''

cleanup() {
  local status=$?
  if [[ -n "$saved" && -f "$saved" ]]; then
    rm -f -- "$evidence" || exit 1
    mv -- "$saved" "$evidence" || exit 1
  elif [[ -f "$evidence" ]]; then
    rm -- "$evidence" || exit 1
  fi
  jeryu_remove_test_scratch || status=1
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

mkdir -p -- "$repo_root/target/jankurai"
if [[ -f "$evidence" && ! -L "$evidence" ]]; then
  saved="$test_root/evidence.saved"
  mv -- "$evidence" "$saved"
fi
printf '{"sentinel":"preserve-me"}\n' > "$evidence"
sentinel_sha="$(sha256sum -- "$evidence" | awk '{print $1}')"

{
  printf '%s\n' '#!/usr/bin/env bash'
  printf 'printf fake > %q\n' "$marker"
  printf '%s\n' 'exit 0'
} > "$fake"
chmod 0755 "$fake"

expect_rejected() {
  local mode="$1"
  shift
  if env "$@" JERYU_JANKURAI_BIN="$fake" bash "$score" > /dev/null 2> "$stderr_log"; then
    printf 'score auditor hostile test failed: %s override passed\n' "$mode" >&2
    exit 1
  fi
  [[ ! -e "$marker" ]] || {
    printf 'score auditor hostile test failed: %s fake executed\n' "$mode" >&2
    exit 1
  }
  [[ "$(sha256sum -- "$evidence" | awk '{print $1}')" == "$sentinel_sha" ]] || {
    printf 'score auditor hostile test failed: %s destroyed prior evidence\n' "$mode" >&2
    exit 1
  }
}

expect_rejected local JAIN_RELEASE_CI=0
expect_rejected release JAIN_RELEASE_CI=1

grep -Fq 'jeryu_with_scrubbed_git "$auditor_bin" audit' "$score"
if grep -Fq '"$JANKURAI_BIN" audit .' "$score"; then
  printf 'score auditor hostile test failed: score still executes the legacy alias\n' >&2
  exit 1
fi
if grep -Eq '^[[:space:]]*"\$auditor_bin"[[:space:]]+audit[[:space:]]' "$score"; then
  printf 'score auditor hostile test failed: score still launches auditor under ambient Git\n' >&2
  exit 1
fi

printf 'score auditor hostile tests ok\n'
