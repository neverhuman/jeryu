#!/usr/bin/env bash
# Canonical, fail-closed Jankurai evidence lane for this standalone repository.
# Compatibility entrypoints delegate here; no caller may synthesize artifacts
# or use candidate output as its own ratchet baseline.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"
source "${ROOT}/ops/ci/common.sh"

BASE_REF="${JERYU_JANKURAI_BASE_REF:-origin/main}"
BASELINE_REPORT="agent/baselines/main.repo-score.json"
BASELINE_PROVENANCE="agent/baselines/main.repo-score.provenance.json"

ensure_base_ref() {
  if git rev-parse --verify "${BASE_REF}^{commit}" >/dev/null 2>&1; then
    return 0
  fi
  case "${BASE_REF}" in
    origin/*)
      git fetch --no-tags --depth "${JERYU_CI_FETCH_DEPTH:-128}" origin \
        "${BASE_REF#origin/}:refs/remotes/${BASE_REF}"
      ;;
  esac
  if ! git rev-parse --verify "${BASE_REF}^{commit}" >/dev/null 2>&1; then
    printf 'missing proof-evidence base ref: %s\n' "${BASE_REF}" >&2
    exit 1
  fi
}

validate_baseline() {
  local base_commit baseline_commit baseline_sha baseline_tree current_head
  for path in "${BASELINE_REPORT}" "${BASELINE_PROVENANCE}"; do
    if [[ ! -f "${path}" || -L "${path}" || "$(stat -c '%h' -- "${path}")" != 1 ]]; then
      printf 'baseline input must be a one-link regular file: %s\n' "${path}" >&2
      return 1
    fi
  done
  base_commit="$(git rev-parse --verify "${BASE_REF}^{commit}")"
  current_head="$(git rev-parse --verify 'HEAD^{commit}')"
  baseline_commit="$(jq -er '.source.commit' "${BASELINE_PROVENANCE}")"
  if ! git cat-file -e "${baseline_commit}^{commit}" 2>/dev/null; then
    printf 'baseline source commit is unavailable: %s\n' "${baseline_commit}" >&2
    return 1
  fi
  if [[ "${current_head}" != "${base_commit}" ]]; then
    if [[ "${baseline_commit}" != "${base_commit}" ]]; then
      printf 'topic baseline must match protected base: baseline=%s base=%s\n' \
        "${baseline_commit}" "${base_commit}" >&2
      return 1
    fi
  elif [[ "${baseline_commit}" == "${current_head}" ]] ||
       ! git merge-base --is-ancestor "${baseline_commit}" "${current_head}"; then
    printf 'protected-main baseline must name a strict protected ancestor: baseline=%s head=%s\n' \
      "${baseline_commit}" "${current_head}" >&2
    return 1
  fi
  baseline_tree="$(git rev-parse --verify "${baseline_commit}^{tree}")"
  baseline_sha="$(sha256sum "${BASELINE_REPORT}" | awk '{print $1}')"
  jq -e \
    --slurpfile baseline "${BASELINE_REPORT}" \
    --arg baseline_commit "${baseline_commit}" \
    --arg baseline_tree "${baseline_tree}" \
    --arg baseline_sha "${baseline_sha}" \
    --arg binary_sha "${JERYU_JANKURAI_SHA256}" \
    --arg version "${JERYU_JANKURAI_VERSION}" \
    --arg source_rev "${JERYU_JANKURAI_SOURCE_REV}" \
    --arg source_tag "${JERYU_JANKURAI_SOURCE_TAG}" \
    --arg source_tree "${JERYU_JANKURAI_SOURCE_TREE}" \
    '.schema == "jeryu.jankurai-baseline-provenance/v1" and
     .repository == "jeryu/jeryu-deploy" and
     .source.remote == "https://git.neverhuman.org/git/jeryu/jeryu-deploy.git" and
     .source.branch == "main" and
     .source.commit == $baseline_commit and
     .source.tree == $baseline_tree and
     .source.dirty_worktree == false and
     .jankurai.version == $version and
     .jankurai.source_commit == $source_rev and
     .jankurai.source_tag == $source_tag and
     .jankurai.source_tree == $source_tree and
     .jankurai.binary_sha256 == $binary_sha and
     .report.path == "agent/baselines/main.repo-score.json" and
     .report.sha256 == $baseline_sha and
     (.report.generator_artifact_sha256 | test("^[0-9a-f]{64}$")) and
     .report.mode == "full" and
     .report.score == $baseline[0].score and
     .report.hard_findings == $baseline[0].decision.hard_findings and
     .report.caps == ($baseline[0].caps_applied | length) and
     .report.report_fingerprint == $baseline[0].report_fingerprint and
     .report.input_fingerprint == $baseline[0].input_fingerprint and
     .report.policy_fingerprint == $baseline[0].policy_fingerprint and
     .report.score >= 85 and .report.hard_findings == 0 and .report.caps == 0' \
    "${BASELINE_PROVENANCE}" >/dev/null
  jq -e \
    --arg short "${baseline_commit:0:7}" \
    '.schema_version == "1.9.0" and
     .git.head == $short and
     .git.mode == "full" and
     .git.dirty_worktree == false and
     .score >= 85 and
     (.caps_applied | length) == 0 and
     .decision.hard_findings == 0 and
     (.report_fingerprint | test("^sha256:[0-9a-f]{64}$")) and
     (.input_fingerprint | test("^sha256:[0-9a-f]{64}$")) and
     (.policy_fingerprint | test("^sha256:[0-9a-f]{64}$"))' \
    "${BASELINE_REPORT}" >/dev/null
}

mkdir -p \
  .jankurai \
  target/jankurai \
  target/jankurai/diff \
  target/jankurai/proofbind \
  target/jankurai/proofmark \
  target/jankurai/rust \
  target/jankurai/security

# Remove stale outputs whose producers do not apply to standalone Deploy. A
# later upload must not mistake leftovers from the retired implementation for
# outputs of this invocation.
rm -f \
  target/jankurai/migration-report.json \
  target/jankurai/migration-report.md \
  target/jankurai/ux-qa.json \
  target/jankurai/vibe-coverage.json \
  target/jankurai/vibe-coverage.md

ensure_base_ref
if [[ "${BASE_REF}" != "origin/main" ]]; then
  printf 'proof-evidence base must be protected origin/main, got %s\n' "${BASE_REF}" >&2
  exit 1
fi
bash ops/ci/security-tools.sh
require_jankurai
validate_baseline

# Run the canonical repository-owned wrapper through Jankurai. It delegates to
# the exact ops implementation and is itself ownership/test-map governed.
jankurai security run . --out target/jankurai/security/evidence.json \
  --script tools/security-lane.sh --strict --profile ci
current_head="$(git rev-parse HEAD)"
jq -e \
  --arg head "${current_head}" \
  '.schema_version == "1.0.0" and
   .git_head == $head and
   .lane == "security" and
   .wrapper.path == "tools/security-lane.sh" and
   .wrapper.strict == true and
   .exit_code == 0 and
   ([.commands[] | select(.status == "ran" and .exit_code == 0)] | length) >= 1' \
  target/jankurai/security/evidence.json >/dev/null

# Produce the advisory view first so repairs remain inspectable, then compare a
# fresh full audit with the immutable protected-main report.
jankurai audit . --mode advisory --json .jankurai/repo-score.json \
  --md .jankurai/repo-score.md --policy agent/audit-policy.toml \
  --repair-queue-jsonl target/jankurai/repair-queue.jsonl \
  --full \
  --no-score-history

raw_policy="target/jankurai/raw-audit-policy.toml"
jeryu_raw_policy "${raw_policy}"
jankurai audit . --mode advisory --json target/jankurai/raw-repo-score.json \
  --md target/jankurai/raw-repo-score.md --policy "${raw_policy}" \
  --full \
  --no-score-history

mapfile -d '' -t proof_changed < <(
  {
    git diff --no-ext-diff --name-only -z --diff-filter=ACDMRT "${BASE_REF}...HEAD"
    git diff --no-ext-diff --name-only -z --diff-filter=ACDMRT --cached
    git diff --no-ext-diff --name-only -z --diff-filter=ACDMRT
    git ls-files -z --others --exclude-standard
  } | LC_ALL=C sort -zu
)
proof_uses_protected_diff=true
if [[ "${#proof_changed[@]}" -eq 0 ]]; then
  proof_changed=(agent/tool-adoption.toml)
  proof_uses_protected_diff=false
fi

# Jankurai 1.6.11 proofbind classifies a path by reading its current bytes. Its
# --changed-from resolver includes deleted paths and then fails with ENOENT.
# Keep deletions in the authoritative proof plan below, but give proofbind only
# the extant subset. The assertions after each producer make both scopes exact;
# a deleted path therefore cannot disappear from the overall proof silently.
proofbind_changed=()
for changed_path in "${proof_changed[@]}"; do
  if [[ -e "${changed_path}" || -L "${changed_path}" ]]; then
    proofbind_changed+=("${changed_path}")
  fi
done
if [[ "${#proofbind_changed[@]}" -eq 0 ]]; then
  printf 'proofbind cannot safely classify a delete-only change with Jankurai 1.6.11\n' >&2
  exit 1
fi

assert_changed_paths() {
  local artifact="$1"
  local producer="$2"
  local expected_json
  shift 2
  expected_json="$(printf '%s\0' "$@" | jq -Rs 'split("\u0000") | map(select(length > 0)) | sort | unique')"
  if ! jq -e --argjson expected "${expected_json}" \
    '(.changed_paths | sort | unique) == $expected' "${artifact}" >/dev/null; then
    printf '%s changed-path scope diverged from the authenticated Git inventory: %s\n' \
      "${producer}" "${artifact}" >&2
    return 1
  fi
}

proof_args=()
if [[ "${proof_uses_protected_diff}" == "true" ]]; then
  proof_args+=(--changed-from "${BASE_REF}")
fi
for changed_path in "${proof_changed[@]}"; do
  proof_args+=(--changed "${changed_path}")
done
proofbind_args=()
for changed_path in "${proofbind_changed[@]}"; do
  proofbind_args+=(--changed "${changed_path}")
done

jankurai proof "${proof_args[@]}" \
  --out target/jankurai/proof-plan.json \
  --md target/jankurai/proof-plan.md \
  .
assert_changed_paths target/jankurai/proof-plan.json "proof plan" "${proof_changed[@]}"
jankurai proofbind map . "${proofbind_args[@]}" \
  --mode advisory \
  --out target/jankurai/proofbind/surface-witness.json \
  --obligations-out target/jankurai/proofbind/obligations.json \
  --md target/jankurai/proofbind/proofbind.md
assert_changed_paths target/jankurai/proofbind/surface-witness.json \
  "proofbind map" "${proofbind_changed[@]}"
jankurai proofbind verify . "${proofbind_args[@]}" \
  --mode advisory \
  --out target/jankurai/proofbind/surface-witness.json \
  --obligations-out target/jankurai/proofbind/obligations.json \
  --md target/jankurai/proofbind/proofbind.md
assert_changed_paths target/jankurai/proofbind/surface-witness.json \
  "proofbind verify" "${proofbind_changed[@]}"
jankurai proofmark rust . --obligations target/jankurai/proofbind/obligations.json \
  "${proofbind_args[@]}" --mode advisory \
  --out target/jankurai/proofmark/proofmark-receipt.json \
  --proof-receipt target/jankurai/proofmark/proof-receipt.json \
  --md target/jankurai/proofmark/proofmark.md
jankurai copy-code . --json target/jankurai/copy-code.json --md target/jankurai/copy-code.md
jankurai rust map . --out-dir target/jankurai/rust
jankurai rust witness build . --out target/jankurai/rust/witness-graph.json
jankurai rust diagnose . --out target/jankurai/rust/compile-packets.json

cp "${BASELINE_REPORT}" target/jankurai/accepted-baseline.json
jankurai audit . --mode ratchet --baseline target/jankurai/accepted-baseline.json --json target/jankurai/repo-score.json --md target/jankurai/repo-score.md --policy agent/audit-policy.toml --full --no-score-history
baseline_fingerprint="$(jq -r '.report_fingerprint' "${BASELINE_REPORT}")"
jq -e \
  --arg baseline_fingerprint "${baseline_fingerprint}" \
  '.score >= 85 and
   (.caps_applied | length) == 0 and
   .decision.status == "pass" and
   .decision.passed == true and
   .decision.hard_findings == 0 and
   .decision.ratchet.passed == true and
   .decision.ratchet.baseline_report_fingerprint == $baseline_fingerprint and
   (.decision.ratchet.new_caps | length) == 0 and
   (.decision.ratchet.new_hard_findings | length) == 0 and
   .decision.ratchet.policy_changed == false' \
  target/jankurai/repo-score.json >/dev/null

score_sha="$(sha256sum target/jankurai/repo-score.json | awk '{print $1}')"
score="$(jq -r '.score' target/jankurai/repo-score.json)"
printf 'schema=jeryu.jankurai-language-audit/v1\ncommand=jankurai audit --mode ratchet\nhead=%s\nreport_sha256=%s\nscore=%s\nhard_findings=0\ncaps=0\n' \
  "${current_head}" "${score_sha}" "${score}" \
  > target/jankurai/language-bad-behavior.log

printf 'proof evidence ok: head=%s score=%s baseline=%s\n' \
  "${current_head}" "${score}" "$(git rev-parse "${BASE_REF}^{commit}")"
