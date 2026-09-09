#!/usr/bin/env bash
# Generate exact-head proof artifacts only from commands that actually ran.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${root}"
source ops/ci/lib.sh
require_jankurai
command -v git >/dev/null
command -v jq >/dev/null

base_ref="${JERYU_JANKURAI_BASE_REF:-origin/main}"
if [[ "${base_ref}" != "origin/main" ]]; then
  printf 'proof evidence base must be protected origin/main, got %s\n' "${base_ref}" >&2
  exit 1
fi
base_commit="$(git rev-parse --verify "${base_ref}^{commit}")" || {
  printf 'missing proof evidence base: %s\n' "${base_ref}" >&2
  exit 1
}
current_head="$(git rev-parse --verify 'HEAD^{commit}')"
if [[ "${current_head}" == "${base_commit}" ]] ||
   ! git merge-base --is-ancestor "${base_commit}" "${current_head}"; then
  printf 'proof head must be a non-empty descendant of protected main: base=%s head=%s\n' \
    "${base_commit}" "${current_head}" >&2
  exit 1
fi
if [[ "$(git remote get-url origin)" != \
      "https://git.neverhuman.org/git/jeryu/jeryu-jira.git" ]]; then
  printf 'proof origin is not the hosted source repository\n' >&2
  exit 1
fi
if [[ -n "$(git status --porcelain=v1 --untracked-files=all)" ]]; then
  printf 'proof evidence requires a clean exact-head checkout\n' >&2
  exit 1
fi

candidate_policy="${root}/agent/audit-policy.toml"
if [[ ! -f "${candidate_policy}" || -L "${candidate_policy}" ||
      "$(stat -c '%h' -- "${candidate_policy}")" != "1" ||
      "$(realpath -- "${candidate_policy}")" != "${candidate_policy}" ]]; then
  printf 'candidate audit policy must be a canonical one-link regular file\n' >&2
  exit 1
fi

mapfile -d '' -t changed_paths < <(
  git diff --no-ext-diff --name-only -z --diff-filter=ACDMRT \
    "${base_ref}...${current_head}" | LC_ALL=C sort -zu
)
if [[ "${#changed_paths[@]}" -eq 0 ]]; then
  printf 'proof evidence refuses an empty protected-main change set\n' >&2
  exit 1
fi
for changed_path in "${changed_paths[@]}"; do
  if [[ ! -e "${changed_path}" && ! -L "${changed_path}" ]]; then
    printf 'proof evidence does not accept deleted paths: %s\n' "${changed_path}" >&2
    exit 1
  fi
done
expected_changed="$(printf '%s\0' "${changed_paths[@]}" |
  jq -Rs 'split("\u0000") | map(select(length > 0)) | sort | unique')"

mkdir -p .jankurai target/jankurai/copy-code target/jankurai/proofbind \
  target/jankurai/proofmark target/jankurai/rust target/jankurai/security
receipt_dir="target/jankurai/proof-receipts/run-${current_head:0:12}-$$"

jankurai proof . --changed-from origin/main --out target/jankurai/proof-plan.json --md target/jankurai/proof-plan.md
jq -e \
  --arg head "${current_head}" \
  --argjson changed "${expected_changed}" \
  '.schema_version == "1.0.0" and .git_head == $head and
   (.changed_paths | sort | unique) == $changed and
   (.risk_notes | length) == 0 and
   (.human_approval_requirements | length) == 0 and
   ([.route_decisions[] | select(.decision != "pass")] | length) == 0 and
   (.commands | length) > 0' \
  target/jankurai/proof-plan.json >/dev/null

jankurai prove . --plan target/jankurai/proof-plan.json --out-dir "${receipt_dir}" --evidence-index target/jankurai/evidence-index.json
jq -e \
  --arg head "${current_head}" \
  --arg receipt_dir "${receipt_dir}" \
  --argjson changed "${expected_changed}" \
  '.schema_version == "1.2.0" and .git_head == $head and
   .receipt_dir == $receipt_dir and
   (.changed_paths | sort | unique) == $changed and
   (.risk_notes | length) == 0 and
   (.human_approval_requirements | length) == 0 and
   (.failed_receipts | length) == 0 and
   (.receipts | length) == (.commands | length) and
   (.receipts | length) > 0' \
  target/jankurai/evidence-index.json >/dev/null
