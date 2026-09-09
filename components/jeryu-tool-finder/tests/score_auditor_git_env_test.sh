#!/usr/bin/env bash
# Prove the governed auditor is spawned under scrubbed Git authority and that
# foreign ambient GIT_* cannot bind a foreign head into a passing score report.
# Local and release modes both preserve prior evidence on identity reject.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
score="$repo_root/ops/ci/score.sh"
authority="$repo_root/ops/ci/source-authority.sh"
lib="$repo_root/ops/ci/lib.sh"
# shellcheck source=/dev/null
source "$repo_root/tests/scratch.sh"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/jeryu-tool-finder-auditor-git.XXXXXX")"
jeryu_record_test_scratch "$test_root"
foreign="$test_root/foreign"
stderr_log="$test_root/stderr.log"
evidence="$repo_root/target/jankurai/evidence.json"
saved=''
evidence_owned=false

cleanup() {
  local status=$?
  if [[ -n "${saved:-}" && -f "$saved" ]]; then
    rm -f -- "$evidence" || exit 1
    mv -- "$saved" "$evidence" || exit 1
  elif [[ "$evidence_owned" == true && -f "$evidence" ]]; then
    rm -- "$evidence" || exit 1
  fi
  jeryu_remove_test_scratch || status=1
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

fail() {
  printf 'score auditor git-env hostile test failed: %s\n' "$1" >&2
  if [[ -f "$stderr_log" ]]; then
    sed -n '1,40p' "$stderr_log" >&2 || true
  fi
  exit 1
}

grep -Fq 'jeryu_with_scrubbed_git "$auditor_bin" audit' "$score" ||
  fail 'score does not launch the auditor under scrubbed Git'
if grep -Eq '^[[:space:]]*"\$auditor_bin"[[:space:]]+audit[[:space:]]' "$score"; then
  fail 'score still launches the auditor under ambient Git'
fi
grep -Fq 'jeryu_require_score_report_matches_source' "$score" ||
  fail 'score does not reject mismatched report Git identity'
grep -Fq 'jeryu_require_score_report_matches_source' "$authority" ||
  fail 'source authority lacks score report identity gate'

mkdir -p -- "$repo_root/target/jankurai"
if [[ -f "$evidence" && ! -L "$evidence" ]]; then
  saved="$test_root/evidence.saved"
  mv -- "$evidence" "$saved"
fi
printf '{"sentinel":"preserve-auditor-git-env"}\n' > "$evidence"
evidence_owned=true
sentinel_sha="$(sha256sum -- "$evidence" | awk '{print $1}')"

# shellcheck source=../ops/ci/source-authority.sh
source "$authority"
jeryu_source_snapshot 2> "$stderr_log" || fail 'bound source snapshot failed'
bound_head="$JERYU_SOURCE_HEAD"
[[ "$bound_head" =~ ^[0-9a-f]{40}$ ]] || fail 'bound source head is malformed'

mkdir -p -- "$foreign"
/usr/bin/git -C "$foreign" init -q
/usr/bin/git -C "$foreign" config user.name fixture
/usr/bin/git -C "$foreign" config user.email fixture@example.invalid
printf 'foreign\n' > "$foreign/README"
/usr/bin/git -C "$foreign" add README
/usr/bin/git -C "$foreign" commit -q -m foreign
foreign_head="$(/usr/bin/git -C "$foreign" rev-parse HEAD)"
foreign_short="$(/usr/bin/git -C "$foreign" rev-parse --short=7 HEAD)"
[[ "$foreign_head" != "$bound_head" ]] || fail 'foreign head collided with bound source'

jq -n \
  --arg head "$foreign_short" \
  '{
    score: 99,
    raw_score: 99,
    caps_applied: [],
    decision: {hard_findings: []},
    repo: ".",
    dirty_worktree: false,
    git: {head: $head, dirty_worktree: false}
  }' > "$test_root/foreign-report.json"
if jeryu_require_score_report_matches_source \
  "$test_root/foreign-report.json" "$bound_head" 2> "$stderr_log"; then
  fail 'foreign report identity was accepted'
fi
actual_sentinel="$(sha256sum -- "$evidence" | awk '{print $1}')"
[[ "$actual_sentinel" == "$sentinel_sha" ]] ||
  fail 'identity reject destroyed prior evidence'

# shellcheck source=../ops/ci/lib.sh
source "$lib"
export JAIN_RELEASE_CI=0
require_jankurai
auditor_bin="$JERYU_GOVERNED_JANKURAI_BIN"

