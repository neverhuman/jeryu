#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
cd "$repo_root"
source "$repo_root/ops/ci/lib.sh"
source "$repo_root/ops/ci/source-authority.sh"

die() {
  printf 'score check failed: %s\n' "$*" >&2
  exit 1
}

# The renderer-owned verifier exports the only auditor identity this lane may
# execute. A legacy caller override is accepted only when it names that exact
# verified file; it can never select another executable after verification.
require_jankurai
auditor_bin="$JERYU_GOVERNED_JANKURAI_BIN"
if [[ -n "${JERYU_JANKURAI_BIN:-}" && "$JERYU_JANKURAI_BIN" != "$auditor_bin" ]]; then
  die "caller JERYU_JANKURAI_BIN differs from governed auditor: $JERYU_JANKURAI_BIN"
fi
auditor_sha="$(sha256sum -- "$auditor_bin" | awk '{print $1}')"
auditor_version="$($auditor_bin --version)"
[[ "$auditor_sha" == "$JERYU_JANKURAI_SHA256" &&
   "$auditor_version" == "$JERYU_JANKURAI_VERSION" ]] ||
  die 'governed auditor moved after verification'
auditor_mode=installation-receipt
auditor_receipt_path="${JERYU_JANKURAI_RECEIPT:-}"
auditor_receipt_sha="${JERYU_JANKURAI_RECEIPT_SHA256:-}"
if [[ "${JAIN_RELEASE_CI:-0}" == 1 ]]; then
  auditor_mode=release-broker
  auditor_receipt_path=''
  auditor_receipt_sha=''