for receipt in "${receipt_dir}"/*.json; do
  [[ -f "${receipt}" && ! -L "${receipt}" ]] || {
    printf 'missing regular proof receipt in %s\n' "${receipt_dir}" >&2
    exit 1
  }
  jq -e --arg head "${current_head}" \
    '.schema_version == "1.9.0" and .git_head == $head and
     .dirty_worktree == false and .exit_code == 0 and
     (.command | type == "string" and length > 0) and
     (.log_sha256 | test("^sha256:[0-9a-f]{64}$"))' \
    "${receipt}" >/dev/null
done

jankurai proof-verify . --plan target/jankurai/proof-plan.json --evidence-index target/jankurai/evidence-index.json --out target/jankurai/proof-verification.json --md target/jankurai/proof-verification.md
jq -e '.schema_version == "1.0.0" and .verdict == "pass" and
       ((.issues // []) | length) == 0' \
  target/jankurai/proof-verification.json >/dev/null

# Jankurai keeps proofmark diagnostic when non-Rust documentation and shell
# governance paths have no meaningful Rust line or mutation coverage. Its
# review count is retained below; routing and executed receipts remain strict.
jankurai proofbind verify . --changed-from origin/main --mode advisory --proof-receipts "${receipt_dir}" --out target/jankurai/proofbind/surface-witness.json --obligations-out target/jankurai/proofbind/obligations.json --md target/jankurai/proofbind/proofbind.md
jankurai proofmark rust . --obligations target/jankurai/proofbind/obligations.json --changed-from origin/main --mode advisory --out target/jankurai/proofmark/proofmark-receipt.json --proof-receipt target/jankurai/proofmark/proof-receipt.json --md target/jankurai/proofmark/proofmark.md

jankurai copy-code . --json target/jankurai/copy-code.json --md target/jankurai/copy-code.md
jankurai rust map . --out-dir target/jankurai/rust
jankurai rust witness build . --out target/jankurai/rust/witness-graph.json
jankurai rust diagnose . --out target/jankurai/rust/compile-packets.json
jankurai security run . --out target/jankurai/security/evidence.json --script tools/security-lane.sh --strict --profile ci
if [[ -d db/migrations ]]; then
  jankurai migrate . --analyze --out target/jankurai/migration-report.json --md target/jankurai/migration-report.md
fi

jq -e --arg head "${current_head}" \
  '.schema_version == "1.0.0" and .git_head == $head and
   .lane == "security" and .wrapper.path == "tools/security-lane.sh" and
   .wrapper.strict == true and .exit_code == 0 and
   ([.commands[] | select(.status == "ran" and .exit_code == 0)] | length) == 1' \
  target/jankurai/security/evidence.json >/dev/null
jq -e \
  '.schema_version == "jeryu.split.security/v1" and
   ([.checks[] | select(.status != "pass")] | length) == 0' \
  target/security/evidence.json >/dev/null
jq -e \
  '.schema_version == "1.1.0" and
   .generated_by == "jankurai copy-code" and
   (.status == "pass" or .status == "review") and
   .summary.hard_classes == 0 and .summary.hard_instances == 0' \
  target/jankurai/copy-code.json >/dev/null
jq -e --arg root "${root}" \
  '.workspace_root == $root and
   (.crates | type == "array" and length > 0) and
   all(.crates[];
     (.interface_hash | test("^[0-9a-f]{64}$")) and
     (.implementation_hash | test("^[0-9a-f]{64}$")))' \
  target/jankurai/rust/witness-graph.json >/dev/null

# Build the ratchet baseline from authenticated protected main, never from the
# candidate. Exact-SHA isolation is an automatically removed no-local clone.
baseline_parent="$(mktemp -d /tmp/jeryu-jira-baseline.XXXXXXXX)"
cleanup_baseline() {
  case "${baseline_parent}" in
    /tmp/jeryu-jira-baseline.*) rm -rf -- "${baseline_parent}" ;;
    *) printf 'refusing unexpected baseline cleanup path: %s\n' \
         "${baseline_parent}" >&2; return 1 ;;
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
  https://git.neverhuman.org/git/jeryu/jeryu-jira.git \
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
      "$(stat -c '%h' -- "${baseline_policy}")" != "1" ||
      "$(realpath -- "${baseline_policy}")" != "${baseline_policy}" ||
      "$(sha256sum "${candidate_policy}" | awk '{print $1}')" != \
      "$(sha256sum "${baseline_policy}" | awk '{print $1}')" ]]; then
  printf 'candidate and protected-main audit policies must be canonical and byte-identical\n' >&2
  exit 1
fi
(
  cd "${baseline_parent}/repo"
  mkdir -p .jankurai
  jankurai audit . --mode advisory --json .jankurai/repo-score.json --md .jankurai/repo-score.md --policy agent/audit-policy.toml --fail-under 85 --full --no-score-history
)
cp "${baseline_parent}/repo/.jankurai/repo-score.json" \
  target/jankurai/accepted-baseline.json
baseline_score="$(jq -er '.score | select(type == "number") | floor' \
  target/jankurai/accepted-baseline.json)"
baseline_sha256="$(sha256sum target/jankurai/accepted-baseline.json |
  awk '{print $1}')"
trap - EXIT HUP INT TERM
cleanup_baseline

short_base="${base_commit:0:7}"
jq -e --arg base "${short_base}" \
  '.git.head == $base and .git.dirty_worktree == false and
   .score >= 85 and .decision.hard_findings == 0 and
   (.caps_applied | length) == 0 and .decision.passed == true' \
  target/jankurai/accepted-baseline.json >/dev/null

jankurai audit . --mode ratchet --baseline target/jankurai/accepted-baseline.json --json target/jankurai/repo-score.json --md target/jankurai/repo-score.md --policy agent/audit-policy.toml --fail-under 91 --repair-queue-jsonl target/jankurai/repair-queue.jsonl --full --no-score-history

short_head="${current_head:0:7}"
jq -e \
  --arg head "${short_head}" \
  --argjson baseline_score "${baseline_score}" \
  '.git.head == $head and .git.dirty_worktree == false and
   .score >= 91 and .decision.minimum_score == 91 and
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

if [[ "$(git rev-parse 'HEAD^{commit}')" != "${current_head}" ||
      "$(git rev-parse "${base_ref}^{commit}")" != "${base_commit}" ||
      "$(git remote get-url origin)" != \
        "https://git.neverhuman.org/git/jeryu/jeryu-jira.git" ||
      -n "$(git status --porcelain=v1 --untracked-files=all)" ]]; then
  printf 'proof commands changed the head, protected base, origin, or source tree\n' >&2
  exit 1
fi

proofmark_review="$(jq -er '.summary.review_obligations' \
  target/jankurai/proofmark/proofmark-receipt.json)"
jq -n \
  --arg head "${current_head}" \
  --arg base "${base_commit}" \
  --arg receipt_dir "${receipt_dir}" \
  --arg baseline_sha256 "${baseline_sha256}" \
  --arg plan_sha256 "$(sha256sum target/jankurai/proof-plan.json | awk '{print $1}')" \
  --arg verification_sha256 "$(sha256sum target/jankurai/proof-verification.json | awk '{print $1}')" \
  --arg security_sha256 "$(sha256sum target/jankurai/security/evidence.json | awk '{print $1}')" \
  --argjson changed_count "${#changed_paths[@]}" \
  --argjson receipt_count "$(jq -er '.receipts | length' target/jankurai/evidence-index.json)" \
  --argjson proofmark_review "${proofmark_review}" \
  --argjson baseline_score "${baseline_score}" \
  '{schema_version:"jeryu.jira.proof-evidence/v1",git_head:$head,
    base_commit:$base,dirty_worktree:false,changed_path_count:$changed_count,
    receipt_count:$receipt_count,receipt_dir:$receipt_dir,
    proof_plan_sha256:$plan_sha256,
    proof_verification_sha256:$verification_sha256,
    security_sha256:$security_sha256,baseline_score:$baseline_score,
    baseline_sha256:$baseline_sha256,proof_verification:"pass",
    proofmark_mode:"advisory",proofmark_review_obligations:$proofmark_review,
    synthetic_fallbacks:0,conclusion:"success"}' \
  >target/jankurai/proof-evidence-summary.json

printf 'proof evidence ok: head=%s base=%s paths=%s receipts=%s proofmark_review=%s\n' \
  "${current_head}" "${base_commit}" "${#changed_paths[@]}" \
  "$(jq -r '.receipts | length' target/jankurai/evidence-index.json)" \
  "${proofmark_review}"