assert_report_bound() {
  local mode="$1" report_path="$2"
  if ! jq -e --arg bound "$bound_head" --arg foreign "$foreign_short" '
    (.git // {}) as $git
    | ($git.head // "") as $head
    | (if ($git | has("dirty_worktree")) then $git.dirty_worktree
       elif has("dirty_worktree") then .dirty_worktree else null end) as $dirty
    | .repo == "."
      and ($head | type) == "string"
      and ($head | test("^[0-9a-f]{7,40}$"))
      and ($bound | startswith($head))
      and $head != $foreign
      and $dirty == false
  ' "$report_path" >/dev/null; then
    printf '%s: score report is not bound to the canonical source\n' "$mode" >&2
    return 1
  fi
}

# Local + release labels exercise the same scrubbed spawn under foreign ambient Git.
for mode in local release; do
  out_json="$test_root/${mode}.repo-score.json"
  out_md="$test_root/${mode}.repo-score.md"
  rm -f -- "$out_json" "$out_md"
  spawn_status=0
  (
    export GIT_DIR="$foreign/.git"
    export GIT_WORK_TREE="$foreign"
    export GIT_INDEX_FILE="$foreign/.git/index"
    export GIT_COMMON_DIR="$foreign/.git"
    export GIT_OBJECT_DIRECTORY="$foreign/.git/objects"
    export GIT_NAMESPACE=foreign
    jeryu_with_scrubbed_git "$auditor_bin" audit . --full --mode advisory \
      --policy agent/audit-policy.toml --json "$out_json" --md "$out_md"
  ) >"$test_root/${mode}.stdout" 2>"$stderr_log" || spawn_status=$?
  [[ "$spawn_status" -eq 0 ]] || fail "${mode}: scrubbed auditor spawn failed under foreign Git"
  [[ -f "$out_json" ]] || fail "${mode}: scrubbed auditor produced no report"
  assert_report_bound "$mode" "$out_json" ||
    fail "${mode}: scrubbed auditor report identity mismatch"
  actual_sentinel="$(sha256sum -- "$evidence" | awk '{print $1}')"
  [[ "$actual_sentinel" == "$sentinel_sha" ]] ||
    fail "${mode}: scrubbed auditor destroyed prior evidence"
  jeryu_require_score_report_matches_source "$out_json" "$bound_head" 2> "$stderr_log" ||
    fail "${mode}: bound scrubbed report failed identity gate"
done

# Full local score under foreign ambient Git must publish bound evidence.
if ! env JAIN_RELEASE_CI=0 \
  GIT_DIR="$foreign/.git" \
  GIT_WORK_TREE="$foreign" \
  GIT_INDEX_FILE="$foreign/.git/index" \
  GIT_COMMON_DIR="$foreign/.git" \
  GIT_OBJECT_DIRECTORY="$foreign/.git/objects" \
  GIT_NAMESPACE=foreign \
  bash "$score" >"$test_root/local.score.stdout" 2>"$stderr_log"; then
  fail 'local: full score failed under foreign ambient Git'
fi
[[ -f "$evidence" && ! -L "$evidence" ]] || fail 'local: score evidence missing after foreign-env run'
evidence_head="$(jq -er '.head' "$evidence")"
[[ "$evidence_head" == "$bound_head" ]] ||
  fail 'local: evidence head drifted under foreign ambient Git'
assert_report_bound local-full "$repo_root/target/jankurai/repo-score.json" ||
  fail 'local: published report identity drifted under foreign ambient Git'

# Release score under foreign Git: broker present => bound success; otherwise
# fail-closed broker refusal with prior evidence preserved (never foreign success).
printf '{"sentinel":"preserve-release-foreign-env"}\n' > "$evidence"
release_sentinel="$(sha256sum -- "$evidence" | awk '{print $1}')"
release_status=0
env JAIN_RELEASE_CI=1 \
  GIT_DIR="$foreign/.git" \
  GIT_WORK_TREE="$foreign" \
  GIT_INDEX_FILE="$foreign/.git/index" \
  GIT_COMMON_DIR="$foreign/.git" \
  GIT_OBJECT_DIRECTORY="$foreign/.git/objects" \
  GIT_NAMESPACE=foreign \
  bash "$score" >"$test_root/release.score.stdout" 2>"$stderr_log" || release_status=$?
if [[ "$release_status" -eq 0 ]]; then
  [[ "${JERYU_MONOREPO_CANDIDATE:-0}" != 1 ]] ||
    fail 'candidate source incorrectly satisfied protected release authority'
  evidence_head="$(jq -er '.head' "$evidence")"
  [[ "$evidence_head" == "$bound_head" ]] ||
    fail 'release: evidence head drifted under foreign ambient Git'
  assert_report_bound release-full "$repo_root/target/jankurai/repo-score.json" ||
    fail 'release: published report identity drifted under foreign ambient Git'
else
  if [[ "${JERYU_MONOREPO_CANDIDATE:-0}" == 1 ]]; then
    grep -Fq 'candidate mode cannot satisfy release-broker or test authority' "$stderr_log" ||
      fail 'candidate: release authority refusal was not explicit'
  else
    grep -Eq 'release broker Jankurai path mismatch|release broker Jankurai custody mismatch' \
      "$stderr_log" ||
      fail 'release: foreign-env failure was not a fail-closed release-broker refusal'
  fi
  [[ "$(sha256sum -- "$evidence" | awk '{print $1}')" == "$release_sentinel" ]] ||
    fail 'release: fail-closed foreign-env run destroyed prior evidence'
fi

printf 'score auditor git-env hostile tests ok\n'
