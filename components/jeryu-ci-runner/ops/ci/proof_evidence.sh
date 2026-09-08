#!/usr/bin/env bash
# Generate exact-head proof artifacts only from commands that actually ran.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"
# shellcheck source=ops/ci/lib.sh
source "${ROOT}/ops/ci/lib.sh"
require_tool jq
require_jankurai

candidate_policy="${ROOT}/agent/audit-policy.toml"
if [[ ! -f "${candidate_policy}" || -L "${candidate_policy}" ||
      "$(stat -c '%h' -- "${candidate_policy}")" != 1 ||
      "$(realpath -- "${candidate_policy}")" != "${candidate_policy}" ]]; then
  printf 'candidate audit policy must be a canonical one-link regular file\n' >&2
  exit 1
fi
effective_floor="$(audit_effective_floor "${candidate_policy}")"

BASE_REF="${JERYU_JANKURAI_BASE_REF:-origin/main}"
if [[ "${BASE_REF}" != origin/main ]]; then
  printf 'proof evidence base must be protected origin/main, got %s\n' "${BASE_REF}" >&2
  exit 1
fi
base_commit="$(git rev-parse --verify "${BASE_REF}^{commit}")" || {
  printf 'missing proof evidence base: %s\n' "${BASE_REF}" >&2
  exit 1
}
current_head="$(git rev-parse --verify 'HEAD^{commit}')"
if ! git merge-base --is-ancestor "${base_commit}" "${current_head}"; then
  printf 'proof head is not a descendant of protected base: base=%s head=%s\n' \
    "${base_commit}" "${current_head}" >&2
  exit 1
fi
if [[ -n "$(git status --porcelain=v1 --untracked-files=all)" ]]; then
  printf 'proof evidence requires a clean exact-head checkout\n' >&2
  exit 1
fi
if [[ "$(git remote get-url origin)" != \
      'https://git.neverhuman.org/git/jeryu/jeryu-ci-runner.git' ]]; then
  printf 'proof origin is not the hosted source repository\n' >&2
  exit 1
fi

mkdir -p \
  .jankurai \
  target/jankurai \
  target/jankurai/copy-code \
  target/jankurai/proofbind \
  target/jankurai/proofmark \
  target/jankurai/rust \
  target/jankurai/security

mapfile -d '' -t changed_paths < <(
  git diff --no-ext-diff --name-only -z --diff-filter=ACDMRT \
    "${BASE_REF}...${current_head}" | LC_ALL=C sort -zu
)
if [[ "${#changed_paths[@]}" -eq 0 ]]; then
  printf 'proof evidence refuses an empty protected-main change set\n' >&2
  exit 1
fi
expected_changed="$(
  printf '%s\0' "${changed_paths[@]}" |
    jq -Rs 'split("\u0000") | map(select(length > 0)) | sort | unique'
)"
for changed_path in "${changed_paths[@]}"; do
  if [[ ! -e "${changed_path}" && ! -L "${changed_path}" ]]; then
    printf 'Jankurai 1.6.10 cannot safely classify deleted proof path: %s\n' \
      "${changed_path}" >&2
    exit 1
  fi
done

receipt_dir="target/jankurai/proof-receipts/run-${current_head:0:12}-$$"
jankurai proof . --changed-from origin/main --out target/jankurai/proof-plan.json --md target/jankurai/proof-plan.md
jankurai prove . --plan target/jankurai/proof-plan.json --out-dir "${receipt_dir}" --evidence-index target/jankurai/evidence-index.json

jq -e \
  --arg head "${current_head}" \
  --argjson changed "${expected_changed}" \
  '.schema_version == "1.0.0" and
   .git_head == $head and
   (.changed_paths | sort | unique) == $changed and
   (.risk_notes | length) == 0 and
   (.human_approval_requirements | length) == 0 and
   ([.route_decisions[] | select(.decision != "pass")] | length) == 0 and
   (.commands | length) > 0' \
  target/jankurai/proof-plan.json >/dev/null
jq -e \
  --arg head "${current_head}" \
  --arg receipt_dir "${receipt_dir}" \
  --argjson changed "${expected_changed}" \
  '.schema_version == "1.2.0" and
   .git_head == $head and
   .receipt_dir == $receipt_dir and
   (.changed_paths | sort | unique) == $changed and
   (.failed_receipts | length) == 0 and
   (.receipts | length) == (.commands | length) and
   (.receipts | length) > 0' \
  target/jankurai/evidence-index.json >/dev/null