else
  [[ "$auditor_receipt_path" == /* && -f "$auditor_receipt_path" &&
     ! -L "$auditor_receipt_path" && "$auditor_receipt_sha" =~ ^[0-9a-f]{64}$ &&
     "$(sha256sum -- "$auditor_receipt_path" | awk '{print $1}')" == "$auditor_receipt_sha" ]] ||
    die 'governed auditor installation receipt moved after verification'
fi

jeryu_source_snapshot
head_sha="$JERYU_SOURCE_HEAD"
tree_sha="$JERYU_SOURCE_TREE"
source_inputs_sha="$JERYU_SOURCE_INPUTS_SHA256"

prepare_dir() {
  local relative="$1"
  local path="$repo_root/$relative"
  [[ ! -L "$path" ]] || die "$relative must not be a symlink"
  mkdir -p -- "$path"
  [[ -d "$path" ]] || die "$relative is not a directory"
  [[ "$(realpath -e -- "$path")" == "$path" ]] ||
    die "$relative escapes the repository"
}

clear_output() {
  local relative="$1"
  local path="$repo_root/$relative"
  if [[ -e "$path" || -L "$path" ]]; then
    [[ -f "$path" && ! -L "$path" ]] || die "$relative is not a regular file"
    [[ "$(stat -c '%h' "$path")" -eq 1 ]] || die "$relative is multiply linked"
    rm -- "$path"
  fi
}

require_output_slot() {
  local relative="$1"
  local path="$repo_root/$relative"
  if [[ -e "$path" || -L "$path" ]]; then
    [[ -f "$path" && ! -L "$path" && "$(stat -c '%h' -- "$path")" -eq 1 &&
       "$(realpath -e -- "$path")" == "$path" ]] ||
      die "$relative lacks physical single-link custody"
  fi
}

required=(
  agent/owner-map.json
  agent/test-map.json
  agent/generated-zones.toml
  agent/proof-lanes.toml
  agent/audit-policy.toml
  agent/boundaries.toml
  agent/JANKURAI_STANDARD.md
)
for path in "${required[@]}"; do
  [[ -s "$path" ]] || { printf 'missing split metadata: %s\n' "$path" >&2; exit 1; }
done
prepare_dir .jankurai
prepare_dir target
prepare_dir target/jankurai
clear_output .jankurai/repo-score.json
clear_output .jankurai/repo-score.md
require_output_slot target/jankurai/evidence.json
require_output_slot target/jankurai/repo-score.json
require_output_slot target/jankurai/repo-score.md
# Spawn the governed auditor under the same scrubbed replacement-blind Git
# authority used for source checks. Ambient GIT_* must not select a foreign head.
jeryu_with_scrubbed_git "$auditor_bin" audit . --full --mode advisory \
  --policy agent/audit-policy.toml \
  --json .jankurai/repo-score.json --md .jankurai/repo-score.md
minimum_score="$(awk -F= '/^[[:space:]]*minimum_score[[:space:]]*=/ {
  gsub(/[[:space:]]/, "", $2); print $2; exit
}' agent/audit-policy.toml)"
[[ "$minimum_score" =~ ^[0-9]+$ ]] || die 'minimum_score is not an integer'
score_value="$(jq -er '.score | numbers' .jankurai/repo-score.json)" ||
  die 'score report has no numeric score'
caps_count="$(jq -er '
  (.caps_applied // .caps // [])
  | if type == "array" then length else error("caps are not an array") end
' .jankurai/repo-score.json)" || die 'score report has malformed caps'
hard_count="$(jq -er '
  (if ((.decision // null) | type) == "object" and (.decision | has("hard_findings"))
   then .decision.hard_findings else (.hard_findings // 0) end)
  | if type == "array" then length
    elif type == "number" then .
    else error("hard findings are neither an array nor a number") end
' .jankurai/repo-score.json)" || die 'score report has malformed hard findings'
(( score_value >= minimum_score )) ||
  die "score ${score_value} is below ${minimum_score}"
(( caps_count == 0 )) || die "caps present: ${caps_count}"
(( hard_count == 0 )) || die "hard findings present: ${hard_count}"
jeryu_require_score_report_matches_source .jankurai/repo-score.json "$head_sha" ||
  die 'score report Git identity does not match bound source'
report_tmp="$(mktemp "$repo_root/target/jankurai/.repo-score.json.XXXXXX")"
report_md_tmp="$(mktemp "$repo_root/target/jankurai/.repo-score.md.XXXXXX")"
evidence_tmp=''
cleanup_temps() {
  rm -f -- "${report_tmp:-}" "${report_md_tmp:-}" "${evidence_tmp:-}"
}
trap cleanup_temps EXIT
cp --reflink=never -- .jankurai/repo-score.json "$report_tmp"
cp --reflink=never -- .jankurai/repo-score.md "$report_md_tmp"
chmod 0600 "$report_tmp" "$report_md_tmp"
[[ "$(stat -c '%h' -- "$report_tmp")" -eq 1 &&
   "$(stat -c '%h' -- "$report_md_tmp")" -eq 1 ]] ||
  die 'candidate score outputs are multiply linked'

jeryu_source_verify "$head_sha" "$tree_sha" "$source_inputs_sha"
jeryu_require_score_report_matches_source "$report_tmp" "$head_sha" ||
  die 'candidate score report Git identity drifted'
[[ "$(sha256sum -- "$auditor_bin" | awk '{print $1}')" == "$auditor_sha" &&
   "$($auditor_bin --version)" == "$auditor_version" ]] ||
  die 'governed auditor moved during scoring'
if [[ "$auditor_mode" == installation-receipt ]]; then
  [[ "$(sha256sum -- "$auditor_receipt_path" | awk '{print $1}')" == "$auditor_receipt_sha" ]] ||
    die 'governed auditor installation receipt moved during scoring'
fi

report="$report_tmp"
[[ -f "$report" && ! -L "$report" && "$(stat -c '%h' "$report")" -eq 1 ]] ||
  die 'score report lacks regular single-link custody'
report_sha="$(sha256sum "$report" | awk '{print $1}')"
policy_sha="$(sha256sum agent/audit-policy.toml | awk '{print $1}')"
score="$(jq -er '.score | numbers' "$report")"
raw_score="$(jq -er '.raw_score | numbers' "$report")"
report_fingerprint="$(jq -er '.report_fingerprint | select(test("^sha256:[0-9a-f]{64}$"))' "$report")"
input_fingerprint="$(jq -er '.input_fingerprint | select(test("^sha256:[0-9a-f]{64}$"))' "$report")"
policy_fingerprint="$(jq -er '.policy_fingerprint | select(test("^sha256:[0-9a-f]{64}$"))' "$report")"

evidence_tmp="$(mktemp "$repo_root/target/jankurai/.evidence.XXXXXX")"
jq -nS \
  --arg schema_version 'jeryu.split.score/v1' \
  --arg repo 'jeryu-tool-finder' \
  --arg status 'pass' \
  --arg head "$head_sha" \
  --arg tree "$tree_sha" \
  --arg source_inputs_sha "$source_inputs_sha" \
  --arg auditor_path "$auditor_bin" \
  --arg auditor_sha "$auditor_sha" \
  --arg auditor_version "$auditor_version" \
  --arg auditor_mode "$auditor_mode" \
  --arg auditor_receipt_path "$auditor_receipt_path" \
  --arg auditor_receipt_sha "$auditor_receipt_sha" \
  --arg report_path 'target/jankurai/repo-score.json' \
  --arg report_sha "$report_sha" \
  --arg report_fingerprint "$report_fingerprint" \
  --arg input_fingerprint "$input_fingerprint" \
  --arg policy_fingerprint "$policy_fingerprint" \
  --arg policy_path 'agent/audit-policy.toml' \
  --arg policy_sha "$policy_sha" \
  --argjson score "$score" \
  --argjson raw_score "$raw_score" \
  --argjson minimum_score "$minimum_score" \
  '{
    schema_version: $schema_version,
    repo: $repo,
    status: $status,
    head: $head,
    tree: $tree,
    source: {tracked_inputs_sha256: $source_inputs_sha},
    auditor: {
      path: $auditor_path,
      sha256: $auditor_sha,
      version_output: $auditor_version,
      authority_mode: $auditor_mode,
      installation_receipt: {
        path: $auditor_receipt_path,
        sha256: $auditor_receipt_sha
      }
    },
    report: {
      path: $report_path,
      sha256: $report_sha,
      report_fingerprint: $report_fingerprint,
      input_fingerprint: $input_fingerprint,
      policy_fingerprint: $policy_fingerprint,
      score: $score,
      raw_score: $raw_score,
      minimum_score: $minimum_score,
      caps_applied: [],
      hard_findings: 0
    },
    policy: {path: $policy_path, sha256: $policy_sha}
  }' > "$evidence_tmp"
chmod 0600 "$evidence_tmp"
jeryu_source_verify "$head_sha" "$tree_sha" "$source_inputs_sha"
[[ "$(sha256sum -- "$auditor_bin" | awk '{print $1}')" == "$auditor_sha" &&
   "$($auditor_bin --version)" == "$auditor_version" ]] ||
  die 'governed auditor moved before score publication'
if [[ "$auditor_mode" == installation-receipt ]]; then
  [[ "$(sha256sum -- "$auditor_receipt_path" | awk '{print $1}')" == "$auditor_receipt_sha" ]] ||
    die 'governed auditor installation receipt moved before score publication'
fi
[[ "$(sha256sum -- .jankurai/repo-score.json | awk '{print $1}')" == "$report_sha" &&
   "$(sha256sum -- "$report_tmp" | awk '{print $1}')" == "$report_sha" ]] ||
  die 'raw score report moved before publication'
mv -- "$report_tmp" target/jankurai/repo-score.json
report_tmp=''
mv -- "$report_md_tmp" target/jankurai/repo-score.md
report_md_tmp=''
mv -- "$evidence_tmp" target/jankurai/evidence.json
evidence_tmp=''
jeryu_source_verify "$head_sha" "$tree_sha" "$source_inputs_sha"
trap - EXIT
printf 'score ok\n'
