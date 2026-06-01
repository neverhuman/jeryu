#!/usr/bin/env bash
# Test for scripts/emit-release-receipt.sh — proves the release receipt + rollback.json
# are well-formed and self-consistent. Runs the real emitter against a mock bundle and
# asserts the contract from docs/release.md (commit + per-artifact digests + rollback
# target). Hermetic: tolerates offline previous-release lookup (asserts shape, not the
# specific previous tag).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EMITTER="${ROOT}/scripts/emit-release-receipt.sh"
JQ_BIN="$(command -v jq || true)"
[ -x "${EMITTER}" ] || { echo "FAIL: ${EMITTER} not executable"; exit 1; }
[ -n "${JQ_BIN}" ] || { echo "SKIP: jq not installed"; exit 0; }

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf 'ok   - %s\n' "$1"; }
no()  { FAIL=$((FAIL+1)); printf 'FAIL - %s\n' "$1"; }
check() { if eval "$2"; then ok "$1"; else no "$1"; fi; }

BUNDLE="$(mktemp -d)"
trap 'rm -rf "${BUNDLE}"' EXIT

# Mock the artifacts release.sh assembles into the bundle.
printf 'mock-jeryu-binary-%s' "$(date -u +%s)" > "${BUNDLE}/jeryu"
printf '{"spdx":"mock"}'      > "${BUNDLE}/sbom.spdx.json"
printf '{"cdx":"mock"}'       > "${BUNDLE}/sbom.cdx.json"
printf '{"slsa":"mock"}'      > "${BUNDLE}/provenance.json"
printf 'cosign mock\n'        > "${BUNDLE}/cosign.txt"

RECEIPT="${BUNDLE}/release-receipt.json"
JERYU_RELEASE_TAG="v0.0.0-test" bash "${EMITTER}" "${BUNDLE}" > "${RECEIPT}"

# --- receipt assertions ---
check "receipt is valid JSON"            "jq -e . '${RECEIPT}' >/dev/null"
check "receipt schema is tagged"         "jq -e '.schema==\"jeryu.release-receipt/v1\"' '${RECEIPT}' >/dev/null"
check "receipt names a 40-hex commit"    "jq -e '.commit|test(\"^[0-9a-f]{40}$\")' '${RECEIPT}' >/dev/null"
check "binary digest is sha256 (64 hex)" "jq -e '.artifacts.jeryu.sha256|test(\"^[0-9a-f]{64}$\")' '${RECEIPT}' >/dev/null"
check "provenance digest present"        "jq -e '.artifacts.\"provenance.json\".sha256|test(\"^[0-9a-f]{64}$\")' '${RECEIPT}' >/dev/null"
check "rollback.json digest present"     "jq -e '.artifacts.\"rollback.json\".sha256|test(\"^[0-9a-f]{64}$\")' '${RECEIPT}' >/dev/null"
check "gate evidence lists required lanes" "jq -e '.gate_evidence.required_lanes|index(\"jankurai-audit\")' '${RECEIPT}' >/dev/null"
check "rollback target named"            "jq -e '.rollback.previous_release|type==\"string\" and length>0' '${RECEIPT}' >/dev/null"

# the receipt's binary digest must equal the actual bundle binary
ACTUAL_BIN_SHA="$(sha256sum "${BUNDLE}/jeryu" | awk '{print $1}')"
RECEIPT_BIN_SHA="$(jq -r '.artifacts.jeryu.sha256' "${RECEIPT}")"
check "receipt binary digest matches the real binary" "[ \"${ACTUAL_BIN_SHA}\" = \"${RECEIPT_BIN_SHA}\" ]"

# --- rollback.json assertions (jeryu-signrail::RollbackMetadata shape) ---
RB="${BUNDLE}/rollback.json"
check "rollback.json written + valid JSON"   "jq -e . '${RB}' >/dev/null"
check "rollback.previous_release present"    "jq -e '.previous_release|type==\"string\" and length>0' '${RB}' >/dev/null"
check "rollback.rollback_command present"    "jq -e '.rollback_command|test(\"gh release download\")' '${RB}' >/dev/null"
check "rollback.config_digest is sha256-tagged" "jq -e '.config_digest|test(\"^sha256:\")' '${RB}' >/dev/null"
check "rollback.data_migration present"      "jq -e '.data_migration|type==\"string\" and length>0' '${RB}' >/dev/null"
check "rollback.verified_at_epoch numeric"   "jq -e '.verified_at_epoch|type==\"number\"' '${RB}' >/dev/null"

printf '\n[test-emit-release-receipt] %d passed, %d failed\n' "${PASS}" "${FAIL}"
[ "${FAIL}" -eq 0 ]