for receipt in "${receipt_dir}"/*.json; do
  [[ -f "${receipt}" && ! -L "${receipt}" ]] || {
    printf 'missing regular proof receipt in %s\n' "${receipt_dir}" >&2
    exit 1
  }
  jq -e \
    --arg head "${current_head}" \
    '.schema_version == "1.9.0" and .git_head == $head and
     .dirty_worktree == false and .exit_code == 0 and
     (.command | type == "string" and length > 0) and
     (.log_sha256 | test("^sha256:[0-9a-f]{64}$"))' \
    "${receipt}" >/dev/null
done

jankurai proof-verify . \
  --plan target/jankurai/proof-plan.json \
  --evidence-index target/jankurai/evidence-index.json \
  --out target/jankurai/proof-verification.json \
  --md target/jankurai/proof-verification.md
jq -e '.schema_version == "1.0.0" and .verdict == "pass" and
       ((.issues // []) | length) == 0' \
  target/jankurai/proof-verification.json >/dev/null

# Proofbind/proofmark remain diagnostic in Jankurai 1.6.10: required mode
# incorrectly demands Rust line/mutation evidence for non-Rust contract prose.
# They still run without fallbacks, against the exact extant changed surface,
# and their unresolved review count is retained in the summary below.
jankurai proofbind verify . --changed-from origin/main --mode advisory --proof-receipts "${receipt_dir}" --out target/jankurai/proofbind/surface-witness.json --obligations-out target/jankurai/proofbind/obligations.json --md target/jankurai/proofbind/proofbind.md
jankurai proofmark rust . --obligations target/jankurai/proofbind/obligations.json --changed-from origin/main --mode advisory --out target/jankurai/proofmark/proofmark-receipt.json --proof-receipt target/jankurai/proofmark/proof-receipt.json --md target/jankurai/proofmark/proofmark.md

jankurai copy-code . --json target/jankurai/copy-code/report.json --md target/jankurai/copy-code/report.md
jankurai rust map . --out-dir target/jankurai/rust
jankurai rust witness build . --out target/jankurai/rust/witness-graph.json
jankurai rust diagnose . --out target/jankurai/rust/compile-packets.json
JERYU_SECURITY_NETWORK=1 jankurai security run . --out target/jankurai/security/evidence.json --script tools/security-lane.sh --strict --profile ci

# Build the ratchet baseline from authenticated protected main, never from the
# candidate. The no-local clone is exact-SHA isolation and is always removed.
baseline_parent="$(mktemp -d "${TMPDIR:-/tmp}/jeryu-ci-runner-baseline.XXXXXXXX")"
cleanup_baseline() {
  case "${baseline_parent}" in
    "${TMPDIR:-/tmp}"/jeryu-ci-runner-baseline.*)
      rm -rf -- "${baseline_parent}"
      ;;
    *)
      printf 'refusing to remove unexpected baseline path: %s\n' \
        "${baseline_parent}" >&2
      return 1
      ;;
  esac
}
baseline_signal() {
  local exit_code="$1"
  trap - EXIT HUP INT TERM
  cleanup_baseline || true
  exit "${exit_code}"
}

trap cleanup_baseline EXIT
trap 'baseline_signal 129' HUP
trap 'baseline_signal 130' INT
trap 'baseline_signal 143' TERM
git clone --quiet --no-local --no-hardlinks --single-branch --branch main \
  'https://git.neverhuman.org/git/jeryu/jeryu-ci-runner.git' \
  "${baseline_parent}/repo"
if [[ "$(git -C "${baseline_parent}/repo" rev-parse 'HEAD^{commit}')" != \
      "${base_commit}" ||
      -n "$(git -C "${baseline_parent}/repo" status --porcelain=v1 \
        --untracked-files=all)" ]]; then
  printf 'protected-main baseline clone drifted or is dirty\n' >&2
  exit 1
fi
baseline_policy="${baseline_parent}/repo/agent/audit-policy.toml"
if [[ ! -f "${baseline_policy}" || -L "${baseline_policy}" ||
      "$(stat -c '%h' -- "${baseline_policy}")" != 1 ||
      "$(realpath -- "${baseline_policy}")" != "${baseline_policy}" ]]; then
  printf 'protected-main audit policy must be a canonical one-link regular file\n' >&2
  exit 1
fi
if [[ "$(sha256sum "${candidate_policy}" | awk '{print $1}')" != \
      "$(sha256sum "${baseline_policy}" | awk '{print $1}')" ]]; then
  printf 'candidate audit policy must be byte-identical to protected main\n' >&2
  exit 1
fi
(
  cd "${baseline_parent}/repo"
  mkdir -p .jankurai
  jankurai audit . --mode advisory --json .jankurai/repo-score.json \
    --md .jankurai/repo-score.md --policy agent/audit-policy.toml \
    --fail-under "${effective_floor}" --full --no-score-history
)
cp "${baseline_parent}/repo/.jankurai/repo-score.json" \
  target/jankurai/accepted-baseline.json
baseline_score="$(jq -er '.score | select(type == "number") | floor' \
  target/jankurai/accepted-baseline.json)"
baseline_sha256="$(sha256sum target/jankurai/accepted-baseline.json | \
  awk '{print $1}')"
trap - EXIT HUP INT TERM
cleanup_baseline

short_base="${base_commit:0:7}"
jq -e --arg base "${short_base}" \
  --argjson floor "${effective_floor}" \
  '.git.head == $base and .git.dirty_worktree == false and
   (.score | type == "number") and .decision.minimum_score == $floor and
   .decision.hard_findings == 0 and (.caps_applied | length) == 0 and
   .decision.passed == true' \
  target/jankurai/accepted-baseline.json >/dev/null

jankurai audit . --mode ratchet --baseline target/jankurai/accepted-baseline.json --json target/jankurai/repo-score.json --md target/jankurai/repo-score.md --policy agent/audit-policy.toml --fail-under "${effective_floor}" --repair-queue-jsonl target/jankurai/repair-queue.jsonl --full --no-score-history

short_head="${current_head:0:7}"
jq -e --arg head "${short_head}" --argjson changed "${expected_changed}" \
  '.schema_version == "1.0.0" and .git_head == $head and
   (.changed_paths | sort | unique) == $changed and
   (.summary.changed_surface_count | type == "number")' \
  target/jankurai/proofbind/surface-witness.json >/dev/null
jq -e --arg head "${short_head}" \
  '.schema_version == "1.0.0" and .git_head == $head and
   (.summary.total_obligations | type == "number") and
   (.summary.satisfied_obligations | type == "number") and
   (.summary.review_obligations | type == "number") and
   (.summary.verdict == "pass" or .summary.verdict == "review")' \
  target/jankurai/proofmark/proofmark-receipt.json >/dev/null
jq -e --arg head "${current_head}" \
  '.schema_version == "1.0.0" and .git_head == $head and
   .lane == "security" and .wrapper.path == "tools/security-lane.sh" and
   .wrapper.strict == true and .exit_code == 0 and
   ([.commands[] | select(.status == "ran" and .exit_code == 0)] | length) >= 1' \
  target/jankurai/security/evidence.json >/dev/null
jq -e --arg head "${current_head}" \
  '.schema_version == "jeryu.split.security/v2" and .git_head == $head and
   .network_dependency_checks == true and .conclusion == "success" and
   .source_name == "jeryu-ci-runner" and
   .source_version == "jeryu-ci-runner-v5.0.0-split.1" and
   (["cargo-audit-0.22.1", "cargo-deny-0.19.8",
     "hosted-dependency-sources", "syft-1.40.0-cyclonedx"] - .checks |
     length) == 0' \
  target/security/evidence.json >/dev/null
jq -e --arg root "${ROOT}" \
  '.schema_version == "1.1.0" and
   .generated_by == "jankurai copy-code" and
   .auditor_version == "1.6.10" and .repo == $root and
   (.status == "pass" or .status == "review") and
   (.classes | type == "array") and
   .summary.hard_classes == 0 and .summary.hard_instances == 0 and
   (.summary.warning_classes | type == "number") and
   (.summary.warning_instances | type == "number") and
   all(.classes[];
     .hard_fail == false and (.instances | type == "array"))' \
  target/jankurai/copy-code/report.json >/dev/null
jq -e --arg root "${ROOT}" \
  '(.generated_at | type == "string" and length > 0) and
   .workspace_root == $root and
   (.crates | type == "array" and length > 0) and
   all(.crates[];
     (.name | type == "string" and length > 0) and
     (.interface_hash | type == "string" and test("^[0-9a-f]{64}$")) and
     (.implementation_hash | type == "string" and test("^[0-9a-f]{64}$")) and
     (.pub_items | type == "array") and
     (.direct_deps | type == "array") and
     (.reverse_deps | type == "array") and
     (.file_count | type == "number" and . > 0) and
     (.total_lines | type == "number" and . > 0))' \
  target/jankurai/rust/witness-graph.json >/dev/null

# Bind and revalidate the protected-main ratchet output at the repository's
# fleet-wide merge floor.
jq -e \
  --arg head "${short_head}" \
  --argjson baseline_score "${baseline_score}" \
  --argjson floor "${effective_floor}" \
  '.git.head == $head and .git.dirty_worktree == false and
   .score >= $floor and .decision.minimum_score == $floor and
   (.caps_applied | length) == 0 and
   .decision.hard_findings == 0 and .decision.passed == true and
   .decision.ratchet.baseline_score == $baseline_score and
   .decision.ratchet.score_delta >= 0 and
   (.decision.ratchet.new_caps | length) == 0 and
   (.decision.ratchet.new_hard_findings | length) == 0 and
   .decision.ratchet.policy_changed == false and
   .decision.ratchet.passed == true' \
  target/jankurai/repo-score.json >/dev/null
cp target/jankurai/repo-score.json .jankurai/repo-score.json
cp target/jankurai/repo-score.md .jankurai/repo-score.md

if [[ "$(git rev-parse --verify 'HEAD^{commit}')" != "${current_head}" ||
      "$(git rev-parse --verify "${BASE_REF}^{commit}")" != "${base_commit}" ||
      "$(git remote get-url origin)" != \
        'https://git.neverhuman.org/git/jeryu/jeryu-ci-runner.git' ||
      -n "$(git status --porcelain=v1 --untracked-files=all)" ]]; then
  printf 'proof commands changed the exact head, protected base, origin, or tracked tree\n' >&2
  exit 1
fi

proofmark_review="$(jq -er '.summary.review_obligations' \
  target/jankurai/proofmark/proofmark-receipt.json)"
jq -n \
  --arg head "${current_head}" \
  --arg base "${base_commit}" \
  --arg receipt_dir "${receipt_dir}" \
  --arg plan_sha256 "$(sha256sum target/jankurai/proof-plan.json | awk '{print $1}')" \
  --arg evidence_index_sha256 "$(sha256sum target/jankurai/evidence-index.json | awk '{print $1}')" \
  --arg verification_sha256 "$(sha256sum target/jankurai/proof-verification.json | awk '{print $1}')" \
  --arg security_sha256 "$(sha256sum target/jankurai/security/evidence.json | awk '{print $1}')" \
  --arg supply_chain_sha256 "$(sha256sum target/security/evidence.json | awk '{print $1}')" \
  --arg sbom_sha256 "$(sha256sum target/security/sbom.cdx.json | awk '{print $1}')" \
  --arg baseline_sha256 "${baseline_sha256}" \
  --argjson changed_count "${#changed_paths[@]}" \
  --argjson receipt_count "$(jq -er '.receipts | length' target/jankurai/evidence-index.json)" \
  --argjson proofmark_review "${proofmark_review}" \
  --argjson baseline_score "${baseline_score}" \
  '{schema_version:"jeryu.ci-runner.proof-evidence/v1",git_head:$head,
    base_commit:$base,dirty_worktree:false,changed_path_count:$changed_count,
    receipt_count:$receipt_count,receipt_dir:$receipt_dir,
    proof_plan_sha256:$plan_sha256,evidence_index_sha256:$evidence_index_sha256,
    proof_verification_sha256:$verification_sha256,security_sha256:$security_sha256,
    supply_chain_sha256:$supply_chain_sha256,sbom_sha256:$sbom_sha256,
    baseline_score:$baseline_score,baseline_sha256:$baseline_sha256,
    proof_verification:"pass",proofmark_mode:"advisory",
    proofmark_review_obligations:$proofmark_review,
    synthetic_fallbacks:0,conclusion:"success"}' \
  > target/jankurai/proof-evidence-summary.json

printf 'proof evidence ok: head=%s base=%s paths=%s receipts=%s proofmark_review=%s\n' \
  "${current_head}" "${base_commit}" "${#changed_paths[@]}" \
  "$(jq -r '.receipts | length' target/jankurai/evidence-index.json)" \
  "${proofmark_review}"
