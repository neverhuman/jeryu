#!/usr/bin/env bash
# Build and bind a premerge Jankurai candidate without touching the governed host path.
set -euo pipefail
umask 077

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
candidate_root="${1:-}"
evidence_dir="${2:-}"
[[ "${candidate_root}" == /* && "${evidence_dir}" == /* ]] || {
  printf 'usage: %s ABSOLUTE_CANDIDATE_ROOT ABSOLUTE_EVIDENCE_DIR\n' "$0" >&2
  exit 2
}
[[ "${candidate_root}" != "/home/ubuntu/.jeryu" ]] || {
  printf 'candidate qualification may not target the governed host root\n' >&2
  exit 2
}

JERYU_INSTALL_TEST_MODE=1 \
JERYU_INSTALL_ROOT="${candidate_root}" \
JERYU_RUN_ID="${JERYU_RUN_ID:-jeryu-tool-premerge-$(date -u +%Y%m%dT%H%M%SZ)-$$}" \
JERYU_OPERATOR="${JERYU_OPERATOR:-jeryu-tool-pr-ci}" \
  "${here}/install-jankurai.sh"

mapfile -t receipts < <(find "${candidate_root}/receipts/jankurai/sha256" \
  -maxdepth 1 -type f -name '*.json' -print | sort)
[[ "${#receipts[@]}" -eq 1 ]] || {
  printf 'candidate qualification expected exactly one installation receipt, found %s\n' \
    "${#receipts[@]}" >&2
  exit 1
}
receipt="${receipts[0]}"
receipt_sha="$(basename "${receipt}" .json)"
[[ "${receipt_sha}" =~ ^[0-9a-f]{64}$ ]] || {
  printf 'candidate receipt name is not content-addressed: %s\n' "${receipt}" >&2
  exit 1
}
[[ "$(sha256sum "${receipt}" | awk '{print $1}')" == "${receipt_sha}" ]] || {
  printf 'candidate receipt content address mismatch: %s\n' "${receipt}" >&2
  exit 1
}
jq -e '
  .test_mode == true and
  .source.verification == "diagnostic-candidate" and
  .governance.status == "diagnostic-candidate" and
  .governance.protected_main == false and
  .governance.protection_policy == "not-applicable"
' "${receipt}" >/dev/null || {
  printf 'candidate receipt incorrectly claims release authority\n' >&2
  exit 1
}

mkdir -p "${evidence_dir}/receipts"
evidence_receipt="${evidence_dir}/receipts/${receipt_sha}.json"
cp "${receipt}" "${evidence_receipt}"
[[ "$(sha256sum "${evidence_receipt}" | awk '{print $1}')" == "${receipt_sha}" ]] || {
  printf 'persisted candidate receipt content address mismatch\n' >&2
  exit 1
}

env_file="${evidence_dir}/${receipt_sha}.env"
{
  printf 'export JERYU_JANKURAI_BIN=%q\n' "${candidate_root}/bin/jankurai"
  printf 'export JERYU_GOVERNED_JANKURAI_BIN=%q\n' "${candidate_root}/bin/jankurai"
  printf 'export JERYU_JANKURAI_RECEIPT=%q\n' "${evidence_receipt}"
  printf 'export JERYU_JANKURAI_ALLOW_TEST_RECEIPT=1\n'
} > "${env_file}"
printf 'candidate qualification receipt=%s env=%s\n' "${evidence_receipt}" "${env_file}"
