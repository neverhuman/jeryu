#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
score="$repo_root/ops/ci/score.sh"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/jeryu-tool-finder-auditor.XXXXXX")"
fake="$test_root/fake-jankurai"
marker="$test_root/fake-executed"
stderr_log="$test_root/stderr.log"
evidence="$repo_root/target/jankurai/evidence.json"
saved=''

cleanup() {
  if [[ -n "$saved" && -f "$saved" ]]; then
    rm -f -- "$evidence"
    mv -- "$saved" "$evidence"
  elif [[ -f "$evidence" ]]; then
    rm -- "$evidence"
  fi
  rm -rf -- "$test_root"
}
trap cleanup EXIT

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
